//! The message loop: documents, analyses, and one handler per LSP method.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use agentmark::project::normalize;
use agentmark_rt::ModelClient;
use crossbeam_channel::TryRecvError;
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    Notification as _, PublishDiagnostics, ShowMessage,
};
use lsp_types::request::{
    CodeActionRequest, CodeActionResolveRequest, Completion, ExecuteCommand, Formatting, GotoDefinition, HoverRequest,
    RegisterCapability, Request as _,
};
use lsp_types::{
    CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams, CompletionOptions, CompletionParams,
    CompletionResponse, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentFormattingParams, ExecuteCommandOptions, ExecuteCommandParams,
    FileSystemWatcher, GlobPattern, GotoDefinitionParams, GotoDefinitionResponse, HoverParams, HoverProviderCapability,
    InitializeParams, InitializeResult, MessageType, OneOf, PublishDiagnosticsParams, Registration, RegistrationParams,
    SaveOptions, ServerCapabilities, ServerInfo, ShowMessageParams, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, TextEdit, Uri, WorkDoneProgressOptions,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::actions::{self, ModelRunner};
use crate::text::end_position;
use crate::uri::{path_from_uri, uri_from_path};
use crate::workspace::{in_bindings, project_target, Analysis, Doc, Docs, Good, OverlayFs};
use crate::{complete, diagnostics, nav};

/// The message of an error that is answered with `null` instead (Q259).
const QUIET: &str = "(quiet)";

type Reply = Result<Value, (ErrorCode, String)>;

type BoxErr = Box<dyn Error + Sync + Send>;

pub struct Server {
    conn: Connection,
    docs: Docs,
    cache: HashMap<String, Rc<Analysis>>,
    goods: HashMap<String, Rc<Good>>,
    /// URIs that currently show diagnostics we published.
    published: HashSet<String>,
    runner: Option<Rc<ModelRunner>>,
    /// No fixed model: each project's own (binder's choice) is resolved when an action runs (Q256).
    from_env: bool,
    /// Resolved models per (project root, AGENTMARK_MODEL, AGENTMARK_ENV, [models] content), so a code action
    /// does not rebuild a client and a tokio runtime every time.
    resolved: RefCell<HashMap<String, Rc<ModelRunner>>>,
    dirty: bool,
    /// The client understands snippet text edits (`experimental.snippetTextEdit`, Q255).
    snippets: bool,
}

fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::FULL),
            save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions { include_text: Some(false) })),
            ..Default::default()
        })),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".into(), " ".into()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(lsp_types::CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![
                CodeActionKind::QUICKFIX,
                CodeActionKind::new(actions::KIND_EXPLAIN),
                CodeActionKind::new(actions::KIND_GUARD),
            ]),
            resolve_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: vec![actions::EXPLAIN_COMMAND.to_string()],
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        ..Default::default()
    }
}

fn cast<P: DeserializeOwned>(v: Value) -> Result<P, String> {
    serde_json::from_value(v).map_err(|e| format!("bad parameters: {e}"))
}

impl Server {
    pub fn new(conn: Connection, model: Option<Arc<dyn ModelClient>>) -> Server {
        let runner = model.and_then(|m| ModelRunner::new(m).ok()).map(Rc::new);
        Server { conn, docs: Docs::new(), cache: HashMap::new(), goods: HashMap::new(), published: HashSet::new(), runner, from_env: false, resolved: RefCell::new(HashMap::new()), dirty: false, snippets: false }
    }

    /// A server whose model is the binder's choice for each project (`AGENTMARK_MODEL`, `AGENTMARK_ENV` + `[models]`).
    pub fn from_env(conn: Connection) -> Server {
        Server { from_env: true, ..Server::new(conn, None) }
    }

    /// The model for a project: the fixed one, else what `agentmark bind` would use there.
    fn runner_for(&self, a: &Analysis) -> Result<Rc<ModelRunner>, (ErrorCode, String)> {
        if let Some(r) = &self.runner {
            return Ok(r.clone());
        }
        if !self.from_env {
            return Err((ErrorCode::InvalidRequest, "no model is configured".to_string()));
        }
        let config = agentmark::cli::provider_config(&a.project);
        let var = |k: &str| std::env::var(k).unwrap_or_default();
        let key = format!("{}\0{}\0{}\0{:?}", a.project.root, var("AGENTMARK_MODEL"), var("AGENTMARK_ENV"), config);
        if let Some(r) = self.resolved.borrow().get(&key) {
            return Ok(r.clone());
        }
        let (model, _) = agentmark::cli::model_for(config)
            .map_err(|e| (ErrorCode::InvalidRequest, format!("no model is configured: {e}")))?;
        let runner = Rc::new(ModelRunner::new(model).map_err(|e| (ErrorCode::InternalError, e.to_string()))?);
        self.resolved.borrow_mut().insert(key, runner.clone());
        Ok(runner)
    }

