//! L2 AST (after `agent_lower`) + `Checked` -> IR (DESIGN 4.2).
//!
//! Types, effects, name resolution, narrowing and tool targets all come from `Checked`; this
//! pass only re-shapes them. Constants are inlined here. Nodes that `agent_lower` made up
//! (synthetic spans) have no entry in `Checked`; their types and effects are worked out from
//! their children.

use std::collections::HashMap;

use crate::ir::{self, Callee, EffectSet, Type};
use crate::lower::{guard_text, is_synthetic, Lowered};
use crate::project::{Project, ResolvedImport};
use crate::sema::{BlockKind, CallKind, Checked, NodeKey, Resolution, ToolKind, ToolResolution};
use crate::syntax::ast::{self, Decl, Ident, StmtKind as K, TargetPart};

/// What the builder knows about a callable.
struct FnInfo {
    callee: Callee,
    ret: Option<Type>,
}

struct Builder<'a> {
    project: &'a Project,
    checked: &'a Checked,
    /// File being converted (an index into `Project::files`).
    fi: usize,
    /// Variables in scope, innermost frame last.
    env: Vec<HashMap<String, (Type, Option<ir::Expr>)>>,
    fns: HashMap<String, FnInfo>,
    /// Record of the project's service state (commands use it, Q9).
    state_record: Option<String>,
}

/// Build the IR of a checked, lowered project.
pub fn build(project: &Project, checked: &Checked, lowered: &Lowered) -> ir::Program {
    let mut b = Builder {
        project,
        checked,
        fi: 0,
        env: Vec::new(),
        fns: HashMap::new(),
        state_record: None,
    };
    b.collect(lowered);
    let mut p = ir::Program {
        package: ir::Package {
            name: project.config.name.clone(),
            version: project.config.version.clone(),
        },
        records: Vec::new(),
        functions: Vec::new(),
        entries: Vec::new(),
        services: Vec::new(),
        commands: Vec::new(),
        rust_fns: Vec::new(),
        bindings: Vec::new(),
        crates: Vec::new(),
        tools: checked.manifest.tools.values().cloned().collect(),
        run: Vec::new(),
    };
    for (fi, file) in lowered.files.iter().enumerate() {
        b.fi = fi;
        for d in &file.decls {
            match d {
                Decl::Record(r) => p.records.push(b.record(r)),
                Decl::Function(f) => p.functions.push(b.function(f)),
                Decl::Entry(e) => p.entries.push(b.entry(e)),
                Decl::Service(s) => p.services.push(b.service(s)),
                Decl::Command(c) => p.commands.push(b.command(c)),
                Decl::RustBlock(r) => p.rust_fns.push(b.rust_fn(r)),
                Decl::Binding(bi) => b.binding(bi, &mut p.bindings),
                Decl::Run(r) => p.run.extend(r.items.iter().map(|i| i.name.clone())),
                Decl::Constant(_) => {} // inlined at every use
                Decl::Agent(_) | Decl::Rule(_) => {
                    panic!("internal compiler error: agent_lower left an agent or rule behind")
                }
            }
        }
        b.crates(fi, &mut p.crates);
    }
    p
}

