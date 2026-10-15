//! Regression tests for the wording of syntax errors (SPEC 10: plain language, a caret under the
//! offending word, a suggestion when one is within edit distance 2). Each test compares the whole
//! rendered message.

use agentmark::diag::render_all;
use agentmark::syntax::parser::parse_file;

fn render(src: &str) -> String {
    match parse_file("t.agent.md", src) {
        Ok(_) => panic!("expected a syntax error in:\n{src}"),
        Err(d) => render_all(&d, src),
    }
}

/// One statement inside `## On start` of a small agent (so the statement is on line 5).
fn stmt(line: &str) -> String {
    render(&format!(
        "# Agent: a\n- model: claude\n\n## On start\n1. {line}\n"
    ))
}

fn heading(h: &str) -> String {
    render(&format!("# Agent: a\n- model: claude\n\n{h}\n1. stop\n"))
}

#[test]
fn money_with_too_many_decimals() {
    assert_eq!(
        stmt("12.505 EUR into x"),
        "t.agent.md:5\n  1. 12.505 EUR into x\n     ^^^^^^^^^^\n  EUR has 2 decimal places, so \"12.505 EUR\" cannot be written exactly. AgentMark never rounds money silently."
    );
}

#[test]
fn decimal_without_currency() {
    assert_eq!(
        stmt("3.5 into x"),
        "t.agent.md:5\n  1. 3.5 into x\n     ^^^\n  Decimal numbers need a currency, like \"12.50 EUR\". AgentMark has no floating point numbers."
    );
}

#[test]
fn impossible_date() {
    assert_eq!(
        stmt("2026-13-01 into d"),
        "t.agent.md:5\n  1. 2026-13-01 into d\n     ^^^^^^^^^^\n  \"2026-13-01\" is not a real date. Dates are written year-month-day, like 2026-10-13."
    );
}

#[test]
fn impossible_time_in_a_heading() {
    assert_eq!(
        heading("## Every day at 25:00"),
        "t.agent.md:4\n  ## Every day at 25:00\n                  ^^^^^\n  A time looks like 09:00 (hours 00-23, minutes 00-59)."
    );
}

#[test]
fn http_status_message_is_a_sentence() {
    assert_eq!(
        stmt("respond 999"),
        "t.agent.md:5\n  1. respond 999\n             ^^^\n  Expected an HTTP status such as 200 or 404."
    );
}

#[test]
fn misspelled_trigger_words() {
    assert_eq!(
        heading("## Evry day at 09:00"),
        "t.agent.md:4\n  ## Evry day at 09:00\n     ^^^^\n  Unknown block heading \"Evry\". Did you mean \"Every\"?\n  A block starts with one of: Every, When, On, Do, Rust."
    );
    assert_eq!(
        heading("## Wehn crm receives lead"),
        "t.agent.md:4\n  ## Wehn crm receives lead\n     ^^^^\n  Unknown block heading \"Wehn\". Did you mean \"When\"?\n  A block starts with one of: Every, When, On, Do, Rust."
    );
}

#[test]
fn misspelled_statement_words() {
    assert_eq!(
        stmt("ask modle: \"hi\" into x"),
        "t.agent.md:5\n  1. ask modle: \"hi\" into x\n         ^^^^^\n  Unknown word \"modle\" after \"ask\". The statement is \"ask model: ...\". Did you mean \"model\"?"
    );
    assert_eq!(
        stmt("respnd 200 x"),
        "t.agent.md:5\n  1. respnd 200 x\n     ^^^^^^\n  Unknown statement \"respnd\". Did you mean \"respond\"?"
    );
}

#[test]
fn misspelled_duration_units() {
    assert_eq!(
        heading("## Every 5 minuts"),
        "t.agent.md:4\n  ## Every 5 minuts\n             ^^^^^^\n  Unknown time unit \"minuts\". Did you mean \"minutes\"?"
    );
    assert_eq!(
        stmt("today + 5 minuts into d"),
        "t.agent.md:5\n  1. today + 5 minuts into d\n               ^^^^^^\n  Unknown time unit \"minuts\". Did you mean \"minutes\"?"
    );
}

#[test]
fn misspelled_type_words() {
    assert_eq!(
        render("# Record: R\n- x: lst of int\n"),
        "t.agent.md:2\n  - x: lst of int\n       ^^^\n  Unknown type \"lst\". Did you mean \"list\"?"
    );
    assert_eq!(
        stmt("read body as lst of int into x"),
        "t.agent.md:5\n  1. read body as lst of int into x\n                  ^^^\n  Unknown type \"lst\". Did you mean \"list\"?"
    );
}

#[test]
fn a_second_value_after_a_complete_statement() {
    assert_eq!(
        stmt("emial send: \"x\" \"y\""),
        "t.agent.md:5\n  1. emial send: \"x\" \"y\"\n                     ^^^\n  I did not expect `\"y\"` here."
    );
}

#[test]
fn names_cannot_start_with_an_underscore() {
    assert_eq!(
        stmt("1 into _x"),
        "t.agent.md:5\n  1. 1 into _x\n            ^^\n  Names must start with a letter, but \"_x\" starts with an underscore."
    );
}

#[test]
fn control_lines_need_their_colon() {
    let help =
        "A line that has statements under it ends with a colon, like \"for each row in rows:\".";
    assert_eq!(
        render("# Agent: a\n- model: claude\n\n## On start\n1. for each x in y\n   1. stop\n"),
        format!("t.agent.md:5\n  1. for each x in y\n                    ^\n  This line is missing the \":\" at its end.\n  {help}")
    );
    assert_eq!(
        stmt("for each x in y"),
        format!("t.agent.md:5\n  1. for each x in y\n                    ^\n  This line is missing the \":\" at its end.\n  {help}")
    );
    assert!(stmt("if x = 1").contains("missing the \":\""));
}

