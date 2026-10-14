//! `parallel:` blocks (SPEC 16): statements inside must not write the same variable.

use std::collections::HashMap;

use crate::diag::{Diagnostic, Span};
use crate::syntax::ast::{ExprKind, Stmt, StmtKind};

pub const E_PARALLEL: &str = "E212";

/// Variables a statement writes (results of `->`, `set`, `append`), with the span of the write.
pub fn writes(s: &Stmt, out: &mut Vec<(String, Span)>) {
    match &s.kind {
        StmtKind::Do { into: Some(i), .. }
        | StmtKind::AskModel { into: i, .. }
        | StmtKind::ToolCall { into: Some(i), .. }
        | StmtKind::ReadBody { into: i, .. }
        | StmtKind::ReadFile { into: i, .. }
        | StmtKind::ReadInput { into: i }
        | StmtKind::CallInto { into: i, .. }
        | StmtKind::File { into: Some(i), .. } => out.push((i.name.clone(), s.span)),
        StmtKind::Set { var, .. } => out.push((var.name.clone(), s.span)),
        StmtKind::Append { list, .. } => {
            if let ExprKind::Path(segs) = &list.kind {
                out.push((segs[0].name.clone(), s.span));
            }
        }
        StmtKind::ForEach { body, .. } => body.iter().for_each(|b| writes(b, out)),
        StmtKind::If { then, else_, .. } | StmtKind::IfSome { then, else_, .. } => {
            then.iter().chain(else_).for_each(|b| writes(b, out));
        }
        StmtKind::Match { arms, otherwise, .. } => {
            arms.iter().flat_map(|a| &a.body).for_each(|b| writes(b, out));
            otherwise.iter().flat_map(|o| &o.body).for_each(|b| writes(b, out));
        }
        StmtKind::Guard { else_: Some(e), .. } => writes(e, out),
        StmtKind::Try { stmt, otherwise } => {
            writes(stmt, out);
            otherwise.iter().flatten().for_each(|b| writes(b, out));
        }
        StmtKind::Parallel(body) => body.iter().for_each(|b| writes(b, out)),
        _ => {}
    }
}

/// One error per variable written by more than one statement of the block.
pub fn conflicts(file: &str, body: &[Stmt]) -> Vec<Diagnostic> {
    let mut first: HashMap<String, (usize, u32)> = HashMap::new();
    let mut diags = Vec::new();
    let mut reported: Vec<String> = Vec::new();
    for (i, s) in body.iter().enumerate() {
        let mut w = Vec::new();
        writes(s, &mut w);
        for (name, _) in w {
            match first.get(&name) {
                Some((j, line)) if *j != i && !reported.contains(&name) => {
                    reported.push(name.clone());
                    diags.push(
                        Diagnostic::error(
                            E_PARALLEL,
                            file,
                            s.span,
                            format!(
                                "Two statements of this \"parallel\" block both write \"{name}\" (the first is on line {line})."
                            ),
                        )
                        .with_help("Give each statement its own variable, or run them one after the other."),
                    );
                }
                Some(_) => {}
                None => {
                    first.insert(name, (i, s.span.line));
                }
            }
        }
    }
    diags
}
