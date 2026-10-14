//! Collection words (SPEC 22.2): typing, element scopes and purity.
//!
//! Inside the expression of `where`, `map … to`, `sort … by`, `group … by`, `sum of`, `min of`,
//! `max of` and `unique`, the element is the variable `item` and, when it is a record, its fields
//! are in scope by their bare names. Those expressions must be pure (OPEN_QUESTIONS Q324).

use crate::diag::{Diagnostic, Span};
use crate::syntax::ast::{Effect, Expr, ExprKind, Stmt};
use crate::syntax::collections::{CollExpr, CollStmt};

use super::body::{Cx, Implicit, PureKind};
use super::expr::E_TYPE;
use super::types::Type;
use super::Checker;

pub const E_LIST_ITEMS: &str = "E320";
pub const E_MAP_KEY: &str = "E321";
pub const E_ORDER: &str = "E322";
pub const E_SUM: &str = "E323";
pub const E_NOT_COLLECTION: &str = "E324";
pub const E_JOIN: &str = "E325";
pub const E_ENTRY: &str = "E326";

/// What a list or map key may be: values with a total order that print as a JSON object key.
fn key_type(t: &Type) -> bool {
    matches!(
        t,
        Type::Text | Type::Int | Type::Date | Type::DateTime | Type::Bool | Type::Unknown
    )
}

/// What can be put in order (`sort`, `min`, `max`).
fn ordered(t: &Type) -> bool {
    matches!(
        t,
        Type::Text
            | Type::Int
            | Type::Money
            | Type::Date
            | Type::DateTime
            | Type::Duration
            | Type::Unknown
    )
}

