//! Statement grammar (SPEC 4, 13, 14, 16). One parser for the text of one statement line; the
//! block structure (children, `else`, `otherwise`) is assembled from indentation in `parser.rs`.

use chumsky::prelude::*;

use super::expr::*;
use crate::syntax::ast::*;

/// What one statement line is, before its indented children are attached.
#[derive(Debug, Clone)]
pub enum Head {
    ForEach {
        filter_rule: Option<Ident>,
        var: Ident,
        source: Expr,
    },
    If {
        cond: Expr,
    },
    IfSome {
        expr: Expr,
        bind: Ident,
    },
    Else,
    Otherwise,
    Parallel,
    /// A statement that has no body of its own.
    Simple(Stmt),
    /// `try <statement>`; an `otherwise:` child line may follow.
    Try(Stmt),
}

/// Words that start a statement; they can never be a tool name.
const STATEMENT_WORDS: &[&str] = &[
    "for",
    "if",
    "else",
    "stop",
    "do",
    "ask",
    "guard",
    "set",
    "return",
    "respond",
    "read",
    "try",
    "parallel",
    "append",
    "print",
    "otherwise",
];

fn stmt_at<'a>(kind: StmtKind, e: &mut chumsky::input::MapExtra<'a, '_, &'a str, Ex<'a>>) -> Stmt {
    Stmt { span: at(e), kind }
}

fn arrow<'a>() -> impl Parser<'a, &'a str, (), Ex<'a>> + Clone {
    sp().ignore_then(just("->"))
        .ignore_then(sp())
        .labelled("\"->\"")
}

fn colon<'a>() -> impl Parser<'a, &'a str, (), Ex<'a>> + Clone {
    sp().ignore_then(just(':')).ignored().labelled("\":\"")
}

