//! Hard limits (SPEC 18.3). They cannot be switched off, and every message says how to fix it.

use crate::diag::{Diagnostic, Span};
use crate::project::ProjectKind;
use crate::syntax::ast::{Decl, Stmt, StmtKind};

use super::Checker;

pub const E_LIMIT: &str = "E211";
pub const MAX_NESTING: usize = 3;
pub const MAX_STATEMENTS: usize = 25;
pub const MAX_LINE: usize = 120;

pub fn check(ck: &mut Checker<'_>) {
    let p = ck.p;
    for (fi, f) in p.files.iter().enumerate() {
        let mut diags = Vec::new();
        line_lengths(&f.path, &f.text, &mut diags);
        declarations(p.kind, &f.path, &f.rel_dir, &f.ast.decls, &mut diags);
        for d in &f.ast.decls {
            match d {
                Decl::Agent(a) => {
                    for b in &a.blocks {
                        let what = format!("The block \"{}\"", b.label);
                        body(&f.path, &what, b.span, &b.body, Fix::DoBlock, &mut diags);
                    }
                }
                Decl::Function(x) => {
                    let what = format!("The function \"{}\"", x.name.name);
                    body(
                        &f.path,
                        &what,
                        x.name.span,
                        &x.body,
                        Fix::Function,
                        &mut diags,
                    );
                }
                Decl::Service(s) => {
                    for r in &s.routes {
                        let what = format!("The route \"{} {}\"", r.method, r.path);
                        body(&f.path, &what, r.span, &r.body, Fix::Function, &mut diags);
                    }
                }
                Decl::Command(c) => {
                    let what = format!("The command \"{}\"", c.name.name);
                    body(
                        &f.path,
                        &what,
                        c.name.span,
                        &c.body,
                        Fix::Function,
                        &mut diags,
                    );
                }
                _ => {}
            }
        }
        let _ = fi;
        ck.diags.extend(diags);
    }
}

// ---------------------------------------------------------------------------------- line length

/// Length of a line for the 120-character limit. The text of a string literal is prose that
/// cannot be broken across lines, so it does not count (OPEN_QUESTIONS Q35); the quotes do.
fn counted_len(line: &str, in_fence: bool) -> usize {
    if in_fence {
        return line.chars().count();
    }
    let mut n = 0;
    let mut in_str = false;
    let mut escaped = false;
    for c in line.chars() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
                n += 1;
            }
        } else {
            n += 1;
            if c == '"' {
                in_str = true;
            }
        }
    }
    n
}

fn line_lengths(path: &str, text: &str, out: &mut Vec<Diagnostic>) {
    let mut in_fence = false;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let is_fence = trimmed.starts_with("```");
        if is_fence {
            in_fence = !in_fence;
        }
        let len = counted_len(line, in_fence || is_fence);
        if len > MAX_LINE {
            let total = line.chars().count();
            let help = if line.contains('"') && total > len + 20 {
                "Shorten the statement around the text, or put the long text into a \"# Constant\" and use its name."
            } else {
                "Split the statement into shorter ones, or use shorter names."
            };
            out.push(
                Diagnostic::error(
                    E_LIMIT,
                    path,
                    Span::new(
                        i as u32 + 1,
                        MAX_LINE as u32 + 1,
                        (total - MAX_LINE).max(1) as u32,
                    ),
                    format!("This line is {len} characters long; the limit is {MAX_LINE}."),
                )
                .with_help(help),
            );
        }
    }
}

// ---------------------------------------------------------------------------------- declarations

fn declarations(
    kind: ProjectKind,
    path: &str,
    rel_dir: &str,
    decls: &[Decl],
    out: &mut Vec<Diagnostic>,
) {
    let mut agents = Vec::new();
    let mut services = Vec::new();
    let mut commands = Vec::new();
    let mut records = Vec::new();
    let mut rules = Vec::new();
    for d in decls {
        match d {
            Decl::Agent(a) => agents.push(&a.name),
            Decl::Service(s) => services.push(&s.name),
            Decl::Command(c) => commands.push(&c.name),
            Decl::Record(r) => records.push(&r.name),
            Decl::Rule(r) => rules.push(&r.name),
            _ => {}
        }
    }
    for (noun, folder, list) in [
        ("agent", "agents", &agents),
        ("service", "services", &services),
        ("command", "commands", &commands),
    ] {
        if let [first, second, ..] = list.as_slice() {
            out.push(
                Diagnostic::error(
                    E_LIMIT,
                    path,
                    second.span,
                    format!(
                        "A file may hold only one {noun}, but \"{}\" is the second (the first is \"{}\").",
                        second.name, first.name
                    ),
                )
                .with_help(format!(
                    "Move \"{}\" into its own file, for example \"{folder}/{}.agent.md\", and import it.",
                    second.name, second.name
                )),
            );
        }
    }
    // Records and rules share a file only inside `records/` and `rules/`. A single-file project is
    // a whole program in one file and is exempt (OPEN_QUESTIONS Q34).
    if kind == ProjectKind::Folder {
        let in_dir = |d: &str| rel_dir == d || rel_dir.starts_with(&format!("{d}/"));
        if let [_, second, ..] = records.as_slice() {
            if !in_dir("records") {
                out.push(
                    Diagnostic::error(
                        E_LIMIT,
                        path,
                        second.span,
                        "Several records in one file are only allowed in a \"records/\" folder.",
                    )
                    .with_help(format!(
                        "Move \"{}\" into its own file, for example \"records/{}.agent.md\".",
                        second.name, second.name
                    )),
                );
            }
        }
        if let [_, second, ..] = rules.as_slice() {
            if !in_dir("rules") {
                out.push(
                    Diagnostic::error(
                        E_LIMIT,
                        path,
                        second.span,
                        "Several rules in one file are only allowed in a \"rules/\" folder.",
                    )
                    .with_help(format!(
                        "Move \"{}\" into its own file, for example \"rules/{}.agent.md\".",
                        second.name, second.name
                    )),
                );
            }
        }
        if let (Some(r), Some(ru)) = (records.first(), rules.first()) {
            let later = if r.span.line > ru.span.line { r } else { ru };
            out.push(
                Diagnostic::error(E_LIMIT, path, later.span, "Records and rules cannot share a file.")
                    .with_help("Put records into \"records/\" and rules into \"rules/\", one declaration per file."),
            );
        }
    }
}

