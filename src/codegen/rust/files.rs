//! The files of the generated project, except `main.rs` and `Cargo.toml`.

use std::collections::BTreeSet;

use super::body::{lit, Body, Kind};
use super::names::{slug, value_ident, Names, Namespace};
use super::ty::{default_expr, field_ident, rust_type, schema_word};
use super::Options;
use crate::ir::*;

pub struct Ctxt<'a> {
    pub p: &'a Program,
    pub n: &'a Names,
    pub o: &'a Options,
}

/// Does `w` occur in `s` as a whole word?
pub fn has_word(s: &str, w: &str) -> bool {
    let b = s.as_bytes();
    let mut from = 0;
    while let Some(i) = s[from..].find(w) {
        let a = from + i;
        let e = a + w.len();
        let before = a == 0 || !(b[a - 1].is_ascii_alphanumeric() || b[a - 1] == b'_');
        let after = e >= b.len() || !(b[e].is_ascii_alphanumeric() || b[e] == b'_');
        if before && after {
            return true;
        }
        from = a + 1;
    }
    false
}

fn use_line(path: &str, items: &[&str]) -> String {
    if items.len() == 1 {
        format!("use {path}::{};\n", items[0])
    } else {
        format!("use {path}::{{{}}};\n", items.join(", "))
    }
}

fn pick<'a>(body: &str, names: &[&'a str]) -> Vec<&'a str> {
    names.iter().copied().filter(|w| has_word(body, w)).collect()
}

/// `use` lines for exactly what `body` mentions (generated code stays warning free).
pub fn imports(body: &str, c: &Ctxt) -> String {
    imports_with(body, c, true)
}

fn imports_with(body: &str, c: &Ctxt, records: bool) -> String {
    let mut out = String::new();
    let rt = pick(
        body,
        &["Ctx", "Currency", "Money", "no_amounts_changed", "parse_model_json", "read_data"],
    );
    if !rt.is_empty() {
        out.push_str(&use_line("agentmark_rt", &rt));
    }
    let ch = pick(body, &["DateTime", "Duration", "NaiveDate", "Utc"]);
    if !ch.is_empty() {
        out.push_str(&use_line("chrono", &ch));
    }
    if body.contains("json!(") {
        out.push_str("use serde_json::json;\n");
    }
    let recs: Vec<&str> = c.n.records.values().map(String::as_str).filter(|r| has_word(body, r)).collect();
    if records && !recs.is_empty() {
        let mut v = recs;
        v.sort();
        out.push_str(&use_line("crate::records", &v));
    }
    let crate_items = pick(body, &["AgentError", "BadRequest", "say", "log"]);
    if !crate_items.is_empty() {
        out.push_str(&use_line("crate", &crate_items));
    }
    let mut mods: BTreeSet<String> = BTreeSet::new();
    for m in ["rules", "tools", "bindings"] {
        if body.contains(&format!("{m}::")) {
            mods.insert(m.to_string());
        }
    }
    for (_, m) in &c.n.units {
        if body.contains(&format!("{m}::")) {
            mods.insert(m.clone());
        }
    }
    if !mods.is_empty() {
        let mods: Vec<String> = mods.into_iter().collect();
        out.push_str(&use_line("crate", &mods.iter().map(String::as_str).collect::<Vec<_>>()));
    }
    out
}

fn rel(c: &Ctxt, file: &str) -> String {
    let root = c.o.root.trim_end_matches('/');
    file.strip_prefix(root).map(|r| r.trim_start_matches('/')).filter(|r| !r.is_empty()).unwrap_or(file).to_string()
}

fn doc_lines(doc: &Option<String>) -> String {
    match doc {
        Some(d) => d.lines().map(|l| format!("/// {l}\n")).collect(),
        None => String::new(),
    }
}

fn effects_header(rows: &[(String, EffectSet)]) -> String {
    let mut all = EffectSet::pure();
    for (_, e) in rows {
        all.merge(e);
    }
    let mut s = format!("// Effects: {all}\n");
    if rows.len() > 1 {
        for (label, e) in rows {
            s.push_str(&format!("//   {label}: {e}\n"));
        }
    }
    s
}

