//! Declarations: record fields (with SPEC 2.1 inference), signatures of functions, Rust blocks,
//! bindings and commands, constants, agent headers.

use crate::diag::Diagnostic;
use crate::syntax::ast::{Decl, Effect, Param, TypeExpr, TypeKind};

use super::body::Cx;
use super::effects::effect_name;
use super::scope::Kind;
use super::types::{assignable, infer_field_type, Type, BUILTIN_TYPES};
use super::{
    NodeKey, BindingInfo, BindingItemInfo, Checker, ConstInfo, ConstState, FieldInfo, ParamInfo, RecordInfo,
    RustInfo, Sig,
};

pub const E_UNKNOWN_TYPE: &str = "E201";
pub const W_INFERRED: &str = "W001";
const E_BAD_AGENT: &str = "E215";
const E_BAD_MODEL: &str = "E221";
const E_CONST: &str = "E216";
const W_UNVERIFIED: &str = "W005";
const W_ASSUMED: &str = "W008";

impl Checker<'_> {
    /// Find a declaration visible in `file` under the name as written, marking it (and the
    /// import that brought it) as used.
    pub fn lookup(&mut self, file: usize, visible: &str, kinds: &[Kind]) -> Option<(Kind, String)> {
        let (key, via) = self.scopes[file].find(visible, kinds)?;
        if let Some(i) = via {
            self.used_imports.insert((file, i));
        }
        self.used.insert(key.clone());
        Some(key)
    }

    /// Fields of a record as `(name, type)`.
    pub fn record_fields(&self, name: &str) -> Option<Vec<(String, Type)>> {
        self.records.get(name).map(|r| {
            r.fields
                .iter()
                .map(|f| (f.name.clone(), f.ty.clone()))
                .collect()
        })
    }

    /// Extra sentence for a mismatch involving a record literal (missing fields, misspelled keys).
    pub fn note(&self, from: &Type, to: &Type) -> String {
        let n = super::types::object_note(from, to, &|r| self.record_fields(r));
        if n.is_empty() {
            n
        } else {
            format!(" {n}")
        }
    }

    pub fn is_assignable(&self, from: &Type, to: &Type) -> bool {
        assignable(from, to, &|r| self.record_fields(r))
    }

    /// Resolve a written type in the scope of `file`. Unknown names are errors. The answer is
    /// remembered for every written type (`Checked::type_exprs`), aliases already replaced.
    pub fn resolve_type(&mut self, file: usize, te: &TypeExpr) -> Type {
        let t = self.resolve_type_inner(file, te);
        self.type_exprs.insert(NodeKey::new(file, te.span), t.clone());
        t
    }

    fn resolve_type_inner(&mut self, file: usize, te: &TypeExpr) -> Type {
        match &te.kind {
            TypeKind::Named(n) => {
                if let Some(t) = Type::builtin(n) {
                    return t;
                }
                let kinds = [Kind::Record, Kind::Enum, Kind::TypeAlias];
                if let Some((kind, name)) = self.lookup(file, n, &kinds) {
                    return match kind {
                        Kind::Enum => Type::Enum(name),
                        Kind::TypeAlias => self.alias_type(&name),
                        _ => Type::Record(name),
                    };
                }
                let mut cands: Vec<String> = BUILTIN_TYPES.iter().map(|s| s.to_string()).collect();
                cands.extend(self.scopes[file].names_of(&kinds));
                let d = Diagnostic::error(E_UNKNOWN_TYPE, self.path(file), te.span, format!("Unknown type \"{n}\"."))
                    .with_did_you_mean(super::scope::suggest(n, &cands))
                    .with_help(
                        "Types are text, int, money, date, datetime, duration, bool, bytes, or a record, \
                         enum or type that this file declares or imports.",
                    );
                self.diags.push(d);
                Type::Unknown
            }
            TypeKind::Infer(words) => self.infer_type(file, te, words),
            TypeKind::Handle { name, mode } => self.resolve_handle_type(file, te, name, *mode),
            TypeKind::List(t) => {
                let inner = self.resolve_type(file, t);
                self.nest_check(file, te, inner).list()
            }
            TypeKind::Maybe(t) => {
                let inner = self.resolve_type(file, t);
                self.nest_check(file, te, inner).maybe()
            }
            TypeKind::Result(t) => Type::Result(Box::new(self.resolve_type(file, t))),
            TypeKind::Map(k, v) => {
                let key = self.resolve_type(file, k);
                self.check_map_key(file, k.span, &key);
                Type::Map(Box::new(key), Box::new(self.resolve_type(file, v)))
            }
        }
    }

    /// SPEC 23.6: the type of a name written without one in `- in:` / `- out:`. The words are
    /// matched against the records, enums and type aliases `file` can see, longest ending first;
    /// a regular plural means a list. Else the last word decides as for a record field (2.1).
    fn infer_type(&mut self, file: usize, te: &TypeExpr, words: &[String]) -> Type {
        if let Some(t) = self.match_user_type(file, words, &[Kind::Record, Kind::Enum, Kind::TypeAlias]) {
            return t;
        }
        let last = words.last().map(String::as_str).unwrap_or("");
        let (t, matched) = infer_field_type(last);
        if matched {
            return t;
        }
        let said = words.join(" ");
        let named = words.join("_");
        let help = if self.resolving_out {
            "Write the type instead, for example \"- out: text\" or \"- out: list of Sale\". A name works when it is a record's name (\"sales\" for a list of Sale)."
                .to_string()
        } else {
            format!("A name works when it is a record's name (\"sales\" for a list of Sale) or ends in a word such as date, amount or count; otherwise write \"{named}: text\" (or int, money, …).")
        };
        let d = Diagnostic::error(
            "E230",
            self.path(file),
            te.span,
            if self.resolving_out {
                format!("Could not tell what \"{said}\" is. Write its type: \"- out: <type>\".")
            } else {
                format!("Could not tell what \"{said}\" is. Write its type: \"{named}: <type>\".")
            },
        )
        .with_help(help);
        self.diags.push(d);
        Type::Unknown
    }

    /// The user type (of the given kinds) the words of a name stand for, if any (SPEC 23.6).
    pub fn match_user_type(&mut self, file: usize, words: &[String], kinds: &[Kind]) -> Option<Type> {
        let visible: Vec<String> = self.scopes[file].names_of(kinds);
        let norm = |s: &str| -> String {
            s.chars().filter(|c| *c != '_' && *c != '-').flat_map(char::to_lowercase).collect()
        };
        for k in (1..=words.len()).rev() {
            let tail = &words[words.len() - k..];
            let (lead, last) = tail.split_at(k - 1);
            let mut forms: Vec<(String, bool)> = vec![(last[0].clone(), false)];
            forms.extend(singulars(&last[0]).into_iter().map(|s| (s, true)));
            for (form, plural) in forms {
                let want = norm(&format!("{}{form}", lead.concat()));
                let hit = visible.iter().find(|v| norm(v.rsplit('.').next().unwrap_or(v)) == want).cloned();
                if let Some(v) = hit {
                    if let Some((kind, name)) = self.lookup(file, &v, kinds) {
                        let t = match kind {
                            Kind::Enum => Type::Enum(name),
                            Kind::TypeAlias => self.alias_type(&name),
                            _ => Type::Record(name),
                        };
                        return Some(if plural { t.list() } else { t });
                    }
                }
            }
        }
        None
    }

    /// SPEC 23.8: the record named last in a rule's heading, as written or in any case.
    fn rule_record(&mut self, file: usize, written: &str) -> Option<String> {
        if let Some((_, rec)) = self.lookup(file, written, &[Kind::Record]) {
            return Some(rec);
        }
        let want = written.to_lowercase();
        let hit = self.scopes[file]
            .names_of(&[Kind::Record])
            .into_iter()
            .find(|v| v.rsplit('.').next().unwrap_or(v).to_lowercase() == want)?;
        self.lookup(file, &hit, &[Kind::Record]).map(|(_, r)| r)
    }

    /// `- touches:` (SPEC 23.7) of every function, Rust block and tool, as effect sets; the
    /// signatures get the class.
    pub(super) fn declare_touches(&mut self) {
        let p = self.p;
        for (fi, f) in p.files.iter().enumerate() {
            for d in &f.ast.decls {
                match d {
                    Decl::Function(x) => {
                        if let Some(t) = &x.touches {
                            let set = self.touch_set(fi, t);
                            let key = (Kind::Function, x.name.name.clone());
                            if let Some(sig) = self.sigs.get_mut(&key) {
                                sig.effect = set.class();
                            }
                            self.touches_decl.entry(key).or_insert(set);
                        }
                    }
                    Decl::RustBlock(x) => {
                        let set = self.touch_set(fi, &x.touches);
                        let key = (Kind::RustBlock, x.name.name.clone());
                        if let Some(sig) = self.sigs.get_mut(&key) {
                            sig.effect = set.class();
                        }
                        for r in self.rust_blocks.iter_mut().filter(|r| r.name == x.name.name) {
                            r.sig.effect = set.class();
                        }
                        self.touches_decl.entry(key).or_insert(set);
                    }
                    Decl::Tool(t) => {
                        let rust_verb = t.verbs.iter().find(|v| matches!(v.body, crate::syntax::ast::ToolBody::Rust(_)));
                        let Some(touches) = &t.touches else {
                            if let Some(v) = rust_verb {
                                let d = Diagnostic::error(
                                    "E012",
                                    self.path(fi),
                                    t.name.span,
                                    format!(
                                        "The tool \"{}\" has a verb written in Rust (\"{}\"), so it must say what it touches.",
                                        t.name.name, v.name.name
                                    ),
                                )
                                .with_help("Add \"- touches: nothing\" (or files, print, AI, or the tools it uses) under the heading. AgentMark cannot read Rust, so it trusts this line.");
                                self.diags.push(d);
                            }
                            continue;
                        };
                        let set = self.touch_set(fi, touches);
                        for v in &t.verbs {
                            let key = (Kind::Tool, super::user_tools::fn_name(&t.name.name, &v.name.name));
                            if let Some(sig) = self.sigs.get_mut(&key) {
                                sig.effect = set.class();
                            }
                        }
                        for r in self.rust_blocks.iter_mut().filter(|r| r.parent.as_deref() == Some(&t.name.name)) {
                            r.sig.effect = set.class();
                        }
                        self.touches_decl.entry((Kind::Tool, t.name.name.clone())).or_insert(set);
                    }
                    _ => {}
                }
            }
        }
    }

    /// The effect set the words after `touches:` stand for. Unknown words are E232.
    fn touch_set(&mut self, file: usize, t: &crate::syntax::ast::Touches) -> super::EffectSet {
        let mut set = super::EffectSet::pure();
        let nothing = t.words.iter().any(|w| w.name == "nothing");
        if nothing {
            if t.words.len() > 1 {
                self.error(
                    "E232",
                    file,
                    t.span,
                    "\"nothing\" stands alone: a part that touches something does not touch nothing.",
                );
            }
            return set;
        }
        for w in &t.words {
            if let Some((class, source)) = super::effects::builtin_touch(&w.name) {
                set.add(class, source);
                continue;
            }
            if let Some(def) = self.p.manifest.tool(&w.name) {
                set.add(def.effect, &w.name);
                continue;
            }
            if let Some(b) = self
                .bindings
                .values()
                .find(|b| b.name == w.name || b.crate_name == w.name || b.crate_name.replace('-', "_") == w.name)
            {
                let class = b.effective(self.p.assume_proposed);
                set.add(class, &b.name.clone());
                continue;
            }
            if self.lookup(file, &w.name, &[Kind::Tool]).is_some() {
                set.add(Effect::Local, &w.name);
                continue;
            }
            let mut known: Vec<String> = ["nothing", "AI", "files", "print", "input", "state"].iter().map(|s| s.to_string()).collect();
            known.extend(self.p.manifest.tool_names().map(str::to_string));
            known.extend(self.bindings.values().map(|b| b.name.clone()));
            known.extend(self.scopes[file].names_of(&[Kind::Tool]));
            let d = Diagnostic::error(
                "E232",
                self.path(file),
                w.span,
                format!("\"touches:\" names \"{}\", which is nothing this program can touch.", w.name),
            )
            .with_did_you_mean(super::scope::suggest(&w.name, &known))
            .with_help("Use nothing, AI, files, print, input, state, or the name of a tool or library.");
            self.diags.push(d);
        }
        set
    }

    pub(super) fn declare_records(&mut self) {
        let p = self.p;
        for (fi, f) in p.files.iter().enumerate() {
            for d in &f.ast.decls {
                let Decl::Record(r) = d else { continue };
                if self.records.contains_key(&r.name.name) {
                    continue; // duplicate, already reported
                }
                let mut fields: Vec<FieldInfo> = Vec::new();
                for fld in &r.fields {
                    let (ty, inferred) = match &fld.ty {
                        Some(te) => {
                            let t = self.resolve_type(fi, te);
                            if t.is_handle() {
                                self.error(
                                    super::handles::E_HANDLE_NEST,
                                    fi,
                                    te.span,
                                    format!("A record is saved and sent as JSON, so it cannot have a field of {t}."),
                                );
                                (Type::Unknown, false)
                            } else {
                                (t, false)
                            }
                        }
                        None => {
                            let (t, matched) = infer_field_type(&fld.name.name);
                            self.inferred_fields.push(super::InferredField {
                                record: r.name.name.clone(),
                                field: fld.name.name.clone(),
                                ty: t.clone(),
                                fallback: !matched,
                                file: fi,
                                span: fld.name.span,
                            });
                            if !matched {
                                self.warn(
                                    W_INFERRED,
                                    fi,
                                    fld.name.span,
                                    format!(
                                        "The field \"{}\" has no type and its name gives no hint, so it is inferred as {t}.",
                                        fld.name.name
                                    ),
                                );
                                self.diags.last_mut().unwrap().help = Some(format!(
                                    "Write the type explicitly, for example \"- {}: text\".",
                                    fld.name.name
                                ));
                            }
                            (t, true)
                        }
                    };
                    if fields.iter().any(|x| x.name == fld.name.name) {
                        self.error(
                            "E217",
                            fi,
                            fld.name.span,
                            format!(
                                "The record \"{}\" has the field \"{}\" twice.",
                                r.name.name, fld.name.name
                            ),
                        );
                        continue;
                    }
                    fields.push(FieldInfo {
                        name: fld.name.name.clone(),
                        ty,
                        inferred,
                        span: fld.name.span,
                    });
                }
                self.records.insert(
                    r.name.name.clone(),
                    RecordInfo {
                        name: r.name.name.clone(),
                        file: fi,
                        span: r.name.span,
                        fields,
                    },
                );
            }
        }
    }

    fn params(&mut self, file: usize, ps: &[Param]) -> Vec<ParamInfo> {
        ps.iter()
            .map(|p| {
                let ty = self.resolve_type(file, &p.ty);
                if matches!(ty, Type::Result(_)) {
                    self.error(
                        super::enums::E_RESULT_MATCH,
                        file,
                        p.ty.span,
                        "A result is made by a call and matched where it is stored; it cannot be passed in.",
                    );
                    self.diags.last_mut().unwrap().help =
                        Some("Pass the plain value, or call the function that makes the result inside this one.".into());
                }
                ParamInfo {
                    name: p.name.name.clone(),
                    ty,
                    has_default: p.default.is_some(),
                }
            })
            .collect()
    }

    pub(super) fn declare_signatures(&mut self) {
        let p = self.p;
        for (fi, f) in p.files.iter().enumerate() {
            for d in &f.ast.decls {
                match d {
                    Decl::Function(x) => {
                        let params = self.params(fi, &x.params);
                        self.resolving_out = true;
                        let out = x.out.as_ref().map(|t| self.resolve_type(fi, t));
                        self.resolving_out = false;
                        // the class of `- touches:` is filled in by `declare_touches`
                        let sig = Sig {
                            params,
                            out,
                            effect: Effect::Pure,
                            file: fi,
                            span: x.name.span,
                        };
                        self.sigs
                            .entry((Kind::Function, x.name.name.clone()))
                            .or_insert(sig);
                    }
                    Decl::RustBlock(x) => {
                        let sig = Sig {
                            params: self.params(fi, &x.params),
                            out: x.out.as_ref().map(|t| self.resolve_type(fi, t)),
                            effect: Effect::Pure,
                            file: fi,
                            span: x.name.span,
                        };
                        self.sigs
                            .entry((Kind::RustBlock, x.name.name.clone()))
                            .or_insert(sig.clone());
                        self.rust_blocks.push(RustInfo {
                            name: x.name.name.clone(),
                            file: fi,
                            span: x.name.span,
                            parent: x.parent.as_ref().map(|i| i.name.clone()),
                            sig,
                        });
                    }
                    Decl::Command(x) => {
                        let params = self.params(fi, &x.args);
                        for (p, ast_p) in params.iter().zip(&x.args) {
                            self.outside_handle(fi, ast_p.ty.span, &p.ty, &format!("The command argument \"{}\"", p.name));
                        }
                        let sig = Sig {
                            params,
                            out: None,
                            effect: Effect::Model,
                            file: fi,
                            span: x.name.span,
                        };
                        self.sigs
                            .entry((Kind::Command, x.name.name.clone()))
                            .or_insert(sig);
                    }
                    Decl::Rule(x) => {
                        if let Some(rec) = self.rule_record(fi, &x.for_record.name) {
                            self.rule_records.insert(x.name.name.clone(), rec);
                        } else {
                            let te = TypeExpr {
                                kind: TypeKind::Named(x.for_record.name.clone()),
                                span: x.for_record.span,
                            };
                            self.resolve_type(fi, &te); // reports the unknown record
                        }
                    }
                    Decl::Binding(x) => {
                        let mut items = Vec::new();
                        for it in &x.items {
                            let (ptys, modes): (Vec<Type>, Vec<_>) =
                                it.params.iter().map(|p| self.resolve_binding_param(fi, &p.ty)).unzip();
                            self.check_self_params(fi, it, &ptys);
                            let params: Vec<ParamInfo> = it
                                .params
                                .iter()
                                .zip(ptys)
                                .map(|(p, ty)| ParamInfo { name: p.name.name.clone(), ty, has_default: p.default.is_some() })
                                .collect();
                            let sig = Sig {
                                params,
                                out: it.out.as_ref().map(|t| self.resolve_type(fi, t)),
                                effect: match (x.effect.value, &x.proposed_effect) {
                                    (Effect::Unverified, Some(pe)) if self.p.assume_proposed => pe.value,
                                    (e, _) => e,
                                },
                                file: fi,
                                span: it.name.span,
                            };
                            items.push(BindingItemInfo {
                                name: it.name.name.clone(),
                                sig,
                                modes,
                            });
                        }
                        if let (Effect::Unverified, Some(pe), true) = (x.effect.value, &x.proposed_effect, self.p.assume_proposed) {
                            self.warn(
                                W_ASSUMED,
                                fi,
                                x.effect.span,
                                format!(
                                    "The binding \"{}\" is unverified: its effect is assumed to be \"{}\" from the proposal (--allow-unverified).",
                                    x.name.name,
                                    effect_name(pe.value)
                                ),
                            );
                            self.diags.last_mut().unwrap().help = Some(format!(
                                "Approve it before a release build by changing the line to \"- effect: {}\" (or the right class).",
                                effect_name(pe.value)
                            ));
                        }
                        for h in &x.handles {
                            if !self.handle_types.contains_key(&h.name) {
                                self.error(
                                    super::handles::E_UNKNOWN_HANDLE,
                                    fi,
                                    h.span,
                                    format!("No item of this binding uses the handle \"{}\".", h.name),
                                );
                            }
                        }
                        let assumed = x.effect.value == Effect::Unverified
                            && x.proposed_effect.is_some()
                            && self.p.assume_proposed;
                        // W008 says it already; no second warning about the same binding
                        if x.effect.value == Effect::Unverified && !assumed {
                            self.warn(
                                W_UNVERIFIED,
                                fi,
                                x.effect.span,
                                format!(
                                    "The binding \"{}\" is unverified: nobody approved what it may touch.",
                                    x.name.name
                                ),
                            );
                            self.diags.last_mut().unwrap().help = Some(match &x.proposed_effect {
                                Some(pe) => format!(
                                    "Approve it by changing the line to \"- effect: {}\" (or the right class).",
                                    effect_name(pe.value)
                                ),
                                None => "Approve it by changing the line to \"- effect: network\" (or the right class)."
                                    .to_string(),
                            });
                        }
                        self.bindings.insert(
                            x.name.name.clone(),
                            BindingInfo {
                                name: x.name.name.clone(),
                                file: fi,
                                span: x.name.span,
                                crate_name: x.crate_name.name.clone(),
                                effect: x.effect.value,
                                proposed_effect: x.proposed_effect.as_ref().map(|e| e.value),
                                items,
                                handles: x.handles.iter().map(|h| (h.name.clone(), h.rust.code.trim().to_string())).collect(),
                            },
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    /// Agent `tools:` must name manifest tools or bindings.
    pub(super) fn check_agent_headers(&mut self) {
        let p = self.p;
        for (fi, f) in p.files.iter().enumerate() {
            for d in &f.ast.decls {
                let Decl::Agent(a) = d else { continue };
                if let Err(e) = agentmark_rt::Driver::parse(&a.model.value) {
                    let mut d = Diagnostic::error(E_BAD_MODEL, self.path(fi), a.model.span, e.to_string()).with_help(agentmark_rt::DRIVER_HELP);
                    if let agentmark_rt::DriverError::Unknown(word) = &e {
                        d = d.with_did_you_mean(super::scope::suggest(word, &agentmark_rt::PROVIDERS.map(String::from)));
                    }
                    self.diags.push(d);
                }
                for t in &a.tools {
                    if p.manifest.tool(&t.name).is_some()
                        || self.lookup(fi, &t.name, &[Kind::Binding, Kind::Tool]).is_some()
                        || self.unbound_crate_tool(fi, &t.name)
                    {
                        continue;
                    }
                    let mut cands: Vec<String> =
                        p.manifest.tool_names().map(str::to_string).collect();
                    cands.extend(self.scopes[fi].names_of(&[Kind::Binding, Kind::Tool]));
                    let d = Diagnostic::error(
                        E_BAD_AGENT,
                        self.path(fi),
                        t.span,
                        format!("Unknown tool \"{}\".", t.name),
                    )
                    .with_did_you_mean(super::scope::suggest(&t.name, &cands))
                    .with_help("Tools are declared in tools.toml next to the project.");
                    self.diags.push(d);
                }
            }
        }
    }

    /// Type of the constant `name` (computed on first use; constants may use other constants).
    pub fn const_type(&mut self, name: &str) -> Type {
        match self.consts.get(name) {
            Some(ConstState::Done(t)) => return t.clone(),
            Some(ConstState::InProgress) => {
                if let Some(e) = self.reg.get(Kind::Constant, name).cloned() {
                    self.error(
                        E_CONST,
                        e.file,
                        e.span,
                        format!("The constant \"{name}\" is defined in terms of itself."),
                    );
                }
                return Type::Unknown;
            }
            None => {}
        }
        let Some(ent) = self.reg.get(Kind::Constant, name).cloned() else {
            return Type::Unknown;
        };
        self.consts.insert(name.to_string(), ConstState::InProgress);
        let p = self.p;
        let Decl::Constant(c) = &p.files[ent.file].ast.decls[ent.decl] else {
            return Type::Unknown;
        };
        let mut cx = Cx::constant(ent.file);
        let ty = self.expr(&mut cx, &c.value);
        self.consts
            .insert(name.to_string(), ConstState::Done(ty.clone()));
        self.const_infos.insert(
            name.to_string(),
            ConstInfo {
                name: name.to_string(),
                file: ent.file,
                span: c.name.span,
                ty: ty.clone(),
                value: c.value.clone(),
            },
        );
        ty
    }

    /// Check a parameter default against the parameter type.
    pub fn check_default(&mut self, file: usize, p: &Param, ty: &Type) {
        if let Some(d) = &p.default {
            let mut cx = Cx::constant(file);
            let got = self.expr(&mut cx, d);
            if !self.is_assignable(&got, ty) {
                self.error(
                    "E203",
                    file,
                    d.span,
                    format!(
                        "The default of \"{}\" is {got}, but the parameter is {ty}.",
                        p.name.name
                    ),
                );
            }
        }
    }
}

/// The singular forms a regular English plural may have (SPEC 23.6): `sales` -> `sale`,
/// `boxes` -> `box`, `entries` -> `entry`.
pub fn singulars(word: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(stem) = word.strip_suffix("ies") {
        if !stem.is_empty() {
            out.push(format!("{stem}y"));
        }
    }
    if let Some(stem) = word.strip_suffix("es") {
        if !stem.is_empty() {
            out.push(stem.to_string());
        }
    }
    if let Some(stem) = word.strip_suffix('s') {
        if !stem.is_empty() && !stem.ends_with('s') {
            out.push(stem.to_string());
        }
    }
    out
}
