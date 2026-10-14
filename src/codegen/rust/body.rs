//! Statements and expressions -> Rust source for one function body.
//!
//! The IR is already flat (no agents, rules or guards), so this is a direct, readable
//! translation: one `// <file>.agent.md:<line>` comment per source line, one Rust statement (or
//! a few) per IR statement. Values are owned; a place (`invoice.id`) is cloned when it is used by
//! value and its type is not `Copy`. Every generated function returns `Result<_, AgentError>`, so
//! anything that can fail is a `?`.

use std::collections::HashSet;

use super::names::{value_ident, Names};
use super::ty::{default_expr, field_ident, is_copy, rust_type};
use crate::ir::*;

mod collections;

// precedence levels of an emitted expression
const P_OR: u8 = 1;
const P_AND: u8 = 2;
const P_CMP: u8 = 3;
const P_ADD: u8 = 4;
const P_MUL: u8 = 5;
const P_UNARY: u8 = 6;
const P_ATOM: u8 = 7;

#[derive(Clone)]
struct E {
    s: String,
    p: u8,
}

fn atom(s: impl Into<String>) -> E {
    E { s: s.into(), p: P_ATOM }
}

fn paren(e: E, min: u8) -> String {
    if e.p < min {
        format!("({})", e.s)
    } else {
        e.s
    }
}

/// A Rust string literal.
pub fn lit(s: &str) -> String {
    format!("{s:?}")
}

/// What kind of block is being written (decides what `stop` and the end of the body return).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Entry, command or function that returns nothing.
    Unit,
    /// Function with a result.
    Value,
    /// Service route: returns the reply.
    Route,
}

/// Partial result of an effect: statements to run, then the value (if it has one).
struct Eff {
    /// Helper bindings; when there are any, the effect is wrapped in a block so they do not leak.
    temps: Vec<String>,
    lines: Vec<String>,
    value: Option<String>,
}

impl Eff {
    fn value(v: String) -> Eff {
        Eff { temps: vec![], lines: vec![], value: Some(v) }
    }
    fn lines(l: Vec<String>) -> Eff {
        Eff { temps: vec![], lines: l, value: None }
    }
}

pub struct Body<'a> {
    p: &'a Program,
    n: &'a Names,
    root: &'a str,
    module: &'a str,
    kind: Kind,
    ret: Option<Type>,
    out: Vec<String>,
    indent: usize,
    last_src: Option<(String, u32)>,
    assigned: HashSet<String>,
    lenient: HashSet<String>,
    /// `maybe` variables read through `expect` several times in the statement being written:
    /// bound once, under this name.
    narrowed: std::collections::HashMap<String, String>,
}

impl<'a> Body<'a> {
    pub fn new(p: &'a Program, n: &'a Names, root: &'a str, module: &'a str, kind: Kind, ret: Option<Type>) -> Self {
        Body {
            p,
            n,
            root,
            module,
            kind,
            ret,
            out: Vec::new(),
            indent: 1,
            last_src: None,
            assigned: HashSet::new(),
            lenient: HashSet::new(),
            narrowed: Default::default(),
        }
    }

    pub fn finish(self) -> String {
        self.out.join("\n")
    }

    /// Write a function body (top level: the last statement becomes the tail expression).
    pub fn function_body(&mut self, stmts: &[Stmt]) {
        collect_assigned(stmts, &mut self.assigned);
        collect_mut_handles(self.p, stmts, &mut self.assigned);
        collect_matches(stmts, &mut self.lenient);
        self.block(stmts, true);
    }

    pub fn assigned(&self, name: &str) -> bool {
        self.assigned.contains(name)
    }

    pub fn raw_line(&mut self, s: &str) {
        self.line(s);
    }

    // ------------------------------------------------------------------------------ output

    fn line(&mut self, s: &str) {
        for l in s.split('\n') {
            if l.is_empty() {
                self.out.push(String::new());
            } else {
                self.out.push(format!("{}{}", "    ".repeat(self.indent), l));
            }
        }
    }

    fn rel(&self, file: &str) -> String {
        let root = self.root.trim_end_matches('/');
        file.strip_prefix(root).map(|r| r.trim_start_matches('/')).filter(|r| !r.is_empty()).unwrap_or(file).to_string()
    }

    pub fn src_comment(&self, s: &SourceRef) -> String {
        format!("// {}:{}", self.rel(&s.file), s.line)
    }

    fn src(&mut self, s: &SourceRef) {
        let key = (s.file.clone(), s.line);
        if self.last_src.as_ref() != Some(&key) {
            let c = self.src_comment(s);
            self.line(&c);
            self.last_src = Some(key);
        }
    }

    fn rty(&self, t: &Type) -> String {
        rust_type(t, self.n)
    }

    /// A handle variable is `mut` when any variable of that name is lent mutably; one that
    /// is only handed on (or made again later) would warn, so the line says it is fine.
    fn allow_unused_mut(&mut self, m: &str, ty: &Type) {
        if !m.is_empty() && matches!(ty, Type::Handle(_)) {
            self.line("#[allow(unused_mut)]");
        }
    }

    fn stop_value(&self) -> String {
        match self.kind {
            Kind::Unit => "()".into(),
            Kind::Route => "Reply::status(200)".into(),
            Kind::Value => self.ret.as_ref().map_or("()".into(), |t| default_expr(t, self.n)),
        }
    }

    // ------------------------------------------------------------------------------ blocks

    fn block(&mut self, stmts: &[Stmt], top: bool) {
        let last = stmts.len().wrapping_sub(1);
        let mut ended = false;
        for (i, s) in stmts.iter().enumerate() {
            let tail = top && i == last;
            ended = self.stmt(s, tail);
        }
        if top && !ended {
            let v = self.stop_value();
            self.line(&format!("Ok({v})"));
        }
    }

    fn nested(&mut self, stmts: &[Stmt]) {
        let saved = std::mem::take(&mut self.narrowed);
        self.indent += 1;
        self.block(stmts, false);
        self.indent -= 1;
        self.narrowed = saved;
    }

    /// Returns true when it wrote the function's tail expression.
    fn stmt(&mut self, s: &Stmt, tail: bool) -> bool {
        self.src(&s.source);
        let before = self.narrowed.clone();
        self.narrow(s);
        let ended = self.stmt_kind(s, tail);
        self.narrowed = before;
        ended
    }

    /// A `maybe` variable that is read through `expect` more than once in one statement is
    /// unwrapped once, on the line before.
    fn narrow(&mut self, s: &Stmt) {
        let mut counts: Vec<(String, String, usize)> = Vec::new();
        for e in stmt_exprs(s) {
            count_unwraps(e, &mut counts);
        }
        for (var, what, n) in counts {
            if n >= 2 && !self.narrowed.contains_key(&var) {
                let name = format!("{}_value", self.n.local(&var));
                let msg = lit(&format!("{what} is not empty here"));
                self.line(&format!("let {name} = {}.as_ref().expect({msg});", self.n.local(&var)));
                self.narrowed.insert(var, name);
            }
        }
    }

