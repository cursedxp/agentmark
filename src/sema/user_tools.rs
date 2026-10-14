//! User-defined tools (SPEC 22.5, OPEN_QUESTIONS Q380 to Q389): `# Tool: name` with a verb per
//! `## verb(params) -> type` block.
//!
//! A verb is checked exactly like a `# Function` of its tool's effect (its body may not exceed
//! the declared effect). It is called as a statement, `- slugger slug title -> s`, or, when the
//! tool is `pure`, as an expression, `slugger.slug(title)`. Both forms resolve to the function
//! [`fn_name`] (`slugger__slug`) that the IR builds from the verb, so the later passes see an
//! ordinary function call; the statement form is a [`ToolKind::User`](super::ToolKind::User).

use crate::diag::Diagnostic;
use crate::syntax::ast::{Decl, Effect, Expr, ExprKind, Ident, Payload, Stmt, TargetPart, Tool, ToolBody};

use super::body::{BodyKey, Cx, FnCx};
use super::effects::{effect_name, rank, EffectSet};
use super::scope::Kind;
use super::types::Type;
use super::{BlockInfo, BlockKind, Checker, NodeKey, ParamInfo, Resolution, RustInfo, Sig, ToolKind, ToolResolution};

/// A tool's name clashes with a tool of `tools.toml`, the built-in `file` tool or a binding.
pub const E_TOOL_NAME: &str = "E380";
/// One tool has two verbs of the same name.
pub const E_TOOL_VERB_TWICE: &str = "E381";
/// `tool.verb(...)` used as an expression, but the tool is not pure.
pub const E_TOOL_EXPR: &str = "E382";
/// The tool has no such verb (or the statement names none).
pub const E_TOOL_VERB: &str = "E383";
/// A tool statement written in a shape it cannot have (`:` with assignments).
pub const E_TOOL_SHAPE: &str = "E384";
/// A verb that returns nothing used as a value, or whose result is stored.
pub const E_TOOL_NO_VALUE: &str = "E385";
/// A verb says it returns a value but its body never does.
pub const E_TOOL_NO_RETURN: &str = "E386";
/// The program's default command has an argument named like a flag of every generated program.
pub const E_ARG_RESERVED: &str = "E387";
/// A tool is named like a library module (`text`, `json`, ...).
pub const W_TOOL_MODULE: &str = "W380";

/// Flags every generated program has (`--once`, ...); a default command cannot use them as argument names.
pub const RESERVED_FLAGS: [&str; 8] = ["once", "schedule", "mock", "fixtures", "trace", "serve", "help", "version"];

/// The function the IR makes of a verb: unique, and not a name an author can write.
pub fn fn_name(tool: &str, verb: &str) -> String {
    format!("{tool}__{verb}")
}

/// Where a verb is declared.
#[derive(Debug, Clone, Copy)]
pub struct VerbRef {
    pub file: usize,
    pub decl: usize,
    pub verb: usize,
}

impl<'a> Checker<'a> {
    /// The declaration of the tool called `name` (declared name) and where it is.
    fn tool_decl(&self, name: &str) -> Option<(&'a Tool, usize)> {
        let p = self.p;
        let ent = self.reg.get(Kind::Tool, name)?;
        match &p.files[ent.file].ast.decls[ent.decl] {
            Decl::Tool(t) => Some((t, ent.file)),
            _ => None,
        }
    }

