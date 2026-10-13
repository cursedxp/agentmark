//! Lint (SPEC 18.4): the soft rules that keep a file readable.
//!
//! Every rule works on the AST alone. Findings are warnings unless the project promotes the rule
//! with `[lint] deny = ["rule-id"]` in `agentmark.toml` ([`LintConfig`]); then they are errors.
//! A finding follows the SPEC 10 shape (file and line, the source line, a caret, plain wording) and
//! ends with the rule id in brackets so it can be copied into `deny`.

use std::collections::{BTreeSet, HashMap};

use crate::diag::{did_you_mean, Diagnostic, Span};
use crate::fmt::expr_text;
use crate::syntax::ast::*;
use crate::syntax::parser::parse_file;

/// Every rule id, in the order SPEC 18.4 lists them.
pub const RULES: [&str; 7] = [
    "long-file",
    "magic-value",
    "unnamed-guard",
    "duplicate-rule",
    "prompt-in-loop",
    "missing-doc",
    "vague-name",
];

/// Files longer than this many lines trigger `long-file`.
const MAX_FILE_LINES: usize = 150;
/// Guard conditions longer than this many characters need a comment above them.
const MAX_GUARD_CHARS: usize = 60;
/// Identifiers that say nothing about what they hold.
const VAGUE_NAMES: [&str; 5] = ["data", "tmp", "x", "result", "thing"];

/// The `[lint]` table of `agentmark.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LintConfig {
    /// Rules whose findings are errors instead of warnings.
    pub deny: BTreeSet<String>,
}

impl LintConfig {
    pub fn new<I, S>(deny: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        LintConfig {
            deny: deny.into_iter().map(Into::into).collect(),
        }
    }

    pub fn denies(&self, rule: &str) -> bool {
        self.deny.contains(rule)
    }

    /// Read the `[lint] deny = [...]` table out of the text of an `agentmark.toml`. Everything
    /// else in the file is ignored (the project loader reads it). `path` names the file in the
    /// diagnostic when the table is wrong.
    pub fn from_toml_str(src: &str, path: &str) -> Result<LintConfig, Box<Diagnostic>> {
        let at = |offset: usize, len: usize| {
            let before = &src[..offset.min(src.len())];
            let line = before.matches('\n').count() as u32 + 1;
            let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) as u32 + 1;
            Span::new(line, col, len.max(1) as u32)
        };
        let err = |e: toml::de::Error, message: Option<&str>| {
            let span = e.span().map_or(Span::new(1, 1, 1), |r| at(r.start, r.len()));
            let message = message.map_or_else(|| e.message().to_string(), str::to_string);
            Box::new(Diagnostic::error("lint-config", path, span, message))
        };
        // Syntax first, so a broken file gets TOML's own message.
        src.parse::<toml::Table>().map_err(|e| err(e, None))?;
        let shape: RawShape = toml::from_str(src).map_err(|e| err(e, None))?;
        if let Some(l) = shape.lint.filter(|l| !l.get_ref().is_table()) {
            let r = l.span();
            return Err(Box::new(Diagnostic::error(
                "lint-config",
                path,
                at(r.start, r.len()),
                "[lint] must be a table, like [lint] with deny = [\"missing-doc\"] under it.",
            )));
        }
        let doc: RawDoc = toml::from_str(src).map_err(|e| {
            err(e, Some("[lint] deny must be a list of rule names, like [\"missing-doc\"]."))
        })?;
        let mut cfg = LintConfig::default();
        for item in doc.lint.and_then(|l| l.deny).unwrap_or_default() {
            let name = item.get_ref().as_str();
            if !RULES.contains(&name) {
                let r = item.span();
                // the span covers the quotes
                let (off, len) = (r.start + 1, r.len().saturating_sub(2));
                return Err(Box::new(
                    Diagnostic::error(
                        "lint-config",
                        path,
                        at(off, len),
                        format!("Unknown lint rule \"{name}\" in [lint] deny."),
                    )
                    .with_did_you_mean(did_you_mean(name, RULES))
                    .with_help(format!("The rules are: {}.", RULES.join(", "))),
                ));
            }
            cfg.deny.insert(name.to_string());
        }
        Ok(cfg)
    }
}

