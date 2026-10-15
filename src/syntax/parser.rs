//! Parser (DESIGN 2.2): `Vec<Line>` -> [`File`].
//!
//! Two levels. The *structure* level groups classified lines into declarations, properties,
//! blocks and an indentation tree of statements (plain code over the line list). The *inline*
//! level parses the text of one line with chumsky grammars: headings and property values in
//! [`heading`], statements in [`stmt`], expressions and types in [`expr`]. All token-level
//! grammar is chumsky; nothing is parsed by hand.

use std::collections::{HashMap, HashSet};

use chumsky::error::{Rich, RichPattern, RichReason};

use crate::diag::{did_you_mean, Diagnostic, Span};
use crate::syntax::ast::*;
use crate::syntax::lexer::{lex, Line, LineKind};

pub mod collections;
pub mod expr;
pub mod heading;
pub mod stmt;
mod tool;

use expr::{run, LineCtx, P};
use heading::*;
use stmt::Head;

/// First words of the blocks an agent may have.
const BLOCK_WORDS: &[&str] = &["Every", "When", "On", "Do", "Rust"];

/// Words that start a statement (a near miss gets a suggestion).
const STATEMENT_WORDS: &[&str] = &[
    "for",
    "if",
    "else",
    "stop",
    "do",
    "ask",
    "guard",
    "return",
    "respond",
    "read",
    "try",
    "parallel",
    "add",
    "put",
    "remove",
    "print",
    "otherwise",
    "match",
    "file",
];

// Diagnostic codes owned by the parser (the lexer owns E001-E004).
const E_UNKNOWN_PROP: &str = "E010";
const E_DUP_PROP: &str = "E011";
const E_MISSING_PROP: &str = "E012";
const E_UNKNOWN_DECL: &str = "E020";
const E_OUTSIDE: &str = "E030";
const E_INDENT_DEEP: &str = "E031";
const E_STRUCTURE: &str = "E032";
const E_SYNTAX: &str = "E040";
const E_IMPORT: &str = "E050";
// Removed syntax (SPEC 23.13) and the v0.4 structure.
const E_RUN_REMOVED: &str = "E060";
const E_RUN_BLOCK: &str = "E061";
const E_ARGS: &str = "E062";
const E_EFFECT: &str = "E063";
const E_SET: &str = "E064";
const E_ARROW: &str = "E065";
const E_APPEND: &str = "E066";
const E_RULE_FOR: &str = "E067";
const E_DOC_ABOVE: &str = "E068";
const E_IMPORT_FILE: &str = "E069";
const E_STEP_BULLET: &str = "E070";
const E_PROP_NUMBER: &str = "E071";
const E_PROGRAM: &str = "E072";
const E_USES: &str = "E073";

/// Stack of the thread that parses (virtual memory: only what the parser touches is committed).
/// The grammar is recursive and its frames are large in a debug build: nested collection words
/// (`join (map (sort (rows where …) by …) to …) with …`) need more than the 2 MiB of a test thread.
const PARSE_STACK: usize = 64 * 1024 * 1024;

/// Parse one `.agent.md` file. `path` is the name shown in diagnostics.
pub fn parse_file(path: &str, src: &str) -> Result<File, Vec<Diagnostic>> {
    std::thread::scope(|scope| {
        let job = std::thread::Builder::new()
            .name("agentmark-parse".into())
            .stack_size(PARSE_STACK)
            .spawn_scoped(scope, || parse_file_here(path, src));
        match job {
            Ok(handle) => handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
            // no thread to be had: parse on this one
            Err(_) => parse_file_here(path, src),
        }
    })
}

/// How many parentheses or braces one line may nest. The grammar recurses once per level; past
/// this a line is refused with a plain message instead of exhausting the stack (E329).
const MAX_NESTING: usize = 100;

fn too_deep(path: &str, lines: &[Line]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for l in lines {
        if !matches!(l.kind, LineKind::Item | LineKind::Numbered(_) | LineKind::Control | LineKind::Heading1 | LineKind::Heading2) {
            continue;
        }
        let (mut depth, mut chars) = (0usize, 0u32);
        for c in l.text.chars() {
            chars += 1;
            match c {
                '(' | '{' => depth += 1,
                ')' | '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
            if depth > MAX_NESTING {
                out.push(
                    Diagnostic::error(
                        "E329",
                        path,
                        Span::new(l.line_no, l.col + chars - 1, 1),
                        format!("This line nests more than {MAX_NESTING} levels of parentheses or braces."),
                    )
                    .with_help("Name the inner part on a step of its own: \"<value> into <name>\"."),
                );
                break;
            }
        }
    }
    out
}

fn parse_file_here(path: &str, src: &str) -> Result<File, Vec<Diagnostic>> {
    let (lines, diags) = lex(path, src);
    if !diags.is_empty() {
        // Lines were dropped by the lexer, so the structure cannot be trusted any further.
        return Err(diags);
    }
    let deep = too_deep(path, &lines);
    if !deep.is_empty() {
        return Err(deep);
    }
    let mut b = Builder::new(path);
    let file = b.file(&lines);
    if b.diags.is_empty() {
        Ok(file)
    } else {
        b.diags.sort_by_key(|d| (d.span.line, d.span.col));
        Err(b.diags)
    }
}

// ---------------------------------------------------------------------------------- grammar set

/// Selects one grammar out of the set.
type Pick<'l, T> = for<'g> fn(&'g Grammar<'l>) -> &'g P<'l, T>;

struct Grammar<'l> {
    h1_word: P<'l, Ident>,
    h1: P<'l, H1>,
    trigger: P<'l, Trigger>,
    route: P<'l, (String, String, Vec<String>)>,
    rust_heading: P<'l, (Ident, Option<TypeExpr>)>,
    not_bound: P<'l, ()>,
    binding_item: P<'l, (Ident, Vec<Param>, Option<TypeExpr>)>,
    prop_head: P<'l, (Ident, &'l str, chumsky::span::SimpleSpan)>,
    field: P<'l, (Ident, Option<TypeExpr>)>,
    variant: P<'l, (Ident, Vec<(Ident, TypeExpr)>)>,
    arm: P<'l, stmt::Arm>,
    params: P<'l, Vec<Param>>,
    in_params: P<'l, Vec<Param>>,
    out_type: P<'l, TypeExpr>,
    name_list: P<'l, Vec<Ident>>,
    model: P<'l, Spanned<String>>,
    memory: P<'l, Memory>,
    effect: P<'l, Effect>,
    single_name: P<'l, Ident>,
    rest: P<'l, Spanned<String>>,
    uses_item: P<'l, (String, Spanned<String>, Option<Ident>)>,
    start: P<'l, (Option<String>, Spanned<String>)>,
    add_field: P<'l, (Ident, Option<Expr>)>,
    not_bound_item: P<'l, (String, Option<String>)>,
    expr: P<'l, Expr>,
    ty: P<'l, TypeExpr>,
    duration: P<'l, DurationLit>,
    head: P<'l, Head>,
}

impl<'l> Grammar<'l> {
    fn new() -> Self {
        Grammar {
            h1_word: h1_word(),
            h1: h1(),
            trigger: trigger(),
            route: route(),
            rust_heading: rust_heading(),
            not_bound: not_bound_heading(),
            binding_item: binding_item_heading(),
            prop_head: property_head(),
            field: field(),
            variant: variant(),
            arm: stmt::arm(),
            params: params(),
            in_params: in_params(),
            out_type: out_type(),
            name_list: name_list(),
            model: model_string(),
            memory: memory(),
            effect: effect(),
            single_name: single_name(),
            rest: rest_of_line(),
            uses_item: uses_item(),
            start: start_value(),
            add_field: stmt::add_field(),
            not_bound_item: not_bound_item(),
            expr: expr::expr(),
            ty: expr::type_expr(),
            duration: expr::duration(),
            head: stmt::head(),
        }
    }
}

// ---------------------------------------------------------------------------------- tree

struct Fence {
    lang: String,
    code: String,
    /// Source line of the first code line.
    first_line: u32,
}

struct Node<'l> {
    line: &'l Line,
    comments: Vec<String>,
    kids: Vec<Node<'l>>,
    fence: Option<Fence>,
}

struct Prop<'n, 'l> {
    key: Ident,
    comments: Vec<String>,
    value: &'l str,
    line: u32,
    col: u32,
    node: &'n Node<'l>,
}

struct Builder<'l> {
    file: &'l str,
    g: Grammar<'l>,
    diags: Vec<Diagnostic>,
    docs: HashMap<u32, String>,
    /// `## Rust:` blocks found inside the declaration being built; pushed after it.
    hoisted: Vec<Decl>,
    /// Lines already reported as wrongly indented (their follow-on structure errors are noise).
    bad_lines: HashSet<u32>,
    /// A property of the declaration being built was rejected, so a "missing" report would be noise.
    rejected_prop: bool,
    /// Plain comments waiting for the next node.
    pending: Vec<String>,
}

fn line_span(l: &Line) -> Span {
    Span::new(l.line_no, l.col, l.text.chars().count() as u32)
}

impl<'l> Builder<'l> {
    fn new(file: &'l str) -> Self {
        Builder {
            file,
            g: Grammar::new(),
            diags: Vec::new(),
            docs: HashMap::new(),
            hoisted: Vec::new(),
            bad_lines: HashSet::new(),
            rejected_prop: false,
            pending: Vec::new(),
        }
    }

    fn error(&mut self, code: &'static str, span: Span, msg: impl Into<String>) {
        self.diags
            .push(Diagnostic::error(code, self.file, span, msg));
    }

    /// A structure error about a line below another; skipped when that line's indentation was
    /// already reported.
    fn kid_error(&mut self, l: &Line, msg: impl Into<String>) {
        if !self.bad_lines.contains(&l.line_no) {
            self.error(E_STRUCTURE, line_span(l), msg);
        }
    }

    /// Run a grammar over `text`; a failure becomes a diagnostic and `None`.
    fn parse<T>(&mut self, pick: Pick<'l, T>, text: &'l str, line: u32, col: u32) -> Option<T> {
        match run(pick(&self.g), text, line, col) {
            Ok(v) => Some(v),
            Err((ctx, errs)) => {
                if !self.report_problems(&ctx) {
                    if let Some(e) = errs.first() {
                        self.diags.push(rich_diag(self.file, &ctx, e));
                    }
                }
                None
            }
        }
    }

