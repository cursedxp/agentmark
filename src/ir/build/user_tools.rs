//! User-defined tools (SPEC 22.5) in the IR: every verb of a `# Tool` becomes an ordinary function
//! `<tool>__<verb>`; a Rust-bodied verb is a [`ir::RustFn`] with that name plus `_rust` and a
//! function around it. The first statement of each function is the trace line of the call
//! (`{"kind":"tool","tool":…,"verb":…,"args":{…}}`, OPEN_QUESTIONS Q385).

use std::collections::HashMap;

use super::*;
use crate::sema::user_tools::fn_name;

impl Builder<'_> {
    /// Register the verbs so calls know what they return.
    pub(super) fn collect_tool(&mut self, t: &ast::Tool) {
        for v in &t.verbs {
            let ret = v.out.as_ref().map(|o| self.ty(o));
            self.fns.insert(fn_name(&t.name.name, &v.name.name), FnInfo { callee: Callee::Fn, ret });
        }
    }

    pub(super) fn tool(&mut self, t: &ast::Tool, functions: &mut Vec<ir::Fn>, rust_fns: &mut Vec<ir::RustFn>) {
        for (vi, v) in t.verbs.iter().enumerate() {
            // a duplicate verb was an error; the first one is the verb
            if t.verbs[..vi].iter().any(|o| o.name.name == v.name.name) {
                continue;
            }
            let name = fn_name(&t.name.name, &v.name.name);
            let params = self.params(&v.params);
            self.env = vec![HashMap::new()];
            for p in &params {
                self.define(&p.name, p.ty.clone());
            }
            let ret = v.out.as_ref().map(|o| self.ty(o));
            let source = self.src(v.span.line);
            let var = |p: &ir::Param| ir::Expr {
                kind: ir::ExprKind::Var(p.name.clone()),
                ty: p.ty.clone(),
                effect: EffectSet::pure(),
            };
            let traced: Vec<(String, ir::Expr)> =
                params.iter().filter(|p| !p.ty.is_handle()).map(|p| (p.name.clone(), var(p))).collect();
            let mut body = vec![ir::Stmt {
                kind: ir::StmtKind::Effect {
                    call: ir::EffectCall::Trace(ir::TraceEvent::ToolCall {
                        tool: t.name.name.clone(),
                        verb: v.name.name.clone(),
                        args: traced,
                    }),
                    into: None,
                },
                effect: EffectSet::pure(),
                source: source.clone(),
            }];
            match &v.body {
                ast::ToolBody::Stmts(stmts) => body.extend(self.stmts(stmts)),
                ast::ToolBody::Rust(rust) => {
                    let rname = format!("{}_{}_rust", t.name.name, v.name.name);
                    let effect = EffectSet::of(t.effect.value, &t.name.name);
                    rust_fns.push(ir::RustFn {
                        name: rname.clone(),
                        params: params.clone(),
                        ret: ret.clone(),
                        effect: effect.clone(),
                        code: rust.code.clone(),
                        code_line: rust.line,
                        source: source.clone(),
                    });
                    let args: Vec<ir::Expr> = params.iter().map(var).collect();
                    let kind = match &ret {
                        Some(ty) => ir::StmtKind::Return(ir::Expr {
                            kind: ir::ExprKind::Call { callee: Callee::Rust, name: rname, args },
                            ty: ty.clone(),
                            effect: effect.clone(),
                        }),
                        None => ir::StmtKind::Effect { call: ir::EffectCall::Rust { name: rname, args }, into: None },
                    };
                    body.push(ir::Stmt { kind, effect, source: source.clone() });
                }
            }
            let mut effect = ir::union_effects(&body);
            if crate::sema::effects::rank(t.effect.value) > crate::sema::effects::rank(effect.class()) {
                effect.add(t.effect.value, &t.name.name);
            }
            functions.push(ir::Fn {
                name,
                display: Some((t.name.name.clone(), v.name.name.clone())),
                doc: v.doc.clone(),
                params,
                ret,
                effect,
                body,
                source,
            });
        }
    }

    /// `- slugger slug title -> s`: a call of the function the verb became.
    pub(super) fn user_tool_stmt(
        &mut self,
        tr: &ToolResolution,
        _tool: &Ident,
        target: &[TargetPart],
        payload: &Option<ast::Payload>,
        into: &Option<Ident>,
    ) -> ir::StmtKind {
        let name = fn_name(&tr.tool, tr.verb.as_deref().unwrap_or_default());
        let mut args: Vec<ir::Expr> = target
            .iter()
            .map(|t| match t {
                TargetPart::Word(w) => self.word_expr(w),
                TargetPart::Value(e) => self.expr(e),
            })
            .collect();
        if let Some(ast::Payload::Value(e)) = payload {
            args.push(self.expr(e));
        }
        // `result of T`: the value after `try` is a `T`
        let ret = self.fns.get(&name).and_then(|f| f.ret.clone()).map(|t| match t {
            Type::Result(i) => *i,
            t => t,
        });
        let into = into.as_ref().map(|i| {
            let ty = ret.unwrap_or(Type::Unknown);
            self.define(&i.name, ty.clone());
            ir::LocalVar { name: i.name.clone(), ty }
        });
        ir::StmtKind::Effect { call: ir::EffectCall::Fn { name, args }, into }
    }
}