#[derive(serde::Deserialize)]
struct RawShape {
    lint: Option<toml::Spanned<toml::Value>>,
}

#[derive(serde::Deserialize)]
struct RawDoc {
    lint: Option<RawLint>,
}

#[derive(serde::Deserialize)]
struct RawLint {
    deny: Option<Vec<toml::Spanned<String>>>,
}

/// Lint several parsed files together (`duplicate-rule` looks across all of them). Findings come
/// back in file order, then line order. The length of a file is taken from the last line its AST
/// points at; use [`lint_source`] to count the real lines.
pub fn lint(files: &[File], cfg: &LintConfig) -> Vec<Diagnostic> {
    let units: Vec<(&File, Option<usize>)> = files.iter().map(|f| (f, None)).collect();
    run(&units, cfg)
}

/// Lint several parsed files together, each with its source text, so `long-file` counts the real
/// lines. This is the entry point for project linting.
pub fn lint_with_sources(files: &[(&File, &str)], cfg: &LintConfig) -> Vec<Diagnostic> {
    let units: Vec<(&File, Option<usize>)> =
        files.iter().map(|(f, src)| (*f, Some(src.lines().count()))).collect();
    run(&units, cfg)
}

/// Lint one parsed file.
pub fn lint_file(file: &File, cfg: &LintConfig) -> Vec<Diagnostic> {
    run(&[(file, None)], cfg)
}

/// Parse and lint one file's text. Parse errors come back as `Err` unchanged.
pub fn lint_source(
    path: &str,
    src: &str,
    cfg: &LintConfig,
) -> Result<Vec<Diagnostic>, Vec<Diagnostic>> {
    let file = parse_file(path, src)?;
    Ok(run(&[(&file, Some(src.lines().count()))], cfg))
}

fn run(units: &[(&File, Option<usize>)], cfg: &LintConfig) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut dups = duplicate_rule(units, cfg).into_iter();
    for (file, line_count) in units {
        let mut ctx = Ctx {
            cfg,
            file: &file.path,
            out: dups.next().unwrap_or_default(),
        };
        long_file(&mut ctx, file, *line_count);
        magic_value(&mut ctx, file);
        unnamed_guard(&mut ctx, file);
        prompt_in_loop(&mut ctx, file);
        missing_doc(&mut ctx, file);
        vague_name(&mut ctx, file);
        let mut found = ctx.out;
        found.sort_by_key(|d| (d.span.line, d.span.col));
        out.extend(found);
    }
    out
}

struct Ctx<'a> {
    cfg: &'a LintConfig,
    file: &'a str,
    out: Vec<Diagnostic>,
}

impl Ctx<'_> {
    fn report(&mut self, rule: &'static str, span: Span, message: String, help: String) {
        self.out.push(finding(self.cfg, rule, self.file, span, message, help));
    }
}

fn finding(
    cfg: &LintConfig,
    rule: &'static str,
    file: &str,
    span: Span,
    message: String,
    help: String,
) -> Diagnostic {
    let message = format!("{message} [{rule}]");
    let d = if cfg.denies(rule) {
        Diagnostic::error(rule, file, span, message)
    } else {
        Diagnostic::warning(rule, file, span, message)
    };
    d.with_help(help)
}

// ---------------------------------------------------------------------------------- walkers

/// The expressions directly inside an expression.
fn expr_children(e: &Expr) -> Vec<&Expr> {
    match &e.kind {
        ExprKind::Interp(parts) => parts
            .iter()
            .filter_map(|p| match p {
                InterpPart::Expr(x) => Some(x),
                InterpPart::Text(_) => None,
            })
            .collect(),
        ExprKind::Binary { lhs, rhs, .. } => vec![lhs, rhs],
        ExprKind::Not(x)
        | ExprKind::IsEmpty(x)
        | ExprKind::IsNotEmpty(x)
        | ExprKind::Length(x)
        | ExprKind::NoAmountsChanged(x) => vec![x],
        ExprKind::Matches { expr, .. } => vec![expr],
        ExprKind::Call { args, .. } => args.iter().collect(),
        ExprKind::Record(fields) => fields.iter().map(|f| &f.value).collect(),
        ExprKind::CountOf { source, .. } => vec![source],
        ExprKind::FirstIn { source, cond, .. } => vec![source, cond],
        _ => Vec::new(),
    }
}