    fn stmt_kind(&mut self, s: &Stmt, tail: bool) -> bool {
        match &s.kind {
            StmtKind::Let { name, ty, value } => {
                let m = if self.assigned.contains(name) { "mut " } else { "" };
                self.allow_unused_mut(m, ty);
                let v = self.conv(value, ty);
                self.line(&format!("let {m}{} = {v};", self.n.local(name)));
            }
            StmtKind::Assign { name, ty, value } => {
                let v = self.conv(value, ty);
                self.line(&format!("{} = {v};", self.n.local(name)));
            }
            StmtKind::If { cond, then, else_ } => {
                let c = self.ex(cond).s;
                self.line(&format!("if {c} {{"));
                self.nested(then);
                if else_.is_empty() {
                    self.line("}");
                } else {
                    self.line("} else {");
                    self.nested(else_);
                    self.line("}");
                }
            }
            StmtKind::IfSome { expr, bind, then, else_ } => {
                let v = self.val(expr);
                self.line(&format!("if let Some({}) = {v} {{", self.n.local(&bind.name)));
                self.nested(then);
                if else_.is_empty() {
                    self.line("}");
                } else {
                    self.line("} else {");
                    self.nested(else_);
                    self.line("}");
                }
            }
            StmtKind::ForEach { var, source, body } => {
                let v = self.val(source);
                let m = if self.assigned.contains(&var.name) { "mut " } else { "" };
                self.line(&format!("for {m}{} in {v} {{", self.n.local(&var.name)));
                self.nested(body);
                self.line("}");
            }
            StmtKind::Return(e) => {
                let ret = self.ret.clone().unwrap_or(e.ty.clone());
                let v = self.conv(e, &ret);
                if tail {
                    self.line(&format!("Ok({v})"));
                    return true;
                }
                self.line(&format!("return Ok({v});"));
            }
            StmtKind::Stop => {
                let v = self.stop_value();
                if tail {
                    self.line(&format!("Ok({v})"));
                    return true;
                }
                self.line(&format!("return Ok({v});"));
            }
            StmtKind::Respond { status, body } => {
                let r = match body {
                    None => format!("Reply::status({status})"),
                    Some(b) => format!("Reply::json({status}, {})", self.json_of(b)),
                };
                if tail {
                    self.line(&format!("Ok({r})"));
                    return true;
                }
                self.line(&format!("return Ok({r});"));
            }
            StmtKind::Append { value, list } => {
                let elem = match &list.ty {
                    Type::List(t) => (**t).clone(),
                    _ => value.ty.clone(),
                };
                let v = self.conv(value, &elem);
                let l = paren(self.ex(list), P_ATOM);
                self.line(&format!("{l}.push({v});"));
            }
            StmtKind::Parallel(stmts) => self.parallel(stmts),
            StmtKind::Coll(c) => self.coll_stmt(c),
            StmtKind::Effect { call, into } => {
                self.effect_stmt(call, into.as_ref());
            }
            StmtKind::Try { stmt, otherwise, defines } => {
                self.try_stmt(stmt, otherwise.as_deref(), defines.as_ref());
            }
        }
        false
    }

    /// `parallel:` - independent effects run together (`tokio::try_join!`); anything else in the
    /// list runs in order.
    fn parallel(&mut self, stmts: &[Stmt]) {
        let all_effects = stmts.len() > 1 && stmts.iter().all(|s| matches!(s.kind, StmtKind::Effect { .. }));
        if !all_effects {
            for st in stmts {
                self.stmt(st, false);
            }
            return;
        }
        let mut futs: Vec<String> = Vec::new();
        let mut pats: Vec<String> = Vec::new();
        for st in stmts {
            let StmtKind::Effect { call, into } = &st.kind else { continue };
            let eff = self.effect(call, into.as_ref());
            let mut block = String::from("async {\n");
            for l in eff.temps.iter().chain(&eff.lines) {
                for sub in l.split('\n') {
                    block.push_str(&format!("    {sub}\n"));
                }
            }
            block.push_str(&format!("    Ok::<_, AgentError>({})\n}}", eff.value.unwrap_or_else(|| "()".into())));
            futs.push(block);
            pats.push(match into {
                Some(v) => format!("{}{}", if self.assigned.contains(&v.name) { "mut " } else { "" }, self.n.local(&v.name)),
                None => "_".into(),
            });
        }
        self.line("// the steps below do not depend on each other, so they run together");
        let pat = if pats.iter().all(|p| p == "_") { String::new() } else { format!("let ({}) = ", pats.join(", ")) };
        self.line(&format!("{pat}tokio::try_join!("));
        self.indent += 1;
        for f in futs {
            self.line(&format!("{f},"));
        }
        self.indent -= 1;
        self.line(")?;");
    }

    // ------------------------------------------------------------------------------ effects

    fn effect_stmt(&mut self, call: &EffectCall, into: Option<&LocalVar>) {
        // `ask model … as R -> q` where the program asks `q matches R`: a bad answer is a failed guard
        if let (EffectCall::Model(m), Some(v)) = (call, into) {
            if m.schema.is_some() && self.lenient.contains(&v.name) {
                let (head, _) = self.model_parts(m);
                let rec = self.rty(&v.ty);
                let q = self.n.local(&v.name);
                self.line(&format!("let ({q}, {q}_matches) = match parse_model_json::<{rec}>(&{head}) {{"));
                self.line("    Ok(answer) => (answer, true),");
                self.line(&format!("    Err(_) => ({rec}::empty(), false),"));
                self.line("};");
                return;
            }
        }
        let eff = self.effect(call, into);
        match into {
            Some(v) => {
                let m = if self.assigned.contains(&v.name) { "mut " } else { "" };
                self.allow_unused_mut(m, &v.ty);
                let name = self.n.local(&v.name);
                let value = eff.value.unwrap_or_else(|| "()".into());
                if eff.temps.is_empty() && eff.lines.is_empty() {
                    self.line(&format!("let {m}{name} = {value};"));
                } else {
                    self.line(&format!("let {m}{name} = {{"));
                    self.indent += 1;
                    for l in eff.temps.iter().chain(&eff.lines) {
                        self.line(l);
                    }
                    self.line(&value);
                    self.indent -= 1;
                    self.line("};");
                }
            }
            None => {
                if eff.temps.is_empty() {
                    for l in &eff.lines {
                        self.line(l);
                    }
                    if let Some(v) = eff.value {
                        self.line(&format!("{v};"));
                    }
                } else {
                    self.line("{");
                    self.indent += 1;
                    for l in eff.temps.iter().chain(&eff.lines) {
                        self.line(l);
                    }
                    if let Some(v) = eff.value {
                        self.line(&format!("{v};"));
                    }
                    self.indent -= 1;
                    self.line("}");
                }
            }
        }
    }