    /// Names, signatures and effects of every tool (after the records are known).
    pub(super) fn declare_tools(&mut self) {
        let p = self.p;
        for (fi, f) in p.files.iter().enumerate() {
            for (di, d) in f.ast.decls.iter().enumerate() {
                let Decl::Tool(t) = d else { continue };
                // only the first declaration of a name is the tool (duplicates are E200)
                if self.reg.get(Kind::Tool, &t.name.name).is_none_or(|e| (e.file, e.decl) != (fi, di)) {
                    continue;
                }
                self.tool_name_clash(fi, t);
                for (vi, v) in t.verbs.iter().enumerate() {
                    if t.verbs[..vi].iter().any(|o| o.name.name == v.name.name) {
                        self.error(
                            E_TOOL_VERB_TWICE,
                            fi,
                            v.name.span,
                            format!("The tool \"{}\" has the verb \"{}\" twice.", t.name.name, v.name.name),
                        );
                        continue;
                    }
                    let params = v
                        .params
                        .iter()
                        .map(|a| ParamInfo {
                            name: a.name.name.clone(),
                            ty: self.resolve_type(fi, &a.ty),
                            has_default: a.default.is_some(),
                        })
                        .collect::<Vec<_>>();
                    let sig = Sig {
                        params,
                        out: v.out.as_ref().map(|o| self.resolve_type(fi, o)),
                        effect: t.effect.value,
                        file: fi,
                        span: v.name.span,
                    };
                    let name = fn_name(&t.name.name, &v.name.name);
                    if matches!(v.body, ToolBody::Rust(_)) {
                        // Layer 1: `check` lists it with the other trusted code
                        self.rust_blocks.push(RustInfo {
                            name: format!("{}.{}", t.name.name, v.name.name),
                            file: fi,
                            span: v.name.span,
                            parent: Some(t.name.name.clone()),
                            sig: sig.clone(),
                        });
                    }
                    self.sigs.insert((Kind::Tool, name.clone()), sig);
                    self.tool_verbs.insert(name, VerbRef { file: fi, decl: di, verb: vi });
                }
            }
        }
    }

    /// The command that becomes the program's default (SPEC 22.6) cannot name an argument like one of
    /// the flags every program has (Q388).
    pub(super) fn check_default_command(&mut self) {
        let p = self.p;
        let mut runs = Vec::new();
        let mut commands = Vec::new();
        for (fi, f) in p.files.iter().enumerate() {
            for d in &f.ast.decls {
                match d {
                    Decl::Run(r) => runs.extend(r.items.iter().map(|i| i.name.clone())),
                    Decl::Command(c) => commands.push((fi, c)),
                    Decl::Agent(_) | Decl::Service(_) => return,
                    _ => {}
                }
            }
        }
        let ([(fi, c)], [only]) = (commands.as_slice(), runs.as_slice()) else { return };
        if &c.name.name != only {
            return;
        }
        for a in &c.args {
            let flag = a.name.name.replace('_', "-");
            if RESERVED_FLAGS.contains(&flag.as_str()) {
                let d = Diagnostic::error(
                    E_ARG_RESERVED,
                    self.path(*fi),
                    a.name.span,
                    format!(
                        "The argument \"{}\" would be the flag \"--{flag}\", which every program already has, and this command is the program's default.",
                        a.name.name
                    ),
                )
                .with_help(format!("Rename the argument (for example \"{}_value\"); the flags {} are taken.", a.name.name, RESERVED_FLAGS.join(", ")));
                self.diags.push(d);
            }
        }
    }

    /// A tool cannot take the name of a tool that `tools.toml` declares, of the built-in `file`
    /// tool, or of a binding (they are all spelled the same way in a statement).
    fn tool_name_clash(&mut self, fi: usize, t: &Tool) {
        let name = &t.name.name;
        let declared = |this: &Self, kind: Kind| {
            this.reg
                .get(kind, name)
                .map(|e| format!("the {} \"{name}\" ({}:{})", kind.noun(), this.path(e.file), e.span.line))
        };
        let other = if name == "file" {
            Some("the built-in file tool".to_string())
        } else if self.p.manifest.tool(name).is_some() {
            Some(format!("the tool \"{name}\" of tools.toml"))
        } else {
            [Kind::Binding, Kind::Agent, Kind::Command, Kind::Service].into_iter().find_map(|k| declared(self, k))
        };
        if crate::stdlib::is_module(name) {
            self.warn(
                W_TOOL_MODULE,
                fi,
                t.name.span,
                format!("The tool \"{name}\" has the name of a library module, so \"{name}.something(...)\" is easy to misread."),
            );
            self.diags.last_mut().unwrap().help = Some(format!("A call \"{name}.f(...)\" means this tool only when it has the verb \"f\"; consider another name."));
        }
        if let Some(other) = other {
            let d = Diagnostic::error(
                E_TOOL_NAME,
                self.path(fi),
                t.name.span,
                format!("The tool \"{name}\" has the same name as {other}."),
            )
            .with_help("A tool name must be unique: rename this one.");
            self.diags.push(d);
        }
    }