/// `e` and every expression inside it, outermost first.
fn each_expr<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a Expr)) {
    f(e);
    for c in expr_children(e) {
        each_expr(c, f);
    }
}

/// The expressions a statement holds itself (not those of the statements nested in it).
fn stmt_exprs(s: &Stmt) -> Vec<&Expr> {
    match &s.kind {
        StmtKind::ForEach { source, .. } => vec![source],
        StmtKind::If { cond, .. } | StmtKind::Guard { cond, .. } => vec![cond],
        StmtKind::IfSome { expr, .. } | StmtKind::Set { expr, .. } => vec![expr],
        StmtKind::Do { args, .. } => args.iter().collect(),
        StmtKind::AskModel { with, prompt, .. } => with.iter().chain([prompt]).collect(),
        StmtKind::ToolCall {
            target, payload, ..
        } => {
            let mut v: Vec<&Expr> = target
                .iter()
                .filter_map(|t| match t {
                    TargetPart::Value(e) => Some(e),
                    TargetPart::Word(_) => None,
                })
                .collect();
            match payload {
                Some(Payload::Assign(a)) => v.extend(a.iter().map(|a| &a.value)),
                Some(Payload::Value(e)) => v.push(e),
                None => {}
            }
            v
        }
        StmtKind::Return(e) | StmtKind::Print(e) => vec![e],
        StmtKind::Respond { body, .. } => body.iter().collect(),
        StmtKind::ReadFile { path, .. } => vec![path],
        StmtKind::Append { value, list } => vec![value, list],
        StmtKind::Stop | StmtKind::ReadBody { .. } | StmtKind::Try { .. } | StmtKind::Parallel(_) => {
            Vec::new()
        }
    }
}

/// The statement lists nested directly in a statement.
fn stmt_children(s: &Stmt) -> Vec<&[Stmt]> {
    match &s.kind {
        StmtKind::ForEach { body, .. } => vec![body],
        StmtKind::If { then, else_, .. } | StmtKind::IfSome { then, else_, .. } => {
            vec![then, else_]
        }
        StmtKind::Parallel(body) => vec![body],
        StmtKind::Try { stmt, otherwise } => {
            let mut v = vec![std::slice::from_ref(&**stmt)];
            if let Some(o) = otherwise {
                v.push(o);
            }
            v
        }
        StmtKind::Guard {
            else_: Some(inner), ..
        } => vec![std::slice::from_ref(&**inner)],
        _ => Vec::new(),
    }
}

/// Every statement in `body`, outer before inner.
fn each_stmt<'a>(body: &'a [Stmt], f: &mut dyn FnMut(&'a Stmt)) {
    for s in body {
        f(s);
        for c in stmt_children(s) {
            each_stmt(c, f);
        }
    }
}

/// The statement lists of a declaration (blocks, routes, function and command bodies).
fn decl_bodies(d: &Decl) -> Vec<&[Stmt]> {
    match d {
        Decl::Agent(a) => a.blocks.iter().map(|b| b.body.as_slice()).collect(),
        Decl::Function(f) => vec![&f.body],
        Decl::Service(s) => s.routes.iter().map(|r| r.body.as_slice()).collect(),
        Decl::Command(c) => vec![&c.body],
        _ => Vec::new(),
    }
}

fn decl_params(d: &Decl) -> Vec<&Param> {
    match d {
        Decl::Function(f) => f.params.iter().collect(),
        Decl::Command(c) => c.args.iter().collect(),
        Decl::RustBlock(r) => r.params.iter().collect(),
        Decl::Binding(b) => b.items.iter().flat_map(|i| i.params.iter()).collect(),
        _ => Vec::new(),
    }
}

