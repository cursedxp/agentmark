//! `agent_lower` (DESIGN 4.1): Layer 3 -> Layer 2, AST -> AST.
//!
//! After this pass no `Agent`, `Rule`, trigger, `AskModel` or agent-block `Guard` is left:
//!
//! | Layer 3 | Layer 2 |
//! |---|---|
//! | `## Every …` / `## When …` / `## On start` | [`Decl::Entry`] (agent name = `unit`, heading = `label`) |
//! | `## Do remind` of agent `invoice-chaser` | [`Decl::Function`] `invoice-chaser__remind` |
//! | `do remind` | `do invoice-chaser__remind` with the record in front of the caller as its argument |
//! | `# Rule: r` | [`Decl::Function`] `r(<record>) -> bool`, `pure`, body `return <when>` |
//! | `for each r rec in src:` | `for each rec in src:` + `if r(rec):` around the body |
//! | `count of r recs in src` | same node, `rule` renamed to the rule's function |
//! | `ask model …` | [`StmtKind::ModelCall`] |
//! | `guard: c [else s]` in an agent | `set guard_ok_<line> = c`, [`StmtKind::Trace`], `if not guard_ok_<line>: s \| stop` |
//!
//! Everything else is copied untouched, so a file with no agent construct (example 05) comes out
//! equal to what went in. A `guard` outside an agent is a plain Layer-2 check with no trace line
//! (OPEN_QUESTIONS Q90); it stays a `Guard` and the IR builder turns it into `if not c: …`.
//!
//! Nodes made here have *synthetic* spans (`col == 0`, the line of the source they come from;
//! [`is_synthetic`]). Everything copied keeps its span, so the `NodeKey`s of `Checked` still find
//! it; synthetic nodes have no entry there and the IR builder works their types and effects out.

use std::collections::{HashMap, HashSet};

use crate::diag::Span;
use crate::project::Project;
use crate::sema::{BlockInfo, BlockKind, CallKind, Checked, NodeKey, Resolution, Type};
use crate::syntax::ast::*;

/// The Layer-2 program: one [`File`] per project file, same indices as `Project::files` (so the
/// file ids in `Checked`'s `NodeKey`s stay valid).
#[derive(Debug, Clone)]
pub struct Lowered {
    pub files: Vec<File>,
}

/// Is this a span `agent_lower` made up (no counterpart in `Checked`)? Real spans are 1-based.
pub fn is_synthetic(span: Span) -> bool {
    span.col == 0
}

fn synth(span: Span) -> Span {
    Span::new(span.line, 0, 0)
}

/// Post-order mutating walk over an expression tree.
pub fn walk_expr_mut(e: &mut Expr, f: &mut dyn FnMut(&mut Expr)) {
    match &mut e.kind {
        ExprKind::Str(_)
        | ExprKind::Int(_)
        | ExprKind::Money { .. }
        | ExprKind::Duration(_)
        | ExprKind::Date(_)
        | ExprKind::Bool(_)
        | ExprKind::Today
        | ExprKind::Now
        | ExprKind::Path(_) => {}
        ExprKind::Interp(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    walk_expr_mut(x, f);
                }
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr_mut(lhs, f);
            walk_expr_mut(rhs, f);
        }
        ExprKind::Not(x)
        | ExprKind::IsEmpty(x)
        | ExprKind::IsNotEmpty(x)
        | ExprKind::Length(x)
        | ExprKind::NoAmountsChanged(x) => walk_expr_mut(x, f),
        ExprKind::Matches { expr, .. } => walk_expr_mut(expr, f),
        ExprKind::Some(x) => walk_expr_mut(x, f),
        ExprKind::None => {}
        ExprKind::Call { args, .. } => args.iter_mut().for_each(|a| walk_expr_mut(a, f)),
        ExprKind::Decode { call, .. } => walk_expr_mut(call, f),
        ExprKind::Record(fields) | ExprKind::Variant { fields, .. } => {
            fields.iter_mut().for_each(|r| walk_expr_mut(&mut r.value, f))
        }
        ExprKind::CountOf { source, .. } => walk_expr_mut(source, f),
        ExprKind::FirstIn { source, cond, .. } => {
            walk_expr_mut(source, f);
            walk_expr_mut(cond, f);
        }
        ExprKind::Coll(c) => c.parts_mut().into_iter().for_each(|x| walk_expr_mut(x, f)),
    }
    f(e);
}

