//! Tool use (SPEC 4.3, 7): tool names, verbs, targets, sources and events against the manifest,
//! and the rule that an agent may only call the tools it lists.

use crate::diag::Diagnostic;
use crate::manifest::{singular, ToolDef};
use crate::syntax::ast::{Ident, Payload, Stmt, StmtKind, TargetPart};

use super::body::{Cx, Implicit};
use super::effects::EffectSet;
use super::expr::{E_TYPE, E_UNKNOWN_NAME};
use super::scope::Kind;
use super::types::Type;
use super::{Checker, Resolution, ToolKind, ToolResolution};

pub const E_MANIFEST: &str = "E207";
pub const E_UNLISTED: &str = "E208";

impl Checker<'_> {
    /// The record whose name equals `word` ignoring case (`invoice` -> `Invoice`), visible in
    /// `file` (OPEN_QUESTIONS Q5).
    pub fn record_for_word(&mut self, file: usize, word: &str) -> Option<String> {
        let name = self.scopes[file]
            .names_of(&[Kind::Record])
            .into_iter()
            .find(|n| n.eq_ignore_ascii_case(word))?;
        self.lookup(file, &name, &[Kind::Record]).map(|(_, n)| n)
    }

    /// An agent may only use tools in its `tools:` line.
    pub fn require_listed(&mut self, cx: &Cx, tool: &Ident) {
        let Some(a) = &cx.agent else { return };
        if a.tools.contains(&tool.name) {
            return;
        }
        let mut example = a.tools.clone();
        example.push(tool.name.clone());
        let d = Diagnostic::error(
            E_UNLISTED,
            self.path(cx.file),
            tool.span,
            format!(
                "The tool \"{}\" is not listed in the tools of agent \"{}\".",
                tool.name, a.name
            ),
        )
        .with_help(format!(
            "Add it to the \"- tools:\" line, for example \"- tools: {}\".",
            example.join(", ")
        ));
        self.diags.push(d);
    }

    fn unknown_tool(&mut self, cx: &Cx, tool: &Ident) {
        let mut cands: Vec<String> = self.p.manifest.tool_names().map(str::to_string).collect();
        cands.extend(self.scopes[cx.file].names_of(&[Kind::Binding]));
        let d = Diagnostic::error(
            E_MANIFEST,
            self.path(cx.file),
            tool.span,
            format!("Unknown tool \"{}\".", tool.name),
        )
        .suggesting(&tool.name, cands.iter().map(String::as_str));
        self.diags.push(d);
    }

    /// `crm.invoices`: a readable collection of a tool.
    pub(super) fn source(
        &mut self,
        cx: &mut Cx,
        segs: &[Ident],
        def: &ToolDef,
        key: super::NodeKey,
    ) -> Type {
        let first = &segs[0];
        let span = first.span.to(segs[segs.len() - 1].span);
        self.require_listed(cx, first);
        if segs.len() < 2 {
            let d = Diagnostic::error(
                E_MANIFEST,
                self.path(cx.file),
                first.span,
                format!("\"{}\" is a tool, not a value.", first.name),
            )
            .with_help(format!(
                "To read from it, name a source, for example \"{}.{}\".",
                first.name,
                def.sources.first().map(String::as_str).unwrap_or("items")
            ));
            self.diags.push(d);
            return Type::Unknown;
        }
        let src = &segs[1];
        if !def.sources.contains(&src.name) {
            let d = Diagnostic::error(
                E_MANIFEST,
                self.path(cx.file),
                src.span,
                format!("Unknown source \"{}\" for tool {}.", src.name, def.name),
            )
            .suggesting(&src.name, def.sources.iter().map(String::as_str));
            self.diags.push(d);
            return Type::Unknown;
        }
        if segs.len() > 2 {
            let d = Diagnostic::error(
                E_TYPE,
                self.path(cx.file),
                segs[2].span,
                format!(
                    "\"{}.{}\" is a whole list; loop over it to read the fields of its items.",
                    first.name, src.name
                ),
            );
            self.diags.push(d);
            return Type::Unknown;
        }
        let label = format!("{}.{}", first.name, src.name);
        let eff = EffectSet::of(def.effect, &def.name);
        self.touch(cx, span, &label, &eff);
        let word = singular(&src.name);
        let record = self.record_for_word(cx.file, &word);
        self.resolutions.insert(
            key,
            Resolution::Source {
                tool: def.name.clone(),
                source: src.name.clone(),
                record: record.clone(),
            },
        );
        match record {
            Some(rec) => Type::List(Box::new(Type::Record(rec))),
            None => {
                if !cx.lenient {
                    let cap = {
                        let mut c = word.chars();
                        c.next()
                            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                            .unwrap_or_default()
                    };
                    let d = Diagnostic::error(
                        E_UNKNOWN_NAME,
                        self.path(cx.file),
                        src.span,
                        format!("\"{label}\" holds {}, but no record \"{cap}\" is visible in this file.", src.name),
                    )
                    .with_help(format!("Declare \"# Record: {cap}\" or import the file that does."));
                    self.diags.push(d);
                }
                Type::List(Box::new(Type::Unknown))
            }
        }
    }

    /// `## When <tool> receives <event>`: validates the tool and event; the event becomes a
    /// variable of the record with the same name (`lead` -> `Lead`).
    pub(super) fn check_event(&mut self, cx: &Cx, tool: &Ident, event: &Ident) -> Option<Implicit> {
        let Some(def) = self.p.manifest.tool(&tool.name).cloned() else {
            self.unknown_tool(cx, tool);
            return None;
        };
        self.require_listed(cx, tool);
        if !def.events.contains(&event.name) {
            let d = Diagnostic::error(
                E_MANIFEST,
                self.path(cx.file),
                event.span,
                format!("Unknown event \"{}\" for tool {}.", event.name, def.name),
            )
            .suggesting(&event.name, def.events.iter().map(String::as_str))
            .with_help(if def.events.is_empty() {
                format!("The tool {} has no events in tools.toml.", def.name)
            } else {
                format!("Events of {}: {}.", def.name, def.events.join(", "))
            });
            self.diags.push(d);
            return None;
        }
        match self.record_for_word(cx.file, &event.name) {
            Some(record) => Some(Implicit {
                var: event.name.clone(),
                record,
            }),
            None => {
                let cap = {
                    let mut c = event.name.chars();
                    c.next()
                        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                        .unwrap_or_default()
                };
                let d = Diagnostic::error(
                    E_UNKNOWN_NAME,
                    self.path(cx.file),
                    event.span,
                    format!("The event \"{}\" arrives as a record, but no record \"{cap}\" is visible in this file.", event.name),
                )
                .with_help(format!("Declare \"# Record: {cap}\" or import the file that does."));
                self.diags.push(d);
                None
            }
        }
    }

    /// `email send customer: "..."`, `crm update invoice: status = "x"`, `log: "..."`,
    /// `pdf render doc -> bytes`.
    pub(super) fn tool_call(&mut self, cx: &mut Cx, s: &Stmt) {
        let StmtKind::ToolCall {
            tool,
            verb,
            target,
            payload,
            into,
            ..
        } = &s.kind
        else {
            return;
        };
        let sk = super::NodeKey::new(cx.file, s.span);
        if let Some(def) = self.p.manifest.tool(&tool.name).cloned() {
            self.manifest_call(cx, sk, &def, tool, verb, target, payload, into);
            return;
        }
        if let Some((_, bname)) = self.lookup(cx.file, &tool.name, &[Kind::Binding]) {
            self.binding_call(cx, sk, &bname, tool, verb, target, payload, into);
            return;
        }
        self.unknown_tool(cx, tool);
        // still check what is written, so names inside are not reported as unused later
        self.value_parts(cx, target, payload);
        if let Some(i) = into {
            cx.define(&i.name, Type::Unknown);
        }
    }

    fn value_parts(&mut self, cx: &mut Cx, target: &[TargetPart], payload: &Option<Payload>) {
        for t in target {
            match t {
                TargetPart::Value(e) => {
                    self.expr(cx, e);
                }
                TargetPart::Word(i) => {
                    if cx.var(&i.name).is_some() {
                        self.path_expr(cx, std::slice::from_ref(i), super::NodeKey::new(cx.file, i.span));
                    }
                }
            }
        }
        match payload {
            Some(Payload::Value(e)) => {
                self.expr(cx, e);
            }
            Some(Payload::Assign(list)) => {
                for a in list {
                    self.expr(cx, &a.value);
                }
            }
            None => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn manifest_call(
        &mut self,
        cx: &mut Cx,
        sk: super::NodeKey,
        def: &ToolDef,
        tool: &Ident,
        verb: &Option<Ident>,
        target: &[TargetPart],
        payload: &Option<Payload>,
        into: &Option<Ident>,
    ) {
        self.require_listed(cx, tool);
        match verb {
            Some(v) if def.verbs.is_empty() => {
                let d = Diagnostic::error(
                    E_MANIFEST,
                    self.path(cx.file),
                    v.span,
                    format!(
                        "The tool {} has no verbs, but \"{}\" was given.",
                        def.name, v.name
                    ),
                )
                .with_help(format!("Write it as \"- {}: <text>\".", def.name));
                self.diags.push(d);
            }
            Some(v) if !def.verbs.contains(&v.name) => {
                let d = Diagnostic::error(
                    E_MANIFEST,
                    self.path(cx.file),
                    v.span,
                    format!("Unknown verb \"{}\" for tool {}.", v.name, def.name),
                )
                .suggesting(&v.name, def.verbs.iter().map(String::as_str));
                self.diags.push(d);
            }
            None if !def.verbs.is_empty() => {
                let d = Diagnostic::error(
                    E_MANIFEST,
                    self.path(cx.file),
                    tool.span,
                    format!("The tool {} needs a verb.", def.name),
                )
                .with_help(format!(
                    "Verbs of {}: {}.",
                    def.name,
                    def.verbs.join(", ")
                ));
                self.diags.push(d);
            }
            _ => {}
        }

        // target
        let words = def.target_words();
        let mut target_record: Option<String> = None;
        let mut target_word: Option<&Ident> = None;
        for (i, part) in target.iter().enumerate() {
            match part {
                TargetPart::Word(w) if i == 0 => {
                    target_word = Some(w);
                    if words.is_empty() {
                        let d = Diagnostic::error(
                            E_MANIFEST,
                            self.path(cx.file),
                            w.span,
                            format!(
                                "The tool {} has no targets, so \"{}\" is not expected here.",
                                def.name, w.name
                            ),
                        );
                        self.diags.push(d);
                    } else if !words.contains(&w.name) {
                        let d = Diagnostic::error(
                            E_MANIFEST,
                            self.path(cx.file),
                            w.span,
                            format!("Unknown target \"{}\" for tool {}.", w.name, def.name),
                        )
                        .suggesting(&w.name, words.iter().map(String::as_str));
                        self.diags.push(d);
                    } else {
                        target_record = self.record_for_word(cx.file, &singular(&w.name));
                    }
                }
                TargetPart::Word(w) => {
                    // a later word names a value: `crm get invoice id`
                    self.path_expr(cx, std::slice::from_ref(w), super::NodeKey::new(cx.file, w.span));
                }
                TargetPart::Value(e) => {
                    let t = self.expr(cx, e);
                    if !matches!(t, Type::Text | Type::Unknown) {
                        let d = Diagnostic::error(
                            E_TYPE,
                            self.path(cx.file),
                            e.span,
                            format!(
                                "A target value must be text, but this is {}.",
                                t.with_article()
                            ),
                        );
                        self.diags.push(d);
                    }
                }
            }
        }

        // payload
        match payload {
            Some(Payload::Value(e)) => {
                let t = self.expr(cx, e);
                self.check_plain_payload(cx, def, &target_record, e, &t);
            }
            Some(Payload::Assign(list)) => {
                let rec_fields = target_record
                    .as_ref()
                    .and_then(|r| self.records.get(r).map(|x| (r.clone(), x.fields.clone())));
                for a in list {
                    let t = self.expr(cx, &a.value);
                    if let Some((rname, fields)) = &rec_fields {
                        match fields.iter().find(|f| f.name == a.field.name) {
                            Some(f) => {
                                if !self.is_assignable(&t, &f.ty) {
                                    let d = Diagnostic::error(
                                        E_TYPE,
                                        self.path(cx.file),
                                        a.value.span,
                                        format!(
                                            "The field \"{}\" of {} is {}, but this is {}.{}",
                                            f.name,
                                            rname,
                                            f.ty,
                                            t.with_article(),
                                            self.note(&t, &f.ty)
                                        ),
                                    );
                                    self.diags.push(d);
                                }
                            }
                            None => {
                                let d = Diagnostic::error(
                                    E_UNKNOWN_NAME,
                                    self.path(cx.file),
                                    a.field.span,
                                    format!("Unknown field \"{}\" on {}.", a.field.name, rname),
                                )
                                .suggesting(&a.field.name, fields.iter().map(|f| f.name.as_str()));
                                self.diags.push(d);
                            }
                        }
                    }
                }
            }
            None => {}
        }

        // result
        if let Some(i) = into {
            let ty = match (verb.as_ref().map(|v| v.name.as_str()), &target_record) {
                (Some("get"), Some(r)) => Type::Record(r.clone()),
                (Some("list"), Some(r)) => Type::List(Box::new(Type::Record(r.clone()))),
                _ if def.sources.is_empty() && def.targets.is_empty() => Type::Text,
                _ => Type::Unknown,
            };
            cx.define(&i.name, ty);
        }
        let (record, var, field) = self.target_binding(cx, def, target, target_word, &target_record);
        self.tool_calls.insert(
            sk,
            ToolResolution {
                tool: def.name.clone(),
                kind: ToolKind::Manifest,
                verb: verb.as_ref().map(|v| v.name.clone()),
                target_word: target_word.map(|w| w.name.clone()),
                record,
                var,
                field,
            },
        );
        cx.acc.add(def.effect, &def.name);
    }

    /// A plain value after the `:` must fit what the tool takes: text for tools that send or
    /// log text, the target's record for a crm-like tool.
    fn check_plain_payload(
        &mut self,
        cx: &Cx,
        def: &ToolDef,
        target_record: &Option<String>,
        e: &crate::syntax::ast::Expr,
        t: &Type,
    ) {
        if let Some(r) = target_record {
            let want = Type::Record(r.clone());
            if !self.is_assignable(t, &want) {
                let d = Diagnostic::error(
                    E_TYPE,
                    self.path(cx.file),
                    e.span,
                    format!(
                        "The tool {} takes a {r} here, but this is {}.{}",
                        def.name,
                        t.with_article(),
                        self.note(t, &want)
                    ),
                );
                self.diags.push(d);
            }
        } else if def.sources.is_empty()
            && (def.verbs.is_empty() || !def.targets.is_empty())
            && !matches!(t, Type::Text | Type::Unknown)
        {
            let d = Diagnostic::error(
                E_TYPE,
                self.path(cx.file),
                e.span,
                format!(
                    "The tool {} takes text here, but this is {}.",
                    def.name,
                    t.with_article()
                ),
            )
            .with_help("Write the text in quotes; use {name} inside it to include a value.");
            self.diags.push(d);
        }
    }

    /// The record, variable and field a target word stands for: `crm update invoice` ->
    /// (Invoice, the in-scope Invoice variable, `id`); `email send customer` -> (the record in
    /// front of the block, its variable, `customer_email` else `customer`).
    fn target_binding(
        &self,
        cx: &Cx,
        def: &ToolDef,
        target: &[TargetPart],
        word: Option<&Ident>,
        target_record: &Option<String>,
    ) -> (Option<String>, Option<String>, Option<String>) {
        let has_field = |rec: &str, f: &str| {
            self.records
                .get(rec)
                .is_some_and(|r| r.fields.iter().any(|x| x.name == f))
        };
        if let Some(r) = target_record {
            let var = match cx.bare() {
                Some(b) if b.record == *r => Some(b.var.clone()),
                _ => cx
                    .frames
                    .iter()
                    .rev()
                    .flat_map(|f| {
                        let mut v: Vec<_> = f.vars.iter().collect();
                        v.sort_by(|a, b| a.0.cmp(b.0));
                        v
                    })
                    .find(|(_, t)| **t == Type::Record(r.clone()))
                    .map(|(n, _)| n.clone()),
            };
            let field = has_field(r, "id").then(|| "id".to_string());
            return (Some(r.clone()), var, field);
        }
        let Some(w) = word else {
            return (None, None, None);
        };
        if !def.targets.contains(&w.name) || target.len() > 1 {
            return (None, None, None);
        }
        match cx.bare() {
            Some(b) => {
                let email = format!("{}_email", w.name);
                let field = if has_field(&b.record, &email) {
                    Some(email)
                } else if has_field(&b.record, &w.name) {
                    Some(w.name.clone())
                } else {
                    None
                };
                (Some(b.record.clone()), Some(b.var.clone()), field)
            }
            None => (None, None, None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn binding_call(
        &mut self,
        cx: &mut Cx,
        sk: super::NodeKey,
        bname: &str,
        tool: &Ident,
        verb: &Option<Ident>,
        target: &[TargetPart],
        payload: &Option<Payload>,
        into: &Option<Ident>,
    ) {
        let info = self.bindings.get(bname).cloned();
        let mut out = Type::Unknown;
        let mut item_params: Option<Vec<super::ParamInfo>> = None;
        if let Some(info) = &info {
            match verb {
                None => {
                    let d = Diagnostic::error(
                        E_MANIFEST,
                        self.path(cx.file),
                        tool.span,
                        format!("The binding {bname} needs a verb."),
                    )
                    .with_help(format!(
                        "Bound functions: {}.",
                        info.items
                            .iter()
                            .map(|i| i.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                    self.diags.push(d);
                }
                Some(v) => match info.items.iter().find(|i| i.name == v.name) {
                    None => {
                        let d = Diagnostic::error(
                            E_MANIFEST,
                            self.path(cx.file),
                            v.span,
                            format!("Unknown verb \"{}\" for binding {bname}.", v.name),
                        )
                        .suggesting(&v.name, info.items.iter().map(|i| i.name.as_str()));
                        self.diags.push(d);
                    }
                    Some(item) => {
                        out = item.sig.out.clone().unwrap_or(Type::Unknown);
                        item_params = Some(item.sig.params.clone());
                        let words = target
                            .iter()
                            .filter(|t| matches!(t, TargetPart::Word(_) | TargetPart::Value(_)))
                            .count();
                        if words != item.sig.params.len() {
                            let names: Vec<&str> =
                                item.sig.params.iter().map(|p| p.name.as_str()).collect();
                            let d = Diagnostic::error(
                                E_MANIFEST,
                                self.path(cx.file),
                                v.span,
                                format!(
                                    "\"{bname} {}\" takes {} value{} ({}), but {} {} given.",
                                    v.name,
                                    names.len(),
                                    if names.len() == 1 { "" } else { "s" },
                                    names.join(", "),
                                    words,
                                    if words == 1 { "was" } else { "were" }
                                ),
                            );
                            self.diags.push(d);
                        }
                    }
                },
            }
        }
        // the values written after the verb, checked against the bound function's parameters
        let mut args: Vec<(Type, crate::diag::Span)> = Vec::new();
        for t in target {
            match t {
                TargetPart::Word(w) => {
                    let ty = self.path_expr(
                        cx,
                        std::slice::from_ref(w),
                        super::NodeKey::new(cx.file, w.span),
                    );
                    args.push((ty, w.span));
                }
                TargetPart::Value(e) => {
                    let ty = self.expr(cx, e);
                    args.push((ty, e.span));
                }
            }
        }
        if let Some(params) = &item_params {
            if params.len() == args.len() {
                for (p, (t, span)) in params.iter().zip(&args) {
                    if !self.is_assignable(t, &p.ty) {
                        let d = Diagnostic::error(
                            E_TYPE,
                            self.path(cx.file),
                            *span,
                            format!(
                                "\"{}\" of \"{bname} {}\" must be {}, but this is {}.{}",
                                p.name,
                                verb.as_ref().map(|v| v.name.as_str()).unwrap_or(""),
                                p.ty,
                                t.with_article(),
                                self.note(t, &p.ty)
                            ),
                        );
                        self.diags.push(d);
                    }
                }
            }
        }
        if let Some(Payload::Value(e)) = payload {
            self.expr(cx, e);
        }
        if let Some(Payload::Assign(list)) = payload {
            for a in list {
                self.expr(cx, &a.value);
            }
        }
        self.tool_calls.insert(
            sk,
            ToolResolution {
                tool: bname.to_string(),
                kind: ToolKind::Binding,
                verb: verb.as_ref().map(|v| v.name.clone()),
                target_word: None,
                record: None,
                var: None,
                field: None,
            },
        );
        if let Some(i) = into {
            cx.define(&i.name, out);
        }
        if let Some(info) = &info {
            cx.acc.add(info.effect, bname);
        }
    }
}
