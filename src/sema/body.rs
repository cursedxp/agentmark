//! Bodies: the per-function / per-block checking context, and the driver that checks every
//! function, rule, agent block, route and command exactly once (on demand, because a call needs
//! the effect of its callee).

use std::collections::HashMap;

use crate::syntax::ast::{Decl, Effect, Stmt, StmtKind, Trigger};

use super::effects::EffectSet;
use super::scope::Kind;
use super::types::Type;
use super::{BlockInfo, BlockKind, Checker};

/// The record value a rule, loop, event or `do` call puts "in front of" a block: its fields can
/// be written bare (SPEC 5: "bare `amount` when inside a `for each` over a record").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Implicit {
    pub var: String,
    pub record: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PureKind {
    Rule,
    Guard,
    Constant,
    /// The expression of a collection word, run once per element (SPEC 22.2).
    Element,
}

#[derive(Debug, Default)]
pub struct Frame {
    pub vars: HashMap<String, Type>,
    /// Where each variable of this frame was defined (same file), when there is a place.
    pub defs: HashMap<String, crate::diag::Span>,
    pub bare: Option<Implicit>,
    /// The frame of a collection word's per-element expression (`bare` is the element's record).
    pub element: bool,
    /// Paths (`found`, `ticket.priority`) known not to be empty in this frame.
    pub narrowed: Vec<String>,
    /// Handles this frame shadows (`into h` inside a branch or loop, where codegen declares a new
    /// `h`): what was known of the outer one, put back when the frame ends.
    pub shadowed: Vec<(String, Option<crate::diag::Span>)>,
}

pub struct AgentCx {
    pub name: String,
    pub tools: Vec<String>,
    pub decl: usize,
}

pub struct FnCx {
    pub name: String,
    pub out: Option<Type>,
    /// What `- touches:` promises, and its words as written; `None` when the line is left out
    /// (then nothing is checked, SPEC 23.7).
    pub declared: Option<(EffectSet, String)>,
    /// An "exceeds its effect" error was already reported for this function.
    pub reported: bool,
    /// What messages call it: `function`, or `tool` for a verb of a user tool.
    pub noun: &'static str,
}

pub struct Cx {
    pub file: usize,
    pub frames: Vec<Frame>,
    /// Effect of the statement being checked (children merged in when they finish).
    pub acc: EffectSet,
    pub pure_ctx: Option<PureKind>,
    pub agent: Option<AgentCx>,
    pub func: Option<FnCx>,
    /// Inside a service route: `respond` and `read body` are allowed.
    pub route: bool,
    /// A `## Do` block nobody calls: names it cannot resolve are not reported.
    pub lenient: bool,
    /// Heading text of the block, for suggested fixes.
    pub unit: String,
    /// Segments unwrapped (known non-empty) while resolving the last path.
    pub unwrap_log: Vec<usize>,
    /// The statement about to be checked is the one directly after `try` (a `result of` function
    /// may only be called there).
    pub in_try: bool,
    /// The call expression directly after `match`: a `result of T` function may be called there
    /// without `try`.
    pub match_call: Option<crate::diag::Span>,
    /// The built-in library call (by span) that may be a `result`: the one directly after `try`.
    pub std_ok: Option<crate::diag::Span>,
    /// Stored results (`f() into n`) not matched yet on some path: name -> the line.
    pub pending: HashMap<String, crate::diag::Span>,
    /// Pending results already reported (by the span of the `set`).
    pub pending_reported: std::collections::HashSet<crate::diag::Span>,
    /// Handles (SPEC 22.4) that were used up (`self` methods, passing them on): variable ->
    /// the span of the line that used it up.
    pub consumed: HashMap<String, crate::diag::Span>,
    /// Handle variables this body uses up, changes or hands back (a function's parameter among them
    /// is then taken from the caller; otherwise it is only lent, Q414).
    pub took: std::collections::HashSet<String>,
    /// The path read next may be a handle: the caller moves or borrows it (`set`, call
    /// arguments, `return`). Anywhere else a handle read is an error (E362).
    pub handle_ok: bool,
}

impl Cx {
    pub fn new(file: usize) -> Self {
        Cx {
            file,
            frames: vec![Frame::default()],
            acc: EffectSet::pure(),
            pure_ctx: None,
            agent: None,
            func: None,
            route: false,
            lenient: false,
            unit: String::new(),
            unwrap_log: Vec::new(),
            in_try: false,
            match_call: None,
            std_ok: None,
            pending: HashMap::new(),
            pending_reported: Default::default(),
            consumed: HashMap::new(),
            took: Default::default(),
            handle_ok: false,
        }
    }