fn body_lines(file: &File) -> u32 {
    let mut max = 0;
    let mut see = |l: u32| max = max.max(l);
    for i in &file.imports {
        see(i.span.line);
    }
    for d in &file.decls {
        match d {
            Decl::Record(r) => {
                see(r.span.line);
                r.fields.iter().for_each(|f| see(f.span.line));
            }
            Decl::Rule(r) => see(r.when.span.line.max(r.span.line)),
            Decl::Constant(c) => see(c.value.span.line.max(c.span.line)),
            Decl::Run(r) => {
                see(r.span.line);
                r.items.iter().for_each(|i| see(i.span.line));
            }
            Decl::Agent(a) => {
                see(a.span.line);
                a.blocks.iter().for_each(|b| see(b.span.line));
            }
            Decl::Function(f) => see(f.span.line.max(f.effect.span.line)),
            Decl::Service(s) => {
                see(s.span.line);
                s.routes.iter().for_each(|r| see(r.span.line));
            }
            Decl::Command(c) => see(c.span.line),
            Decl::Binding(b) => {
                see(b.span.line);
                for i in &b.items {
                    see(i.span.line);
                    see(i.rust.line + i.rust.code.lines().count() as u32);
                }
                b.not_bound.iter().for_each(|n| see(n.span.line));
            }
            Decl::RustBlock(r) => see(r.rust.line + r.rust.code.lines().count() as u32),
        }
        for body in decl_bodies(d) {
            each_stmt(body, &mut |s| see(s.span.line));
        }
    }
    max
}

// ---------------------------------------------------------------------------------- long-file

fn long_file(ctx: &mut Ctx, file: &File, line_count: Option<usize>) {
    let n = line_count.unwrap_or_else(|| body_lines(file) as usize);
    if n > MAX_FILE_LINES {
        ctx.report(
            "long-file",
            Span::new(1, 1, 1),
            format!("This file has {n} lines; files longer than {MAX_FILE_LINES} lines are hard to read."),
            "Split it: one record per file in records/, one rule per file in rules/, one agent per file in agents/, then import them.".to_string(),
        );
    }
}

// ---------------------------------------------------------------------------------- magic-value

/// A literal worth naming: what kind it is, and its canonical text (the key).
fn magic_literals(e: &Expr) -> Vec<(&'static str, String)> {
    match &e.kind {
        ExprKind::Money { minor, .. } if *minor != 0 => vec![("amount", expr_text(e))],
        ExprKind::Duration(_) => vec![("duration", expr_text(e))],
        ExprKind::Str(s) => urls_in(s).into_iter().map(|u| ("URL", u)).collect(),
        ExprKind::Interp(parts) => parts
            .iter()
            .filter_map(|p| match p {
                InterpPart::Text(t) => Some(t),
                InterpPart::Expr(_) => None,
            })
            .flat_map(|t| urls_in(t))
            .map(|u| ("URL", u))
            .collect(),
        _ => Vec::new(),
    }
}

fn urls_in(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("http://").into_iter().chain(rest.find("https://")).min() {
        let tail = &rest[at..];
        let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let url = tail[..end].trim_end_matches(['.', ',', ';', ')']);
        urls.push(url.to_string());
        rest = &tail[end..];
    }
    urls
}

