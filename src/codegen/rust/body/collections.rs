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

    /// The per-element value converted to what the collection holds.
    fn elem_val(&self, x: &Expr) -> String {
        self.val(x)
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
            CollOp::CountWhere { source, cond } => block(vec![
                "let mut count: i64 = 0;".into(),
                format!("for {item} in {} {{", pad(&self.val(source), 1)),
                format!("    if {} {{", self.ex(cond).s),
                "        count += 1;".into(),
                "    }".into(),
                "}".into(),
                "count".into(),
            ]),
            CollOp::MapTo { source, expr } => block(vec![
                format!("let mut out = {};", self.new_vec(&e.ty)),
                format!("for {item} in {} {{", pad(&self.val(source), 1)),
                format!("    out.push({});", self.elem_val(expr)),
                "}".into(),
                "out".into(),
            ]),
            CollOp::Sort {
                source,
                key,
                descending,
            } => block(vec![
                "let mut keyed = Vec::new();".into(),
                format!("for {item} in {} {{", pad(&self.val(source), 1)),
                format!("    let key = {};", self.elem_val(key)),
                format!("    keyed.push((key, {item}));"),
                "}".into(),
                format!("{RT}::sorted(keyed, {descending})?"),
            ]),
            CollOp::Group { source, key } => block(vec![
                format!("let mut groups = {};", self.new_map(&e.ty)),
                format!("for {item} in {} {{", pad(&self.val(source), 1)),
                format!("    let key = {};", self.elem_val(key)),
                format!("    groups.entry(key).or_insert_with(Vec::new).push({item});"),
                "}".into(),
                "groups".into(),
            ]),
            CollOp::Sum { source, expr } => {
                let money = e.ty == Type::Money;
                match (expr, money) {
                    (None, true) => atom(format!("{RT}::sum_money({})?", self.val(source))),
                    (None, false) => atom(format!("{}.into_iter().sum::<i64>()", paren(atom(self.val(source)), P_ATOM))),
                    (Some(x), true) => block(vec![
                        "let mut values = Vec::new();".into(),
                        format!("for {item} in {} {{", pad(&self.val(source), 1)),
                        format!("    values.push({});", self.elem_val(x)),
                        "}".into(),
                        format!("{RT}::sum_money(values)?"),
                    ]),
                    (Some(x), false) => block(vec![
                        "let mut total: i64 = 0;".into(),
                        format!("for {item} in {} {{", pad(&self.val(source), 1)),
                        format!("    total += {};", self.elem_val(x)),
                        "}".into(),
                        "total".into(),
                    ]),
                }
            }
            CollOp::Extreme { max, source, expr } => match expr {
                None => atom(format!("{RT}::extreme({}, {max})?", self.val(source))),
                Some(x) => block(vec![
                    "let mut values = Vec::new();".into(),
                    format!("for {item} in {} {{", pad(&self.val(source), 1)),
                    format!("    values.push({});", self.elem_val(x)),
                    "}".into(),
                    format!("{RT}::extreme(values, {max})?"),
                ]),
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
                Some(x) => block(vec![
                    "let mut values = Vec::new();".into(),
                    format!("for {item} in {} {{", pad(&self.val(source), 1)),
                    format!("    values.push({});", self.elem_val(x)),
                    "}".into(),
                    format!("{RT}::unique(values)"),
                ]),
            },
            CollOp::Join { source, sep } => {
                let src = paren(self.ex(source), P_ATOM);
                atom(format!("{src}.join({})", self.txt(sep)))
            }
            CollOp::At { map, key } => {
                let m = paren(self.ex(map), P_ATOM);
                atom(format!("{m}.get(&{}).cloned()", paren(self.ex_val_key(key), P_UNARY)))
            }
            CollOp::MapPart { values, map } => {
                let m = paren(self.ex(map), P_ATOM);
                let word = if *values { "values" } else { "keys" };
                atom(format!("{m}.{word}().cloned().collect::<Vec<_>>()"))
            }
        }
    }

    /// A map key as an expression (a text literal stays an owned `String`).
    fn ex_val_key(&self, key: &Expr) -> E {
        self.ex(key)
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
                let pos = format!("{}_pos", self.n.local(&index.name));
                let mutq = |this: &Self, v: &LocalVar| if this.assigned.contains(&v.name) { "mut " } else { "" };
                self.line(&format!("for ({pos}, {}{}) in {src}.into_iter().enumerate() {{", mutq(self, var), self.n.local(&var.name)));
                self.indent += 1;
                self.line(&format!("let {}{} = {pos} as i64 + 1;", mutq(self, index), self.n.local(&index.name)));
                self.indent -= 1;
                self.nested(body);
                self.line("}");
            }
            CollStmt::ForEachEntry {
                key,
                value,
                source,
                body,
            } => {
                let src = self.val(source);
                let mutq = |this: &Self, v: &LocalVar| if this.assigned.contains(&v.name) { "mut " } else { "" };
                self.line(&format!(
                    "for ({}{}, {}{}) in {src} {{",
                    mutq(self, key),
                    self.n.local(&key.name),
                    mutq(self, value),
                    self.n.local(&value.name)
                ));
                self.nested(body);
                self.line("}");
            }
        }
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