/// The statement-line grammar.
pub fn head<'a>() -> P<'a, Head> {
    let expr = expr();
    let ty = type_expr();

    // ------------------------------------------------------------ simple statements
    let simple = recursive(
        |simple: Recursive<chumsky::recursive::Direct<'a, 'a, &'a str, Stmt, Ex<'a>>>| {
            let into = arrow().ignore_then(ident());

            // do name | do name(args) -> x | do tool verb target: payload -> x
            let tool_call_tail = tool_call_tail(expr.clone());
            let do_call = name()
                .then(
                    expr.clone()
                        .padded_by(sp())
                        .separated_by(just(','))
                        .allow_trailing()
                        .collect::<Vec<_>>()
                        .delimited_by(just('('), just(')')),
                )
                .then(into.clone().or_not())
                .map(|((name, args), into)| StmtKind::Do { name, args, into });
            let do_tool = name()
                .then_ignore(sp1())
                .then(name())
                .then(targets_payload_into(expr.clone()))
                .map(
                    |((tool, verb), (target, payload, into))| StmtKind::ToolCall {
                        tool,
                        verb: Some(verb),
                        target,
                        payload,
                        into,
                        via_do: true,
                    },
                );
            let do_plain = name()
                .then(into.clone().or_not())
                .map(|(name, into)| StmtKind::Do {
                    name,
                    args: vec![],
                    into,
                });
            let do_stmt = kw("do")
                .ignore_then(sp1())
                .ignore_then(choice((do_call, do_tool, do_plain)))
                .map_with(|k, e| stmt_at(k, e));

            // ask model [with v]: "prompt" [as Record] -> var
            let ask = kw("ask")
                .ignore_then(sp1())
                .ignore_then(kw("model"))
                .ignore_then(
                    sp1()
                        .ignore_then(kw("with"))
                        .ignore_then(sp1())
                        .ignore_then(expr.clone())
                        .or_not(),
                )
                .then_ignore(colon())
                .then_ignore(sp())
                .then(expr.clone())
                .then(
                    sp1()
                        .ignore_then(kw("as"))
                        .ignore_then(sp1())
                        .ignore_then(ident())
                        .or_not(),
                )
                .then(into.clone())
                .map_with(|(((with, prompt), as_record), into), e| {
                    stmt_at(
                        StmtKind::AskModel {
                            with,
                            prompt,
                            as_record,
                            into,
                        },
                        e,
                    )
                });

            // guard: cond [else statement]
            let guard = kw("guard")
                .ignore_then(colon())
                .ignore_then(sp())
                .ignore_then(expr.clone())
                .then(
                    sp1()
                        .ignore_then(kw("else"))
                        .ignore_then(sp1())
                        .ignore_then(simple)
                        .or_not(),
                )
                .map_with(|(cond, else_), e| {
                    stmt_at(
                        StmtKind::Guard {
                            cond,
                            else_: else_.map(Box::new),
                        },
                        e,
                    )
                });

            let set = kw("set")
                .ignore_then(sp1())
                .ignore_then(ident())
                .then_ignore(sp())
                .then_ignore(just('='))
                .then_ignore(sp())
                .then(expr.clone())
                .map_with(|(var, expr), e| stmt_at(StmtKind::Set { var, expr }, e));
            let ret = kw("return")
                .ignore_then(sp1())
                .ignore_then(expr.clone())
                .map_with(|x, e| stmt_at(StmtKind::Return(x), e));
            let respond = kw("respond")
                .ignore_then(sp1())
                .ignore_then(text::int(10).try_map(|s: &str, span| {
                    s.parse::<u16>()
                        .ok()
                        .filter(|n| (100..=599).contains(n))
                        .ok_or_else(|| {
                            Rich::custom(span, "an HTTP status such as 200 or 404 is expected")
                        })
                }))
                .then(sp1().ignore_then(expr.clone()).or_not())
                .map_with(|(status, body), e| stmt_at(StmtKind::Respond { status, body }, e));
            let read_body = kw("read")
                .ignore_then(sp1())
                .ignore_then(kw("body"))
                .ignore_then(sp1())
                .ignore_then(kw("as"))
                .ignore_then(sp1())
                .ignore_then(ty.clone())
                .then(into.clone())
                .map_with(|(ty, into), e| stmt_at(StmtKind::ReadBody { ty, into }, e));
            let read_file = kw("read")
                .ignore_then(sp1())
                .ignore_then(kw("file"))
                .ignore_then(sp1())
                .ignore_then(expr.clone())
                .then_ignore(sp1())
                .then_ignore(kw("as"))
                .then_ignore(sp1())
                .then(ty.clone())
                .then(into.clone())
                .map_with(|((path, ty), into), e| {
                    stmt_at(StmtKind::ReadFile { path, ty, into }, e)
                });
            let append = kw("append")
                .ignore_then(sp1())
                .ignore_then(expr.clone())
                .then_ignore(sp1())
                .then_ignore(kw("to"))
                .then_ignore(sp1())
                .then(expr.clone())
                .map_with(|(value, list), e| stmt_at(StmtKind::Append { value, list }, e));
            let print = kw("print")
                .ignore_then(colon())
                .ignore_then(sp())
                .ignore_then(expr.clone())
                .map_with(|x, e| stmt_at(StmtKind::Print(x), e));
            let stop = kw("stop").map_with(|_, e| stmt_at(StmtKind::Stop, e));

            // <tool> [verb [target...]] [: payload] [-> var]
            let tool = name().filter(|id: &Ident| !STATEMENT_WORDS.contains(&id.name.as_str()));
            let tool_call =
                tool.then(tool_call_tail)
                    .map_with(|(tool, (verb, target, payload, into)), e| {
                        stmt_at(
                            StmtKind::ToolCall {
                                tool,
                                verb,
                                target,
                                payload,
                                into,
                                via_do: false,
                            },
                            e,
                        )
                    });

            choice((
                stop, do_stmt, ask, guard, set, ret, respond, read_body, read_file, append, print,
                tool_call,
            ))
            .labelled("a statement")
        },
    );

    // ------------------------------------------------------------ statements with bodies
    let for_each = kw("for")
        .ignore_then(sp1())
        .ignore_then(kw("each"))
        .ignore_then(sp1())
        .ignore_then(choice((
            ident_word()
                .then_ignore(sp1())
                .then(ident_word())
                .then_ignore(sp1())
                .then_ignore(kw("in"))
                .map(|(rule, var)| (Some(rule), var)),
            ident_word()
                .then_ignore(sp1())
                .then_ignore(kw("in"))
                .map(|var| (None, var)),
        )))
        .then_ignore(sp1())
        .then(expr.clone())
        .then_ignore(colon())
        .map(|((filter_rule, var), source)| Head::ForEach {
            filter_rule,
            var,
            source,
        });

    let if_head = kw("if")
        .ignore_then(sp1())
        .ignore_then(expr.clone())
        .then(
            sp1()
                .ignore_then(kw("is"))
                .ignore_then(sp1())
                .ignore_then(kw("some"))
                .ignore_then(sp1())
                .ignore_then(ident())
                .or_not(),
        )
        .then_ignore(colon())
        .map(|(cond, bind)| match bind {
            Some(bind) => Head::IfSome { expr: cond, bind },
            None => Head::If { cond },
        });

    let else_head = kw("else").ignore_then(colon()).to(Head::Else);
    let otherwise = kw("otherwise").ignore_then(colon()).to(Head::Otherwise);
    let parallel = kw("parallel").ignore_then(colon()).to(Head::Parallel);
    let try_head = kw("try")
        .ignore_then(sp1())
        .ignore_then(simple.clone())
        .map(Head::Try);

    choice((
        for_each,
        if_head,
        else_head,
        otherwise,
        parallel,
        try_head,
        simple.map(Head::Simple),
    ))
    .boxed()
}

