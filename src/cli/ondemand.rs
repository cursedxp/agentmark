//! The on-demand binding tier in the command line (SPEC 15.2, last row): a plain statement
//! against an item the binding lists under `## Not bound` makes the binder write that item now.
//! Shared by `check` and `build` (`run`, `trace`), like the automatic first binding.

use std::path::Path;

use super::bind;
use crate::binder::{bind_on_demand, BindOptions, OnDemandError, OnDemandUse};
use crate::diag::{Diagnostic, Span};
use crate::project::Project;
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
/// project again), `Ok(false)` = nothing to do.
pub(crate) fn resolve(project: &Project) -> Result<bool, Failure> {
    let found = find(project);
    if found.is_empty() {
        return Ok(false);
    }
    let (model, mock_today) = bind::model_from_env().map_err(Failure::Setup)?;
    let offline = matches!(std::env::var("AGENTMARK_OFFLINE").as_deref(), Ok("1") | Ok("true"));
    let mut changed = false;
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
        let mut log = |l: &str| eprintln!("{l}");
        match bind_on_demand(&f.crate_name, &uses, &opts, &mut log) {
            Ok(items) => changed |= !items.is_empty(),
            Err(OnDemandError::Setup(e)) => {
                return Err(Failure::Setup(format!("cannot write the library code for \"{}\" on demand: {e}", f.crate_name)))
            }
            Err(OnDemandError::Failed(fail)) => {
                // the statement that asked for the failed item
                let at = found.iter().find(|g| g.binding_file == f.binding_file && g.use_.item == fail.item).unwrap_or(f);
                return Err(Failure::Item(Box::new(failure_diagnostic(project, at, &fail.item))));
            }
        }
    }
    Ok(changed)
}

/// The error of SPEC 10 shape at the author's statement (documented in docs/reference.md 15.2).
fn failure_diagnostic(project: &Project, f: &Found, item: &str) -> Diagnostic {
    Diagnostic::error(
        E_ON_DEMAND,
        &project.files[f.file].path,
        f.verb,
        format!(
            "Could not bind \"{item}\" of {} on demand: the code the model wrote did not pass the type check, twice.",
            f.binding
        ),
    )
    .with_help(format!(
        "Nothing was changed. Use a ## Rust block for \"{item}\", or bind it by hand in bindings/{}.agent.md.",
        f.crate_name
    ))
}