    /// Do the `initialize` handshake and serve until `exit`.
    pub fn run(mut self) -> Result<(), BoxErr> {
        let (id, params) = self.conn.initialize_start()?;
        let init: InitializeParams = serde_json::from_value(params)?;
        let result = InitializeResult {
            capabilities: capabilities(),
            server_info: Some(ServerInfo { name: "agentmark-lsp".into(), version: Some(env!("CARGO_PKG_VERSION").into()) }),
        };
        self.conn.initialize_finish(id, serde_json::to_value(result)?)?;
        let dynamic_watch = init
            .capabilities
            .workspace
            .and_then(|w| w.did_change_watched_files)
            .and_then(|d| d.dynamic_registration)
            .unwrap_or(false);
        self.snippets = init
            .capabilities
            .experimental
            .as_ref()
            .and_then(|e| e.get("snippetTextEdit"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if dynamic_watch {
            self.register_watchers();
        }
        loop {
            let msg = if self.dirty {
                match self.conn.receiver.try_recv() {
                    Ok(m) => m,
                    Err(TryRecvError::Empty) => {
                        self.flush_diagnostics();
                        continue;
                    }
                    Err(TryRecvError::Disconnected) => break,
                }
            } else {
                match self.conn.receiver.recv() {
                    Ok(m) => m,
                    Err(_) => break,
                }
            };
            match msg {
                Message::Request(req) => {
                    if self.conn.handle_shutdown(&req)? {
                        return Ok(());
                    }
                    self.request(req);
                }
                Message::Notification(n) if n.method == "exit" => {
                    // `exit` without a `shutdown` before it: the protocol says exit code 1
                    return Err("exit without shutdown".into());
                }
                Message::Notification(n) => self.notification(n),
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    fn send(&self, m: impl Into<Message>) {
        let _ = self.conn.sender.send(m.into());
    }

    fn register_watchers(&self) {
        let watchers: Vec<FileSystemWatcher> = ["**/*.agent.md", "**/tools.toml", "**/agentmark.toml"]
            .iter()
            .map(|g| FileSystemWatcher { glob_pattern: GlobPattern::String((*g).to_string()), kind: None })
            .collect();
        let params = RegistrationParams {
            registrations: vec![Registration {
                id: "agentmark-watch".into(),
                method: "workspace/didChangeWatchedFiles".into(),
                register_options: Some(json!({ "watchers": serde_json::to_value(watchers).unwrap_or_default() })),
            }],
        };
        self.send(Request::new(RequestId::from("agentmark-register".to_string()), RegisterCapability::METHOD.to_string(), params));
    }

    // ------------------------------------------------------------------------ analyses

    fn disk_or_buffer(&self, path: &str) -> Option<String> {
        use agentmark::project::Fs;
        OverlayFs::new(&self.docs).read(path)
    }

    fn analysis_for(&mut self, path: &str) -> Rc<Analysis> {
        let target = project_target(&OverlayFs::new(&self.docs), path);
        if let Some(a) = self.cache.get(&target) {
            if !path.ends_with(".agent.md") || a.project.sources.contains_key(path) {
                return a.clone();
            }
        }
        if let Some(a) = self.cache.get(path) {
            return a.clone();
        }
        let previous = self.goods.get(&target).cloned();
        let a = Rc::new(Analysis::run(&self.docs, path, &target, previous));
        if let Some(g) = &a.good {
            self.goods.insert(a.root.clone(), g.clone());
        }
        self.cache.insert(a.root.clone(), a.clone());
        a
    }

    fn invalidate(&mut self) {
        self.cache.clear();
        self.dirty = true;
    }

    fn flush_diagnostics(&mut self) {
        self.dirty = false;
        let open: Vec<String> = self.docs.keys().cloned().collect();
        let mut new: BTreeMap<String, (Vec<lsp_types::Diagnostic>, Option<i32>)> = BTreeMap::new();
        for path in &open {
            let a = self.analysis_for(path);
            let by_file = diagnostics::by_file(&a);
            for (file, list) in by_file {
                new.insert(uri_from_path(&file).as_str().to_string(), (list, self.docs.get(&file).map(|d| d.version)));
            }
        }
        for path in &open {
            let uri = uri_from_path(path).as_str().to_string();
            let version = self.docs.get(path).map(|d| d.version);
            new.entry(uri).or_insert((Vec::new(), version));
        }
        for old in self.published.clone() {
            new.entry(old).or_insert_with(|| (Vec::new(), None));
        }
        self.published = new.iter().filter(|(_, (d, _))| !d.is_empty()).map(|(u, _)| u.clone()).collect();
        for (uri, (diagnostics, version)) in new {
            let Ok(uri) = uri.parse::<Uri>() else { continue };
            let params = PublishDiagnosticsParams { uri, diagnostics, version };
            self.send(Notification::new(PublishDiagnostics::METHOD.to_string(), params));
        }
    }

    // ------------------------------------------------------------------------ notifications

    fn notification(&mut self, n: Notification) {
        match n.method.as_str() {
            DidOpenTextDocument::METHOD => {
                if let Ok(p) = cast::<DidOpenTextDocumentParams>(n.params) {
                    if let Some(path) = path_from_uri(&p.text_document.uri) {
                        self.docs.insert(normalize(&path), Doc { text: p.text_document.text, version: p.text_document.version });
                        self.invalidate();
                    }
                }
            }
            DidChangeTextDocument::METHOD => {
                if let Ok(p) = cast::<DidChangeTextDocumentParams>(n.params) {
                    if let (Some(path), Some(change)) = (path_from_uri(&p.text_document.uri), p.content_changes.into_iter().last()) {
                        self.docs.insert(normalize(&path), Doc { text: change.text, version: p.text_document.version });
                        self.invalidate();
                    }
                }
            }
            DidSaveTextDocument::METHOD => {
                if let Ok(p) = cast::<DidSaveTextDocumentParams>(n.params) {
                    if let (Some(path), Some(text)) = (path_from_uri(&p.text_document.uri), p.text) {
                        if let Some(d) = self.docs.get_mut(&normalize(&path)) {
                            d.text = text;
                        }
                    }
                    self.invalidate();
                }
            }
            DidCloseTextDocument::METHOD => {
                if let Ok(p) = cast::<DidCloseTextDocumentParams>(n.params) {
                    if let Some(path) = path_from_uri(&p.text_document.uri) {
                        self.docs.remove(&normalize(&path));
                        self.invalidate();
                    }
                }
            }
            DidChangeWatchedFiles::METHOD => self.invalidate(),
            _ => {}
        }
    }

    // ------------------------------------------------------------------------ requests

    fn request(&mut self, req: Request) {
        let id = req.id.clone();
        let result = match req.method.as_str() {
            Completion::METHOD => self.with(req, Self::completion),
            GotoDefinition::METHOD => self.with(req, Self::definition),
            HoverRequest::METHOD => self.with(req, Self::hover),
            Formatting::METHOD => self.with(req, Self::formatting),
            CodeActionRequest::METHOD => self.with(req, Self::code_action),
            CodeActionResolveRequest::METHOD => self.with(req, Self::resolve),
            ExecuteCommand::METHOD => self.with(req, Self::execute),
            m => Err((ErrorCode::MethodNotFound, format!("unknown method {m}"))),
        };
        let resp = match result {
            Ok(v) => Response::new_ok(id, v),
            Err((_, msg)) if msg == QUIET => Response::new_ok(id, Value::Null),
            Err((code, msg)) => Response::new_err(id, code as i32, msg),
        };
        self.send(resp);
    }

    fn with<P: DeserializeOwned>(
        &mut self,
        req: Request,
        f: fn(&mut Self, P) -> Reply,
    ) -> Reply {
        let p = cast::<P>(req.params).map_err(|e| (ErrorCode::InvalidParams, e))?;
        f(self, p)
    }

    fn doc(&self, uri: &Uri) -> Result<(String, String), (ErrorCode, String)> {
        // not a file we can read (another scheme, gone from the disk): answered with `null`
        let quiet = || (ErrorCode::InvalidParams, QUIET.to_string());
        let path = path_from_uri(uri).map(|p| normalize(&p)).ok_or_else(quiet)?;
        let text = self.disk_or_buffer(&path).ok_or_else(quiet)?;
        Ok((path, text))
    }

    fn completion(&mut self, p: CompletionParams) -> Result<Value, (ErrorCode, String)> {
        let tdp = p.text_document_position;
        let (path, text) = self.doc(&tdp.text_document.uri)?;
        let a = self.analysis_for(&path);
        let items = complete::complete(&a, &self.docs, &path, &text, tdp.position);
        Ok(json!(CompletionResponse::Array(items)))
    }

    fn definition(&mut self, p: GotoDefinitionParams) -> Result<Value, (ErrorCode, String)> {
        let tdp = p.text_document_position_params;
        let (path, text) = self.doc(&tdp.text_document.uri)?;
        let a = self.analysis_for(&path);
        let locs = nav::definition(&a, &path, &text, tdp.position);
        Ok(if locs.is_empty() { Value::Null } else { json!(GotoDefinitionResponse::Array(locs)) })
    }

    fn hover(&mut self, p: HoverParams) -> Result<Value, (ErrorCode, String)> {
        let tdp = p.text_document_position_params;
        let (path, text) = self.doc(&tdp.text_document.uri)?;
        let a = self.analysis_for(&path);
        Ok(match nav::hover(&a, &path, &text, tdp.position) {
            Some(h) => json!(h),
            None => Value::Null,
        })
    }

    fn formatting(&mut self, p: DocumentFormattingParams) -> Result<Value, (ErrorCode, String)> {
        let (path, text) = self.doc(&p.text_document.uri)?;
        // generated bindings are rewritten by `agentmark bind`, never formatted (Q111)
        if in_bindings(&path) || !path.ends_with(".agent.md") {
            return Ok(Value::Null);
        }
        match agentmark::fmt::format_source(&text, &path) {
            Ok(out) if out == text => Ok(json!(Vec::<TextEdit>::new())),
            Ok(out) => {
                let range = lsp_types::Range::new(lsp_types::Position::new(0, 0), end_position(&text));
                Ok(json!(vec![TextEdit::new(range, out)]))
            }
            // a file that does not parse is left alone; the diagnostics say why
            Err(_) => Ok(Value::Null),
        }
    }

    fn code_action(&mut self, p: CodeActionParams) -> Result<Value, (ErrorCode, String)> {
        let (path, text) = self.doc(&p.text_document.uri)?;
        let a = self.analysis_for(&path);
        let mut acts = actions::quick_fixes(&a, &path, &text, p.range, self.snippets);
        if a.model_actions_enabled() && self.runner_for(&a).is_ok() {
            acts.extend(actions::model_actions(&p.text_document.uri, &text, p.range.start.line as usize));
        }
        if let Some(only) = &p.context.only {
            acts.retain(|c| c.kind.as_ref().is_some_and(|k| only.iter().any(|o| k.as_str().starts_with(o.as_str()))));
        }
        let list: Vec<Value> = acts
            .into_iter()
            .map(|a| {
                let mut v = json!(CodeActionOrCommand::CodeAction(a));
                actions::mark_snippets(&mut v);
                v
            })
            .collect();
        Ok(json!(list))
    }

    fn resolve(&mut self, mut action: lsp_types::CodeAction) -> Result<Value, (ErrorCode, String)> {
        let Some((uri, line)) = action.data.as_ref().and_then(actions::resolve_args) else {
            return Ok(json!(action));
        };
        let uri: Uri = uri.parse().map_err(|_| (ErrorCode::InvalidParams, "bad uri".to_string()))?;
        let (path, text) = self.doc(&uri)?;
        self.model_gate(&path)?;
        let a = self.analysis_for(&path);
        let runner = self.runner_for(&a)?;
        let edits = actions::suggest_guard(&runner, &path, &text, line).map_err(|e| (ErrorCode::RequestFailed, e))?;
        action.edit = Some(lsp_types::WorkspaceEdit { changes: Some(HashMap::from([(uri, edits)])), ..Default::default() });
        Ok(json!(action))
    }

    /// Model actions only run when the project's `agentmark.toml` has `[lsp] model_actions = true`.
    fn model_gate(&mut self, path: &str) -> Result<(), (ErrorCode, String)> {
        if self.analysis_for(path).model_actions_enabled() {
            Ok(())
        } else {
            Err((ErrorCode::InvalidRequest, "model actions are off: set [lsp] model_actions = true in agentmark.toml".to_string()))
        }
    }

    fn execute(&mut self, p: ExecuteCommandParams) -> Result<Value, (ErrorCode, String)> {
        if p.command != actions::EXPLAIN_COMMAND {
            return Err((ErrorCode::InvalidParams, format!("unknown command {}", p.command)));
        }
        let uri = p.arguments.first().and_then(Value::as_str).and_then(|s| s.parse::<Uri>().ok());
        let line = p.arguments.get(1).and_then(Value::as_u64);
        let (Some(uri), Some(line)) = (uri, line) else {
            return Err((ErrorCode::InvalidParams, "expected [uri, line]".to_string()));
        };
        let (path, text) = self.doc(&uri)?;
        self.model_gate(&path)?;
        let a = self.analysis_for(&path);
        let effects = a.facts(&path).and_then(|(ck, map)| {
            let fid = ck.files.iter().position(|f| *f == path)?;
            let (start, _) = actions::block_lines(&text, line as usize)?;
            let old = map.new_to_old(start)?;
            ck.blocks.iter().find(|b| b.file == fid && b.span.line as usize == old + 1).map(|b| b.effect.to_string())
        });
        let runner = self.runner_for(&a)?;
        let answer = actions::explain(&runner, &text, line as usize, effects.as_deref()).map_err(|e| (ErrorCode::RequestFailed, e))?;
        self.send(Notification::new(
            ShowMessage::METHOD.to_string(),
            ShowMessageParams { typ: MessageType::INFO, message: answer.clone() },
        ));
        Ok(Value::String(answer))
    }
}