    fn try_stmt(&mut self, stmt: &Stmt, otherwise: Option<&[Stmt]>, defines: Option<&LocalVar>) {
        let StmtKind::Effect { call, into } = &stmt.kind else {
            // `try` only wraps effects; anything else cannot fail
            self.stmt(stmt, false);
            return;
        };
        let eff = self.effect(call, into.as_ref());
        // the effect, as a future that yields Result<value, AgentError>
        let mut inner: Vec<String> = Vec::new();
        inner.extend(eff.temps.iter().cloned());
        inner.extend(eff.lines.iter().cloned());
        let value = eff.value.clone().unwrap_or_else(|| "()".into());
        let fut = |_: &Self, _: usize| -> String {
            let mut s = String::from("async {\n");
            for l in &inner {
                for sub in l.split('\n') {
                    s.push_str(&format!("    {sub}\n"));
                }
            }
            s.push_str(&format!("    Ok::<_, AgentError>({value})\n}}.await"));
            s
        };
        let ind = self.indent;
        match (defines, otherwise) {
            (Some(d), None) => {
                let name = self.n.local(&d.name);
                let m = if self.assigned.contains(&d.name) { "mut " } else { "" };
                self.allow_unused_mut(m, &d.ty);
                let f = fut(self, ind);
                self.line("#[allow(unused_variables)]");
                self.line(&format!("let {m}{name} = match {f} {{"));
                self.line("    Ok(answer) => answer,");
                self.line("    Err(err) => {");
                self.failure_exit("        ");
                self.line("    }");
                self.line("};");
            }
            (None, None) => {
                let f = fut(self, ind);
                self.line(&format!("if let Err(err) = {f} {{"));
                self.failure_exit("    ");
                self.line("}");
            }
            (Some(d), Some(other)) => {
                let name = self.n.local(&d.name);
                let ty = self.rty(&d.ty);
                // the value may never be read after the `try` (it only had to parse)
                self.line("#[allow(unused_variables, unused_assignments)]");
                self.line(&format!("let {name}: {ty};"));
                let f = fut(self, ind);
                self.line("#[allow(unused_assignments)]");
                self.line(&format!("match {f} {{"));
                self.line(&format!("    Ok(answer) => {name} = answer,"));
                self.line("    Err(_) => {");
                self.indent += 2;
                self.block(other, false);
                if !assigns_or_leaves(other, &d.name) {
                    let dflt = default_expr(&d.ty, self.n);
                    self.line(&format!("{name} = {dflt};"));
                }
                self.indent -= 2;
                self.line("    }");
                self.line("}");
            }
            (None, Some(other)) => {
                let f = fut(self, ind);
                self.line(&format!("if let Err(_) = {f} {{"));
                self.nested(other);
                self.line("}");
            }
        }
    }

    /// A `try` without `otherwise` failed (the error is in `err`). In a `result of T` function the
    /// error goes to the caller (Q200); anywhere else it is traced and the block stops.
    fn failure_exit(&mut self, pad: &str) {
        if self.kind == Kind::Value && matches!(self.ret, Some(Type::Result(_))) {
            self.line(&format!("{pad}return Err(err);"));
        } else {
            self.line(&format!("{pad}ctx.trace_error(&err.to_string())?;"));
            let sv = self.stop_value();
            self.line(&format!("{pad}return Ok({sv});"));
        }
    }

    /// `ask model` pieces: the call expression and the `as Record` hint expression.
    fn model_parts(&self, m: &ModelCall) -> (String, Option<String>) {
        let prompt = self.txt(&m.prompt);
        let subject = match &m.context {
            Some(c) => format!("Some(&{})", self.json_of_ref(c)),
            None => "None".into(),
        };
        let hint = match &m.schema {
            Some(r) => format!("Some({}::SCHEMA_HINT)", self.n.record(r)),
            None => "None".into(),
        };
        (format!("ctx.ask_model_about({prompt}, {subject}, {hint}).await?"), m.schema.clone())
    }

    fn effect(&self, call: &EffectCall, into: Option<&LocalVar>) -> Eff {
        match call {
            EffectCall::Model(m) => {
                let (head, schema) = self.model_parts(m);
                match schema {
                    Some(r) => {
                        let rec = self.n.record(&r);
                        Eff::value(format!("parse_model_json::<{rec}>(&{head})?"))
                    }
                    None => Eff::value(head),
                }
            }
            EffectCall::Tool(t) => self.tool_call(t, into),
            EffectCall::Binding { binding, item, on, args } => {
                let bs = self.n.bindings.get(binding).cloned().unwrap_or_else(|| binding.clone());
                let key = binding_key(item, on.as_deref());
                let m = self.n.binding_items.get(&(binding.clone(), key)).cloned().unwrap_or_else(|| value_ident(item));
                let bf = self.p.bindings.iter().find(|b| &b.binding == binding && &b.item == item && &b.on == on);
                let a = self.binding_args(args, bf);
                Eff::value(format!("bindings::{bs}::{m}({a}).await?"))
            }
            EffectCall::Rust { name, args } => {
                let f = self.n.rust_fns.get(name).cloned().unwrap_or_else(|| value_ident(name));
                let params = self.p.rust_fns.iter().find(|r| &r.name == name).map(|r| r.params.clone());
                let a = self.args(args, params.as_deref());
                Eff::value(format!("bindings::{f}({a})"))
            }
            EffectCall::Fn { name, args } => Eff::value(self.fn_call(name, args)),
            EffectCall::File(fc) => {
                let path = self.txt(&fc.path);
                let more = fc.arg.as_ref().map(|a| format!(", {}", self.txt(a))).unwrap_or_default();
                let code = format!("agentmark_rt::stdlib::file::{}(ctx, {path}{more})?", fc.verb);
                match fc.verb.as_str() {
                    "read" | "list" | "exists" => Eff::value(code),
                    _ => Eff::lines(vec![format!("{code};")]),
                }
            }
            EffectCall::Std(x) => match &x.kind {
                ExprKind::Std { module, name, args, decode } => Eff::value(self.std_call(module, name, args, decode.as_ref(), true)),
                _ => Eff::value(self.val(x)),
            },
            EffectCall::Io(io) => match io {
                IoCall::ReadInput => Eff::value("agentmark_rt::stdlib::read_input()?".to_string()),
                IoCall::ReadFile { path, ty } => {
                    let p = self.txt(path);
                    let v = match ty {
                        Type::Text => format!("std::fs::read_to_string({p})?"),
                        Type::Bytes => format!("std::fs::read({p})?"),
                        _ => format!("read_data::<{}>({p})?", self.rty(ty)),
                    };
                    Eff::value(v)
                }
                IoCall::ReadBody { ty } => Eff::value(format!(
                    "serde_json::from_slice::<{}>(body).map_err(|err| BadRequest(err.to_string()))?",
                    self.rty(ty)
                )),
                IoCall::Print { message } => {
                    Eff::lines(vec![format!("say(ctx, {})?;", self.txt(message))])
                }
            },
            EffectCall::Trace(TraceEvent::Guard { name, ok }) => {
                Eff::lines(vec![format!("ctx.trace_guard({}, {})?;", lit(name), self.ex(ok).s)])
            }
        }
    }

