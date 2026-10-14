//! Statements (SPEC 4, 13, 14, 16): control flow, model calls, guards, assignments, `try`,
//! `parallel`; effect accumulation per statement.

use crate::diag::Diagnostic;
use crate::syntax::ast::{Effect, ExprKind, Stmt, StmtKind, Trigger, TypeExpr};

use super::body::{BodyKey, Cx, Implicit, PureKind};
use super::effects::EffectSet;
use super::expr::{facts, neg_facts, path_key, E_CALL, E_MAYBE, E_TYPE, E_UNKNOWN_NAME};
use super::parallel;
use super::scope::Kind;
use super::types::Type;
use super::Checker;

pub const E_STMT: &str = "E210";
pub const E_EXCEEDS: &str = "E206";
pub const E_DO: &str = "E213";

fn terminates(body: &[Stmt]) -> bool {
    matches!(
        body.last().map(|s| &s.kind),
        Some(StmtKind::Stop | StmtKind::Return(_) | StmtKind::Respond { .. })
    )
}

impl Checker<'_> {
    pub fn stmts(&mut self, cx: &mut Cx, body: &[Stmt]) {
        for s in body {
            self.stmt(cx, s);
        }
    }

    /// Check one statement; its effect (children included) is recorded and added to the
    /// enclosing statement's.
    pub fn stmt(&mut self, cx: &mut Cx, s: &Stmt) {
        let outer = std::mem::take(&mut cx.acc);
        self.stmt_inner(cx, s);
        let eff = std::mem::replace(&mut cx.acc, outer);
        self.stmt_effects
            .insert(super::NodeKey::new(cx.file, s.span), eff.clone());
        cx.acc.merge(&eff);
    }

    /// A function body must not exceed its declared effect (SPEC 13.1). Called when the effect of
    /// the statement itself (not of its children) is known, so the error lands on the right line.
    pub(super) fn check_limit(&mut self, cx: &mut Cx, s: &Stmt) {
        let Some(f) = &mut cx.func else { return };
        if f.reported || !cx.acc.exceeds(f.declared) {
            return;
        }
        f.reported = true;
        let declared = super::effects::effect_name(f.declared);
        let name = f.name.clone();
        let have = cx.acc.detail();
        let class = super::effects::effect_name(cx.acc.class());
        let d = Diagnostic::error(
            E_EXCEEDS,
            self.path(cx.file),
            s.span,
            format!("The function \"{name}\" says \"- effect: {declared}\", but this line has the effect {have}."),
        )
        .with_help(if cx.acc.class() == Effect::Unverified {
            // `unverified` is not an effect a function can declare (SPEC 13.1)
            format!(
                "A library call nobody approved is unverified ({}). Approve the binding's \"- effect:\" line (while developing, --allow-unverified assumes the proposed effect), or move this line out of the function.",
                cx.acc.unverified.join(", ")
            )
        } else {
            format!("Change the function to \"- effect: {class}\", or move this line out of the function.")
        });
        self.diags.push(d);
    }

    fn stmt_inner(&mut self, cx: &mut Cx, s: &Stmt) {
        let in_try = std::mem::take(&mut cx.in_try);
        match &s.kind {
            StmtKind::ForEach {
                filter_rule,
                var,
                source,
                body,
            } => {
                let st = self.expr(cx, source);
                let elem = self.list_elem(cx, source.span, &st, "for each");
                if let Some(r) = filter_rule {
                    self.check_rule_use(cx, r, &elem);
                }
                self.check_limit(cx, s);
                cx.push();
                cx.top().vars.insert(var.name.clone(), elem.clone());
                cx.top().defs.insert(var.name.clone(), var.span);
                if let Type::Record(r) = &elem {
                    cx.top().bare = Some(Implicit {
                        var: var.name.clone(),
                        record: r.clone(),
                    });
                }
                self.stmts(cx, body);
                cx.pop();
            }
            StmtKind::If { cond, then, else_ } => {
                let t = self.expr(cx, cond);
                self.want_bool(cx, cond.span, &t, "if");
                self.check_limit(cx, s);
                let (pos, neg) = (
                    self.norm_facts(cx, facts(cond)),
                    self.norm_facts(cx, neg_facts(cond)),
                );
                cx.push();
                cx.narrow(&pos);
                self.stmts(cx, then);
                cx.pop();
                if !else_.is_empty() {
                    cx.push();
                    cx.narrow(&neg);
                    self.stmts(cx, else_);
                    cx.pop();
                }
                if terminates(then) {
                    cx.narrow(&neg);
                } else if !else_.is_empty() && terminates(else_) {
                    cx.narrow(&pos);
                }
            }
            StmtKind::IfSome {
                expr,
                bind,
                then,
                else_,
            } => {
                let t = self.expr(cx, expr);
                let inner = match &t {
                    Type::Maybe(i) => (**i).clone(),
                    Type::Unknown => Type::Unknown,
                    other => {
                        let d = Diagnostic::error(
                            E_MAYBE,
                            self.path(cx.file),
                            expr.span,
                            format!(
                                "This is {}, which is never empty, so \"is some\" is not needed.",
                                other.with_article()
                            ),
                        )
                        .with_help("\"is some\" unwraps values declared \"maybe\".");
                        self.diags.push(d);
                        Type::Unknown
                    }
                };
                self.check_limit(cx, s);
                cx.push();
                cx.top().vars.insert(bind.name.clone(), inner);
                cx.top().defs.insert(bind.name.clone(), bind.span);
                if let Some(k) = path_key(expr) {
                    let k = self.norm_key(cx, &k);
                    cx.narrow(&[k]);
                }
                self.stmts(cx, then);
                cx.pop();
                if !else_.is_empty() {
                    cx.push();
                    self.stmts(cx, else_);
                    cx.pop();
                }
            }
            StmtKind::Stop => {
                if cx.func.is_some() {
                    let d = Diagnostic::error(
                        E_STMT,
                        self.path(cx.file),
                        s.span,
                        "\"stop\" ends a block, but this is a function.",
                    )
                    .with_help("Use \"return <value>\" to leave a function.");
                    self.diags.push(d);
                }
            }
            StmtKind::Do { name, args, into } => self.do_stmt(cx, s, name, args, into, in_try),
            StmtKind::AskModel {
                with,
                prompt,
                as_record,
                into,
            } => {
                if let Some(w) = with {
                    self.expr(cx, w);
                }
                let pt = self.expr(cx, prompt);
                if !matches!(pt, Type::Text | Type::Unknown) {
                    let d = Diagnostic::error(
                        E_TYPE,
                        self.path(cx.file),
                        prompt.span,
                        format!(
                            "The prompt must be text, but this is {}.",
                            pt.with_article()
                        ),
                    );
                    self.diags.push(d);
                }
                let result = match as_record {
                    Some(r) => {
                        let te = TypeExpr {
                            kind: crate::syntax::ast::TypeKind::Named(r.name.clone()),
                            span: r.span,
                        };
                        let t = self.resolve_type(cx.file, &te);
                        if !matches!(t, Type::Record(_) | Type::Unknown) {
                            let d = Diagnostic::error(
                                E_TYPE,
                                self.path(cx.file),
                                r.span,
                                format!("\"as\" needs a record the model's answer is checked against, but \"{}\" is {t}.", r.name),
                            );
                            self.diags.push(d);
                            Type::Unknown
                        } else {
                            t
                        }
                    }
                    None => Type::Text,
                };
                cx.define(&into.name, result, into.span);
                cx.acc.add(Effect::Model, "model");
                self.check_limit(cx, s);
            }
            StmtKind::ToolCall { .. } => {
                self.tool_call(cx, s);
                self.check_limit(cx, s);
            }
            StmtKind::Guard { cond, else_ } => {
                let saved = cx.pure_ctx.replace(PureKind::Guard);
                let t = self.expr(cx, cond);
                cx.pure_ctx = saved;
                self.want_bool(cx, cond.span, &t, "guard");
                if let Some(e) = else_ {
                    self.stmt(cx, e);
                }
                let f = self.norm_facts(cx, facts(cond));
                cx.narrow(&f);
                self.check_limit(cx, s);
            }
            StmtKind::Set { var, expr } => {
                let t = self.expr(cx, expr);
                match cx.var(&var.name).cloned() {
                    Some(old) => {
                        if !self.is_assignable(&t, &old) {
                            let d = Diagnostic::error(
                                E_TYPE,
                                self.path(cx.file),
                                expr.span,
                                format!(
                                    "\"{}\" holds {old}, but this is {}.{}",
                                    var.name,
                                    t.with_article(),
                                    self.note(&t, &old)
                                ),
                            );
                            self.diags.push(d);
                        }
                    }
                    None => cx.define(&var.name, t, var.span),
                }
                cx.forget(&var.name);
                self.check_limit(cx, s);
            }
            StmtKind::Return(e) => self.return_stmt(cx, s, e),
            StmtKind::Respond { body, .. } => {
                if !cx.route {
                    let d = Diagnostic::error(
                        E_STMT,
                        self.path(cx.file),
                        s.span,
                        "\"respond\" only works inside a service route.",
                    )
                    .with_help("Routes are the \"## GET /path\" blocks of a \"# Service\".");
                    self.diags.push(d);
                }
                if let Some(b) = body {
                    self.expr(cx, b);
                }
            }
            StmtKind::ReadBody { ty, into } => {
                if !cx.route {
                    let d = Diagnostic::error(
                        E_STMT,
                        self.path(cx.file),
                        s.span,
                        "\"read body\" only works inside a service route.",
                    );
                    self.diags.push(d);
                }
                let t = self.resolve_type(cx.file, ty);
                cx.define(&into.name, t, into.span);
                self.check_limit(cx, s);
            }
            StmtKind::ReadFile { path, ty, into } => {
                let pt = self.expr(cx, path);
                if !matches!(pt, Type::Text | Type::Unknown) {
                    let d = Diagnostic::error(
                        E_TYPE,
                        self.path(cx.file),
                        path.span,
                        format!(
                            "A file path must be text, but this is {}.",
                            pt.with_article()
                        ),
                    );
                    self.diags.push(d);
                }
                let t = self.resolve_type(cx.file, ty);
                cx.define(&into.name, t, into.span);
                cx.acc.add(Effect::Local, "file");
                self.check_limit(cx, s);
            }
            StmtKind::Append { value, list } => {
                let vt = self.expr(cx, value);
                let lt = self.expr(cx, list);
                match &lt {
                    Type::List(e) => {
                        if !self.is_assignable(&vt, e) {
                            let d = Diagnostic::error(
                                E_TYPE,
                                self.path(cx.file),
                                value.span,
                                format!(
                                    "This list holds {e}, but this is {}.{}",
                                    vt.with_article(),
                                    self.note(&vt, e)
                                ),
                            );
                            self.diags.push(d);
                        }
                    }
                    Type::Unknown => {}
                    other => {
                        let d = Diagnostic::error(
                            E_TYPE,
                            self.path(cx.file),
                            list.span,
                            format!(
                                "\"append\" needs a list, but this is {}.",
                                other.with_article()
                            ),
                        );
                        self.diags.push(d);
                    }
                }
                if let ExprKind::Path(segs) = &list.kind {
                    if segs[0].name == "state" {
                        cx.acc.add(Effect::Local, "state");
                    }
                }
                self.check_limit(cx, s);
            }
            StmtKind::Coll(c) => self.coll_stmt(cx, s, c),
            StmtKind::Print(e) => {
                self.expr(cx, e);
                cx.acc.add(Effect::Local, "print");
                self.check_limit(cx, s);
            }
            StmtKind::Try { stmt, otherwise } => {
                cx.in_try = true;
                self.stmt(cx, stmt);
                cx.in_try = false;
                if let Some(o) = otherwise {
                    cx.push();
                    self.stmts(cx, o);
                    cx.pop();
                }
            }
            StmtKind::Parallel(body) => {
                self.stmts(cx, body);
                let diags = parallel::conflicts(self.path(cx.file), body);
                self.diags.extend(diags);
            }
            StmtKind::ModelCall { .. } | StmtKind::Trace(_) => {
                unreachable!("L2-only node in sema")
            }
        }
    }

    fn return_stmt(&mut self, cx: &mut Cx, s: &Stmt, e: &crate::syntax::ast::Expr) {
        let t = self.expr(cx, e);
        let Some(f) = &cx.func else {
            let d = Diagnostic::error(
                E_STMT,
                self.path(cx.file),
                s.span,
                "\"return\" only works inside a function.",
            )
            .with_help("Blocks end with \"stop\"; routes answer with \"respond\".");
            self.diags.push(d);
            return;
        };
        let Some(out) = f.out.clone().map(|o| match o {
            // `return v` in a `result of T` function wraps `v` in Ok (Q200)
            Type::Result(t) => *t,
            o => o,
        }) else {
            let d = Diagnostic::error(
                E_STMT,
                self.path(cx.file),
                s.span,
                format!(
                    "The function \"{}\" has no \"- out:\", so it cannot return a value.",
                    f.name
                ),
            )
            .with_help("Add \"- out: <type>\" to the function.");
            self.diags.push(d);
            return;
        };
        if !self.is_assignable(&t, &out) {
            if let Type::Maybe(inner) = &t {
                if self.is_assignable(inner, &out) {
                    let msg = if matches!(e.kind, ExprKind::FirstIn { .. }) {
                        format!(
                            "This can find nothing, but the function says it returns \"{out}\"."
                        )
                    } else {
                        format!(
                            "This can be empty ({t}), but the function says it returns \"{out}\"."
                        )
                    };
                    let d = Diagnostic::error(E_MAYBE, self.path(cx.file), e.span, msg).with_help(format!(
                        "Declare the result as \"maybe {out}\" and check it with \"is some\" or \"is not empty\" before using it."
                    ));
                    self.diags.push(d);
                    self.check_limit(cx, s);
                    return;
                }
            }
            let d = Diagnostic::error(
                E_TYPE,
                self.path(cx.file),
                e.span,
                format!(
                    "This returns {}, but the function says it returns \"{out}\".{}",
                    t.with_article(),
                    self.note(&t, &out)
                ),
            );
            self.diags.push(d);
        }
        self.check_limit(cx, s);
    }

    fn do_stmt(
        &mut self,
        cx: &mut Cx,
        s: &Stmt,
        name: &crate::syntax::ast::Ident,
        args: &[crate::syntax::ast::Expr],
        into: &Option<crate::syntax::ast::Ident>,
        in_try: bool,
    ) {
        let p = self.p;
        // a `## Do` block of the same agent?
        let mut block_names: Vec<String> = Vec::new();
        let mut found_block = None;
        if let Some(a) = &cx.agent {
            if let crate::syntax::ast::Decl::Agent(agent) = &p.files[cx.file].ast.decls[a.decl] {
                for (bi, b) in agent.blocks.iter().enumerate() {
                    if let Trigger::Do(n) = &b.trigger {
                        block_names.push(n.name.clone());
                        if n.name == name.name {
                            found_block = Some((a.decl, bi));
                        }
                    }
                }
            }
        }
        if let Some((decl, bi)) = found_block {
            let key = BodyKey::Block {
                file: cx.file,
                decl,
                block: bi,
            };
            self.resolutions.insert(
                super::NodeKey::new(cx.file, s.span),
                super::Resolution::Call {
                    kind: super::CallKind::DoBlock,
                    decl: name.name.clone(),
                },
            );
            self.called_blocks.insert((cx.file, decl, bi));
            for a in args {
                self.expr(cx, a);
            }
            if !args.is_empty() {
                let d = Diagnostic::error(
                    E_DO,
                    self.path(cx.file),
                    s.span,
                    format!("The block \"{}\" takes no values.", name.name),
                )
                .with_help(
                    "A \"## Do\" block works on the record of the loop or event it is called from.",
                );
                self.diags.push(d);
            }
            if let Some(i) = into {
                let d = Diagnostic::error(
                    E_DO,
                    self.path(cx.file),
                    i.span,
                    format!(
                        "The block \"{}\" returns nothing, so there is no result to store.",
                        name.name
                    ),
                )
                .with_help("Use a \"# Function\" when you need a value back.");
                self.diags.push(d);
                cx.define(&i.name, Type::Unknown, i.span);
            }
            let implicit = cx.bare().cloned();
            match self.do_ctx.get(&key) {
                None => {
                    self.do_ctx.insert(key, implicit);
                }
                Some(prev) if prev.as_ref().map(|i| &i.record) != implicit.as_ref().map(|i| &i.record) => {
                    let show = |i: &Option<Implicit>| match i {
                        Some(i) => format!("\"{}\" ({})", i.var, i.record),
                        None => "no record".to_string(),
                    };
                    let d = Diagnostic::error(
                        E_DO,
                        self.path(cx.file),
                        s.span,
                        format!(
                            "The block \"{}\" works on {}, but here it is called with {}.",
                            name.name,
                            show(prev),
                            show(&implicit)
                        ),
                    )
                    .with_help("Call a \"## Do\" block from places that have the same record in front of them.");
                    self.diags.push(d);
                }
                Some(_) => {}
            }
            let eff = self.ensure_body(key);
            cx.acc.merge(&eff);
            self.check_limit(cx, s);
            return;
        }
        // a function or Rust block
        let arg_types: Vec<Type> = args.iter().map(|a| self.expr(cx, a)).collect();
        let Some((kind, decl)) =
            self.lookup(cx.file, &name.name, &[Kind::Function, Kind::RustBlock])
        else {
            let mut cands = block_names;
            cands.extend(self.scopes[cx.file].names_of(&[Kind::Function, Kind::RustBlock]));
            let d = Diagnostic::error(
                E_CALL,
                self.path(cx.file),
                name.span,
                format!("Unknown action or function \"{}\".", name.name),
            )
            .with_did_you_mean(super::scope::suggest(&name.name, &cands))
            .with_help(
                "\"do\" calls a \"## Do\" block of this agent, or a function visible in this file.",
            );
            self.diags.push(d);
            if let Some(i) = into {
                cx.define(&i.name, Type::Unknown, i.span);
            }
            let _ = E_UNKNOWN_NAME;
            return;
        };
        self.resolutions.insert(
            super::NodeKey::new(cx.file, s.span),
            super::Resolution::Call {
                kind: if kind == Kind::RustBlock {
                    super::CallKind::RustBlock
                } else {
                    super::CallKind::Function
                },
                decl: decl.clone(),
            },
        );
        if let Some(sig) = self.sigs.get(&(kind, decl.clone())).cloned() {
            self.check_args(cx, s.span, &name.name, &sig, args, &arg_types);
            let fallible = matches!(sig.out, Some(Type::Result(_)));
            if fallible && !in_try {
                let d = Diagnostic::error(
                    E_CALL,
                    self.path(cx.file),
                    s.span,
                    format!(
                        "The function \"{}\" can fail (it returns \"{}\"), so it has to be called with \"try\".",
                        name.name,
                        sig.out.as_ref().map(|t| t.to_string()).unwrap_or_default()
                    ),
                )
                .with_help(format!(
                    "Write \"- try do {}(...) -> value\", optionally with an \"otherwise:\" branch.",
                    name.name
                ));
                self.diags.push(d);
            }
            if let Some(i) = into {
                match &sig.out {
                    Some(Type::Result(t)) => cx.define(&i.name, (**t).clone(), i.span),
                    Some(t) => cx.define(&i.name, t.clone(), i.span),
                    None => {
                        let d = Diagnostic::error(
                            E_CALL,
                            self.path(cx.file),
                            i.span,
                            format!("The function \"{}\" returns nothing, so there is no result to store.", name.name),
                        );
                        self.diags.push(d);
                        cx.define(&i.name, Type::Unknown, i.span);
                    }
                }
            }
        }
        let eff: EffectSet = self.callee_effect(kind, &decl);
        cx.acc.merge(&eff);
        self.check_limit(cx, s);
    }
}
