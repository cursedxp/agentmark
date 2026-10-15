//! Formatter (SPEC 18.6, 23): one canonical style, no options.
//!
//! The file is parsed, then printed again from the AST, so the style is the printer's and not a
//! patched-up version of what the author typed:
//!
//! - no trailing whitespace, one trailing newline;
//! - `# Program` first, then `# Uses` (sorted: files and globs, then folders), then
//!   `# Import: crate …`, then the other parts; one blank line between parts (and before every
//!   `##` block);
//! - a description is the paragraph right under its heading, followed by a blank line;
//! - properties are `-` bullets, in the order the language documents them;
//! - steps are numbered 1, 2, 3, … at every level; the steps under a step start where its text
//!   starts (3 spaces under `1.`, 4 under `10.`), bullets under a bullet 2 spaces deeper;
//!   `else:` and `otherwise:` are plain lines at the indentation of those steps (SPEC 23.3);
//! - `add X { … } to list` is written with one bullet per field (SPEC 23.11);
//! - `- start:` is written as a link (SPEC 23.4);
//! - expressions are re-printed with single spaces and only the parentheses precedence needs.
//!
//! Comments and fenced Rust code are preserved. The AST keeps documentation comments and the
//! comments directly above a statement, field or property. A comment that sits where the parser
//! drops it (after the last line of a block, above an `else:`, above an item of `## Not bound`)
//! is found again through the lexer and written back right after the closest line that precedes it
//! (OPEN_QUESTIONS Q52), so `fmt` never deletes a comment.
//!
//! `fmt(fmt(x)) == fmt(x)` holds, and the output parses to the same AST as the input apart from
//! spans and the heading text of a block ([`Block::label`], OPEN_QUESTIONS Q50).

use std::collections::{HashMap, HashSet};

use crate::diag::Diagnostic;
use crate::syntax::ast::*;
use crate::syntax::lexer::{lex, Line, LineKind};
use crate::syntax::parser::parse_file;

/// Format `src`. `path` is the name shown in diagnostics when the file does not parse.
pub fn format_source(src: &str, path: &str) -> Result<String, Vec<Diagnostic>> {
    let file = parse_file(path, src)?;
    let (lines, _) = lex(path, src);
    let links = start_links(&file, path);
    Ok(format_file(&file, &lines, links))
}