#[test]
fn no_message_contains_internal_grammar_words() {
    let probes = [
        "12.505 EUR into x",
        "3.5 into x",
        "ask modle: \"x\" into y",
        "emial send: \"x\" \"y\"",
        "guard: x is not emptyy",
        "1 into",
        "crm update invoice: = 3",
        "if x is some:",
    ];
    for p in probes {
        let out = stmt(p);
        for bad in [
            "identifier",
            "inline whitespace",
            "whitespace",
            "or or",
            "\"\"",
            "found \"\"",
        ] {
            assert!(!out.contains(bad), "{p}: message contains {bad:?}:\n{out}");
        }
        assert!(
            out.lines()
                .nth(3)
                .unwrap()
                .trim_start()
                .chars()
                .next()
                .unwrap()
                .is_uppercase(),
            "{p}:\n{out}"
        );
    }
}

// ---- errors later in the line are not blamed on the first word

#[test]
fn guard_errors_point_at_the_real_problem() {
    assert_eq!(
        stmt("guard: x is not emptyy"),
        "t.agent.md:5\n  1. guard: x is not emptyy\n                     ^^^^^^\n  Expected \"empty\" here, but found \"emptyy\". Did you mean \"empty\"?"
    );
    assert_eq!(
        stmt("guard: x ="),
        "t.agent.md:5\n  1. guard: x =\n               ^\n  The line ends too early: expected a value after \"=\"."
    );
    assert_eq!(
        stmt("guard: x lenght < 3"),
        "t.agent.md:5\n  1. guard: x lenght < 3\n              ^^^^^^\n  Expected \"length\" here, but found \"lenght\". Did you mean \"length\"?"
    );
}

#[test]
fn print_and_tool_calls_are_not_unknown_statements() {
    assert_eq!(
        stmt("print: \"x\" y"),
        "t.agent.md:5\n  1. print: \"x\" y\n                ^\n  I did not expect \"y\" here."
    );
    assert_eq!(
        stmt("crm update invoice: status ="),
        "t.agent.md:5\n  1. crm update invoice: status =\n                                 ^\n  The line ends too early: expected a value after \"=\"."
    );
}

#[test]
fn misspelled_words_inside_statements() {
    assert_eq!(
        stmt("if x is somee s:"),
        "t.agent.md:5\n  1. if x is somee s:\n             ^^^^^\n  Expected \"some\" here, but found \"somee\". Did you mean \"some\"?"
    );
    assert_eq!(
        stmt("add x too y"),
        "t.agent.md:5\n  1. add x too y\n           ^^^\n  Expected \"to\" here, but found \"too\". Did you mean \"to\"?"
    );
    assert_eq!(
        stmt("if a and:"),
        "t.agent.md:5\n  1. if a and:\n             ^\n  Expected a value after \"and\", but found \":\"."
    );
}

#[test]
fn closed_word_properties_suggest() {
    assert_eq!(
        render("# Agent: a\n- model: claude\n- memory: fiel\n"),
        "t.agent.md:3\n  - memory: fiel\n            ^^^^\n  Unknown memory setting \"fiel\". Did you mean \"file\"?"
    );
}

#[test]
fn dates_must_exist_on_the_calendar() {
    assert!(stmt("2026-02-30 into d").contains("\"2026-02-30\" is not a real date"));
    assert!(stmt("2026-04-31 into d").contains("is not a real date"));
    assert!(stmt("2025-02-29 into d").contains("is not a real date"));
    let ok = |d: &str| {
        parse_file(
            "t.agent.md",
            &format!("# Agent: a\n- model: claude\n\n## On start\n1. {d} into d\n"),
        )
        .is_ok()
    };
    assert!(ok("2024-02-29") && ok("2000-02-29") && ok("2026-12-31"));
}

#[test]
fn diagnostics_are_sorted_by_position() {
    let src = "# Agent: a\n- modle: claude\n\n## On start\n1. sett x = 1\n2. 2 into _y\n\n# Record: R\n- x: lst of int\n";
    let diags = parse_file("t.agent.md", src).unwrap_err();
    let pos: Vec<(u32, u32)> = diags.iter().map(|d| (d.span.line, d.span.col)).collect();
    let mut sorted = pos.clone();
    sorted.sort();
    assert_eq!(pos, sorted);
    assert!(pos.len() >= 3);
}

#[test]
fn a_tool_statement_inside_a_condition_is_not_a_typo_of_and() {
    // `send` is within edit distance 2 of `and`, but suggesting the word that stands right before is
    // nonsense: say what is wrong instead
    let e = stmt("guard: status != \"paid\" and email send customer");
    assert!(e.contains("I did not expect \"send\" here: \"email send\" looks like a statement"), "{e}");
    assert!(!e.contains("Did you mean"), "{e}");
    assert!(!e.contains("Expected \"and\""), "{e}");
}

#[test]
fn typos_of_and_or_still_get_did_you_mean() {
    for (src, want) in [
        ("guard: ready adn done", "and"),
        ("guard: ready nad done", "and"),
        ("guard: ready an done", "and"),
        ("guard: a or b orr c", "or"),
    ] {
        let e = stmt(src);
        assert!(e.contains(&format!("Did you mean \"{want}\"?")), "{src}:\n{e}");
        assert!(!e.contains("looks like a statement"), "{src}:\n{e}");
    }
    assert!(stmt("guard: status != \"paid\" and email send customer").contains("looks like a statement"));
}
