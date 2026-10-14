//! An in-process LSP client: the server runs on a thread behind `Connection::memory()`.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use agentmark_rt::ModelClient;
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{Initialize, Shutdown};
use lsp_types::{
    ClientCapabilities, Diagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, InitializeParams, InitializedParams, Position,
    PublishDiagnosticsParams, Range, TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem, Uri,
    VersionedTextDocumentIdentifier, WorkspaceClientCapabilities, DidChangeWatchedFilesClientCapabilities,
};
use serde::de::DeserializeOwned;
use serde::Serialize;

const WAIT: Duration = Duration::from_secs(20);

pub struct Client {
    conn: Connection,
    next: i32,
    thread: Option<JoinHandle<()>>,
    /// Notifications that arrived while waiting for something else.
    queue: Vec<Notification>,
    pub registrations: usize,
}

pub fn uri(path: &Path) -> Uri {
    agentmark_lsp::uri::uri_from_path(&path.to_string_lossy())
}

impl Client {
    pub fn start(model: Option<Arc<dyn ModelClient>>) -> Client {
        Client::start_with(model, false)
    }

    /// `snippets`: the client announces `experimental.snippetTextEdit`.
    pub fn start_with(model: Option<Arc<dyn ModelClient>>, snippets: bool) -> Client {
        let (server, conn) = Connection::memory();
        let thread = std::thread::spawn(move || {
            agentmark_lsp::serve(server, model).expect("server ends cleanly");
        });
        let mut c = Client { conn, next: 1, thread: Some(thread), queue: Vec::new(), registrations: 0 };
        let params = InitializeParams {
            capabilities: ClientCapabilities {
                experimental: snippets.then(|| serde_json::json!({ "snippetTextEdit": true })),
                workspace: Some(WorkspaceClientCapabilities {
                    did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
                        dynamic_registration: Some(true),
                        relative_pattern_support: None,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let _: lsp_types::InitializeResult = c.request::<Initialize>(params);
        c.notify_raw("initialized", InitializedParams {});
        c
    }

    fn notify_raw(&mut self, method: &str, params: impl Serialize) {
        self.conn.sender.send(Message::Notification(Notification::new(method.to_string(), params))).unwrap();
    }

    pub fn notify_value(&mut self, method: &str, params: serde_json::Value) {
        self.notify_raw(method, params);
    }

    pub fn notify<N: lsp_types::notification::Notification>(&mut self, params: N::Params) {
        self.notify_raw(N::METHOD, params);
    }

    fn recv(&mut self) -> Message {
        self.conn.receiver.recv_timeout(WAIT).expect("the server answered in time")
    }

    /// Send a request and wait for its answer (server requests are answered with `null`).
    pub fn request<R: lsp_types::request::Request>(&mut self, params: R::Params) -> R::Result
    where
        R::Result: DeserializeOwned,
    {
        let v = self.request_value(R::METHOD, serde_json::to_value(params).unwrap());
        serde_json::from_value(v).expect("the result has the documented shape")
    }

    pub fn request_value(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        self.try_request(method, params).unwrap_or_else(|e| panic!("{method} failed: {e}"))
    }

    pub fn try_request(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
        let id = RequestId::from(self.next);
        self.next += 1;
        self.conn.sender.send(Message::Request(Request { id: id.clone(), method: method.to_string(), params })).unwrap();
        loop {
            match self.recv() {
                Message::Response(Response { id: rid, result, error, .. }) if rid == id => {
                    return match error {
                        Some(e) => Err(e.message),
                        None => Ok(result.unwrap_or(serde_json::Value::Null)),
                    };
                }
                Message::Response(_) => {}
                Message::Request(r) => {
                    if r.method == "client/registerCapability" {
                        self.registrations += 1;
                    }
                    self.conn.sender.send(Message::Response(Response::new_ok(r.id, serde_json::Value::Null))).unwrap();
                }
                Message::Notification(n) => self.queue.push(n),
            }
        }
    }

    pub fn open(&mut self, path: &Path, text: &str, version: i32) {
        self.notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: TextDocumentItem { uri: uri(path), language_id: "agentmark".into(), version, text: text.into() },
        });
    }

    pub fn change(&mut self, path: &Path, text: &str, version: i32) {
        self.notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier { uri: uri(path), version },
            content_changes: vec![TextDocumentContentChangeEvent { range: None, range_length: None, text: text.into() }],
        });
    }