/// Where a `- start:` written as a plain name links to (SPEC 23.4): the command's heading when
/// it is in the same file, else the used file that declares it (read from disk next to `path`).
fn start_links(file: &File, path: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(start) = file.decls.iter().find_map(|d| match d {
        Decl::Program(p) => p.start.as_ref().filter(|s| s.label.is_none()),
        _ => None,
    }) else {
        return out;
    };
    let declares = |f: &File| f.decls.iter().any(|d| matches!(d, Decl::Command(c) if c.name.name == start.target));
    if declares(file) {
        let anchor = crate::sema::github_anchor(&format!("Command: {}", start.target));
        out.insert(start.target.clone(), format!("#{anchor}"));
        return out;
    }
    let dir = std::path::Path::new(path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    for imp in &file.imports {
        let ImportKind::File { path: rel, .. } = &imp.kind else { continue };
        let Ok(text) = std::fs::read_to_string(dir.join(rel)) else { continue };
        if parse_file(rel, &text).is_ok_and(|f| declares(&f)) {
            out.insert(start.target.clone(), rel.clone());
            return out;
        }
    }
    out
}

/// `fmt --check`: is `src` already in canonical form?
pub fn is_canonical(src: &str, path: &str) -> Result<bool, Vec<Diagnostic>> {
    Ok(format_source(src, path)? == src)
}

/// Print a parsed file. `lines` is the lexed source of the same file (used to keep comments the
/// AST does not hold).
mod collections;

fn format_file(file: &File, lines: &[Line], start_links: HashMap<String, String>) -> String {
    let mut p = Printer {
        start_links,
        src_lines: lines
            .iter()
            .filter(|l| l.kind != LineKind::Blank)
            .map(|l| (l.line_no, l.kind.clone(), l.text.clone()))
            .collect(),
        ..Printer::default()
    };
    p.file(file);
    let orphans = find_orphans(lines, &p);
    p.render(orphans)
}

// ---------------------------------------------------------------------------------- printer

/// One output line. `src` is the line of the source this one came from (0 = none), so a comment
/// the AST dropped can be put back next to its neighbour.
struct Out {
    /// Verbatim (a line of fenced code): never collapsed or re-indented when empty.
    raw: bool,
    src: u32,
    indent: usize,
    text: String,
}

#[derive(Default)]
struct Printer {
    out: Vec<Out>,
    /// Source lines whose attached comments the AST holds (statements, fields, properties).
    attached: HashSet<u32>,
    /// `## Not bound` entries (source line): their `  > ` continuation lines are part of the reason.
    nb_items: HashSet<u32>,
    /// Heading lines whose documentation was printed.
    docs: HashSet<u32>,
    file_doc: bool,
    /// Index in `out` right after the file documentation.
    after_file_doc: usize,
    /// Every non-blank source line: (line number, kind, text). Used to find the real line of
    /// headings and control lines the AST does not record, and the inline comment of an import.
    src_lines: Vec<(u32, LineKind, String)>,
    /// `- start:` names written plainly, and the link `fmt` writes for them.
    start_links: HashMap<String, String>,
}

fn comment_text(t: &str) -> String {
    if t.is_empty() {
        ">".to_string()
    } else {
        format!("> {t}")
    }
}

impl Printer {
    fn push(&mut self, src: u32, indent: usize, text: impl Into<String>) {
        self.out.push(Out {
            raw: false,
            src,
            indent,
            text: text.into(),
        });
    }

    /// One blank line, unless the output is empty or already ends in one.
    fn blank(&mut self) {
        if self.out.last().is_some_and(|l| !l.text.is_empty()) {
            self.push(0, 0, "");
        }
    }

    fn comments(&mut self, indent: usize, texts: &[String]) {
        for t in texts {
            self.push(0, indent, comment_text(t));
        }
    }

    /// The description: the paragraph under a heading (SPEC 23.2), then a blank line.
    fn description(&mut self, doc: &Option<String>, heading_line: u32) {
        if let Some(d) = doc {
            for l in d.split('\n') {
                if l.trim().is_empty() {
                    self.blank();
                } else {
                    self.push(0, 0, l.trim_end());
                }
            }
            self.blank();
            self.docs.insert(heading_line);
        }
    }

    // ------------------------------------------------------------------ file level

    fn file(&mut self, f: &File) {
        if let Some(d) = &f.doc {
            for l in d.split('\n') {
                self.push(0, 0, comment_text(l));
            }
            self.file_doc = true;
        }
        self.after_file_doc = self.out.len();

        // `# Program` first, then `# Uses`, then the libraries, then everything else (SPEC 23.4)
        for d in &f.decls {
            if matches!(d, Decl::Program(_)) {
                self.blank();
                self.decl(d);
            }
        }
        let mut uses: Vec<&Import> = f.imports.iter().filter(|i| !matches!(i.kind, ImportKind::Crate { .. })).collect();
        uses.sort_by_key(|i| import_key(i));
        if !uses.is_empty() {
            self.blank();
            let first = uses.iter().map(|i| i.span.line).min().unwrap_or(0);
            let at = self.find_line(first, |k, t| *k == LineKind::Heading1 && t.trim().eq_ignore_ascii_case("uses"));
            self.push(at, 0, "# Uses");
            for imp in uses {
                self.uses_item(imp);
            }
        }
        let mut crates: Vec<&Import> = f.imports.iter().filter(|i| matches!(i.kind, ImportKind::Crate { .. })).collect();
        crates.sort_by_key(|i| import_key(i));
        if !crates.is_empty() {
            self.blank();
            for imp in crates {
                self.import(imp);
            }
        }
        for d in &f.decls {
            if matches!(d, Decl::Program(_)) {
                continue;
            }
            self.blank();
            self.decl(d);
        }
    }

    /// `- [label](path)` under `# Uses`.
    fn uses_item(&mut self, imp: &Import) {
        let (path, alias) = match &imp.kind {
            ImportKind::File { path, alias } => (path.clone(), alias.as_ref()),
            ImportKind::Glob { pattern } => (pattern.clone(), None),
            ImportKind::Folder { path, alias } => (format!("{path}/"), Some(alias)),
            ImportKind::Crate { .. } => return,
        };
        let label = imp.label.clone().filter(|l| !l.is_empty()).unwrap_or_else(|| default_label(&path));
        let mut text = format!("- [{label}]({path})");
        if let Some(a) = alias {
            text.push_str(&format!(" as {}", a.name));
        }
        self.push(imp.span.line, 0, text);
        self.attached.insert(imp.span.line);
    }

    fn import(&mut self, imp: &Import) {
        let mut text = match &imp.kind {
            ImportKind::Crate { name, alias } => match alias {
                Some(a) => format!("# Import: crate {} as {}", name.name, a.name),
                None => format!("# Import: crate {}", name.name),
            },
            _ => return,
        };
        if let Some(c) = self
            .src_lines
            .iter()
            .find(|(n, k, _)| *n == imp.span.line && *k == LineKind::Heading1)
            .and_then(|(_, _, t)| inline_comment(t))
        {
            text.push_str("  ");
            text.push_str(&comment_text(&c));
        }
        self.push(imp.span.line, 0, text);
    }

    fn decl(&mut self, d: &Decl) {
        match d {
            Decl::Record(r) => self.record(r),
            Decl::Enum(e) => self.enum_decl(e),
            Decl::TypeAlias(t) => self.type_alias(t),
            Decl::Rule(r) => self.rule(r),
            Decl::Agent(a) => self.agent(a),
            Decl::Function(f) => self.function(f),
            Decl::Service(s) => self.service(s),
            Decl::Command(c) => self.command(c),
            Decl::Binding(b) => self.binding(b),
            Decl::Tool(t) => self.tool(t),
            Decl::Constant(c) => self.constant(c),
            Decl::Program(p) => self.program(p),
            Decl::RustBlock(r) => self.rust_block(r),
            Decl::Entry(_) => unreachable!("L2-only node in fmt"),
        }
    }

    /// A property line with the plain comments written above it.
    fn prop(&mut self, pcs: &[PropComment], key: &str, value: &str, src: u32) {
        if let Some(pc) = pcs.iter().find(|p| p.key == key) {
            self.comments(0, &pc.comments);
        }
        let text = if value.is_empty() {
            format!("- {key}:")
        } else {
            format!("- {key}: {value}")
        };
        self.push(src, 0, text);
        self.attached.insert(src);
    }

    /// Properties are printed in the language's order, not the author's. Give the lines printed
    /// since `from` the largest source line among them, so a comment that followed the author's
    /// last property is put after the last property printed.
    fn group(&mut self, from: usize) {
        let max = self.out[from..].iter().map(|l| l.src).max().unwrap_or(0);
        for l in &mut self.out[from..] {
            if l.src > 0 {
                l.src = max;
            }
        }
    }

    /// The source line of the last line before `before` that satisfies `pred`, or 0.
    fn find_line(&self, before: u32, pred: impl Fn(&LineKind, &str) -> bool) -> u32 {
        self.src_lines
            .iter()
            .filter(|(n, k, t)| *n < before && pred(k, t))
            .map(|(n, _, _)| *n)
            .next_back()
            .unwrap_or(0)
    }

    fn heading(&mut self, doc: &Option<String>, line: u32, text: String) {
        self.push(line, 0, text);
        self.description(doc, line);
    }

    fn record(&mut self, r: &Record) {
        self.heading(&r.doc, r.span.line, format!("# Record: {}", r.name.name));
        for f in &r.fields {
            self.comments(0, &f.comments);
            let text = match &f.ty {
                Some(t) => format!("- {}: {}", f.name.name, type_text(t)),
                None => format!("- {}", f.name.name),
            };
            self.push(f.span.line, 0, text);
            self.attached.insert(f.span.line);
        }
    }

    fn enum_decl(&mut self, e: &Enum) {
        self.heading(&e.doc, e.span.line, format!("# Enum: {}", e.name.name));
        for v in &e.variants {
            self.comments(0, &v.comments);
            let text = if v.fields.is_empty() {
                format!("- {}", v.name.name)
            } else {
                let fields: Vec<String> = v
                    .fields
                    .iter()
                    .map(|f| match &f.ty {
                        Some(t) => format!("{}: {}", f.name.name, type_text(t)),
                        None => f.name.name.clone(),
                    })
                    .collect();
                format!("- {}: {}", v.name.name, fields.join(", "))
            };
            self.push(v.span.line, 0, text);
            self.attached.insert(v.span.line);
        }
    }

    fn type_alias(&mut self, t: &TypeAlias) {
        self.heading(&t.doc, t.span.line, format!("# Type: {}", t.name.name));
        self.prop(&t.prop_comments, "is", &type_text(&t.target), t.target.span.line);
    }

    fn rule(&mut self, r: &Rule) {
        self.heading(&r.doc, r.span.line, format!("# Rule: {} {}", r.name.name, r.for_record.name));
        self.prop(&r.prop_comments, "when", &expr_text(&r.when), r.when.span.line);
    }

    fn constant(&mut self, c: &Constant) {
        self.heading(&c.doc, c.span.line, format!("# Constant: {}", c.name.name));
        self.prop(&c.prop_comments, "value", &expr_text(&c.value), c.value.span.line);
    }

    fn program(&mut self, p: &Program) {
        self.heading(&p.doc, p.span.line, format!("# Program: {}", p.name.name));
        if let Some(s) = &p.start {
            let value = match &s.label {
                Some(label) => format!("[{label}]({})", s.target),
                None => match self.start_links.get(&s.target) {
                    Some(target) => format!("[{}]({target})", s.target),
                    None => s.target.clone(),
                },
            };
            self.prop(&[], "start", &value, s.span.line);
        }
    }

    fn agent(&mut self, a: &Agent) {
        self.heading(&a.doc, a.span.line, format!("# Agent: {}", a.name.name));
        let pcs = &a.prop_comments;
        let from = self.out.len();
        self.prop(pcs, "model", &a.model.value, a.model.span.line);
        if let Some(first) = a.tools.first() {
            let tools: Vec<&str> = a.tools.iter().map(|t| t.name.as_str()).collect();
            self.prop(pcs, "tools", &tools.join(", "), first.span.line);
        }
        if let Some(m) = &a.memory {
            let v = match m.value {
                Memory::None => "none",
                Memory::File => "file",
            };
            self.prop(pcs, "memory", v, m.span.line);
        }
        if let Some(t) = &a.timeout {
            self.prop(pcs, "timeout", &duration_text(&t.value), t.span.line);
        }
        self.group(from);
        for b in &a.blocks {
            self.blank();
            self.heading(&b.doc, b.span.line, format!("## {}", trigger_text(&b.trigger)));
            self.stmts(&b.body, 0);
        }
    }

    /// `- steps:` with the numbered steps under it.
    fn steps_prop(&mut self, body: &[Stmt]) {
        let before = body.first().map_or(u32::MAX, |s| s.span.line);
        let at = self.find_line(before, |k, t| *k == LineKind::Item && t.trim_end() == "steps:");
        self.push(at, 0, "- steps:");
        self.attached.insert(at);
        self.stmts(body, 2);
    }

    fn function(&mut self, f: &Function) {
        self.heading(&f.doc, f.span.line, format!("# Function: {}", f.name.name));
        let pcs = &f.prop_comments;
        let from = self.out.len();
        if let Some(first) = f.params.first() {
            self.prop(pcs, "in", &params_text(&f.params), first.name.span.line);
        }
        if let Some(o) = &f.out {
            self.prop(pcs, "out", &type_text(o), o.span.line);
        }
        if let Some(t) = &f.touches {
            self.prop(pcs, "touches", &touches_text(t), t.span.line);
        }
        self.group(from);
        if let Some(pc) = pcs.iter().find(|p| p.key == "steps") {
            self.comments(0, &pc.comments);
        }
        self.steps_prop(&f.body);
    }

    fn service(&mut self, s: &Service) {
        self.heading(&s.doc, s.span.line, format!("# Service: {}", s.name.name));
        let pcs = &s.prop_comments;
        let from = self.out.len();
        if let Some(l) = &s.listen {
            self.prop(pcs, "listen", &l.value, l.span.line);
        }
        if let Some(st) = &s.state {
            self.prop(pcs, "state", &st.name, st.span.line);
        }
        self.group(from);
        for r in &s.routes {
            self.blank();
            self.heading(&r.doc, r.span.line, format!("## {} {}", r.method, r.path));
            self.stmts(&r.body, 0);
        }
    }

    fn command(&mut self, c: &Command) {
        self.heading(&c.doc, c.span.line, format!("# Command: {}", c.name.name));
        if let Some(first) = c.args.first() {
            self.prop(&c.prop_comments, "in", &params_text(&c.args), first.name.span.line);
        }
        if let Some(pc) = c.prop_comments.iter().find(|p| p.key == "steps") {
            self.comments(0, &pc.comments);
        }
        self.steps_prop(&c.body);
    }

    fn binding(&mut self, b: &Binding) {
        self.heading(&b.doc, b.span.line, format!("# Binding: {}", b.name.name));
        let pcs = &b.prop_comments;
        let from = self.out.len();
        self.prop(pcs, "crate", &b.crate_name.name, b.crate_name.span.line);
        self.prop(pcs, "effect", effect_word(b.effect.value), b.effect.span.line);
        if let Some(p) = &b.proposed_effect {
            self.prop(pcs, "proposed-effect", effect_word(p.value), p.span.line);
        }
        self.group(from);
        for item in &b.items {
            self.blank();
            // the one-line heading, or `## name` with `- in:` / `- out:` lines when it would pass 120 (Q416)
            let params: Vec<String> = item.params.iter().map(|p| params_text(std::slice::from_ref(p))).collect();
            let out = item.out.as_ref().map(type_text);
            let lines = crate::binder::core::model::item_lines(&item.name.name, &params, out.as_deref()).unwrap_or_else(|| {
                let mut h = format!("## {}({})", item.name.name, params.join(", "));
                if let Some(o) = &out {
                    h.push_str(&format!(" -> {o}"));
                }
                vec![h]
            });
            let mut lines = lines.into_iter();
            self.heading(&item.doc, item.span.line, lines.next().unwrap());
            for l in lines {
                self.push(0, 0, l);
            }
            self.fence(2, &item.rust);
        }
        for h in &b.handles {
            self.blank();
            self.heading(&None, h.span.line, format!("## Handle: {}", h.name));
            self.fence(2, &h.rust);
        }
        if !b.not_bound.is_empty() {
            self.blank();
            let before = b.not_bound.first().map_or(u32::MAX, |n| n.span.line);
            let nb = self.find_line(before, |k, t| {
                *k == LineKind::Heading2
                    && t.split_whitespace().map(str::to_lowercase).eq(["not", "bound"])
            });
            self.push(nb, 0, "## Not bound");
            for n in &b.not_bound {
                self.nb_items.insert(n.span.line);
                let Some(r) = &n.reason else {
                    self.push(n.span.line, 0, format!("- {}", n.item));
                    continue;
                };
                // the same wrapping `agentmark bind` writes: long reasons continue on `  > ` lines
                let entry = crate::binder::core::writer::not_bound_entry(&n.item, r);
                for (i, l) in entry.lines().enumerate() {
                    match l.strip_prefix("  > ").or_else(|| l.strip_prefix("  >")) {
                        Some(c) if i > 0 => self.push(0, 2, comment_text(c.trim_start())),
                        _ => self.push(if i == 0 { n.span.line } else { 0 }, 0, l.to_string()),
                    }
                }
            }
        }
    }

    fn tool(&mut self, t: &Tool) {
        self.heading(&t.doc, t.span.line, format!("# Tool: {}", t.name.name));
        if let Some(x) = &t.touches {
            self.prop(&t.prop_comments, "touches", &touches_text(x), x.span.line);
        }
        for v in &t.verbs {
            self.blank();
            let mut h = format!("## {}({})", v.name.name, params_text(&v.params));
            if let Some(o) = &v.out {
                h.push_str(" -> ");
                h.push_str(&type_text(o));
            }
            self.heading(&v.doc, v.span.line, h);
            match &v.body {
                ToolBody::Stmts(body) => self.stmts(body, 0),
                ToolBody::Rust(rust) => self.fence(0, rust),
            }
        }
    }

    /// A `## Rust: name` block. It is hoisted by the parser, so it is printed as a block of the
    /// declaration before it (no `#` heading of its own).
    fn rust_block(&mut self, r: &RustBlock) {
        let mut h = format!("## Rust: {}", r.name.name);
        if let Some(o) = &r.out {
            h.push_str(" -> ");
            h.push_str(&type_text(o));
        }
        self.heading(&r.doc, r.span.line, h);
        let pcs = &r.prop_comments;
        let from = self.out.len();
        if let Some(first) = r.params.first() {
            self.prop(pcs, "in", &params_text(&r.params), first.name.span.line);
        }
        self.prop(pcs, "touches", &touches_text(&r.touches), r.touches.span.line);
        self.group(from);
        self.blank();
        self.fence(0, &r.rust);
    }

    fn fence(&mut self, indent: usize, rust: &RustCode) {
        self.push(rust.line.saturating_sub(1), indent, "```rust");
        let mut n = 0;
        if !rust.code.is_empty() {
            for (k, l) in rust.code.split('\n').enumerate() {
                self.push(rust.line + k as u32, indent, l);
                if let Some(last) = self.out.last_mut() {
                    last.raw = true;
                }
                n += 1;
            }
        }
        self.push(rust.line + n, indent, "```");
    }

    // ------------------------------------------------------------------ steps

    /// Numbered steps at `indent` spaces (SPEC 23.3): 1, 2, 3, ….
    fn stmts(&mut self, body: &[Stmt], indent: usize) {
        for (i, s) in body.iter().enumerate() {
            self.stmt(s, indent, &format!("{}. ", i + 1));
        }
    }

    /// One step. `marker` is its number (`3. `); the steps under it start where its text does.
    fn stmt(&mut self, s: &Stmt, indent: usize, marker: &str) {
        self.comments(indent, &s.comments);
        let line = s.span.line;
        self.attached.insert(line);
        let inner = indent + marker.len();
        match &s.kind {
            StmtKind::ForEach {
                filter_rule,
                var,
                source,
                body,
            } => {
                let rule = filter_rule
                    .as_ref()
                    .map(|r| format!("{} ", r.name))
                    .unwrap_or_default();
                self.push(
                    line,
                    indent,
                    format!("{marker}for each {rule}{} in {}:", var.name, expr_text(source)),
                );
                self.stmts(body, inner);
            }
            StmtKind::If { cond, then, else_ } => {
                self.push(line, indent, format!("{marker}if {}:", expr_text(cond)));
                self.stmts(then, inner);
                self.else_branch(else_, inner);
            }
            StmtKind::IfSome {
                expr,
                bind,
                then,
                else_,
            } => {
                self.push(
                    line,
                    indent,
                    format!("{marker}if {} is some {}:", expr_text(expr), bind.name),
                );
                self.stmts(then, inner);
                self.else_branch(else_, inner);
            }
            StmtKind::Parallel(body) => {
                self.push(line, indent, format!("{marker}parallel:"));
                self.stmts(body, inner);
            }
            StmtKind::Match {
                scrutinee,
                arms,
                otherwise,
            } => {
                self.push(line, indent, format!("{marker}match {}:", expr_text(scrutinee)));
                for a in arms {
                    self.comments(inner, &a.comments);
                    let mut head = String::new();
                    if let Some(q) = &a.qualifier {
                        head.push_str(&q.name);
                        head.push('.');
                    }
                    head.push_str(&a.pattern.name);
                    if let Some(b) = &a.bind {
                        head.push(' ');
                        head.push_str(&b.name);
                    }
                    self.push(a.span.line, inner, format!("- {head}:"));
                    self.attached.insert(a.span.line);
                    self.stmts(&a.body, inner + 2);
                }
                if let Some(o) = otherwise {
                    self.comments(inner, &o.comments);
                    self.push(o.span.line, inner, "- otherwise:");
                    self.attached.insert(o.span.line);
                    self.stmts(&o.body, inner + 2);
                }
            }
            StmtKind::Try { stmt, otherwise } => {
                self.push(line, indent, format!("{marker}try {}", simple_text(stmt)));
                if let Some(o) = otherwise {
                    let before = o.first().map_or(u32::MAX, |s| s.span.line);
                    let at = self.find_line(before, |_, t| t.trim() == "otherwise:");
                    self.push(at, inner, "otherwise:");
                    self.stmts(o, inner);
                }
            }
            StmtKind::Append { value, list } => match &value.kind {
                // `add RegionTotal to totals:` with one bullet per field (SPEC 23.11)
                ExprKind::Variant { qualifier, name, fields } if !fields.is_empty() => {
                    let head = match qualifier {
                        Some(q) => format!("{}.{}", q.name, name.name),
                        None => name.name.clone(),
                    };
                    self.push(line, indent, format!("{marker}add {head} to {}:", expr_text(list)));
                    for f in fields {
                        let same = matches!(&f.value.kind, ExprKind::Path(p) if p.len() == 1 && p[0].name == f.name.name);
                        let text = if same {
                            format!("- {}", f.name.name)
                        } else {
                            format!("- {}: {}", f.name.name, expr_text(&f.value))
                        };
                        self.push(f.name.span.line, inner, text);
                        self.attached.insert(f.name.span.line);
                    }
                }
                _ => self.push(line, indent, format!("{marker}{}", simple_text(s))),
            },
            StmtKind::Coll(c) => self.coll_stmt(c, line, indent, marker),
            _ => self.push(line, indent, format!("{marker}{}", simple_text(s))),
        }
    }

    /// `else:` inside the `if` step, at the indentation of its steps, then the steps it runs.
    fn else_branch(&mut self, else_: &[Stmt], indent: usize) {
        if !else_.is_empty() {
            let at = self.find_line(else_[0].span.line, |_, t| t.trim() == "else:");
            self.push(at, indent, "else:");
            self.stmts(else_, indent);
        }
    }

    // ------------------------------------------------------------------ output

    /// Put the comments the AST dropped back, then produce the text.
    fn render(mut self, orphans: Vec<Orphan>) -> String {
        let mut top: Vec<Out> = Vec::new();
        let mut after: HashMap<usize, Vec<Out>> = HashMap::new();
        for o in orphans {
            let mut best: Option<(u32, usize)> = None;
            for (i, l) in self.out.iter().enumerate() {
                if l.src > 0 && l.src <= o.line && best.is_none_or(|(s, _)| l.src >= s) {
                    best = Some((l.src, i));
                }
            }
            match best {
                Some((_, i)) => {
                    // Above `else:` / `otherwise:` the comment lines up with that line.
                    let indent = match self.out.get(i + 1) {
                        Some(n) if n.text == "otherwise:" || n.text == "else:" => {
                            // never shallower than that line, but a comment that trails the
                            // body stays in it
                            o.indent.min(self.out[i].indent).max(n.indent)
                        }
                        _ => o.indent.min(self.out[i].indent),
                    };
                    after.entry(i).or_default().push(Out {
                        raw: false,
                        src: 0,
                        indent,
                        text: comment_text(&o.text),
                    });
                }
                None => top.push(Out {
                    raw: false,
                    src: 0,
                    indent: 0,
                    text: comment_text(&o.text),
                }),
            }
        }

        let mut all: Vec<Out> = Vec::new();
        let mut top = Some(top);
        for (i, l) in self.out.drain(..).enumerate() {
            if i == self.after_file_doc {
                put_top(&mut all, top.take().unwrap_or_default(), self.file_doc);
            }
            all.push(l);
            if let Some(extra) = after.remove(&i) {
                all.extend(extra);
            }
        }
        if let Some(t) = top.take() {
            put_top(&mut all, t, self.file_doc);
        }

        let mut text = String::new();
        let mut prev_blank = true;
        for l in &all {
            if l.raw {
                if !l.text.is_empty() {
                    text.push_str(&" ".repeat(l.indent));
                }
                text.push_str(&l.text);
                text.push('\n');
                prev_blank = l.text.is_empty();
                continue;
            }
            if l.text.is_empty() {
                if !prev_blank {
                    text.push('\n');
                }
                prev_blank = true;
                continue;
            }
            text.push_str(&" ".repeat(l.indent));
            text.push_str(&l.text);
            text.push('\n');
            prev_blank = false;
        }
        while text.ends_with("\n\n") {
            text.pop();
        }
        text
    }
}

/// Comments that come before every line of the file go right after the file documentation, set
/// apart by blank lines so they do not become part of it (or documentation of the first heading).
fn put_top(all: &mut Vec<Out>, top: Vec<Out>, has_file_doc: bool) {
    if top.is_empty() {
        return;
    }
    if has_file_doc {
        all.push(Out {
            raw: false,
            src: 0,
            indent: 0,
            text: String::new(),
        });
    }
    all.extend(top);
    all.push(Out {
        raw: false,
        src: 0,
        indent: 0,
        text: String::new(),
    });
}

// ---------------------------------------------------------------------------------- orphans

struct Orphan {
    line: u32,
    indent: usize,
    text: String,
}

/// Comment lines of the source that the AST does not hold.
///
/// This mirrors how the parser attaches comments (OPEN_QUESTIONS Q17, Q28): a run directly above
/// a heading is that heading's documentation; the first run of the file is the file's; any other
/// run belongs to the next statement/field/property line before the next heading. A run is lost
/// when the AST has no such documented heading or comment-carrying line for it.
fn find_orphans(lines: &[Line], p: &Printer) -> Vec<Orphan> {
    let mut res = Vec::new();
    let mut seen_heading = false;
    let mut file_doc_taken = false;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind == LineKind::Comment {
            let start = i;
            while i < lines.len() && lines[i].kind == LineKind::Comment {
                i += 1;
            }
            let next = lines.get(i);
            let continuation = start > 0
                && p.nb_items.contains(&lines[start - 1].line_no)
                && lines[start..i].iter().all(|l| l.indent >= 1);
            let accounted = continuation
                || match next.map(|l| &l.kind) {
                Some(LineKind::Heading1 | LineKind::Heading2) => {
                    next.is_some_and(|n| p.docs.contains(&n.line_no))
                }
                _ if !seen_heading && !file_doc_taken => {
                    file_doc_taken = true;
                    p.file_doc
                }
                _ => lines[i..]
                    .iter()
                    .take_while(|l| !matches!(l.kind, LineKind::Heading1 | LineKind::Heading2))
                    .find(|l| l.kind.is_item() || l.kind == LineKind::Control)
                    .is_some_and(|o| p.attached.contains(&o.line_no)),
            };
            if !accounted {
                for l in &lines[start..i] {
                    res.push(Orphan {
                        line: l.line_no,
                        indent: l.indent as usize,
                        text: l.text.clone(),
                    });
                }
            }
            continue;
        }
        if matches!(lines[i].kind, LineKind::Heading1 | LineKind::Heading2) {
            seen_heading = true;
        }
        i += 1;
    }
    res
}

