//! Expression typing (SPEC 5, 6): names, fields, unit-aware arithmetic, `maybe` handling,
//! calls, and the purity of rule/guard/constant expressions.

use crate::diag::{Diagnostic, Span};
use crate::syntax::ast::{BinOp, Expr, ExprKind, Ident, InterpPart};

use super::body::{Cx, PureKind};
use super::effects::{effect_name, EffectSet};
use super::scope::Kind;
use super::types::Type;
use super::{CallKind, Checker, NodeKey, Resolution};

pub const E_UNKNOWN_NAME: &str = "E202";
pub const E_TYPE: &str = "E203";
pub const E_MAYBE: &str = "E204";
pub const E_IMPURE: &str = "E205";
pub const E_CALL: &str = "E209";
const E_MANIFEST_TOOL: &str = "E207";

/// `invoice.amount` -> `"invoice.amount"`; `None` if the expression is not a plain path.
pub fn path_key(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Path(segs) => Some(
            segs.iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join("."),
        ),
        _ => None,
    }
}

/// Paths known to be non-empty when `e` is true.
pub fn facts(e: &Expr) -> Vec<String> {
    match &e.kind {
        ExprKind::IsNotEmpty(x) => path_key(x).into_iter().collect(),
        ExprKind::Binary {
            op: BinOp::And,
            lhs,
            rhs,
        } => {
            let mut v = facts(lhs);
            v.extend(facts(rhs));
            v
        }
        _ => Vec::new(),
    }
}

/// Paths known to be non-empty when `e` is false.
pub fn neg_facts(e: &Expr) -> Vec<String> {
    match &e.kind {
        ExprKind::IsEmpty(x) => path_key(x).into_iter().collect(),
        ExprKind::Not(x) => facts(x),
        ExprKind::Binary {
            op: BinOp::Or,
            lhs,
            rhs,
        } => {
            let mut v = neg_facts(lhs);
            v.extend(neg_facts(rhs));
            v
        }
        _ => Vec::new(),
    }
}

fn pure_texts(k: PureKind) -> (&'static str, &'static str) {
    match k {
        PureKind::Rule => (
            "A rule must be pure",
            "Rules are pure checks: they may only read the record's own fields and call pure functions.",
        ),
        PureKind::Guard => (
            "A guard must be pure",
            "Guards are pure checks: they may only compare values you already have and call pure functions.",
        ),
        PureKind::Constant => (
            "A constant must be pure",
            "Constants are fixed values: they may only use literals and other constants.",
        ),
    }
}

fn currency_of(e: &Expr) -> Option<&str> {
    match &e.kind {
        ExprKind::Money { currency, .. } => Some(currency),
        _ => None,
    }
}