/// Names of the functions this pass creates, fixed up front so call sites and declarations agree.
struct Names {
    /// Rule name -> function name.
    rules: HashMap<String, String>,
    /// `(file, declaration index, block index)` of a `## Do` block -> function name.
    actions: HashMap<(usize, usize, usize), String>,
}

impl Names {
    fn build(project: &Project) -> Names {
        let mut used: HashSet<String> = HashSet::new();
        for f in &project.files {
            for d in &f.ast.decls {
                match d {
                    Decl::Function(x) => {
                        used.insert(x.name.name.clone());
                    }
                    Decl::RustBlock(x) => {
                        used.insert(x.name.name.clone());
                    }
                    _ => {}
                }
            }
        }
        fn alloc(used: &mut HashSet<String>, base: String) -> String {
            let mut name = base.clone();
            let mut n = 2;
            while !used.insert(name.clone()) {
                name = format!("{base}_{n}");
                n += 1;
            }
            name
        }
        let mut rules = HashMap::new();
        let mut actions = HashMap::new();
        for (fi, f) in project.files.iter().enumerate() {
            for (di, d) in f.ast.decls.iter().enumerate() {
                match d {
                    Decl::Rule(r) => {
                        let base = r.name.name.clone();
                        // a rule and a function may share a name (kinds are separate in sema)
                        let base = if used.contains(&base) { format!("{base}__rule") } else { base };
                        rules.insert(r.name.name.clone(), alloc(&mut used, base));
                    }
                    Decl::Agent(a) => {
                        for (bi, b) in a.blocks.iter().enumerate() {
                            if let Trigger::Do(n) = &b.trigger {
                                let base = format!("{}__{}", a.name.name, n.name);
                                actions.insert((fi, di, bi), alloc(&mut used, base));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Names { rules, actions }
    }
}

/// Lower a checked project. `checked` must come from `sema::check(project)`.
pub fn agent_lower(project: &Project, checked: &Checked) -> Lowered {
    let names = Names::build(project);
    let files = project
        .files
        .iter()
        .enumerate()
        .map(|(fi, f)| {
            let mut lw = Lw {
                checked,
                names: &names,
                fi,
                in_agent: false,
                model: None,
                decl: 0,
                do_blocks: HashMap::new(),
                bare: Vec::new(),
            };
            let mut decls = Vec::new();
            for (di, d) in f.ast.decls.iter().enumerate() {
                lw.decl(di, d, &mut decls);
            }
            File {
                path: f.ast.path.clone(),
                doc: f.ast.doc.clone(),
                imports: f.ast.imports.clone(),
                decls,
            }
        })
        .collect();
    Lowered { files }
}

struct Lw<'a> {
    checked: &'a Checked,
    names: &'a Names,
    fi: usize,
    in_agent: bool,
    /// `model:` of the agent being lowered.
    model: Option<String>,
    /// Index of the agent declaration being lowered.
    decl: usize,
    /// `## Do` blocks of the agent: name -> (block index, takes a record).
    do_blocks: HashMap<String, (usize, bool)>,
    /// Records "in front of" the code being lowered (innermost last): `(variable, record)`.
    bare: Vec<Option<(String, String)>>,
}

fn path_expr(name: &str, span: Span) -> Expr {
    Expr {
        span: synth(span),
        kind: ExprKind::Path(vec![Ident {
            name: name.to_string(),
            span: synth(span),
        }]),
    }
}

fn named_type(name: &str, span: Span) -> TypeExpr {
    TypeExpr {
        kind: TypeKind::Named(name.to_string()),
        span,
    }
}

fn snake(record: &str) -> String {
    let mut out = String::new();
    for (i, c) in record.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else if c == '-' {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    out
}

impl Lw<'_> {
    fn block_info(&self, span: Span, kinds: &[BlockKind]) -> Option<&BlockInfo> {
        self.checked
            .blocks
            .iter()
            .find(|b| b.file == self.fi && b.span == span && kinds.contains(&b.kind))
    }

    fn resolution(&self, span: Span) -> Option<&Resolution> {
        self.checked.resolutions.get(&NodeKey::new(self.fi, span))
    }

    fn current_bare(&self) -> Option<&(String, String)> {
        self.bare.iter().rev().find_map(|b| b.as_ref())
    }

    // ------------------------------------------------------------------ declarations

    fn decl(&mut self, di: usize, d: &Decl, out: &mut Vec<Decl>) {
        self.in_agent = false;
        self.model = None;
        self.bare.clear();
        match d {
            Decl::Rule(r) => out.push(Decl::Function(self.rule(r))),
            Decl::Agent(a) => self.agent(di, a, out),
            Decl::Function(f) => {
                let mut f = f.clone();
                f.params = self.params(&f.params);
                f.body = self.stmts(&f.body);
                out.push(Decl::Function(f));
            }
            Decl::Service(s) => {
                let mut s = s.clone();
                for r in &mut s.routes {
                    r.body = self.stmts(&r.body);
                }
                out.push(Decl::Service(s));
            }
            Decl::Command(c) => {
                let mut c = c.clone();
                c.args = self.params(&c.args);
                c.body = self.stmts(&c.body);
                out.push(Decl::Command(c));
            }
            Decl::Tool(t) => {
                let mut t = t.clone();
                for v in &mut t.verbs {
                    v.params = self.params(&v.params);
                    if let ToolBody::Stmts(body) = &mut v.body {
                        *body = self.stmts(body);
                    }
                }
                out.push(Decl::Tool(t));
            }
            Decl::Constant(c) => {
                let mut c = c.clone();
                c.value = self.expr(&c.value);
                out.push(Decl::Constant(c));
            }
            other => out.push(other.clone()),
        }
    }

    fn params(&self, ps: &[Param]) -> Vec<Param> {
        ps.iter()
            .map(|p| Param {
                name: p.name.clone(),
                ty: p.ty.clone(),
                default: p.default.as_ref().map(|d| self.expr(d)),
            })
            .collect()
    }

    fn rule(&mut self, r: &Rule) -> Function {
        let (var, record) = self
            .block_info(r.name.span, &[BlockKind::Rule])
            .and_then(|b| b.param.clone())
            .unwrap_or_else(|| (snake(&r.for_record.name), r.for_record.name.clone()));
        self.bare = vec![Some((var.clone(), record.clone()))];
        let when = self.expr(&r.when);
        Function {
            span: r.span,
            doc: r.doc.clone(),
            prop_comments: r.prop_comments.clone(),
            name: Ident {
                name: self.names.rules[&r.name.name].clone(),
                span: r.name.span,
            },
            params: vec![Param {
                name: Ident {
                    name: var,
                    span: r.name.span,
                },
                ty: named_type(&record, r.for_record.span),
                default: None,
            }],
            out: Some(named_type("bool", r.span)),
            effect: Spanned {
                value: Effect::Pure,
                span: r.span,
            },
            body: vec![Stmt {
                span: synth(r.when.span),
                comments: Vec::new(),
                kind: StmtKind::Return(when),
            }],
        }
    }

    fn agent(&mut self, di: usize, a: &Agent, out: &mut Vec<Decl>) {
        self.in_agent = true;
        self.model = Some(a.model.value.clone());
        self.decl = di;
        self.do_blocks.clear();
        for (bi, b) in a.blocks.iter().enumerate() {
            if let Trigger::Do(n) = &b.trigger {
                let takes = self
                    .block_info(b.span, &[BlockKind::Action])
                    .is_some_and(|i| i.param.is_some());
                self.do_blocks.insert(n.name.clone(), (bi, takes));
            }
        }
        for (bi, b) in a.blocks.iter().enumerate() {
            let info = self.block_info(b.span, &[BlockKind::Entry, BlockKind::Action]);
            let param = info.and_then(|i| i.param.clone());
            let class = info.map(|i| i.effect.class()).unwrap_or(Effect::Pure);
            self.bare = vec![param.clone()];
            let body = self.stmts(&b.body);
            let param_decl = param.map(|(v, r)| Param {
                name: Ident {
                    name: v,
                    span: b.span,
                },
                ty: named_type(&r, b.span),
                default: None,
            });
            match &b.trigger {
                Trigger::Do(n) => {
                    // an unverified binding counts like network (SPEC 15.3)
                    let class = if class == Effect::Unverified { Effect::Network } else { class };
                    out.push(Decl::Function(Function {
                        span: b.span,
                        doc: b.doc.clone(),
                        prop_comments: Vec::new(),
                        name: Ident {
                            name: self.names.actions[&(self.fi, di, bi)].clone(),
                            span: n.span,
                        },
                        params: param_decl.into_iter().collect(),
                        out: None,
                        effect: Spanned {
                            value: class,
                            span: b.span,
                        },
                        body,
                    }));
                }
                t => {
                    let trigger = match t {
                        Trigger::Every(d) => EntryTrigger::Every(*d),
                        Trigger::DailyAt { hour, minute } => EntryTrigger::DailyAt {
                            hour: *hour,
                            minute: *minute,
                        },
                        Trigger::Weekly { day, hour, minute } => EntryTrigger::Weekly {
                            day: *day,
                            hour: *hour,
                            minute: *minute,
                        },
                        Trigger::OnEvent { tool, event } => EntryTrigger::Event {
                            tool: tool.clone(),
                            event: event.clone(),
                        },
                        Trigger::OnStart => EntryTrigger::Start,
                        Trigger::Do(_) => unreachable!("handled above"),
                    };
                    out.push(Decl::Entry(Entry {
                        span: b.span,
                        doc: b.doc.clone(),
                        unit: a.name.name.clone(),
                        label: b.label.clone(),
                        trigger,
                        param: param_decl,
                        timeout: a.timeout.as_ref().map(|t| t.value),
                        memory: a.memory.as_ref().map(|m| m.value).unwrap_or(Memory::None),
                        body,
                    }));
                }
            }
        }
    }

    // ------------------------------------------------------------------ statements

    fn stmts(&mut self, body: &[Stmt]) -> Vec<Stmt> {
        body.iter().flat_map(|s| self.stmt(s)).collect()
    }

    /// One statement that must stay one (inside `guard … else s`, `try s`).
    fn stmt_one(&mut self, s: &Stmt) -> Stmt {
        let mut v = self.stmt(s);
        if v.len() == 1 {
            return v.remove(0);
        }
        Stmt {
            span: synth(s.span),
            comments: Vec::new(),
            kind: StmtKind::If {
                cond: Expr {
                    span: synth(s.span),
                    kind: ExprKind::Bool(true),
                },
                then: v,
                else_: Vec::new(),
            },
        }
    }

    fn record_elem(&self, source: &Expr) -> Option<String> {
        match self.checked.expr_type(self.fi, source.span) {
            Some(Type::List(inner)) => match &**inner {
                Type::Record(r) => Some(r.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    fn rule_fn(&self, rule: &Ident) -> String {
        let decl = match self.resolution(rule.span) {
            Some(Resolution::Call {
                kind: CallKind::Rule,
                decl,
            }) => decl.clone(),
            _ => rule.name.clone(),
        };
        self.names
            .rules
            .get(&decl)
            .cloned()
            .unwrap_or(decl)
    }

    fn stmt(&mut self, s: &Stmt) -> Vec<Stmt> {
        let mk = |kind: StmtKind| Stmt {
            span: s.span,
            comments: s.comments.clone(),
            kind,
        };
        let kind = match &s.kind {
            StmtKind::ForEach {
                filter_rule,
                var,
                source,
                body,
            } => {
                let elem = self.record_elem(source);
                self.bare.push(elem.map(|r| (var.name.clone(), r)));
                let inner = self.stmts(body);
                self.bare.pop();
                let source = self.expr(source);
                let body = match filter_rule {
                    Some(rule) => {
                        let f = self.rule_fn(rule);
                        let call = Expr {
                            span: synth(s.span),
                            kind: ExprKind::Call {
                                func: vec![Ident {
                                    name: f,
                                    span: synth(rule.span),
                                }],
                                args: vec![path_expr(&var.name, s.span)],
                            },
                        };
                        vec![Stmt {
                            span: synth(s.span),
                            comments: Vec::new(),
                            kind: StmtKind::If {
                                cond: call,
                                then: inner,
                                else_: Vec::new(),
                            },
                        }]
                    }
                    None => inner,
                };
                StmtKind::ForEach {
                    filter_rule: None,
                    var: var.clone(),
                    source,
                    body,
                }
            }
            StmtKind::If { cond, then, else_ } => StmtKind::If {
                cond: self.expr(cond),
                then: self.stmts(then),
                else_: self.stmts(else_),
            },
            StmtKind::IfSome {
                expr,
                bind,
                then,
                else_,
            } => StmtKind::IfSome {
                expr: self.expr(expr),
                bind: bind.clone(),
                then: self.stmts(then),
                else_: self.stmts(else_),
            },
            StmtKind::Match {
                scrutinee,
                arms,
                otherwise,
            } => StmtKind::Match {
                scrutinee: self.expr(scrutinee),
                arms: arms
                    .iter()
                    .map(|a| MatchArm {
                        body: self.stmts(&a.body),
                        ..a.clone()
                    })
                    .collect(),
                otherwise: otherwise.as_ref().map(|o| Otherwise {
                    body: self.stmts(&o.body),
                    ..o.clone()
                }),
            },
            StmtKind::Stop => StmtKind::Stop,
            StmtKind::Do { name, args, into, bare } => {
                let block = match self.resolution(s.span) {
                    Some(Resolution::Call {
                        kind: CallKind::DoBlock,
                        decl,
                    }) if self.in_agent => self.do_blocks.get(decl).copied(),
                    _ => None,
                };
                match block {
                    Some((bi, takes_record)) => {
                        let fn_name = self.names.actions[&(self.fi, self.decl, bi)].clone();
                        let args = match (takes_record, self.current_bare()) {
                            (true, Some((var, _))) => vec![path_expr(var, s.span)],
                            // sema rejects a call whose record differs from the first caller's
                            (true, None) => panic!(
                                "internal compiler error: `do {}` at line {} needs a record but none is in front of it",
                                name.name, s.span.line
                            ),
                            _ => Vec::new(),
                        };
                        StmtKind::Do {
                            name: Ident {
                                name: fn_name,
                                span: name.span,
                            },
                            args,
                            into: None,
                            bare: false,
                        }
                    }
                    None => StmtKind::Do {
                        name: name.clone(),
                        args: args.iter().map(|a| self.expr(a)).collect(),
                        into: into.clone(),
                        bare: *bare,
                    },
                }
            }
            StmtKind::AskModel {
                with,
                prompt,
                as_record,
                into,
            } => StmtKind::ModelCall {
                with: with.as_ref().map(|w| self.expr(w)),
                prompt: self.expr(prompt),
                schema: as_record.clone(),
                into: into.clone(),
                model: self.model.clone(),
            },
            StmtKind::ToolCall {
                tool,
                verb,
                target,
                payload,
                into,
                via_do,
            } => StmtKind::ToolCall {
                tool: tool.clone(),
                verb: verb.clone(),
                target: target
                    .iter()
                    .map(|t| match t {
                        TargetPart::Word(w) => TargetPart::Word(w.clone()),
                        TargetPart::Value(e) => TargetPart::Value(self.expr(e)),
                    })
                    .collect(),
                payload: payload.as_ref().map(|p| match p {
                    Payload::Assign(list) => Payload::Assign(
                        list.iter()
                            .map(|a| Assign {
                                field: a.field.clone(),
                                value: self.expr(&a.value),
                            })
                            .collect(),
                    ),
                    Payload::Value(e) => Payload::Value(self.expr(e)),
                }),
                into: into.clone(),
                via_do: *via_do,
            },
            StmtKind::Guard { cond, else_ } => {
                if !self.in_agent {
                    let else_ = else_.as_ref().map(|e| Box::new(self.stmt_one(e)));
                    return vec![mk(StmtKind::Guard {
                        cond: self.expr(cond),
                        else_,
                    })];
                }
                return self.guard(s, cond, else_.as_deref());
            }
            StmtKind::Set { var, expr } => StmtKind::Set {
                var: var.clone(),
                expr: self.expr(expr),
            },
            StmtKind::Return(e) => StmtKind::Return(self.expr(e)),
            StmtKind::Respond { status, body } => StmtKind::Respond {
                status: *status,
                body: body.as_ref().map(|b| self.expr(b)),
            },
            StmtKind::ReadBody { ty, into } => StmtKind::ReadBody {
                ty: ty.clone(),
                into: into.clone(),
            },
            StmtKind::ReadFile { path, ty, into } => StmtKind::ReadFile {
                path: self.expr(path),
                ty: ty.clone(),
                into: into.clone(),
            },
            StmtKind::Append { value, list } => StmtKind::Append {
                value: self.expr(value),
                list: self.expr(list),
            },
            StmtKind::ReadInput { into } => StmtKind::ReadInput { into: into.clone() },
            StmtKind::File { verb, path, arg, into } => StmtKind::File {
                verb: verb.clone(),
                path: self.expr(path),
                arg: arg.as_ref().map(|a| match a {
                    FileArg::To(e) => FileArg::To(self.expr(e)),
                    FileArg::Content(e) => FileArg::Content(self.expr(e)),
                }),
                into: into.clone(),
            },
            StmtKind::CallInto { call, into, via_do } => StmtKind::CallInto {
                call: self.expr(call),
                into: into.clone(),
                via_do: *via_do,
            },
            StmtKind::Print(e) => StmtKind::Print(self.expr(e)),
            StmtKind::Coll(c) => {
                let mut c = c.clone();
                for x in c.exprs_mut() {
                    *x = self.expr(x);
                }
                if let Some(b) = c.body_mut() {
                    self.bare.push(None);
                    let inner = self.stmts(b);
                    self.bare.pop();
                    *b = inner;
                }
                StmtKind::Coll(c)
            }
            StmtKind::Try { stmt, otherwise } => StmtKind::Try {
                stmt: Box::new(self.stmt_one(stmt)),
                otherwise: otherwise.as_ref().map(|o| self.stmts(o)),
            },
            StmtKind::Parallel(body) => StmtKind::Parallel(self.stmts(body)),
            // already Layer 2 (a second run over lowered code changes nothing)
            StmtKind::ModelCall { .. } | StmtKind::Trace(_) => s.kind.clone(),
        };
        vec![mk(kind)]
    }

    /// `guard: c [else s]` in an agent: evaluate, trace, branch.
    fn guard(&mut self, s: &Stmt, cond: &Expr, else_: Option<&Stmt>) -> Vec<Stmt> {
        let tmp = format!("guard_ok_{}", s.span.line);
        let name = self.guard_name(cond);
        let syn = synth(s.span);
        let failed = match else_ {
            Some(e) => self.stmt(e),
            None => vec![Stmt {
                span: syn,
                comments: Vec::new(),
                kind: StmtKind::Stop,
            }],
        };
        vec![
            Stmt {
                span: syn,
                comments: s.comments.clone(),
                kind: StmtKind::Set {
                    var: Ident {
                        name: tmp.clone(),
                        span: syn,
                    },
                    expr: self.expr(cond),
                },
            },
            Stmt {
                span: syn,
                comments: Vec::new(),
                kind: StmtKind::Trace(TraceEvent::Guard {
                    name,
                    ok: path_expr(&tmp, s.span),
                }),
            },
            Stmt {
                span: syn,
                comments: Vec::new(),
                kind: StmtKind::If {
                    cond: Expr {
                        span: syn,
                        kind: ExprKind::Not(Box::new(path_expr(&tmp, s.span))),
                    },
                    then: failed,
                    else_: Vec::new(),
                },
            },
        ]
    }

    fn guard_name(&self, cond: &Expr) -> String {
        guard_text(self.checked, self.fi, cond)
    }

    // ------------------------------------------------------------------ expressions

    fn expr(&self, e: &Expr) -> Expr {
        let mut c = e.clone();
        walk_expr_mut(&mut c, &mut |n| {
            if let ExprKind::CountOf {
                rule: Some(r),
                noun,
                source,
            } = &n.kind
            {
                let renamed = Ident {
                    name: self.rule_fn(r),
                    span: r.span,
                };
                n.kind = ExprKind::CountOf {
                    rule: Some(renamed),
                    noun: noun.clone(),
                    source: source.clone(),
                };
            }
        });
        c
    }
}

/// A guard's source text with constants inlined: `reminder length < 800`. `fi` is the file the
/// guard is written in.
pub fn guard_text(checked: &Checked, fi: usize, cond: &Expr) -> String {
    let mut c = cond.clone();
    inline_consts(checked, fi, &mut c, 0);
    crate::fmt::expr_text(&c)
}

fn inline_consts(checked: &Checked, fi: usize, e: &mut Expr, depth: u8) {
    if depth > 8 {
        return;
    }
    walk_expr_mut(e, &mut |n| {
        let ExprKind::Path(segs) = &n.kind else { return };
        let Some(Resolution::Const { name }) = checked.resolutions.get(&NodeKey::new(fi, n.span))
        else {
            return;
        };
        // only a path that is the whole constant (`max` or `module.max`), not `max.field`
        if segs.last().map(|s| s.name.as_str()) != Some(name.as_str()) {
            return;
        }
        if let Some(info) = checked.constants.get(name) {
            let mut v = info.value.clone();
            // a constant is written in its own file, so its names resolve there
            inline_consts(checked, info.file, &mut v, depth + 1);
            *n = v;
        }
    });
}