    /// A call of the built-in library (SPEC 22.3). `question`: turn a `result` into its value.
    fn std_call(&self, module: &str, name: &str, args: &[Expr], decode: Option<&Type>, question: bool) -> String {
        use super::stdlib::Pass;
        let Some(f) = crate::stdlib::lookup(module, name) else {
            panic!("internal compiler error: {module}.{name} is not in the library table");
        };
        let rendered: Vec<String> = f
            .params
            .iter()
            .zip(args)
            .map(|((_, want), a)| match super::stdlib::pass(*want) {
                Pass::Text => self.txt(a),
                Pass::Ref => format!("&{}", paren(self.ex(a), P_UNARY)),
                Pass::Value => self.val(a),
            })
            .collect();
        let num = args
            .first()
            .filter(|_| f.params.iter().any(|p| p.1 == crate::stdlib::Ty::Num))
            .map(|a| self.rty(&a.ty));
        let dec = decode.map(|t| match (module, t) {
            ("csv", Type::List(item)) => self.rty(item),
            _ => self.rty(t),
        });
        super::stdlib::call(f, &rendered, num.as_deref(), dec.as_deref(), question)
    }

    fn fn_call(&self, name: &str, args: &[Expr]) -> String {
        let f = self.p.functions.iter().find(|f| f.name == name);
        let a = self.args(args, f.map(|f| f.params.as_slice()));
        let home = self.n.fns.get(name);
        let path = match home {
            Some(h) if h.module == self.module => h.ident.clone(),
            Some(h) => format!("{}::{}", h.module, h.ident),
            None => value_ident(name),
        };
        if a.is_empty() {
            format!("{path}(ctx).await?")
        } else {
            format!("{path}(ctx, {a}).await?")
        }
    }