impl Checker<'_> {
    /// Type of `e`, recording it in `expr_types`.
    /// Write a path the way narrowing remembers it: a bare field `score` is `lead.score` when
    /// `lead` is the record in front of the block.
    pub(super) fn norm_key(&self, cx: &Cx, key: &str) -> String {
        let first = key.split('.').next().unwrap_or(key);
        if cx.var(first).is_none() {
            if let Some(b) = cx.bare() {
                let has = self
                    .records
                    .get(&b.record)
                    .is_some_and(|r| r.fields.iter().any(|f| f.name == first));
                if has {
                    return format!("{}.{key}", b.var);
                }
            }
        }
        key.to_string()
    }

    pub(super) fn norm_facts(&self, cx: &Cx, keys: Vec<String>) -> Vec<String> {
        keys.iter().map(|k| self.norm_key(cx, k)).collect()
    }

    pub fn expr(&mut self, cx: &mut Cx, e: &Expr) -> Type {
        let outer = std::mem::take(&mut cx.acc);
        let ty = self.expr_inner(cx, e);
        let eff = std::mem::replace(&mut cx.acc, outer);
        cx.acc.merge(&eff);
        let key = NodeKey::new(cx.file, e.span);
        self.expr_types.insert(key, ty.clone());
        self.expr_effects.insert(key, eff);
        ty
    }

    /// Add the effect of something the code touches. In a pure context (rule, guard, constant)
    /// anything but `pure` is an error at `span`, and is not added (so it is reported once).
    pub fn touch(&mut self, cx: &mut Cx, span: Span, label: &str, eff: &EffectSet) {
        if eff.is_pure() {
            return;
        }
        if let Some(kind) = cx.pure_ctx {
            let (head, help) = pure_texts(kind);
            let d = Diagnostic::error(
                E_IMPURE,
                self.path(cx.file),
                span,
                format!(
                    "{head}, but \"{label}\" has the effect {}.",
                    effect_name(eff.class())
                ),
            )
            .with_help(help);
            self.diags.push(d);
            return;
        }
        cx.acc.merge(eff);
    }

    fn maybe_error(&mut self, cx: &Cx, span: Span, what: &str, ty: &Type) {
        let d = Diagnostic::error(
            E_MAYBE,
            self.path(cx.file),
            span,
            format!("This value can be empty ({ty}), so it cannot be {what} yet."),
        )
        .with_help(
            "Check it first with \"if <value> is some x:\" or a \"guard: <value> is not empty\".",
        );
        self.diags.push(d);
    }

    fn expr_inner(&mut self, cx: &mut Cx, e: &Expr) -> Type {
        match &e.kind {
            ExprKind::Str(_) => Type::Text,
            ExprKind::Int(_) => Type::Int,
            ExprKind::Money { .. } => Type::Money,
            ExprKind::Duration(_) => Type::Duration,
            ExprKind::Date(_) => Type::Date,
            ExprKind::Bool(_) => Type::Bool,
            ExprKind::Today => Type::Date,
            ExprKind::Now => Type::DateTime,
            ExprKind::Interp(parts) => {
                for part in parts {
                    if let InterpPart::Expr(x) = part {
                        let t = self.expr(cx, x);
                        if t.is_maybe() {
                            self.maybe_error(cx, x.span, "written into text", &t);
                        }
                    }
                }
                Type::Text
            }
            ExprKind::Path(segs) => {
                cx.unwrap_log.clear();
                let t = self.path_expr(cx, segs, NodeKey::new(cx.file, e.span));
                if !cx.unwrap_log.is_empty() {
                    let log = std::mem::take(&mut cx.unwrap_log);
                    self.unwraps.insert(NodeKey::new(cx.file, e.span), log);
                }
                t
            }
            ExprKind::Binary { op, lhs, rhs } => self.binary(cx, e, *op, lhs, rhs),
            ExprKind::Not(x) => {
                let t = self.expr(cx, x);
                self.want_bool(cx, x.span, &t, "not");
                Type::Bool
            }
            ExprKind::Call { func, args } => self.call(cx, e, func, args),
            ExprKind::Record(fields) => {
                let mut out = Vec::new();
                for f in fields {
                    let t = self.expr(cx, &f.value);
                    out.push((f.name.name.clone(), t));
                }
                Type::Object(out)
            }
            ExprKind::CountOf { rule, source, .. } => {
                let st = self.expr(cx, source);
                let elem = self.list_elem(cx, source.span, &st, "count of");
                if let Some(r) = rule {
                    self.check_rule_use(cx, r, &elem);
                }
                Type::Int
            }
            ExprKind::FirstIn { var, source, cond } => {
                let st = self.expr(cx, source);
                let elem = self.list_elem(cx, source.span, &st, "first … in");
                cx.push();
                cx.top().vars.insert(var.name.clone(), elem.clone());
                cx.top().defs.insert(var.name.clone(), var.span);
                if let Type::Record(r) = &elem {
                    cx.top().bare = Some(super::body::Implicit {
                        var: var.name.clone(),
                        record: r.clone(),
                    });
                }
                let ct = self.expr(cx, cond);
                self.want_bool(cx, cond.span, &ct, "where");
                cx.pop();
                Type::Maybe(Box::new(elem))
            }
            ExprKind::IsEmpty(x) | ExprKind::IsNotEmpty(x) => {
                let t = self.expr(cx, x);
                // a second check of a value already known to be present is harmless
                let rechecked = self.unwraps.contains_key(&NodeKey::new(cx.file, x.span));
                if !rechecked
                    && !matches!(
                        t,
                        Type::Maybe(_) | Type::Text | Type::List(_) | Type::Bytes | Type::Unknown
                    )
                {
                    let d = Diagnostic::error(
                        E_TYPE,
                        self.path(cx.file),
                        x.span,
                        format!("This is {}, which can never be empty.", t.with_article()),
                    )
                    .with_help("\"is empty\" and \"is not empty\" work on text, lists and \"maybe\" values.");
                    self.diags.push(d);
                }
                Type::Bool
            }
            ExprKind::Matches { expr, record } => {
                self.expr(cx, expr);
                let te = crate::syntax::ast::TypeExpr {
                    kind: crate::syntax::ast::TypeKind::Named(record.name.clone()),
                    span: record.span,
                };
                let t = self.resolve_type(cx.file, &te);
                if !matches!(t, Type::Record(_) | Type::Unknown) {
                    self.error(
                        E_TYPE,
                        cx.file,
                        record.span,
                        format!(
                            "\"matches\" needs a record, but \"{}\" is {t}.",
                            record.name
                        ),
                    );
                }
                Type::Bool
            }
            ExprKind::Length(x) => {
                let t = self.expr(cx, x);
                if !matches!(t, Type::Text | Type::List(_) | Type::Bytes | Type::Unknown) {
                    self.error(
                        E_TYPE,
                        cx.file,
                        x.span,
                        format!("Only text and lists have a length, but this is {t}."),
                    );
                }
                Type::Int
            }
            ExprKind::NoAmountsChanged(x) => {
                let t = self.expr(cx, x);
                if !matches!(t, Type::Text | Type::Unknown) {
                    self.error(
                        E_TYPE,
                        cx.file,
                        x.span,
                        format!(
                            "\"no amounts changed\" checks text the model wrote, but this is {t}."
                        ),
                    );
                }
                Type::Bool
            }
        }
    }

    pub(super) fn want_bool(&mut self, cx: &Cx, span: Span, t: &Type, what: &str) {
        if !matches!(t, Type::Bool | Type::Unknown) {
            let d = Diagnostic::error(
                E_TYPE,
                self.path(cx.file),
                span,
                format!("\"{what}\" needs a yes/no condition, but this is {t}."),
            );
            self.diags.push(d);
        }
    }

    /// Element type of a list-typed expression; an error for anything else.
    pub fn list_elem(&mut self, cx: &Cx, span: Span, t: &Type, what: &str) -> Type {
        match t {
            Type::List(e) => (**e).clone(),
            Type::Unknown => Type::Unknown,
            other => {
                let d = Diagnostic::error(
                    E_TYPE,
                    self.path(cx.file),
                    span,
                    format!(
                        "\"{what}\" needs a list, but this is {}.",
                        other.with_article()
                    ),
                );
                self.diags.push(d);
                Type::Unknown
            }
        }
    }

    /// A rule used as a filter over elements of type `elem`.
    pub fn check_rule_use(&mut self, cx: &Cx, rule: &Ident, elem: &Type) {
        match self.lookup(cx.file, &rule.name, &[Kind::Rule]) {
            Some((_, name)) => {
                self.resolutions.insert(
                    NodeKey::new(cx.file, rule.span),
                    Resolution::Call {
                        kind: CallKind::Rule,
                        decl: name.clone(),
                    },
                );
                if let (Some(rec), Type::Record(er)) = (self.rule_records.get(&name), elem) {
                    if rec != er {
                        let d = Diagnostic::error(
                            E_TYPE,
                            self.path(cx.file),
                            rule.span,
                            format!("The rule \"{name}\" is about {rec}, but the list holds {er}."),
                        );
                        self.diags.push(d);
                    }
                }
            }
            None => {
                let cands = self.scopes[cx.file].names_of(&[Kind::Rule]);
                let d = Diagnostic::error(
                    E_UNKNOWN_NAME,
                    self.path(cx.file),
                    rule.span,
                    format!("Unknown rule \"{}\".", rule.name),
                )
                .with_did_you_mean(super::scope::suggest(&rule.name, &cands))
                .with_help("A rule must be declared in this file or imported into it.");
                self.diags.push(d);
            }
        }
    }

    // ------------------------------------------------------------------------ paths

    /// Resolution order: variables, bare fields of the record in front of the block, constants,
    /// and only with a dot a tool's source (`crm.invoices`). A single word never names a tool.
    pub(super) fn path_expr(&mut self, cx: &mut Cx, segs: &[Ident], key: NodeKey) -> Type {
        let first = &segs[0];
        // a variable (loop variable, `set`/`->` result, parameter, route parameter, `state`)
        if let Some(t) = cx.var(&first.name).cloned() {
            self.resolutions.insert(
                key,
                Resolution::Var {
                    name: first.name.clone(),
                    def: cx.def_of(&first.name),
                },
            );
            return self.chain(cx, t, segs, 1, first.name.clone());
        }
        // a bare field of the record in front of us
        if let Some(imp) = cx.bare().cloned() {
            let fty = self
                .records
                .get(&imp.record)
                .and_then(|r| r.fields.iter().find(|f| f.name == first.name))
                .map(|f| f.ty.clone());
            if let Some(t) = fty {
                self.resolutions.insert(
                    key,
                    Resolution::Field {
                        var: Some(imp.var.clone()),
                        record: imp.record.clone(),
                        field: first.name.clone(),
                    },
                );
                return self.chain(cx, t, segs, 1, format!("{}.{}", imp.var, first.name));
            }
        }
        // a constant, possibly qualified by a module alias
        for n in (1..=segs.len()).rev() {
            let joined = segs[..n]
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(".");
            if let Some((_, name)) = self.lookup(cx.file, &joined, &[Kind::Constant]) {
                let t = self.const_type(&name);
                self.resolutions.insert(key, Resolution::Const { name });
                return self.chain(cx, t, segs, n, joined);
            }
        }
        // a readable source of a tool: `crm.invoices`
        if segs.len() >= 2 {
            if let Some(def) = self.p.manifest.tool(&first.name).cloned() {
                return self.source(cx, segs, &def, key);
            }
        }
        if cx.lenient {
            // a `## Do` block nobody calls: we do not know its record, but a name that is no
            // field of any record can never work
            let known = self
                .records
                .values()
                .any(|r| r.fields.iter().any(|f| f.name == first.name));
            if known {
                return Type::Unknown;
            }
        }
        if segs.len() == 1 && self.p.manifest.tool(&first.name).is_some() {
            let d = Diagnostic::error(
                E_MANIFEST_TOOL,
                self.path(cx.file),
                first.span,
                format!("\"{}\" is a tool, not a value.", first.name),
            )
            .with_help(format!(
                "To read from it, name a source, for example \"{}.{}\".",
                first.name,
                self.p
                    .manifest
                    .tool(&first.name)
                    .and_then(|d| d.sources.first().cloned())
                    .unwrap_or_else(|| "items".into())
            ));
            self.diags.push(d);
            return Type::Unknown;
        }
        if let Some(imp) = cx.bare().cloned() {
            let mut cands: Vec<String> = self
                .records
                .get(&imp.record)
                .map(|r| r.fields.iter().map(|f| f.name.clone()).collect())
                .unwrap_or_default();
            cands.extend(cx.in_agent_names());
            let d = Diagnostic::error(
                E_UNKNOWN_NAME,
                self.path(cx.file),
                first.span,
                format!("Unknown field \"{}\" on {}.", first.name, imp.record),
            )
            .with_did_you_mean(super::scope::suggest(&first.name, &cands));
            self.diags.push(d);
            return Type::Unknown;
        }
        if self.rule_without_record(cx) {
            return Type::Unknown;
        }
        let mut cands = cx.in_agent_names();
        cands.extend(self.scopes[cx.file].names_of(&[Kind::Constant]));
        cands.extend(self.p.manifest.tool_names().map(str::to_string));
        let d = Diagnostic::error(
            E_UNKNOWN_NAME,
            self.path(cx.file),
            first.span,
            format!("Unknown name \"{}\".", first.name),
        )
        .with_did_you_mean(super::scope::suggest(&first.name, &cands))
        .with_help("Names come from \"set\" and \"->\" lines above, loop variables, constants and imported declarations.");
        self.diags.push(d);
        Type::Unknown
    }

    /// A rule whose `for:` record was unknown (already reported): stay quiet about its names.
    fn rule_without_record(&self, cx: &Cx) -> bool {
        cx.lenient && cx.bare().is_none() && cx.agent.is_none() && cx.func.is_none()
    }

    /// Follow `.field` accesses from segment `start` on, with `ty` the type so far and `key` the
    /// path text so far (for `is not empty` knowledge).
    fn chain(
        &mut self,
        cx: &mut Cx,
        mut ty: Type,
        segs: &[Ident],
        start: usize,
        mut key: String,
    ) -> Type {
        for i in start..segs.len() {
            if let Type::Maybe(inner) = &ty {
                if cx.is_narrowed(&key) {
                    cx.unwrap_log.push(i - 1);
                    ty = (**inner).clone();
                } else {
                    let d = Diagnostic::error(
                        E_MAYBE,
                        self.path(cx.file),
                        segs[i - 1].span,
                        format!("\"{key}\" can be empty here, so \"{}\" cannot be read from it yet.", segs[i].name),
                    )
                    .with_help(format!(
                        "Check it first with \"if {key} is some x:\" or a \"guard: {key} is not empty\"."
                    ));
                    self.diags.push(d);
                    return Type::Unknown;
                }
            }
            let seg = &segs[i];
            let found: Option<Type> = match &ty {
                Type::Record(r) => {
                    let fields = self
                        .records
                        .get(r)
                        .map(|rec| rec.fields.clone())
                        .unwrap_or_default();
                    match fields.iter().find(|f| f.name == seg.name) {
                        Some(f) => Some(f.ty.clone()),
                        None => {
                            let d = Diagnostic::error(
                                E_UNKNOWN_NAME,
                                self.path(cx.file),
                                seg.span,
                                format!("Unknown field \"{}\" on {r}.", seg.name),
                            )
                            .suggesting(&seg.name, fields.iter().map(|f| f.name.as_str()));
                            self.diags.push(d);
                            return Type::Unknown;
                        }
                    }
                }
                Type::Object(fields) => match fields.iter().find(|(n, _)| *n == seg.name) {
                    Some((_, t)) => Some(t.clone()),
                    None => {
                        let d = Diagnostic::error(
                            E_UNKNOWN_NAME,
                            self.path(cx.file),
                            seg.span,
                            format!("Unknown field \"{}\" on this record.", seg.name),
                        )
                        .suggesting(&seg.name, fields.iter().map(|(n, _)| n.as_str()));
                        self.diags.push(d);
                        return Type::Unknown;
                    }
                },
                Type::Unknown => return Type::Unknown,
                other => {
                    let d = Diagnostic::error(
                        E_TYPE,
                        self.path(cx.file),
                        seg.span,
                        format!(
                            "\"{key}\" is {}, which has no fields, so \".{}\" cannot be read.",
                            other.with_article(),
                            seg.name
                        ),
                    );
                    self.diags.push(d);
                    return Type::Unknown;
                }
            };
            ty = found.unwrap_or(Type::Unknown);
            key = format!("{key}.{}", seg.name);
        }
        if let Type::Maybe(inner) = &ty {
            if cx.is_narrowed(&key) {
                cx.unwrap_log.push(segs.len() - 1);
                return (**inner).clone();
            }
        }
        ty
    }

    // ------------------------------------------------------------------------ operators

    fn binary(&mut self, cx: &mut Cx, e: &Expr, op: BinOp, l: &Expr, r: &Expr) -> Type {
        if matches!(op, BinOp::And | BinOp::Or) {
            let lt = self.expr(cx, l);
            let word = if op == BinOp::And { "and" } else { "or" };
            self.want_bool(cx, l.span, &lt, word);
            cx.push();
            if op == BinOp::And {
                let f = self.norm_facts(cx, facts(l));
                cx.narrow(&f);
            } else {
                let f = self.norm_facts(cx, neg_facts(l));
                cx.narrow(&f);
            }
            let rt = self.expr(cx, r);
            cx.pop();
            self.want_bool(cx, r.span, &rt, word);
            return Type::Bool;
        }
        let lt = self.expr(cx, l);
        let rt = self.expr(cx, r);
        let comparing = matches!(op, BinOp::Eq | BinOp::Ne);
        if !comparing {
            for (t, x) in [(&lt, l), (&rt, r)] {
                if t.is_maybe() {
                    let what = if matches!(op, BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge) {
                        "compared"
                    } else {
                        "used in arithmetic"
                    };
                    self.maybe_error(cx, x.span, what, t);
                    return Type::Unknown;
                }
            }
        }
        if lt.is_unknown() || rt.is_unknown() {
            return match op {
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => Type::Bool,
                _ => Type::Unknown,
            };
        }
        let result = match op {
            BinOp::Add => match (&lt, &rt) {
                (Type::Int, Type::Int) => Some(Type::Int),
                (Type::Money, Type::Money) => Some(Type::Money),
                (Type::Date, Type::Duration) | (Type::Duration, Type::Date) => Some(Type::Date),
                (Type::DateTime, Type::Duration) | (Type::Duration, Type::DateTime) => {
                    Some(Type::DateTime)
                }
                (Type::Duration, Type::Duration) => Some(Type::Duration),
                _ => None,
            },
            BinOp::Sub => match (&lt, &rt) {
                (Type::Int, Type::Int) => Some(Type::Int),
                (Type::Money, Type::Money) => Some(Type::Money),
                (Type::Date, Type::Duration) => Some(Type::Date),
                (Type::DateTime, Type::Duration) => Some(Type::DateTime),
                (Type::Date, Type::Date) | (Type::DateTime, Type::DateTime) => Some(Type::Duration),
                (Type::Duration, Type::Duration) => Some(Type::Duration),
                _ => None,
            },
            BinOp::Mul => match (&lt, &rt) {
                (Type::Int, Type::Int) => Some(Type::Int),
                (Type::Money, Type::Int) | (Type::Int, Type::Money) => Some(Type::Money),
                (Type::Duration, Type::Int) | (Type::Int, Type::Duration) => Some(Type::Duration),
                _ => None,
            },
            BinOp::Div => match (&lt, &rt) {
                (Type::Int, Type::Int) => Some(Type::Int),
                (Type::Money, Type::Int) => Some(Type::Money),
                (Type::Duration, Type::Int) => Some(Type::Duration),
                _ => None,
            },
            BinOp::Eq | BinOp::Ne => {
                let (a, b) = (lt.unwrap_maybe(), rt.unwrap_maybe());
                (a == b || self.is_assignable(a, b) && self.is_assignable(b, a))
                    .then_some(Type::Bool)
            }
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => match (&lt, &rt) {
                (Type::Int, Type::Int)
                | (Type::Money, Type::Money)
                | (Type::Date, Type::Date)
                | (Type::DateTime, Type::DateTime)
                | (Type::Duration, Type::Duration) => Some(Type::Bool),
                _ => None,
            },
            BinOp::And | BinOp::Or => unreachable!("handled above"),
        };
        let Some(result) = result else {
            self.unit_error(cx, e.span, op, &lt, &rt);
            return match op {
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => Type::Bool,
                _ => Type::Unknown,
            };
        };
        // money in two different currencies, when both sides are written out
        if lt == Type::Money && rt == Type::Money {
            if let (Some(a), Some(b)) = (currency_of(l), currency_of(r)) {
                if a != b {
                    let verb = match op {
                        BinOp::Add => "add",
                        BinOp::Sub => "subtract",
                        _ => "compare",
                    };
                    let d = Diagnostic::error(
                        E_TYPE,
                        self.path(cx.file),
                        e.span,
                        format!("Cannot {verb} {b} money and {a} money."),
                    )
                    .with_help("Money in different currencies cannot be combined without an explicit conversion.");
                    self.diags.push(d);
                }
            }
        }
        result
    }

    fn unit_error(&mut self, cx: &Cx, span: Span, op: BinOp, a: &Type, b: &Type) {
        let (aa, bb) = (a.with_article(), b.with_article());
        let msg = match op {
            BinOp::Add => format!("Cannot add {bb} to {aa}."),
            BinOp::Sub => format!("Cannot subtract {bb} from {aa}."),
            BinOp::Mul => format!("Cannot multiply {aa} by {bb}."),
            BinOp::Div => format!("Cannot divide {aa} by {bb}."),
            _ => format!("Cannot compare {aa} with {bb}."),
        };
        let money = *a == Type::Money || *b == Type::Money;
        let help = match op {
            BinOp::Add | BinOp::Sub if money => "Money only combines with money; durations combine with dates and other durations.",
            BinOp::Mul | BinOp::Div if money => "Money can only be multiplied or divided by a whole number.",
            BinOp::Add | BinOp::Sub => "Numbers combine with numbers, dates with durations, durations with durations.",
            BinOp::Mul | BinOp::Div => "Only money, durations and numbers can be multiplied or divided, by a whole number.",
            _ => "Both sides of a comparison must be the same kind of value (a text can only be compared with \"=\" or \"!=\").",
        };
        let d = Diagnostic::error(E_TYPE, self.path(cx.file), span, msg).with_help(help);
        self.diags.push(d);
    }

    // ------------------------------------------------------------------------ calls

    fn call(&mut self, cx: &mut Cx, e: &Expr, func: &[Ident], args: &[Expr]) -> Type {
        let name = func
            .iter()
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>()
            .join(".");
        let arg_types: Vec<Type> = args.iter().map(|a| self.expr(cx, a)).collect();
        let Some((kind, decl)) = self.lookup(cx.file, &name, &[Kind::Function, Kind::RustBlock])
        else {
            let mut cands = self.scopes[cx.file].names_of(&[Kind::Function, Kind::RustBlock]);
            cands.sort();
            let span = func[0].span.to(func[func.len() - 1].span);
            let d = Diagnostic::error(
                E_CALL,
                self.path(cx.file),
                span,
                format!("Unknown function \"{name}\"."),
            )
            .with_did_you_mean(super::scope::suggest(&name, &cands))
            .with_help("A function must be declared in this file or imported into it.");
            self.diags.push(d);
            return Type::Unknown;
        };
        self.resolutions.insert(
            NodeKey::new(cx.file, e.span),
            Resolution::Call {
                kind: if kind == Kind::RustBlock {
                    CallKind::RustBlock
                } else {
                    CallKind::Function
                },
                decl: decl.clone(),
            },
        );
        let sig = self.sigs.get(&(kind, decl.clone())).cloned();
        let ty = match &sig {
            Some(sig) => {
                self.check_args(cx, e.span, &name, sig, args, &arg_types);
                match &sig.out {
                    Some(t) => t.clone(),
                    None => {
                        let d = Diagnostic::error(
                            E_CALL,
                            self.path(cx.file),
                            e.span,
                            format!("The function \"{name}\" returns nothing, so it cannot be used as a value."),
                        )
                        .with_help(format!("Call it as a statement instead: \"- do {name}(...)\"."));
                        self.diags.push(d);
                        Type::Unknown
                    }
                }
            }
            None => Type::Unknown,
        };
        let eff = self.callee_effect(kind, &decl);
        self.touch(cx, e.span, &name, &eff);
        ty
    }

    /// Argument count and types against a signature.
    pub fn check_args(
        &mut self,
        cx: &Cx,
        span: Span,
        name: &str,
        sig: &super::Sig,
        args: &[Expr],
        types: &[Type],
    ) {
        let required = sig.params.iter().filter(|p| !p.has_default).count();
        if args.len() < required || args.len() > sig.params.len() {
            let names: Vec<&str> = sig.params.iter().map(|p| p.name.as_str()).collect();
            let d = Diagnostic::error(
                E_CALL,
                self.path(cx.file),
                span,
                format!(
                    "\"{name}\" takes {} value{} ({}), but {} {} given.",
                    sig.params.len(),
                    if sig.params.len() == 1 { "" } else { "s" },
                    names.join(", "),
                    args.len(),
                    if args.len() == 1 { "was" } else { "were" },
                ),
            );
            self.diags.push(d);
            return;
        }
        for ((p, a), t) in sig.params.iter().zip(args).zip(types) {
            if !self.is_assignable(t, &p.ty) {
                let d = Diagnostic::error(
                    E_TYPE,
                    self.path(cx.file),
                    a.span,
                    format!(
                        "\"{}\" of \"{name}\" must be {}, but this is {}.{}",
                        p.name,
                        p.ty,
                        t.with_article(),
                        self.note(t, &p.ty)
                    ),
                );
                self.diags.push(d);
            }
        }
    }
}
