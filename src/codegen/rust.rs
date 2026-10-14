//! Rust code generator (DESIGN 4.3, SPEC 9.1): IR -> a Cargo project.
//!
//! ```text
//! Cargo.toml  src/main.rs  src/records.rs  src/rules.rs  src/tools.rs  src/bindings.rs
//! src/agent_<name>.rs  src/service_<name>.rs  src/command_<name>.rs
//! ```
//! Reads the IR only. Tests compare the behavior of the built program (its trace), never the text.

use std::collections::BTreeMap;

mod body;
mod files;
mod main_rs;
pub mod names;
mod ty;

use files::Ctxt;
use names::Names;

use crate::ir::Program;

/// What codegen needs besides the IR.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Project root, stripped from source paths in the `// file.agent.md:line` comments.
    pub root: String,
    /// Absolute path of the `agentmark-rt` crate the generated project depends on.
    pub rt_path: String,
    /// Crate -> version from its binding header (pinned as `=version` in Cargo.toml).
    pub crate_versions: BTreeMap<String, String>,
    /// Package name when `agentmark.toml` has none.
    pub fallback_name: String,
}

#[derive(Debug, Clone)]
pub struct GeneratedFile {
    /// Relative to the project folder: `src/main.rs`.
    pub path: String,
    pub content: String,
    /// Written once and never overwritten (`src/connect.rs`).
    pub user_owned: bool,
}

/// The `agentmark-rt` crate of the checkout this compiler was built from.
pub fn default_rt_path() -> String {
    format!("{}/crates/agentmark-rt", env!("CARGO_MANIFEST_DIR"))
}

/// The Anthropic model id for a `- model:` driver (`claude`, `claude:haiku`, `claude:<id>`).
pub fn model_id(driver: &str) -> String {
    match driver.strip_prefix("claude:") {
        None => "claude-sonnet-4-5".to_string(),
        Some("haiku") => "claude-haiku-4-5".to_string(),
        Some("sonnet") => "claude-sonnet-4-5".to_string(),
        Some("opus") => "claude-opus-4-1".to_string(),
        Some(other) => other.to_string(),
    }
}

/// Things the author should know about the generated program (printed by `build`).
pub fn notes(p: &Program, root: &str) -> Vec<String> {
    use crate::ir::{Entry, StmtKind, Trigger};
    let rel = |f: &str| f.strip_prefix(root.trim_end_matches('/')).map(|r| r.trim_start_matches('/')).unwrap_or(f).to_string();
    let mut out = Vec::new();
    for e in p.entries.iter().filter(|e| e.memory_file) {
        out.push(format!("warning: {}:{}: `memory: file` is not supported in v0.1 and is ignored.", rel(&e.source.file), e.source.line));
    }
    let mut check = |stmts: &[crate::ir::Stmt]| {
        body::visit_stmts(stmts, &mut |s| {
            if let StmtKind::Parallel(list) = &s.kind {
                let together = list.len() > 1 && list.iter().all(|x| matches!(x.kind, StmtKind::Effect { .. }));
                if !together {
                    out.push(format!(
                        "note: {}:{}: this `parallel:` runs in order; only a list of two or more tool, model or binding steps runs together.",
                        rel(&s.source.file),
                        s.source.line
                    ));
                }
            }
        });
    };
    for e in &p.entries {
        check(&e.body);
    }
    for f in &p.functions {
        check(&f.body);
    }
    for sv in &p.services {
        for r in &sv.routes {
            check(&r.body);
        }
    }
    for c in &p.commands {
        check(&c.body);
    }
    let _: Option<&Entry> = None;
    for (unit, _) in &Names::build(p).units {
        let mine: Vec<_> = p.entries.iter().filter(|e| &e.unit == unit).collect();
        if !mine.is_empty() && mine.iter().all(|e| matches!(e.trigger, Trigger::Event { .. })) {
            out.push(format!(
                "note: agent {unit} has only event entries (`When <tool> receives <event>`): they run with --once (mock), but a normal run needs a real tool connection (src/connect.rs)."
            ));
        }
    }
    out
}

/// The files of the generated Cargo project.
pub fn generate(p: &Program, o: &Options) -> Vec<GeneratedFile> {
    let n = Names::build(p);
    let c = Ctxt { p, n: &n, o };
    let mut out = vec![
        GeneratedFile { path: "Cargo.toml".into(), content: cargo_toml(p, o), user_owned: false },
        GeneratedFile { path: "src/main.rs".into(), content: main_rs::main_rs(&c), user_owned: false },
        GeneratedFile { path: "src/records.rs".into(), content: files::records_rs(&c), user_owned: false },
        GeneratedFile { path: "src/rules.rs".into(), content: files::rules_rs(&c), user_owned: false },
        GeneratedFile { path: "src/tools.rs".into(), content: files::tools_rs(&c), user_owned: false },
        GeneratedFile { path: "src/bindings.rs".into(), content: files::bindings_rs(&c), user_owned: false },
        GeneratedFile { path: "src/connect.rs".into(), content: files::connect_rs(&c), user_owned: true },
    ];
    for (unit, module) in &n.units {
        out.push(GeneratedFile { path: format!("src/{module}.rs"), content: files::agent_rs(&c, unit, module), user_owned: false });
    }
    for (i, s) in p.services.iter().enumerate() {
        out.push(GeneratedFile { path: format!("src/{}.rs", n.services[i]), content: files::service_rs(&c, i, s), user_owned: false });
    }
    for (i, cmd) in p.commands.iter().enumerate() {
        out.push(GeneratedFile { path: format!("src/{}.rs", n.commands[i]), content: files::command_rs(&c, i, cmd), user_owned: false });
    }
    out
}

fn package_name(p: &Program, o: &Options) -> String {
    let raw = p.package.name.clone().unwrap_or_else(|| o.fallback_name.clone());
    let mut s: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '-' })
        .collect();
    if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert_str(0, "agentmark-");
    }
    s
}

fn cargo_toml(p: &Program, o: &Options) -> String {
    let mut s = String::new();
    s.push_str("# Generated by agentmark. Do not edit.\n[package]\n");
    s.push_str(&format!("name = {:?}\n", package_name(p, o)));
    s.push_str(&format!("version = {:?}\n", p.package.version.clone().unwrap_or_else(|| "0.1.0".into())));
    s.push_str("edition = \"2021\"\npublish = false\n\n[dependencies]\n");
    s.push_str(&format!("agentmark-rt = {{ path = {:?} }}\n", o.rt_path));
    s.push_str("async-trait = \"0.1\"\n");
    s.push_str("chrono = { version = \"0.4\", default-features = false, features = [\"std\", \"clock\", \"serde\"] }\n");
    s.push_str("clap = { version = \"4\", features = [\"derive\"] }\n");
    s.push_str("serde = { version = \"1\", features = [\"derive\"] }\n");
    s.push_str("serde_json = \"1\"\n");
    s.push_str("tokio = { version = \"1\", features = [\"full\"] }\n");
    if !p.services.is_empty() {
        s.push_str("axum = \"0.7\"\ntower = { version = \"0.5\", features = [\"util\"] }\n");
    }
    for cr in &p.crates {
        let ver = o.crate_versions.get(&cr.name).map(|v| format!("={v}")).unwrap_or_else(|| "*".into());
        match &cr.alias {
            Some(a) => s.push_str(&format!("{a} = {{ package = {:?}, version = {:?} }}\n", cr.name, ver)),
            None => s.push_str(&format!("{} = {:?}\n", cr.name, ver)),
        }
    }
    s.push_str("\n# keep the project standalone, even when it is written inside another workspace\n[workspace]\n");
    s
}