    /// Effect of a call of the verb `mangled`: what its body does, and at least what the tool
    /// declares.
    pub(super) fn tool_verb_effect(&mut self, mangled: &str) -> EffectSet {
        let Some(r) = self.tool_verbs.get(mangled).copied() else {
            return EffectSet::pure();
        };
        let mut set = self.ensure_body(BodyKey::ToolVerb { file: r.file, decl: r.decl, verb: r.verb });
        if let Some(sig) = self.sigs.get(&(Kind::Tool, mangled.to_string())) {
            if rank(sig.effect) > rank(set.class()) {
                let tool = mangled.split("__").next().unwrap_or(mangled).to_string();
                set.add(sig.effect, &tool);
            }
        }
        set
    }

    /// Check the body of one verb and return its effect.
    pub(super) fn check_verb_body(&mut self, file: usize, decl: usize, verb: usize) -> EffectSet {
        let p = self.p;
        let Decl::Tool(t) = &p.files[file].ast.decls[decl] else {
            return EffectSet::pure();
        };
        let v = &t.verbs[verb];
        let mangled = fn_name(&t.name.name, &v.name.name);
        let Some(sig) = self.sigs.get(&(Kind::Tool, mangled)).cloned() else {
            return EffectSet::pure(); // a duplicate verb, reported already
        };
        let mut cx = Cx::new(file);
        cx.unit = t.name.name.clone();
        for (param, info) in v.params.iter().zip(&sig.params) {
            cx.top().vars.insert(info.name.clone(), info.ty.clone());
            cx.top().defs.insert(info.name.clone(), param.name.span);
            self.check_default(file, param, &info.ty);
        }
        let eff = match &v.body {
            ToolBody::Stmts(body) => {
                cx.func = Some(FnCx {
                    name: t.name.name.clone(),
                    out: sig.out.clone(),
                    declared: sig.effect,
                    reported: false,
                    noun: "tool",
                });
                self.stmts(&mut cx, body);
                if sig.out.is_some() && !super::body::contains_return(body) {
                    self.error(
                        E_TOOL_NO_RETURN,
                        file,
                        v.name.span,
                        format!(
                            "The verb \"{}\" of the tool \"{}\" says it returns a value but never does.",
                            v.name.name, t.name.name
                        ),
                    );
                    self.diags.last_mut().unwrap().help =
                        Some("End the body with \"- return <value>\", or remove the \"-> type\" from the heading.".into());
                }
                cx.acc.clone()
            }
            // Layer 1: the declared effect is trusted, like a `## Rust` block
            ToolBody::Rust(_) => EffectSet::of(sig.effect, &t.name.name),
        };
        self.blocks.push(BlockInfo {
            kind: BlockKind::Tool,
            file,
            span: v.name.span,
            unit: t.name.name.clone(),
            label: v.name.name.clone(),
            effect: eff.clone(),
            declared: Some(t.effect.value),
            param: None,
        });
        eff
    }