    /// Arguments of a binding call: handles are lent (`&h`, `&mut h`) or handed over (`h`)
    /// as the item's parameter says; everything else is a value.
    fn binding_args(&self, args: &[Expr], item: Option<&BindingFn>) -> String {
        args.iter()
            .enumerate()
            .map(|(i, a)| {
                let p = item.and_then(|b| b.params.get(i));
                let mode = item.and_then(|b| b.modes.get(i)).copied().unwrap_or(HandleMode::Shared);
                match (p, &a.ty) {
                    (Some(_), Type::Handle(_)) => match mode {
                        HandleMode::Shared => format!("&{}", paren(self.ex(a), P_ATOM)),
                        HandleMode::Mut => format!("&mut {}", paren(self.ex(a), P_ATOM)),
                        HandleMode::Consume => self.val(a),
                    },
                    (Some(p), _) => self.conv(a, &p.ty),
                    (None, _) => self.val(a),
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn args(&self, args: &[Expr], params: Option<&[Param]>) -> String {
        args.iter()
            .enumerate()
            .map(|(i, a)| match params.and_then(|p| p.get(i)) {
                Some(p) => self.conv(a, &p.ty),
                None => self.val(a),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn tool_call(&self, t: &ToolCall, into: Option<&LocalVar>) -> Eff {
        let verb = t.verb.clone().unwrap_or_default();
        let call = |args: String| -> String {
            format!("ctx.tools.call({}, {}, &{args}).await?", lit(&t.tool), lit(&verb))
        };
        let mut temps: Vec<String> = Vec::new();
        let once = |temps: &mut Vec<String>, name: &str, e: &Expr, as_ref: bool| -> String {
            let s = if as_ref { self.txt(e) } else { self.val(e) };
            if cheap(e) {
                s
            } else {
                temps.push(format!("let {name} = {};", if as_ref { self.val(e) } else { s }));
                if as_ref {
                    format!("&{name}")
                } else {
                    name.to_string()
                }
            }
        };
        // the result of the call as the statement's value
        let result = |raw: &str| -> String {
            match into {
                Some(v) if !matches!(v.ty, Type::Unknown | Type::Object(_)) => {
                    format!("serde_json::from_value::<{}>({raw})?", self.rty(&v.ty))
                }
                _ => raw.to_string(),
            }
        };

        match t.tool.as_str() {
            "log" => {
                let msg = match &t.payload {
                    Some(Payload::Value(e)) => self.txt(e),
                    _ => "\"\"".into(),
                };
                Eff::lines(vec![format!("log(ctx, {msg})?;")])
            }
            "email" if t.verb.as_deref() == Some("send") => {
                let (word, to_e) = match &t.target {
                    Some(Target::Field { word, value, .. }) => (word.clone(), Some(value)),
                    Some(Target::Literal { word, value }) => (word.clone().unwrap_or_default(), Some(value)),
                    Some(Target::Source { word, id, .. }) => (word.clone(), id.as_ref()),
                    None => (String::new(), None),
                };
                let to_ref = match to_e {
                    Some(e) => once(&mut temps, "to", e, true),
                    None => "\"\"".into(),
                };
                let text = match &t.payload {
                    Some(Payload::Value(e)) => once(&mut temps, "text", e, true),
                    _ => "\"\"".into(),
                };
                let args = format!(
                    "json!({{\"target\": {}, \"to\": {}, \"text\": {}}})",
                    lit(&word),
                    to_ref.trim_start_matches('&'),
                    text.trim_start_matches('&')
                );
                let mut lines = vec![format!("{};", call(args))];
                lines.push(format!("ctx.trace_tool_email({}, {}, {})?;", lit(&verb), lit(&word), to_ref));
                Eff { temps, lines, value: None }
            }
            "crm" => {
                let (source, id_e) = match &t.target {
                    Some(Target::Source { source, id, .. }) => (source.clone(), id.as_ref()),
                    Some(Target::Literal { value, .. }) => (String::new(), Some(value)),
                    _ => (String::new(), None),
                };
                let id = match id_e {
                    Some(e) => once(&mut temps, "id", e, true),
                    None => "\"\"".into(),
                };
                let id_val = id.trim_start_matches('&').to_string();
                let mut fields = format!("\"source\": {}, \"id\": {}", lit(&source), id_val);
                let mut set_trace = "None".to_string();
                if let Some(p) = &t.payload {
                    let set = match p {
                        Payload::Assign(fs) => {
                            let inner = fs
                                .iter()
                                .map(|(k, v)| format!("{}: {}", lit(k), self.jv(v)))
                                .collect::<Vec<_>>()
                                .join(", ");
                            if fs.iter().all(|(_, v)| cheap(v)) {
                                format!("json!({{{inner}}})")
                            } else {
                                temps.push(format!("let set = json!({{{inner}}});"));
                                "set".into()
                            }
                        }
                        Payload::Value(e) => {
                            let j = self.json_of(e);
                            if cheap(e) {
                                j
                            } else {
                                temps.push(format!("let set = {j};"));
                                "set".into()
                            }
                        }
                    };
                    fields.push_str(&format!(", \"set\": {set}"));
                    set_trace = format!("Some({set})");
                }
                let raw = call(format!("json!({{{fields}}})"));
                let trace = format!(
                    "ctx.trace_tool_crm({}, {}, &{}, {})?;",
                    lit(&verb),
                    lit(&source),
                    id_val,
                    set_trace
                );
                if into.is_some() {
                    Eff {
                        temps,
                        lines: vec![format!("let row = {raw};"), trace],
                        value: Some(result("row")),
                    }
                } else {
                    Eff { temps, lines: vec![format!("{raw};"), trace], value: None }
                }
            }
            "http" => {
                let url = match &t.target {
                    Some(Target::Literal { value, .. }) => once(&mut temps, "url", value, true),
                    _ => "\"\"".into(),
                };
                let url_val = url.trim_start_matches('&').to_string();
                let mut fields = format!("\"url\": {url_val}");
                let mut body_trace = "None".to_string();
                if let Some(Payload::Value(e)) = &t.payload {
                    let j = self.json_of(e);
                    let b = if cheap(e) {
                        j
                    } else {
                        temps.push(format!("let body = {j};"));
                        "body".into()
                    };
                    fields.push_str(&format!(", \"body\": {b}"));
                    body_trace = format!("Some({b})");
                }
                let raw = call(format!("json!({{{fields}}})"));
                let trace = format!("ctx.trace_tool_http({}, {}, {})?;", lit(&verb), url, body_trace);
                if into.is_some() {
                    Eff { temps, lines: vec![format!("let reply = {raw};"), trace], value: Some(result("reply")) }
                } else {
                    Eff { temps, lines: vec![format!("{raw};"), trace], value: None }
                }
            }
            _ => {
                // a tool the trace schema has no kind for: record what was asked, generically
                let mut fields = String::new();
                if let Some(Target::Field { word, value, .. } | Target::Literal { word: Some(word), value }) = &t.target {
                    fields.push_str(&format!("\"target\": {}, \"value\": {}", lit(word), self.ex(value).s));
                }
                if let Some(Payload::Value(e)) = &t.payload {
                    if !fields.is_empty() {
                        fields.push_str(", ");
                    }
                    fields.push_str(&format!("\"payload\": {}", self.json_of(e)));
                }
                let args = format!("json!({{{fields}}})");
                let trace = format!(
                    "ctx.trace.record(&json!({{\"kind\": \"tool\", \"tool\": {}, \"verb\": {}, \"args\": {args}}}))?;",
                    lit(&t.tool),
                    lit(&verb)
                );
                if into.is_some() {
                    Eff { temps, lines: vec![format!("let reply = {};", call(args)), trace], value: Some(result("reply")) }
                } else {
                    Eff { temps, lines: vec![format!("{};", call(args)), trace], value: None }
                }
            }
        }
    }

    // ------------------------------------------------------------------------------ expressions

    /// An expression to be used by value: places of non-`Copy` types are cloned.
    pub fn val(&self, e: &Expr) -> String {
        let x = self.ex(e);
        // a handle moves; it is never cloned
        if is_place(e) && !is_copy(&e.ty) && !matches!(e.ty, Type::Handle(_)) {
            format!("{}.clone()", paren(x, P_ATOM))
        } else {
            x.s
        }
    }

    /// By value, converted to the type the receiver wants (`T` into `maybe T`, a record literal
    /// into a record, a record into a JSON value).
    fn conv(&self, e: &Expr, to: &Type) -> String {
        bare(self.conv_inner(e, to))
    }

    fn conv_inner(&self, e: &Expr, to: &Type) -> String {
        let v = self.val(e);
        match (to, &e.ty) {
            (Type::Maybe(_), Type::Maybe(_)) => v,
            (Type::Maybe(inner), from) if !matches!(from, Type::Unknown) => {
                if **inner == Type::Unknown {
                    format!("Some({v})")
                } else {
                    format!("Some({})", self.conv_plain(&v, from, inner))
                }
            }
            (to, from) => self.conv_plain(&v, from, to),
        }
    }

    fn conv_plain(&self, v: &str, from: &Type, to: &Type) -> String {
        match (to, from) {
            (Type::Record(r), Type::Object(_)) => format!("serde_json::from_value::<{}>({v})?", self.n.record(r)),
            (Type::Unknown | Type::Object(_), Type::Record(_) | Type::List(_) | Type::Money | Type::Text | Type::Int | Type::Bool | Type::Date)
                if !matches!(from, Type::Object(_)) =>
            {
                format!("serde_json::to_value(&{v})?")
            }
            (Type::Record(_) | Type::List(_), Type::Unknown) => {
                format!("serde_json::from_value::<{}>({v})?", self.rty(to))
            }
            _ => v.to_string(),
        }
    }

    /// An expression as a `&str`-compatible argument: a literal stays a literal, anything else is borrowed.
    fn txt(&self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Str(s) => lit(s),
            _ => {
                let x = self.ex(e);
                format!("&{}", paren(x, P_UNARY))
            }
        }
    }

    /// The JSON value of an expression, owned.
    fn json_of(&self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Record(_) => self.ex(e).s,
            _ if matches!(e.ty, Type::Unknown | Type::Object(_)) => self.val(e),
            _ => format!("serde_json::to_value(&{})?", paren(self.ex(e), P_UNARY)),
        }
    }

    /// A value inside `json!({...})`: text literals stay literals.
    fn jv(&self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Str(s) => lit(s),
            _ => self.ex(e).s,
        }
    }

    /// The JSON value of an expression as a borrowable temporary.
    fn json_of_ref(&self, e: &Expr) -> String {
        self.json_of(e)
    }

    fn ex(&self, e: &Expr) -> E {
        match &e.kind {
            ExprKind::Str(s) => atom(format!("{}.to_string()", lit(s))),
            ExprKind::Int(i) => {
                if *i < 0 {
                    E { s: i.to_string(), p: P_UNARY }
                } else {
                    atom(i.to_string())
                }
            }
            ExprKind::Money { minor, currency } => atom(format!("Money::new({minor}, Currency::{currency})")),
            ExprKind::Duration(d) => atom(duration_lit(d)),
            ExprKind::Date(s) => {
                let mut it = s.split('-');
                let (y, m, d) = (it.next().unwrap_or("1970"), it.next().unwrap_or("1"), it.next().unwrap_or("1"));
                let (y, m, d) = (y.parse::<i32>().unwrap_or(1970), m.parse::<u32>().unwrap_or(1), d.parse::<u32>().unwrap_or(1));
                atom(format!("NaiveDate::from_ymd_opt({y}, {m}, {d}).expect(\"{s} is a valid date\")"))
            }
            ExprKind::Bool(b) => atom(b.to_string()),
            ExprKind::Today => atom("ctx.clock.today()"),
            ExprKind::Now => atom("ctx.clock.now()"),
            ExprKind::Var(v) => atom(self.n.local(v)),
            ExprKind::Field { base, field } => {
                let b = paren(self.ex(base), P_ATOM);
                atom(format!("{b}.{}", field_ident(field).0))
            }
            ExprKind::Unwrap { inner, what } => {
                if let ExprKind::Var(v) = &inner.kind {
                    if let Some(name) = self.narrowed.get(v) {
                        return atom(name.clone());
                    }
                }
                let b = paren(self.ex(inner), P_ATOM);
                let msg = lit(&format!("{what} is not empty here"));
                if is_copy(&e.ty) {
                    atom(format!("{b}.expect({msg})"))
                } else {
                    atom(format!("{b}.as_ref().expect({msg})"))
                }
            }
            ExprKind::Interp(parts) => atom(self.interp(parts)),
            ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs),
            ExprKind::Not(x) => E { s: format!("!{}", paren(self.ex(x), P_UNARY)), p: P_UNARY },
            ExprKind::Call { callee, name, args } => match callee {
                Callee::Fn => atom(self.fn_call(name, args)),
                Callee::Rust => {
                    let f = self.n.rust_fns.get(name).cloned().unwrap_or_else(|| value_ident(name));
                    let params = self.p.rust_fns.iter().find(|r| &r.name == name).map(|r| r.params.clone());
                    atom(format!("bindings::{f}({})", self.args(args, params.as_deref())))
                }
            },
            ExprKind::Std { module, name, args, decode } => atom(self.std_call(module, name, args, decode.as_ref(), false)),
            ExprKind::Record(fields) => {
                let inner = fields
                    .iter()
                    .map(|(k, v)| format!("{}: {}", lit(k), self.jv(v)))
                    .collect::<Vec<_>>()
                    .join(", ");
                atom(format!("json!({{{inner}}})"))
            }
            ExprKind::Source { tool, source, .. } => {
                atom(format!("tools::read_source::<{}>(ctx, {}, {}).await?", self.rty(&e.ty), lit(tool), lit(source)))
            }
            ExprKind::Count { source, filter } => match filter {
                Some(f) => {
                    let h = self.n.counters.get(f).cloned().unwrap_or_else(|| format!("count_{f}"));
                    atom(format!("rules::{h}(ctx, {}).await?", self.val(source)))
                }
                None => atom(format!("({}.len() as i64)", paren(self.ex(source), P_ATOM))),
            },
            ExprKind::First { var, source, cond } => self.first(var, source, cond),
            ExprKind::Coll(op) => self.coll_expr(e, op),
            ExprKind::IsEmpty(x) => self.emptiness(x, true),
            ExprKind::IsNotEmpty(x) => self.emptiness(x, false),
            ExprKind::Matches { expr, record } => match &expr.kind {
                ExprKind::Var(v) if self.lenient.contains(v) => atom(format!("{}_matches", self.n.local(v))),
                _ if matches!(expr.ty, Type::Record(_)) => atom("true"),
                _ => atom(format!(
                    "serde_json::from_value::<{}>({}).is_ok()",
                    self.n.record(record),
                    self.val(expr)
                )),
            },
            ExprKind::TextToBytes(x) => atom(format!("{}.as_bytes().to_vec()", paren(self.ex(x), P_ATOM))),
            ExprKind::Length(x) => {
                let b = paren(self.ex(x), P_ATOM);
                match x.ty {
                    Type::Text => atom(format!("({b}.chars().count() as i64)")),
                    _ => atom(format!("({b}.len() as i64)")),
                }
            }
            ExprKind::NoAmountsChanged { output, input } => {
                let out = format!("&{}", paren(self.ex(output), P_ATOM));
                let inp = match input.ty {
                    Type::Text => format!("&serde_json::Value::String({})", self.val(input)),
                    _ => format!("&{}", self.json_of(input)),
                };
                atom(format!("no_amounts_changed({out}, {inp})"))
            }
        }
    }

    fn emptiness(&self, x: &Expr, empty: bool) -> E {
        let b = paren(self.ex(x), P_ATOM);
        match (&x.ty, empty) {
            (Type::Maybe(_), true) => atom(format!("{b}.is_none()")),
            (Type::Maybe(_), false) => atom(format!("{b}.is_some()")),
            (_, true) => atom(format!("{b}.is_empty()")),
            (_, false) => E { s: format!("!{b}.is_empty()"), p: P_UNARY },
        }
    }

    fn first(&self, var: &str, source: &Expr, cond: &Expr) -> E {
        let v = self.n.local(var);
        let simple = !has_await(cond) && !uses_bare(cond, var);
        let place = is_place(source);
        if simple {
            let src = paren(self.ex(source), P_ATOM);
            if place {
                atom(format!("{src}.iter().find(|{v}| {}).cloned()", self.ex(cond).s))
            } else {
                atom(format!("{src}.into_iter().find(|{v}| {})", self.ex(cond).s))
            }
        } else {
            atom(format!(
                "{{\n    let mut found = None;\n    for {v} in {} {{\n        if {} {{\n            found = Some({v});\n            break;\n        }}\n    }}\n    found\n}}",
                self.val(source),
                self.ex(cond).s
            ))
        }
    }

    fn interp(&self, parts: &[InterpPart]) -> String {
        if let [InterpPart::Expr(e)] = parts {
            if e.ty == Type::Text {
                return self.val(e);
            }
        }
        let mut fmt = String::new();
        let mut args: Vec<String> = Vec::new();
        for p in parts {
            match p {
                InterpPart::Text(t) => fmt.push_str(&t.replace('{', "{{").replace('}', "}}")),
                InterpPart::Expr(e) => {
                    fmt.push_str("{}");
                    args.push(self.ex(e).s);
                }
            }
        }
        if args.is_empty() {
            return format!("{}.to_string()", lit(&fmt.replace("{{", "{").replace("}}", "}")));
        }
        format!("format!({}, {})", lit(&fmt), args.join(", "))
    }

    fn binary(&self, op: BinOp, l: &Expr, r: &Expr) -> E {
        let money = |t: &Type| *t == Type::Money;
        let int = |t: &Type| *t == Type::Int;
        match op {
            BinOp::Or => self.infix("||", P_OR, l, r),
            BinOp::And => self.infix("&&", P_AND, l, r),
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                let sym = match op {
                    BinOp::Eq => "==",
                    BinOp::Ne => "!=",
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::Le => "<=",
                    _ => ">=",
                };
                // a literal text next to a text place compares as &str; a lone `maybe` is lifted
                let (ls, rs) = match (&l.ty, &r.ty) {
                    (Type::Maybe(_), t) if !matches!(t, Type::Maybe(_)) => (paren(self.ex(l), P_ADD), format!("Some({})", self.val(r))),
                    (t, Type::Maybe(_)) if !matches!(t, Type::Maybe(_)) => (format!("Some({})", self.val(l)), paren(self.ex(r), P_ADD)),
                    _ => (paren(self.cmp_operand(l), P_ADD), paren(self.cmp_operand(r), P_ADD)),
                };
                E { s: format!("{ls} {sym} {rs}"), p: P_CMP }
            }
            BinOp::Add if money(&l.ty) && money(&r.ty) => {
                atom(format!("{}.checked_add({})?", paren(self.ex(l), P_ATOM), self.ex(r).s))
            }
            BinOp::Sub if money(&l.ty) && money(&r.ty) => {
                atom(format!("{}.checked_sub({})?", paren(self.ex(l), P_ATOM), self.ex(r).s))
            }
            BinOp::Mul if money(&l.ty) && int(&r.ty) => {
                atom(format!("{}.mul_int({})?", paren(self.ex(l), P_ATOM), self.ex(r).s))
            }
            BinOp::Mul if int(&l.ty) && money(&r.ty) => {
                atom(format!("{}.mul_int({})?", paren(self.ex(r), P_ATOM), self.ex(l).s))
            }
            BinOp::Div if money(&l.ty) && int(&r.ty) => {
                atom(format!("{}.div_int({})?", paren(self.ex(l), P_ATOM), self.ex(r).s))
            }
            BinOp::Mul if l.ty == Type::Duration && int(&r.ty) => {
                E { s: format!("{} * ({} as i32)", paren(self.ex(l), P_MUL), self.ex(r).s), p: P_MUL }
            }
            BinOp::Mul if int(&l.ty) && r.ty == Type::Duration => {
                E { s: format!("{} * ({} as i32)", paren(self.ex(r), P_MUL), self.ex(l).s), p: P_MUL }
            }
            BinOp::Add if l.ty == Type::Text => atom(format!("format!(\"{{}}{{}}\", {}, {})", self.ex(l).s, self.ex(r).s)),
            BinOp::Add => self.infix("+", P_ADD, l, r),
            BinOp::Sub => self.infix("-", P_ADD, l, r),
            BinOp::Mul => self.infix("*", P_MUL, l, r),
            BinOp::Div => self.infix("/", P_MUL, l, r),
        }
    }

    fn cmp_operand(&self, e: &Expr) -> E {
        match &e.kind {
            ExprKind::Str(s) => atom(lit(s)),
            _ => self.ex(e),
        }
    }

    fn infix(&self, sym: &str, p: u8, l: &Expr, r: &Expr) -> E {
        let ls = paren(self.ex(l), p);
        let rs = paren(self.ex(r), p + 1);
        E { s: format!("{ls} {sym} {rs}"), p }
    }
}

// ---------------------------------------------------------------------------------- helpers

pub fn duration_lit(d: &Duration) -> String {
    let n = d.amount;
    match d.unit {
        DurationUnit::Second => format!("Duration::seconds({n})"),
        DurationUnit::Minute => format!("Duration::minutes({n})"),
        DurationUnit::Hour => format!("Duration::hours({n})"),
        DurationUnit::Day => format!("Duration::days({n})"),
        DurationUnit::Week => format!("Duration::weeks({n})"),
        DurationUnit::Month => format!("Duration::days({})", n * 30),
        DurationUnit::Year => format!("Duration::days({})", n * 365),
    }
}

/// `(xs.len() as i64)` as a whole value needs no parentheses (rustc warns about them); inside an
/// operator it does, and `ex` keeps them for that.
fn bare(s: String) -> String {
    if !(s.starts_with('(') && s.ends_with(')')) {
        return s;
    }
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && i + 1 != s.len() {
                    return s; // the first group closes before the end: `(a) + (b)`
                }
            }
            _ => {}
        }
    }
    s[1..s.len() - 1].to_string()
}

