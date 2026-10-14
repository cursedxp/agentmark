//! Collection words (SPEC 22.2) -> Rust.
//!
//! Every word becomes a block of plain `for` loops, never a closure: the per-element expressions
//! may call functions (`.await`) and use `?`, which closures cannot. Sorting, grouping, sums of
//! money and `unique` use the helpers of `agentmark_rt::collections`.

use super::*;
use crate::ir::collections::{CollOp, CollStmt, ELEM};

const RT: &str = "agentmark_rt::collections";

/// Indent the continuation lines of a (possibly multi-line) expression by `n` levels.
fn pad(s: &str, n: usize) -> String {
    let p = "    ".repeat(n);
    s.replace('\n', &format!("\n{p}"))
}

/// Does the generated Rust `code` use the identifier `name` (outside string literals)?
pub(super) fn mentions(code: &str, name: &str) -> bool {
    let mut plain = String::with_capacity(code.len());
    let (mut in_str, mut esc) = (false, false);
    for c in code.chars() {
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            plain.push(' ');
        } else {
            plain.push(c);
        }
    }
    super::super::files::has_word(&plain, name)
}

/// A block expression from its lines (each line one level in).
fn block(lines: Vec<String>) -> E {
    let mut s = String::from("{\n");
    for l in lines {
        for sub in l.split('\n') {
            s.push_str("    ");
            s.push_str(sub);
            s.push('\n');
        }
    }
    s.push('}');
    atom(s)
}

fn elem_of(t: &Type) -> Type {
    match t {
        Type::List(e) => (**e).clone(),
        _ => Type::Unknown,
    }
}

