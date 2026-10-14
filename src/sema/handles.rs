//! Opaque library handles (SPEC 22.4): the `handle of <crate>.<Type>` type, calls on a handle
//! variable (`- hasher update data`), and the ownership rules that go with them.
//!
//! A handle is a value of a library type AgentMark cannot look into. Rules (OPEN_QUESTIONS Q360-Q369):
//! - it moves: passing it on (`set`, a function argument) or a `consume` method uses it up, and
//!   reading it afterwards is E361, which names the line that used it up;
//! - it can only be read where a binding or a function takes it; printing, comparing, saving,
//!   sending or tracing it is E362;
//! - it is a scalar: no `list of`, `maybe` or record field (E363);
//! - a binding parameter says how the call holds a handle: `handle of` reads it, `mut handle of`
//!   changes it, `consume handle of` takes it (E365 anywhere else).

use std::collections::BTreeMap;

use crate::diag::{Diagnostic, Span};
use crate::syntax::ast::{Decl, Expr, ExprKind, HandleMode, Ident, Payload, TargetPart, TypeExpr, TypeKind};

use super::body::Cx;
use super::types::Type;
use super::Checker;

pub const E_UNKNOWN_HANDLE: &str = "E360";
pub const E_CONSUMED: &str = "E361";
pub const E_HANDLE_USE: &str = "E362";
pub const E_HANDLE_NEST: &str = "E363";
pub const E_HANDLE_MODE: &str = "E365";
pub const E_HANDLE_TRY: &str = "E366";
/// A handle cannot come from outside the program (command line, request body, file).
pub const E_HANDLE_OUTSIDE: &str = "E367";
// E364 is intentionally unused: an unknown method of a handle is reported as E207 (the same
// code as an unknown verb of a binding), with a did-you-mean over that handle type's methods.

/// Every handle type a type expression names.
fn handles_in(te: &TypeExpr, out: &mut Vec<String>) {
    match &te.kind {
        TypeKind::Handle { name, .. } => out.push(name.clone()),
        TypeKind::List(i) | TypeKind::Maybe(i) | TypeKind::Result(i) => handles_in(i, out),
        TypeKind::Map(k, v) => {
            handles_in(k, out);
            handles_in(v, out);
        }
        TypeKind::Named(_) => {}
    }
}