/// A variable, a field of one, or a narrowed `maybe`: something you can read without computing.
fn is_place(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Var(_) => true,
        ExprKind::Field { base, .. } => is_place(base) || matches!(base.kind, ExprKind::Unwrap { .. }),
        ExprKind::Unwrap { inner, .. } => is_place(inner),
        _ => false,
    }
}

/// Free of calls that wait (`.await`), so it may be written twice.
fn cheap(e: &Expr) -> bool {
    !has_await(e)
}

fn has_await(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Call { .. } | ExprKind::Source { .. } | ExprKind::Coll(_) => true,
        ExprKind::Std { args, .. } => args.iter().any(has_await),
        ExprKind::Count { filter, source } => filter.is_some() || has_await(source),
        ExprKind::Field { base, .. } => has_await(base),
        ExprKind::Unwrap { inner, .. } => has_await(inner),
        ExprKind::Interp(parts) => parts.iter().any(|p| matches!(p, InterpPart::Expr(e) if has_await(e))),
        ExprKind::Binary { lhs, rhs, .. } => has_await(lhs) || has_await(rhs),
        ExprKind::Not(x) | ExprKind::IsEmpty(x) | ExprKind::IsNotEmpty(x) | ExprKind::Length(x) | ExprKind::TextToBytes(x) => has_await(x),
        ExprKind::Record(fs) => fs.iter().any(|(_, v)| has_await(v)),
        ExprKind::First { source, cond, .. } => has_await(source) || has_await(cond),
        ExprKind::Matches { expr, .. } => has_await(expr),
        ExprKind::NoAmountsChanged { output, input } => has_await(output) || has_await(input),
        _ => false,
    }
}