/// A name that is not an expression keyword (for loop variables).
fn ident_word<'a>() -> P<'a, Ident> {
    ident()
        .try_map(|id, span| {
            if ["in", "and", "or", "not", "is", "as", "where", "else"].contains(&id.name.as_str()) {
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

type TargetsPayloadInto = (Vec<TargetPart>, Option<Payload>, Option<Ident>);

/// ` target target...` `: payload` `-> into`, every part optional.
fn targets_payload_into<'a>(expr: P<'a, Expr>) -> P<'a, TargetsPayloadInto> {
    let part = choice((
        expr_string().map(TargetPart::Value),
        name().map(TargetPart::Word),
    ));
    let targets = sp1().ignore_then(part).repeated().collect::<Vec<_>>();
    let assign = ident()
        .then_ignore(sp())
        .then_ignore(just('=').then_ignore(just('=').not()))
        .then_ignore(sp())
        .then(expr.clone())
        .map(|(field, value)| Assign { field, value });
    let payload = colon().ignore_then(sp()).ignore_then(choice((
        assign
            .separated_by(sp().then(just(',')).then(sp()))
            .at_least(1)
            .collect::<Vec<_>>()
            .map(Payload::Assign),
        expr.map(Payload::Value),
    )));
    let into = arrow().ignore_then(ident());
    targets
        .then(payload.or_not())
        .then(into.or_not())
        .map(|((t, p), i)| (t, p, i))
        .boxed()
}

type ToolTail = (
    Option<Ident>,
    Vec<TargetPart>,
    Option<Payload>,
    Option<Ident>,
);

/// Everything after the tool name: `[verb [target...]] [: payload] [-> var]`.
fn tool_call_tail<'a>(expr: P<'a, Expr>) -> P<'a, ToolTail> {
    let verb = sp1().ignore_then(name());
    verb.or_not()
        .then(targets_payload_into(expr))
        .map(|(verb, (t, p, i))| (verb, t, p, i))
        .boxed()
}