fn param_list(params: &[Param], body: &str, c: &Ctxt, assigned: &[bool]) -> String {
    params
        .iter()
        .zip(assigned)
        .map(|(p, assigned)| {
            let id = c.n.local(&p.name);
            let used = has_word(body, &id);
            let mutq = if *assigned { "mut " } else { "" };
            format!("{mutq}{}{id}: {}", if used { "" } else { "_" }, rust_type(&p.ty, c.n))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------------- functions

/// One generated function.
fn function(c: &Ctxt, module: &str, f: &Fn, doc: &str, ident: &str) -> String {
    let kind = if f.ret.is_some() { Kind::Value } else { Kind::Unit };
    let mut b = Body::new(c.p, c.n, &c.o.root, module, kind, f.ret.clone());
    b.function_body(&f.body);
    let src = b.src_comment(&f.source);
    let assigned: Vec<bool> = f.params.iter().map(|p| b.assigned(&p.name)).collect();
    let text = b.finish();
    let params = param_list(&f.params, &text, c, &assigned);
    let ctx_name = if has_word(&text, "ctx") { "ctx" } else { "_ctx" };
    let ret = f.ret.as_ref().map_or("()".to_string(), |t| rust_type(t, c.n));
    let sep = if params.is_empty() { "" } else { ", " };
    format!("{doc}{src}\npub async fn {ident}({ctx_name}: &Ctx{sep}{params}) -> Result<{ret}, AgentError> {{\n{text}\n}}\n")
}

pub fn rules_rs(c: &Ctxt) -> String {
    let mut body = String::new();
    for f in c.p.functions.iter().filter(|f| f.display.is_none()) {
        let home = &c.n.fns[&f.name];
        body.push_str(&function(c, "rules", f, &doc_lines(&f.doc), &home.ident));
        body.push('\n');
    }
    // `count of <rule> x in src`: one helper per single-argument predicate
    for f in c.p.functions.iter().filter(|f| f.display.is_none() && f.ret == Some(Type::Bool) && f.params.len() == 1) {
        let helper = &c.n.counters[&f.name];
        let me = &c.n.fns[&f.name].ident;
        let t = rust_type(&f.params[0].ty, c.n);
        body.push_str(&format!(
            "/// How many of `items` satisfy `{}`.\npub async fn {helper}(ctx: &Ctx, items: Vec<{t}>) -> Result<i64, AgentError> {{\n    let mut n = 0;\n    for item in items {{\n        if {me}(ctx, item).await? {{\n            n += 1;\n        }}\n    }}\n    Ok(n)\n}}\n\n",
            f.name
        ));
    }
    let rows: Vec<(String, EffectSet)> =
        c.p.functions.iter().filter(|f| f.display.is_none()).map(|f| (f.name.clone(), f.effect.clone())).collect();
    format!(
        "//! Functions and rules.\n{}#![allow(dead_code)]\n\n{}\n{}",
        effects_header(&rows),
        imports(&body, c),
        body.trim_end()
    ) + "\n"
}

pub fn agent_rs(c: &Ctxt, unit: &str, module: &str) -> String {
    // entries and Do blocks in the order they are written in the source
    enum Item<'b> {
        E(usize, &'b Entry),
        F(&'b Fn),
    }
    let mut items: Vec<(String, u32, Item)> = Vec::new();
    for (i, e) in c.p.entries.iter().enumerate().filter(|(_, e)| e.unit == unit) {
        items.push((e.source.file.clone(), e.source.line, Item::E(i, e)));
    }
    for f in c.p.functions.iter().filter(|f| f.display.as_ref().is_some_and(|(u, _)| u == unit)) {
        items.push((f.source.file.clone(), f.source.line, Item::F(f)));
    }
    items.sort_by(|a, b| (a.0.as_str(), a.1).cmp(&(b.0.as_str(), b.1)));
    let mut body = String::new();
    let mut rows: Vec<(String, EffectSet)> = Vec::new();
    let mut file = String::new();
    for (_, _, it) in &items {
        match it {
            Item::E(i, e) => {
                file = rel(c, &e.source.file);
                rows.push((e.label.clone(), e.effect.clone()));
                body.push_str(&entry_fn(c, module, *i, e));
            }
            Item::F(f) => {
                file = rel(c, &f.source.file);
                let (u, label) = f.display.clone().unwrap_or_default();
                rows.push((label.clone(), f.effect.clone()));
                let home = &c.n.fns[&f.name];
                let doc = format!("/// {label}\n");
                body.push_str(&function(c, module, f, &doc, &home.ident));
                let _ = u;
            }
        }
        body.push('\n');
    }
    format!(
        "//! Agent `{unit}` ({file}).\n{}\n{}\n{}",
        effects_header(&rows),
        imports(&body, c),
        body.trim_end()
    ) + "\n"
}

fn entry_fn(c: &Ctxt, module: &str, idx: usize, e: &Entry) -> String {
    let ident = &c.n.entries[idx];
    let mut b = Body::new(c.p, c.n, &c.o.root, module, Kind::Unit, None);
    let with_model = match unit_driver(c.p, &e.unit) {
        Some(d) => format!(".with_model({})", lit(&super::model_id(&d))),
        None => String::new(),
    };
    b.raw_line(&format!("#[allow(unused_variables)] let ctx = &ctx.enter({}, {}){with_model};", lit(&e.unit), lit(&e.label)));
    b.function_body(&e.body);
    let src = b.src_comment(&e.source);
    let text = b.finish();
    let param = match &e.param {
        Some(p) => format!(", {}: {}", c.n.local(&p.name), rust_type(&p.ty, c.n)),
        None => String::new(),
    };
    let when = match &e.trigger {
        Trigger::Event { tool, event } => format!("\n/// Runs once for every `{event}` the `{tool}` tool receives."),
        _ => String::new(),
    };
    format!(
        "/// {}{when}\n{src}\npub async fn {ident}(ctx: &Ctx{param}) -> Result<(), AgentError> {{\n{text}\n}}\n",
        e.label
    )
}

/// The `- model:` of an agent: the driver of the first `ask model` in its blocks.
pub fn unit_driver(p: &Program, unit: &str) -> Option<String> {
    let mut found: Option<String> = None;
    let mut look = |stmts: &[Stmt]| {
        super::body::visit_stmts(stmts, &mut |s| {
            if let StmtKind::Effect { call: EffectCall::Model(m), .. } = &s.kind {
                if found.is_none() {
                    found = m.model.clone();
                }
            }
        });
    };
    for e in p.entries.iter().filter(|e| e.unit == unit) {
        look(&e.body);
    }
    for f in p.functions.iter().filter(|f| f.display.as_ref().is_some_and(|(u, _)| u == unit)) {
        look(&f.body);
    }
    found
}

// ---------------------------------------------------------------------------------- services

pub fn service_rs(c: &Ctxt, idx: usize, s: &Service) -> String {
    let module = &c.n.services[idx];
    let mut ns = Namespace::default();
    let mut fns = String::new();
    let mut routes: Vec<(String, String, String, bool)> = Vec::new(); // (path, method, ident, takes_body)
    let mut rows: Vec<(String, EffectSet)> = Vec::new();
    for r in &s.routes {
        let ident = ns.take(&slug(&r.label));
        rows.push((r.label.clone(), r.effect.clone()));
        let mut b = Body::new(c.p, c.n, &c.o.root, module, Kind::Route, None);
        b.raw_line(&format!("#[allow(unused_variables)] let ctx = &ctx.enter({}, {});", lit(&s.name), lit(&r.label)));
        let text_probe = {
            let mut p = Body::new(c.p, c.n, &c.o.root, module, Kind::Route, None);
            p.function_body(&r.body);
            p.finish()
        };
        let uses_state = s.state.is_some() && has_word(&text_probe, "state");
        let mut appends = false;
        super::body::visit_stmts(&r.body, &mut |st| appends |= matches!(st.kind, StmtKind::Append { .. }));
        let mutates = uses_state && appends;
        if uses_state {
            b.raw_line(&format!("let {}state = state.lock().await;", if mutates { "mut " } else { "" }));
        }
        b.function_body(&r.body);
        let src = b.src_comment(&r.source);
        let text = b.finish();
        let takes_body = text.contains("body");
        let state_param = match (&s.state, uses_state) {
            (Some(rec), true) => format!(", state: &Mutex<{}>", c.n.record(rec)),
            (Some(rec), false) => format!(", _state: &Mutex<{}>", c.n.record(rec)),
            _ => String::new(),
        };
        let path_params: String = r
            .path_params
            .iter()
            .map(|p| {
                let id = c.n.local(p);
                format!(", {}{id}: String", if has_word(&text, &id) { "" } else { "_" })
            })
            .collect();
        let body_param = if takes_body { ", body: &[u8]" } else { "" };
        fns.push_str(&format!(
            "/// {}\n{src}\nasync fn {ident}(ctx: &Ctx{state_param}{path_params}{body_param}) -> Result<Reply, AgentError> {{\n{text}\n}}\n\n",
            r.label
        ));
        routes.push((r.path.clone(), r.method.clone(), ident, takes_body));
    }

    // router: one `.route` per path, methods chained
    let mut paths: Vec<String> = Vec::new();
    for (p, ..) in &routes {
        if !paths.contains(p) {
            paths.push(p.clone());
        }
    }
    let state_rec = s.state.as_ref().map(|r| c.n.record(r));
    let mut router = String::new();
    for path in &paths {
        let axum_path = axum_path(path);
        let mut chain = String::new();
        for (i, (_, method, ident, takes_body)) in routes.iter().enumerate().filter(|(_, r)| &r.0 == path) {
            let route = &s.routes[i];
            let mut extractors = vec!["State(app): State<App>".to_string()];
            let mut call_args: Vec<String> = Vec::new();
            match route.path_params.len() {
                0 => {}
                1 => {
                    let id = c.n.local(&route.path_params[0]);
                    extractors.push(format!("Path({id}): Path<String>"));
                    call_args.push(id);
                }
                _ => {
                    let ids: Vec<String> = route.path_params.iter().map(|p| c.n.local(p)).collect();
                    let tys = vec!["String"; ids.len()].join(", ");
                    extractors.push(format!("Path(({})): Path<({tys})>", ids.join(", ")));
                    call_args.extend(ids);
                }
            }
            extractors.push("uri: Uri".into());
            if *takes_body {
                extractors.push("body: Bytes".into());
                call_args.push("&body".into());
            }
            let state_arg = if state_rec.is_some() { ", &app.state" } else { "" };
            let args = if call_args.is_empty() { String::new() } else { format!(", {}", call_args.join(", ")) };
            let m = method.to_ascii_lowercase();
            let first = chain.is_empty();
            let head = if first { format!("routing::{m}") } else { format!(".{m}") };
            let h = format!(
                "{head}(|{}| async move {{\n                let result = {ident}(&app.ctx{state_arg}{args}).await;\n                reply(&app.ctx, {}, {}, {}, &uri, result)\n            }})",
                extractors.join(", "),
                lit(&s.name),
                lit(&route.label),
                lit(method)
            );
            chain.push_str(&h);
        }
        router.push_str(&format!("        .route({}, {})\n", lit(&axum_path), chain));
    }
    let (app_state_field, app_new) = match &state_rec {
        Some(r) => (format!("    state: Arc<Mutex<{r}>>,\n"), "state: Arc::new(Mutex::new(state))".to_string()),
        None => (String::new(), String::new()),
    };
    let router_args = match &state_rec {
        Some(r) => format!("ctx: Ctx, state: {r}"),
        None => "ctx: Ctx".to_string(),
    };
    let app_init = if app_new.is_empty() { "App { ctx }".to_string() } else { format!("App {{ ctx, {app_new} }}") };
    let listen = s.listen.clone().unwrap_or_else(|| "127.0.0.1:8080".into());
    let mut body = String::new();
    body.push_str(&format!(
        "/// Where the service listens unless `--serve <addr>` says otherwise.\npub const LISTEN: &str = {};\n\n",
        lit(&listen)
    ));
    body.push_str(&format!(
        "#[derive(Clone)]\nstruct App {{\n    ctx: Ctx,\n{app_state_field}}}\n\n"
    ));
    body.push_str(&format!(
        "/// The HTTP routes of `{}`.\npub fn router({router_args}) -> Router {{\n    let app = {app_init};\n    Router::new()\n{router}        .with_state(app)\n}}\n\n",
        s.name
    ));
    body.push_str(&fns);
    body.push_str(REPLY);
    let mut head_imports = String::from(
        "use axum::body::Bytes;\nuse axum::extract::{Path, State};\nuse axum::http::{StatusCode, Uri};\nuse axum::response::{IntoResponse, Response};\nuse axum::{routing, Json, Router};\nuse serde_json::Value;\nuse std::sync::Arc;\nuse tokio::sync::Mutex;\n",
    );
    if !body.contains("Bytes") || !body.contains("body: Bytes") {
        head_imports = head_imports.replace("use axum::body::Bytes;\n", "");
    }
    if !body.contains("Path(") {
        head_imports = head_imports.replace("use axum::extract::{Path, State};", "use axum::extract::State;");
    }
    if state_rec.is_none() {
        head_imports = head_imports.replace("use std::sync::Arc;\nuse tokio::sync::Mutex;\n", "");
    }
    format!(
        "//! Service `{}`.\n{}\n{head_imports}{}\n{}",
        s.name,
        effects_header(&rows),
        imports(&body, c),
        body.trim_end()
    ) + "\n"
}

fn axum_path(p: &str) -> String {
    let mut out = String::new();
    let mut it = p.chars();
    while let Some(ch) = it.next() {
        if ch == '{' {
            out.push(':');
            for c in it.by_ref() {
                if c == '}' {
                    break;
                }
                out.push(c);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

const REPLY: &str = r#"/// What a route answers.
struct Reply {
    status: u16,
    body: Option<Value>,
}

impl Reply {
    #[allow(dead_code)]
    fn status(status: u16) -> Reply {
        Reply { status, body: None }
    }

    fn json(status: u16, body: Value) -> Reply {
        Reply { status, body: Some(body) }
    }
}

/// Turn a route's result into the HTTP response and record the `http` trace line. A failure is
/// traced as an `error`; the caller only sees "internal error".
fn reply(ctx: &Ctx, unit: &str, label: &str, method: &str, uri: &Uri, result: Result<Reply, AgentError>) -> Response {
    let (status, body) = match result {
        Ok(r) => (r.status, r.body),
        Err(e) if e.downcast_ref::<BadRequest>().is_some() => (400, Some(serde_json::json!({ "error": e.to_string() }))),
        Err(e) => {
            eprintln!("{unit} / {label}: {e}");
            if let Err(t) = ctx.enter(unit, label).trace_error(&e.to_string()) {
                eprintln!("trace: {t}");
            }
            (500, Some(serde_json::json!({ "error": "internal error" })))
        }
    };
    if let Err(e) = ctx.trace_http(method, uri.path(), status) {
        eprintln!("trace: {e}");
    }
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    match body {
        Some(b) => (code, Json(b)).into_response(),
        None => code.into_response(),
    }
}
"#;

// ---------------------------------------------------------------------------------- commands

pub fn command_rs(c: &Ctxt, idx: usize, cmd: &Command) -> String {
    let module = &c.n.commands[idx];
    let mut b = Body::new(c.p, c.n, &c.o.root, module, Kind::Unit, None);
    b.raw_line(&format!("#[allow(unused_variables)] let ctx = &ctx.enter({}, \"Run\");", lit(&cmd.name)));
    b.function_body(&cmd.body);
    let src = b.src_comment(&cmd.run);
    let text = b.finish();
    let params: String = cmd
        .args
        .iter()
        .map(|a| {
            let id = c.n.local(&a.name);
            format!(", {}{id}: {}", if has_word(&text, &id) { "" } else { "_" }, rust_type(&a.ty, c.n))
        })
        .collect();
    let state = match &cmd.state {
        Some(rec) => {
            let used = has_word(&text, "state");
            format!(", {}state: {}", if used { "mut " } else { "_" }, c.n.record(rec))
        }
        None => String::new(),
    };
    let rows = vec![("Run".to_string(), cmd.effect.clone())];
    let body = format!(
        "/// Run\n{src}\npub async fn run(ctx: &Ctx{params}{state}) -> Result<(), AgentError> {{\n{text}\n}}\n"
    );
    format!("//! Command `{}`.\n{}\n{}\n{}", cmd.name, effects_header(&rows), imports(&body, c), body.trim_end()) + "\n"
}

// ---------------------------------------------------------------------------------- records

pub fn records_rs(c: &Ctxt) -> String {
    let mut body = String::new();
    for r in &c.p.records {
        let name = c.n.record(&r.name);
        body.push_str(&doc_lines(&r.doc));
        body.push_str(&format!("// {}:{}\n", rel(c, &r.source.file), r.source.line));
        body.push_str("#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n");
        body.push_str(&format!("pub struct {name} {{\n"));
        for f in &r.fields {
            let (id, rename) = field_ident(&f.name);
            if let Some(js) = rename {
                body.push_str(&format!("    #[serde(rename = {})]\n", lit(&js)));
            }
            if matches!(f.ty, Type::List(_)) {
                body.push_str("    #[serde(default)]\n");
            }
            body.push_str(&format!("    pub {id}: {},\n", rust_type(&f.ty, c.n)));
        }
        body.push_str("}\n\n");
        let hint = format!(
            "Answer with only a JSON object like {{{}}}.",
            r.fields.iter().map(|f| format!("\"{}\": {}", f.name, schema_word(&f.ty))).collect::<Vec<_>>().join(", ")
        );
        body.push_str(&format!("impl {name} {{\n    /// What to tell a model that must answer with this record.\n    pub const SCHEMA_HINT: &'static str = {};\n\n", lit(&hint)));
        body.push_str("    /// Placeholder values: what a failed `try` or an unparseable model answer leaves behind.\n");
        body.push_str("    pub fn empty() -> Self {\n        Self {\n");
        for f in &r.fields {
            body.push_str(&format!("            {}: {},\n", field_ident(&f.name).0, default_expr(&f.ty, c.n)));
        }
        body.push_str("        }\n    }\n}\n\n");
    }
    format!(
        "//! Records.\n#![allow(dead_code)]\n\nuse serde::{{Deserialize, Serialize}};\n{}\n{}",
        imports_with(&body, c, false),
        body.trim_end()
    ) + "\n"
}

// ---------------------------------------------------------------------------------- tools

fn tools_with_verbs<'a>(c: &'a Ctxt) -> Vec<&'a ToolDef> {
    c.p.tools.iter().filter(|t| !t.verbs.is_empty()).collect()
}

pub fn tools_rs(c: &Ctxt) -> String {
    let with_verbs = tools_with_verbs(c);
    let mut body = String::new();
    for t in &with_verbs {
        let tr = &c.n.tools[&t.name];
        let verbs: Vec<String> = t.verbs.iter().map(|v| value_ident(v)).collect();
        body.push_str(&format!("/// The `{}` tool ({:?}): implement it in `rust/connect.rs`.\n", t.name, t.effect));
        body.push_str(&format!("#[async_trait]\npub trait {tr}: Send + Sync {{\n"));
        for v in &verbs {
            body.push_str(&format!("    async fn {v}(&self, args: Value) -> Result<Value, RtError>;\n"));
        }
        body.push_str("}\n\n");
        body.push_str(&format!("/// Mock runs: the fixtures answer.\n#[async_trait]\nimpl {tr} for MockTools {{\n"));
        for (v, raw) in verbs.iter().zip(&t.verbs) {
            body.push_str(&format!(
                "    async fn {v}(&self, args: Value) -> Result<Value, RtError> {{\n        self.call({}, {}, &args).await\n    }}\n",
                lit(&t.name),
                lit(raw)
            ));
        }
        body.push_str("}\n\n");
    }
    // the adapter real runs use
    body.push_str("/// The tools of a real run: one implementation per tool, chosen in `rust/connect.rs`.\npub struct Connected {\n");
    for t in &with_verbs {
        body.push_str(&format!("    pub {}: Box<dyn {}>,\n", value_ident(&t.name), c.n.tools[&t.name]));
    }
    body.push_str("}\n\n");
    let args = if with_verbs.is_empty() { "_args" } else { "args" };
    body.push_str(&format!(
        "#[async_trait]\nimpl Tools for Connected {{\n    async fn call(&self, tool: &str, verb: &str, {args}: &Value) -> Result<Value, RtError> {{\n        match (tool, verb) {{\n"
    ));
    for t in &with_verbs {
        for v in &t.verbs {
            body.push_str(&format!(
                "            ({}, {}) => self.{}.{}(args.clone()).await,\n",
                lit(&t.name),
                lit(v),
                value_ident(&t.name),
                value_ident(v)
            ));
        }
    }
    body.push_str("            _ => Err(RtError::Tool(format!(\"{tool} {verb}: this program has no such tool or verb\"))),\n        }\n    }\n}\n\n");
    body.push_str(
        "/// Read a tool's collection (`crm.invoices`) as typed values.\npub async fn read_source<T: DeserializeOwned>(ctx: &Ctx, tool: &str, source: &str) -> Result<T, AgentError> {\n    let rows = ctx.tools.call(tool, \"list\", &json!({ \"source\": source })).await?;\n    Ok(serde_json::from_value(rows)?)\n}\n",
    );
    let mock = if with_verbs.is_empty() { "" } else { "MockTools, " };
    format!(
        "//! One trait per tool of tools.toml. Mock runs answer from fixtures; a real run uses the\n//! implementations `rust/connect.rs` (your project's file) returns, wrapped in `Connected`.\n#![allow(dead_code)]\n\nuse agentmark_rt::{{Ctx, {mock}RtError, Tools}};\nuse async_trait::async_trait;\nuse serde::de::DeserializeOwned;\nuse serde_json::{{json, Value}};\n\nuse crate::AgentError;\n\n{body}"
    )
}

/// `src/connect.rs`: written once, owned by the user afterwards.
pub fn connect_rs(c: &Ctxt) -> String {
    let with_verbs = tools_with_verbs(c);
    let mut s = String::from(
        "//! Connect the tools to the real world.\n//!\n//! This file is yours: it lives in your project as `rust/connect.rs` (beside agentmark.toml);\n//! `agentmark build` creates it once, when it is missing, and never changes it again. Every\n//! build copies it to `src/connect.rs` of the generated project, so edit THIS one. Implement the traits of `tools.rs` for your systems and return them from `connect()`.\n//! A real run (without --mock) uses what `connect()` returns. If tools.toml gains a tool, add it here.\n\n",
    );
    if with_verbs.is_empty() {
        s.push_str("use crate::tools::Connected;\n\n/// The tools of a real run (this program has none that need connecting).\npub fn connect() -> Connected {\n    Connected {}\n}\n");
        return s;
    }
    s.push_str("use agentmark_rt::RtError;\nuse async_trait::async_trait;\nuse serde_json::Value;\n\n");
    let traits: Vec<&str> = with_verbs.iter().map(|t| c.n.tools[&t.name].as_str()).collect();
    s.push_str(&format!("use crate::tools::{{Connected, {}}};\n\n", traits.join(", ")));
    s.push_str("/// A tool nobody has connected yet: every call says what to implement.\nstruct NotConnected;\n\n");
    for t in &with_verbs {
        let tr = &c.n.tools[&t.name];
        s.push_str(&format!("#[async_trait]\nimpl {tr} for NotConnected {{\n"));
        for v in &t.verbs {
            s.push_str(&format!(
                "    async fn {}(&self, _args: Value) -> Result<Value, RtError> {{\n        Err(RtError::Tool({}.into()))\n    }}\n",
                value_ident(v),
                lit(&format!("not connected yet: implement {tr}::{} in rust/connect.rs", value_ident(v)))
            ));
        }
        s.push_str("}\n\n");
    }
    s.push_str("/// The tools of a real run.\npub fn connect() -> Connected {\n    Connected {\n");
    for t in &with_verbs {
        s.push_str(&format!("        {}: Box::new(NotConnected),\n", value_ident(&t.name)));
    }
    s.push_str("    }\n}\n");
    s
}

// ---------------------------------------------------------------------------------- bindings

fn user_code(code: &str, file: &str, line: u32, indent: &str) -> String {
    let mut s = format!("{indent}// agentmark:begin {file}:{line}\n");
    for l in code.lines() {
        if l.is_empty() {
            s.push('\n');
        } else {
            s.push_str(&format!("{indent}{l}\n"));
        }
    }
    s.push_str(&format!("{indent}// agentmark:end\n"));
    s
}

pub fn bindings_rs(c: &Ctxt) -> String {
    let mut body = String::new();
    for r in &c.p.rust_fns {
        let f = &c.n.rust_fns[&r.name];
        let file = rel(c, &r.source.file);
        let params = r
            .params
            .iter()
            .map(|p| format!("{}: {}", c.n.local(&p.name), rust_type(&p.ty, c.n)))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = r.ret.as_ref().map_or("()".to_string(), |t| rust_type(t, c.n));
        body.push_str(&format!(
            "/// Rust block `{}` (declared effect: {}).\n// agentmark:block {file}:{}\n#[allow(unused)]\npub fn {f}({params}) -> {ret} {{\n{}}}\n\n",
            r.name,
            r.effect,
            r.source.line,
            user_code(&r.code, &file, r.code_line, "    ")
        ));
    }
    let mut seen: Vec<&str> = Vec::new();
    for b in &c.p.bindings {
        if seen.contains(&b.binding.as_str()) {
            continue;
        }
        seen.push(&b.binding);
        let st = &c.n.bindings[&b.binding];
        body.push_str(&format!(
            "/// Binding `{}` for the `{}` crate (effect: {}).\npub struct {st};\n\nimpl {st} {{\n",
            b.binding,
            b.crate_name,
            effect_word(b.effect)
        ));
        for it in c.p.bindings.iter().filter(|x| x.binding == b.binding) {
            let m = &c.n.binding_items[&(it.binding.clone(), crate::ir::binding_key(&it.item, it.on.as_deref()))];
            let file = rel(c, &it.source.file);
            let params = it
                .params
                .iter()
                .zip(&it.modes)
                .map(|(p, mode)| {
                    let ty = rust_type(&p.ty, c.n);
                    let ty = match (&p.ty, mode) {
                        (Type::Handle(_), HandleMode::Shared) => format!("&{ty}"),
                        (Type::Handle(_), HandleMode::Mut) => format!("&mut {ty}"),
                        _ => ty,
                    };
                    format!("{}: {}", c.n.local(&p.name), ty)
                })
                .collect::<Vec<_>>()
                .join(", ");
            let ret = it.ret.as_ref().map_or("()".to_string(), |t| rust_type(t, c.n));
            body.push_str(&doc_lines(&it.doc).lines().map(|l| format!("    {l}\n")).collect::<String>());
            body.push_str(&format!(
                "    // agentmark:block {file}:{}\n    pub async fn {m}({params}) -> Result<{ret}, Box<dyn std::error::Error + Send + Sync>> {{\n{}    }}\n\n",
                it.source.line,
                user_code(&it.code, &file, it.code_line, "        ")
            ));
        }
        body.push_str("}\n\n");
    }
    format!(
        "//! `## Rust` blocks and `# Binding` bodies, copied verbatim. A compiler error inside a marked\n//! region is reported against the .agent.md line it came from.\n#![allow(unused)]\n\n{}\n{}",
        imports(&body, c),
        body.trim_end()
    ) + "\n"
}

fn effect_word(e: Effect) -> &'static str {
    match e {
        Effect::Pure => "pure",
        Effect::Local => "local",
        Effect::Network => "network",
        Effect::Model => "model",
        Effect::Unverified => "unverified",
    }
}