/// The `> comment` after an import on the same line, if any.
fn inline_comment(heading: &str) -> Option<String> {
    let mut prev_space = false;
    for (i, c) in heading.char_indices() {
        if c == '>' && prev_space {
            return Some(heading[i + 1..].trim().to_string());
        }
        prev_space = c.is_whitespace();
    }
    None
}

fn import_key(i: &Import) -> (u8, String) {
    match &i.kind {
        ImportKind::File { path, .. } => (0, path.clone()),
        ImportKind::Glob { pattern } => (0, pattern.clone()),
        ImportKind::Folder { path, .. } => (1, path.clone()),
        ImportKind::Crate { name, .. } => (2, name.name.clone()),
    }
}

// ---------------------------------------------------------------------------------- text of nodes

fn effect_word(e: Effect) -> &'static str {
    match e {
        Effect::Pure => "pure",
        Effect::Local => "local",
        Effect::Network => "network",
        Effect::Model => "model",
        Effect::Unverified => "unverified",
    }
}

pub(crate) fn duration_text(d: &DurationLit) -> String {
    let unit = match d.unit {
        DurationUnit::Second => "second",
        DurationUnit::Minute => "minute",
        DurationUnit::Hour => "hour",
        DurationUnit::Day => "day",
        DurationUnit::Week => "week",
        DurationUnit::Month => "month",
        DurationUnit::Year => "year",
    };
    let plural = if d.amount == 1 { "" } else { "s" };
    format!("{} {unit}{plural}", d.amount)
}