/// A plain or interpolated string literal, as a target value (`address "x@y.z"`).
fn expr_string<'a>() -> P<'a, Expr> {
    expr()
        .try_map(|e: Expr, span| match e.kind {
            ExprKind::Str(_) | ExprKind::Interp(_) => Ok(e),
            _ => Err(Rich::custom(span, "a quoted text is expected here")),
        })
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(src: &str) -> Head {
        let p = head();
        match run(&p, src, 1, 1) {
            Ok(h) => h,
            Err((_, errs)) => panic!("failed to parse {src:?}: {errs:?}"),
        }
    }

    fn kind(src: &str) -> StmtKind {
        match h(src) {
            Head::Simple(s) => s.kind,
            other => panic!("not a simple statement: {other:?}"),
        }
    }

    fn fails(src: &str) -> bool {
        let p = head();
        run(&p, src, 1, 1).is_err()
    }

    #[test]
    fn control_lines() {
        assert!(matches!(
            h("for each row in rows:"),
            Head::ForEach {
                filter_rule: None,
                ..
            }
        ));
        let Head::ForEach {
            filter_rule, var, ..
        } = h("for each overdue invoice in crm.invoices:")
        else {
            panic!()
        };
        assert_eq!(filter_rule.unwrap().name, "overdue");
        assert_eq!(var.name, "invoice");
        assert!(matches!(h("if not dry_run:"), Head::If { .. }));
        assert!(matches!(
            h("if invoice.score is some s:"),
            Head::IfSome { .. }
        ));
        assert!(matches!(h("else:"), Head::Else));
        assert!(matches!(h("otherwise:"), Head::Otherwise));
        assert!(matches!(h("parallel:"), Head::Parallel));
        assert!(matches!(
            h("if ticket.priority is not empty:"),
            Head::If { .. }
        ));
    }

    #[test]
    fn stop_and_do() {
        assert!(matches!(kind("stop"), StmtKind::Stop));
        let StmtKind::Do { name, args, into } = kind("do remind") else {
            panic!()
        };
        assert_eq!(name.name, "remind");
        assert!(args.is_empty() && into.is_none());
        let StmtKind::Do { args, into, .. } = kind("do net_amount(invoice, 19) -> n") else {
            panic!()
        };
        assert_eq!(args.len(), 2);
        assert_eq!(into.unwrap().name, "n");
    }

    #[test]
    fn do_with_a_tool_call() {
        let StmtKind::ToolCall {
            tool,
            verb,
            target,
            into,
            via_do,
            ..
        } = kind("do crm get invoice id -> inv")
        else {
            panic!()
        };
        assert!(via_do);
        assert_eq!(
            (tool.name.as_str(), verb.unwrap().name.as_str()),
            ("crm", "get")
        );
        assert_eq!(target.len(), 2);
        assert_eq!(into.unwrap().name, "inv");
        let StmtKind::ToolCall { payload, .. } = kind("do crm update invoice: inv") else {
            panic!()
        };
        assert!(matches!(payload, Some(Payload::Value(_))));
    }

    #[test]
    fn ask_model_forms() {
        let StmtKind::AskModel {
            with,
            as_record,
            into,
            ..
        } = kind("ask model with lead: \"Score this lead.\" as Qualification -> q")
        else {
            panic!()
        };
        assert!(with.is_some());
        assert_eq!(as_record.unwrap().name, "Qualification");
        assert_eq!(into.name, "q");
        let StmtKind::AskModel {
            with, as_record, ..
        } = kind("ask model: \"Hi {x}\" -> summary")
        else {
            panic!()
        };
        assert!(with.is_none() && as_record.is_none());
    }

    #[test]
    fn guards() {
        let StmtKind::Guard { cond, else_ } = kind("guard: no amounts changed in reminder") else {
            panic!()
        };
        assert!(matches!(cond.kind, ExprKind::NoAmountsChanged(_)));
        assert!(else_.is_none());
        let StmtKind::Guard { else_, .. } = kind("guard: found is not empty else respond 404")
        else {
            panic!()
        };
        let else_ = else_.unwrap();
        assert!(matches!(
            else_.kind,
            StmtKind::Respond {
                status: 404,
                body: None
            }
        ));
        let StmtKind::Guard { else_, .. } = kind("guard: inv.amount > 0 EUR else stop") else {
            panic!()
        };
        assert!(matches!(else_.unwrap().kind, StmtKind::Stop));
    }

    #[test]
    fn tool_calls() {
        let StmtKind::ToolCall {
            tool,
            verb,
            target,
            payload,
            ..
        } = kind("email send customer: \"{reminder}\"")
        else {
            panic!()
        };
        assert_eq!(
            (tool.name.as_str(), verb.unwrap().name.as_str()),
            ("email", "send")
        );
        assert!(matches!(&target[0], TargetPart::Word(w) if w.name == "customer"));
        assert!(matches!(payload, Some(Payload::Value(_))));

        let StmtKind::ToolCall { target, .. } =
            kind("email send address \"sales@optimotion.dev\": \"hi\"")
        else {
            panic!()
        };
        assert!(matches!(&target[0], TargetPart::Word(w) if w.name == "address"));
        assert!(matches!(&target[1], TargetPart::Value(_)));

        let StmtKind::ToolCall { verb, payload, .. } =
            kind("crm update invoice: status = \"reminded\"")
        else {
            panic!()
        };
        assert_eq!(verb.unwrap().name, "update");
        let Some(Payload::Assign(a)) = payload else {
            panic!()
        };
        assert_eq!(a[0].field.name, "status");

        let StmtKind::ToolCall { payload, .. } = kind("crm update ticket: priority = t.priority")
        else {
            panic!()
        };
        assert!(matches!(payload, Some(Payload::Assign(_))));

        let StmtKind::ToolCall { verb, target, payload, .. } = kind(
            "http post \"https://hooks.example.com/alert\": { ticket: ticket.id, category: t.category }",
        ) else {
            panic!()
        };
        assert_eq!(verb.unwrap().name, "post");
        assert!(matches!(&target[0], TargetPart::Value(_)));
        assert!(
            matches!(payload, Some(Payload::Value(e)) if matches!(e.kind, ExprKind::Record(_)))
        );

        let StmtKind::ToolCall {
            tool,
            verb,
            payload,
            ..
        } = kind("log: \"Processed {customer}\"")
        else {
            panic!()
        };
        assert_eq!(tool.name, "log");
        assert!(verb.is_none() && payload.is_some());
    }

    #[test]
    fn two_assignments_in_one_payload() {
        let StmtKind::ToolCall { payload, .. } =
            kind("crm update lead: score = q.score, status = \"scored\"")
        else {
            panic!()
        };
        let Some(Payload::Assign(a)) = payload else {
            panic!()
        };
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn tool_call_with_result_and_no_target() {
        let StmtKind::ToolCall {
            target,
            payload,
            into,
            ..
        } = kind("pdf render doc -> bytes")
        else {
            panic!()
        };
        assert_eq!(target.len(), 1);
        assert!(payload.is_none());
        assert_eq!(into.unwrap().name, "bytes");
    }

    #[test]
    fn data_statements() {
        assert!(matches!(
            kind("set open_count = count of open invoices in crm.invoices"),
            StmtKind::Set { .. }
        ));
        assert!(matches!(
            kind("return invoice.amount - invoice.amount * vat / 100"),
            StmtKind::Return(_)
        ));
        assert!(matches!(
            kind("respond 200 { id: found.id }"),
            StmtKind::Respond {
                status: 200,
                body: Some(_)
            }
        ));
        assert!(matches!(
            kind("respond 404"),
            StmtKind::Respond {
                status: 404,
                body: None
            }
        ));
        let StmtKind::ReadBody { ty, into } = kind("read body as Invoice -> inv") else {
            panic!()
        };
        assert!(matches!(ty.kind, TypeKind::Named(n) if n == "Invoice"));
        assert_eq!(into.name, "inv");
        let Head::Try(s) = h("try read file path as list of Invoice -> rows") else {
            panic!()
        };
        let StmtKind::ReadFile { ty, .. } = s.kind else {
            panic!()
        };
        assert!(matches!(ty.kind, TypeKind::List(_)));
        assert!(matches!(
            kind("append inv to state.invoices"),
            StmtKind::Append { .. }
        ));
        assert!(matches!(
            kind("print: \"Imported {count of rows} rows\""),
            StmtKind::Print(_)
        ));
    }

    #[test]
    fn try_wraps_a_statement() {
        let Head::Try(s) = h("try http get \"https://api.example.com/rates\" -> rates") else {
            panic!()
        };
        assert!(matches!(s.kind, StmtKind::ToolCall { .. }));
    }

    #[test]
    fn statement_keywords_are_not_tool_names() {
        assert!(fails("set"));
        assert!(fails("guard: a else"));
        assert!(fails("if x is not"));
    }

    #[test]
    fn spans_point_into_the_file() {
        let p = head();
        let Ok(Head::Simple(s)) = run(&p, "stop", 12, 5) else {
            panic!()
        };
        assert_eq!(s.span, crate::diag::Span::new(12, 5, 4));
    }
}