// ---------------------------------------------------------------------------------- bodies

#[derive(Clone, Copy)]
enum Fix {
    DoBlock,
    Function,
}

impl Fix {
    fn text(self, from: u32, to: u32) -> String {
        match self {
            Fix::DoBlock => {
                format!("Move lines {from}–{to} into a \"## Do <name>\" block and call it with \"do <name>\".")
            }
            Fix::Function => format!(
                "Move lines {from}–{to} into a separate \"# Function\" and call it with \"do <name>(...)\"."
            ),
        }
    }
}

fn children(s: &Stmt) -> Vec<&[Stmt]> {
    match &s.kind {
        StmtKind::ForEach { body, .. } => vec![body],
        StmtKind::If { then, else_, .. } | StmtKind::IfSome { then, else_, .. } => {
            vec![then, else_]
        }
        StmtKind::Try { stmt, otherwise } => {
            let mut v = vec![std::slice::from_ref(&**stmt)];
            if let Some(o) = otherwise {
                v.push(o);
            }
            v
        }
        StmtKind::Parallel(b) => vec![b],
        StmtKind::Guard { else_: Some(e), .. } => vec![std::slice::from_ref(&**e)],
        _ => vec![],
    }
}

/// Does the statement open a nesting level for what is inside it?
fn is_compound(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::ForEach { .. }
        | StmtKind::If { .. }
        | StmtKind::IfSome { .. }
        | StmtKind::Parallel(_) => true,
        StmtKind::Try { otherwise, .. } => otherwise.is_some(),
        _ => false,
    }
}

fn count(stmts: &[Stmt]) -> usize {
    stmts
        .iter()
        .map(|s| 1 + children(s).iter().map(|c| count(c)).sum::<usize>())
        .sum()
}

fn last_line(s: &Stmt) -> u32 {
    children(s)
        .iter()
        .flat_map(|c| c.iter())
        .map(last_line)
        .max()
        .unwrap_or(0)
        .max(s.span.line)
}

fn body(
    path: &str,
    what: &str,
    heading: Span,
    stmts: &[Stmt],
    fix: Fix,
    out: &mut Vec<Diagnostic>,
) {
    if stmts.is_empty() {
        out.push(
            Diagnostic::error(E_LIMIT, path, heading, format!("{what} has no statements."))
                .with_help("An empty block hides what was meant. Add the statements it should run, or delete it."),
        );
        return;
    }
    let total = count(stmts);
    if total > MAX_STATEMENTS {
        // the top-level statement that contains the 25th statement, up to the end of the block
        let mut seen = 0;
        let mut start = stmts[0].span.line;
        for s in stmts {
            let n = 1 + children(s).iter().map(|c| count(c)).sum::<usize>();
            if seen + n >= MAX_STATEMENTS {
                start = s.span.line;
                break;
            }
            seen += n;
        }
        let end = stmts.iter().map(last_line).max().unwrap_or(start);
        out.push(
            Diagnostic::error(
                E_LIMIT,
                path,
                heading,
                format!("{what} has {total} statements; the limit is {MAX_STATEMENTS}."),
            )
            .with_help(fix.text(start, end)),
        );
    }
    nesting(path, stmts, 0, fix, out);
}

/// `level` = number of compound statements around `stmts`.
fn nesting(path: &str, stmts: &[Stmt], level: usize, fix: Fix, out: &mut Vec<Diagnostic>) {
    for s in stmts {
        let compound = is_compound(s);
        if compound && level >= MAX_NESTING {
            out.push(
                Diagnostic::error(
                    E_LIMIT,
                    path,
                    s.span,
                    format!(
                        "Statements are nested {} levels deep; the limit is {MAX_NESTING}.",
                        level + 1
                    ),
                )
                .with_help(fix.text(s.span.line, last_line(s))),
            );
            continue; // everything below is part of the same problem
        }
        let inner = if compound { level + 1 } else { level };
        for c in children(s) {
            nesting(path, c, inner, fix, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_text_does_not_count_but_code_does() {
        let line = format!("- log: \"{}\"", "x".repeat(200));
        assert_eq!(counted_len(&line, false), "- log: \"\"".len());
        assert_eq!(counted_len(&line, true), line.len());
        let line = "- say \"a \\\" b\" more";
        assert_eq!(counted_len(line, false), "- say \"\" more".len());
    }
}