impl Body<'_> {
    fn item(&self) -> String {
        self.n.local(ELEM)
    }

    /// `Vec::<T>::new()` for a list type; plain `Vec::new()` when the type is not known.
    fn new_vec(&self, list: &Type) -> String {
        match elem_of(list) {
            Type::Unknown => "Vec::new()".into(),
            t => format!("Vec::<{}>::new()", self.rty(&t)),
        }
    }

    /// `std::collections::BTreeMap::<K, V>::new()`.
    pub(super) fn new_map(&self, map: &Type) -> String {
        match map {
            Type::Map(k, v) => format!(
                "std::collections::BTreeMap::<{}, {}>::new()",
                self.rty(k),
                self.rty(v)
            ),
            _ => "std::collections::BTreeMap::new()".into(),
        }
    }

    /// The name to bind the element to: `_item` when the generated code never reads it.
    fn elem_binding(&self, code: &[&String]) -> String {
        let item = self.item();
        if code.iter().any(|c| mentions(c, &item)) {
            item
        } else {
            format!("_{item}")
        }
    }

    /// Collect the per-element value `x` of every element into `values`, then `finish(values)`.
    fn values_loop(&self, source: &Expr, x: &Expr, finish: impl std::ops::Fn(&str) -> String) -> E {
        let value = self.val(x);
        let var = self.elem_binding(&[&value]);
        block(vec![
            "let mut values = Vec::new();".into(),
            format!("for {var} in {} {{", pad(&self.val(source), 1)),
            format!("    values.push({value});"),
            "}".into(),
            finish("values"),
        ])
    }

    pub(super) fn coll_expr(&self, e: &Expr, op: &CollOp) -> E {
        let item = self.item();
        match op {
            CollOp::List(items) => {
                if items.is_empty() {
                    return atom(self.new_vec(&e.ty));
                }
                let elem = elem_of(&e.ty);
                let parts: Vec<String> = items.iter().map(|x| self.conv(x, &elem)).collect();
                atom(format!("vec![{}]", parts.join(", ")))
            }
            CollOp::Map(entries) => {
                if entries.is_empty() {
                    return atom(self.new_map(&e.ty));
                }
                let (kt, vt) = match &e.ty {
                    Type::Map(k, v) => ((**k).clone(), (**v).clone()),
                    _ => (Type::Unknown, Type::Unknown),
                };
                let parts: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("({}, {})", self.conv(k, &kt), self.conv(v, &vt)))
                    .collect();
                atom(format!("std::collections::BTreeMap::from([{}])", parts.join(", ")))
            }
            CollOp::Where { source, cond } => block(vec![
                format!("let mut out = {};", self.new_vec(&e.ty)),
                format!("for {item} in {} {{", pad(&self.val(source), 1)),
                format!("    if {} {{", self.ex(cond).s),
                format!("        out.push({item});"),
                "    }".into(),
                "}".into(),
                "out".into(),
            ]),
            CollOp::CountWhere { source, cond } => {
                let cond = self.ex(cond).s;
                let var = self.elem_binding(&[&cond]);
                block(vec![
                "let mut count: i64 = 0;".into(),
                format!("for {var} in {} {{", pad(&self.val(source), 1)),
                format!("    if {cond} {{"),
                "        count += 1;".into(),
                "    }".into(),
                "}".into(),
                "count".into(),
                ])
            }
            CollOp::MapTo { source, expr } => {
                let value = self.val(expr);
                let var = self.elem_binding(&[&value]);
                block(vec![
                    format!("let mut out = {};", self.new_vec(&e.ty)),
                    format!("for {var} in {} {{", pad(&self.val(source), 1)),
                    format!("    out.push({value});"),
                    "}".into(),
                    "out".into(),
                ])
            }
            CollOp::Sort {
                source,
                key,
                descending,
            } => block(vec![
                "let mut keyed = Vec::new();".into(),
                format!("for {item} in {} {{", pad(&self.val(source), 1)),
                format!("    let key = {};", self.val(key)),
                format!("    keyed.push((key, {item}));"),
                "}".into(),
                format!("{RT}::sorted(keyed, {descending})?"),
            ]),
            CollOp::Group { source, key } => block(vec![
                format!("let mut groups = {};", self.new_map(&e.ty)),
                format!("for {item} in {} {{", pad(&self.val(source), 1)),
                format!("    let key = {};", self.val(key)),
                format!("    groups.entry(key).or_insert_with(Vec::new).push({item});"),
                "}".into(),
                "groups".into(),
            ]),
            CollOp::Sum { source, expr } => {
                let money = e.ty == Type::Money;
                match (expr, money) {
                    (None, true) => atom(format!("{RT}::sum_money({})?", self.val(source))),
                    (None, false) => atom(format!("{}.into_iter().sum::<i64>()", paren(atom(self.val(source)), P_ATOM))),
                    (Some(x), true) => self.values_loop(source, x, |v| format!("{RT}::sum_money({v})?")),
                    (Some(x), false) => {
                        // plain `+=`, like `+` on numbers anywhere else
                        let value = self.val(x);
                        let var = self.elem_binding(&[&value]);
                        block(vec![
                            "let mut total: i64 = 0;".into(),
                            format!("for {var} in {} {{", pad(&self.val(source), 1)),
                            format!("    total += {value};"),
                            "}".into(),
                            "total".into(),
                        ])
                    }
                }
            }
            CollOp::Extreme { max, source, expr } => match expr {
                None => atom(format!("{RT}::extreme({}, {max})?", self.val(source))),
                Some(x) => self.values_loop(source, x, |v| format!("{RT}::extreme({v}, {max})?")),
            },
            CollOp::Edge { last, source } => {
                if is_place(source) {
                    let src = paren(self.ex(source), P_ATOM);
                    atom(format!("{src}.{}().cloned()", if *last { "last" } else { "first" }))
                } else {
                    let src = paren(self.ex(source), P_ATOM);
                    atom(format!("{src}.into_iter().{}()", if *last { "last" } else { "next" }))
                }
            }
            CollOp::Slice {
                skip,
                count,
                source,
            } => {
                let n = format!("usize::try_from({}).unwrap_or(0)", self.ex(count).s);
                let src = paren(atom(self.val(source)), P_ATOM);
                let word = if *skip { "skip" } else { "take" };
                atom(format!("{src}.into_iter().{word}({n}).collect::<Vec<_>>()"))
            }
            CollOp::Unique { source, expr } => match expr {
                None => atom(format!("{RT}::unique({})", self.val(source))),
                Some(x) => self.values_loop(source, x, |v| format!("{RT}::unique({v})")),
            },
            CollOp::Join { source, sep } => {
                let src = paren(self.ex(source), P_ATOM);
                atom(format!("{src}.join({})", self.txt(sep)))
            }
            CollOp::At { map, key } => {
                let m = paren(self.ex(map), P_ATOM);
                atom(format!("{m}.get(&{}).cloned()", paren(self.ex(key), P_UNARY)))
            }
            CollOp::MapPart { values, map } => {
                let m = paren(self.ex(map), P_ATOM);
                let word = if *values { "values" } else { "keys" };
                atom(format!("{m}.{word}().cloned().collect::<Vec<_>>()"))
            }
        }
    }

    pub(super) fn coll_stmt(&mut self, c: &CollStmt) {
        match c {
            CollStmt::Put { value, key, map } => {
                let (kt, vt) = match &map.ty {
                    Type::Map(k, v) => ((**k).clone(), (**v).clone()),
                    _ => (key.ty.clone(), value.ty.clone()),
                };
                let m = paren(self.ex(map), P_ATOM);
                let (k, v) = (self.conv(key, &kt), self.conv(value, &vt));
                self.line(&format!("{m}.insert({k}, {v});"));
            }
            CollStmt::Remove { key, map } => {
                let m = paren(self.ex(map), P_ATOM);
                let k = paren(self.ex(key), P_UNARY);
                self.line(&format!("{m}.remove(&{k});"));
            }
            CollStmt::ForEachIndex {
                var,
                index,
                source,
                body,
            } => {
                let src = self.val(source);
                let (v, i) = (self.n.local(&var.name), self.n.local(&index.name));
                let start = self.out.len();
                self.line("for _ in {}");
                self.nested(body);
                self.line("}");
                let used = |this: &Self, name: &str| this.body_mentions(start + 1, name);
                let vm = if self.assigned.contains(&var.name) { "mut " } else { "" };
                let v_bound = if used(self, &v) { format!("{vm}{v}") } else { format!("_{v}") };
                let pos = format!("{i}_pos");
                let (im, i_used) = (if self.assigned.contains(&index.name) { "mut " } else { "" }, used(self, &i));
                let ind = "    ".repeat(self.indent);
                if i_used {
                    self.out.insert(start + 1, format!("{ind}    let {im}{i} = {pos} as i64 + 1;"));
                }
                let pos_bound = if i_used { pos } else { "_pos".to_string() };
                self.out[start] = format!("{ind}for ({pos_bound}, {v_bound}) in {src}.into_iter().enumerate() {{");
            }
            CollStmt::ForEachEntry {
                key,
                value,
                source,
                body,
            } => {
                let src = self.val(source);
                let (k, v) = (self.n.local(&key.name), self.n.local(&value.name));
                let start = self.out.len();
                self.line("for _ in {}");
                self.nested(body);
                self.line("}");
                let bind = |this: &Self, var: &LocalVar, id: &str| {
                    if !this.body_mentions(start + 1, id) {
                        format!("_{id}")
                    } else if this.assigned.contains(&var.name) {
                        format!("mut {id}")
                    } else {
                        id.to_string()
                    }
                };
                let (kb, vb) = (bind(self, key, &k), bind(self, value, &v));
                let ind = "    ".repeat(self.indent);
                self.out[start] = format!("{ind}for ({kb}, {vb}) in {src} {{");
            }
        }
    }

    /// Does any line written since `from` use the identifier `name`?
    pub(super) fn body_mentions(&self, from: usize, name: &str) -> bool {
        self.out[from..].iter().any(|l| !l.trim_start().starts_with("//") && mentions(l, name))
    }
}

/// The variable at the root of a place (`totals`, `state.counts`, `row.items`).
pub(super) fn root_var(e: &Expr) -> Option<&str> {
    match &e.kind {
        ExprKind::Var(v) => Some(v),
        ExprKind::Field { base, .. } => root_var(base),
        ExprKind::Unwrap { inner, .. } => root_var(inner),
        _ => None,
    }
}