fn weekday_word(d: Weekday) -> &'static str {
    match d {
        Weekday::Monday => "Monday",
        Weekday::Tuesday => "Tuesday",
        Weekday::Wednesday => "Wednesday",
        Weekday::Thursday => "Thursday",
        Weekday::Friday => "Friday",
        Weekday::Saturday => "Saturday",
        Weekday::Sunday => "Sunday",
    }
}

fn trigger_text(t: &Trigger) -> String {
    match t {
        Trigger::Every(d) => format!("Every {}", duration_text(d)),
        Trigger::DailyAt { hour, minute } => format!("Every day at {hour:02}:{minute:02}"),
        Trigger::Weekly { day, hour, minute } => {
            format!("Every {} at {hour:02}:{minute:02}", weekday_word(*day))
        }
        Trigger::OnEvent { tool, event } => format!("When {} receives {}", tool.name, event.name),
        Trigger::OnStart => "On start".to_string(),
        Trigger::Do(n) => format!("Do {}", n.name),
    }
}

pub(crate) fn type_text(t: &TypeExpr) -> String {
    match &t.kind {
        TypeKind::Named(n) => n.clone(),
        TypeKind::List(i) => format!("list of {}", type_text(i)),
        TypeKind::Maybe(i) => format!("maybe {}", type_text(i)),
        TypeKind::Result(i) => format!("result of {}", type_text(i)),
        TypeKind::Map(k, v) => format!("map of {} to {}", type_text(k), type_text(v)),
        TypeKind::Handle { name, mode } => match mode.word() {
            Some(w) => format!("{w} handle of {name}"),
            None => format!("handle of {name}"),
        },
        TypeKind::Infer(words) => words.join(" "),
    }
}

