//! IR of the collection words (SPEC 22.2). Pure: every operation computes a new value from its
//! operands; the only statements are `put`, `remove` and the two kinds of loop.
//!
//! The per-element expressions (`cond`, `key`, `expr`) are ordinary IR expressions over the
//! variable [`ELEM`]; a record element's bare fields are already `Field { base: Var(ELEM) … }`.

use serde::Serialize;

use super::{Expr, LocalVar, Stmt};

/// The variable that holds the current element inside a per-element expression.
pub const ELEM: &str = "item";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum CollOp {
    /// `list of a, b` / `empty list of T`; the element type is the expression's `List` type.
    List(Vec<Expr>),
    /// `map of k to v, …` / `empty map of K to V`.
    Map(Vec<(Expr, Expr)>),
    Where {
        source: Box<Expr>,
        cond: Box<Expr>,
    },
    CountWhere {
        source: Box<Expr>,
        cond: Box<Expr>,
    },
    MapTo {
        source: Box<Expr>,
        expr: Box<Expr>,
    },
    /// Stable; `descending` keeps equal keys in their original order too.
    Sort {
        source: Box<Expr>,
        key: Box<Expr>,
        descending: bool,
    },
    Group {
        source: Box<Expr>,
        key: Box<Expr>,
    },
    /// `expr` is the per-element number or amount; `None` sums the elements themselves.
    Sum {
        source: Box<Expr>,
        expr: Option<Box<Expr>>,
    },
    Extreme {
        max: bool,
        source: Box<Expr>,
        expr: Option<Box<Expr>>,
    },
    Edge {
        last: bool,
        source: Box<Expr>,
    },
    Slice {
        skip: bool,
        count: Box<Expr>,
        source: Box<Expr>,
    },
    Unique {
        source: Box<Expr>,
        expr: Option<Box<Expr>>,
    },
    Join {
        source: Box<Expr>,
        sep: Box<Expr>,
    },
    At {
        map: Box<Expr>,
        key: Box<Expr>,
    },
    MapPart {
        values: bool,
        map: Box<Expr>,
    },
}

impl CollOp {
    /// Every operand, per-element expressions included.
    pub fn children(&self) -> Vec<&Expr> {
        match self {
            CollOp::List(items) => items.iter().collect(),
            CollOp::Map(entries) => entries.iter().flat_map(|(k, v)| [k, v]).collect(),
            CollOp::Where { source, cond } | CollOp::CountWhere { source, cond } => vec![source, cond],
            CollOp::MapTo { source, expr } => vec![source, expr],
            CollOp::Sort { source, key, .. } | CollOp::Group { source, key } => vec![source, key],
            CollOp::Sum { source, expr }
            | CollOp::Extreme { source, expr, .. }
            | CollOp::Unique { source, expr } => {
                let mut v: Vec<&Expr> = vec![source];
                v.extend(expr.iter().map(|x| &**x));
                v
            }
            CollOp::Edge { source, .. } => vec![source],
            CollOp::Slice { count, source, .. } => vec![count, source],
            CollOp::Join { source, sep } => vec![source, sep],
            CollOp::At { map, key } => vec![map, key],
            CollOp::MapPart { map, .. } => vec![map],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[allow(clippy::large_enum_variant)]
pub enum CollStmt {
    /// `put v at k in m`; `map` is a path to the map.
    Put { value: Expr, key: Expr, map: Expr },
    /// `remove k from m`.
    Remove { key: Expr, map: Expr },
    /// `for each row in rows with index i:`; the index counts from 1.
    ForEachIndex {
        var: LocalVar,
        index: LocalVar,
        source: Expr,
        body: Vec<Stmt>,
    },
    /// `for each k, v in m:` in key order.
    ForEachEntry {
        key: LocalVar,
        value: LocalVar,
        source: Expr,
        body: Vec<Stmt>,
    },
}

impl CollStmt {
    /// The expressions the statement evaluates itself.
    pub fn exprs(&self) -> Vec<&Expr> {
        match self {
            CollStmt::Put { value, key, map } => vec![value, key, map],
            CollStmt::Remove { key, map } => vec![key, map],
            CollStmt::ForEachIndex { source, .. } | CollStmt::ForEachEntry { source, .. } => vec![source],
        }
    }

    pub fn body(&self) -> Option<&Vec<Stmt>> {
        match self {
            CollStmt::ForEachIndex { body, .. } | CollStmt::ForEachEntry { body, .. } => Some(body),
            _ => None,
        }
    }
}
