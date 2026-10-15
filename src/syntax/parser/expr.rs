//! Token-level chumsky grammar: names, types, literals and expressions (SPEC 5, 6).
//!
//! Every parser here works on the text of ONE source line. The parse state is a [`LineCtx`] that
//! knows which line and column the text came from, so a node's `Span` is a real position in the
//! file, not an offset into the fragment.

use chumsky::error::{LabelError, RichPattern};
use chumsky::extra;
use chumsky::input::MapExtra;
use chumsky::prelude::*;
use chumsky::util::MaybeRef;

use crate::diag::{did_you_mean, Span};
use crate::syntax::ast::*;

/// Parse state: where in the file the text being parsed came from.
#[derive(Debug, Clone)]
pub struct LineCtx {
    /// 1-based line number.
    pub line: u32,
    /// 1-based character column of the first character of `text`.
    pub col: u32,
    pub text: String,
    /// Mistakes found inside a construct that still parsed (a bad date, an unknown unit, ...).
    /// They carry the plain-language message, which a failed alternative could not keep.
    pub problems: Vec<Problem>,
}

/// A mistake with its own plain-language message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub span: Span,
    pub message: String,
    pub did_you_mean: Option<String>,
}

impl LineCtx {
    pub fn new(line: u32, col: u32, text: &str) -> Self {
        LineCtx {
            line,
            col,
            text: text.to_string(),
            problems: Vec::new(),
        }
    }

    pub fn flag(&mut self, r: SimpleSpan, message: String, did_you_mean: Option<String>) {
        let span = self.span(r);
        if !self
            .problems
            .iter()
            .any(|p| p.span == span && p.message == message)
        {
            self.problems.push(Problem {
                span,
                message,
                did_you_mean,
            });
        }
    }

    /// Convert a byte range inside `text` to a file [`Span`] (columns count characters).
    pub fn span(&self, r: SimpleSpan) -> Span {
        let start = self.text[..r.start].chars().count() as u32;
        let len = self.text[r.start..r.end].chars().count() as u32;
        Span::new(self.line, self.col + start, len)
    }
}

pub type Ex<'a> = extra::Full<Rich<'a, char>, extra::SimpleState<LineCtx>, ()>;
pub type P<'a, T> = Boxed<'a, 'a, &'a str, T, Ex<'a>>;

/// Words that can never be a variable or field name inside an expression.
const RESERVED: &[&str] = &[
    "and",
    "or",
    "not",
    "is",
    "in",
    "as",
    "where",
    "else",
    "matches",
    "to",
    "true",
    "false",
    "today",
    "now",
    "otherwise",
    "some",
    "none",
    "into",
];

/// Record a mistake in what the parser just matched, and let parsing carry on.
pub fn flag<'a>(
    e: &mut MapExtra<'a, '_, &'a str, Ex<'a>>,
    message: impl Into<String>,
    dym: Option<String>,
) {
    let r = e.span();
    e.state().flag(r, message.into(), dym);
}

/// The file position of whatever the parser just matched.
pub fn at<'a>(e: &mut MapExtra<'a, '_, &'a str, Ex<'a>>) -> Span {
    let r = e.span();
    e.state().span(r)
}

fn bin<'a>(
    lhs: Expr,
    (op, rhs): (BinOp, Expr),
    _e: &mut MapExtra<'a, '_, &'a str, Ex<'a>>,
) -> Expr {
    Expr {
        span: lhs.span.to(rhs.span),
        kind: ExprKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
    }
}

// ------------------------------------------------------------------ whitespace and keywords

/// Zero or more spaces.
pub fn sp<'a>() -> impl Parser<'a, &'a str, (), Ex<'a>> + Copy {
    text::inline_whitespace()
}

/// One or more spaces.
pub fn sp1<'a>() -> impl Parser<'a, &'a str, (), Ex<'a>> + Copy {
    text::inline_whitespace().at_least(1)
}

pub(super) fn wrong_word<'a>(word: &'static str, found: &str, span: SimpleSpan) -> Rich<'a, char> {
    <Rich<'a, char> as LabelError<'a, &'a str, RichPattern<'a, char>>>::expected_found(
        [RichPattern::Label(std::borrow::Cow::Borrowed(word))],
        found.chars().next().map(MaybeRef::Val),
        span,
    )
}

/// A case-sensitive keyword. When the word is something else, the error points at the start of
/// that word (not after it) and says which keyword was expected.
pub fn kw<'a>(word: &'static str) -> impl Parser<'a, &'a str, (), Ex<'a>> + Clone {
    text::ascii::ident().try_map(move |s: &str, span| {
        if s == word {
            Ok(())
        } else {
            Err(wrong_word(word, s, span))
        }
    })
}

/// A case-insensitive keyword, for headings (SPEC 1: "Headings are case-insensitive").
pub fn ikw<'a>(word: &'static str) -> impl Parser<'a, &'a str, (), Ex<'a>> + Clone {
    text::ascii::ident().try_map(move |s: &str, span| {
        if s.eq_ignore_ascii_case(word) {
            Ok(())
        } else {
            Err(wrong_word(word, s, span))
        }
    })
}

fn digit<'a>() -> impl Parser<'a, &'a str, char, Ex<'a>> + Clone {
    any().filter(|c: &char| c.is_ascii_digit())
}

