//! Grammar of the collection words (SPEC 22.2). `expr.rs` calls [`forms`] and [`postfix`] from its
//! atom rule; `stmt.rs` calls [`statements`] and [`loops`]. Nothing else in the grammar knows
//! these words.
//!
//! Reading rules (OPEN_QUESTIONS Q320 to Q323):
//! - `list of …` and `map of … to …` are literals in an expression and types in a type position;
//!   the position decides, so `read file p as list of Sale -> rows` and
//!   `set xs = list of 1, 2, 3` do not clash.
//! - A literal takes every comma after it: write `f((list of 1, 2), 3)` to end one early.
//! - The source of a word is a name, a call or a parenthesised expression: `sort (rows where
//!   amount > 5) by amount`. `rows where cond` and `totals at key` hang off the same kind of
//!   source, and `cond` runs to the end of the expression.

use chumsky::prelude::*;

use super::expr::{at, kw, sp, sp1, wrong_word, Ex, P};
use crate::syntax::ast::*;
use crate::syntax::collections::{CollExpr, CollStmt, MapEntry};

/// ` word ` between two parts of a form.
fn ws<'a>(word: &'static str) -> impl Parser<'a, &'a str, (), Ex<'a>> + Clone {
    sp1().ignore_then(kw(word)).ignore_then(sp1())
}

/// An optional word that follows a complete expression (`where`, `at`, `descending`, `with`).
/// Its "expected" label is the word with a trailing `…`, which `parser::NOISE_WORDS` hides and
/// the did-you-mean search ignores, so a typo further on is still described by the words that
/// really could have stood there (`ready an done` suggests `and`, not `at`).
fn soft<'a>(word: &'static str, label: &'static str) -> impl Parser<'a, &'a str, (), Ex<'a>> + Clone {
    text::ascii::ident().try_map(move |s: &str, span| {
        if s == word {
            Ok(())
        } else {
            Err(wrong_word(label, s, span))
        }
    })
}

/// ` word ` before an optional part.
fn soft_ws<'a>(word: &'static str, label: &'static str) -> impl Parser<'a, &'a str, (), Ex<'a>> + Clone {
    sp1().ignore_then(soft(word, label)).ignore_then(sp1())
}

/// `word `: a form's first word and the space after it.
fn lead<'a>(word: &'static str) -> impl Parser<'a, &'a str, (), Ex<'a>> + Clone {
    kw(word).ignore_then(sp1())
}

/// `word of `
fn lead_of<'a>(word: &'static str) -> impl Parser<'a, &'a str, (), Ex<'a>> + Clone {
    kw(word).ignore_then(ws("of"))
}

fn node<'a>(
    kind: CollExpr,
    e: &mut chumsky::input::MapExtra<'a, '_, &'a str, Ex<'a>>,
) -> Expr {
    Expr {
        span: at(e),
        kind: ExprKind::Coll(Box::new(kind)),
    }
}

fn boxed(e: Expr) -> Box<Expr> {
    Box::new(e)
}

/// `expr in source` (the first part is the per-element expression) or just `source`.
fn elem_in_source<'a>(expr: P<'a, Expr>, source: P<'a, Expr>) -> P<'a, (Option<Box<Expr>>, Box<Expr>)> {
    choice((
        expr.then_ignore(ws("in"))
            .then(source.clone())
            .map(|(e, s)| (Some(boxed(e)), boxed(s))),
        source.map(|s| (None, boxed(s))),
    ))
    .boxed()
}

/// What a word is applied to: a name, a call, a parenthesised expression (`base`), or `keys of`
/// / `values of` another source. Those two end where their own source ends, so they may stand
/// here without parentheses: `sum of values of totals`, `join keys of m with ", "`.
pub fn source<'a>(base: P<'a, Expr>) -> P<'a, Expr> {
    recursive(|src| {
        let part = |word: &'static str, values: bool| {
            lead_of(word)
                .ignore_then(src.clone())
                .map_with(move |m, e| {
                    node(
                        CollExpr::MapPart {
                            values,
                            map: boxed(m),
                        },
                        e,
                    )
                })
        };
        choice((part("keys", false), part("values", true), base.clone()))
    })
    .boxed()
}

/// The words that begin a collection expression.
const WORDS: &[&str] = &[
    "list", "empty", "map", "sort", "group", "sum", "count", "min", "max", "first", "last", "take",
    "skip", "unique", "join", "keys", "values",
];

/// Succeeds, without consuming anything, when the next word starts a collection expression.
fn gate<'a>() -> impl Parser<'a, &'a str, (), Ex<'a>> + Clone {
    text::ascii::ident()
        .try_map(|s: &str, span| {
            if WORDS.contains(&s) {
                Ok(())
            } else {
                Err(wrong_word("a value", s, span))
            }
        })
        .rewind()
}

