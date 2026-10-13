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
        "# Agent: a\n- model: claude\n\n## On start\n{line}\n"
    ))
}

fn heading(h: &str) -> String {
    render(&format!("# Agent: a\n- model: claude\n\n{h}\n- stop\n"))
}

#[test]
fn money_with_too_many_decimals() {
    assert_eq!(
        stmt("- set x = 12.505 EUR"),
        "t.agent.md:5\n  - set x = 12.505 EUR\n            ^^^^^^^^^^\n  EUR has 2 decimal places, so \"12.505 EUR\" cannot be written exactly. AgentMark never rounds money silently."
    );
}

#[test]
fn decimal_without_currency() {
    assert_eq!(
        stmt("- set x = 3.5"),
        "t.agent.md:5\n  - set x = 3.5\n            ^^^\n  Decimal numbers need a currency, like \"12.50 EUR\". AgentMark has no floating point numbers."
    );
}

#[test]
fn impossible_date() {
    assert_eq!(
        stmt("- set d = 2026-13-01"),
        "t.agent.md:5\n  - set d = 2026-13-01\n            ^^^^^^^^^^\n  \"2026-13-01\" is not a real date. Dates are written year-month-day, like 2026-10-13."
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
        stmt("- respond 999"),
        "t.agent.md:5\n  - respond 999\n            ^^^\n  Expected an HTTP status such as 200 or 404."
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
        stmt("- ask modle: \"hi\" -> x"),
        "t.agent.md:5\n  - ask modle: \"hi\" -> x\n        ^^^^^\n  Unknown word \"modle\" after \"ask\". The statement is \"ask model: ...\". Did you mean \"model\"?"
    );
    assert_eq!(
        stmt("- sett x = 1"),
        "t.agent.md:5\n  - sett x = 1\n    ^^^^\n  Unknown statement \"sett\". Did you mean \"set\"?"
    );
}

#[test]
fn misspelled_duration_units() {
    assert_eq!(
        heading("## Every 5 minuts"),
        "t.agent.md:4\n  ## Every 5 minuts\n             ^^^^^^\n  Unknown time unit \"minuts\". Did you mean \"minutes\"?"
    );
    assert_eq!(
        stmt("- set d = today + 5 minuts"),
        "t.agent.md:5\n  - set d = today + 5 minuts\n                      ^^^^^^\n  Unknown time unit \"minuts\". Did you mean \"minutes\"?"
    );
}

#[test]
fn misspelled_type_words() {
    assert_eq!(
        render("# Record: R\n- x: lst of int\n"),
        "t.agent.md:2\n  - x: lst of int\n       ^^^\n  Unknown type \"lst\". Did you mean \"list\"?"
    );
    assert_eq!(
        stmt("- read body as lst of int -> x"),
        "t.agent.md:5\n  - read body as lst of int -> x\n                 ^^^\n  Unknown type \"lst\". Did you mean \"list\"?"
    );
}

#[test]
fn a_second_value_after_a_complete_statement() {
    assert_eq!(
        stmt("- emial send: \"x\" \"y\""),
        "t.agent.md:5\n  - emial send: \"x\" \"y\"\n                    ^^^\n  I did not expect `\"y\"` here."
    );
}

#[test]
fn names_cannot_start_with_an_underscore() {
    assert_eq!(
        stmt("- set _x = 1"),
        "t.agent.md:5\n  - set _x = 1\n        ^^\n  Names must start with a letter, but \"_x\" starts with an underscore."
    );
}

#[test]
fn control_lines_need_their_colon() {
    let help =
        "A line that has statements under it ends with a colon, like \"for each row in rows:\".";
    assert_eq!(
        render("# Agent: a\n- model: claude\n\n## On start\nfor each x in y\n  - stop\n"),
        format!("t.agent.md:5\n  for each x in y\n                 ^\n  This line is missing the \":\" at its end.\n  {help}")
    );
    assert_eq!(
        stmt("- for each x in y"),
        format!("t.agent.md:5\n  - for each x in y\n                   ^\n  This line is missing the \":\" at its end.\n  {help}")
    );
    assert!(stmt("- if x = 1").contains("missing the \":\""));
}