    /// Report the mistakes the grammar flagged itself (they carry the best wording).
    fn report_problems(&mut self, ctx: &LineCtx) -> bool {
        for p in &ctx.problems {
            let d = Diagnostic::error(E_SYNTAX, self.file, p.span, p.message.clone())
                .with_did_you_mean(p.did_you_mean.clone());
            self.diags.push(d);
        }
        !ctx.problems.is_empty()
    }

    /// A statement line that did not parse: look for the likely cause before falling back to the
    /// generic message.
    fn statement_error(&mut self, ctx: &LineCtx, errs: &[Rich<'_, char>]) {
        if self.report_problems(ctx) {
            return;
        }
        let words = words_of(ctx);
        let first = words.first().cloned();
        // Where the generic error sits, and whether it is before the first ":" of the line
        // (after it, the statement's shape was fine and the first word is not to blame).
        let off = errs.first().map_or(0, |e| e.span().start);
        let before_colon = ctx.text.find(':').is_none_or(|c| off <= c);
        if let Some((w, span)) = first {
            let w = w.trim_end_matches(':').to_string();
            let ends_in_colon = ctx.text.trim_end().ends_with(':');
            let needs_colon =
                matches!(w.as_str(), "for" | "if" | "else" | "otherwise" | "parallel" | "match");
            if needs_colon && !ends_in_colon {
                self.diags.push(missing_colon(self.file, ctx));
                return;
            }
            if w == "ask" && before_colon {
                if let Some((m, mspan)) = words.get(1) {
                    let m = m.trim_end_matches(':');
                    if m != "model" {
                        let mut mspan = *mspan;
                        mspan.len = m.chars().count() as u32;
                        let d = Diagnostic::error(
                            E_SYNTAX,
                            self.file,
                            mspan,
                            format!("Unknown word \"{m}\" after \"ask\". The statement is \"ask model: ...\"."),
                        )
                        .suggesting(m, ["model"]);
                        self.diags.push(d);
                        return;
                    }
                }
            }
            if before_colon && !STATEMENT_WORDS.contains(&w.as_str()) {
                if let Some(dym) = did_you_mean(&w, STATEMENT_WORDS.iter().copied()) {
                    let mut span = span;
                    span.len = w.chars().count() as u32;
                    let d = Diagnostic::error(
                        E_SYNTAX,
                        self.file,
                        span,
                        format!("Unknown statement \"{w}\"."),
                    )
                    .with_did_you_mean(Some(dym));
                    self.diags.push(d);
                    return;
                }
            }
        }
        if let Some(e) = errs.first() {
            self.diags.push(rich_diag(self.file, ctx, e));
        }
    }

    // ------------------------------------------------------------------ file level

    fn file(&mut self, lines: &'l [Line]) -> File {
        let (docs, file_doc, doc_lines, doc_errors) = collect_docs(self.file, lines);
        self.docs = docs;
        self.diags.extend(doc_errors);
        let mut file = File {
            path: self.file.to_string(),
            doc: file_doc,
            imports: Vec::new(),
            decls: Vec::new(),
        };

        let live: Vec<&'l Line> = lines
            .iter()
            .filter(|l| match l.kind {
                LineKind::Blank | LineKind::Paragraph => false,
                LineKind::Comment => !doc_lines.contains(&l.line_no),
                _ => true,
            })
            .collect();

        // Group into chunks that start at each level-1 heading.
        let mut chunks: Vec<(&'l Line, Vec<&'l Line>)> = Vec::new();
        let mut stray_reported = false;
        for l in live {
            if l.kind == LineKind::Heading1 {
                chunks.push((l, Vec::new()));
            } else if let Some((_, body)) = chunks.last_mut() {
                body.push(l);
            } else if l.kind == LineKind::Comment {
                // a note before the first heading
            } else if !stray_reported {
                stray_reported = true;
                self.error(
                    E_OUTSIDE,
                    line_span(l),
                    "This line is not inside a part. Start with a heading such as \"# Program: name\" or \"# Record: Name\".",
                );
            }
        }

        let mut seen_decl = false;
        for (i, (h, body)) in chunks.into_iter().enumerate() {
            self.chunk(i, h, &body, &mut file, &mut seen_decl);
        }
        file
    }

    fn chunk(&mut self, index: usize, h: &'l Line, body: &[&'l Line], file: &mut File, seen_decl: &mut bool) {
        let doc = self.docs.get(&h.line_no).cloned();

        // The first word decides whether this is a known kind of part.
        let word = match self.parse(|g| &g.h1_word, &h.text, h.line_no, h.col) {
            Some(w) => w,
            None => return,
        };
        let removed = REMOVED_HEADINGS.iter().any(|k| k.eq_ignore_ascii_case(&word.name));
        if !removed && !HEADING_WORDS.iter().any(|k| k.eq_ignore_ascii_case(&word.name)) {
            let d = Diagnostic::error(
                E_UNKNOWN_DECL,
                self.file,
                word.span,
                format!("Unknown heading \"{}\".", word.name),
            )
            .suggesting(&word.name, HEADING_WORDS.iter().copied())
            .with_help(format!(
                "A part starts with one of: {}.",
                HEADING_WORDS.join(", ")
            ));
            self.diags.push(d);
            return;
        }
        let Some(parsed) = self.parse(|g| &g.h1, &h.text, h.line_no, h.col) else {
            return;
        };

        let span = line_span(h);
        match parsed {
            H1::Removed(w) => {
                let d = Diagnostic::error(
                    E_RUN_REMOVED,
                    self.file,
                    w.span,
                    format!(
                        "\"# {}\" was removed. Start the entry file with \"# Program: <name>\"; name the command it starts with in \"- start: <command>\".",
                        w.name
                    ),
                )
                .with_help("Write:\n# Program: my-program\nWhat the program does, in one sentence.\n\n- start: <command>");
                self.diags.push(d);
            }
            H1::Import(t) => {
                if !t.is_crate {
                    let path = t.target.name.clone();
                    let label = path
                        .rsplit('/')
                        .find(|s| !s.is_empty() && *s != "*.agent.md")
                        .map(|s| s.strip_suffix(".agent.md").unwrap_or(s).to_string())
                        .unwrap_or_else(|| path.clone());
                    let alias = t.alias.as_ref().map(|a| format!(" as {}", a.name)).unwrap_or_default();
                    let d = Diagnostic::error(
                        E_IMPORT_FILE,
                        self.file,
                        span,
                        "File imports moved to a \"# Uses\" section.",
                    )
                    .with_help(format!("Write:\n# Uses\n- [{label}]({path}){alias}"));
                    self.diags.push(d);
                    return;
                }
                if *seen_decl {
                    // crate imports keep their heading; they belong with the other uses, at the top
                }
                self.import(h, body, doc, t, file);
            }
            H1::Uses => {
                if *seen_decl {
                    self.error(
                        E_USES,
                        span,
                        "\"# Uses\" goes at the top of the file, before the other parts (after \"# Program\").",
                    );
                }
                self.uses(h, body, file);
            }
            H1::Program(name) => {
                if index > 0 {
                    self.error(
                        E_PROGRAM,
                        span,
                        "\"# Program\" must be the first heading of the file.",
                    );
                }
                let mut pos = 0;
                let nodes = self.build(body, &mut pos, 0);
                if let Some(p) = self.program(span, doc, name, &nodes) {
                    file.decls.push(p);
                }
            }
            H1::Rule(words) => {
                *seen_decl = true;
                self.hoisted.clear();
                self.rejected_prop = false;
                if let Some(d) = self.rule(span, doc, words, body) {
                    file.decls.push(d);
                }
            }
            H1::Decl(kind, name) => {
                *seen_decl = true;
                self.hoisted.clear();
                self.rejected_prop = false;
                let decl = self.declaration(kind, span, doc, name, body);
                if let Some(d) = decl {
                    file.decls.push(d);
                }
                file.decls.append(&mut self.hoisted);
            }
        }
    }

    fn import(
        &mut self,
        h: &Line,
        body: &[&Line],
        doc: Option<String>,
        t: ImportTarget,
        file: &mut File,
    ) {
        let span = line_span(h);
        if let Some(extra) = body.first() {
            self.error(
                E_STRUCTURE,
                line_span(extra),
                "A library import is a single line and has nothing under it.",
            );
        }
        file.imports.push(Import {
            span,
            doc,
            kind: ImportKind::Crate {
                name: t.target,
                alias: t.alias,
            },
            label: None,
        });
    }

