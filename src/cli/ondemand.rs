//! The on-demand binding tier in the command line (SPEC 15.2, last row): a plain statement
//! against an item the binding lists under `## Not bound` makes the binder write that item now.
//! Shared by `check` and `build` (`run`, `trace`), like the automatic first binding.

use std::path::Path;

use super::bind;
use crate::binder::core::ondemand::Cause;
use crate::binder::{commit_on_demand, produce_on_demand, BindOptions, OnDemandError, OnDemandUse};
use crate::diag::{Diagnostic, Span};
use crate::project::{Project, ResolvedImport};
use crate::syntax::ast::{Decl, Stmt, StmtKind, TargetPart};

/// E220: the model could not produce code that compiles, after two attempts.
pub const E_ON_DEMAND: &str = "E220";

pub(crate) enum Failure {
    /// The tier could not even start or save (no model, no rustdoc, unreadable file).
    Setup(String),
    /// The item could not be written; the diagnostic is at the author's statement.
    Item(Box<Diagnostic>),
}

struct Found {
    file: usize,
    binding_file: usize,
    binding: String,
    crate_name: String,
    verb: Span,
    use_: OnDemandUse,
}

fn statements(project: &Project, file: usize) -> Vec<&Stmt> {
    let mut out: Vec<&Stmt> = Vec::new();
    let mut bodies: Vec<&[Stmt]> = Vec::new();
    for d in &project.files[file].ast.decls {
        match d {
            Decl::Function(f) => bodies.push(&f.body),
            Decl::Command(c) => bodies.push(&c.body),
            Decl::Service(s) => bodies.extend(s.routes.iter().map(|r| r.body.as_slice())),
            Decl::Agent(a) => bodies.extend(a.blocks.iter().map(|b| b.body.as_slice())),
            Decl::Entry(e) => bodies.push(&e.body),
            _ => {}
        }
    }
    fn walk<'a>(stmts: &'a [Stmt], out: &mut Vec<&'a Stmt>) {
        for s in stmts {
            out.push(s);
            match &s.kind {
                StmtKind::If { then, else_, .. } | StmtKind::IfSome { then, else_, .. } => {
                    walk(then, out);
                    walk(else_, out);
                }
                StmtKind::ForEach { body, .. } => walk(body, out),
                StmtKind::Coll(c) => {
                    if let Some(b) = c.body() {
                        walk(b, out);
                    }
                }
                StmtKind::Parallel(b) => walk(b, out),
                StmtKind::Try { stmt, otherwise } => {
                    walk(std::slice::from_ref(stmt), out);
                    if let Some(o) = otherwise {
                        walk(o, out);
                    }
                }
                _ => {}
            }
        }
    }
    for b in bodies {
        walk(b, &mut out);
    }
    out
}

/// Every statement that names an item its binding lists under `## Not bound`.
fn find(project: &Project) -> Vec<Found> {
    let mut bindings: Vec<(usize, &crate::syntax::ast::Binding)> = Vec::new();
    for (i, f) in project.files.iter().enumerate() {
        for d in &f.ast.decls {
            if let Decl::Binding(b) = d {
                if !b.not_bound.is_empty() {
                    bindings.push((i, b));
                }
            }
        }
    }
    let mut out = Vec::new();
    if bindings.is_empty() {
        return out;
    }
    for file in 0..project.files.len() {
        for s in statements(project, file) {
            let StmtKind::ToolCall { tool, verb: Some(verb), target, .. } = &s.kind else { continue };
            let Some((bf, b)) = bindings.iter().find(|(_, b)| b.name.name == tool.name) else { continue };
            // only a binding this file imports (`# Import: crate x`) is one it may extend
            let imported = project.files[file]
                .imports
                .iter()
                .any(|i| matches!(i, ResolvedImport::Crate { name, .. } if *name == b.crate_name.name));
            if !imported {
                continue;
            }
            if !b.not_bound.iter().any(|n| n.item == verb.name) || b.items.iter().any(|i| i.name.name == verb.name) {
                continue;
            }
            let line = project.files[file].text.lines().nth(s.span.line.saturating_sub(1) as usize).unwrap_or("");
            let statement = line.trim().trim_start_matches("- ").trim().to_string();
            let arity = target.iter().filter(|t| matches!(t, TargetPart::Word(_) | TargetPart::Value(_))).count();
            out.push(Found {
                file,
                binding_file: *bf,
                binding: b.name.name.clone(),
                crate_name: b.crate_name.name.clone(),
                verb: verb.span,
                use_: OnDemandUse { item: verb.name.clone(), statement, arity },
            });
        }
    }
    out
}