    pub fn constant(file: usize) -> Self {
        let mut c = Cx::new(file);
        c.pure_ctx = Some(PureKind::Constant);
        c
    }

    pub fn push(&mut self) {
        self.frames.push(Frame::default());
    }

    pub fn pop(&mut self) {
        if let Some(f) = self.frames.pop() {
            for v in f.vars.keys() {
                self.consumed.remove(v);
            }
            for (v, at) in f.shadowed {
                if let Some(at) = at {
                    self.consumed.insert(v, at);
                }
            }
        }
    }

    pub fn top(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("a frame")
    }

    pub fn var(&self, name: &str) -> Option<&Type> {
        self.frames.iter().rev().find_map(|f| f.vars.get(name))
    }

    /// Set (or define in the innermost frame) a variable.
    /// Where the variable `name` in scope was defined.
    pub fn def_of(&self, name: &str) -> Option<crate::diag::Span> {
        self.frames.iter().rev().find_map(|f| {
            f.vars
                .contains_key(name)
                .then(|| f.defs.get(name).copied())
        })?
    }

    pub fn define(&mut self, name: &str, ty: Type, span: crate::diag::Span) {
        self.forget(name);
        // a handle made again inside a branch or loop is a new variable of that block (the
        // generated code declares a new one), so the outer one keeps its state
        if ty.is_handle() && !self.top().vars.contains_key(name) && self.var(name).is_some() {
            let saved = self.consumed.remove(name);
            self.top().shadowed.push((name.to_string(), saved));
            self.top().vars.insert(name.to_string(), ty);
            self.top().defs.insert(name.to_string(), span);
            return;
        }
        self.consumed.remove(name);
        for f in self.frames.iter_mut().rev() {
            if let Some(slot) = f.vars.get_mut(name) {
                *slot = ty;
                f.defs.entry(name.to_string()).or_insert(span);
                return;
            }
        }
        self.top().vars.insert(name.to_string(), ty);
        self.top().defs.insert(name.to_string(), span);
    }

    pub fn bare(&self) -> Option<&Implicit> {
        self.frames.iter().rev().find_map(|f| f.bare.as_ref())
    }

    /// The record whose fields are bare names in the innermost per-element expression, if we are in one.
    pub fn element_bare(&self) -> Option<&Implicit> {
        let f = self.frames.iter().rev().find(|f| f.bare.is_some() || f.element)?;
        if f.element {
            f.bare.as_ref()
        } else {
            None
        }
    }

    pub fn is_narrowed(&self, path: &str) -> bool {
        self.frames
            .iter()
            .any(|f| f.narrowed.iter().any(|n| n == path))
    }

    pub fn narrow(&mut self, paths: &[String]) {
        for p in paths {
            self.top().narrowed.push(p.clone());
        }
    }

    /// A variable was assigned: what we knew about it (and its fields) is gone.
    pub fn forget(&mut self, var: &str) {
        let dotted = format!("{var}.");
        for f in &mut self.frames {
            f.narrowed.retain(|n| n != var && !n.starts_with(&dotted));
        }
    }

    pub fn in_agent_names(&self) -> Vec<String> {
        self.frames
            .iter()
            .flat_map(|f| f.vars.keys().cloned())
            .collect()
    }
}

/// Identifies a checked body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyKey {
    Function {
        file: usize,
        decl: usize,
    },
    Rule {
        file: usize,
        decl: usize,
    },
    Block {
        file: usize,
        decl: usize,
        block: usize,
    },
    Route {
        file: usize,
        decl: usize,
        route: usize,
    },
    Command {
        file: usize,
        decl: usize,
    },
    /// One verb of a `# Tool`.
    ToolVerb {
        file: usize,
        decl: usize,
        verb: usize,
    },
}

pub enum Memo {
    InProgress,
    Done(EffectSet),
}

/// `AppState` -> `app_state`, `Invoice` -> `invoice`.
pub fn snake(name: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if c == '-' {
            out.push('_');
            prev_lower = false;
        } else if c.is_uppercase() {
            if prev_lower {
                out.push('_');
            }
            out.extend(c.to_lowercase());
            prev_lower = false;
        } else {
            out.push(c);
            prev_lower = c.is_lowercase() || c.is_ascii_digit();
        }
    }
    out
}