fn magic_value(ctx: &mut Ctx, file: &File) {
    // key -> (kind, spans of the places it is written)
    let mut seen: Vec<(String, &'static str, Vec<Span>)> = Vec::new();
    let mut note = |e: &Expr| {
        each_expr(e, &mut |x| {
            for (kind, key) in magic_literals(x) {
                match seen.iter_mut().find(|(k, _, _)| *k == key) {
                    Some((_, _, spans)) => spans.push(x.span),
                    None => seen.push((key, kind, vec![x.span])),
                }
            }
        });
    };
    // Constants are where the literals are supposed to live, so their values do not count.
    for d in &file.decls {
        if let Decl::Rule(r) = d {
            note(&r.when);
        }
        for p in decl_params(d) {
            if let Some(def) = &p.default {
                note(def);
            }
        }
        for body in decl_bodies(d) {
            each_stmt(body, &mut |s| stmt_exprs(s).into_iter().for_each(&mut note));
        }
    }
    for (key, kind, spans) in seen {
        if spans.len() > 1 {
            let lines: Vec<String> = spans.iter().map(|s| s.line.to_string()).collect();
            ctx.report(
                "magic-value",
                spans[0],
                format!(
                    "The {kind} {key} is written {} times in this file (lines {}).",
                    spans.len(),
                    lines.join(", ")
                ),
                "Give it a name with \"# Constant: name\" and \"- value: ...\", then use the name.".to_string(),
            );
        }
    }
}

// ---------------------------------------------------------------------------------- unnamed-guard

fn unnamed_guard(ctx: &mut Ctx, file: &File) {
    // Guards written as `try guard: ...`, already looked at through their `try` line.
    let mut inner_of_try: Vec<*const Stmt> = Vec::new();
    for d in &file.decls {
        for body in decl_bodies(d) {
            each_stmt(body, &mut |s| {
                // `try guard: ...` keeps its comments on the `try` line.
                let (guard, comments) = match &s.kind {
                    StmtKind::Guard { .. } if inner_of_try.contains(&(s as *const Stmt)) => return,
                    StmtKind::Guard { .. } => (s, &s.comments),
                    StmtKind::Try { stmt, .. } if matches!(stmt.kind, StmtKind::Guard { .. }) => {
                        inner_of_try.push(&**stmt as *const Stmt);
                        (&**stmt, &s.comments)
                    }
                    _ => return,
                };
                let StmtKind::Guard { cond, .. } = &guard.kind else {
                    return;
                };
                let n = expr_text(cond).chars().count();
                if n > MAX_GUARD_CHARS && comments.is_empty() {
                    ctx.report(
                        "unnamed-guard",
                        cond.span,
                        format!("This guard's condition is {n} characters long and has no comment above it."),
                        "Add a \"> comment\" line above the guard that says what it protects against.".to_string(),
                    );
                }
            });
        }
    }
}

// ---------------------------------------------------------------------------------- duplicate-rule

/// Findings per unit (same order as `units`).
fn duplicate_rule(units: &[(&File, Option<usize>)], cfg: &LintConfig) -> Vec<Vec<Diagnostic>> {
    // when-text -> (rule name, file, line) of the first rule that has it
    let mut first: HashMap<String, (String, String, u32)> = HashMap::new();
    let mut all = Vec::new();
    for (file, _) in units {
        let mut found = Vec::new();
        for d in &file.decls {
            let Decl::Rule(r) = d else { continue };
            let key = expr_text(&r.when);
            match first.get(&key) {
                Some((name, path, line)) => found.push(finding(
                    cfg,
                    "duplicate-rule",
                    &file.path,
                    r.when.span,
                    format!(
                        "Rule \"{}\" has the same \"when:\" as rule \"{name}\" ({path}:{line}).",
                        r.name.name
                    ),
                    "Keep one of the two and use it wherever the other was used.".to_string(),
                )),
                None => {
                    first.insert(key, (r.name.name.clone(), file.path.clone(), r.span.line));
                }
            }
        }
        all.push(found);
    }
    all
}

// ---------------------------------------------------------------------------------- prompt-in-loop

/// Statements of a loop body, descending into branches but not into nested loops (those are
/// checked as loops of their own).
fn flat<'a>(body: &'a [Stmt], out: &mut Vec<&'a Stmt>) {
    for s in body {
        out.push(s);
        if matches!(s.kind, StmtKind::ForEach { .. }) {
            continue;
        }
        for c in stmt_children(s) {
            flat(c, out);
        }
    }
}

fn mentions(e: &Expr, var: &str) -> bool {
    let mut hit = false;
    each_expr(e, &mut |x| {
        if let ExprKind::Path(p) = &x.kind {
            hit |= p.first().is_some_and(|i| i.name == var);
        }
    });
    hit
}

fn prompt_in_loop(ctx: &mut Ctx, file: &File) {
    for d in &file.decls {
        for body in decl_bodies(d) {
            each_stmt(body, &mut |s| {
                let StmtKind::ForEach { body, .. } = &s.kind else {
                    return;
                };
                let mut stmts = Vec::new();
                flat(body, &mut stmts);
                for (i, st) in stmts.iter().enumerate() {
                    let StmtKind::AskModel { into, .. } = &st.kind else {
                        continue;
                    };
                    let guarded = stmts[i + 1..].iter().any(|g| {
                        matches!(&g.kind, StmtKind::Guard { cond, .. } if mentions(cond, &into.name))
                    });
                    if !guarded {
                        ctx.report(
                            "prompt-in-loop",
                            st.span,
                            format!(
                                "\"ask model\" runs for every item of a \"for each\" loop and nothing checks its result \"{}\".",
                                into.name
                            ),
                            format!(
                                "Add \"- guard: ...\" on \"{}\" after it: the model can answer differently for every item (SPEC 4.4).",
                                into.name
                            ),
                        );
                    }
                }
            });
        }
    }
}