// ------------------------------------------------------------------ names

/// A declaration-level name: letters, digits, `-`, `_`, starting with a letter. A `-` is only
/// part of the name when a letter or digit follows it (so `x->y` still reads as `x ->`).
pub fn name<'a>() -> P<'a, Ident> {
    let tail = choice((
        any().filter(|c: &char| c.is_ascii_alphanumeric() || *c == '_'),
        just('-').then_ignore(any().filter(|c: &char| c.is_ascii_alphanumeric()).rewind()),
    ));
    any()
        .filter(|c: &char| c.is_ascii_alphabetic() || *c == '_')
        .then(tail.repeated())
        .to_slice()
        .map_with(|s: &str, e| {
            check_leading(s, e);
            Ident {
                name: s.to_string(),
                span: at(e),
            }
        })
        .labelled("a name")
        .boxed()
}

/// SPEC 1: names start with a letter.
fn check_leading<'a>(s: &str, e: &mut MapExtra<'a, '_, &'a str, Ex<'a>>) {
    if s.starts_with('_') {
        flag(
            e,
            format!("Names must start with a letter, but \"{s}\" starts with an underscore."),
            None,
        );
    }
}

/// A variable, field or record name: letters, digits, `_` (no `-`, which is subtraction).
pub fn ident<'a>() -> P<'a, Ident> {
    text::ascii::ident()
        .map_with(|s: &str, e| {
            check_leading(s, e);
            Ident {
                name: s.to_string(),
                span: at(e),
            }
        })
        .labelled("a name")
        .boxed()
}

/// Like [`ident`] but refuses words that are operators or keywords inside expressions.
fn word<'a>() -> P<'a, Ident> {
    ident()
        .try_map(|id, span| {
            if RESERVED.contains(&id.name.as_str()) {
                Err(Rich::custom(
                    span,
                    format!("\"{}\" is a reserved word", id.name),
                ))
            } else {
                Ok(id)
            }
        })
        .boxed()
}

// ------------------------------------------------------------------ types

pub fn type_expr<'a>() -> P<'a, TypeExpr> {
    recursive(|ty| {
        let wrap = |k: &'static str, ctor: fn(Box<TypeExpr>) -> TypeKind| {
            kw(k)
                .ignore_then(sp1())
                .ignore_then(kw("of"))
                .ignore_then(sp1())
                .ignore_then(ty.clone())
                .map_with(move |inner: TypeExpr, e| TypeExpr {
                    kind: ctor(Box::new(inner)),
                    span: at(e),
                })
        };
        // `maybe T` has no `of`; `list of T` and `result of T` do.
        let maybe = kw("maybe")
            .ignore_then(sp1())
            .ignore_then(ty.clone())
            .map_with(|inner: TypeExpr, e| TypeExpr {
                kind: TypeKind::Maybe(Box::new(inner)),
                span: at(e),
            });
        let list = wrap("list", TypeKind::List);
        let result = wrap("result", TypeKind::Result);
        // `map of K to V` (SPEC 22.2)
        let map = kw("map")
            .ignore_then(sp1())
            .ignore_then(kw("of"))
            .ignore_then(sp1())
            .ignore_then(ty.clone())
            .then_ignore(sp1())
            .then_ignore(kw("to"))
            .then_ignore(sp1())
            .then(ty.clone())
            .map_with(|(k, v): (TypeExpr, TypeExpr), e| TypeExpr {
                kind: TypeKind::Map(Box::new(k), Box::new(v)),
                span: at(e),
            });
        // `[mut|consume] handle of sha2.Sha256` (SPEC 22.4)
        let handle = choice((
            kw("mut").to(HandleMode::Mut),
            kw("consume").to(HandleMode::Consume),
        ))
        .then_ignore(sp1())
        .or_not()
        .then_ignore(kw("handle"))
        .then_ignore(sp1())
        .then_ignore(kw("of"))
        .then_ignore(sp1())
        .then(
            name()
                .map(|id| id.name)
                .separated_by(just('.'))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .map_with(|(mode, path), e| TypeExpr {
            kind: TypeKind::Handle {
                name: path.join("."),
                mode: mode.unwrap_or(HandleMode::Shared),
            },
            span: at(e),
        });
        // `lst of int`, `mayb int`: a word that is nearly a type constructor, followed by more type
        let typo = name()
            .filter(|id: &Ident| did_you_mean(&id.name, ["maybe", "list", "result", "map"]).is_some())
            // a type followed by `into x` (`read file p as int into n`) is a whole type
            .then_ignore(sp1().then(ident().filter(|next: &Ident| next.name != "into").rewind()))
            .map_with(|id, e| {
                let dym = did_you_mean(&id.name, ["maybe", "list", "result", "map"]);
                let r = e.span();
                let word = SimpleSpan::from(r.start..r.start + id.name.len());
                e.state()
                    .flag(word, format!("Unknown type \"{}\".", id.name), dym);
                TypeExpr {
                    span: id.span,
                    kind: TypeKind::Named(id.name),
                }
            });
        let named = name().map(|id| TypeExpr {
            span: id.span,
            kind: TypeKind::Named(id.name),
        });
        choice((handle, maybe, list, result, map, typo, named)).labelled("a type")
    })
    .boxed()
}

