//! Canonical text of the collection words (SPEC 22.2).

use super::{at_least, expr_text, type_text, Printer};
use crate::syntax::ast::*;
use crate::syntax::collections::{CollExpr, CollStmt};

/// Binding strength of a collection expression, in the scale of `fmt::prec`. Forms that end in a
/// free-running expression (`where cond`, `to expr`, `by key`, a literal's last item) swallow the
/// rest of the line, so they are weaker than everything and parenthesised whenever something
/// follows; the others read like a call.
pub(super) fn prec(c: &CollExpr) -> u8 {
    match c {
        CollExpr::ListLit(_)
        | CollExpr::MapLit(_)
        | CollExpr::Where { .. }
        | CollExpr::MapTo { .. }
        | CollExpr::GroupBy { .. }
        | CollExpr::CountWhere { .. }
        | CollExpr::Sort { .. } => 0,
        _ => 8,
    }
}

/// A source reads without parentheses when it is a name, a call, or `keys of` / `values of`.
fn source(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Coll(c) if matches!(**c, CollExpr::MapPart { .. }) => expr_text(e),
        _ => at_least(e, 9),
    }
}

/// Items of a literal: every one but the last is parenthesised when it would take the comma after it.
fn comma_list<'a>(items: impl ExactSizeIterator<Item = &'a Expr>) -> String {
    let last = items.len().wrapping_sub(1);
    items
        .enumerate()
        .map(|(i, x)| if i == last { expr_text(x) } else { at_least(x, 1) })
        .collect::<Vec<_>>()
        .join(", ")
}

fn elem_in(word: &str, x: &Option<Box<Expr>>, s: &Expr) -> String {
    match x {
        Some(x) => format!("{word}{} in {}", at_least(x, 1), source(s)),
        None => format!("{word}{}", source(s)),
    }
}

pub(super) fn expr(c: &CollExpr) -> String {
    match c {
        CollExpr::ListLit(items) => format!("list of {}", comma_list(items.iter())),
        CollExpr::EmptyList(t) => format!("empty list of {}", type_text(t)),
        CollExpr::MapLit(entries) => {
            let last = entries.len().wrapping_sub(1);
            let parts: Vec<String> = entries
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    let value = if i == last { expr_text(&e.value) } else { at_least(&e.value, 1) };
                    format!("{} to {value}", at_least(&e.key, 1))
                })
                .collect();
            format!("map of {}", parts.join(", "))
        }
        CollExpr::EmptyMap { key, value } => {
            format!("empty map of {} to {}", type_text(key), type_text(value))
        }
        CollExpr::Where { source: s, cond } => format!("{} where {}", source(s), expr_text(cond)),
        CollExpr::MapTo { source: s, expr } => format!("map {} to {}", source(s), expr_text(expr)),
        CollExpr::Sort {
            source: s,
            key,
            descending,
        } => format!(
            "sort {} by {}{}",
            source(s),
            if *descending { at_least(key, 1) } else { expr_text(key) },
            if *descending { " descending" } else { "" }
        ),
        CollExpr::GroupBy { source: s, key } => format!("group {} by {}", source(s), expr_text(key)),
        CollExpr::Sum { expr, source: s } => elem_in("sum of ", expr, s),
        CollExpr::CountWhere { source: s, cond } => {
            format!("count of {} where {}", source(s), expr_text(cond))
        }
        CollExpr::Extreme {
            max,
            expr,
            source: s,
        } => elem_in(if *max { "max of " } else { "min of " }, expr, s),
        CollExpr::Edge { last, source: s } => {
            format!("{} of {}", if *last { "last" } else { "first" }, source(s))
        }
        CollExpr::Slice {
            skip,
            count,
            source: s,
        } => format!(
            "{} {} from {}",
            if *skip { "skip" } else { "take" },
            at_least(count, 9),
            source(s)
        ),
        CollExpr::Unique { expr, source: s } => elem_in("unique ", expr, s),
        CollExpr::Join { source: s, sep } => format!("join {} with {}", source(s), at_least(sep, 9)),
        CollExpr::At { map, key } => format!("{} at {}", source(map), at_least(key, 9)),
        CollExpr::MapPart { values, map } => format!(
            "{} of {}",
            if *values { "values" } else { "keys" },
            source(map)
        ),
    }
}

/// One-line statements (`put`, `remove`); `None` for the loops, which have a body.
pub(super) fn simple(c: &CollStmt) -> Option<String> {
    Some(match c {
        CollStmt::Put { value, key, map } => format!(
            "put {} at {} in {}",
            expr_text(value),
            expr_text(key),
            expr_text(map)
        ),
        CollStmt::Remove { key, map } => {
            format!("remove {} from {}", expr_text(key), expr_text(map))
        }
        _ => return None,
    })
}

impl Printer {
    pub(super) fn coll_stmt(&mut self, c: &CollStmt, line: u32, indent: usize) {
        match c {
            CollStmt::ForEachIndex {
                var,
                index,
                source,
                body,
            } => {
                self.push(
                    line,
                    indent,
                    format!(
                        "- for each {} in {} with index {}:",
                        var.name,
                        expr_text(source),
                        index.name
                    ),
                );
                self.stmts(body, indent + 1);
            }
            CollStmt::ForEachEntry {
                key,
                value,
                source,
                body,
            } => {
                self.push(
                    line,
                    indent,
                    format!(
                        "- for each {}, {} in {}:",
                        key.name,
                        value.name,
                        expr_text(source)
                    ),
                );
                self.stmts(body, indent + 1);
            }
            _ => {
                let t = simple(c).unwrap_or_default();
                self.push(line, indent, format!("- {t}"));
            }
        }
    }
}