pub fn contains_return(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Return(_) => true,
        StmtKind::ForEach { body, .. } => contains_return(body),
        StmtKind::Coll(c) => c.body().is_some_and(|b| contains_return(b)),
        StmtKind::If { then, else_, .. } | StmtKind::IfSome { then, else_, .. } => {
            contains_return(then) || contains_return(else_)
        }
        StmtKind::Try { stmt, otherwise } => {
            contains_return(std::slice::from_ref(stmt))
                || otherwise.as_deref().is_some_and(contains_return)
        }
        StmtKind::Parallel(b) => contains_return(b),
        StmtKind::Match {
            arms, otherwise, ..
        } => {
            arms.iter().any(|a| contains_return(&a.body))
                || otherwise.as_ref().is_some_and(|o| contains_return(&o.body))
        }
        StmtKind::Guard { else_: Some(e), .. } => contains_return(std::slice::from_ref(e)),
        _ => false,
    })
}

impl Checker<'_> {
    pub(super) fn check_all_bodies(&mut self) {
        let p = self.p;
        // Entry blocks, functions, rules, routes and commands first: they decide which record each
        // `## Do` block works on.
        for (fi, f) in p.files.iter().enumerate() {
            for (di, d) in f.ast.decls.iter().enumerate() {
                match d {
                    Decl::Function(_) => {
                        self.ensure_body(BodyKey::Function { file: fi, decl: di });
                    }
                    Decl::Rule(_) => {
                        self.ensure_body(BodyKey::Rule { file: fi, decl: di });
                    }
                    Decl::Agent(a) => {
                        for (bi, b) in a.blocks.iter().enumerate() {
                            if !matches!(b.trigger, Trigger::Do(_)) {
                                self.ensure_body(BodyKey::Block {
                                    file: fi,
                                    decl: di,
                                    block: bi,
                                });
                            }
                        }
                    }
                    Decl::Service(s) => {
                        for ri in 0..s.routes.len() {
                            self.ensure_body(BodyKey::Route {
                                file: fi,
                                decl: di,
                                route: ri,
                            });
                        }
                    }
                    Decl::Command(_) => {
                        self.ensure_body(BodyKey::Command { file: fi, decl: di });
                    }
                    Decl::Tool(t) => {
                        for vi in 0..t.verbs.len() {
                            self.ensure_body(BodyKey::ToolVerb { file: fi, decl: di, verb: vi });
                        }
                    }
                    _ => {}
                }
            }
        }
        for (fi, f) in p.files.iter().enumerate() {
            for (di, d) in f.ast.decls.iter().enumerate() {
                if let Decl::Agent(a) = d {
                    for (bi, b) in a.blocks.iter().enumerate() {
                        if matches!(b.trigger, Trigger::Do(_)) {
                            self.ensure_body(BodyKey::Block {
                                file: fi,
                                decl: di,
                                block: bi,
                            });
                        }
                    }
                }
            }
        }
        self.blocks
            .sort_by_key(|b| (b.file, b.span.line, b.span.col));
    }

    /// Effect of the body, checking it first if nobody did.
    pub fn ensure_body(&mut self, key: BodyKey) -> EffectSet {
        match self.memo.get(&key) {
            Some(Memo::Done(e)) => return e.clone(),
            Some(Memo::InProgress) => return EffectSet::pure(),
            None => {}
        }
        self.memo.insert(key, Memo::InProgress);
        let eff = self.check_body(key);
        self.memo.insert(key, Memo::Done(eff.clone()));
        eff
    }

    /// Effect of calling the function or Rust block `name` (SPEC 8): what its body does, and at
    /// least what its `- touches:` says (SPEC 23.7).
    pub fn callee_effect(&mut self, kind: Kind, name: &str) -> EffectSet {
        if !self.sigs.contains_key(&(kind, name.to_string())) {
            return EffectSet::pure();
        }
        if kind == Kind::Tool {
            return self.tool_verb_effect(name);
        }
        let mut set = match (kind, self.reg.get(kind, name).cloned()) {
            (Kind::Function, Some(ent)) => self.ensure_body(BodyKey::Function {
                file: ent.file,
                decl: ent.decl,
            }),
            _ => EffectSet::pure(),
        };
        if let Some(declared) = self.touches_decl.get(&(kind, name.to_string())) {
            set.merge(declared);
        }
        set
    }

    fn check_body(&mut self, key: BodyKey) -> EffectSet {
        let p = self.p;
        match key {
            BodyKey::ToolVerb { file, decl, verb } => self.check_verb_body(file, decl, verb),
            BodyKey::Function { file, decl } => {
                let Decl::Function(f) = &p.files[file].ast.decls[decl] else {
                    return EffectSet::pure();
                };
                let sig = self
                    .sigs
                    .get(&(Kind::Function, f.name.name.clone()))
                    .cloned();
                let mut cx = Cx::new(file);
                cx.unit = f.name.name.clone();
                let out = sig.as_ref().and_then(|s| s.out.clone());
                let declared = self
                    .touches_decl
                    .get(&(Kind::Function, f.name.name.clone()))
                    .cloned()
                    .zip(f.touches.as_ref().map(touches_text));
                let declared_class = declared.as_ref().map(|(s, _)| s.class());
                if let Some(s) = &sig {
                    for (param, info) in f.params.iter().zip(&s.params) {
                        cx.top().vars.insert(info.name.clone(), info.ty.clone());
                        cx.top().defs.insert(info.name.clone(), param.name.span);
                        self.check_default(file, param, &info.ty);
                    }
                }
                cx.func = Some(super::body::FnCx {
                    name: f.name.name.clone(),
                    out: out.clone(),
                    declared,
                    reported: false,
                    noun: "function",
                });
                self.stmts(&mut cx, &f.body);
                self.end_of_body(&mut cx);
                if let Some(s) = &sig {
                    let lends = s.params.iter().map(|p| p.ty.is_handle() && !cx.took.contains(&p.name)).collect();
                    self.fn_lends.insert(f.name.name.clone(), lends);
                }
                if out.is_some() && !f.body.is_empty() && !contains_return(&f.body) {
                    self.error(
                        "E210",
                        file,
                        f.name.span,
                        format!(
                            "The function \"{}\" says it returns a value but never does.",
                            f.name.name
                        ),
                    );
                    self.diags.last_mut().unwrap().help = Some(
                        "End the steps with a \"return <value>\" step, or remove the \"- out:\" line."
                            .into(),
                    );
                }
                let eff = cx.acc.clone();
                self.blocks.push(BlockInfo {
                    kind: BlockKind::Function,
                    file,
                    span: f.name.span,
                    unit: f.name.name.clone(),
                    label: f.name.name.clone(),
                    effect: eff.clone(),
                    declared: declared_class,
                    param: None,
                });
                eff
            }
            BodyKey::Rule { file, decl } => {
                let Decl::Rule(r) = &p.files[file].ast.decls[decl] else {
                    return EffectSet::pure();
                };
                let mut cx = Cx::new(file);
                cx.unit = r.name.name.clone();
                cx.pure_ctx = Some(PureKind::Rule);
                if let Some(rec) = self.rule_records.get(&r.name.name).cloned() {
                    let var = super::body::snake(&rec);
                    cx.top().vars.insert(var.clone(), Type::Record(rec.clone()));
                    cx.top().defs.insert(var.clone(), r.name.span);
                    cx.top().bare = Some(Implicit { var, record: rec });
                } else {
                    cx.lenient = true;
                }
                let ty = self.expr(&mut cx, &r.when);
                if !matches!(ty, Type::Bool | Type::Unknown) {
                    self.error(
                        "E203",
                        file,
                        r.when.span,
                        format!("A rule's \"when:\" must be a yes/no condition, but this is {ty}."),
                    );
                }
                let eff = cx.acc.clone();
                self.blocks.push(BlockInfo {
                    kind: BlockKind::Rule,
                    file,
                    span: r.name.span,
                    unit: r.name.name.clone(),
                    label: r.name.name.clone(),
                    effect: eff.clone(),
                    declared: Some(Effect::Pure),
                    param: self
                        .rule_records
                        .get(&r.name.name)
                        .map(|rec| (super::body::snake(rec), rec.clone())),
                });
                eff
            }
            BodyKey::Block { file, decl, block } => {
                let Decl::Agent(a) = &p.files[file].ast.decls[decl] else {
                    return EffectSet::pure();
                };
                let b = &a.blocks[block];
                let mut cx = Cx::new(file);
                cx.unit = a.name.name.clone();
                cx.agent = Some(super::body::AgentCx {
                    name: a.name.name.clone(),
                    tools: a.tools.iter().map(|t| t.name.clone()).collect(),
                    decl,
                });
                let is_do = matches!(b.trigger, Trigger::Do(_));
                let mut param = None;
                match &b.trigger {
                    Trigger::OnEvent { tool, event } => {
                        if let Some(imp) = self.check_event(&cx, tool, event) {
                            cx.top()
                                .vars
                                .insert(imp.var.clone(), Type::Record(imp.record.clone()));
                            cx.top().defs.insert(imp.var.clone(), event.span);
                            param = Some((imp.var.clone(), imp.record.clone()));
                            cx.top().bare = Some(imp);
                        }
                    }
                    Trigger::Do(_) => match self.do_ctx.get(&key).cloned().flatten() {
                        Some(imp) => {
                            cx.top()
                                .vars
                                .insert(imp.var.clone(), Type::Record(imp.record.clone()));
                            param = Some((imp.var.clone(), imp.record.clone()));
                            cx.top().bare = Some(imp);
                        }
                        None => cx.lenient = !self.called_blocks.contains(&(file, decl, block)),
                    },
                    _ => {}
                }
                self.stmts(&mut cx, &b.body);
                self.end_of_body(&mut cx);
                let eff = cx.acc.clone();
                self.blocks.push(BlockInfo {
                    kind: if is_do {
                        BlockKind::Action
                    } else {
                        BlockKind::Entry
                    },
                    file,
                    span: b.span,
                    unit: a.name.name.clone(),
                    label: b.label.clone(),
                    effect: eff.clone(),
                    declared: None,
                    param,
                });
                eff
            }
            BodyKey::Route { file, decl, route } => {
                let Decl::Service(s) = &p.files[file].ast.decls[decl] else {
                    return EffectSet::pure();
                };
                let r = &s.routes[route];
                let mut cx = Cx::new(file);
                cx.unit = s.name.name.clone();
                cx.route = true;
                for pp in &r.path_params {
                    cx.top().vars.insert(pp.clone(), Type::Text);
                    cx.top().defs.insert(pp.clone(), r.span);
                }
                if let Some(st) = &s.state {
                    let te = crate::syntax::ast::TypeExpr {
                        kind: crate::syntax::ast::TypeKind::Named(st.name.clone()),
                        span: st.span,
                    };
                    let ty = self.resolve_type(file, &te);
                    cx.top().vars.insert("state".into(), ty);
                }
                self.stmts(&mut cx, &r.body);
                self.end_of_body(&mut cx);
                let eff = cx.acc.clone();
                self.blocks.push(BlockInfo {
                    kind: BlockKind::Route,
                    file,
                    span: r.span,
                    unit: s.name.name.clone(),
                    label: format!("{} {}", r.method, r.path),
                    effect: eff.clone(),
                    declared: None,
                    param: None,
                });
                eff
            }
            BodyKey::Command { file, decl } => {
                let Decl::Command(c) = &p.files[file].ast.decls[decl] else {
                    return EffectSet::pure();
                };
                let sig = self
                    .sigs
                    .get(&(Kind::Command, c.name.name.clone()))
                    .cloned();
                let mut cx = Cx::new(file);
                cx.unit = c.name.name.clone();
                if let Some(s) = &sig {
                    for (param, info) in c.args.iter().zip(&s.params) {
                        cx.top().vars.insert(info.name.clone(), info.ty.clone());
                        cx.top().defs.insert(info.name.clone(), param.name.span);
                        self.check_default(file, param, &info.ty);
                    }
                }
                if let Some(ty) = self.service_state_type() {
                    cx.top().vars.insert("state".into(), ty);
                }
                self.stmts(&mut cx, &c.body);
                self.end_of_body(&mut cx);
                let eff = cx.acc.clone();
                self.blocks.push(BlockInfo {
                    kind: BlockKind::Command,
                    file,
                    span: c.name.span,
                    unit: c.name.name.clone(),
                    label: "Run".into(),
                    effect: eff.clone(),
                    declared: None,
                    param: None,
                });
                eff
            }
        }
    }

    /// OPEN_QUESTIONS Q9: a command uses the `state` of the project's (first) service.
    fn service_state_type(&mut self) -> Option<Type> {
        let p = self.p;
        for (fi, f) in p.files.iter().enumerate() {
            for d in &f.ast.decls {
                if let Decl::Service(s) = d {
                    if let Some(st) = &s.state {
                        return self.records.get(&st.name).map(|_| {
                            let _ = fi;
                            Type::Record(st.name.clone())
                        });
                    }
                }
            }
        }
        None
    }
}

/// The words of `- touches:` as written: `nothing`, `crm, email`.
pub fn touches_text(t: &crate::syntax::ast::Touches) -> String {
    t.words.iter().map(|w| w.name.as_str()).collect::<Vec<_>>().join(", ")
}
