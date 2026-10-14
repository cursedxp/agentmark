//! Formatter (SPEC 18.6): one canonical style, no options.
//!
//! The file is parsed, then printed again from the AST, so the style is the printer's and not a
//! patched-up version of what the author typed:
//!
//! - 2 spaces per indentation level, no trailing whitespace, one trailing newline;
//! - one blank line between declarations (and before every `##` block);
//! - properties before blocks, in the order the language documents them;
//! - imports first, sorted: local files (and globs), then folders, then crates;
//! - every statement is a list item (`- for each ...:`, `- if ...:`, `- else:`); `otherwise:` is
//!   the one bare control line, as in SPEC 14;
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
    Ok(format_file(&file, &lines))
}

/// `fmt --check`: is `src` already in canonical form?
pub fn is_canonical(src: &str, path: &str) -> Result<bool, Vec<Diagnostic>> {
    Ok(format_source(src, path)? == src)
}

/// Print a parsed file. `lines` is the lexed source of the same file (used to keep comments the
/// AST does not hold).
fn format_file(file: &File, lines: &[Line]) -> String {
    let mut p = Printer {
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

    fn doc(&mut self, doc: &Option<String>, heading_line: u32) {
        if let Some(d) = doc {
            for l in d.split('\n') {
                self.push(0, 0, comment_text(l));
            }
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

        let mut imports: Vec<&Import> = f.imports.iter().collect();
        imports.sort_by_key(|i| import_key(i));
        if !imports.is_empty() {
            self.blank();
            for imp in imports {
                self.import(imp);
            }
        }
        for d in &f.decls {
            self.blank();
            self.decl(d);
        }
    }

    fn import(&mut self, imp: &Import) {
        self.doc(&imp.doc, imp.span.line);
        let mut text = match &imp.kind {
            ImportKind::File { path, alias } => match alias {
                Some(a) => format!("# Import: {path} as {}", a.name),
                None => format!("# Import: {path}"),
            },
            ImportKind::Glob { pattern } => format!("# Import: {pattern}"),
            ImportKind::Folder { path, alias } => format!("# Import: {path} as {}", alias.name),
            ImportKind::Crate { name, alias } => match alias {
                Some(a) => format!("# Import: crate {} as {}", name.name, a.name),
                None => format!("# Import: crate {}", name.name),
            },
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
            Decl::Rule(r) => self.rule(r),
            Decl::Agent(a) => self.agent(a),
            Decl::Function(f) => self.function(f),
            Decl::Service(s) => self.service(s),
            Decl::Command(c) => self.command(c),
            Decl::Binding(b) => self.binding(b),
            Decl::Constant(c) => self.constant(c),
            Decl::Run(r) => self.run(r),
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
        self.doc(doc, line);
        self.push(line, 0, text);
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

    fn rule(&mut self, r: &Rule) {
        self.heading(&r.doc, r.span.line, format!("# Rule: {}", r.name.name));
        let pcs = &r.prop_comments;
        let from = self.out.len();
        self.prop(pcs, "for", &r.for_record.name, r.for_record.span.line);
        self.prop(pcs, "when", &expr_text(&r.when), r.when.span.line);
        self.group(from);
    }

    fn constant(&mut self, c: &Constant) {
        self.heading(&c.doc, c.span.line, format!("# Constant: {}", c.name.name));
        self.prop(&c.prop_comments, "value", &expr_text(&c.value), c.value.span.line);
    }

    fn run(&mut self, r: &Run) {
        self.heading(&r.doc, r.span.line, "# Run".to_string());
        for i in &r.items {
            self.push(i.span.line, 0, format!("- {}", i.name));
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
        self.prop(pcs, "effect", effect_word(f.effect.value), f.effect.span.line);
        self.group(from);
        self.stmts(&f.body, 1);
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
            self.prop(&c.prop_comments, "args", &params_text(&c.args), first.name.span.line);
        }
        self.blank();
        let before = c.body.first().map_or(u32::MAX, |s| s.span.line);
        let run = self.find_line(before, |k, t| {
            *k == LineKind::Heading2 && t.trim().eq_ignore_ascii_case("run")
        });
        self.push(run, 0, "## Run");
        self.stmts(&c.body, 0);
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
            let mut h = format!("## {}({})", item.name.name, params_text(&item.params));
            if let Some(o) = &item.out {
                h.push_str(" -> ");
                h.push_str(&type_text(o));
            }
            self.heading(&item.doc, item.span.line, h);
            self.fence(1, &item.rust);
        }
        for h in &b.handles {
            self.blank();
            self.heading(&None, h.span.line, format!("## Handle: {}", h.name));
            self.fence(1, &h.rust);
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
                        Some(c) if i > 0 => self.push(0, 1, comment_text(c.trim_start())),
                        _ => self.push(if i == 0 { n.span.line } else { 0 }, 0, l.to_string()),
                    }
                }
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
        self.prop(pcs, "effect", effect_word(r.effect.value), r.effect.span.line);
        self.group(from);
        self.fence(1, &r.rust);
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

    // ------------------------------------------------------------------ statements

    fn stmts(&mut self, body: &[Stmt], indent: usize) {
        for s in body {
            self.stmt(s, indent);
        }
    }

    fn stmt(&mut self, s: &Stmt, indent: usize) {
        self.comments(indent, &s.comments);
        let line = s.span.line;
        self.attached.insert(line);
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
                    format!("- for each {rule}{} in {}:", var.name, expr_text(source)),
                );
                self.stmts(body, indent + 1);
            }
            StmtKind::If { cond, then, else_ } => {
                self.push(line, indent, format!("- if {}:", expr_text(cond)));
                self.stmts(then, indent + 1);
                self.else_branch(else_, indent);
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
                    format!("- if {} is some {}:", expr_text(expr), bind.name),
                );
                self.stmts(then, indent + 1);
                self.else_branch(else_, indent);
            }
            StmtKind::Parallel(body) => {
                self.push(line, indent, "- parallel:");
                self.stmts(body, indent + 1);
            }
            StmtKind::Try { stmt, otherwise } => {
                self.push(line, indent, format!("- try {}", simple_text(stmt)));
                if let Some(o) = otherwise {
                    let before = o.first().map_or(u32::MAX, |s| s.span.line);
                    let at = self.find_line(before, |_, t| t.trim() == "otherwise:");
                    self.push(at, indent + 1, "otherwise:");
                    self.stmts(o, indent + 2);
                }
            }
            _ => self.push(line, indent, format!("- {}", simple_text(s))),
        }
    }

    fn else_branch(&mut self, else_: &[Stmt], indent: usize) {
        if !else_.is_empty() {
            let at = self.find_line(else_[0].span.line, |_, t| {
                matches!(t.trim(), "else:" | "- else:")
            });
            self.push(at, indent, "- else:");
            self.stmts(else_, indent + 1);
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
                        Some(n) if n.text == "otherwise:" || n.text == "- else:" => {
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
                    text.push_str(&"  ".repeat(l.indent));
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
            for _ in 0..l.indent {
                text.push_str("  ");
            }
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
                    .find(|l| matches!(l.kind, LineKind::Item | LineKind::Control))
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
        TypeKind::Handle { name, mode } => match mode.word() {
            Some(w) => format!("{w} handle of {name}"),
            None => format!("handle of {name}"),
        },
    }
}

fn params_text(ps: &[Param]) -> String {
    ps.iter()
        .map(|p| {
            let mut s = format!("{}: {}", p.name.name, type_text(&p.ty));
            if let Some(d) = &p.default {
                s.push_str(" = ");
                s.push_str(&expr_text(d));
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
        StmtKind::Do { name, args, into } => {
            let mut t = format!("do {}", name.name);
            if !args.is_empty() {
                t.push('(');
                t.push_str(&exprs_text(args));
                t.push(')');
            }
            if let Some(i) = into {
                t.push_str(&format!(" -> {}", i.name));
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
            t.push_str(&format!(" -> {}", into.name));
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
                        .map(|a| format!("{} = {}", a.field.name, expr_text(&a.value)))
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
                t.push_str(&format!(" -> {}", i.name));
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
        StmtKind::Set { var, expr } => format!("set {} = {}", var.name, expr_text(expr)),
        StmtKind::Return(e) => format!("return {}", expr_text(e)),
        StmtKind::Respond { status, body } => match body {
            Some(b) => format!("respond {status} {}", expr_text(b)),
            None => format!("respond {status}"),
        },
        StmtKind::ReadBody { ty, into } => {
            format!("read body as {} -> {}", type_text(ty), into.name)
        }
        StmtKind::ReadFile { path, ty, into } => format!(
            "read file {} as {} -> {}",
            expr_text(path),
            type_text(ty),
            into.name
        ),
        StmtKind::Append { value, list } => {
            format!("append {} to {}", expr_text(value), expr_text(list))
        }
        StmtKind::Print(e) => format!("print: {}", expr_text(e)),
        // Statements with a body never reach here from the parser; print the head if they do.
        StmtKind::ForEach { .. }
        | StmtKind::If { .. }
        | StmtKind::IfSome { .. }
        | StmtKind::Try { .. }
        | StmtKind::Parallel(_)
        => "stop".to_string(),
        StmtKind::ModelCall { .. } | StmtKind::Trace(_) => unreachable!("L2-only node in fmt"),
    }
}

// ---------------------------------------------------------------------------------- expressions

fn exprs_text(es: &[Expr]) -> String {
    es.iter().map(expr_text).collect::<Vec<_>>().join(", ")
}

/// Binding strength, weakest first. `first ... where` swallows the rest of the line, so it counts
/// as weaker than everything and is parenthesised whenever something follows it.
fn prec(e: &Expr) -> u8 {
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
        _ => 9,
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
        ExprKind::Record(fields) => {
            if fields.is_empty() {
                "{}".to_string()
            } else {
                let f: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name.name, expr_text(&f.value)))
                    .collect();
                format!("{{ {} }}", f.join(", "))
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
