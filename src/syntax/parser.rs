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

pub mod expr;
pub mod heading;
pub mod stmt;

use expr::{run, LineCtx, P};
use heading::*;
use stmt::Head;

/// First words of the blocks an agent may have.
const BLOCK_WORDS: &[&str] = &["Every", "When", "On", "Do", "Rust"];

/// The effect classes a declaration may state.
const EFFECT_WORDS: [&str; 5] = ["pure", "local", "network", "model", "unverified"];

/// Words that start a statement (a near miss gets a suggestion).
const STATEMENT_WORDS: &[&str] = &[
    "for",
    "if",
    "else",
    "stop",
    "do",
    "ask",
    "guard",
    "set",
    "return",
    "respond",
    "read",
    "try",
    "parallel",
    "append",
    "print",
    "otherwise",
    "match",
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

/// Parse one `.agent.md` file. `path` is the name shown in diagnostics.
pub fn parse_file(path: &str, src: &str) -> Result<File, Vec<Diagnostic>> {
    let (lines, diags) = lex(path, src);
    if !diags.is_empty() {
        // Lines were dropped by the lexer, so the structure cannot be trusted any further.
        return Err(diags);
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
    run_heading: P<'l, ()>,
    not_bound: P<'l, ()>,
    binding_item: P<'l, (Ident, Vec<Param>, Option<TypeExpr>)>,
    prop_head: P<'l, (Ident, &'l str, chumsky::span::SimpleSpan)>,
    field: P<'l, (Ident, Option<TypeExpr>)>,
    variant: P<'l, (Ident, Vec<(Ident, TypeExpr)>)>,
    arm: P<'l, stmt::Arm>,
    params: P<'l, Vec<Param>>,
    name_list: P<'l, Vec<Ident>>,
    model: P<'l, Spanned<String>>,
    memory: P<'l, Memory>,
    effect: P<'l, Effect>,
    single_name: P<'l, Ident>,
    rest: P<'l, Spanned<String>>,
    run_item: P<'l, Ident>,
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
            run_heading: run_heading(),
            not_bound: not_bound_heading(),
            binding_item: binding_item_heading(),
            prop_head: property_head(),
            field: field(),
            variant: variant(),
            arm: stmt::arm(),
            params: params(),
            name_list: name_list(),
            model: model_string(),
            memory: memory(),
            effect: effect(),
            single_name: single_name(),
            rest: rest_of_line(),
            run_item: run_item(),
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
        let (docs, file_doc, doc_lines) = collect_docs(lines);
        self.docs = docs;
        let mut file = File {
            path: self.file.to_string(),
            doc: file_doc,
            imports: Vec::new(),
            decls: Vec::new(),
        };

        let live: Vec<&'l Line> = lines
            .iter()
            .filter(|l| match l.kind {
                LineKind::Blank => false,
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
                // a comment before the first heading that documents nothing
            } else if !stray_reported {
                stray_reported = true;
                self.error(
                    E_OUTSIDE,
                    line_span(l),
                    "This line is not inside a declaration. Start with a heading such as \"# Agent: name\".",
                );
            }
        }

        for (h, body) in chunks {
            self.chunk(h, &body, &mut file);
        }
        file
    }

    fn chunk(&mut self, h: &'l Line, body: &[&'l Line], file: &mut File) {
        let doc = self.docs.get(&h.line_no).cloned();

        // The first word decides whether this is a known kind of declaration.
        let word = match self.parse(|g| &g.h1_word, &h.text, h.line_no, h.col) {
            Some(w) => w,
            None => return,
        };
        if !HEADING_WORDS
            .iter()
            .any(|k| k.eq_ignore_ascii_case(&word.name))
        {
            let d = Diagnostic::error(
                E_UNKNOWN_DECL,
                self.file,
                word.span,
                format!("Unknown declaration \"{}\".", word.name),
            )
            .suggesting(&word.name, HEADING_WORDS.iter().copied())
            .with_help(format!(
                "A declaration starts with one of: {}.",
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
            H1::Import(t) => self.import(h, body, doc, t, file),
            H1::Run => {
                let mut pos = 0;
                let nodes = self.build(body, &mut pos, 0);
                let items = self.run_items(&nodes);
                file.decls.push(Decl::Run(Run { span, doc, items }));
            }
            H1::Decl(kind, name) => {
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
                "An import is a single line and has nothing under it.",
            );
        }
        let path = t.target.name.clone();
        let kind = if t.is_crate {
            ImportKind::Crate {
                name: t.target,
                alias: t.alias,
            }
        } else if path.contains('*') {
            if let Some(a) = t.alias {
                self.error(
                    E_IMPORT,
                    a.span,
                    "A glob import cannot be renamed with \"as\".",
                );
                return;
            }
            ImportKind::Glob { pattern: path }
        } else if path.ends_with(".agent.md") {
            ImportKind::File {
                path,
                alias: t.alias,
            }
        } else {
            match t.alias {
                Some(alias) => ImportKind::Folder { path, alias },
                None => {
                    self.error(
                        E_IMPORT,
                        t.target.span,
                        format!(
                            "\"{path}\" is not a .agent.md file. To import a folder, give it a name: \"# Import: {path} as <name>\"."
                        ),
                    );
                    return;
                }
            }
        };
        file.imports.push(Import { span, doc, kind });
    }

    // ------------------------------------------------------------------ indentation tree

    /// Build the statement/property tree for `lines` starting at `pos` with the given indent.
    fn build(&mut self, lines: &[&'l Line], pos: &mut usize, level: u32) -> Vec<Node<'l>> {
        let mut out = Vec::new();
        if level == 0 {
            self.pending.clear();
        }
        while *pos < lines.len() {
            let l = lines[*pos];
            if l.kind == LineKind::Comment {
                self.pending.push(l.text.clone());
                *pos += 1;
                continue;
            }
            if l.indent < level && !matches!(l.kind, LineKind::FenceBody | LineKind::FenceClose) {
                break;
            }
            let is_fence = matches!(l.kind, LineKind::FenceOpen(_));
            if l.indent > level && !is_fence {
                self.bad_lines.insert(l.line_no);
                self.error(
                    E_INDENT_DEEP,
                    Span::new(l.line_no, 1, l.indent * 2),
                    format!(
                        "This line is indented {} spaces, but only {} are possible here. Indent by one level (2 spaces) at a time.",
                        l.indent * 2,
                        level * 2
                    ),
                );
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
            let kids = self.build(lines, pos, l.indent.max(level) + 1);
            out.push(Node {
                line: l,
                comments,
                kids,
                fence: None,
            });
        }
        if level == 0 {
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
            DeclKind::Rule => {
                self.no_blocks(kind, &blocks);
                self.rule(span, doc, name, &prop_nodes)
            }
            DeclKind::Constant => {
                self.no_blocks(kind, &blocks);
                self.constant(span, doc, name, &prop_nodes)
            }
            DeclKind::Agent => self.agent(span, doc, name, &prop_nodes, blocks),
            DeclKind::Function => self.function(span, doc, name, &prop_nodes, blocks),
            DeclKind::Service => self.service(span, doc, name, &prop_nodes, blocks),
            DeclKind::Command => self.command(span, doc, name, &prop_nodes, blocks),
            DeclKind::Binding => self.binding(span, doc, name, &prop_nodes, blocks),
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
        let mut out: Vec<Prop<'n, 'l>> = Vec::new();
        for n in nodes {
            if n.fence.is_some() {
                continue;
            }
            if n.line.kind != LineKind::Item {
                self.error(
                    E_STRUCTURE,
                    line_span(n.line),
                    "A property starts with \"- \", like \"- model: claude\".",
                );
                continue;
            }
            let Some((key, value, range)) =
                self.parse(|g| &g.prop_head, &n.line.text, n.line.line_no, n.line.col)
            else {
                continue;
            };
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
            if out.iter().any(|p| p.key.name == key.name) {
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
                    E_STRUCTURE,
                    line_span(n.line),
                    "A field starts with \"- \", like \"- amount: money\".",
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
                    E_STRUCTURE,
                    line_span(n.line),
                    "A variant starts with \"- \", like \"- Circle: radius: int\" or \"- Empty\".",
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

    fn rule(
        &mut self,
        span: Span,
        doc: Option<String>,
        name: Ident,
        nodes: &[Node<'l>],
    ) -> Option<Decl> {
        let props = self.props(nodes, &["for", "when"], "a Rule");
        let for_p = props.iter().find(|p| p.key.name == "for");
        let when_p = props.iter().find(|p| p.key.name == "when");
        let mut ok = true;
        if for_p.is_none() {
            self.missing("rule", &name, "for", "Invoice");
            ok = false;
        }
        if when_p.is_none() {
            self.missing("rule", &name, "when", "status != \"paid\"");
            ok = false;
        }
        let for_record = for_p.and_then(|p| self.value(|g| &g.single_name, p));
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

    fn run_items(&mut self, nodes: &[Node<'l>]) -> Vec<Ident> {
        let mut items = Vec::new();
        for n in nodes {
            if n.line.kind != LineKind::Item {
                self.error(
                    E_STRUCTURE,
                    line_span(n.line),
                    "A \"# Run\" entry starts with \"- \" and names an agent, service or command.",
                );
                continue;
            }
            if let Some(i) = self.parse(|g| &g.run_item, &n.line.text, n.line.line_no, n.line.col) {
                items.push(i);
            }
        }
        items
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

    fn function(
        &mut self,
        span: Span,
        doc: Option<String>,
        name: Ident,
        nodes: &[Node<'l>],
        blocks: Vec<(&'l Line, Vec<&'l Line>)>,
    ) -> Option<Decl> {
        let props = self.props(nodes, &["in", "out", "effect"], "a Function");
        let params = match props.iter().find(|p| p.key.name == "in") {
            Some(p) => self.value(|g| &g.params, p).unwrap_or_default(),
            None => Vec::new(),
        };
        let out = props
            .iter()
            .find(|p| p.key.name == "out")
            .and_then(|p| self.value(|g| &g.ty, p));
        let effect_p = props.iter().find(|p| p.key.name == "effect");
        let effect = effect_p.and_then(|p| {
            self.word_value(|g| &g.effect, p, &EFFECT_WORDS, "effect")
                .map(|value| Spanned {
                    value,
                    span: Span::new(p.line, p.col, p.value.chars().count() as u32),
                })
        });
        let mut body = Vec::new();
        for p in &props {
            if p.key.name == "effect" {
                body = self.stmts(&p.node.kids);
            } else if let Some(k) = p.node.kids.first() {
                self.error(
                    E_STRUCTURE,
                    line_span(k.line),
                    format!(
                        "Statements go under the \"effect\" line, not under \"{}\".",
                        p.key.name
                    ),
                );
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
                    "A Function has no blocks except \"## Rust: name\".",
                );
            }
        }
        if effect_p.is_none() && !self.rejected_prop {
            let d = Diagnostic::error(
                E_MISSING_PROP,
                self.file,
                name.span,
                format!("The function \"{}\" does not say what effect it has.", name.name),
            )
            .with_help(
                "Add \"- effect: pure\" (or local, network, model) under the heading; the compiler checks the body against it.",
            );
            self.diags.push(d);
        }
        Some(Decl::Function(Function {
            span,
            doc,
            prop_comments: prop_comments(&props),
            name,
            params,
            out,
            effect: effect?,
            body,
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
        let props = self.props(nodes, &["args"], "a Command");
        for p in &props {
            self.no_kids(p);
        }
        let args = match props.iter().find(|p| p.key.name == "args") {
            Some(p) => self.value(|g| &g.params, p).unwrap_or_default(),
            None => Vec::new(),
        };
        let mut body = None;
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
                if self
                    .parse(|g| &g.run_heading, &h2.text, h2.line_no, h2.col)
                    .is_none()
                {
                    continue;
                }
                if body.is_some() {
                    self.error(
                        E_STRUCTURE,
                        line_span(h2),
                        "A Command has only one \"## Run\" block.",
                    );
                    continue;
                }
                let mut pos = 0;
                let nodes = self.build(&lines, &mut pos, 0);
                body = Some(self.stmts(&nodes));
            } else {
                self.error(
                    E_STRUCTURE,
                    line_span(h2),
                    "A Command has one block, \"## Run\" (plus optional \"## Rust: name\" blocks).",
                );
            }
        }
        if body.is_none() {
            self.error(
                E_MISSING_PROP,
                name.span,
                format!("The command \"{}\" has no \"## Run\" block.", name.name),
            );
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
            let Some((iname, params, out)) =
                self.parse(|g| &g.binding_item, &h2.text, h2.line_no, h2.col)
            else {
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
        let props = self.props(&nodes, &["in", "effect"], "a Rust block");
        let params = match props.iter().find(|p| p.key.name == "in") {
            Some(p) => self.value(|g| &g.params, p).unwrap_or_default(),
            None => Vec::new(),
        };
        let effect = match props.iter().find(|p| p.key.name == "effect") {
            Some(p) => self
                .word_value(|g| &g.effect, p, &EFFECT_WORDS, "effect")
                .map(|value| Spanned {
                    value,
                    span: Span::new(p.line, p.col, p.value.chars().count() as u32),
                }),
            None => {
                let d = Diagnostic::error(
                    E_MISSING_PROP,
                    self.file,
                    name.span,
                    format!("The Rust block \"{}\" does not declare its effect.", name.name),
                )
                .with_help("Add \"- effect: pure\" (or local, network, model). The compiler cannot check Rust, so it trusts this line.");
                self.diags.push(d);
                None
            }
        };
        let rust = self.fence_in(&nodes, h2, &name.name);
        if let (Some(effect), Some(rust)) = (effect, rust) {
            self.hoisted.push(Decl::RustBlock(RustBlock {
                span: line_span(h2),
                doc: self.docs.get(&h2.line_no).cloned(),
                prop_comments: prop_comments(&props),
                name,
                parent,
                params,
                out,
                effect,
                rust,
            }));
        }
    }

    // ------------------------------------------------------------------ statements

    fn heads(&mut self, nodes: &[Node<'l>]) -> Vec<Option<Head>> {
        let mut out = Vec::new();
        for n in nodes {
            let head = match n.line.kind {
                LineKind::Item | LineKind::Control => {
                    match run(&self.g.head, &n.line.text, n.line.line_no, n.line.col) {
                        Ok(h) => Some(h),
                        Err((ctx, errs)) => {
                            self.statement_error(&ctx, &errs);
                            None
                        }
                    }
                }
                _ => {
                    let msg = if n.fence.is_some() {
                        "Code fences belong in a \"## Rust: name\" block, not among statements."
                    } else {
                        "This is not a statement."
                    };
                    self.error(E_STRUCTURE, line_span(n.line), msg);
                    None
                }
            };
            out.push(head);
        }
        out
    }

    /// The branches of a `match`: `Circle c:` lines with statements under them, and at most one
    /// `otherwise:` at the end.
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
            if !matches!(n.line.kind, LineKind::Item | LineKind::Control) {
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

    fn stmts(&mut self, nodes: &[Node<'l>]) -> Vec<Stmt> {
        let heads = self.heads(nodes);
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
                    let then = self.stmts(&node.kids);
                    let mut else_ = Vec::new();
                    if matches!(heads.get(i + 1), Some(Some(Head::Else))) {
                        else_ = self.stmts(&nodes[i + 1].kids);
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
                    "\"else:\" must come directly after an \"if\" at the same indentation.",
                ),
                Head::Otherwise => self.error(
                    E_STRUCTURE,
                    span,
                    "\"otherwise:\" must be indented under a \"try\" line.",
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
                Head::Simple(s) => {
                    if let Some(k) = node.kids.first() {
                        self.kid_error(k.line, "Nothing can be indented under this statement.");
                    }
                    out.push(s.clone());
                }
                Head::Try(inner) => {
                    let kid_heads = self.heads(&node.kids);
                    let mut otherwise = None;
                    for (k, h) in node.kids.iter().zip(kid_heads) {
                        match h {
                            Some(Head::Otherwise) if otherwise.is_none() => {
                                otherwise = Some(self.stmts(&k.kids));
                            }
                            Some(_) => self.kid_error(
                                k.line,
                                "Only \"otherwise:\" can be indented under a \"try\" line.",
                            ),
                            None => {}
                        }
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

/// Doc comments: a run of `>` lines directly above a heading documents it. A run at the top of
/// the file that is separated from what follows by a blank line documents the file.
fn collect_docs(lines: &[Line]) -> (HashMap<u32, String>, Option<String>, HashSet<u32>) {
    let mut docs = HashMap::new();
    let mut used = HashSet::new();
    let mut file_doc = None;
    let mut seen_heading = false;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind == LineKind::Comment {
            let start = i;
            while i < lines.len() && lines[i].kind == LineKind::Comment {
                i += 1;
            }
            let text = lines[start..i]
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let is_doc = match lines.get(i).map(|l| &l.kind) {
                Some(LineKind::Heading1 | LineKind::Heading2) => {
                    docs.insert(lines[i].line_no, text);
                    true
                }
                _ if !seen_heading && file_doc.is_none() => {
                    file_doc = Some(text);
                    true
                }
                _ => false,
            };
            if is_doc {
                used.extend(lines[start..i].iter().map(|l| l.line_no));
            }
            continue;
        }
        if matches!(lines[i].kind, LineKind::Heading1 | LineKind::Heading2) {
            seen_heading = true;
        }
        i += 1;
    }
    (docs, file_doc, used)
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
    "some", "empty",
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