// ------------------------------------------------------------------ literals

// Plurals first: with equal distance the plural is the more natural correction.
const UNIT_WORDS: [&str; 14] = [
    "seconds", "minutes", "hours", "days", "weeks", "months", "years", "second", "minute", "hour",
    "day", "week", "month", "year",
];

/// A duration unit. A word that is nearly a unit (`minuts`) is flagged with a suggestion; any
/// other word is not a unit at all.
fn duration_unit<'a>() -> impl Parser<'a, &'a str, DurationUnit, Ex<'a>> + Clone {
    text::ascii::ident().try_map_with(|s: &str, e| {
        Ok(match s {
            "second" | "seconds" => DurationUnit::Second,
            "minute" | "minutes" => DurationUnit::Minute,
            "hour" | "hours" => DurationUnit::Hour,
            "day" | "days" => DurationUnit::Day,
            "week" | "weeks" => DurationUnit::Week,
            "month" | "months" => DurationUnit::Month,
            "year" | "years" => DurationUnit::Year,
            _ => match did_you_mean(s, UNIT_WORDS) {
                Some(dym) => {
                    flag(e, format!("Unknown time unit \"{s}\"."), Some(dym));
                    DurationUnit::Day
                }
                None => return Err(Rich::custom(e.span(), "not a duration unit")),
            },
        })
    })
}

/// `14 days`, `1 month`, `15 minutes` as a standalone parser (also used by headings/properties).
pub fn duration<'a>() -> P<'a, DurationLit> {
    text::int(10)
        .map_with(|s: &str, e| {
            s.parse::<i64>().unwrap_or_else(|_| {
                flag(e, "This number is too large.", None);
                0
            })
        })
        .then_ignore(sp1())
        .then(duration_unit())
        .map(|(amount, unit)| DurationLit { amount, unit })
        .labelled("a duration such as \"14 days\"")
        .boxed()
}

fn currency<'a>() -> impl Parser<'a, &'a str, &'a str, Ex<'a>> + Clone {
    text::ascii::ident().try_map(|s: &str, span| {
        if s.len() == 3 && s.chars().all(|c| c.is_ascii_uppercase()) {
            Ok(s)
        } else {
            Err(Rich::custom(span, "not a currency code"))
        }
    })
}

/// Minor-unit exponent of a currency (SPEC 6: integer minor units, no floats).
fn exponent(cur: &str) -> usize {
    match cur {
        "JPY" | "KRW" | "VND" | "CLP" | "ISK" => 0,
        "KWD" | "BHD" | "OMR" | "JOD" | "TND" => 3,
        _ => 2,
    }
}

/// `12.50` + `EUR` -> 1250 minor units, with a plain-language error when it cannot be exact.
fn money_minor(whole: &str, frac: Option<&str>, cur: &str) -> Result<i64, String> {
    let exp = exponent(cur);
    let frac = frac.unwrap_or("");
    if frac.len() > exp {
        return Err(format!(
            "{cur} has {exp} decimal places, so \"{whole}.{frac} {cur}\" cannot be written exactly. AgentMark never rounds money silently."
        ));
    }
    let mut digits = String::from(whole);
    digits.push_str(frac);
    for _ in frac.len()..exp {
        digits.push('0');
    }
    digits
        .parse::<i64>()
        .map_err(|_| "This amount is too large.".to_string())
}

fn date_lit<'a>() -> impl Parser<'a, &'a str, String, Ex<'a>> + Clone {
    digit()
        .repeated()
        .exactly(4)
        .then(just('-'))
        .then(digit().repeated().exactly(2))
        .then(just('-'))
        .then(digit().repeated().exactly(2))
        .then_ignore(any().filter(|c: &char| c.is_ascii_alphanumeric()).not())
        .to_slice()
        .map_with(|s: &str, e| {
            let month: u32 = s[5..7].parse().unwrap_or(0);
            let day: u32 = s[8..10].parse().unwrap_or(0);
            let year: u32 = s[0..4].parse().unwrap_or(0);
            let leap = (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
            let days = match month {
                2 if leap => 29,
                2 => 28,
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            };
            if !((1..=12).contains(&month) && (1..=days).contains(&day)) {
                flag(
                    e,
                    format!("\"{s}\" is not a real date. Dates are written year-month-day, like 2026-10-13."),
                    None,
                );
            }
            s.to_string()
        })
}

/// A number, a money amount (`12.50 EUR`), or a duration (`14 days`).
fn number_lit<'a>() -> P<'a, ExprKind> {
    let whole = digit().repeated().at_least(1).to_slice();
    let frac = just('.')
        .ignore_then(digit().repeated().at_least(1).to_slice())
        .or_not();
    let suffix = sp1().ignore_then(choice((
        currency().map(|c: &str| (Some(c), None)),
        duration_unit().map(|u| (None, Some(u))),
    )));
    // a `-` written directly before the digits makes a negative number (`-5`, `-12.50 EUR`)
    just('-')
        .or_not()
        .then(whole)
        .then(frac)
        .then(suffix.or_not())
        .map_with(|(((minus, whole), frac), suffix): (((Option<char>, &str), Option<&str>), _), e| {
            let neg = minus.is_some();
            let bad = |e: &mut MapExtra<'a, '_, &'a str, Ex<'a>>, msg: &str| {
                flag(e, msg, None);
                ExprKind::Int(0)
            };
            match suffix {
                Some((Some(cur), _)) => match money_minor(whole, frac, cur) {
                    Ok(minor) => ExprKind::Money {
                        minor: if neg { -minor } else { minor },
                        currency: cur.to_string(),
                    },
                    Err(m) => bad(e, &m),
                },
                Some((None, Some(unit))) => {
                    if frac.is_some() {
                        return bad(e, "Durations are whole numbers, like \"14 days\".");
                    }
                    if neg {
                        return bad(e, "A duration cannot be negative.");
                    }
                    match whole.parse::<i64>() {
                        Ok(amount) => ExprKind::Duration(DurationLit { amount, unit }),
                        Err(_) => bad(e, "This number is too large."),
                    }
                }
                _ => {
                    if frac.is_some() {
                        return bad(
                            e,
                            "Decimal numbers need a currency, like \"12.50 EUR\". AgentMark has no floating point numbers.",
                        );
                    }
                    match format!("{}{whole}", if neg { "-" } else { "" }).parse::<i64>() {
                        Ok(n) => ExprKind::Int(n),
                        Err(_) => bad(e, "This number is too large."),
                    }
                }
            }
        })
        .boxed()
}

// ------------------------------------------------------------------ expressions

/// `billing.Shape` for the leading segments of `billing.Shape.Circle`; `None` for a bare name.
pub fn qualifier_of(segs: &[Ident]) -> Option<Ident> {
    let first = segs.first()?;
    let last = segs.last()?;
    Some(Ident {
        name: segs.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join("."),
        span: first.span.to(last.span),
    })
}

fn mk<'a>(kind: ExprKind, e: &mut MapExtra<'a, '_, &'a str, Ex<'a>>) -> Expr {
    Expr { span: at(e), kind }
}