/// All the words that begin a collection expression.
///
/// - `expr`: the whole expression grammar (element expressions, list items)
/// - `source`: what a word is applied to: a name, a call or a parenthesised expression
/// - `operand`: a count, a separator or a key: a literal, a name, a call or a parenthesised expression
/// - `ty`: the type grammar, for `empty list of T`
pub fn forms<'a>(
    expr: P<'a, Expr>,
    source: P<'a, Expr>,
    operand: P<'a, Expr>,
    ty: P<'a, TypeExpr>,
) -> P<'a, Expr> {
    let comma = just(',').padded_by(sp());

    let list_lit = lead_of("list")
        .ignore_then(
            expr.clone()
                .separated_by(comma)
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .map_with(|items, e| node(CollExpr::ListLit(items), e));
    let empty_list = lead("empty")
        .ignore_then(lead_of("list"))
        .ignore_then(ty.clone())
        .map_with(|t, e| node(CollExpr::EmptyList(t), e));
    let entry = expr
        .clone()
        .then_ignore(ws("to"))
        .then(expr.clone())
        .map(|(key, value)| MapEntry { key, value });
    let map_lit = lead_of("map")
        .ignore_then(entry.separated_by(comma).at_least(1).collect::<Vec<_>>())
        .map_with(|entries, e| node(CollExpr::MapLit(entries), e));
    let empty_map = lead("empty")
        .ignore_then(lead_of("map"))
        .ignore_then(ty.clone())
        .then_ignore(ws("to"))
        .then(ty)
        .map_with(|(key, value), e| node(CollExpr::EmptyMap { key, value }, e));

    let map_to = lead("map")
        .ignore_then(source.clone())
        .then_ignore(ws("to"))
        .then(expr.clone())
        .map_with(|(s, x), e| {
            node(
                CollExpr::MapTo {
                    source: boxed(s),
                    expr: boxed(x),
                },
                e,
            )
        });
    let sort = lead("sort")
        .ignore_then(source.clone())
        .then_ignore(ws("by"))
        .then(expr.clone())
        .then(sp1().ignore_then(soft("descending", "descending…")).or_not())
        .map_with(|((s, k), d), e| {
            node(
                CollExpr::Sort {
                    source: boxed(s),
                    key: boxed(k),
                    descending: d.is_some(),
                },
                e,
            )
        });
    let group = lead("group")
        .ignore_then(source.clone())
        .then_ignore(ws("by"))
        .then(expr.clone())
        .map_with(|(s, k), e| {
            node(
                CollExpr::GroupBy {
                    source: boxed(s),
                    key: boxed(k),
                },
                e,
            )
        });
    let sum = lead_of("sum")
        .ignore_then(elem_in_source(expr.clone(), source.clone()))
        .map_with(|(x, s), e| node(CollExpr::Sum { expr: x, source: s }, e));
    let count_where = lead_of("count")
        .ignore_then(source.clone())
        .then_ignore(ws("where"))
        .then(expr.clone())
        .map_with(|(s, c), e| {
            node(
                CollExpr::CountWhere {
                    source: boxed(s),
                    cond: boxed(c),
                },
                e,
            )
        });
    let (expr_last, operand_last) = (expr.clone(), operand.clone());
    let part = |word: &'static str, values: bool| {
        lead_of(word)
            .ignore_then(source.clone())
            .map_with(move |m, e| {
                node(
                    CollExpr::MapPart {
                        values,
                        map: boxed(m),
                    },
                    e,
                )
            })
    };
    let extreme = |word: &'static str, max: bool| {
        lead_of(word)
            .ignore_then(elem_in_source(expr.clone(), source.clone()))
            .map_with(move |(x, s), e| {
                node(
                    CollExpr::Extreme {
                        max,
                        expr: x,
                        source: s,
                    },
                    e,
                )
            })
    };
    let edge = |word: &'static str, last: bool| {
        lead_of(word)
            .ignore_then(source.clone())
            .map_with(move |s, e| {
                node(
                    CollExpr::Edge {
                        last,
                        source: boxed(s),
                    },
                    e,
                )
            })
    };
    let slice = |word: &'static str, skip: bool| {
        lead(word)
            .ignore_then(operand.clone())
            .then_ignore(ws("from"))
            .then(source.clone())
            .map_with(move |(n, s), e| {
                node(
                    CollExpr::Slice {
                        skip,
                        count: boxed(n),
                        source: boxed(s),
                    },
                    e,
                )
            })
    };
    let unique = lead("unique")
        .ignore_then(elem_in_source(expr_last, source.clone()))
        .map_with(|(x, s), e| node(CollExpr::Unique { expr: x, source: s }, e));
    let join = lead("join")
        .ignore_then(source.clone())
        .then_ignore(ws("with"))
        .then(operand_last)
        .map_with(|(s, sep), e| {
            node(
                CollExpr::Join {
                    source: boxed(s),
                    sep: boxed(sep),
                },
                e,
            )
        });
    // Small boxed groups keep the stack frame of one attempt small, and the gate keeps the whole
    // lot from being entered for the many expressions that do not start with one of its words.
    let literals = choice((list_lit, empty_list, map_lit, empty_map)).boxed();
    let pipeline = choice((map_to, sort, group, sum, count_where)).boxed();
    let picks = choice((
        extreme("min", false),
        extreme("max", true),
        edge("first", false),
        edge("last", true),
    ))
    .boxed();
    let rest = choice((
        slice("take", false),
        slice("skip", true),
        unique,
        join,
        part("keys", false),
        part("values", true),
    ))
    .boxed();
    gate()
        .ignore_then(choice((literals, pipeline, picks, rest)))
    .labelled("a collection word")
    .boxed()
}