/// Is `var` used as a whole value (not just through its fields)?
fn uses_bare(e: &Expr, var: &str) -> bool {
    match &e.kind {
        ExprKind::Var(v) => v == var,
        ExprKind::Field { base, .. } => match &base.kind {
            ExprKind::Var(_) => false,
            _ => uses_bare(base, var),
        },
        ExprKind::Unwrap { inner, .. } => uses_bare(inner, var),
        ExprKind::Interp(parts) => parts.iter().any(|p| matches!(p, InterpPart::Expr(e) if uses_bare(e, var))),
        ExprKind::Binary { lhs, rhs, .. } => uses_bare(lhs, var) || uses_bare(rhs, var),
        ExprKind::Not(x) | ExprKind::IsEmpty(x) | ExprKind::IsNotEmpty(x) | ExprKind::Length(x) | ExprKind::TextToBytes(x) => uses_bare(x, var),
        ExprKind::Call { args, .. } | ExprKind::Std { args, .. } => args.iter().any(|a| uses_bare(a, var)),
        ExprKind::Record(fs) => fs.iter().any(|(_, v)| uses_bare(v, var)),
        ExprKind::Count { source, .. } => uses_bare(source, var),
        ExprKind::First { source, cond, .. } => uses_bare(source, var) || uses_bare(cond, var),
        ExprKind::Matches { expr, .. } => uses_bare(expr, var),
        ExprKind::NoAmountsChanged { output, input } => uses_bare(output, var) || uses_bare(input, var),
        ExprKind::Coll(op) => op.children().into_iter().any(|x| uses_bare(x, var)),
        _ => false,
    }
}

/// Does `otherwise` set `name` or leave the block (so no default is needed)?
fn assigns_or_leaves(stmts: &[Stmt], name: &str) -> bool {
    let mut found = false;
    visit_stmts(stmts, &mut |s| {
        if matches!(&s.kind, StmtKind::Assign { name: n, .. } if n == name)
            || matches!(s.kind, StmtKind::Stop | StmtKind::Return(_) | StmtKind::Respond { .. })
        {
            found = true;
        }
    });
    found
}