#[test]
fn no_message_contains_internal_grammar_words() {
    let probes = [
        "- set x = 12.505 EUR",
        "- set x = 3.5",
        "- ask modle: \"x\" -> y",
        "- emial send: \"x\" \"y\"",
        "- guard: x is not emptyy",
        "- set x =",
        "- crm update invoice: = 3",
        "- if x is some:",
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
        stmt("- guard: x is not emptyy"),
        "t.agent.md:5\n  - guard: x is not emptyy\n                    ^^^^^^\n  Expected \"empty\" here, but found \"emptyy\". Did you mean \"empty\"?"
    );
    assert_eq!(
        stmt("- guard: x ="),
        "t.agent.md:5\n  - guard: x =\n              ^\n  The line ends too early: expected a value after \"=\"."
    );
    assert_eq!(
        stmt("- guard: x lenght < 3"),
        "t.agent.md:5\n  - guard: x lenght < 3\n             ^^^^^^\n  Expected \"length\" here, but found \"lenght\". Did you mean \"length\"?"
    );
}

#[test]
fn print_and_tool_calls_are_not_unknown_statements() {
    assert_eq!(
        stmt("- print: \"x\" y"),
        "t.agent.md:5\n  - print: \"x\" y\n               ^\n  I did not expect \"y\" here."
    );
    assert_eq!(
        stmt("- crm update invoice: status ="),
        "t.agent.md:5\n  - crm update invoice: status =\n                                ^\n  The line ends too early: expected a value after \"=\"."
    );
}

#[test]
fn misspelled_words_inside_statements() {
    assert_eq!(
        stmt("- if x is somee s:"),
        "t.agent.md:5\n  - if x is somee s:\n            ^^^^^\n  Expected \"some\" here, but found \"somee\". Did you mean \"some\"?"
    );
    assert_eq!(
        stmt("- append x too y"),
        "t.agent.md:5\n  - append x too y\n             ^^^\n  Expected \"to\" here, but found \"too\". Did you mean \"to\"?"
    );
    assert_eq!(
        stmt("- if a and:"),
        "t.agent.md:5\n  - if a and:\n            ^\n  Expected a value after \"and\", but found \":\"."
    );
}

#[test]
fn closed_word_properties_suggest() {
    assert_eq!(
        render("# Function: f\n- effect: pur\n"),
        "t.agent.md:2\n  - effect: pur\n            ^^^\n  Unknown effect \"pur\". Did you mean \"pure\"?"
    );
    assert_eq!(
        render("# Agent: a\n- model: claude\n- memory: fiel\n"),
        "t.agent.md:3\n  - memory: fiel\n            ^^^^\n  Unknown memory setting \"fiel\". Did you mean \"file\"?"
    );
}

#[test]
fn dates_must_exist_on_the_calendar() {
    assert!(stmt("- set d = 2026-02-30").contains("\"2026-02-30\" is not a real date"));
    assert!(stmt("- set d = 2026-04-31").contains("is not a real date"));
    assert!(stmt("- set d = 2025-02-29").contains("is not a real date"));
    let ok = |d: &str| {
        parse_file(
            "t.agent.md",
            &format!("# Agent: a\n- model: claude\n\n## On start\n- set d = {d}\n"),
        )
        .is_ok()
    };
    assert!(ok("2024-02-29") && ok("2000-02-29") && ok("2026-12-31"));
}

#[test]
fn diagnostics_are_sorted_by_position() {
    let src = "# Agent: a\n- modle: claude\n\n## On start\n- sett x = 1\n- set _y = 2\n# Record: R\n- x: lst of int\n";
    let diags = parse_file("t.agent.md", src).unwrap_err();
    let pos: Vec<(u32, u32)> = diags.iter().map(|d| (d.span.line, d.span.col)).collect();
    let mut sorted = pos.clone();
    sorted.sort();
    assert_eq!(pos, sorted);
    assert!(pos.len() >= 3);
}