// ---------------------------------------------------------------------------------- missing-doc

fn missing_doc(ctx: &mut Ctx, file: &File) {
    for d in &file.decls {
        let (kind, name, span, doc) = match d {
            Decl::Agent(a) => ("agent", &a.name, a.span, &a.doc),
            Decl::Service(s) => ("service", &s.name, s.span, &s.doc),
            Decl::Command(c) => ("command", &c.name, c.span, &c.doc),
            _ => continue,
        };
        if doc.is_none() {
            ctx.report(
                "missing-doc",
                span,
                format!("The {kind} \"{}\" has no description.", name.name),
                "Put a \"> description\" comment on the line directly above its heading.".to_string(),
            );
        }
    }
}

// ---------------------------------------------------------------------------------- vague-name

fn vague_name(ctx: &mut Ctx, file: &File) {
    for d in &file.decls {
        let mut names: Vec<(String, Span)> = Vec::new();
        match d {
            Decl::Record(r) => {
                add(&mut names, &r.name);
                for f in &r.fields {
                    add(&mut names, &f.name);
                }
            }
            Decl::Rule(r) => add(&mut names, &r.name),
            Decl::Agent(a) => add(&mut names, &a.name),
            Decl::Function(f) => add(&mut names, &f.name),
            Decl::Service(s) => add(&mut names, &s.name),
            Decl::Command(c) => add(&mut names, &c.name),
            Decl::Binding(b) => {
                add(&mut names, &b.name);
                for i in &b.items {
                    add(&mut names, &i.name);
                }
            }
            Decl::Constant(c) => add(&mut names, &c.name),
            Decl::RustBlock(r) => add(&mut names, &r.name),
            Decl::Run(_) => {}
        }
        for p in decl_params(d) {
            add(&mut names, &p.name);
        }
        if let Decl::Rule(r) = d {
            each_expr(&r.when, &mut |e| first_var(e, &mut names));
        }
        if let Decl::Service(s) = d {
            // `{id}` in `GET /invoices/{id}`: the heading is "METHOD path" starting at span.col.
            for r in &s.routes {
                for p in &r.path_params {
                    let pat = format!("{{{p}}}");
                    let off = r.path.find(&pat).map_or(0, |o| o + 1);
                    let col = r.span.col + r.method.chars().count() as u32 + 1 + off as u32;
                    names.push((p.clone(), Span::new(r.span.line, col, p.chars().count() as u32)));
                }
            }
        }
        for body in decl_bodies(d) {
            each_stmt(body, &mut |s| {
                match &s.kind {
                    StmtKind::Set { var, .. } | StmtKind::ForEach { var, .. } => add(&mut names, var),
                    StmtKind::IfSome { bind, .. } => add(&mut names, bind),
                    StmtKind::AskModel { into, .. }
                    | StmtKind::ReadBody { into, .. }
                    | StmtKind::ReadFile { into, .. } => add(&mut names, into),
                    StmtKind::Do { into: Some(i), .. } | StmtKind::ToolCall { into: Some(i), .. } => {
                        add(&mut names, i)
                    }
                    _ => {}
                }
                for e in stmt_exprs(s) {
                    each_expr(e, &mut |x| first_var(x, &mut names));
                }
            });
        }
        // One finding per name per declaration: the first place it is introduced.
        let mut done: Vec<String> = Vec::new();
        for (name, span) in names {
            if VAGUE_NAMES.contains(&name.as_str()) && !done.contains(&name) {
                done.push(name.clone());
                ctx.report(
                    "vague-name",
                    span,
                    format!("\"{name}\" does not say what it holds."),
                    "Name it after its content, like \"invoice_rows\" or \"reminder_text\".".to_string(),
                );
            }
        }
    }
}

/// `first <var> in ...` introduces `<var>`.
fn first_var(e: &Expr, names: &mut Vec<(String, Span)>) {
    if let ExprKind::FirstIn { var, .. } = &e.kind {
        add(names, var);
    }
}

fn add(names: &mut Vec<(String, Span)>, i: &Ident) {
    names.push((i.name.clone(), i.span));
}