/// The full expression grammar (SPEC 5 plus the example forms).
pub fn expr<'a>() -> P<'a, Expr> {
    recursive(
        |expr: Recursive<chumsky::recursive::Direct<'a, 'a, &'a str, Expr, Ex<'a>>>| {
            // ---- strings with {holes}
            let esc = just('\\').ignore_then(choice((
                just('"'),
                just('\\'),
                just('{'),
                just('}'),
                just('n').to('\n'),
            )));
            let text_seg = choice((none_of("\"\\{"), esc))
                .repeated()
                .at_least(1)
                .collect::<String>()
                .map(InterpPart::Text);
            let hole = expr
                .clone()
                .padded_by(sp())
                .delimited_by(just('{'), just('}'))
                .map(InterpPart::Expr);
            let string = choice((text_seg, hole))
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just('"'), just('"'))
                .map_with(|parts, e| {
                    let kind = if parts.iter().all(|p| matches!(p, InterpPart::Text(_))) {
                        let s: String = parts
                            .into_iter()
                            .map(|p| match p {
                                InterpPart::Text(t) => t,
                                InterpPart::Expr(_) => String::new(),
                            })
                            .collect();
                        ExprKind::Str(s)
                    } else {
                        ExprKind::Interp(parts)
                    };
                    mk(kind, e)
                })
                .labelled("a quoted text");

            // ---- paths and calls
            // `regex.matches(...)`: after a dot a reserved word is fine when a call follows
            let later = choice((word(), ident().then_ignore(just('(').rewind())));
            let path_idents = word()
                .labelled("a value")
                .then(just('.').ignore_then(later).repeated().collect::<Vec<Ident>>())
                .map(|(first, mut rest)| {
                    rest.insert(0, first);
                    rest
                });
            let args = expr
                .clone()
                .padded_by(sp())
                .separated_by(just(','))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just('('), just(')'));
            // `json.parse(t) as Note`, `csv.parse(t) as list of Sale`
            let decode = path_idents
                .clone()
                .then(args.clone())
                .try_map(|(p, a), span| {
                    if p.len() == 2 && matches!(p[0].name.as_str(), "json" | "csv") && p[1].name == "parse" {
                        Ok((p, a))
                    } else {
                        Err(Rich::custom(span, "only json.parse and csv.parse take \"as\""))
                    }
                })
                .map_with(|(p, a), e| mk(ExprKind::Call { func: p, args: a }, e))
                .then(sp1().ignore_then(kw("as")).ignore_then(sp1()).ignore_then(type_expr()))
                .map_with(|(call, ty), e| mk(ExprKind::Decode { call: Box::new(call), ty }, e));
            let path_or_call = path_idents
                .clone()
                .then(args.or_not())
                .map_with(|(p, a), e| {
                    mk(
                        match a {
                            Some(args) => ExprKind::Call { func: p, args },
                            None => ExprKind::Path(p),
                        },
                        e,
                    )
                });
            let path_only = path_idents
                .clone()
                .map_with(|p, e| mk(ExprKind::Path(p), e));

            // ---- record literal { k: v, ... }
            let rec_field = ident()
                .then_ignore(sp())
                .then_ignore(just(':'))
                .then_ignore(sp())
                .then(expr.clone())
                .map(|(name, value)| RecordField { name, value });
            let braced_fields = rec_field
                .padded_by(sp())
                .separated_by(just(','))
                .allow_trailing()
                .collect::<Vec<_>>()
                .padded_by(sp())
                .delimited_by(just('{'), just('}'));
            let record = braced_fields
                .clone()
                .map_with(|fields, e| mk(ExprKind::Record(fields), e))
                .labelled("a record like { id: invoice.id }");

            // ---- an enum variant with fields: `Circle { radius: 3 }`, `Shape.Circle { radius: 3 }`
            let variant = path_idents
                .clone()
                .then_ignore(sp())
                .then(braced_fields)
                .map_with(|(mut p, fields), e| {
                    let name = p.pop().expect("a path has a segment");
                    let qualifier = qualifier_of(&p);
                    mk(
                        ExprKind::Variant {
                            qualifier,
                            name,
                            fields,
                        },
                        e,
                    )
                })
                .labelled("a variant like Circle { radius: 3 }");
            let paren = expr
                .clone()
                .padded_by(sp())
                .delimited_by(just('('), just(')'));
            // what a collection word is applied to (SPEC 22.2)
            let source = super::collections::source(
                choice((path_or_call.clone(), paren.clone())).boxed(),
            );

            // ---- built-in forms
            let count_of = kw("count")
                .ignore_then(sp1())
                .ignore_then(kw("of"))
                .ignore_then(sp1())
                .ignore_then(
                    name()
                        .then_ignore(sp1())
                        .then(word())
                        .then_ignore(sp1())
                        .then_ignore(kw("in"))
                        .then_ignore(sp1())
                        .then(path_only.clone())
                        .map(|((rule, noun), src)| (Some(rule), Some(noun), src)),
                )
                .map_with(|(rule, noun, src), e| {
                    mk(
                        ExprKind::CountOf {
                            rule,
                            noun,
                            source: Box::new(src),
                        },
                        e,
                    )
                });
            let first_in = kw("first")
                .ignore_then(sp1())
                .ignore_then(word())
                .then_ignore(sp1())
                .then_ignore(kw("in"))
                .then_ignore(sp1())
                .then(path_only.clone())
                .then_ignore(sp1())
                .then_ignore(kw("where"))
                .then_ignore(sp1())
                .then(expr.clone())
                .map_with(|((var, src), cond), e| {
                    mk(
                        ExprKind::FirstIn {
                            var,
                            source: Box::new(src),
                            cond: Box::new(cond),
                        },
                        e,
                    )
                });
            let no_amounts = kw("no")
                .ignore_then(sp1())
                .ignore_then(kw("amounts"))
                .ignore_then(sp1())
                .ignore_then(kw("changed"))
                .ignore_then(sp1())
                .ignore_then(kw("in"))
                .ignore_then(sp1())
                .ignore_then(path_only.clone())
                .map_with(|p, e| mk(ExprKind::NoAmountsChanged(Box::new(p)), e));

            let atom_kw = choice((
                kw("true").to(ExprKind::Bool(true)),
                kw("false").to(ExprKind::Bool(false)),
                kw("today").to(ExprKind::Today),
                kw("now").to(ExprKind::Now),
            ))
            .map_with(|k, e| mk(k, e));
            let date = date_lit().map_with(|d, e| mk(ExprKind::Date(d), e));
            let number = number_lit().map_with(|k, e| mk(k, e));

            // collection words (SPEC 22.2): the words first, then names and parentheses with the
            // `where` / `at` tails that hang off a source
            let operand = choice((
                string.clone(),
                paren.clone(),
                number.clone(),
                path_or_call.clone(),
            ))
            .boxed();
            let coll = super::collections::forms(
                expr.clone().boxed(),
                source,
                operand.clone(),
                type_expr(),
                count_of.boxed(),
            );
            let path_or_call =
                super::collections::postfix(path_or_call.boxed(), expr.clone().boxed(), operand);

            // `some x`: the value is one atom (a path, call, literal or parenthesised expression)
            let some_inner = choice((
                string.clone(),
                variant.clone(),
                paren.clone(),
                date.clone(),
                number.clone(),
                path_or_call.clone(),
            ))
            .boxed();
            let some_expr = kw("some")
                .ignore_then(sp1())
                .ignore_then(some_inner)
                .map_with(|x, e| mk(ExprKind::Some(Box::new(x)), e))
                .boxed();
            let none_expr = kw("none").map_with(|_, e| mk(ExprKind::None, e));

            // `-x`, `-(a + b)`, `-f(x)`: a `-` directly before a name or parenthesis is `x * -1`
            // (Q408); `-5` and `-12.50 EUR` are literals
            let negated = just('-')
                .ignore_then(choice((paren.clone(), path_or_call.clone())))
                .map_with(|x: Expr, e| {
                    let minus_one = mk(ExprKind::Int(-1), e);
                    mk(
                        ExprKind::Binary {
                            op: BinOp::Mul,
                            lhs: Box::new(x),
                            rhs: Box::new(minus_one),
                        },
                        e,
                    )
                })
                .boxed();

            let atom = choice((
                some_expr,
                negated,
                none_expr,
                string,
                record,
                variant,
                coll,
                paren,
                date,
                number,
                first_in,
                no_amounts,
                atom_kw,
                decode,
                path_or_call,
            ))
            .labelled("a value");

            // `reminder length` (postfix)
            let post = atom
                .then(
                    sp1()
                        .ignore_then(just('l').rewind())
                        .ignore_then(kw("length"))
                        .or_not(),
                )
                .map_with(|(a, len), e| match len {
                    Some(()) => mk(ExprKind::Length(Box::new(a)), e),
                    None => a,
                });

            let mul_op =
                choice((just('*').to(BinOp::Mul), just('/').to(BinOp::Div))).padded_by(sp());
            let product = post.clone().foldl_with(mul_op.then(post).repeated(), bin);

            // Subtraction needs spaces around `-`: that keeps `2026-13-01` a (bad) date rather than
            // arithmetic, and keeps the `->` that introduces a result variable out of reach.
            let sub = sp1()
                .ignore_then(just('-'))
                .then_ignore(sp1())
                .to(BinOp::Sub);
            let add = just('+').padded_by(sp()).to(BinOp::Add);
            let add_op = choice((add, sub));
            let sum = product
                .clone()
                .foldl_with(add_op.then(product).repeated(), bin);

            let cmp_op = choice((
                just("<=").to(BinOp::Le),
                just(">=").to(BinOp::Ge),
                just("!=").to(BinOp::Ne),
                just("=").to(BinOp::Eq),
                just("<").to(BinOp::Lt),
                just(">").to(BinOp::Gt),
            ))
            .padded_by(sp());

            #[derive(Clone)]
            enum Tail {
                Cmp(BinOp, Expr),
                IsEmpty,
                IsNotEmpty,
                Matches(Ident),
            }
            let tail = choice((
                cmp_op.then(sum.clone()).map(|(op, rhs)| Tail::Cmp(op, rhs)),
                sp1()
                    .ignore_then(kw("is"))
                    .ignore_then(sp1())
                    .ignore_then(kw("not"))
                    .ignore_then(sp1())
                    .ignore_then(kw("empty"))
                    .to(Tail::IsNotEmpty),
                sp1()
                    .ignore_then(kw("is"))
                    .ignore_then(sp1())
                    .ignore_then(kw("empty"))
                    .to(Tail::IsEmpty),
                sp1()
                    .ignore_then(kw("matches"))
                    .ignore_then(sp1())
                    .ignore_then(ident())
                    .map(Tail::Matches),
            ));
            let comparison = sum.then(tail.or_not()).map_with(|(lhs, t), e| match t {
                None => lhs,
                Some(Tail::Cmp(op, rhs)) => Expr {
                    span: lhs.span.to(rhs.span),
                    kind: ExprKind::Binary {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                },
                Some(Tail::IsEmpty) => mk(ExprKind::IsEmpty(Box::new(lhs)), e),
                Some(Tail::IsNotEmpty) => mk(ExprKind::IsNotEmpty(Box::new(lhs)), e),
                Some(Tail::Matches(record)) => mk(
                    ExprKind::Matches {
                        expr: Box::new(lhs),
                        record,
                    },
                    e,
                ),
            });

            let not_expr = kw("not")
                .ignore_then(sp1())
                .map_with(|_, e| at(e))
                .repeated()
                .foldr_with(comparison, |not_span: Span, inner: Expr, _e| Expr {
                    span: not_span.to(inner.span),
                    kind: ExprKind::Not(Box::new(inner)),
                });

            let and_expr = not_expr.clone().foldl_with(
                sp1()
                    .ignore_then(kw("and"))
                    .ignore_then(sp1())
                    .ignore_then(not_expr)
                    .repeated(),
                |lhs, rhs, _e| Expr {
                    span: lhs.span.to(rhs.span),
                    kind: ExprKind::Binary {
                        op: BinOp::And,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                },
            );
            and_expr.clone().foldl_with(
                sp1()
                    .ignore_then(kw("or"))
                    .ignore_then(sp1())
                    .ignore_then(and_expr)
                    .repeated(),
                |lhs, rhs, _e| Expr {
                    span: lhs.span.to(rhs.span),
                    kind: ExprKind::Binary {
                        op: BinOp::Or,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                },
            )
        },
    )
    .boxed()
}

// ------------------------------------------------------------------ running a parser on a line

/// Parse the whole of `text` (which sits at `line`, `col` in the file) with `p`.
pub fn run<'a, T>(
    p: &P<'a, T>,
    text: &'a str,
    line: u32,
    col: u32,
) -> Result<T, (LineCtx, Vec<Rich<'a, char>>)> {
    let mut ctx = extra::SimpleState(LineCtx::new(line, col, text));
    let res = p.parse_with_state(text, &mut ctx);
    match res.into_result() {
        Ok(v) if ctx.0.problems.is_empty() => Ok(v),
        Ok(_) => Err((ctx.0, Vec::new())),
        Err(errs) => Err((ctx.0, errs)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Expr {
        let p = expr();
        match run(&p, src, 1, 1) {
            Ok(e) => e,
            Err((_, errs)) => panic!("failed to parse {src:?}: {errs:?}"),
        }
    }

    fn fails(src: &str) -> bool {
        let p = expr();
        run(&p, src, 1, 1).is_err()
    }

    fn path(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Path(p) => p
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>()
                .join("."),
            other => panic!("not a path: {other:?}"),
        }
    }

    #[test]
    fn literals() {
        assert_eq!(parse("42").kind, ExprKind::Int(42));
        assert_eq!(parse("true").kind, ExprKind::Bool(true));
        assert_eq!(parse("\"hi\"").kind, ExprKind::Str("hi".into()));
        assert_eq!(
            parse("2026-10-13").kind,
            ExprKind::Date("2026-10-13".into())
        );
        assert_eq!(parse("today").kind, ExprKind::Today);
        assert_eq!(parse("now").kind, ExprKind::Now);
    }

    #[test]
    fn money_is_integer_minor_units() {
        assert_eq!(
            parse("12.50 EUR").kind,
            ExprKind::Money {
                minor: 1250,
                currency: "EUR".into()
            }
        );
        assert_eq!(
            parse("0 EUR").kind,
            ExprKind::Money {
                minor: 0,
                currency: "EUR".into()
            }
        );
        assert_eq!(
            parse("7.5 USD").kind,
            ExprKind::Money {
                minor: 750,
                currency: "USD".into()
            }
        );
        assert_eq!(
            parse("500 JPY").kind,
            ExprKind::Money {
                minor: 500,
                currency: "JPY".into()
            }
        );
    }

    #[test]
    fn money_that_cannot_be_exact_is_an_error() {
        assert!(fails("12.505 EUR"));
        assert!(fails("1.5 JPY"));
        assert!(fails("3.5"));
    }

    #[test]
    fn durations() {
        for (src, amount, unit) in [
            ("14 days", 14, DurationUnit::Day),
            ("1 month", 1, DurationUnit::Month),
            ("15 minutes", 15, DurationUnit::Minute),
        ] {
            assert_eq!(
                parse(src).kind,
                ExprKind::Duration(DurationLit { amount, unit })
            );
        }
    }

    #[test]
    fn bad_dates_are_rejected() {
        assert!(fails("2026-13-01"));
        assert!(fails("2026-10-32"));
    }

    #[test]
    fn precedence_and_or_comparison_arithmetic() {
        // today > due + grace and status != "paid"
        let e = parse("today > due + grace_period and status != \"paid\"");
        let ExprKind::Binary {
            op: BinOp::And,
            lhs,
            rhs,
        } = e.kind
        else {
            panic!("{e:?}")
        };
        let ExprKind::Binary {
            op: BinOp::Gt,
            rhs: sum,
            ..
        } = lhs.kind
        else {
            panic!()
        };
        assert!(matches!(sum.kind, ExprKind::Binary { op: BinOp::Add, .. }));
        assert!(matches!(rhs.kind, ExprKind::Binary { op: BinOp::Ne, .. }));

        // a or b and c  ==  a or (b and c)
        let e = parse("a or b and c");
        let ExprKind::Binary {
            op: BinOp::Or, rhs, ..
        } = e.kind
        else {
            panic!()
        };
        assert!(matches!(rhs.kind, ExprKind::Binary { op: BinOp::And, .. }));

        // * binds tighter than -
        let e = parse("a - b * 2");
        let ExprKind::Binary {
            op: BinOp::Sub,
            rhs,
            ..
        } = e.kind
        else {
            panic!()
        };
        assert!(matches!(rhs.kind, ExprKind::Binary { op: BinOp::Mul, .. }));
    }

    #[test]
    fn arithmetic_from_the_spec_examples() {
        assert!(matches!(
            parse("due + 14 days").kind,
            ExprKind::Binary { op: BinOp::Add, .. }
        ));
        assert!(matches!(
            parse("today - 1 month").kind,
            ExprKind::Binary { op: BinOp::Sub, .. }
        ));
        assert!(matches!(
            parse("invoice.amount - invoice.amount * vat / 100").kind,
            ExprKind::Binary { op: BinOp::Sub, .. }
        ));
    }

    #[test]
    fn not_binds_looser_than_comparison() {
        let e = parse("not a = b");
        let ExprKind::Not(inner) = e.kind else {
            panic!()
        };
        assert!(matches!(inner.kind, ExprKind::Binary { op: BinOp::Eq, .. }));
        assert!(matches!(parse("not dry_run").kind, ExprKind::Not(_)));
    }

    #[test]
    fn predicates() {
        assert!(matches!(
            parse("summary is not empty").kind,
            ExprKind::IsNotEmpty(_)
        ));
        assert!(matches!(
            parse("ticket.priority is empty").kind,
            ExprKind::IsEmpty(_)
        ));
        assert!(matches!(
            parse("q matches Qualification").kind,
            ExprKind::Matches { .. }
        ));
        let e = parse("reminder length < max_reminder_length");
        let ExprKind::Binary {
            op: BinOp::Lt, lhs, ..
        } = e.kind
        else {
            panic!()
        };
        assert!(matches!(lhs.kind, ExprKind::Length(_)));
        let e = parse("no amounts changed in reminder");
        let ExprKind::NoAmountsChanged(v) = e.kind else {
            panic!()
        };
        assert_eq!(path(&v), "reminder");
    }

    #[test]
    fn paths_and_calls() {
        assert_eq!(path(&parse("state.invoices")), "state.invoices");
        let e = parse("net_amount(found, 19)");
        let ExprKind::Call { func, args } = e.kind else {
            panic!()
        };
        assert_eq!(func[0].name, "net_amount");
        assert_eq!(args.len(), 2);
        let e = parse("billing.overdue(x)");
        let ExprKind::Call { func, .. } = e.kind else {
            panic!()
        };
        assert_eq!(func.len(), 2);
    }

    #[test]
    fn record_literals() {
        let e = parse("{ ticket: ticket.id, category: t.category }");
        let ExprKind::Record(fields) = e.kind else {
            panic!()
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[1].name.name, "category");
        assert!(matches!(parse("{}").kind, ExprKind::Record(_)));
        let e =
            parse("{ id: found.id, net: net_amount(found, 19), fingerprint: fingerprint(found) }");
        assert!(matches!(e.kind, ExprKind::Record(f) if f.len() == 3));
    }

    #[test]
    fn count_of_forms() {
        let e = parse("count of open invoices in crm.invoices");
        let ExprKind::CountOf { rule, noun, source } = e.kind else {
            panic!()
        };
        assert_eq!(rule.unwrap().name, "open");
        assert_eq!(noun.unwrap().name, "invoices");
        assert_eq!(path(&source), "crm.invoices");
        let e = parse("count of rows");
        let ExprKind::CountOf { rule, source, .. } = e.kind else {
            panic!()
        };
        assert!(rule.is_none());
        assert_eq!(path(&source), "rows");
    }

    #[test]
    fn first_in_where() {
        let e = parse("first invoice in state.invoices where invoice.id = id");
        let ExprKind::FirstIn { var, source, cond } = e.kind else {
            panic!()
        };
        assert_eq!(var.name, "invoice");
        assert_eq!(path(&source), "state.invoices");
        assert!(matches!(cond.kind, ExprKind::Binary { op: BinOp::Eq, .. }));
    }

    #[test]
    fn interpolated_strings() {
        let e = parse("\"Lead {lead.company} scored {q.score}, no action\"");
        let ExprKind::Interp(parts) = e.kind else {
            panic!()
        };
        assert_eq!(parts.len(), 5);
        assert!(matches!(&parts[0], InterpPart::Text(t) if t == "Lead "));
        assert!(matches!(&parts[1], InterpPart::Expr(_)));
        // holes hold full expressions
        let e = parse("\"Imported {count of rows} rows\"");
        let ExprKind::Interp(parts) = e.kind else {
            panic!()
        };
        let InterpPart::Expr(h) = &parts[1] else {
            panic!()
        };
        assert!(matches!(h.kind, ExprKind::CountOf { .. }));
    }

    #[test]
    fn string_escapes() {
        assert_eq!(
            parse(r#""a \"quoted\" \{x\}""#).kind,
            ExprKind::Str("a \"quoted\" {x}".into())
        );
        assert_eq!(parse("\"\"").kind, ExprKind::Str(String::new()));
    }

    #[test]
    fn expression_stops_before_else_and_arrow() {
        let p = expr();
        // the caller (statement grammar) continues after the expression; here we only check that
        // the expression itself does not swallow `else` or `->`.
        let q = p
            .clone()
            .then_ignore(sp1())
            .then_ignore(kw("else"))
            .then_ignore(any().repeated())
            .boxed();
        assert!(run(&q, "found is not empty else respond 404", 1, 1).is_ok());
        let q = p
            .then_ignore(sp())
            .then_ignore(just("->"))
            .then_ignore(any().repeated())
            .boxed();
        assert!(run(&q, "a - b -> n", 1, 1).is_ok());
        assert!(run(&q, "a -> n", 1, 1).is_ok());
    }

    #[test]
    fn spans_are_file_positions() {
        let p = expr();
        let e = run(&p, "due + 14 days", 7, 20).unwrap();
        assert_eq!(e.span, Span::new(7, 20, 13));
        let ExprKind::Binary { rhs, .. } = e.kind else {
            panic!()
        };
        assert_eq!(rhs.span, Span::new(7, 26, 7));
    }

    #[test]
    fn a_number_followed_by_a_keyword_is_not_a_unit_typo() {
        let p = expr();
        for src in [
            "a > 3 and b",
            "x = 1 or y",
            "n < 5 else",
            "7 in rows",
            "2 as x",
        ] {
            let q = p.clone().then_ignore(any().repeated()).boxed();
            match run(&q, src, 1, 1) {
                Ok(_) => {}
                Err((ctx, e)) => panic!("{src}: problems {:?} errors {e:?}", ctx.problems),
            }
        }
    }

    #[test]
    fn reserved_words_are_not_variables() {
        assert!(fails("and"));
        assert!(fails("is"));
    }

    #[test]
    fn types() {
        let p = type_expr();
        let t = run(&p, "list of Invoice", 1, 1).unwrap();
        assert!(
            matches!(t.kind, TypeKind::List(inner) if matches!(&inner.kind, TypeKind::Named(n) if n == "Invoice"))
        );
        let t = run(&p, "maybe int", 1, 1).unwrap();
        assert!(matches!(t.kind, TypeKind::Maybe(_)));
        let t = run(&p, "result of text", 1, 1).unwrap();
        assert!(matches!(t.kind, TypeKind::Result(_)));
        let t = run(&p, "maybe list of text", 1, 1).unwrap();
        assert!(
            matches!(t.kind, TypeKind::Maybe(inner) if matches!(inner.kind, TypeKind::List(_)))
        );
    }
}
