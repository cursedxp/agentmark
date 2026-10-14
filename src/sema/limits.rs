//! Hard limits (SPEC 18.3). They cannot be switched off, and every message says how to fix it.

use crate::diag::{Diagnostic, Span};
use crate::project::ProjectKind;
use crate::syntax::ast::{Decl, Stmt, StmtKind};

use super::Checker;

pub const E_LIMIT: &str = "E211";
pub const W_SINGLE_FILE: &str = "W006";
pub const W_LONG_LINE: &str = "W007";
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

/// Length of a line for the 120-character limit, and the 1-based column of the first counted
/// character past the limit. The text of a string literal is prose that cannot be broken across
/// lines, so it does not count (OPEN_QUESTIONS Q35); the quotes do. `exempt` is false for
/// comments and fenced Rust, which count in full.
fn counted_len(line: &str, exempt: bool) -> (usize, Option<usize>) {
    let mut n = 0;
    let mut over = None;
    let mut in_str = false;
    let mut escaped = false;
    for (i, c) in line.chars().enumerate() {
        let counts = if !exempt {
            true
        } else if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
                n += 1;
                if n == MAX_LINE + 1 {
                    over = Some(i + 1);
                }
            }
            false
        } else {
            if c == '"' {
                in_str = true;
            }
            true
        };
        if counts {
            n += 1;
            if n == MAX_LINE + 1 && over.is_none() {
                over = Some(i + 1);
            }
        }
    }
    (n, over)
}

fn line_lengths(path: &str, text: &str, out: &mut Vec<Diagnostic>) {
    let mut in_fence = false;
    let mut in_not_bound = false;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let is_fence = trimmed.starts_with("```");
        if is_fence {
            in_fence = !in_fence;
        }
        // The binder's `## Not bound` list (SPEC 15.2) is a generated report: one entry per
        // item with the reason and the library's own path, which cannot be wrapped (Q110).
        if !in_fence && line.starts_with('#') {
            in_not_bound = line.trim_end() == "## Not bound";
        }
        if in_not_bound && !in_fence && line.starts_with("- ") {
            continue;
        }
        let exempt = !(in_fence || is_fence || trimmed.starts_with('>'));
        let (len, over) = counted_len(line, exempt);
        let total = line.chars().count();
        if len > MAX_LINE {
            let col = over.unwrap_or(MAX_LINE + 1);
            let help = if line.contains('"') && total > len + 20 {
                "Shorten the statement around the text, or put the long text into a \"# Constant\" and use its name."
            } else {
                "Split the statement into shorter ones, or use shorter names."
            };
            out.push(
                Diagnostic::error(
                    E_LIMIT,
                    path,
                    Span::new(i as u32 + 1, col as u32, (total + 1 - col).max(1) as u32),
                    format!("This line is {len} characters long; the limit is {MAX_LINE}."),
                )
                .with_help(help),
            );
        } else if total > MAX_LINE {
            out.push(
                Diagnostic::warning(
                    W_LONG_LINE,
                    path,
                    Span::new(i as u32 + 1, MAX_LINE as u32 + 1, (total - MAX_LINE) as u32),
                    format!(
                        "This line is {total} characters long; only the text of its strings makes it longer than {MAX_LINE}."
                    ),
                )
                .with_help("Consider moving the long text into a \"# Constant\" and using its name."),
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
    let mut runnable = Vec::new();
    let mut records = Vec::new();
    let mut rules = Vec::new();
    for d in decls {
        match d {
            Decl::Agent(a) => runnable.push(("agent", &a.name)),
            Decl::Service(s) => runnable.push(("service", &s.name)),
            Decl::Command(c) => runnable.push(("command", &c.name)),
            Decl::Record(r) => records.push(&r.name),
            Decl::Rule(r) => rules.push(&r.name),
            _ => {}
        }
    }
    // OPEN_QUESTIONS Q34: a project folder applies the row literally; a single file is a whole
    // program in one file, exempt from the row but flagged once.
    if let [(first_kind, first), (second_kind, second), ..] = runnable.as_slice() {
        if kind == ProjectKind::Folder {
            out.push(
                Diagnostic::error(
                    E_LIMIT,
                    path,
                    second.span,
                    format!(
                        "A file may hold only one agent, service or command, but the {second_kind} \"{}\" comes after the {first_kind} \"{}\".",
                        second.name, first.name
                    ),
                )
                .with_help(format!(
                    "Move \"{}\" into its own file, for example \"{}s/{}.agent.md\", and import it.",
                    second.name, second_kind, second.name
                )),
            );
        } else {
            out.push(
                Diagnostic::warning(
                    W_SINGLE_FILE,
                    path,
                    second.span,
                    format!(
                        "This file holds {} runnable declarations (agent, service or command): it is a single-file draft.",
                        runnable.len()
                    ),
                )
                .with_help("Split it into a project folder with one declaration per file when it grows."),
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

/// Lines to move out: from the top-level statement holding the 25th statement to `end`. When that
/// statement alone is larger than the limit, move the part inside it instead.
fn overflow_range(stmts: &[Stmt], before: usize, end: u32) -> Option<(u32, u32)> {
    let mut seen = before;
    for s in stmts {
        let n = 1 + children(s).iter().map(|c| count(c)).sum::<usize>();
        if seen + n >= MAX_STATEMENTS {
            if n > MAX_STATEMENTS {
                let mut inner = seen + 1;
                for c in children(s) {
                    if let Some(r) = overflow_range(c, inner, last_line(s)) {
                        return Some(r);
                    }
                    inner += count(c);
                }
            }
            return Some((s.span.line, end));
        }
        seen += n;
    }
    None
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
        let end = stmts.iter().map(last_line).max().unwrap_or(heading.line);
        let (start, end) = overflow_range(stmts, 0, end)
            .unwrap_or((stmts[0].span.line, end));
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
        assert_eq!(counted_len(&line, true).0, "- log: \"\"".len());
        assert_eq!(counted_len(&line, false).0, line.len());
        let line = "- say \"a \\\" b\" more";
        assert_eq!(counted_len(line, true).0, "- say \"\" more".len());
    }

    #[test]
    fn the_overflow_column_is_the_first_counted_character_past_the_limit() {
        let line = format!("- log: \"{}\" {}", "x".repeat(300), "y".repeat(130));
        let (n, over) = counted_len(&line, true);
        assert!(n > MAX_LINE);
        // the 121st counted character is a `y`, far to the right of column 121
        let col = over.unwrap();
        assert!(col > 300);
        assert_eq!(line.chars().nth(col - 1), Some('y'));
    }
}