    /// `slugger slug title -> s` (a statement whose first word is a user tool).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn user_tool_call(
        &mut self,
        cx: &mut Cx,
        s: &Stmt,
        tool_name: &str,
        tool: &Ident,
        verb: &Option<Ident>,
        target: &[TargetPart],
        payload: &Option<Payload>,
        into: &Option<Ident>,
    ) {
        let sk = NodeKey::new(cx.file, s.span);
        let in_try = std::mem::take(&mut cx.in_try);
        self.require_listed(cx, tool);
        // the values written after the verb are its arguments; `: value` is the last one
        let mut args: Vec<Expr> = target
            .iter()
            .map(|t| match t {
                TargetPart::Word(w) => Expr { span: w.span, kind: ExprKind::Path(vec![w.clone()]) },
                TargetPart::Value(e) => e.clone(),
            })
            .collect();
        match payload {
            Some(Payload::Value(e)) => args.push(e.clone()),
            Some(Payload::Assign(list)) => {
                let at = list.first().map_or(tool.span, |a| a.field.span);
                let d = Diagnostic::error(
                    E_TOOL_SHAPE,
                    self.path(cx.file),
                    at,
                    format!("The tool \"{}\" takes values, not \"field = value\" assignments.", tool.name),
                )
                .with_help("Write the values after the verb: \"- name verb a b\", and one value after a colon if you like.");
                self.diags.push(d);
                for a in list {
                    self.expr(cx, &a.value);
                }
            }
            None => {}
        }
        let types: Vec<Type> = args
            .iter()
            .map(|a| {
                let t = self.expr_handle_ok(cx, a);
                self.move_out(cx, a);
                t
            })
            .collect();

        let Some((decl, vname)) = self.verb_of(cx, tool_name, tool, verb.as_ref()) else {
            if let Some(i) = into {
                cx.define(&i.name, Type::Unknown, i.span);
            }
            return;
        };
        let sig = self.sigs.get(&(Kind::Tool, decl.clone())).cloned();
        if let Some(sig) = &sig {
            self.check_args(cx, s.span, &format!("{tool_name} {vname}"), sig, &args, &types);
        }
        let out = sig.as_ref().and_then(|s| s.out.clone());
        if matches!(out, Some(Type::Result(_))) && !in_try {
            let d = Diagnostic::error(
                super::expr::E_CALL,
                self.path(cx.file),
                s.span,
                format!(
                    "The verb \"{vname}\" of \"{tool_name}\" can fail (it returns \"{}\"), so it has to be called with \"try\".",
                    out.as_ref().map(|t| t.to_string()).unwrap_or_default()
                ),
            )
            .with_help(format!("Write \"- try {tool_name} {vname} ... -> value\", optionally with an \"otherwise:\" branch."));
            self.diags.push(d);
        }
        if let Some(i) = into {
            match &out {
                Some(Type::Result(t)) => cx.define(&i.name, (**t).clone(), i.span),
                Some(t) => cx.define(&i.name, t.clone(), i.span),
                None => {
                    let d = Diagnostic::error(
                        E_TOOL_NO_VALUE,
                        self.path(cx.file),
                        i.span,
                        format!("The verb \"{vname}\" of \"{tool_name}\" returns nothing, so there is no result to store."),
                    );
                    self.diags.push(d);
                    cx.define(&i.name, Type::Unknown, i.span);
                }
            }
        }
        let eff = self.tool_verb_effect(&decl);
        cx.acc.merge(&eff);
        self.resolutions.insert(sk, Resolution::Call { kind: super::CallKind::Function, decl: decl.clone() });
        self.tool_calls.insert(
            sk,
            ToolResolution {
                tool: tool_name.to_string(),
                kind: ToolKind::User,
                on: None,
                verb: Some(vname.clone()),
                target_word: None,
                record: None,
                var: None,
                field: None,
                source: None,
                result: out.map(|t| match t {
                    Type::Result(i) => *i,
                    t => t,
                }),
            },
        );
    }

    /// The mangled function of `verb` in the tool `tool_name`, or a diagnostic.
    fn verb_of(&mut self, cx: &Cx, tool_name: &str, tool: &Ident, verb: Option<&Ident>) -> Option<(String, String)> {
        let (t, _) = self.tool_decl(tool_name)?;
        let verbs: Vec<&str> = t.verbs.iter().map(|v| v.name.name.as_str()).collect();
        let Some(v) = verb else {
            let d = Diagnostic::error(
                E_TOOL_VERB,
                self.path(cx.file),
                tool.span,
                format!("The tool {tool_name} needs a verb."),
            )
            .with_help(format!("Verbs of {tool_name}: {}.", verbs.join(", ")));
            self.diags.push(d);
            return None;
        };
        if !verbs.contains(&v.name.as_str()) {
            let d = Diagnostic::error(
                E_TOOL_VERB,
                self.path(cx.file),
                v.span,
                format!("Unknown verb \"{}\" for tool {tool_name}.", v.name),
            )
            .suggesting(&v.name, verbs.iter().copied());
            self.diags.push(d);
            return None;
        }
        Some((fn_name(tool_name, &v.name), v.name.clone()))
    }

    /// `slugger.slug(title)` in an expression. `None`: the words are not a call of a user tool
    /// (a function or a library call, or an unknown name, is for the caller to report).
    pub(super) fn user_tool_expr(&mut self, cx: &mut Cx, e: &Expr, func: &[Ident], args: &[Expr], types: &[Type]) -> Option<Type> {
        if func.len() < 2 {
            return None;
        }
        let (verb, head) = func.split_last()?;
        let written = head.iter().map(|i| i.name.as_str()).collect::<Vec<_>>().join(".");
        let (key, _) = self.scopes[cx.file].find(&written, &[Kind::Tool])?;
        let (t, _) = self.tool_decl(&key.1)?;
        if !t.verbs.iter().any(|v| v.name.name == verb.name) {
            // `text.lower(x)` is the library unless the author's `text` tool has that verb
            if func.len() == 2 && crate::stdlib::is_module(&func[0].name) {
                return None;
            }
            let _ = self.lookup(cx.file, &written, &[Kind::Tool]);
            let verbs: Vec<&str> = t.verbs.iter().map(|v| v.name.name.as_str()).collect();
            let d = Diagnostic::error(
                E_TOOL_VERB,
                self.path(cx.file),
                verb.span,
                format!("Unknown verb \"{}\" for tool {written}.", verb.name),
            )
            .suggesting(&verb.name, verbs.iter().copied());
            self.diags.push(d);
            return Some(Type::Unknown);
        }
        let _ = self.lookup(cx.file, &written, &[Kind::Tool]);
        let name = format!("{written}.{}", verb.name);
        let decl = fn_name(&key.1, &verb.name);
        let sig = self.sigs.get(&(Kind::Tool, decl.clone())).cloned()?;
        if t.effect.value != Effect::Pure {
            let d = Diagnostic::error(
                E_TOOL_EXPR,
                self.path(cx.file),
                e.span,
                format!(
                    "The tool \"{written}\" has the effect {}, so \"{name}(...)\" cannot be used inside an expression.",
                    effect_name(t.effect.value)
                ),
            )
            .with_help(format!("Call it as a statement: \"- {written} {} ... -> value\".", verb.name));
            self.diags.push(d);
        }
        self.resolutions.insert(
            NodeKey::new(cx.file, e.span),
            Resolution::Call { kind: super::CallKind::Function, decl: decl.clone() },
        );
        self.check_args(cx, e.span, &name, &sig, args, types);
        let ty = match &sig.out {
            Some(Type::Result(inner)) => {
                let d = Diagnostic::error(
                    super::expr::E_CALL,
                    self.path(cx.file),
                    e.span,
                    format!("The verb \"{name}\" can fail (it returns \"{}\"), so it cannot be used inside an expression.", sig.out.as_ref().unwrap()),
                )
                .with_help(format!("Call it as a statement: \"- try {written} {} ... -> value\".", verb.name));
                self.diags.push(d);
                (**inner).clone()
            }
            Some(t) => t.clone(),
            None => {
                let d = Diagnostic::error(
                    E_TOOL_NO_VALUE,
                    self.path(cx.file),
                    e.span,
                    format!("The verb \"{name}\" returns nothing, so it cannot be used as a value."),
                )
                .with_help(format!("Call it as a statement instead: \"- {written} {} ...\".", verb.name));
                self.diags.push(d);
                Type::Unknown
            }
        };
        let eff = self.tool_verb_effect(&decl);
        self.touch(cx, e.span, &name, &eff);
        Some(ty)
    }
}