fn join(segs: &[Ident]) -> String {
    segs.iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

impl<'a> Builder<'a> {
    // ------------------------------------------------------------------ small helpers

    fn nk(&self, span: crate::diag::Span) -> NodeKey {
        NodeKey::new(self.fi, span)
    }

    fn src(&self, line: u32) -> ir::SourceRef {
        ir::SourceRef {
            file: self.checked.files[self.fi].clone(),
            line,
        }
    }

    fn ty(&self, te: &ast::TypeExpr) -> Type {
        match &te.kind {
            ast::TypeKind::Named(n) => Type::builtin(n).unwrap_or_else(|| Type::Record(n.clone())),
            ast::TypeKind::List(i) => Type::List(Box::new(self.ty(i))),
            ast::TypeKind::Maybe(i) => Type::Maybe(Box::new(self.ty(i))),
            ast::TypeKind::Result(i) => Type::Result(Box::new(self.ty(i))),
        }
    }

    fn ety(&self, e: &ast::Expr) -> Option<Type> {
        self.checked.expr_types.get(&self.nk(e.span)).cloned()
    }

    fn push(&mut self) {
        self.env.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.env.pop();
    }

    fn define(&mut self, name: &str, ty: Type) {
        if let Some(f) = self.env.last_mut() {
            f.insert(name.to_string(), (ty, None));
        }
    }

    /// Declare the result of a model call together with what the model was given, for
    /// `no amounts changed in x` (the input lives and dies with the binding, Q93).
    fn define_model(&mut self, name: &str, ty: Type, input: ir::Expr) {
        if let Some(f) = self.env.last_mut() {
            f.insert(name.to_string(), (ty, Some(input)));
        }
    }

    fn lookup(&self, name: &str) -> Option<&Type> {
        self.env.iter().rev().find_map(|f| f.get(name)).map(|(t, _)| t)
    }

    fn model_input(&self, name: &str) -> Option<ir::Expr> {
        self.env
            .iter()
            .rev()
            .find_map(|f| f.get(name))
            .and_then(|(_, i)| i.clone())
    }

    /// `set x = …` on an existing variable: whatever the model was given no longer describes it.
    fn forget_input(&mut self, name: &str) {
        if let Some(entry) = self.env.iter_mut().rev().find_map(|f| f.get_mut(name)) {
            entry.1 = None;
        }
    }

    fn field_ty(&self, ty: &Type, field: &str) -> Type {
        match ty {
            Type::Record(r) => self
                .checked
                .records
                .get(r)
                .and_then(|rec| rec.fields.iter().find(|f| f.name == field))
                .map(|f| f.ty.clone())
                .unwrap_or(Type::Unknown),
            Type::Object(fs) => fs
                .iter()
                .find(|(n, _)| n == field)
                .map(|(_, t)| t.clone())
                .unwrap_or(Type::Unknown),
            _ => Type::Unknown,
        }
    }

    /// Every function and Rust block, so calls know what they call and what it returns.
    fn collect(&mut self, lowered: &Lowered) {
        for (fi, file) in lowered.files.iter().enumerate() {
            self.fi = fi;
            for d in &file.decls {
                match d {
                    Decl::Function(f) => {
                        let ret = f.out.as_ref().map(|t| self.ty(t));
                        self.fns.insert(
                            f.name.name.clone(),
                            FnInfo {
                                callee: Callee::Fn,
                                ret,
                            },
                        );
                    }
                    Decl::RustBlock(r) => {
                        let ret = r.out.as_ref().map(|t| self.ty(t));
                        self.fns.insert(
                            r.name.name.clone(),
                            FnInfo {
                                callee: Callee::Rust,
                                ret,
                            },
                        );
                    }
                    Decl::Service(s) => {
                        if let (Some(st), None) = (&s.state, &self.state_record) {
                            self.state_record = Some(st.name.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ------------------------------------------------------------------ declarations

    fn record(&self, r: &ast::Record) -> ir::Record {
        let fields = self
            .checked
            .records
            .get(&r.name.name)
            .map(|info| {
                info.fields
                    .iter()
                    .map(|f| ir::RecordField {
                        name: f.name.clone(),
                        ty: f.ty.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        ir::Record {
            name: r.name.name.clone(),
            doc: r.doc.clone(),
            fields,
            source: self.src(r.span.line),
        }
    }

    fn params(&mut self, ps: &[ast::Param]) -> Vec<ir::Param> {
        ps.iter()
            .map(|p| ir::Param {
                name: p.name.name.clone(),
                ty: self.ty(&p.ty),
                default: p.default.as_ref().map(|d| self.expr(d)),
            })
            .collect()
    }

    fn function(&mut self, f: &ast::Function) -> ir::Fn {
        let params = self.params(&f.params);
        self.env = vec![HashMap::new()];
        for p in &params {
            self.define(&p.name, p.ty.clone());
        }
        let body = self.stmts(&f.body);
        ir::Fn {
            name: f.name.name.clone(),
            doc: f.doc.clone(),
            display: self
                .checked
                .blocks
                .iter()
                .find(|b| b.file == self.fi && b.span == f.span && b.kind == BlockKind::Action)
                .map(|b| (b.unit.clone(), b.label.clone())),
            params,
            ret: f.out.as_ref().map(|t| self.ty(t)),
            effect: ir::union_effects(&body),
            body,
            source: self.src(f.span.line),
        }
    }

    fn entry(&mut self, e: &ast::Entry) -> ir::Entry {
        self.env = vec![HashMap::new()];
        let param = e.param.as_ref().map(|p| ir::LocalVar {
            name: p.name.name.clone(),
            ty: self.ty(&p.ty),
        });
        if let Some(p) = &param {
            self.define(&p.name, p.ty.clone());
        }
        let body = self.stmts(&e.body);
        let dur = |d: &ast::DurationLit| ir::Duration {
            amount: d.amount,
            unit: d.unit,
        };
        ir::Entry {
            unit: e.unit.clone(),
            label: e.label.clone(),
            trigger: match &e.trigger {
                ast::EntryTrigger::Every(d) => ir::Trigger::Every(dur(d)),
                ast::EntryTrigger::DailyAt { hour, minute } => ir::Trigger::DailyAt {
                    hour: *hour,
                    minute: *minute,
                },
                ast::EntryTrigger::Weekly { day, hour, minute } => ir::Trigger::Weekly {
                    day: *day,
                    hour: *hour,
                    minute: *minute,
                },
                ast::EntryTrigger::Event { tool, event } => ir::Trigger::Event {
                    tool: tool.name.clone(),
                    event: event.name.clone(),
                },
                ast::EntryTrigger::Start => ir::Trigger::Start,
            },
            param,
            timeout: e.timeout.as_ref().map(dur).unwrap_or(ir::Duration {
                amount: 5,
                unit: ast::DurationUnit::Minute,
            }),
            memory_file: e.memory == ast::Memory::File,
            effect: ir::union_effects(&body),
            body,
            source: self.src(e.span.line),
        }
    }

    fn service(&mut self, s: &ast::Service) -> ir::Service {
        let state = s.state.as_ref().map(|i| i.name.clone());
        let routes = s
            .routes
            .iter()
            .map(|r| {
                self.env = vec![HashMap::new()];
                for pp in &r.path_params {
                    self.define(pp, Type::Text);
                }
                if let Some(st) = &state {
                    self.define("state", Type::Record(st.clone()));
                }
                let body = self.stmts(&r.body);
                ir::Route {
                    method: r.method.clone(),
                    path: r.path.clone(),
                    path_params: r.path_params.clone(),
                    label: format!("{} {}", r.method, r.path),
                    effect: ir::union_effects(&body),
                    body,
                    source: self.src(r.span.line),
                }
            })
            .collect();
        ir::Service {
            name: s.name.name.clone(),
            listen: s.listen.as_ref().map(|l| l.value.clone()),
            state,
            routes,
            source: self.src(s.span.line),
        }
    }

    fn command(&mut self, c: &ast::Command) -> ir::Command {
        let args = self.params(&c.args);
        self.env = vec![HashMap::new()];
        for a in &args {
            self.define(&a.name, a.ty.clone());
        }
        if let Some(st) = self.state_record.clone() {
            self.define("state", Type::Record(st));
        }
        let body = self.stmts(&c.body);
        ir::Command {
            name: c.name.name.clone(),
            args,
            state: self.state_record.clone(),
            effect: ir::union_effects(&body),
            body,
            source: self.src(c.span.line),
            run: self.src(self.run_line(c.span.line)),
        }
    }

    /// Line of the `## Run` heading after the `# Command` heading at `from` (the heading itself if
    /// it cannot be found).
    fn run_line(&self, from: u32) -> u32 {
        self.project.files[self.fi]
            .text
            .lines()
            .enumerate()
            .skip(from as usize)
            .find(|(_, l)| {
                let t = l.trim_start();
                t.len() >= 6 && t.is_char_boundary(6) && t[..6].eq_ignore_ascii_case("## run") && !t[6..].starts_with(|c: char| c.is_alphanumeric() || c == ':')
            })
            .map_or(from, |(i, _)| i as u32 + 1)
    }

    fn rust_fn(&mut self, r: &ast::RustBlock) -> ir::RustFn {
        let name = r.name.name.clone();
        let mut effect = EffectSet::pure();
        effect.add(r.effect.value, &r.name.name);
        ir::RustFn {
            params: self.params(&r.params),
            ret: r.out.as_ref().map(|t| self.ty(t)),
            effect,
            code: r.rust.code.clone(),
            code_line: r.rust.line,
            source: self.src(r.span.line),
            name,
        }
    }

    fn binding(&mut self, b: &ast::Binding, out: &mut Vec<ir::BindingFn>) {
        for item in &b.items {
            out.push(ir::BindingFn {
                binding: b.name.name.clone(),
                crate_name: b.crate_name.name.clone(),
                item: item.name.name.clone(),
                doc: item.doc.clone(),
                params: self.params(&item.params),
                ret: item.out.as_ref().map(|t| self.ty(t)),
                effect: b.effect.value,
                code: item.rust.code.clone(),
                code_line: item.rust.line,
                source: self.src(item.span.line),
            });
        }
    }

    fn crates(&self, fi: usize, out: &mut Vec<ir::CrateImport>) {
        let f = &self.project.files[fi];
        for (imp, resolved) in f.ast.imports.iter().zip(&f.imports) {
            let ast::ImportKind::Crate { name, alias } = &imp.kind else {
                continue;
            };
            if out.iter().any(|c| c.name == name.name) {
                continue;
            }
            let binding = match resolved {
                ResolvedImport::Crate {
                    binding: Some(bf), ..
                } => self.project.files[*bf].ast.decls.iter().find_map(|d| match d {
                    Decl::Binding(b) => Some(b.name.name.clone()),
                    _ => None,
                }),
                _ => None,
            };
            out.push(ir::CrateImport {
                name: name.name.clone(),
                alias: alias.as_ref().map(|a| a.name.clone()),
                binding,
            });
        }
    }

    // ------------------------------------------------------------------ statements

    fn stmts(&mut self, body: &[ast::Stmt]) -> Vec<ir::Stmt> {
        let mut out = Vec::new();
        for s in body {
            match &s.kind {
                K::Guard { cond, else_ } => out.extend(self.guard(s, cond, else_)),
                _ => out.push(self.stmt(s)),
            }
        }
        out
    }

    /// The effect of a statement: `Checked`'s for real ones, the children's for made-up ones.
    fn mk(&self, s: &ast::Stmt, kind: ir::StmtKind) -> ir::Stmt {
        let effect = if is_synthetic(s.span) {
            None
        } else {
            self.checked.stmt_effects.get(&self.nk(s.span)).cloned()
        }
        .unwrap_or_else(|| kids_effect(&kind));
        ir::Stmt {
            kind,
            effect,
            source: self.src(s.span.line),
        }
    }

    /// A guard outside an agent expands exactly as `agent_lower` does inside one (Q90):
    /// `let tmp = cond; trace; if not tmp { else | stop }`.
    fn guard(&mut self, s: &ast::Stmt, cond: &ast::Expr, else_: &Option<Box<ast::Stmt>>) -> Vec<ir::Stmt> {
        let name = guard_text(self.checked, self.fi, cond);
        let c = self.expr(cond);
        let tmp = format!("guard_ok_{}", s.span.line);
        let src = self.src(s.span.line);
        let mkv = || ir::Expr {
            kind: ir::ExprKind::Var(tmp.clone()),
            ty: Type::Bool,
            effect: EffectSet::pure(),
        };
        self.define(&tmp, Type::Bool);
        let then = match else_ {
            Some(e) => vec![self.stmt(e)],
            None => vec![ir::Stmt {
                kind: ir::StmtKind::Stop,
                effect: EffectSet::pure(),
                source: src.clone(),
            }],
        };
        let pure_stmt = |kind| ir::Stmt {
            kind,
            effect: EffectSet::pure(),
            source: src.clone(),
        };
        let mut out = vec![
            ir::Stmt {
                effect: c.effect.clone(),
                kind: ir::StmtKind::Let {
                    name: tmp.clone(),
                    ty: Type::Bool,
                    value: c,
                },
                source: src.clone(),
            },
            pure_stmt(ir::StmtKind::Effect {
                call: ir::EffectCall::Trace(ir::TraceEvent::Guard { name, ok: mkv() }),
                into: None,
            }),
        ];
        let mut cond_stmt = pure_stmt(ir::StmtKind::If {
            cond: ir::Expr {
                kind: ir::ExprKind::Not(Box::new(mkv())),
                ty: Type::Bool,
                effect: EffectSet::pure(),
            },
            then,
            else_: Vec::new(),
        });
        cond_stmt.effect = kids_effect(&cond_stmt.kind);
        out.push(cond_stmt);
        out
    }

    fn scoped(&mut self, body: &[ast::Stmt]) -> Vec<ir::Stmt> {
        self.push();
        let out = self.stmts(body);
        self.pop();
        out
    }

    fn stmt(&mut self, s: &ast::Stmt) -> ir::Stmt {
        let kind = match &s.kind {
            K::ForEach {
                var, source, body, ..
            } => {
                let source = self.expr(source);
                let elem = match &source.ty {
                    Type::List(t) => (**t).clone(),
                    _ => Type::Unknown,
                };
                self.push();
                self.define(&var.name, elem.clone());
                let body = self.stmts(body);
                self.pop();
                ir::StmtKind::ForEach {
                    var: ir::LocalVar {
                        name: var.name.clone(),
                        ty: elem,
                    },
                    source,
                    body,
                }
            }
            K::If { cond, then, else_ } => {
                let cond = self.expr(cond);
                ir::StmtKind::If {
                    cond,
                    then: self.scoped(then),
                    else_: self.scoped(else_),
                }
            }
            K::IfSome {
                expr,
                bind,
                then,
                else_,
            } => {
                let expr = self.expr(expr);
                let inner = match &expr.ty {
                    Type::Maybe(i) => (**i).clone(),
                    other => other.clone(),
                };
                self.push();
                self.define(&bind.name, inner.clone());
                let then = self.stmts(then);
                self.pop();
                ir::StmtKind::IfSome {
                    expr,
                    bind: ir::LocalVar {
                        name: bind.name.clone(),
                        ty: inner,
                    },
                    then,
                    else_: self.scoped(else_),
                }
            }
            K::Stop => ir::StmtKind::Stop,
            K::Do { name, args, into } => self.do_stmt(s, name, args, into),
            K::ModelCall {
                with,
                prompt,
                schema,
                into,
                model,
            } => {
                let with = with.as_ref().map(|w| self.expr(w));
                let prompt = self.expr(prompt);
                let ty = match schema {
                    Some(r) => Type::Record(r.name.clone()),
                    None => Type::Text,
                };
                let input = with.clone().unwrap_or_else(|| prompt.clone());
                self.define_model(&into.name, ty.clone(), input);
                ir::StmtKind::Effect {
                    call: ir::EffectCall::Model(ir::ModelCall {
                        prompt,
                        context: with,
                        schema: schema.as_ref().map(|r| r.name.clone()),
                        model: model.clone(),
                    }),
                    into: Some(ir::LocalVar {
                        name: into.name.clone(),
                        ty,
                    }),
                }
            }
            K::AskModel { .. } => {
                panic!("internal compiler error: agent_lower left an `ask model` behind")
            }
            K::ToolCall {
                tool,
                verb,
                target,
                payload,
                into,
                ..
            } => self.tool_call(s, tool, verb, target, payload, into),
            K::Guard { .. } => unreachable!("guards are expanded by `stmts`"),
            K::Trace(ast::TraceEvent::Guard { name, ok }) => ir::StmtKind::Effect {
                call: ir::EffectCall::Trace(ir::TraceEvent::Guard {
                    name: name.clone(),
                    ok: self.expr(ok),
                }),
                into: None,
            },
            K::Set { var, expr } => {
                let value = self.expr(expr);
                match self.lookup(&var.name).cloned() {
                    Some(ty) => {
                        self.forget_input(&var.name);
                        ir::StmtKind::Assign {
                            name: var.name.clone(),
                            ty,
                            value,
                        }
                    }
                    None => {
                        self.define(&var.name, value.ty.clone());
                        ir::StmtKind::Let {
                            name: var.name.clone(),
                            ty: value.ty.clone(),
                            value,
                        }
                    }
                }
            }
            K::Return(e) => ir::StmtKind::Return(self.expr(e)),
            K::Respond { status, body } => ir::StmtKind::Respond {
                status: *status,
                body: body.as_ref().map(|b| self.expr(b)),
            },
            K::ReadBody { ty, into } => {
                let ty = self.ty(ty);
                self.define(&into.name, ty.clone());
                ir::StmtKind::Effect {
                    call: ir::EffectCall::Io(ir::IoCall::ReadBody { ty: ty.clone() }),
                    into: Some(ir::LocalVar {
                        name: into.name.clone(),
                        ty,
                    }),
                }
            }
            K::ReadFile { path, ty, into } => {
                let path = self.expr(path);
                let ty = self.ty(ty);
                self.define(&into.name, ty.clone());
                ir::StmtKind::Effect {
                    call: ir::EffectCall::Io(ir::IoCall::ReadFile {
                        path,
                        ty: ty.clone(),
                    }),
                    into: Some(ir::LocalVar {
                        name: into.name.clone(),
                        ty,
                    }),
                }
            }
            K::ReadInput { into } => {
                self.define(&into.name, Type::Text);
                ir::StmtKind::Effect {
                    call: ir::EffectCall::Io(ir::IoCall::ReadInput),
                    into: Some(ir::LocalVar { name: into.name.clone(), ty: Type::Text }),
                }
            }
            K::File { verb, path, arg, into } => {
                let path = self.expr(path);
                let arg = arg.as_ref().map(|a| match a {
                    ast::FileArg::To(e) | ast::FileArg::Content(e) => self.expr(e),
                });
                let ty = match verb.name.as_str() {
                    "read" => Some(Type::Text),
                    "list" => Some(Type::List(Box::new(Type::Text))),
                    "exists" => Some(Type::Bool),
                    _ => None,
                };
                let into = match (into, ty) {
                    (Some(i), Some(ty)) => {
                        self.define(&i.name, ty.clone());
                        Some(ir::LocalVar { name: i.name.clone(), ty })
                    }
                    _ => None,
                };
                ir::StmtKind::Effect {
                    call: ir::EffectCall::File(ir::FileCall { verb: verb.name.clone(), path, arg }),
                    into,
                }
            }
            K::CallInto { call, into, .. } => {
                let value = self.expr(call);
                match value.ty.clone() {
                    Type::Result(inner) => {
                        self.define(&into.name, (*inner).clone());
                        ir::StmtKind::Effect {
                            call: ir::EffectCall::Std(value),
                            into: Some(ir::LocalVar { name: into.name.clone(), ty: *inner }),
                        }
                    }
                    ty => {
                        self.define(&into.name, ty.clone());
                        ir::StmtKind::Let { name: into.name.clone(), ty, value }
                    }
                }
            }
            K::Append { value, list } => ir::StmtKind::Append {
                value: self.expr(value),
                list: self.expr(list),
            },
            K::Print(e) => ir::StmtKind::Effect {
                call: ir::EffectCall::Io(ir::IoCall::Print {
                    message: self.expr(e),
                }),
                into: None,
            },
            K::Try { stmt, otherwise } => {
                let inner = self.stmt(stmt);
                let defines = match &inner.kind {
                    ir::StmtKind::Effect { into: Some(v), .. } => Some(v.clone()),
                    ir::StmtKind::Let { name, ty, .. } => Some(ir::LocalVar {
                        name: name.clone(),
                        ty: ty.clone(),
                    }),
                    _ => None,
                };
                ir::StmtKind::Try {
                    stmt: Box::new(inner),
                    otherwise: otherwise.as_ref().map(|o| self.scoped(o)),
                    defines,
                }
            }
            K::Parallel(body) => ir::StmtKind::Parallel(self.stmts(body)),
        };
        self.mk(s, kind)
    }

    fn do_stmt(
        &mut self,
        s: &ast::Stmt,
        name: &Ident,
        args: &[ast::Expr],
        into: &Option<Ident>,
    ) -> ir::StmtKind {
        let res = if is_synthetic(s.span) {
            None
        } else {
            self.checked.resolutions.get(&self.nk(s.span)).cloned()
        };
        let (fname, callee) = match res {
            Some(Resolution::Call {
                kind: CallKind::RustBlock,
                decl,
            }) => (decl.clone(), Callee::Rust),
            Some(Resolution::Call {
                kind: CallKind::Function,
                decl,
            }) => (decl.clone(), Callee::Fn),
            // a former `## Do` block: agent_lower already wrote its function name
            _ => {
                let n = name.name.clone();
                let c = self.fns.get(&n).map(|f| f.callee).unwrap_or(Callee::Fn);
                (n, c)
            }
        };
        let args: Vec<ir::Expr> = args.iter().map(|a| self.expr(a)).collect();
        // `result of T`: the value after `try` is a `T`
        let ret = self.fns.get(&fname).and_then(|f| f.ret.clone()).map(|t| match t {
            Type::Result(i) => *i,
            t => t,
        });
        let into = into.as_ref().map(|i| {
            let ty = ret.unwrap_or(Type::Unknown);
            self.define(&i.name, ty.clone());
            ir::LocalVar {
                name: i.name.clone(),
                ty,
            }
        });
        let call = match callee {
            Callee::Fn => ir::EffectCall::Fn { name: fname, args },
            Callee::Rust => ir::EffectCall::Rust { name: fname, args },
        };
        ir::StmtKind::Effect { call, into }
    }

    fn tool_call(
        &mut self,
        s: &ast::Stmt,
        tool: &Ident,
        verb: &Option<Ident>,
        target: &[TargetPart],
        payload: &Option<ast::Payload>,
        into: &Option<Ident>,
    ) -> ir::StmtKind {
        let tr = self
            .checked
            .tool_calls
            .get(&self.nk(s.span))
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "internal compiler error: tool call at line {} was not checked",
                    s.span.line
                )
            });
        let verb_name = verb.as_ref().map(|v| v.name.clone());
        let (call, ty) = match tr.kind {
            ToolKind::Binding => {
                let args: Vec<ir::Expr> = target
                    .iter()
                    .map(|t| {
                        let (span, x) = match t {
                            TargetPart::Word(w) => (w.span, self.word_expr(w)),
                            TargetPart::Value(e) => (e.span, self.expr(e)),
                        };
                        if self.checked.text_to_bytes.contains(&self.nk(span)) {
                            let effect = x.effect.clone();
                            ir::Expr { kind: ir::ExprKind::TextToBytes(Box::new(x)), ty: Type::Bytes, effect }
                        } else {
                            x
                        }
                    })
                    .collect();
                let ret = tr.result.clone().unwrap_or(Type::Unknown);
                (
                    ir::EffectCall::Binding {
                        binding: tr.tool.clone(),
                        item: verb_name.unwrap_or_default(),
                        args,
                    },
                    ret,
                )
            }
            ToolKind::Manifest => {
                let def = self
                    .checked
                    .manifest
                    .tool(&tool.name)
                    .cloned()
                    .unwrap_or_else(|| panic!("internal compiler error: unknown tool {}", tool.name));
                let tgt = self.target(&tr, target);
                let pay = payload.as_ref().map(|p| match p {
                    ast::Payload::Assign(list) => ir::Payload::Assign(
                        list.iter()
                            .map(|a| (a.field.name.clone(), self.expr(&a.value)))
                            .collect(),
                    ),
                    ast::Payload::Value(e) => ir::Payload::Value(self.expr(e)),
                });
                let ty = tr.result.clone().unwrap_or(Type::Unknown);
                (
                    ir::EffectCall::Tool(ir::ToolCall {
                        tool: def.name.clone(),
                        verb: verb_name,
                        target: tgt,
                        payload: pay,
                    }),
                    ty,
                )
            }
        };
        let into = into.as_ref().map(|i| {
            self.define(&i.name, ty.clone());
            ir::LocalVar {
                name: i.name.clone(),
                ty: ty.clone(),
            }
        });
        ir::StmtKind::Effect { call, into }
    }

    /// A word after a verb that names a value (`crm get invoice id`).
    fn word_expr(&mut self, w: &Ident) -> ir::Expr {
        let e = ast::Expr {
            span: w.span,
            kind: ast::ExprKind::Path(vec![w.clone()]),
        };
        self.expr(&e)
    }

    fn target(
        &mut self,
        tr: &ToolResolution,
        parts: &[TargetPart],
    ) -> Option<ir::Target> {
        let text = |s: &str| ir::Expr {
            kind: ir::ExprKind::Str(s.to_string()),
            ty: Type::Text,
            effect: EffectSet::pure(),
        };
        match parts.first()? {
            TargetPart::Value(e) => Some(ir::Target::Literal {
                word: None,
                value: self.expr(e),
            }),
            TargetPart::Word(w) => {
                let source = tr.source.clone();
                let field_of = |this: &Self, var: &str, field: &str| {
                    let base = ir::Expr {
                        kind: ir::ExprKind::Var(var.to_string()),
                        ty: this.lookup(var).cloned().unwrap_or(Type::Unknown),
                        effect: EffectSet::pure(),
                    };
                    let fty = this.field_ty(&base.ty, field);
                    ir::Expr {
                        kind: ir::ExprKind::Field {
                            base: Box::new(base),
                            field: field.to_string(),
                        },
                        ty: fty,
                        effect: EffectSet::pure(),
                    }
                };
                match parts.get(1) {
                    None => {
                        if let Some(source) = source {
                            let (id, id_from) = match (&tr.var, &tr.field) {
                                (Some(v), Some(f)) => {
                                    (Some(field_of(self, v, f)), Some((v.clone(), f.clone())))
                                }
                                _ => (None, None),
                            };
                            return Some(ir::Target::Source {
                                word: w.name.clone(),
                                source,
                                record: tr.record.clone(),
                                id,
                                id_from,
                            });
                        }
                        if let (Some(v), Some(f)) = (&tr.var, &tr.field) {
                            return Some(ir::Target::Field {
                                word: w.name.clone(),
                                var: v.clone(),
                                field: f.clone(),
                                value: field_of(self, v, f),
                            });
                        }
                        Some(ir::Target::Literal {
                            word: Some(w.name.clone()),
                            value: text(&w.name),
                        })
                    }
                    Some(second) => {
                        let value = match second {
                            TargetPart::Word(w2) => self.word_expr(w2),
                            TargetPart::Value(e) => self.expr(e),
                        };
                        match source {
                            Some(source) => Some(ir::Target::Source {
                                word: w.name.clone(),
                                source,
                                record: tr.record.clone(),
                                id: Some(value),
                                id_from: None,
                            }),
                            None => Some(ir::Target::Literal {
                                word: Some(w.name.clone()),
                                value,
                            }),
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------ expressions

    fn mke(&self, e: &ast::Expr, kind: ir::ExprKind, ty: Type) -> ir::Expr {
        let effect = if is_synthetic(e.span) {
            None
        } else {
            self.checked.expr_effects.get(&self.nk(e.span)).cloned()
        }
        .unwrap_or_else(|| expr_kids_effect(&kind));
        ir::Expr { kind, ty, effect }
    }

    fn expr(&mut self, e: &ast::Expr) -> ir::Expr {
        use ast::ExprKind as X;
        let known = self.ety(e);
        match &e.kind {
            X::Str(s) => self.mke(e, ir::ExprKind::Str(s.clone()), Type::Text),
            X::Int(n) => self.mke(e, ir::ExprKind::Int(*n), Type::Int),
            X::Money { minor, currency } => self.mke(
                e,
                ir::ExprKind::Money {
                    minor: *minor,
                    currency: currency.clone(),
                },
                Type::Money,
            ),
            X::Duration(d) => self.mke(
                e,
                ir::ExprKind::Duration(ir::Duration {
                    amount: d.amount,
                    unit: d.unit,
                }),
                Type::Duration,
            ),
            X::Date(d) => self.mke(e, ir::ExprKind::Date(d.clone()), Type::Date),
            X::Bool(b) => self.mke(e, ir::ExprKind::Bool(*b), Type::Bool),
            X::Today => self.mke(e, ir::ExprKind::Today, Type::Date),
            X::Now => self.mke(e, ir::ExprKind::Now, Type::DateTime),
            X::Path(segs) => self.path(e, segs),
            X::Interp(parts) => {
                let parts = parts
                    .iter()
                    .map(|p| match p {
                        ast::InterpPart::Text(t) => ir::InterpPart::Text(t.clone()),
                        ast::InterpPart::Expr(x) => ir::InterpPart::Expr(self.expr(x)),
                    })
                    .collect();
                self.mke(e, ir::ExprKind::Interp(parts), Type::Text)
            }
            X::Binary { op, lhs, rhs } => {
                let (l, r) = (self.expr(lhs), self.expr(rhs));
                let ty = known.unwrap_or_else(|| match op {
                    ast::BinOp::Add | ast::BinOp::Sub | ast::BinOp::Mul | ast::BinOp::Div => {
                        l.ty.clone()
                    }
                    _ => Type::Bool,
                });
                self.mke(
                    e,
                    ir::ExprKind::Binary {
                        op: *op,
                        lhs: Box::new(l),
                        rhs: Box::new(r),
                    },
                    ty,
                )
            }
            X::Not(x) => {
                let x = self.expr(x);
                self.mke(e, ir::ExprKind::Not(Box::new(x)), Type::Bool)
            }
            X::Call { func, args } => self.call(e, func, args, known),
            X::Decode { call, ty } => {
                let ast::ExprKind::Call { func, args } = &call.kind else { unreachable!("the parser makes Decode around a call") };
                let decode = self.ty(ty);
                let args: Vec<ir::Expr> = args.iter().map(|a| self.expr(a)).collect();
                let ty = known.unwrap_or_else(|| Type::Result(Box::new(decode.clone())));
                self.mke(
                    e,
                    ir::ExprKind::Std {
                        module: func[0].name.clone(),
                        name: func[1].name.clone(),
                        args,
                        decode: Some(decode),
                    },
                    ty,
                )
            }
            X::Record(fields) => {
                let fields: Vec<(String, ir::Expr)> = fields
                    .iter()
                    .map(|f| (f.name.name.clone(), self.expr(&f.value)))
                    .collect();
                let ty = known.unwrap_or_else(|| {
                    Type::Object(fields.iter().map(|(n, x)| (n.clone(), x.ty.clone())).collect())
                });
                self.mke(e, ir::ExprKind::Record(fields), ty)
            }
            X::CountOf { rule, source, .. } => {
                let source = self.expr(source);
                self.mke(
                    e,
                    ir::ExprKind::Count {
                        source: Box::new(source),
                        filter: rule.as_ref().map(|r| r.name.clone()),
                    },
                    Type::Int,
                )
            }
            X::FirstIn { var, source, cond } => {
                let source = self.expr(source);
                let elem = match &source.ty {
                    Type::List(t) => (**t).clone(),
                    _ => Type::Unknown,
                };
                self.push();
                self.define(&var.name, elem.clone());
                let cond = self.expr(cond);
                self.pop();
                self.mke(
                    e,
                    ir::ExprKind::First {
                        var: var.name.clone(),
                        source: Box::new(source),
                        cond: Box::new(cond),
                    },
                    known.unwrap_or(Type::Maybe(Box::new(elem))),
                )
            }
            X::IsEmpty(x) => {
                let x = self.expr(x);
                self.mke(e, ir::ExprKind::IsEmpty(Box::new(x)), Type::Bool)
            }
            X::IsNotEmpty(x) => {
                let x = self.expr(x);
                self.mke(e, ir::ExprKind::IsNotEmpty(Box::new(x)), Type::Bool)
            }
            X::Matches { expr, record } => {
                let x = self.expr(expr);
                self.mke(
                    e,
                    ir::ExprKind::Matches {
                        expr: Box::new(x),
                        record: record.name.clone(),
                    },
                    Type::Bool,
                )
            }
            X::Length(x) => {
                let x = self.expr(x);
                self.mke(e, ir::ExprKind::Length(Box::new(x)), Type::Int)
            }
            X::NoAmountsChanged(x) => {
                let output = self.expr(x);
                let input = match &output.kind {
                    ir::ExprKind::Var(name) => self.model_input(name),
                    _ => None,
                }
                .unwrap_or_else(|| output.clone());
                self.mke(
                    e,
                    ir::ExprKind::NoAmountsChanged {
                        output: Box::new(output),
                        input: Box::new(input),
                    },
                    Type::Bool,
                )
            }
        }
    }

    fn call(
        &mut self,
        e: &ast::Expr,
        func: &[Ident],
        args: &[ast::Expr],
        known: Option<Type>,
    ) -> ir::Expr {
        let args: Vec<ir::Expr> = args.iter().map(|a| self.expr(a)).collect();
        let res = if is_synthetic(e.span) {
            None
        } else {
            self.checked.resolutions.get(&self.nk(e.span)).cloned()
        };
        if res.is_none() && func.len() == 2 && crate::stdlib::lookup(&func[0].name, &func[1].name).is_some() {
            let ty = known.unwrap_or(Type::Unknown);
            return self.mke(
                e,
                ir::ExprKind::Std {
                    module: func[0].name.clone(),
                    name: func[1].name.clone(),
                    args,
                    decode: None,
                },
                ty,
            );
        }
        let name = match res {
            Some(Resolution::Call { decl, .. }) => decl.clone(),
            _ => func[func.len() - 1].name.clone(),
        };
        let info = self.fns.get(&name);
        let callee = info.map(|f| f.callee).unwrap_or(Callee::Fn);
        let ty = known
            .or_else(|| info.and_then(|f| f.ret.clone()))
            .unwrap_or(Type::Bool);
        self.mke(
            e,
            ir::ExprKind::Call { callee, name, args },
            ty,
        )
    }

    fn var_expr(&self, name: &str) -> ir::Expr {
        ir::Expr {
            kind: ir::ExprKind::Var(name.to_string()),
            ty: self.lookup(name).cloned().unwrap_or(Type::Unknown),
            effect: EffectSet::pure(),
        }
    }

    fn path(&mut self, e: &ast::Expr, segs: &[Ident]) -> ir::Expr {
        let key = self.nk(e.span);
        let res = self.checked.resolutions.get(&key).cloned();
        let unwraps = self.checked.unwraps.get(&key).cloned().unwrap_or_default();
        let known = self.ety(e);
        let (mut cur, start) = match res {
            Some(Resolution::Source {
                tool,
                source,
                record,
            }) => {
                let ty = known.unwrap_or(Type::Unknown);
                return self.mke(e, ir::ExprKind::Source { tool, source, record }, ty);
            }
            Some(Resolution::Field {
                var,
                record,
                field,
            }) => {
                let var = var.unwrap_or_else(|| segs[0].name.clone());
                let base = ir::Expr {
                    kind: ir::ExprKind::Var(var),
                    ty: Type::Record(record),
                    effect: EffectSet::pure(),
                };
                let fty = self.field_ty(&base.ty, &field);
                (
                    ir::Expr {
                        kind: ir::ExprKind::Field {
                            base: Box::new(base),
                            field,
                        },
                        ty: fty,
                        effect: EffectSet::pure(),
                    },
                    1,
                )
            }
            Some(Resolution::Const { name }) => {
                let n = (1..=segs.len())
                    .rev()
                    .find(|&n| {
                        let j = join(&segs[..n]);
                        j == name || j.ends_with(&format!(".{name}"))
                    })
                    .unwrap_or(1);
                (self.inline_const(&name), n)
            }
            _ => (self.var_expr(&segs[0].name), 1),
        };
        for i in start..segs.len() {
            if unwraps.contains(&(i - 1)) {
                let ty = match &cur.ty {
                    Type::Maybe(inner) => (**inner).clone(),
                    other => other.clone(),
                };
                cur = ir::Expr {
                    effect: cur.effect.clone(),
                    kind: ir::ExprKind::Unwrap {
                        inner: Box::new(cur),
                        what: join(&segs[..i]),
                    },
                    ty,
                };
            }
            let fty = self.field_ty(&cur.ty, &segs[i].name);
            cur = ir::Expr {
                effect: cur.effect.clone(),
                kind: ir::ExprKind::Field {
                    base: Box::new(cur),
                    field: segs[i].name.clone(),
                },
                ty: fty,
            };
        }
        // the whole path is a `maybe` that is known to be present (`respond 200 found`)
        if let (Type::Maybe(inner), Some(t)) = (&cur.ty, &known) {
            if **inner == *t {
                cur = ir::Expr {
                    effect: cur.effect.clone(),
                    ty: t.clone(),
                    kind: ir::ExprKind::Unwrap {
                        inner: Box::new(cur),
                        what: join(segs),
                    },
                };
            }
        }
        if let Some(t) = known {
            cur.ty = t;
        }
        if !is_synthetic(e.span) {
            if let Some(eff) = self.checked.expr_effects.get(&key) {
                cur.effect = eff.clone();
            }
        }
        cur
    }

    /// A constant's value, written in the constant's own file.
    fn inline_const(&mut self, name: &str) -> ir::Expr {
        let info = self
            .checked
            .constants
            .get(name)
            .unwrap_or_else(|| panic!("internal compiler error: unknown constant {name}"));
        let (file, value) = (info.file, info.value.clone());
        let saved = std::mem::replace(&mut self.fi, file);
        let out = self.expr(&value);
        self.fi = saved;
        out
    }
}

// ---------------------------------------------------------------------------------- effects

fn expr_kids_effect(kind: &ir::ExprKind) -> EffectSet {
    use ir::ExprKind as X;
    let mut e = EffectSet::pure();
    let mut add = |x: &ir::Expr| e.merge(&x.effect);
    match kind {
        X::Field { base, .. } => add(base),
        X::Unwrap { inner, .. } => add(inner),
        X::Interp(parts) => {
            for p in parts {
                if let ir::InterpPart::Expr(x) = p {
                    add(x);
                }
            }
        }
        X::Binary { lhs, rhs, .. } => {
            add(lhs);
            add(rhs);
        }
        X::TextToBytes(x) | X::Not(x) | X::IsEmpty(x) | X::IsNotEmpty(x) | X::Length(x) => add(x),
        X::Call { args, .. } | X::Std { args, .. } => args.iter().for_each(&mut add),
        X::Record(fields) => fields.iter().for_each(|(_, x)| add(x)),
        X::Count { source, .. } => add(source),
        X::First { source, cond, .. } => {
            add(source);
            add(cond);
        }
        X::Matches { expr, .. } => add(expr),
        X::NoAmountsChanged { output, input } => {
            add(output);
            add(input);
        }
        X::Str(_)
        | X::Int(_)
        | X::Money { .. }
        | X::Duration(_)
        | X::Date(_)
        | X::Bool(_)
        | X::Today
        | X::Now
        | X::Var(_)
        | X::Source { .. } => {}
    }
    e
}

/// The effect of a made-up statement: what its parts do.
fn kids_effect(kind: &ir::StmtKind) -> EffectSet {
    use ir::StmtKind as S;
    let mut e = EffectSet::pure();
    match kind {
        S::Let { value, .. } | S::Assign { value, .. } | S::Return(value) => e.merge(&value.effect),
        S::If { cond, then, else_ } => {
            e.merge(&cond.effect);
            e.merge(&ir::union_effects(then));
            e.merge(&ir::union_effects(else_));
        }
        S::IfSome {
            expr, then, else_, ..
        } => {
            e.merge(&expr.effect);
            e.merge(&ir::union_effects(then));
            e.merge(&ir::union_effects(else_));
        }
        S::ForEach { source, body, .. } => {
            e.merge(&source.effect);
            e.merge(&ir::union_effects(body));
        }
        S::Stop => {}
        S::Effect { call, .. } => match call {
            ir::EffectCall::Tool(t) => {
                match &t.target {
                    Some(ir::Target::Field { value, .. }) | Some(ir::Target::Literal { value, .. }) => {
                        e.merge(&value.effect)
                    }
                    Some(ir::Target::Source { id: Some(id), .. }) => e.merge(&id.effect),
                    _ => {}
                }
                match &t.payload {
                    Some(ir::Payload::Value(x)) => e.merge(&x.effect),
                    Some(ir::Payload::Assign(list)) => list.iter().for_each(|(_, x)| e.merge(&x.effect)),
                    None => {}
                }
            }
            ir::EffectCall::Model(m) => {
                e.merge(&m.prompt.effect);
                if let Some(c) = &m.context {
                    e.merge(&c.effect);
                }
            }
            ir::EffectCall::Binding { args, .. }
            | ir::EffectCall::Rust { args, .. }
            | ir::EffectCall::Fn { args, .. } => args.iter().for_each(|x| e.merge(&x.effect)),
            ir::EffectCall::File(f) => {
                e.merge(&f.path.effect);
                if let Some(a) = &f.arg {
                    e.merge(&a.effect);
                }
            }
            ir::EffectCall::Std(x) => e.merge(&x.effect),
            ir::EffectCall::Io(io) => match io {
                ir::IoCall::ReadFile { path, .. } => e.merge(&path.effect),
                ir::IoCall::ReadBody { .. } | ir::IoCall::ReadInput => {}
                ir::IoCall::Print { message } => e.merge(&message.effect),
            },
            ir::EffectCall::Trace(ir::TraceEvent::Guard { ok, .. }) => e.merge(&ok.effect),
        },
        S::Respond { body, .. } => {
            if let Some(b) = body {
                e.merge(&b.effect);
            }
        }
        S::Append { value, list } => {
            e.merge(&value.effect);
            e.merge(&list.effect);
        }
        S::Parallel(v) => e.merge(&ir::union_effects(v)),
        S::Try {
            stmt, otherwise, ..
        } => {
            e.merge(&stmt.effect);
            if let Some(o) = otherwise {
                e.merge(&ir::union_effects(o));
            }
        }
    }
    e
}