    pub fn save(&mut self, path: &Path) {
        self.notify::<DidSaveTextDocument>(DidSaveTextDocumentParams { text_document: TextDocumentIdentifier { uri: uri(path) }, text: None });
    }

    pub fn close(&mut self, path: &Path) {
        self.notify::<DidCloseTextDocument>(DidCloseTextDocumentParams { text_document: TextDocumentIdentifier { uri: uri(path) } });
    }

    /// The next diagnostics published for `path` at document `version` (`None` = any version).
    pub fn diagnostics(&mut self, path: &Path, version: Option<i32>) -> Vec<Diagnostic> {
        let want = uri(path);
        loop {
            let found = self.queue.iter().position(|n| {
                n.method == PublishDiagnostics::METHOD && {
                    let p: PublishDiagnosticsParams = serde_json::from_value(n.params.clone()).unwrap();
                    p.uri == want && (version.is_none() || p.version == version)
                }
            });
            if let Some(i) = found {
                let n = self.queue.remove(i);
                return serde_json::from_value::<PublishDiagnosticsParams>(n.params).unwrap().diagnostics;
            }
            match self.recv() {
                Message::Notification(n) => self.queue.push(n),
                Message::Request(r) => {
                    if r.method == "client/registerCapability" {
                        self.registrations += 1;
                    }
                    self.conn.sender.send(Message::Response(Response::new_ok(r.id, serde_json::Value::Null))).unwrap();
                }
                Message::Response(_) => {}
            }
        }
    }

    /// A notification of this method that is already queued or arrives next.
    pub fn notification(&mut self, method: &str) -> Notification {
        loop {
            if let Some(i) = self.queue.iter().position(|n| n.method == method) {
                return self.queue.remove(i);
            }
            if let Message::Notification(n) = self.recv() {
                self.queue.push(n);
            }
        }
    }

    pub fn shutdown(mut self) {
        let _: () = self.request::<Shutdown>(());
        self.notify_raw("exit", ());
        if let Some(t) = self.thread.take() {
            t.join().expect("server thread ends");
        }
    }
}

/// A project in a temp folder. Files are `(relative path, text)`.
pub fn project(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    for (rel, text) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    (dir, root)
}

/// Copy an example project of the repository into a temp folder.
pub fn example(name: &str) -> (tempfile::TempDir, PathBuf) {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap() {
            let e = e.unwrap();
            let t = to.join(e.file_name());
            if e.path().is_dir() {
                copy(&e.path(), &t);
            } else {
                std::fs::copy(e.path(), t).unwrap();
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name), &root);
    (dir, root)
}

/// The position of the `n`-th (0-based) occurrence of `needle` in `text`, `shift` characters in.
pub fn at(text: &str, needle: &str, n: usize, shift: usize) -> Position {
    let mut from = 0;
    let mut idx = 0;
    for _ in 0..=n {
        idx = from + text[from..].find(needle).unwrap_or_else(|| panic!("{needle:?} not in text"));
        from = idx + 1;
    }
    let idx = idx + needle.char_indices().nth(shift).map_or(needle.len(), |(i, _)| i);
    let line = text[..idx].matches('\n').count();
    let col_start = text[..idx].rfind('\n').map_or(0, |i| i + 1);
    Position::new(line as u32, text[col_start..idx].encode_utf16().count() as u32)
}

pub fn range_of(text: &str, needle: &str) -> Range {
    let start = at(text, needle, 0, 0);
    Range::new(start, Position::new(start.line, start.character + needle.encode_utf16().count() as u32))
}