impl Checker<'_> {
    /// Before anything is typed: which handle types do the bindings declare?
    pub(super) fn collect_handle_types(&mut self) {
        let p = self.p;
        let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for f in &p.files {
            for d in &f.ast.decls {
                let Decl::Binding(b) = d else { continue };
                let mut names = Vec::new();
                for it in &b.items {
                    for prm in &it.params {
                        handles_in(&prm.ty, &mut names);
                    }
                    if let Some(o) = &it.out {
                        handles_in(o, &mut names);
                    }
                }
                for n in names {
                    let v = found.entry(n).or_default();
                    if !v.contains(&b.name.name) {
                        v.push(b.name.name.clone());
                    }
                }
            }
        }
        self.handle_types = found;
    }

    /// `handle of x.Y` written in code: the type must exist, and a mode word belongs to a binding.
    pub(super) fn resolve_handle_type(&mut self, file: usize, te: &TypeExpr, name: &str, mode: HandleMode) -> Type {
        if let Some(w) = mode.word() {
            self.diags.push(
                Diagnostic::error(
                    E_HANDLE_MODE,
                    self.path(file),
                    te.span,
                    format!("\"{w}\" in front of \"handle of\" only belongs in a binding's parameter list."),
                )
                .with_help(
                    "In your own code a handle parameter is handed over: the function owns it and can \
                     return it. A binding says how a library call holds it (\"mut\" changes it, \"consume\" uses it up).",
                ),
            );
            return Type::Unknown;
        }
        if !name.contains('.') {
            self.diags.push(
                Diagnostic::error(
                    E_UNKNOWN_HANDLE,
                    self.path(file),
                    te.span,
                    format!("A handle type is written with its crate, but \"{name}\" has none."),
                )
                .with_help("Write it like \"handle of sha2.Sha256\"."),
            );
            return Type::Unknown;
        }
        if !self.handle_types.contains_key(name) {
            let cands: Vec<&str> = self.handle_types.keys().map(String::as_str).collect();
            let d = Diagnostic::error(
                E_UNKNOWN_HANDLE,
                self.path(file),
                te.span,
                format!("No binding declares the handle type \"{name}\"."),
            )
            .suggesting(name, cands)
            .with_help("Handle types come from the bindings in bindings/ (\"agentmark bind <crate>\" writes them).");
            self.diags.push(d);
            return Type::Unknown;
        }
        Type::Handle(name.to_string())
    }

    /// `list of handle …` / `maybe handle …`: not in v0.3.
    pub(super) fn nest_check(&mut self, file: usize, te: &TypeExpr, inner: Type) -> Type {
        if inner.is_handle() {
            self.diags.push(
                Diagnostic::error(
                    E_HANDLE_NEST,
                    self.path(file),
                    te.span,
                    format!("A handle cannot be in a {}: {inner}.", match &te.kind {
                        TypeKind::List(_) => "list",
                        _ => "maybe",
                    }),
                )
                .with_help("Keep one variable per handle. A handle is a live library value, so it cannot be stored in a collection."),
            );
            return Type::Unknown;
        }
        inner
    }

    /// A binding item parameter: the mode word and the type it applies to.
    pub(super) fn resolve_binding_param(&mut self, file: usize, te: &TypeExpr) -> (Type, HandleMode) {
        match &te.kind {
            TypeKind::Handle { name, mode } => {
                let plain = TypeExpr { kind: TypeKind::Handle { name: name.clone(), mode: HandleMode::Shared }, span: te.span };
                (self.resolve_type(file, &plain), *mode)
            }
            _ => (self.resolve_type(file, te), HandleMode::Shared),
        }
    }

    /// `self` must be the first parameter of a binding item and a handle.
    pub(super) fn check_self_params(&mut self, file: usize, it: &crate::syntax::ast::BindingItem, tys: &[Type]) {
        for (i, p) in it.params.iter().enumerate() {
            if p.name.name != "self" {
                continue;
            }
            if i != 0 || !tys.get(i).is_some_and(Type::is_handle) {
                self.diags.push(
                    Diagnostic::error(
                        E_HANDLE_MODE,
                        self.path(file),
                        p.name.span,
                        "\"self\" must be the first parameter of a binding item, and a handle.",
                    )
                    .with_help("Write it like \"self: mut handle of sha2.Sha256\"."),
                );
            }
        }
    }

    // ------------------------------------------------------------------ ownership

    /// A handle read: used up already?
    pub(super) fn check_not_consumed(&mut self, cx: &Cx, name: &Ident) {
        let Some(&at) = cx.consumed.get(&name.name) else { return };
        let text = self.p.files[cx.file]
            .text
            .lines()
            .nth(at.line.saturating_sub(1) as usize)
            .map(|l| l.trim().trim_start_matches("- ").to_string())
            .unwrap_or_default();
        self.diags.push(
            Diagnostic::error(
                E_CONSUMED,
                self.path(cx.file),
                name.span,
                format!("\"{}\" was used up on line {} (\"{text}\"), so it cannot be used again here.", name.name, at.line),
            )
            .with_help(if matches!(cx.var(&name.name), Some(Type::Result(_))) {
                format!("A stored result is matched once. Call the function again, or do everything with \"{}\" in that one match.", name.name)
            } else {
                format!(
                    "A handle moves when a library call takes it or you pass it on. Create a new one \
                     (the line that made \"{}\") or keep using it before line {}.",
                    name.name, at.line
                )
            }),
        );
    }

    /// A handle read in a place that does not take it: print, compare, save, send, trace.
    pub(super) fn handle_misuse(&mut self, cx: &Cx, name: &Ident, ty: &Type) {
        self.diags.push(
            Diagnostic::error(
                E_HANDLE_USE,
                self.path(cx.file),
                name.span,
                format!("\"{}\" is {ty}, a library value that cannot be printed, compared, saved, sent or traced.", name.name),
            )
            .with_help(
                "Use it with a call that takes it (\"- hasher update data\"), or take a value out of it first \
                 (\"- hasher finalize -> digest\").",
            ),
        );
    }

    /// A handle where the value would have to come from outside the program.
    pub(super) fn outside_handle(&mut self, file: usize, span: Span, ty: &Type, what: &str) {
        if ty.is_handle() {
            self.error(
                E_HANDLE_OUTSIDE,
                file,
                span,
                format!("{what} comes from outside the program, and a handle cannot: it is a live library value ({ty})."),
            );
        }
    }

    /// A handle that is the value of an expression (a call) in a place that does not take it.
    pub(super) fn handle_value_misuse(&mut self, cx: &Cx, span: Span, ty: &Type) {
        self.diags.push(
            Diagnostic::error(
                E_HANDLE_USE,
                self.path(cx.file),
                span,
                format!("This is {ty}, a library value that cannot be printed, compared, saved, sent or traced."),
            )
            .with_help("Store it in a variable first (\"... -> name\") and use it with a call that takes it."),
        );
    }

    /// The handles given to a function or `## Rust` block: a `## Rust` block takes them; a `# Function`
    /// takes the ones its body uses up, changes or hands back, and only borrows the rest (Q414).
    pub(super) fn move_call_args(&mut self, cx: &mut Cx, kind: super::scope::Kind, decl: &str, args: &[Expr]) {
        let lends: Vec<bool> = if kind == super::scope::Kind::Function {
            if let Some(ent) = self.reg.get(kind, decl).cloned() {
                self.ensure_body(super::body::BodyKey::Function { file: ent.file, decl: ent.decl });
            }
            self.fn_lends.get(decl).cloned().unwrap_or_default()
        } else {
            Vec::new()
        };
        for (i, a) in args.iter().enumerate() {
            if !lends.get(i).copied().unwrap_or(false) {
                self.move_out(cx, a);
            }
        }
    }

    /// An expression whose value was handed over (`set`, a function argument): a plain handle
    /// variable is used up.
    pub(super) fn move_out(&mut self, cx: &mut Cx, e: &Expr) {
        if let ExprKind::Path(segs) = &e.kind {
            if segs.len() == 1 && matches!(cx.var(&segs[0].name), Some(Type::Handle(_))) {
                cx.took.insert(segs[0].name.clone());
                cx.consumed.entry(segs[0].name.clone()).or_insert(e.span);
            }
        }
    }

    /// Check an expression that may be a handle read (`set`, call arguments, `return`).
    pub(super) fn expr_handle_ok(&mut self, cx: &mut Cx, e: &Expr) -> Type {
        cx.handle_ok = true;
        let t = self.expr(cx, e);
        cx.handle_ok = false;
        t
    }

    /// Arguments of a handle-taking call: handles passed to `consume` parameters are used up,
    /// `handle of` and `mut handle of` ones must be variables, and one handle cannot go twice.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_args(
        &mut self,
        cx: &mut Cx,
        sk: super::NodeKey,
        tool: &Ident,
        recv_mode: HandleMode,
        params: &[super::ParamInfo],
        modes: &[HandleMode],
        args: &[(Type, Span)],
        arg_vars: &[Option<String>],
    ) {
        let mut seen: Vec<String> = Vec::new();
        let recv_is_var = matches!(cx.var(&tool.name), Some(Type::Handle(_)));
        if recv_is_var {
            seen.push(tool.name.clone());
        }
        let mut used_up: Vec<String> = Vec::new();
        for (i, p) in params.iter().enumerate() {
            if !p.ty.is_handle() {
                continue;
            }
            let (mode, span) = (modes.get(i).copied().unwrap_or(HandleMode::Shared), args[i].1);
            let var = arg_vars.get(i).cloned().flatten().filter(|v| matches!(cx.var(v), Some(Type::Handle(_))));
            match var {
                Some(v) => {
                    if seen.contains(&v) {
                        self.diags.push(
                            Diagnostic::error(E_HANDLE_MODE, self.path(cx.file), span, format!("\"{v}\" is given twice in one call."))
                                .with_help("A handle cannot be borrowed twice, or borrowed and used up, by the same call."),
                        );
                    }
                    seen.push(v.clone());
                    if mode != HandleMode::Shared {
                        cx.took.insert(v.clone());
                    }
                    if mode == HandleMode::Consume {
                        used_up.push(v);
                    }
                }
                None => {
                    // a handle that is not a variable (the result of a function call) can only be handed over
                    if mode != HandleMode::Consume && !args[i].0.is_unknown() {
                        self.diags.push(
                            Diagnostic::error(
                                E_HANDLE_MODE,
                                self.path(cx.file),
                                span,
                                format!("\"{}\" borrows its handle, so give it a variable, not a new value.", p.name),
                            )
                            .with_help("Store the handle in a variable first (\"... -> name\")."),
                        );
                    }
                }
            }
        }
        for v in used_up {
            cx.consumed.entry(v).or_insert(sk.span);
        }
        if recv_mode != HandleMode::Shared && recv_is_var {
            cx.took.insert(tool.name.clone());
        }
        if recv_mode == HandleMode::Consume && recv_is_var {
            cx.consumed.entry(tool.name.clone()).or_insert(sk.span);
        }
    }

    /// `- hasher update data`: the tool word is a handle variable.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_call(
        &mut self,
        cx: &mut Cx,
        sk: super::NodeKey,
        handle: &str,
        tool: &Ident,
        verb: &Option<Ident>,
        target: &[TargetPart],
        payload: &Option<Payload>,
        into: &Option<Ident>,
    ) {
        self.check_not_consumed(cx, tool);
        let owners = self.handle_types.get(handle).cloned().unwrap_or_default();
        let has = |ck: &Checker<'_>, b: &str| {
            verb.as_ref().is_some_and(|v| {
                ck.bindings.get(b).is_some_and(|i| i.items.iter().any(|it| it.name == v.name && it.receiver() == Some(handle)))
            })
        };
        let bname = owners.iter().find(|b| has(self, b)).or_else(|| owners.first()).cloned();
        let Some(bname) = bname else { return };
        // a file that only calls methods still uses the library it imported
        let _ = self.lookup(cx.file, &bname, &[super::scope::Kind::Binding]);
        if verb.is_none() {
            let methods: Vec<String> = owners
                .iter()
                .filter_map(|b| self.bindings.get(b))
                .flat_map(|i| i.items.iter().filter(|it| it.receiver() == Some(handle)).map(|it| it.name.clone()))
                .collect();
            self.diags.push(
                Diagnostic::error(
                    super::tools::E_MANIFEST,
                    self.path(cx.file),
                    tool.span,
                    format!("\"{}\" is a handle, so the line needs a method after it.", tool.name),
                )
                .with_help(format!("Methods of {handle}: {}.", methods.join(", "))),
            );
        }
        self.binding_call(cx, sk, &bname, tool, verb, target, payload, into, Some(handle));
    }

    /// `try … -> h otherwise:` for a handle: the `otherwise` branch must leave, because there is
    /// no handle to put in its place.
    pub(super) fn check_try_handle(&mut self, cx: &Cx, into: Option<&Ident>, otherwise_leaves: bool) {
        let Some(i) = into else { return };
        if otherwise_leaves || !matches!(cx.var(&i.name), Some(Type::Handle(_))) {
            return;
        }
        self.diags.push(
            Diagnostic::error(
                E_HANDLE_TRY,
                self.path(cx.file),
                i.span,
                format!("\"{}\" is a handle, and there is none to use when this step fails.", i.name),
            )
            .with_help("End the \"otherwise:\" branch with \"stop\" (or \"return\"), or drop the \"otherwise:\"."),
        );
    }
}