/// The words of `- touches:`.
fn touches_text(t: &Touches) -> String {
    t.words.iter().map(|w| w.name.as_str()).collect::<Vec<_>>().join(", ")
}

/// The link text of a used file nobody labelled: its name without `.agent.md`.
fn default_label(path: &str) -> String {
    let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".agent.md").unwrap_or(name).to_string()
}

fn params_text(ps: &[Param]) -> String {
    ps.iter()
        .map(|p| {
            // a name whose type is found from it is written alone (SPEC 23.6)
            let mut s = match &p.ty.kind {
                TypeKind::Infer(words) => words.join(" "),
                _ => format!("{}: {}", p.name.name, type_text(&p.ty)),
            };
            if let Some(d) = &p.default {
                s.push_str(" = ");
                s.push_str(&at_least(d, 1));
            }
            s
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The text of a statement that has no body, without the leading `- `.
fn simple_text(s: &Stmt) -> String {
    match &s.kind {
        StmtKind::Stop => "stop".to_string(),
        StmtKind::Do { name, args, into, bare } => {
            let mut t = if *bare { name.name.clone() } else { format!("do {}", name.name) };
            if !args.is_empty() {
                t.push('(');
                t.push_str(&exprs_text(args));
                t.push(')');
            }
            if let Some(i) = into {
                t.push_str(&format!(" into {}", i.name));
            }
            t
        }
        StmtKind::AskModel {
            with,
            prompt,
            as_record,
            into,
        } => {
            let mut t = "ask model".to_string();
            if let Some(w) = with {
                t.push_str(&format!(" with {}", expr_text(w)));
            }
            t.push_str(&format!(": {}", expr_text(prompt)));
            if let Some(r) = as_record {
                t.push_str(&format!(" as {}", r.name));
            }
            t.push_str(&format!(" into {}", into.name));
            t
        }
        StmtKind::ToolCall {
            tool,
            verb,
            target,
            payload,
            into,
            via_do,
        } => {
            let mut t = String::new();
            if *via_do {
                t.push_str("do ");
            }
            t.push_str(&tool.name);
            if let Some(v) = verb {
                t.push(' ');
                t.push_str(&v.name);
            }
            for part in target {
                t.push(' ');
                match part {
                    TargetPart::Word(w) => t.push_str(&w.name),
                    TargetPart::Value(e) => t.push_str(&expr_text(e)),
                }
            }
            match payload {
                Some(Payload::Assign(assigns)) => {
                    let a: Vec<String> = assigns
                        .iter()
                        .map(|a| format!("{} = {}", a.field.name, at_least(&a.value, 1)))
                        .collect();
                    t.push_str(": ");
                    t.push_str(&a.join(", "));
                }
                Some(Payload::Value(e)) => {
                    t.push_str(": ");
                    t.push_str(&expr_text(e));
                }
                None => {}
            }
            if let Some(i) = into {
                t.push_str(&format!(" into {}", i.name));
            }
            t
        }
        StmtKind::Guard { cond, else_ } => {
            let mut t = format!("guard: {}", expr_text(cond));
            if let Some(e) = else_ {
                t.push_str(" else ");
                t.push_str(&simple_text(e));
            }
            t
        }
        StmtKind::Set { var, expr } => format!("{} into {}", expr_text(expr), var.name),
        StmtKind::Return(e) => format!("return {}", expr_text(e)),
        StmtKind::Respond { status, body } => match body {
            Some(b) => format!("respond {status} {}", expr_text(b)),
            None => format!("respond {status}"),
        },
        StmtKind::ReadBody { ty, into } => {
            format!("read body as {} into {}", type_text(ty), into.name)
        }
        StmtKind::ReadFile { path, ty, into } => format!(
            "read file {} as {} into {}",
            expr_text(path),
            type_text(ty),
            into.name
        ),
        StmtKind::Append { value, list } => {
            format!("add {} to {}", expr_text(value), expr_text(list))
        }
        StmtKind::ReadInput { into } => format!("read input into {}", into.name),
        StmtKind::File { verb, path, arg, into, into_ty } => {
            let mut t = format!("file {} {}", verb.name, expr_text(path));
            match arg {
                Some(FileArg::To(e)) => t.push_str(&format!(" to {}", expr_text(e))),
                Some(FileArg::Content(e)) => t.push_str(&format!(": {}", expr_text(e))),
                None => {}
            }
            if let Some(i) = into {
                t.push_str(&format!(" into {}", i.name));
            }
            if let Some(ty) = into_ty {
                t.push_str(&format!(": {}", type_text(ty)));
            }
            t
        }
        StmtKind::CallInto { call, into, via_do } => {
            format!("{}{} into {}", if *via_do { "do " } else { "" }, expr_text(call), into.name)
        }
        StmtKind::Print(e) => format!("print: {}", expr_text(e)),
        StmtKind::Coll(c) => collections::simple(c).unwrap_or_else(|| "stop".to_string()),
        // Statements with a body never reach here from the parser; print the head if they do.
        StmtKind::ForEach { .. }
        | StmtKind::If { .. }
        | StmtKind::IfSome { .. }
        | StmtKind::Try { .. }
        | StmtKind::Parallel(_)
        | StmtKind::Match { .. }
        => "stop".to_string(),
        StmtKind::ModelCall { .. } | StmtKind::Trace(_) => unreachable!("L2-only node in fmt"),
    }
}

// ---------------------------------------------------------------------------------- expressions

fn exprs_text(es: &[Expr]) -> String {
    // a list or map literal would take the commas that follow it, so it is parenthesised here
    es.iter().map(|e| at_least(e, 1)).collect::<Vec<_>>().join(", ")
}

/// Binding strength, weakest first. `first ... where` swallows the rest of the line, so it counts
/// as weaker than everything and is parenthesised whenever something follows it.
fn prec(e: &Expr) -> u8 {
    if negated(e).is_some() {
        return 9;
    }
    match &e.kind {
        ExprKind::FirstIn { .. } => 0,
        ExprKind::Binary { op, .. } => match op {
            BinOp::Or => 1,
            BinOp::And => 2,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => 4,
            BinOp::Add | BinOp::Sub => 5,
            BinOp::Mul | BinOp::Div => 6,
        },
        ExprKind::Not(_) => 3,
        ExprKind::IsEmpty(_) | ExprKind::IsNotEmpty(_) | ExprKind::Matches { .. } => 4,
        ExprKind::Length(_) => 7,
        ExprKind::Coll(c) => collections::prec(c),
        _ => 9,
    }
}

/// `-x` is parsed as `x * -1` whose `-1` has the span of the whole expression (Q419); a written `x * -1`
/// has a smaller one. The operand of the negation, when `e` is one.
fn negated(e: &Expr) -> Option<&Expr> {
    match &e.kind {
        ExprKind::Binary { op: BinOp::Mul, lhs, rhs } if matches!(rhs.kind, ExprKind::Int(-1)) && rhs.span == e.span => Some(lhs),
        _ => None,
    }
}

fn op_text(op: BinOp) -> &'static str {
    match op {
        BinOp::Or => "or",
        BinOp::And => "and",
        BinOp::Eq => "=",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
    }
}

/// `e` as canonical source text.
pub(crate) fn expr_text(e: &Expr) -> String {
    at_least(e, 0)
}

/// `e`, in parentheses when it binds weaker than `min`.
fn at_least(e: &Expr, min: u8) -> String {
    let s = raw_expr(e);
    if prec(e) < min {
        format!("({s})")
    } else {
        s
    }
}

fn raw_expr(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Str(s) => format!("\"{}\"", escape(s)),
        ExprKind::Int(n) => n.to_string(),
        ExprKind::Money { minor, currency } => money_text(*minor, currency),
        ExprKind::Duration(d) => duration_text(d),
        ExprKind::Date(d) => d.clone(),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Today => "today".to_string(),
        ExprKind::Now => "now".to_string(),
        ExprKind::Path(p) => path_text(p),
        ExprKind::Interp(parts) => {
            let mut s = String::from("\"");
            for part in parts {
                match part {
                    InterpPart::Text(t) => s.push_str(&escape(t)),
                    InterpPart::Expr(x) => {
                        s.push('{');
                        s.push_str(&expr_text(x));
                        s.push('}');
                    }
                }
            }
            s.push('"');
            s
        }
        ExprKind::Binary { lhs, .. } if negated(e).is_some() => {
            if negated(lhs).is_some() {
                format!("-({})", expr_text(lhs))
            } else {
                format!("-{}", at_least(lhs, 9))
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let p = prec(e);
            // Comparisons do not chain: both sides must be sums or tighter.
            let left_min = if p == 4 { 5 } else { p };
            format!(
                "{} {} {}",
                at_least(lhs, left_min),
                op_text(*op),
                at_least(rhs, p + 1)
            )
        }
        ExprKind::Not(x) => format!("not {}", at_least(x, 3)),
        ExprKind::Call { func, args } => format!("{}({})", path_text(func), exprs_text(args)),
        ExprKind::Decode { call, ty } => format!("{} as {}", raw_expr(call), type_text(ty)),
        ExprKind::Record(fields) => {
            if fields.is_empty() {
                "{}".to_string()
            } else {
                let f: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name.name, at_least(&f.value, 1)))
                    .collect();
                format!("{{ {} }}", f.join(", "))
            }
        }
        ExprKind::Some(x) => format!("some {}", at_least(x, 9)),
        ExprKind::None => "none".to_string(),
        ExprKind::Variant {
            qualifier,
            name,
            fields,
        } => {
            let head = match qualifier {
                Some(q) => format!("{}.{}", q.name, name.name),
                None => name.name.clone(),
            };
            if fields.is_empty() {
                format!("{head} {{}}")
            } else {
                let f: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name.name, expr_text(&f.value)))
                    .collect();
                format!("{head} {{ {} }}", f.join(", "))
            }
        }
        ExprKind::CountOf { rule, noun, source } => match (rule, noun) {
            (Some(r), Some(n)) => format!(
                "count of {} {} in {}",
                r.name,
                n.name,
                at_least(source, 9)
            ),
            _ => format!("count of {}", at_least(source, 9)),
        },
        ExprKind::FirstIn { var, source, cond } => format!(
            "first {} in {} where {}",
            var.name,
            at_least(source, 9),
            expr_text(cond)
        ),
        ExprKind::IsEmpty(x) => format!("{} is empty", at_least(x, 5)),
        ExprKind::IsNotEmpty(x) => format!("{} is not empty", at_least(x, 5)),
        ExprKind::Matches { expr, record } => {
            format!("{} matches {}", at_least(expr, 5), record.name)
        }
        ExprKind::Length(x) => format!("{} length", at_least(x, 9)),
        ExprKind::NoAmountsChanged(x) => {
            format!("no amounts changed in {}", at_least(x, 9))
        }
        ExprKind::Coll(c) => collections::expr(c),
    }
}

fn path_text(p: &[Ident]) -> String {
    p.iter().map(|i| i.name.as_str()).collect::<Vec<_>>().join(".")
}

/// Text inside quotes. A raw `}` is fine; a raw `{` would open a hole.
fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '{' => o.push_str("\\{"),
            '\n' => o.push_str("\\n"),
            c => o.push(c),
        }
    }
    o
}

/// Decimal places of a currency's minor unit (the same table as the parser).
fn exponent(cur: &str) -> u32 {
    match cur {
        "JPY" | "KRW" | "VND" | "CLP" | "ISK" => 0,
        "KWD" | "BHD" | "OMR" | "JOD" | "TND" => 3,
        _ => 2,
    }
}

/// `1250 EUR` minor units as `12.50 EUR`; whole amounts print without decimals (`14 EUR`).
fn money_text(minor: i64, cur: &str) -> String {
    let exp = exponent(cur);
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    let div = 10u64.pow(exp);
    let (whole, frac) = (abs / div, abs % div);
    if exp == 0 || frac == 0 {
        format!("{sign}{whole} {cur}")
    } else {
        format!("{sign}{whole}.{frac:0>w$} {cur}", w = exp as usize)
    }
}