/// Write every item the project's statements need. `Ok(true)` = a binding file changed (load the
/// project again), `Ok(false)` = nothing to do. All crates are produced first and saved only when
/// every one of them succeeded.
pub(crate) fn resolve(project: &Project) -> Result<bool, Failure> {
    let found = find(project);
    if found.is_empty() {
        return Ok(false);
    }
    let (model, mock_today) = bind::model_for(bind::provider_config(project)).map_err(Failure::Setup)?;
    let offline = matches!(std::env::var("AGENTMARK_OFFLINE").as_deref(), Ok("1") | Ok("true"));
    let mut produced = Vec::new();
    let mut done: Vec<usize> = Vec::new();
    for f in &found {
        if done.contains(&f.binding_file) {
            continue;
        }
        done.push(f.binding_file);
        let uses: Vec<OnDemandUse> =
            found.iter().filter(|g| g.binding_file == f.binding_file).map(|g| g.use_.clone()).collect();
        // the binding file as the project loaded it: `<root>/bindings/<crate>.agent.md`
        let dir = Path::new(&project.files[f.binding_file].path).parent().map(Path::to_path_buf).unwrap_or_default();
        let mut opts = BindOptions::new(&dir, model.clone(), mock_today.unwrap_or_else(bind::system_today));
        opts.offline = offline;
        bind::apply_crate_config(&mut opts, project);
        let mut log = |l: &str| eprintln!("{l}");
        match produce_on_demand(&f.crate_name, &uses, &opts, &mut log) {
            Ok(p) => produced.extend(p),
            Err(OnDemandError::Setup(e)) => {
                return Err(Failure::Setup(format!("cannot write the library code for \"{}\" on demand: {e}", f.crate_name)))
            }
            Err(OnDemandError::Failed(fail)) => {
                // the statement that asked for the failed item
                let at = found.iter().find(|g| g.binding_file == f.binding_file && g.use_.item == fail.item).unwrap_or(f);
                let log_path = write_log(project, at, &fail);
                return Err(Failure::Item(Box::new(failure_diagnostic(project, at, &fail, log_path.as_deref()))));
            }
        }
    }
    if produced.is_empty() {
        return Ok(false);
    }
    commit_on_demand(&produced).map_err(|e| Failure::Setup(format!("cannot save the new library code: {e}")))?;
    Ok(true)
}

/// Where the details of a failed item go: `<project>/target/agentmark/ondemand-<crate>-<item>.log`.
pub(crate) fn log_path(project: &Project, crate_name: &str, item: &str) -> std::path::PathBuf {
    Path::new(&project.root).join("target/agentmark").join(format!("ondemand-{crate_name}-{item}.log"))
}

/// Every complaint, compiler text included, for a bug report. `None` if it cannot be written.
fn write_log(project: &Project, f: &Found, fail: &crate::binder::core::ondemand::Failure) -> Option<String> {
    let path = log_path(project, &f.crate_name, &fail.item);
    std::fs::create_dir_all(path.parent()?).ok()?;
    let mut text = format!("on-demand binding of \"{}\" ({}), statement: {}\n\n", fail.item, f.crate_name, f.use_.statement);
    for (i, c) in fail.complaints.iter().enumerate() {
        text.push_str(&format!("attempt {}: {c}\n", i + 1));
    }
    std::fs::write(&path, text).ok()?;
    Some(path.display().to_string())
}

/// The error of SPEC 10 shape at the author's statement (documented in docs/reference.md 15.2).
fn failure_diagnostic(
    project: &Project,
    f: &Found,
    fail: &crate::binder::core::ondemand::Failure,
    log: Option<&str>,
) -> Diagnostic {
    let item = &fail.item;
    let why = match fail.cause {
        Cause::NoAnswer => format!("the model could not be reached: {}.", fail.detail),
        Cause::Signature => format!(
            "the signature the model chose does not fit your statement ({}).",
            fail.detail
        ),
        Cause::Compile => "the code the model wrote did not pass the type check, twice.".to_string(),
    };
    let mut help = format!(
        "Nothing was changed. Use a ## Rust block for \"{item}\", or bind it by hand in bindings/{}.agent.md.",
        f.crate_name
    );
    if let Some(l) = log {
        help.push_str(&format!(" Details for a bug report: {l}"));
    }
    Diagnostic::error(
        E_ON_DEMAND,
        &project.files[f.file].path,
        f.verb,
        format!("Could not bind \"{item}\" of {} on demand: {why}", f.binding),
    )
    .with_help(help)
}