pub fn visit_stmts(stmts: &[Stmt], f: &mut dyn FnMut(&Stmt)) {
    for s in stmts {
        f(s);
        match &s.kind {
            StmtKind::If { then, else_, .. } | StmtKind::IfSome { then, else_, .. } => {
                visit_stmts(then, f);
                visit_stmts(else_, f);
            }
            StmtKind::ForEach { body, .. } => visit_stmts(body, f),
            StmtKind::Coll(c) => {
                if let Some(b) = c.body() {
                    visit_stmts(b, f);
                }
            }
            StmtKind::Parallel(b) => visit_stmts(b, f),
            StmtKind::Try { stmt, otherwise, .. } => {
                visit_stmts(std::slice::from_ref(stmt), f);
                if let Some(o) = otherwise {
                    visit_stmts(o, f);
                }
            }
            _ => {}
        }
    }
}

/// Handle variables a binding call borrows mutably (`mut handle of …`): they need `let mut`.
fn collect_mut_handles(p: &Program, stmts: &[Stmt], out: &mut HashSet<String>) {
    visit_stmts(stmts, &mut |s| {
        if let StmtKind::Effect { call: EffectCall::Binding { binding, item, on, args }, .. } = &s.kind {
            let Some(bf) = p.bindings.iter().find(|b| &b.binding == binding && &b.item == item && &b.on == on) else {
                return;
            };
            for (m, a) in bf.modes.iter().zip(args) {
                if let (HandleMode::Mut, ExprKind::Var(v)) = (m, &a.kind) {
                    out.insert(v.clone());
                }
            }
        }
    });
}

fn collect_assigned(stmts: &[Stmt], out: &mut HashSet<String>) {
    visit_stmts(stmts, &mut |s| {
        match &s.kind {
            StmtKind::Assign { name, .. } => {
                out.insert(name.clone());
            }
            // changing a list or map in place needs a `mut` binding for the variable it lives in
            StmtKind::Append { list: place, .. }
            | StmtKind::Coll(crate::ir::collections::CollStmt::Put { map: place, .. })
            | StmtKind::Coll(crate::ir::collections::CollStmt::Remove { map: place, .. }) => {
                if let Some(v) = collections::root_var(place) {
                    out.insert(v.to_string());
                }
            }
            _ => {}
        }
    });
}

/// Variables some guard asks `x matches R` about.
fn collect_matches(stmts: &[Stmt], out: &mut HashSet<String>) {
    visit_stmts(stmts, &mut |s| {
        if let StmtKind::Let { value, .. } = &s.kind {
            find_matches(value, out);
        }
    });
}

fn find_matches(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Matches { expr, .. } => {
            if let ExprKind::Var(v) = &expr.kind {
                out.insert(v.clone());
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            find_matches(lhs, out);
            find_matches(rhs, out);
        }
        ExprKind::Not(x) => find_matches(x, out),
        _ => {}
    }
}

/// The expressions a statement evaluates itself (not those of the statements nested in it).
fn stmt_exprs(s: &Stmt) -> Vec<&Expr> {
    match &s.kind {
        StmtKind::Let { value, .. } | StmtKind::Assign { value, .. } => vec![value],
        StmtKind::If { cond, .. } => vec![cond],
        StmtKind::IfSome { expr, .. } => vec![expr],
        StmtKind::ForEach { source, .. } => vec![source],
        StmtKind::Return(e) => vec![e],
        StmtKind::Respond { body, .. } => body.iter().collect(),
        StmtKind::Append { value, list } => vec![value, list],
        StmtKind::Coll(c) => c.exprs(),
        StmtKind::Effect { call, .. } => call_exprs(call),
        StmtKind::Try { stmt, .. } => stmt_exprs(stmt),
        StmtKind::Stop | StmtKind::Parallel(_) => vec![],
    }
}

fn call_exprs(call: &EffectCall) -> Vec<&Expr> {
    match call {
        EffectCall::Tool(t) => {
            let mut v: Vec<&Expr> = Vec::new();
            match &t.target {
                Some(Target::Field { value, .. }) | Some(Target::Literal { value, .. }) => v.push(value),
                Some(Target::Source { id, .. }) => v.extend(id.iter()),
                None => {}
            }
            match &t.payload {
                Some(Payload::Assign(fs)) => v.extend(fs.iter().map(|(_, e)| e)),
                Some(Payload::Value(e)) => v.push(e),
                None => {}
            }
            v
        }
        EffectCall::Model(m) => std::iter::once(&m.prompt).chain(m.context.iter()).collect(),
        EffectCall::Binding { args, .. } | EffectCall::Rust { args, .. } | EffectCall::Fn { args, .. } => args.iter().collect(),
        EffectCall::Io(IoCall::ReadFile { path, .. }) => vec![path],
        EffectCall::Io(IoCall::Print { message }) => vec![message],
        EffectCall::Io(IoCall::ReadBody { .. } | IoCall::ReadInput) => vec![],
        EffectCall::File(f) => std::iter::once(&f.path).chain(f.arg.iter()).collect(),
        EffectCall::Std(x) => vec![x],
        EffectCall::Trace(TraceEvent::Guard { ok, .. }) => vec![ok],
    }
}

/// Count `expect` reads of non-`Copy` `maybe` variables: `(variable, source text, reads)`.
fn count_unwraps(e: &Expr, out: &mut Vec<(String, String, usize)>) {
    match &e.kind {
        ExprKind::Unwrap { inner, what } => {
            if let ExprKind::Var(v) = &inner.kind {
                if !is_copy(&e.ty) {
                    match out.iter_mut().find(|(n, _, _)| n == v) {
                        Some(x) => x.2 += 1,
                        None => out.push((v.clone(), what.clone(), 1)),
                    }
                    return;
                }
            }
            count_unwraps(inner, out);
        }
        ExprKind::Field { base, .. } => count_unwraps(base, out),
        ExprKind::Interp(parts) => parts.iter().for_each(|p| {
            if let InterpPart::Expr(e) = p {
                count_unwraps(e, out)
            }
        }),
        ExprKind::Binary { lhs, rhs, .. } => {
            count_unwraps(lhs, out);
            count_unwraps(rhs, out);
        }
        ExprKind::Not(x) | ExprKind::IsEmpty(x) | ExprKind::IsNotEmpty(x) | ExprKind::Length(x) | ExprKind::TextToBytes(x) => count_unwraps(x, out),
        ExprKind::Call { args, .. } | ExprKind::Std { args, .. } => args.iter().for_each(|a| count_unwraps(a, out)),
        ExprKind::Record(fs) => fs.iter().for_each(|(_, v)| count_unwraps(v, out)),
        ExprKind::Count { source, .. } => count_unwraps(source, out),
        ExprKind::Matches { expr, .. } => count_unwraps(expr, out),
        ExprKind::NoAmountsChanged { output, input } => {
            count_unwraps(output, out);
            count_unwraps(input, out);
        }
        ExprKind::Coll(op) => op.children().into_iter().for_each(|x| count_unwraps(x, out)),
        _ => {}
    }
}