/// A name, call or parenthesised expression with the tails that hang off a source:
/// `rows where cond` and `totals at key`.
///
/// `totals at key` is not read when ` in` follows it, so `put total at "EU" in m` keeps its own
/// `at` (Q322).
pub fn postfix<'a>(base: P<'a, Expr>, expr: P<'a, Expr>, operand: P<'a, Expr>) -> P<'a, Expr> {
    #[derive(Clone)]
    enum Tail {
        Where(Expr),
        At(Expr),
    }
    let where_tail = soft_ws("where", "where…").ignore_then(expr).map(Tail::Where);
    let at_tail = soft_ws("at", "at…")
        .ignore_then(operand)
        .then_ignore(sp1().ignore_then(kw("in")).not())
        .map(Tail::At);
    base.then(choice((where_tail, at_tail)).or_not())
        .map_with(|(b, tail), e| match tail {
            None => b,
            Some(Tail::Where(cond)) => node(
                CollExpr::Where {
                    source: boxed(b),
                    cond: boxed(cond),
                },
                e,
            ),
            Some(Tail::At(key)) => node(
                CollExpr::At {
                    map: boxed(b),
                    key: boxed(key),
                },
                e,
            ),
        })
        .boxed()
}

/// `put v at k in m` and `remove k from m`.
pub fn statements<'a>(expr: P<'a, Expr>) -> P<'a, Stmt> {
    let put = lead("put")
        .ignore_then(expr.clone())
        .then_ignore(ws("at"))
        .then(expr.clone())
        .then_ignore(ws("in"))
        .then(expr.clone())
        .map_with(|((value, key), map), e| {
            stmt(CollStmt::Put { value, key, map }, e)
        });
    let remove = lead("remove")
        .ignore_then(expr.clone())
        .then_ignore(ws("from"))
        .then(expr)
        .map_with(|(key, map), e| stmt(CollStmt::Remove { key, map }, e));
    choice((put, remove)).boxed()
}

fn stmt<'a>(c: CollStmt, e: &mut chumsky::input::MapExtra<'a, '_, &'a str, Ex<'a>>) -> Stmt {
    Stmt {
        span: at(e),
        comments: Vec::new(),
        kind: StmtKind::Coll(c),
    }
}

/// The two loop heads that are not `for each x in …`.
#[derive(Debug, Clone)]
pub enum LoopHead {
    Index {
        var: Ident,
        index: Ident,
        source: Expr,
    },
    Entry {
        key: Ident,
        value: Ident,
        source: Expr,
    },
}

/// `for each row in rows with index i:` and `for each k, v in map:` (without the colon).
pub fn loops<'a>(var: P<'a, Ident>, expr: P<'a, Expr>) -> P<'a, LoopHead> {
    let head = kw("for")
        .ignore_then(ws("each"));
    let entry = head
        .clone()
        .ignore_then(var.clone())
        .then_ignore(just(',').padded_by(sp()))
        .then(var.clone())
        .then_ignore(ws("in"))
        .then(expr.clone())
        .map(|((key, value), source)| LoopHead::Entry { key, value, source });
    let index = head
        .ignore_then(var.clone())
        .then_ignore(ws("in"))
        .then(expr)
        .then_ignore(soft_ws("with", "with…"))
        .then_ignore(kw("index"))
        .then_ignore(sp1())
        .then(var)
        .map(|((var, source), index)| LoopHead::Index { var, index, source });
    choice((entry, index)).boxed()
}
