//! AST of the collection words (SPEC 22.2). `ExprKind::Coll` and `StmtKind::Coll` carry these, so
//! the shared AST gains two variants and everything else lives here.
//!
//! Some sub-expressions are evaluated once per element with the element in scope (`item`, and
//! the bare fields of a record element): the `cond` of `where`, the `expr` of `map`, the `key`
//! of `sort` and `group`, the `expr` of `sum`, `min`, `max` and `unique`. [`CollExpr::parts`]
//! says which is which, so every pass can walk the tree without knowing the individual words.

use serde::Serialize;

use super::ast::{Expr, Ident, Stmt, TypeExpr};

/// One `key to value` pair of a map literal.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MapEntry {
    pub key: Expr,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum CollExpr {
    /// `list of 1, 2, 3`
    ListLit(Vec<Expr>),
    /// `empty list of text`
    EmptyList(TypeExpr),
    /// `map of "EU" to 1, "US" to 2`
    MapLit(Vec<MapEntry>),
    /// `empty map of text to int`
    EmptyMap { key: TypeExpr, value: TypeExpr },
    /// `rows where region = "EU"`
    Where { source: Box<Expr>, cond: Box<Expr> },
    /// `map rows to amount`
    MapTo { source: Box<Expr>, expr: Box<Expr> },
    /// `sort rows by amount [descending]`
    Sort {
        source: Box<Expr>,
        key: Box<Expr>,
        descending: bool,
    },
    /// `group rows by region`
    GroupBy { source: Box<Expr>, key: Box<Expr> },
    /// `sum of amount in rows` (`expr` given) or `sum of numbers`
    Sum {
        expr: Option<Box<Expr>>,
        source: Box<Expr>,
    },
    /// `count of rows where …`
    CountWhere { source: Box<Expr>, cond: Box<Expr> },
    /// `min of amount in rows` / `max of …`
    Extreme {
        max: bool,
        expr: Option<Box<Expr>>,
        source: Box<Expr>,
    },
    /// `first of rows` / `last of rows`
    Edge { last: bool, source: Box<Expr> },
    /// `take 10 from rows` / `skip 2 from rows`
    Slice {
        skip: bool,
        count: Box<Expr>,
        source: Box<Expr>,
    },
    /// `unique rows` / `unique region in rows`
    Unique {
        expr: Option<Box<Expr>>,
        source: Box<Expr>,
    },
    /// `join names with ", "`
    Join { source: Box<Expr>, sep: Box<Expr> },
    /// `totals at "EU"`
    At { map: Box<Expr>, key: Box<Expr> },
    /// `keys of totals` / `values of totals`
    MapPart { values: bool, map: Box<Expr> },
}

/// Where a child expression is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Like any other expression.
    Outer,
    /// Once per element of the collection, with the element in scope.
    PerElement,
}

impl CollExpr {
    /// Every child expression, in source order, with the scope it is evaluated in.
    pub fn parts(&self) -> Vec<(&Expr, Scope)> {
        use Scope::*;
        match self {
            CollExpr::ListLit(items) => items.iter().map(|e| (e, Outer)).collect(),
            CollExpr::EmptyList(_) | CollExpr::EmptyMap { .. } => vec![],
            CollExpr::MapLit(entries) => entries
                .iter()
                .flat_map(|e| [(&e.key, Outer), (&e.value, Outer)])
                .collect(),
            CollExpr::Where { source, cond } | CollExpr::CountWhere { source, cond } => {
                vec![(source, Outer), (cond, PerElement)]
            }
            CollExpr::MapTo { source, expr } => vec![(source, Outer), (expr, PerElement)],
            CollExpr::Sort { source, key, .. } | CollExpr::GroupBy { source, key } => {
                vec![(source, Outer), (key, PerElement)]
            }
            CollExpr::Sum { expr, source }
            | CollExpr::Extreme { expr, source, .. }
            | CollExpr::Unique { expr, source } => {
                let mut v = vec![(&**source, Outer)];
                if let Some(e) = expr {
                    v.push((e, PerElement));
                }
                v
            }
            CollExpr::Edge { source, .. } => vec![(source, Outer)],
            CollExpr::Slice { count, source, .. } => vec![(source, Outer), (count, Outer)],
            CollExpr::Join { source, sep } => vec![(source, Outer), (sep, Outer)],
            CollExpr::At { map, key } => vec![(map, Outer), (key, Outer)],
            CollExpr::MapPart { map, .. } => vec![(map, Outer)],
        }
    }

    /// The same children, mutable (for the passes that rewrite expression trees).
    pub fn parts_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            CollExpr::ListLit(items) => items.iter_mut().collect(),
            CollExpr::EmptyList(_) | CollExpr::EmptyMap { .. } => vec![],
            CollExpr::MapLit(entries) => entries
                .iter_mut()
                .flat_map(|e| [&mut e.key, &mut e.value])
                .collect(),
            CollExpr::Where { source, cond } | CollExpr::CountWhere { source, cond } => {
                vec![&mut **source, &mut **cond]
            }
            CollExpr::MapTo { source, expr } => vec![&mut **source, &mut **expr],
            CollExpr::Sort { source, key, .. } | CollExpr::GroupBy { source, key } => {
                vec![&mut **source, &mut **key]
            }
            CollExpr::Sum { expr, source }
            | CollExpr::Extreme { expr, source, .. }
            | CollExpr::Unique { expr, source } => {
                let mut v = vec![&mut **source];
                if let Some(e) = expr {
                    v.push(&mut **e);
                }
                v
            }
            CollExpr::Edge { source, .. } => vec![&mut **source],
            CollExpr::Slice { count, source, .. } => vec![&mut **source, &mut **count],
            CollExpr::Join { source, sep } => vec![&mut **source, &mut **sep],
            CollExpr::At { map, key } => vec![&mut **map, &mut **key],
            CollExpr::MapPart { map, .. } => vec![&mut **map],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum CollStmt {
    /// `put v at k in m`
    Put { value: Expr, key: Expr, map: Expr },
    /// `remove k from m`
    Remove { key: Expr, map: Expr },
    /// `for each row in rows with index i:`
    ForEachIndex {
        var: Ident,
        index: Ident,
        source: Expr,
        body: Vec<Stmt>,
    },
    /// `for each region, total in totals:`
    ForEachEntry {
        key: Ident,
        value: Ident,
        source: Expr,
        body: Vec<Stmt>,
    },
}

impl CollStmt {
    /// The expressions the statement evaluates itself (not those of its body).
    pub fn exprs(&self) -> Vec<&Expr> {
        match self {
            CollStmt::Put { value, key, map } => vec![value, key, map],
            CollStmt::Remove { key, map } => vec![key, map],
            CollStmt::ForEachIndex { source, .. } | CollStmt::ForEachEntry { source, .. } => {
                vec![source]
            }
        }
    }

    pub fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            CollStmt::Put { value, key, map } => vec![value, key, map],
            CollStmt::Remove { key, map } => vec![key, map],
            CollStmt::ForEachIndex { source, .. } | CollStmt::ForEachEntry { source, .. } => {
                vec![source]
            }
        }
    }

    /// The nested statements, if it has a body.
    pub fn body(&self) -> Option<&Vec<Stmt>> {
        match self {
            CollStmt::ForEachIndex { body, .. } | CollStmt::ForEachEntry { body, .. } => Some(body),
            _ => None,
        }
    }

    pub fn body_mut(&mut self) -> Option<&mut Vec<Stmt>> {
        match self {
            CollStmt::ForEachIndex { body, .. } | CollStmt::ForEachEntry { body, .. } => Some(body),
            _ => None,
        }
    }
}