    /// `# Uses` with one `- [label](path)` line per file, folder or glob (SPEC 23.4).
    fn uses(&mut self, h: &'l Line, body: &[&'l Line], file: &mut File) {
        let mut pos = 0;
        let nodes = self.build(body, &mut pos, 0);
        if nodes.is_empty() {
            self.error(
                E_USES,
                line_span(h),
                "\"# Uses\" lists the files this one uses, one link per line: \"- [constants](./constants.agent.md)\".",
            );
        }
        for n in &nodes {
            if n.line.kind != LineKind::Item || !n.line.text.trim_start().starts_with('[') {
                self.error(
                    E_USES,
                    line_span(n.line),
                    "A used file is a \"- \" line with a link: \"- [constants](./constants.agent.md)\".",
                );
                continue;
            }
            if let Some(k) = n.kids.first() {
                self.kid_error(k.line, "Nothing can be indented under a used file.");
            }
            let Some((label, target, alias)) =
                self.parse(|g| &g.uses_item, &n.line.text, n.line.line_no, n.line.col)
            else {
                continue;
            };
            let path = target.value.clone();
            let kind = if path.contains('*') {
                if let Some(a) = alias {
                    self.error(E_IMPORT, a.span, "A glob cannot be given a name with \"as\".");
                    continue;
                }
                ImportKind::Glob { pattern: path }
            } else if path.ends_with(".agent.md") {
                ImportKind::File { path, alias }
            } else {
                let folder = path.trim_end_matches('/').to_string();
                match alias {
                    Some(alias) => ImportKind::Folder { path: folder, alias },
                    None => {
                        self.error(
                            E_IMPORT,
                            target.span,
                            format!(
                                "\"{path}\" is not a .agent.md file. To use a folder, give it a name: \"- [{label}]({path}) as <name>\"."
                            ),
                        );
                        continue;
                    }
                }
            };
            file.imports.push(Import {
                span: line_span(n.line),
                doc: None,
                kind,
                label: Some(label),
            });
        }
    }

    /// `# Program: name` with its description and `- start:` (SPEC 23.4).
    fn program(&mut self, span: Span, doc: Option<String>, name: Ident, nodes: &[Node<'l>]) -> Option<Decl> {
        let props = self.props(nodes, &["start"], "a Program");
        for p in &props {
            self.no_kids(p);
        }
        let start = props.iter().find(|p| p.key.name == "start").and_then(|p| {
            self.value(|g| &g.start, p).map(|(label, target)| Start {
                span: Span::new(p.line, p.col, p.value.chars().count() as u32),
                label,
                target: target.value,
            })
        });
        Some(Decl::Program(Program { span, doc, name, start }))
    }

    // ------------------------------------------------------------------ indentation tree

    /// Build the tree of list items for `lines` starting at `pos`. Every line indented at least
    /// `min` spaces belongs to this level or below it: the first one sets the level, a deeper line
    /// belongs to the item above it (SPEC 23.3).
    fn build(&mut self, lines: &[&'l Line], pos: &mut usize, min: u32) -> Vec<Node<'l>> {
        let mut out = Vec::new();
        if min == 0 {
            self.pending.clear();
        }
        let mut level: Option<u32> = None;
        while *pos < lines.len() {
            let l = lines[*pos];
            if l.kind == LineKind::Comment {
                self.pending.push(l.text.clone());
                *pos += 1;
                continue;
            }
            if matches!(l.kind, LineKind::FenceBody | LineKind::FenceClose) {
                *pos += 1;
                continue;
            }
            if l.indent < min {
                break;
            }
            let is_fence = matches!(l.kind, LineKind::FenceOpen(_));
            let lv = *level.get_or_insert(l.indent);
            if l.indent != lv && !is_fence && !self.bad_lines.contains(&l.line_no) {
                self.bad_lines.insert(l.line_no);
                let (what, help) = if l.indent > lv {
                    ("deeper than", "Indent a line under the step or property it belongs to, by as many spaces as that line's text is indented.")
                } else {
                    ("less than", "Lines of one list start at the same column.")
                };
                self.error(
                    E_INDENT_DEEP,
                    Span::new(l.line_no, 1, l.indent.max(1)),
                    format!(
                        "This line is indented {} spaces, {what} the {} of the lines above it at this level.",
                        l.indent, lv
                    ),
                );
                if let Some(d) = self.diags.last_mut() {
                    d.help = Some(help.into());
                }
            }
            *pos += 1;
            if let LineKind::FenceOpen(lang) = &l.kind {
                let mut code = Vec::new();
                while *pos < lines.len() && lines[*pos].kind == LineKind::FenceBody {
                    code.push(lines[*pos].text.as_str());
                    *pos += 1;
                }
                if *pos < lines.len() && lines[*pos].kind == LineKind::FenceClose {
                    *pos += 1;
                }
                out.push(Node {
                    line: l,
                    comments: Vec::new(),
                    kids: Vec::new(),
                    fence: Some(Fence {
                        lang: lang.clone(),
                        code: code.join("\n"),
                        first_line: l.line_no + 1,
                    }),
                });
                continue;
            }
            let comments = std::mem::take(&mut self.pending);
            let kids = self.build(lines, pos, l.indent + 1);
            out.push(Node {
                line: l,
                comments,
                kids,
                fence: None,
            });
        }
        if min == 0 {
            self.pending.clear();
        }
        out
    }

    // ------------------------------------------------------------------ declarations

    fn declaration(
        &mut self,
        kind: DeclKind,
        span: Span,
        doc: Option<String>,
        name: Ident,
        body: &[&'l Line],
    ) -> Option<Decl> {
        // Split the body at level-2 headings.
        let first_h2 = body
            .iter()
            .position(|l| l.kind == LineKind::Heading2)
            .unwrap_or(body.len());
        let (prop_lines, block_lines) = body.split_at(first_h2);
        let mut pos = 0;
        let prop_nodes = self.build(prop_lines, &mut pos, 0);

        let mut blocks: Vec<(&'l Line, Vec<&'l Line>)> = Vec::new();
        for l in block_lines {
            if l.kind == LineKind::Heading2 {
                blocks.push((l, Vec::new()));
            } else if let Some((_, b)) = blocks.last_mut() {
                b.push(l);
            }
        }

        match kind {
            DeclKind::Record => {
                self.no_blocks(kind, &blocks);
                self.record(span, doc, name, &prop_nodes)
            }
            DeclKind::Enum => {
                self.no_blocks(kind, &blocks);
                self.enum_decl(span, doc, name, &prop_nodes)
            }
            DeclKind::TypeAlias => {
                self.no_blocks(kind, &blocks);
                self.type_alias(span, doc, name, &prop_nodes)
            }
            DeclKind::Rule => unreachable!("rules are parsed from their heading words"),
            DeclKind::Constant => {
                self.no_blocks(kind, &blocks);
                self.constant(span, doc, name, &prop_nodes)
            }
            DeclKind::Agent => self.agent(span, doc, name, &prop_nodes, blocks),
            DeclKind::Function => self.function(span, doc, name, &prop_nodes, blocks),
            DeclKind::Service => self.service(span, doc, name, &prop_nodes, blocks),
            DeclKind::Command => self.command(span, doc, name, &prop_nodes, blocks),
            DeclKind::Binding => self.binding(span, doc, name, &prop_nodes, blocks),
            DeclKind::Tool => self.tool(span, doc, name, &prop_nodes, blocks),
        }
    }

    fn no_blocks(&mut self, kind: DeclKind, blocks: &[(&'l Line, Vec<&'l Line>)]) {
        if let Some((l, _)) = blocks.first() {
            self.error(
                E_STRUCTURE,
                line_span(l),
                format!(
                    "{} {} has no \"##\" blocks.",
                    if kind.word().starts_with(['A', 'E', 'I', 'O', 'U']) { "An" } else { "A" },
                    kind.word()
                ),
            );
        }
    }

    /// Parse the `- key: value` lines of a declaration. Unknown and repeated keys are errors.
    fn props<'n>(
        &mut self,
        nodes: &'n [Node<'l>],
        allowed: &[&str],
        what: &str,
    ) -> Vec<Prop<'n, 'l>> {
        self.props_with(nodes, allowed, &[], what)
    }

    /// [`Self::props`] where the keys in `repeatable` may be given more than once.
    fn props_with<'n>(
        &mut self,
        nodes: &'n [Node<'l>],
        allowed: &[&str],
        repeatable: &[&str],
        what: &str,
    ) -> Vec<Prop<'n, 'l>> {
        let mut out: Vec<Prop<'n, 'l>> = Vec::new();
        for n in nodes {
            if n.fence.is_some() {
                continue;
            }
            if n.line.kind != LineKind::Item {
                let numbered = matches!(n.line.kind, LineKind::Numbered(_));
                self.error(
                    if numbered { E_PROP_NUMBER } else { E_STRUCTURE },
                    line_span(n.line),
                    if numbered {
                        format!("A property is a bullet, not a numbered step. Write: - {}", n.line.text)
                    } else {
                        "A property starts with \"- \", like \"- model: claude\".".to_string()
                    },
                );
                continue;
            }
            let Some((key, value, range)) =
                self.parse(|g| &g.prop_head, &n.line.text, n.line.line_no, n.line.col)
            else {
                continue;
            };
            if let Some(d) = removed_property(self.file, what, &key, value) {
                self.diags.push(d);
                self.rejected_prop = true;
                continue;
            }
            if !allowed.contains(&key.name.as_str()) {
                let d = Diagnostic::error(
                    E_UNKNOWN_PROP,
                    self.file,
                    key.span,
                    format!("Unknown property \"{}\" for {what}.", key.name),
                )
                .suggesting(&key.name, allowed.iter().copied())
                .with_help(format!(
                    "{} accepts: {}.",
                    capitalize(what),
                    allowed.join(", ")
                ));
                self.diags.push(d);
                self.rejected_prop = true;
                continue;
            }
            if !repeatable.contains(&key.name.as_str()) && out.iter().any(|p| p.key.name == key.name) {
                self.error(
                    E_DUP_PROP,
                    key.span,
                    format!("The property \"{}\" is given twice.", key.name),
                );
                self.rejected_prop = true;
                continue;
            }
            let value_col = n.line.col + n.line.text[..range.start].chars().count() as u32;
            out.push(Prop {
                key,
                comments: n.comments.clone(),
                value,
                line: n.line.line_no,
                col: value_col,
                node: n,
            });
        }
        out
    }

    fn missing(&mut self, kind: &str, name: &Ident, key: &str, example: &str) {
        if self.rejected_prop {
            return;
        }
        let d = Diagnostic::error(
            E_MISSING_PROP,
            self.file,
            name.span,
            format!("The {kind} \"{}\" has no \"{key}\" line.", name.name),
        )
        .with_help(format!("Add \"- {key}: {example}\" under the heading."));
        self.diags.push(d);
    }

    fn value<T>(&mut self, pick: Pick<'l, T>, p: &Prop<'_, 'l>) -> Option<T> {
        self.parse(pick, p.value, p.line, p.col)
    }

    /// A property whose value is one of a closed set of words (`effect`, `memory`): a near miss
    /// gets a suggestion.
    fn word_value<T>(
        &mut self,
        pick: Pick<'l, T>,
        p: &Prop<'_, 'l>,
        words: &[&str],
        what: &str,
    ) -> Option<T> {
        match run(pick(&self.g), p.value, p.line, p.col) {
            Ok(v) => Some(v),
            Err((ctx, errs)) => {
                let w = p.value.trim();
                let dym = did_you_mean(w, words.iter().copied());
                if dym.is_some() {
                    let span = Span::new(p.line, p.col, w.chars().count() as u32);
                    let d = Diagnostic::error(
                        E_SYNTAX,
                        self.file,
                        span,
                        format!("Unknown {what} \"{w}\"."),
                    )
                    .with_did_you_mean(dym);
                    self.diags.push(d);
                } else if !self.report_problems(&ctx) {
                    if let Some(e) = errs.first() {
                        self.diags.push(rich_diag(self.file, &ctx, e));
                    }
                }
                None
            }
        }
    }

    fn no_kids(&mut self, p: &Prop<'_, 'l>) {
        if let Some(k) = p.node.kids.first() {
            self.kid_error(
                k.line,
                format!("Nothing can be indented under \"{}:\".", p.key.name),
            );
        }
    }

    fn record(
        &mut self,
        span: Span,
        doc: Option<String>,
        name: Ident,
        nodes: &[Node<'l>],
    ) -> Option<Decl> {
        let mut fields = Vec::new();
        for n in nodes {
            if n.line.kind != LineKind::Item {
                self.error(
                    if matches!(n.line.kind, LineKind::Numbered(_)) { E_PROP_NUMBER } else { E_STRUCTURE },
                    line_span(n.line),
                    "A field is a bullet: \"- amount: money\".",
                );
                continue;
            }
            if let Some(k) = n.kids.first() {
                self.kid_error(k.line, "Nothing can be indented under a field.");
            }
            if let Some((fname, ty)) =
                self.parse(|g| &g.field, &n.line.text, n.line.line_no, n.line.col)
            {
                fields.push(Field {
                    span: line_span(n.line),
                    comments: n.comments.clone(),
                    name: fname,
                    ty,
                });
            }
        }
        Some(Decl::Record(Record {
            span,
            doc,
            name,
            fields,
        }))
    }

    /// `# Enum: Shape`: one `- Variant` or `- Variant: field: type, …` line per variant.
    fn enum_decl(
        &mut self,
        span: Span,
        doc: Option<String>,
        name: Ident,
        nodes: &[Node<'l>],
    ) -> Option<Decl> {
        let mut variants = Vec::new();
        for n in nodes {
            if n.line.kind != LineKind::Item {
                self.error(
                    if matches!(n.line.kind, LineKind::Numbered(_)) { E_PROP_NUMBER } else { E_STRUCTURE },
                    line_span(n.line),
                    "A variant is a bullet: \"- Circle: radius: int\" or \"- Empty\".",
                );
                continue;
            }
            if let Some(k) = n.kids.first() {
                self.kid_error(k.line, "Nothing can be indented under a variant.");
            }
            if let Some((vname, fields)) =
                self.parse(|g| &g.variant, &n.line.text, n.line.line_no, n.line.col)
            {
                let fields = fields
                    .into_iter()
                    .map(|(fname, ty)| Field {
                        span: fname.span.to(ty.span),
                        comments: Vec::new(),
                        name: fname,
                        ty: Some(ty),
                    })
                    .collect();
                variants.push(Variant {
                    span: line_span(n.line),
                    comments: n.comments.clone(),
                    name: vname,
                    fields,
                });
            }
        }
        Some(Decl::Enum(Enum {
            span,
            doc,
            name,
            variants,
        }))
    }

    /// `# Type: Totals` with `- is: <type>`.
    fn type_alias(
        &mut self,
        span: Span,
        doc: Option<String>,
        name: Ident,
        nodes: &[Node<'l>],
    ) -> Option<Decl> {
        let props = self.props(nodes, &["is"], "a Type");
        for p in &props {
            self.no_kids(p);
        }
        let Some(p) = props.iter().find(|p| p.key.name == "is") else {
            self.missing("type", &name, "is", "list of text");
            return None;
        };
        let target = self.value(|g| &g.ty, p)?;
        Some(Decl::TypeAlias(TypeAlias {
            span,
            doc,
            prop_comments: prop_comments(&props),
            name,
            target,
        }))
    }

    /// `# Rule: big Sale`: the last word is the record, the words before it the name (SPEC 23.8).
    fn rule(&mut self, span: Span, doc: Option<String>, words: Vec<Ident>, body: &[&'l Line]) -> Option<Decl> {
        let first_h2 = body.iter().position(|l| l.kind == LineKind::Heading2).unwrap_or(body.len());
        let (prop_lines, block_lines) = body.split_at(first_h2);
        if let Some(l) = block_lines.first() {
            self.error(E_STRUCTURE, line_span(l), "A Rule has no \"##\" blocks.");
        }
        let mut pos = 0;
        let nodes = self.build(prop_lines, &mut pos, 0);
        let (name, for_record) = match words.as_slice() {
            [] => return None,
            [only] => (only.clone(), None),
            [names @ .., record] => {
                let span = names[0].span.to(names[names.len() - 1].span);
                let name = names.iter().map(|w| w.name.as_str()).collect::<Vec<_>>().join("_");
                (Ident { name, span }, Some(record.clone()))
            }
        };
        let props = self.props(&nodes, &["when"], "a Rule");
        let when_p = props.iter().find(|p| p.key.name == "when");
        let mut ok = true;
        if for_record.is_none() && !self.rejected_prop {
            let d = Diagnostic::error(
                E_RULE_FOR,
                self.file,
                name.span,
                format!("The rule \"{}\" does not say which record it is about.", name.name),
            )
            .with_help(format!("Put the record last in the heading: \"# Rule: {} Invoice\".", name.name));
            self.diags.push(d);
            ok = false;
        }
        if when_p.is_none() {
            self.missing("rule", &name, "when", "status != \"paid\"");
            ok = false;
        }
        let when = when_p.and_then(|p| self.value(|g| &g.expr, p));
        for p in &props {
            self.no_kids(p);
        }
        match (ok, for_record, when) {
            (true, Some(for_record), Some(when)) => Some(Decl::Rule(Rule {
                span,
                doc,
                prop_comments: prop_comments(&props),
                name,
                for_record,
                when,
            })),
            _ => None,
        }
    }

    fn constant(
        &mut self,
        span: Span,
        doc: Option<String>,
        name: Ident,
        nodes: &[Node<'l>],
    ) -> Option<Decl> {
        let props = self.props(nodes, &["value"], "a Constant");
        let Some(p) = props.iter().find(|p| p.key.name == "value") else {
            self.missing("constant", &name, "value", "14 days");
            return None;
        };
        self.no_kids(p);
        let value = self.value(|g| &g.expr, p)?;
        Some(Decl::Constant(Constant {
            span,
            doc,
            prop_comments: prop_comments(&props),
            name,
            value,
        }))
    }

    fn agent(
        &mut self,
        span: Span,
        doc: Option<String>,
        name: Ident,
        nodes: &[Node<'l>],
        blocks: Vec<(&'l Line, Vec<&'l Line>)>,
    ) -> Option<Decl> {
        let props = self.props(nodes, &["model", "tools", "memory", "timeout"], "an Agent");
        for p in &props {
            self.no_kids(p);
        }
        let model_p = props.iter().find(|p| p.key.name == "model");
        let model = match model_p {
            Some(p) => self.value(|g| &g.model, p),
            None => {
                self.missing("agent", &name, "model", "claude");
                None
            }
        };
        let tools = match props.iter().find(|p| p.key.name == "tools") {
            Some(p) => self.value(|g| &g.name_list, p).unwrap_or_default(),
            None => Vec::new(),
        };
        let memory = props.iter().find(|p| p.key.name == "memory").and_then(|p| {
            self.word_value(|g| &g.memory, p, &["none", "file"], "memory setting")
                .map(|value| Spanned {
                    value,
                    span: Span::new(p.line, p.col, p.value.chars().count() as u32),
                })
        });
        let timeout = props
            .iter()
            .find(|p| p.key.name == "timeout")
            .and_then(|p| {
                self.value(|g| &g.duration, p).map(|value| Spanned {
                    value,
                    span: Span::new(p.line, p.col, p.value.chars().count() as u32),
                })
            });

        let mut out_blocks = Vec::new();
        for (h2, lines) in blocks {
            let word = self.first_word(h2);
            if word
                .as_deref()
                .is_some_and(|w| w.eq_ignore_ascii_case("rust"))
            {
                self.rust_block(h2, &lines, Some(name.clone()));
                continue;
            }
            if let Some(w) = &word {
                if !BLOCK_WORDS.iter().any(|k| k.eq_ignore_ascii_case(w)) {
                    let span = Span::new(h2.line_no, h2.col, w.chars().count() as u32);
                    let d = Diagnostic::error(
                        E_SYNTAX,
                        self.file,
                        span,
                        format!("Unknown block heading \"{w}\"."),
                    )
                    .with_did_you_mean(did_you_mean(&capitalize(w), BLOCK_WORDS.iter().copied()))
                    .with_help(format!(
                        "A block starts with one of: {}.",
                        BLOCK_WORDS.join(", ")
                    ));
                    self.diags.push(d);
                    continue;
                }
            }
            let Some(trigger) = self.parse(|g| &g.trigger, &h2.text, h2.line_no, h2.col) else {
                continue;
            };
            let mut pos = 0;
            let nodes = self.build(&lines, &mut pos, 0);
            let body = self.stmts(&nodes);
            out_blocks.push(Block {
                span: line_span(h2),
                doc: self.docs.get(&h2.line_no).cloned(),
                trigger,
                label: h2.text.clone(),
                body,
            });
        }
        Some(Decl::Agent(Agent {
            span,
            doc,
            prop_comments: prop_comments(&props),
            name,
            model: model?,
            tools,
            memory,
            timeout,
            blocks: out_blocks,
        }))
    }

    /// `- touches:` as written (SPEC 23.7).
    fn touches(&mut self, p: &Prop<'_, 'l>) -> Option<Touches> {
        let words = self.value(|g| &g.name_list, p)?;
        Some(Touches {
            span: Span::new(p.line, p.col, p.value.trim_end().chars().count() as u32),
            words,
        })
    }

    /// The numbered steps under `- steps:` (SPEC 23.3, 23.5).
    fn steps_of(&mut self, p: &Prop<'_, 'l>) -> Vec<Stmt> {
        if !p.value.trim().is_empty() {
            self.error(
                E_STRUCTURE,
                Span::new(p.line, p.col, p.value.chars().count() as u32),
                "The steps go on the lines under \"- steps:\", as a numbered list.",
            );
        }
        if p.node.kids.is_empty() {
            self.error(
                E_STRUCTURE,
                Span::new(p.line, p.col.saturating_sub(7).max(1), 6),
                "\"- steps:\" has no steps under it. Write them as a numbered list, for example: 1. print: \"hello\"",
            );
        }
        self.stmts(&p.node.kids)
    }

    /// A property other than `- steps:` with lines indented under it.
    fn no_steps_here(&mut self, p: &Prop<'_, 'l>) {
        if let Some(k) = p.node.kids.first() {
            self.kid_error(
                k.line,
                format!("Steps go under \"- steps:\", not under \"{}:\".", p.key.name),
            );
        }
    }

    fn function(
        &mut self,
        span: Span,
        doc: Option<String>,
        name: Ident,
        nodes: &[Node<'l>],
        blocks: Vec<(&'l Line, Vec<&'l Line>)>,
    ) -> Option<Decl> {
        let props = self.props(nodes, &["in", "out", "touches", "steps"], "a Function");
        let params = match props.iter().find(|p| p.key.name == "in") {
            Some(p) => self.value(|g| &g.in_params, p).unwrap_or_default(),
            None => Vec::new(),
        };
        let out = props
            .iter()
            .find(|p| p.key.name == "out")
            .and_then(|p| self.value(|g| &g.out_type, p));
        let touches = props
            .iter()
            .find(|p| p.key.name == "touches")
            .and_then(|p| self.touches(p));
        let mut body = None;
        for p in &props {
            if p.key.name == "steps" {
                body = Some(self.steps_of(p));
            } else {
                self.no_steps_here(p);
            }
        }
        for (h2, lines) in blocks {
            if self
                .first_word(h2)
                .as_deref()
                .is_some_and(|w| w.eq_ignore_ascii_case("rust"))
            {
                self.rust_block(h2, &lines, Some(name.clone()));
            } else {
                self.error(
                    E_STRUCTURE,
                    line_span(h2),
                    "A Function has no blocks except \"## Rust: name\"; its steps go under \"- steps:\".",
                );
            }
        }
        if body.is_none() && !self.rejected_prop {
            let d = Diagnostic::error(
                E_MISSING_PROP,
                self.file,
                name.span,
                format!("The function \"{}\" has no steps.", name.name),
            )
            .with_help("Add \"- steps:\" with the steps under it as a numbered list.");
            self.diags.push(d);
        }
        Some(Decl::Function(Function {
            span,
            doc,
            prop_comments: prop_comments(&props),
            name,
            params,
            out,
            touches,
            body: body.unwrap_or_default(),
        }))
    }

    fn service(
        &mut self,
        span: Span,
        doc: Option<String>,
        name: Ident,
        nodes: &[Node<'l>],
        blocks: Vec<(&'l Line, Vec<&'l Line>)>,
    ) -> Option<Decl> {
        let props = self.props(nodes, &["listen", "state"], "a Service");
        for p in &props {
            self.no_kids(p);
        }
        let listen = props
            .iter()
            .find(|p| p.key.name == "listen")
            .and_then(|p| self.value(|g| &g.rest, p));
        let state = props
            .iter()
            .find(|p| p.key.name == "state")
            .and_then(|p| self.value(|g| &g.single_name, p));
        let mut routes = Vec::new();
        for (h2, lines) in blocks {
            if self
                .first_word(h2)
                .as_deref()
                .is_some_and(|w| w.eq_ignore_ascii_case("rust"))
            {
                self.rust_block(h2, &lines, Some(name.clone()));
                continue;
            }
            let Some((method, path, path_params)) =
                self.parse(|g| &g.route, &h2.text, h2.line_no, h2.col)
            else {
                continue;
            };
            let mut pos = 0;
            let nodes = self.build(&lines, &mut pos, 0);
            let body = self.stmts(&nodes);
            routes.push(Route {
                span: line_span(h2),
                doc: self.docs.get(&h2.line_no).cloned(),
                method,
                path,
                path_params,
                body,
            });
        }
        Some(Decl::Service(Service {
            span,
            doc,
            prop_comments: prop_comments(&props),
            name,
            listen,
            state,
            routes,
        }))
    }

    fn command(
        &mut self,
        span: Span,
        doc: Option<String>,
        name: Ident,
        nodes: &[Node<'l>],
        blocks: Vec<(&'l Line, Vec<&'l Line>)>,
    ) -> Option<Decl> {
        let props = self.props(nodes, &["in", "steps"], "a Command");
        let args = match props.iter().find(|p| p.key.name == "in") {
            Some(p) => self.value(|g| &g.in_params, p).unwrap_or_default(),
            None => Vec::new(),
        };
        let mut body = None;
        for p in &props {
            if p.key.name == "steps" {
                body = Some(self.steps_of(p));
            } else {
                self.no_steps_here(p);
            }
        }
        let mut old_run = false;
        for (h2, lines) in blocks {
            let word = self.first_word(h2);
            if word
                .as_deref()
                .is_some_and(|w| w.eq_ignore_ascii_case("rust"))
            {
                self.rust_block(h2, &lines, Some(name.clone()));
            } else if word
                .as_deref()
                .is_some_and(|w| w.eq_ignore_ascii_case("run"))
            {
                old_run = true;
                let d = Diagnostic::error(
                    E_RUN_BLOCK,
                    self.file,
                    line_span(h2),
                    "\"## Run\" was removed. Write the command's steps under \"- steps:\" as a numbered list.",
                )
                .with_help("Write:\n- steps:\n  1. print: \"hello\"");
                self.diags.push(d);
            } else {
                self.error(
                    E_STRUCTURE,
                    line_span(h2),
                    "A Command has no blocks except \"## Rust: name\"; its steps go under \"- steps:\".",
                );
            }
        }
        if body.is_none() && !old_run && !self.rejected_prop {
            let d = Diagnostic::error(
                E_MISSING_PROP,
                self.file,
                name.span,
                format!("The command \"{}\" has no steps.", name.name),
            )
            .with_help("Add \"- steps:\" with the steps under it as a numbered list.");
            self.diags.push(d);
        }
        Some(Decl::Command(Command {
            span,
            doc,
            prop_comments: prop_comments(&props),
            name,
            args,
            body: body?,
        }))
    }

    fn binding(
        &mut self,
        span: Span,
        doc: Option<String>,
        name: Ident,
        nodes: &[Node<'l>],
        blocks: Vec<(&'l Line, Vec<&'l Line>)>,
    ) -> Option<Decl> {
        let props = self.props(nodes, &["crate", "effect", "proposed-effect"], "a Binding");
        for p in &props {
            self.no_kids(p);
        }
        let crate_name = match props.iter().find(|p| p.key.name == "crate") {
            Some(p) => self.value(|g| &g.single_name, p),
            None => {
                self.missing("binding", &name, "crate", "printpdf");
                None
            }
        };
        let eff = |this: &mut Self, key: &str| {
            props.iter().find(|p| p.key.name == key).and_then(|p| {
                this.value(|g| &g.effect, p).map(|value| Spanned {
                    value,
                    span: Span::new(p.line, p.col, p.value.chars().count() as u32),
                })
            })
        };
        let effect = if props.iter().any(|p| p.key.name == "effect") {
            eff(self, "effect")
        } else {
            self.missing("binding", &name, "effect", "unverified");
            None
        };
        let proposed_effect = eff(self, "proposed-effect");

        let mut items = Vec::new();
        let mut handles = Vec::new();
        let mut not_bound = Vec::new();
        for (h2, lines) in blocks {
            let mut pos = 0;
            let nodes = self.build(&lines, &mut pos, 0);
            if let Some(hname) = h2.text.strip_prefix("Handle:").filter(|_| !h2.text.contains('(')) {
                let hname = hname.trim().to_string();
                let ok = !hname.is_empty()
                    && hname.split('.').all(|seg| {
                        !seg.is_empty() && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    });
                if !ok {
                    self.error(
                        E_STRUCTURE,
                        line_span(h2),
                        "A handle block is written \"## Handle: <crate>.<Type>\", for example \"## Handle: sqlx.Query\".",
                    );
                    continue;
                }
                if let Some(rust) = self.fence_in(&nodes, h2, &hname) {
                    handles.push(BindingHandle { span: line_span(h2), name: hname, rust });
                }
                continue;
            }
            if self.parse_ok(|g| &g.not_bound, h2) {
                for n in &nodes {
                    if n.line.kind != LineKind::Item {
                        continue;
                    }
                    if let Some((item, reason)) = self.parse(
                        |g| &g.not_bound_item,
                        &n.line.text,
                        n.line.line_no,
                        n.line.col,
                    ) {
                        not_bound.push(NotBound {
                            span: line_span(n.line),
                            item,
                            reason,
                        });
                    }
                }
                // `  > ` lines directly under an entry continue its reason (Q24, Q110)
                let mut last: Option<usize> = None;
                let mut pieces: Vec<Vec<&str>> = vec![Vec::new(); not_bound.len()];
                for l in &lines {
                    match l.kind {
                        LineKind::Item => {
                            last = not_bound.iter().position(|n| n.span.line == l.line_no);
                        }
                        LineKind::Comment if l.indent >= 1 => {
                            if let (Some(i), false) = (last, l.text.is_empty()) {
                                pieces[i].push(&l.text);
                            }
                        }
                        _ => last = None,
                    }
                }
                // the writer's line wrapping, undone (Q202)
                for (n, p) in not_bound.iter_mut().zip(&pieces) {
                    if !p.is_empty() {
                        n.reason = Some(join_wrapped(n.reason.as_deref().unwrap_or(""), p));
                    }
                }
                continue;
            }
            let Some((iname, params, out)) = (if h2.text.contains('(') {
                self.parse(|g| &g.binding_item, &h2.text, h2.line_no, h2.col)
            } else {
                self.wrapped_item(h2, &nodes)
            }) else {
                continue;
            };
            let Some(rust) = self.fence_in(&nodes, h2, &iname.name) else {
                continue;
            };
            items.push(BindingItem {
                span: line_span(h2),
                doc: self.docs.get(&h2.line_no).cloned(),
                name: iname,
                params,
                out,
                rust,
            });
        }
        Some(Decl::Binding(Binding {
            span,
            doc,
            prop_comments: prop_comments(&props),
            name,
            crate_name: crate_name?,
            effect: effect?,
            proposed_effect,
            items,
            handles,
            not_bound,
        }))
    }

    /// A binding item whose signature does not fit on one line (Q416): `## name`, then `- in:` lines
    /// (they add up) and `- out:`, the words of a `# Function`, then the fenced code.
    fn wrapped_item(&mut self, h2: &'l Line, nodes: &[Node<'l>]) -> Option<(Ident, Vec<Param>, Option<TypeExpr>)> {
        let name = self.parse(|g| &g.single_name, &h2.text, h2.line_no, h2.col)?;
        let props = self.props_with(nodes, &["in", "out"], &["in"], "a binding item");
        let mut params = Vec::new();
        for p in props.iter().filter(|p| p.key.name == "in") {
            params.extend(self.value(|g| &g.params, p).unwrap_or_default());
        }
        let out = props.iter().find(|p| p.key.name == "out").and_then(|p| self.value(|g| &g.ty, p));
        Some((name, params, out))
    }

    /// Does the heading match `pick` (without reporting a failure)?
    fn parse_ok(&self, pick: Pick<'l, ()>, h2: &'l Line) -> bool {
        run(pick(&self.g), &h2.text, h2.line_no, h2.col).is_ok()
    }

    fn first_word(&self, h2: &'l Line) -> Option<String> {
        run(&self.g.h1_word, &h2.text, h2.line_no, h2.col)
            .ok()
            .map(|i| i.name)
    }

    // ------------------------------------------------------------------ Rust blocks

    /// The verbatim code of the first fence in `nodes` (anywhere in the tree).
    fn fence_in(&mut self, nodes: &[Node<'l>], h2: &Line, what: &str) -> Option<RustCode> {
        fn find<'a, 'l>(nodes: &'a [Node<'l>]) -> Option<&'a Node<'l>> {
            for n in nodes {
                if n.fence.is_some() {
                    return Some(n);
                }
                if let Some(f) = find(&n.kids) {
                    return Some(f);
                }
            }
            None
        }
        let Some(node) = find(nodes) else {
            self.error(
                E_STRUCTURE,
                line_span(h2),
                format!("\"{what}\" needs its Rust code in a fenced block (```rust ... ```)."),
            );
            return None;
        };
        let f = node.fence.as_ref()?;
        if f.lang != "rust" {
            self.error(
                E_STRUCTURE,
                line_span(node.line),
                "Only ```rust code fences are supported here.",
            );
            return None;
        }
        Some(RustCode {
            code: f.code.clone(),
            line: f.first_line,
        })
    }

    fn rust_block(&mut self, h2: &'l Line, lines: &[&'l Line], parent: Option<Ident>) {
        let Some((name, out)) = self.parse(|g| &g.rust_heading, &h2.text, h2.line_no, h2.col)
        else {
            return;
        };
        let mut pos = 0;
        let nodes = self.build(lines, &mut pos, 0);
        self.rejected_prop = false;
        let props = self.props(&nodes, &["in", "touches"], "a Rust block");
        let params = match props.iter().find(|p| p.key.name == "in") {
            Some(p) => self.value(|g| &g.params, p).unwrap_or_default(),
            None => Vec::new(),
        };
        let touches = match props.iter().find(|p| p.key.name == "touches") {
            Some(p) => self.touches(p),
            None => {
                if !self.rejected_prop {
                    let d = Diagnostic::error(
                        E_MISSING_PROP,
                        self.file,
                        name.span,
                        format!("The Rust block \"{}\" does not say what it touches.", name.name),
                    )
                    .with_help("Add \"- touches: nothing\" (or files, print, AI, or the tools it uses). AgentMark cannot read Rust, so it trusts this line.");
                    self.diags.push(d);
                }
                None
            }
        };
        let rust = self.fence_in(&nodes, h2, &name.name);
        if let (Some(touches), Some(rust)) = (touches, rust) {
            self.hoisted.push(Decl::RustBlock(RustBlock {
                span: line_span(h2),
                doc: self.docs.get(&h2.line_no).cloned(),
                prop_comments: prop_comments(&props),
                name,
                parent,
                params,
                out,
                touches,
                rust,
            }));
        }
    }

    // ------------------------------------------------------------------ statements

    /// The statement each step line holds. A step is a numbered item; `else:` and `otherwise:`
    /// are plain lines inside a step (SPEC 23.3).
    fn heads(&mut self, nodes: &[Node<'l>]) -> Vec<Option<Head>> {
        let mut out = Vec::new();
        for n in nodes {
            let head = match &n.line.kind {
                LineKind::Numbered(_) | LineKind::Control | LineKind::Item => {
                    if let Some(d) = removed_statement(self.file, n.line) {
                        self.diags.push(d);
                        None
                    } else {
                        match run(&self.g.head, &n.line.text, n.line.line_no, n.line.col) {
                            Ok(h) => {
                                let bare = matches!(h, Head::Else | Head::Otherwise);
                                if n.line.kind == LineKind::Item && !bare {
                                    self.error(
                                        E_STEP_BULLET,
                                        line_span(n.line),
                                        format!("Steps are a numbered list. Write: 1. {}", n.line.text),
                                    );
                                    None
                                } else if n.line.kind == LineKind::Control && !bare {
                                    self.error(
                                        E_STEP_BULLET,
                                        line_span(n.line),
                                        format!("Steps are a numbered list. Write: 1. {}", n.line.text),
                                    );
                                    None
                                } else {
                                    Some(h)
                                }
                            }
                            Err((ctx, errs)) => {
                                self.statement_error(&ctx, &errs);
                                None
                            }
                        }
                    }
                }
                _ => {
                    let msg = if n.fence.is_some() {
                        "Code fences belong in a \"## Rust: name\" block, not among steps."
                    } else {
                        "This is not a step."
                    };
                    self.error(E_STRUCTURE, line_span(n.line), msg);
                    None
                }
            };
            out.push(head);
        }
        out
    }

    /// The branches of a `match`: `- Circle c:` bullets with numbered steps under them, and at
    /// most one `- otherwise:` at the end.
    fn match_arms(&mut self, nodes: &[Node<'l>], at: Span) -> (Vec<MatchArm>, Option<Otherwise>) {
        use stmt::Arm;
        let mut arms: Vec<MatchArm> = Vec::new();
        let mut otherwise: Option<Otherwise> = None;
        if nodes.is_empty() {
            self.error(
                E_STRUCTURE,
                at,
                "A \"match\" needs branches indented under it, like \"- Circle c:\" or \"- otherwise:\".",
            );
        }
        for n in nodes {
            if !matches!(n.line.kind, LineKind::Item | LineKind::Numbered(_) | LineKind::Control) {
                self.error(
                    E_STRUCTURE,
                    line_span(n.line),
                    "A branch of \"match\" starts with \"- \", like \"- Circle c:\".",
                );
                continue;
            }
            let parsed = match run(&self.g.arm, &n.line.text, n.line.line_no, n.line.col) {
                Ok(a) => a,
                Err((ctx, errs)) => {
                    if !self.report_problems(&ctx) {
                        if let Some(e) = errs.first() {
                            self.diags.push(rich_diag(self.file, &ctx, e));
                        }
                    }
                    continue;
                }
            };
            if n.line.kind != LineKind::Item {
                self.error(
                    E_STRUCTURE,
                    line_span(n.line),
                    format!("The branches of a \"match\" are bullets (their order does not matter). Write: - {}", n.line.text),
                );
            }
            let span = line_span(n.line);
            match parsed {
                Arm::Otherwise => {
                    if otherwise.is_some() {
                        self.error(E_STRUCTURE, span, "A \"match\" has only one \"otherwise:\" branch.");
                        continue;
                    }
                    let body = self.stmts(&n.kids);
                    otherwise = Some(Otherwise {
                        span,
                        comments: n.comments.clone(),
                        body,
                    });
                }
                Arm::Branch {
                    qualifier,
                    pattern,
                    bind,
                } => {
                    if otherwise.is_some() {
                        self.error(
                            E_STRUCTURE,
                            span,
                            "\"otherwise:\" must be the last branch of a \"match\".",
                        );
                    }
                    let body = self.stmts(&n.kids);
                    arms.push(MatchArm {
                        span,
                        comments: n.comments.clone(),
                        qualifier,
                        pattern,
                        bind,
                        body,
                    });
                }
            }
        }
        (arms, otherwise)
    }

    /// The steps under a step that has an `else:` or `otherwise:` line among them: the steps
    /// before that line and the steps after it (SPEC 23.3). `word` is `else` or `otherwise`.
    fn split_at_word(&mut self, kids: &[Node<'l>], heads: &[Option<Head>], word: &str) -> (Vec<Stmt>, Option<Vec<Stmt>>) {
        let at = heads.iter().position(|h| match h {
            Some(Head::Else) => word == "else",
            Some(Head::Otherwise) => word == "otherwise",
            _ => false,
        });
        let Some(k) = at else {
            return (self.stmts_with(kids, heads), None);
        };
        let before = self.stmts_with(&kids[..k], &heads[..k]);
        // the old shape, steps indented under the word itself, is read too (`fmt` rewrites it)
        let mut after = self.stmts(&kids[k].kids);
        after.extend(self.stmts_with(&kids[k + 1..], &heads[k + 1..]));
        (before, Some(after))
    }

    fn stmts(&mut self, nodes: &[Node<'l>]) -> Vec<Stmt> {
        let heads = self.heads(nodes);
        self.stmts_with(nodes, &heads)
    }

    fn stmts_with(&mut self, nodes: &[Node<'l>], heads: &[Option<Head>]) -> Vec<Stmt> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < nodes.len() {
            let node = &nodes[i];
            let span = line_span(node.line);
            let before = out.len();
            let Some(head) = &heads[i] else {
                i += 1;
                continue;
            };
            match head {
                Head::ForEach {
                    filter_rule,
                    var,
                    source,
                } => {
                    let body = self.stmts(&node.kids);
                    out.push(Stmt {
                        span,
                        comments: Vec::new(),
                        kind: StmtKind::ForEach {
                            filter_rule: filter_rule.clone(),
                            var: var.clone(),
                            source: source.clone(),
                            body,
                        },
                    });
                }
                Head::If { .. } | Head::IfSome { .. } => {
                    let kid_heads = self.heads(&node.kids);
                    let (then, inner_else) = self.split_at_word(&node.kids, &kid_heads, "else");
                    let mut else_ = inner_else.unwrap_or_default();
                    // an `else:` step right after the `if` step
                    if matches!(heads.get(i + 1), Some(Some(Head::Else))) {
                        else_.extend(self.stmts(&nodes[i + 1].kids));
                        i += 1;
                    }
                    let kind = match head {
                        Head::If { cond } => StmtKind::If {
                            cond: cond.clone(),
                            then,
                            else_,
                        },
                        Head::IfSome { expr, bind } => StmtKind::IfSome {
                            expr: expr.clone(),
                            bind: bind.clone(),
                            then,
                            else_,
                        },
                        _ => unreachable!(),
                    };
                    out.push(Stmt {
                        span,
                        comments: Vec::new(),
                        kind,
                    });
                }
                Head::Else => self.error(
                    E_STRUCTURE,
                    span,
                    "\"else:\" belongs inside an \"if\" step, after its steps.",
                ),
                Head::Otherwise => self.error(
                    E_STRUCTURE,
                    span,
                    "\"otherwise:\" belongs inside a \"try\" step.",
                ),
                Head::Match { scrutinee } => {
                    let (arms, otherwise) = self.match_arms(&node.kids, span);
                    out.push(Stmt {
                        span,
                        comments: Vec::new(),
                        kind: StmtKind::Match {
                            scrutinee: scrutinee.clone(),
                            arms,
                            otherwise,
                        },
                    });
                }
                Head::Parallel => {
                    let body = self.stmts(&node.kids);
                    out.push(Stmt {
                        span,
                        comments: Vec::new(),
                        kind: StmtKind::Parallel(body),
                    });
                }
                Head::Loop(l) => {
                    use crate::syntax::collections::CollStmt;
                    let body = self.stmts(&node.kids);
                    let kind = match l.clone() {
                        collections::LoopHead::Index { var, index, source } => {
                            CollStmt::ForEachIndex { var, index, source, body }
                        }
                        collections::LoopHead::Entry { key, value, source } => {
                            CollStmt::ForEachEntry { key, value, source, body }
                        }
                    };
                    out.push(Stmt {
                        span,
                        comments: Vec::new(),
                        kind: StmtKind::Coll(kind),
                    });
                }
                Head::AddFields { qualifier, name, list } => {
                    let fields = self.add_fields(node, name);
                    let value = Expr {
                        span: name.span,
                        kind: ExprKind::Variant {
                            qualifier: qualifier.clone(),
                            name: name.clone(),
                            fields,
                        },
                    };
                    out.push(Stmt {
                        span,
                        comments: Vec::new(),
                        kind: StmtKind::Append { value, list: list.clone() },
                    });
                }
                Head::Simple(s) => {
                    if let Some(k) = node.kids.first() {
                        self.kid_error(k.line, "Nothing can be indented under this step.");
                    }
                    out.push(s.clone());
                }
                Head::Try(inner) => {
                    let kid_heads = self.heads(&node.kids);
                    let (before, otherwise) = self.split_at_word(&node.kids, &kid_heads, "otherwise");
                    if let Some(first) = before.first() {
                        self.error(
                            E_STRUCTURE,
                            first.span,
                            "Only \"otherwise:\" and its steps can be under a \"try\" step.",
                        );
                    }
                    out.push(Stmt {
                        span,
                        comments: Vec::new(),
                        kind: StmtKind::Try {
                            stmt: Box::new(inner.clone()),
                            otherwise,
                        },
                    });
                }
            }
            if out.len() > before {
                out[before].comments = node.comments.clone();
            }
            i += 1;
        }
        out
    }

    /// The bullets under `add <Record> to <list>:`: `- region`, `- orders: count of rows`.
    fn add_fields(&mut self, node: &Node<'l>, name: &Ident) -> Vec<RecordField> {
        let mut fields = Vec::new();
        if node.kids.is_empty() {
            self.error(
                E_STRUCTURE,
                line_span(node.line),
                format!("\"add {} to …:\" needs the fields as bullets under it, like \"- region\" or \"- orders: count of rows\".", name.name),
            );
        }
        for k in &node.kids {
            if k.line.kind != LineKind::Item {
                self.error(
                    E_STRUCTURE,
                    line_span(k.line),
                    format!("A field of the new {} is a bullet: \"- name: value\" or \"- name\".", name.name),
                );
                continue;
            }
            if let Some(g) = k.kids.first() {
                self.kid_error(g.line, "Nothing can be indented under a field.");
            }
            let Some((fname, value)) = self.parse(|g| &g.add_field, &k.line.text, k.line.line_no, k.line.col) else {
                continue;
            };
            let value = value.unwrap_or_else(|| Expr {
                span: fname.span,
                kind: ExprKind::Path(vec![fname.clone()]),
            });
            fields.push(RecordField { name: fname, value });
        }
        fields
    }
}

/// A property that v0.4 removed (SPEC 23.13), with the line to write instead.
fn removed_property(file: &str, what: &str, key: &Ident, value: &str) -> Option<Diagnostic> {
    let value = value.trim();
    let (code, msg, help) = match (what, key.name.as_str()) {
        ("a Command", "args") => (
            E_ARGS,
            format!("\"args:\" was renamed. Write: - in: {value}"),
            "A command's inputs are written like a function's.".to_string(),
        ),
        ("a Function" | "a Tool" | "a Rust block", "effect") => {
            let write = if value == "pure" { "nothing" } else { "<what it touches>" };
            (
                E_EFFECT,
                format!("\"effect:\" was removed. Write: - touches: {write}"),
                if what == "a Rust block" {
                    "List what the code touches: nothing, files, print, AI, or the tools it uses (crm, email).".to_string()
                } else {
                    "List what it touches: nothing, files, print, AI, or the tools it uses (crm, email). The line may also be left out: the checker works it out.".to_string()
                },
            )
        }
        ("a Rule", "for") => (
            E_RULE_FOR,
            format!("\"for:\" was removed. Put the record last in the heading: # Rule: <name> {value}"),
            "A rule reads like its use: \"# Rule: overdue Invoice\" is used as \"for each overdue invoice in …\".".to_string(),
        ),
        _ => return None,
    };
    Some(Diagnostic::error(code, file, key.span, msg).with_help(help))
}

/// A step written in a form v0.4 removed (SPEC 23.13): `set`, `->`, `append`.
fn removed_statement(file: &str, l: &Line) -> Option<Diagnostic> {
    let text = l.text.as_str();
    let word_span = |w: &str| Span::new(l.line_no, l.col, w.chars().count() as u32);
    if let Some(rest) = text.strip_prefix("set ") {
        let write = match rest.split_once('=') {
            Some((name, value)) if !name.trim().is_empty() && !value.trim().is_empty() => {
                format!("{} into {}", value.trim(), name.trim())
            }
            _ => "<value> into <name>".to_string(),
        };
        return Some(
            Diagnostic::error(E_SET, file, word_span("set"), format!("\"set\" was removed. Write: {write}"))
                .with_help("A step names its result with \"into\" at its end."),
        );
    }
    if let Some(rest) = text.strip_prefix("append ") {
        let write = match rest.rsplit_once(" to ") {
            Some((value, list)) if value.contains('{') => {
                let name = value.split('{').next().unwrap_or("").trim();
                format!("add {name} to {}: with one bullet per field under it", list.trim())
            }
            _ => format!("add {rest}"),
        };
        return Some(
            Diagnostic::error(E_APPEND, file, word_span("append"), format!("\"append\" was renamed. Write: {write}")),
        );
    }
    if let Some(at) = arrow_outside_quotes(text) {
        let before = text[..at].trim_end();
        let after = text[at + 2..].trim_start();
        let col = l.col + text[..at].chars().count() as u32;
        return Some(
            Diagnostic::error(
                E_ARROW,
                file,
                Span::new(l.line_no, col, 2),
                format!("\"->\" was replaced by \"into\". Write: {before} into {after}"),
            )
            .with_help("A step names its result with \"into\" at its end."),
        );
    }
    None
}

/// Byte offset of a `->` that is not inside a quoted text.
fn arrow_outside_quotes(text: &str) -> Option<usize> {
    let mut in_str = false;
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if in_str => escaped = true,
            '"' => in_str = !in_str,
            '-' if !in_str && text[i..].starts_with("->") => return Some(i),
            _ => {}
        }
    }
    None
}

/// The reverse of the line wrapping of `binder::core::writer::not_bound_entry` (Q202): the first
/// line's reason and the texts of the `  > ` lines below it become one reason. A line that ends in
/// an odd number of backslashes was split inside a word (joined without a space, marker removed);
/// every pair of trailing backslashes stands for one real backslash. The runs are read per line.
pub fn join_wrapped(first: &str, rest: &[&str]) -> String {
    let mut out = String::new();
    let mut join_plain = false;
    for (i, line) in std::iter::once(first).chain(rest.iter().copied()).enumerate() {
        if i > 0 && !join_plain && !out.is_empty() {
            out.push(' ');
        }
        let last = i == rest.len();
        if last {
            out.push_str(line);
            break;
        }
        let run = line.chars().rev().take_while(|c| *c == '\\').count();
        out.push_str(&line[..line.len() - run]);
        out.push_str(&"\\".repeat(run / 2));
        join_plain = run % 2 == 1;
    }
    out
}

// ---------------------------------------------------------------------------------- docs

/// Descriptions (SPEC 23.2): the paragraph lines directly under a heading describe it; a blank
/// line between them starts a new paragraph. A run of `>` lines directly above a heading is the
/// v0.3 way and an error now (E068). A `>` run at the top of the file that is separated from what
/// follows by a blank line is the file's note.
fn collect_docs(
    file: &str,
    lines: &[Line],
) -> (HashMap<u32, String>, Option<String>, HashSet<u32>, Vec<Diagnostic>) {
    let mut docs = HashMap::new();
    let mut used = HashSet::new();
    let mut errors = Vec::new();
    let mut file_doc = None;
    let mut seen_heading = false;
    let mut i = 0;
    while i < lines.len() {
        let l = &lines[i];
        if l.kind == LineKind::Comment && l.indent == 0 {
            let start = i;
            while i < lines.len() && lines[i].kind == LineKind::Comment {
                i += 1;
            }
            match lines.get(i).map(|l| &l.kind) {
                Some(LineKind::Heading1 | LineKind::Heading2) => {
                    let first = &lines[start];
                    let text: Vec<&str> = lines[start..i].iter().map(|l| l.text.as_str()).collect();
                    errors.push(
                        Diagnostic::error(
                            E_DOC_ABOVE,
                            file,
                            Span::new(first.line_no, 1, first.text.chars().count() as u32 + 2),
                            "A description goes under its heading as a plain paragraph, not in a \">\" line above it.",
                        )
                        .with_help(format!("Write:\n{} {}\n{}", if lines[i].kind == LineKind::Heading1 { "#" } else { "##" }, lines[i].text, text.join("\n"))),
                    );
                    used.extend(lines[start..i].iter().map(|l| l.line_no));
                }
                _ if !seen_heading && file_doc.is_none() => {
                    let text = lines[start..i].iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
                    file_doc = Some(text);
                    used.extend(lines[start..i].iter().map(|l| l.line_no));
                }
                _ => {}
            }
            continue;
        }
        if matches!(l.kind, LineKind::Heading1 | LineKind::Heading2) {
            seen_heading = true;
            let mut paragraphs: Vec<String> = Vec::new();
            let mut current: Vec<&str> = Vec::new();
            let mut j = i + 1;
            while j < lines.len() {
                match lines[j].kind {
                    LineKind::Paragraph => current.push(&lines[j].text),
                    LineKind::Blank | LineKind::Comment => {
                        if !current.is_empty() {
                            paragraphs.push(current.join("\n"));
                            current.clear();
                        }
                    }
                    _ => break,
                }
                j += 1;
            }
            if !current.is_empty() {
                paragraphs.push(current.join("\n"));
            }
            if !paragraphs.is_empty() {
                docs.insert(l.line_no, paragraphs.join("\n\n"));
            }
        }
        i += 1;
    }
    (docs, file_doc, used, errors)
}

// ---------------------------------------------------------------------------------- errors

/// The whitespace-delimited word of `text` that contains byte `at`.
fn word_around(text: &str, at: usize) -> (usize, usize) {
    let start = text[..at]
        .rfind(char::is_whitespace)
        .map(|i| i + text[i..].chars().next().map_or(1, char::len_utf8))
        .unwrap_or(0);
    let end = text[at..]
        .find(char::is_whitespace)
        .map(|i| at + i)
        .unwrap_or(text.len());
    (start, end)
}

/// The words of a line with their file spans.
fn words_of(ctx: &LineCtx) -> Vec<(String, Span)> {
    use chumsky::span::SimpleSpan;
    let mut out = Vec::new();
    let mut start = None;
    let text = ctx.text.as_str();
    for (i, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                out.push((text[s..i].to_string(), ctx.span(SimpleSpan::from(s..i))));
                start = None;
            }
            _ => {}
        }
    }
    out
}

fn missing_colon(file: &str, ctx: &LineCtx) -> Diagnostic {
    let text = ctx.text.trim_end();
    let col = ctx.col + text.chars().count() as u32;
    Diagnostic::error(
        E_SYNTAX,
        file,
        Span::new(ctx.line, col, 1),
        "This line is missing the \":\" at its end.",
    )
    .with_help(
        "A line that has statements under it ends with a colon, like \"for each row in rows:\".",
    )
}

/// Words and symbols that only ever continue an expression; listing them as "expected" is noise.
const NOISE_WORDS: &[&str] = &[
    "and", "or", "not", "is", "matches", "length", "else", "in", "as", "where", "to", "of", "with",
    "some", "empty", "at…", "where…", "descending…", "with…",
];

/// Put a word in quotes (backticks when it already contains a quote).
fn quote(word: &str) -> String {
    if word.contains('"') {
        format!("`{word}`")
    } else {
        format!("\"{word}\"")
    }
}

/// Turn a chumsky error into a plain-language diagnostic.
fn rich_diag(file: &str, ctx: &LineCtx, e: &Rich<'_, char>) -> Diagnostic {
    use chumsky::span::SimpleSpan;
    let text = ctx.text.as_str();
    let start = e.span().start.min(text.len());
    let (message, range) = match e.reason() {
        RichReason::Custom(m) => (sentence(m), *e.span()),
        RichReason::ExpectedFound { .. } => {
            let mut expected: Vec<String> = Vec::new();
            let mut words: Vec<String> = Vec::new();
            let mut at_end = false;
            for p in e.expected() {
                let s = match p {
                    RichPattern::Token(t) => {
                        let c: char = **t;
                        if matches!(c, '"' | ':' | ',') {
                            quote(&c.to_string())
                        } else {
                            continue; // operators and spacing
                        }
                    }
                    RichPattern::Label(l) => {
                        let l: &str = l;
                        if l.chars().all(|c| c.is_ascii_lowercase()) {
                            words.push(l.to_string());
                        }
                        if l.contains("whitespace") || NOISE_WORDS.contains(&l) {
                            continue;
                        }
                        if l == "identifier" {
                            continue; // the keyword matchers' own probe; real names say "a name"
                        } else if l.chars().all(|c| c.is_ascii_lowercase()) {
                            quote(l)
                        } else {
                            l.to_string()
                        }
                    }
                    RichPattern::EndOfInput => {
                        at_end = true;
                        continue;
                    }
                    _ => continue,
                };
                if !expected.contains(&s) {
                    expected.push(s);
                }
            }
            if expected.iter().any(|x| x == "a value") {
                expected.retain(|x| x != "a name");
            }
            // The offending word: skip spaces so we never point at nothing.
            let mut ws = start;
            while ws < text.len() && text[ws..].starts_with(char::is_whitespace) {
                ws += text[ws..].chars().next().map_or(1, char::len_utf8);
            }
            let found_word = (e.found().is_some() && ws < text.len()).then(|| {
                let (_, we) = word_around(text, ws);
                (ws, we)
            });
            // An operator with nothing after it, or a `:` right after `and`/`or`/`not`/an operator:
            // a value is what is missing.
            if let Some((ws, we)) = found_word {
                let w = &text[ws..we];
                let opish = |t: &str| {
                    !t.is_empty() && t.chars().all(|c| "=<>!+-*/".contains(c))
                        || matches!(t, "and" | "or" | "not")
                };
                if opish(w) && text[we..].trim().is_empty() {
                    let m = format!(
                        "The line ends too early: expected a value after {}.",
                        quote(w)
                    );
                    return Diagnostic::error(
                        E_SYNTAX,
                        file,
                        ctx.span(SimpleSpan::from(text.len()..text.len())),
                        m,
                    );
                }
                let before = text[..ws].split_whitespace().last().unwrap_or("");
                if opish(before)
                    && !w
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '"')
                {
                    let m = format!(
                        "Expected a value after {}, but found {}.",
                        quote(before),
                        quote(w)
                    );
                    return Diagnostic::error(
                        E_SYNTAX,
                        file,
                        ctx.span(SimpleSpan::from(ws..we)),
                        m,
                    );
                }
            }
            if let Some((ws, we)) = found_word {
                let q = quote(&text[ws..we]);
                let near = did_you_mean(&text[ws..we], words.iter().map(String::as_str));
                // Two names in a row inside an expression (`... and email send customer`): the
                // second one is not a typo of a keyword, it is a statement where a value belongs.
                let before = text[..ws].split_whitespace().last().unwrap_or("");
                let word_like = |t: &str| t.chars().next().is_some_and(char::is_alphabetic) && t.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');
                let keywordish = |t: &str| matches!(t, "and" | "or" | "not" | "is" | "in" | "of" | "to" | "as" | "else");
                // a real typo of and/or (one edit or the same letters) keeps "Did you mean"
                let found = &text[ws..we];
                let typo_of = |kw: &str| {
                    strsim::osa_distance(found, kw) <= 1 || {
                        let (mut a, mut b): (Vec<char>, Vec<char>) = (found.chars().collect(), kw.chars().collect());
                        a.sort_unstable();
                        b.sort_unstable();
                        a == b
                    }
                };
                // and a statement has more after the verb: `email send customer`
                let after = text[we..].split_whitespace().next().unwrap_or("");
                if matches!(near.as_deref(), Some("and" | "or"))
                    && !typo_of("and")
                    && !typo_of("or")
                    && word_like(after)
                    && word_like(before)
                    && word_like(&text[ws..we])
                    && !keywordish(before)
                    && !keywordish(&text[ws..we])
                {
                    let m = format!(
                        "I did not expect {q} here: \"{before} {}\" looks like a statement, but only values, comparisons and \"and\" / \"or\" / \"not\" can be written in a condition.",
                        &text[ws..we]
                    );
                    return Diagnostic::error(E_SYNTAX, file, ctx.span(SimpleSpan::from(ws..we)), m);
                }
                if let Some(w) = near {
                    let m = format!("Expected {} here, but found {q}.", quote(&w));
                    let mut span = ctx.span(SimpleSpan::from(ws..we));
                    span.len = span.len.max(1);
                    return Diagnostic::error(E_SYNTAX, file, span, m).with_did_you_mean(Some(w));
                }
                let m = if expected.is_empty() {
                    let tail = if at_end {
                        " The line should end before it."
                    } else {
                        ""
                    };
                    format!("I did not expect {q} here.{tail}")
                } else {
                    format!("Expected {} here, but found {q}.", join_or(&expected))
                };
                (m, SimpleSpan::from(ws..we))
            } else if !expected.is_empty() {
                let m = format!("The line ends too early: expected {}.", join_or(&expected));
                (m, SimpleSpan::from(text.len()..text.len()))
            } else {
                // The parser consumed a word and then gave up on it.
                let (ws, we) = word_around(text, start.saturating_sub(1));
                let m = format!("I could not understand {} here.", quote(&text[ws..we]));
                (m, SimpleSpan::from(ws..we))
            }
        }
    };
    let mut span = ctx.span(range);
    if span.len == 0 {
        span.len = 1;
    }
    Diagnostic::error(E_SYNTAX, file, span, message)
}

/// A custom message as a sentence: capital first letter, full stop at the end.
fn sentence(m: &impl ToString) -> String {
    let mut m = capitalize(&m.to_string());
    if !m.ends_with('.') {
        m.push('.');
    }
    m
}

fn prop_comments(props: &[Prop<'_, '_>]) -> Vec<PropComment> {
    props
        .iter()
        .filter(|p| !p.comments.is_empty())
        .map(|p| PropComment {
            key: p.key.name.clone(),
            comments: p.comments.clone(),
        })
        .collect()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn join_or(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} or {b}"),
        many => {
            let (last, head) = many.split_last().expect("non-empty");
            format!("{}, or {last}", head.join(", "))
        }
    }
}