impl Checker<'_> {
    /// `map of K to V`: K must be a type with an order that is also a JSON key.
    pub(super) fn check_map_key(&mut self, file: usize, span: Span, key: &Type) {
        if !key_type(key) {
            let d = Diagnostic::error(
                E_MAP_KEY,
                self.path(file),
                span,
                format!("A map cannot be keyed by {}.", key.with_article()),
            )
            .with_help("Map keys are text, numbers, dates, datetimes or yes/no values.");
            self.diags.push(d);
        }
    }

    // ------------------------------------------------------------------------ helpers

    /// The element type of a list, or an error naming the word.
    fn need_list(&mut self, cx: &Cx, span: Span, t: &Type, word: &str) -> Type {
        match t {
            Type::List(e) => (**e).clone(),
            Type::Unknown => Type::Unknown,
            other => {
                let mut d = Diagnostic::error(
                    E_NOT_COLLECTION,
                    self.path(cx.file),
                    span,
                    format!("\"{word}\" needs a list, but this is {}.", other.with_article()),
                );
                if let Type::Map(..) = other {
                    d = d.with_help("Use \"values of\" or \"keys of\" to get a list out of a map.");
                } else if let Type::Maybe(_) = other {
                    d = d.with_help("Check it first with \"if <value> is some x:\".");
                }
                self.diags.push(d);
                Type::Unknown
            }
        }
    }

    /// The key and value types of a map, or an error naming the word.
    fn need_map(&mut self, cx: &Cx, span: Span, t: &Type, word: &str) -> (Type, Type) {
        match t {
            Type::Map(k, v) => ((**k).clone(), (**v).clone()),
            Type::Unknown => (Type::Unknown, Type::Unknown),
            other => {
                let d = Diagnostic::error(
                    E_NOT_COLLECTION,
                    self.path(cx.file),
                    span,
                    format!("\"{word}\" needs a map, but this is {}.", other.with_article()),
                )
                .with_help("A map is written like: map of \"EU\" to 1, \"US\" to 2 (or \"empty map of text to int\").");
                self.diags.push(d);
                (Type::Unknown, Type::Unknown)
            }
        }
    }

    /// Check `x` once per element: `item` (and the bare fields of a record element) in scope,
    /// no effects allowed.
    fn per_element(&mut self, cx: &mut Cx, elem: &Type, at: Span, x: &Expr) -> Type {
        cx.push();
        cx.top().vars.insert("item".into(), elem.clone());
        cx.top().defs.insert("item".into(), at);
        if let Type::Record(r) = elem {
            cx.top().bare = Some(Implicit {
                var: "item".into(),
                record: r.clone(),
            });
        }
        let saved = cx.pure_ctx;
        if saved.is_none() {
            cx.pure_ctx = Some(PureKind::Element);
        }
        let t = self.expr(cx, x);
        cx.pure_ctx = saved;
        cx.pop();
        if t.is_maybe() {
            self.maybe_error(cx, x.span, "used here", &t);
            return Type::Unknown;
        }
        t
    }

    fn want_order(&mut self, cx: &Cx, span: Span, t: &Type, word: &str) {
        if !ordered(t) {
            let d = Diagnostic::error(
                E_ORDER,
                self.path(cx.file),
                span,
                format!(
                    "\"{word}\" needs values it can put in order, but this is {}.",
                    t.with_article()
                ),
            )
            .with_help("Numbers, money, text, dates and durations have an order; records and lists do not.");
            self.diags.push(d);
        }
    }

    /// An `int` argument (`take 3 from`, `skip n from`).
    fn want_int(&mut self, cx: &Cx, span: Span, t: &Type, word: &str) {
        if !matches!(t, Type::Int | Type::Unknown) {
            let d = Diagnostic::error(
                E_TYPE,
                self.path(cx.file),
                span,
                format!("\"{word}\" needs a whole number, but this is {}.", t.with_article()),
            );
            self.diags.push(d);
        }
    }

    // ------------------------------------------------------------------------ expressions

    pub(super) fn coll_expr(&mut self, cx: &mut Cx, c: &CollExpr) -> Type {
        match c {
            CollExpr::ListLit(items) => {
                let elem = self.same_type(cx, items, "list item");
                elem.list()
            }
            CollExpr::EmptyList(te) => {
                let t = self.resolve_type(cx.file, te);
                t.list()
            }
            CollExpr::MapLit(entries) => {
                let keys: Vec<&Expr> = entries.iter().map(|e| &e.key).collect();
                let values: Vec<&Expr> = entries.iter().map(|e| &e.value).collect();
                let k = self.same_type_refs(cx, &keys, "map key");
                self.check_map_key(cx.file, keys[0].span, &k);
                let v = self.same_type_refs(cx, &values, "map value");
                Type::Map(Box::new(k), Box::new(v))
            }
            CollExpr::EmptyMap { key, value } => {
                let k = self.resolve_type(cx.file, key);
                self.check_map_key(cx.file, key.span, &k);
                let v = self.resolve_type(cx.file, value);
                Type::Map(Box::new(k), Box::new(v))
            }
            CollExpr::Where { source, cond } => {
                let st = self.expr(cx, source);
                let elem = self.need_list(cx, source.span, &st, "where");
                let ct = self.per_element(cx, &elem, source.span, cond);
                self.want_bool(cx, cond.span, &ct, "where");
                st
            }
            CollExpr::CountWhere { source, cond } => {
                let st = self.expr(cx, source);
                let elem = self.need_list(cx, source.span, &st, "count of");
                let ct = self.per_element(cx, &elem, source.span, cond);
                self.want_bool(cx, cond.span, &ct, "where");
                Type::Int
            }
            CollExpr::MapTo { source, expr } => {
                let st = self.expr(cx, source);
                let elem = self.need_list(cx, source.span, &st, "map … to");
                self.per_element(cx, &elem, source.span, expr).list()
            }
            CollExpr::Sort { source, key, .. } => {
                let st = self.expr(cx, source);
                let elem = self.need_list(cx, source.span, &st, "sort");
                let kt = self.per_element(cx, &elem, source.span, key);
                self.want_order(cx, key.span, &kt, "sort … by");
                st
            }
            CollExpr::GroupBy { source, key } => {
                let st = self.expr(cx, source);
                let elem = self.need_list(cx, source.span, &st, "group");
                let kt = self.per_element(cx, &elem, source.span, key);
                if !key_type(&kt) {
                    let d = Diagnostic::error(
                        E_MAP_KEY,
                        self.path(cx.file),
                        key.span,
                        format!("\"group … by\" needs a key that is text, a number, a date or a yes/no value, but this is {}.", kt.with_article()),
                    );
                    self.diags.push(d);
                }
                Type::Map(Box::new(kt), Box::new(elem.list()))
            }
            CollExpr::Sum { expr, source } => {
                let st = self.expr(cx, source);
                let elem = self.need_list(cx, source.span, &st, "sum of");
                let t = match expr {
                    Some(x) => self.per_element(cx, &elem, source.span, x),
                    None => elem,
                };
                if !matches!(t, Type::Int | Type::Money | Type::Unknown) {
                    let span = expr.as_ref().map_or(source.span, |x| x.span);
                    let d = Diagnostic::error(
                        E_SUM,
                        self.path(cx.file),
                        span,
                        format!("\"sum of\" adds numbers or money, but these are {}.", t.with_article()),
                    )
                    .with_help("Sum the numbers or amounts of a field: \"sum of amount in rows\".");
                    self.diags.push(d);
                    return Type::Unknown;
                }
                t
            }
            CollExpr::Extreme { max, expr, source } => {
                let word = if *max { "max of" } else { "min of" };
                let st = self.expr(cx, source);
                let elem = self.need_list(cx, source.span, &st, word);
                let t = match expr {
                    Some(x) => self.per_element(cx, &elem, source.span, x),
                    None => elem,
                };
                let span = expr.as_ref().map_or(source.span, |x| x.span);
                self.want_order(cx, span, &t, word);
                t.maybe()
            }
            CollExpr::Edge { last, source } => {
                let st = self.expr(cx, source);
                let word = if *last { "last of" } else { "first of" };
                self.need_list(cx, source.span, &st, word).maybe()
            }
            CollExpr::Slice {
                skip,
                count,
                source,
            } => {
                let word = if *skip { "skip" } else { "take" };
                let nt = self.expr(cx, count);
                self.want_int(cx, count.span, &nt, word);
                let st = self.expr(cx, source);
                self.need_list(cx, source.span, &st, word);
                st
            }
            CollExpr::Unique { expr, source } => {
                let st = self.expr(cx, source);
                let elem = self.need_list(cx, source.span, &st, "unique");
                match expr {
                    Some(x) => self.per_element(cx, &elem, source.span, x).list(),
                    None => st,
                }
            }
            CollExpr::Join { source, sep } => {
                let st = self.expr(cx, source);
                let elem = self.need_list(cx, source.span, &st, "join");
                if !matches!(elem, Type::Text | Type::Unknown) {
                    let d = Diagnostic::error(
                        E_JOIN,
                        self.path(cx.file),
                        source.span,
                        format!("\"join\" glues a list of text, but this is a list of {elem}."),
                    )
                    .with_help("Turn the values into text first, for example: map rows to \"{item.id}\"");
                    self.diags.push(d);
                }
                let pt = self.expr(cx, sep);
                if !matches!(pt, Type::Text | Type::Unknown) {
                    let d = Diagnostic::error(
                        E_TYPE,
                        self.path(cx.file),
                        sep.span,
                        format!("The text between the parts must be text, but this is {}.", pt.with_article()),
                    );
                    self.diags.push(d);
                }
                Type::Text
            }
            CollExpr::At { map, key } => {
                let mt = self.expr(cx, map);
                let (k, v) = self.need_map(cx, map.span, &mt, "at");
                let kt = self.expr(cx, key);
                if !self.is_assignable(&kt, &k) {
                    let d = Diagnostic::error(
                        E_ENTRY,
                        self.path(cx.file),
                        key.span,
                        format!("The keys of this map are {k}, but this is {}.", kt.with_article()),
                    );
                    self.diags.push(d);
                }
                v.maybe()
            }
            CollExpr::MapPart { values, map } => {
                let word = if *values { "values of" } else { "keys of" };
                let mt = self.expr(cx, map);
                let (k, v) = self.need_map(cx, map.span, &mt, word);
                if *values {
                    v.list()
                } else {
                    k.list()
                }
            }
        }
    }

    /// All `items` must have one type; the first fixes it.
    fn same_type(&mut self, cx: &mut Cx, items: &[Expr], what: &str) -> Type {
        let refs: Vec<&Expr> = items.iter().collect();
        self.same_type_refs(cx, &refs, what)
    }

    fn same_type_refs(&mut self, cx: &mut Cx, items: &[&Expr], what: &str) -> Type {
        let mut first: Option<Type> = None;
        for x in items {
            let t = self.expr(cx, x);
            if t.is_unknown() {
                continue;
            }
            match &first {
                None => first = Some(t),
                Some(f) => {
                    if self.is_assignable(&t, f) {
                        continue;
                    }
                    if self.is_assignable(f, &t) {
                        first = Some(t);
                        continue;
                    }
                    let d = Diagnostic::error(
                        E_LIST_ITEMS,
                        self.path(cx.file),
                        x.span,
                        format!(
                            "Every {what} must be the same kind of value: the first is {}, but this is {}.",
                            f.with_article(),
                            t.with_article()
                        ),
                    );
                    self.diags.push(d);
                }
            }
        }
        first.unwrap_or(Type::Unknown)
    }

    // ------------------------------------------------------------------------ statements

    pub(super) fn coll_stmt(&mut self, cx: &mut Cx, s: &Stmt, c: &CollStmt) {
        match c {
            CollStmt::Put { value, key, map } => {
                let mt = self.expr(cx, map);
                let (k, v) = self.need_map(cx, map.span, &mt, "put … in");
                let kt = self.expr(cx, key);
                if !self.is_assignable(&kt, &k) {
                    let d = Diagnostic::error(
                        E_ENTRY,
                        self.path(cx.file),
                        key.span,
                        format!("The keys of this map are {k}, but this is {}.", kt.with_article()),
                    );
                    self.diags.push(d);
                }
                let vt = self.expr(cx, value);
                if !self.is_assignable(&vt, &v) {
                    let d = Diagnostic::error(
                        E_ENTRY,
                        self.path(cx.file),
                        value.span,
                        format!(
                            "This map holds {v}, but this is {}.{}",
                            vt.with_article(),
                            self.note(&vt, &v)
                        ),
                    );
                    self.diags.push(d);
                }
                self.writes_state(cx, map);
                self.check_limit(cx, s);
            }
            CollStmt::Remove { key, map } => {
                let mt = self.expr(cx, map);
                let (k, _) = self.need_map(cx, map.span, &mt, "remove … from");
                let kt = self.expr(cx, key);
                if !self.is_assignable(&kt, &k) {
                    let d = Diagnostic::error(
                        E_ENTRY,
                        self.path(cx.file),
                        key.span,
                        format!("The keys of this map are {k}, but this is {}.", kt.with_article()),
                    );
                    self.diags.push(d);
                }
                self.writes_state(cx, map);
                self.check_limit(cx, s);
            }
            CollStmt::ForEachIndex {
                var,
                index,
                source,
                body,
            } => {
                let st = self.expr(cx, source);
                let elem = self.need_list(cx, source.span, &st, "for each");
                self.check_limit(cx, s);
                cx.push();
                self.bind(cx, &var.name, var.span, &elem, true);
                self.bind(cx, &index.name, index.span, &Type::Int, false);
                self.stmts(cx, body);
                cx.pop();
            }
            CollStmt::ForEachEntry {
                key,
                value,
                source,
                body,
            } => {
                let st = self.expr(cx, source);
                let (k, v) = self.need_map(cx, source.span, &st, "for each … , … in");
                self.check_limit(cx, s);
                cx.push();
                self.bind(cx, &key.name, key.span, &k, false);
                self.bind(cx, &value.name, value.span, &v, true);
                self.stmts(cx, body);
                cx.pop();
            }
        }
    }

    /// Define a loop variable in the top frame; a record's fields become bare names.
    fn bind(&mut self, cx: &mut Cx, name: &str, span: Span, ty: &Type, bare: bool) {
        cx.top().vars.insert(name.to_string(), ty.clone());
        cx.top().defs.insert(name.to_string(), span);
        if let (true, Type::Record(r)) = (bare, ty) {
            cx.top().bare = Some(Implicit {
                var: name.to_string(),
                record: r.clone(),
            });
        }
    }

    /// Changing `state.…` is the one local effect of the map statements (as for `append`, Q39).
    fn writes_state(&mut self, cx: &mut Cx, map: &Expr) {
        if let ExprKind::Path(segs) = &map.kind {
            if segs[0].name == "state" {
                cx.acc.add(Effect::Local, "state");
            }
        }
    }
}
