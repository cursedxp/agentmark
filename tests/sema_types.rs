//! Type rules (SPEC 2.1, 5, 6): field types and inference, units, `maybe`, calls, returns.

mod common;
use common::*;

use agentmark::sema::Type;

const INVOICE: &str =
    "# Record: Invoice\n- id: text\n- amount: money\n- due: date\n- status: text\n\n";

fn rule(when: &str) -> String {
    format!("{INVOICE}# Rule: r\n- for: Invoice\n- when: {when}\n")
}

fn func(out: &str, body: &str) -> String {
    format!("{INVOICE}# Function: f\n- in: invoice: Invoice, n: int, tag: maybe text\n- out: {out}\n- effect: pure\n  {body}\n")
}

// ------------------------------------------------------------------ record fields (2.1)

#[test]
fn explicit_field_types_are_resolved() {
    let c = ok("# Record: A\n- a: text\n- b: list of int\n- c: maybe money\n- d: bytes\n");
    let f = &c.records["A"].fields;
    assert_eq!(f[0].ty, Type::Text);
    assert_eq!(f[1].ty.to_string(), "list of int");
    assert_eq!(f[2].ty.to_string(), "maybe money");
    assert_eq!(f[3].ty, Type::Bytes);
    assert!(f.iter().all(|x| !x.inferred));
}

#[test]
fn field_types_are_inferred_from_the_name_without_a_warning() {
    let src = "# Record: A\n- due_date\n- created_at\n- paid_on\n- total\n- unit_price\n- retry_count\n- qty\n";
    let c = ok(src);
    let tys: Vec<String> = c.records["A"]
        .fields
        .iter()
        .map(|f| f.ty.to_string())
        .collect();
    assert_eq!(
        tys,
        ["date", "date", "date", "money", "money", "int", "int"]
    );
    assert!(c.records["A"].fields.iter().all(|f| f.inferred));
    assert!(
        c.warnings.iter().all(|w| w.code != "W001"),
        "{:?}",
        c.warnings
    );
}

#[test]
fn inference_failure_is_a_warning_that_names_the_inferred_type() {
    let w = warnings("# Record: A\n- customer\n");
    assert!(
        w.contains(
            "t.agent.md:2\n  - customer\n    ^^^^^^^^\n  The field \"customer\" has no type and its name gives no hint, so it is inferred as text."
        ),
        "{w}"
    );
    assert!(w.contains("Write the type explicitly"), "{w}");
}

#[test]
fn inference_failure_does_not_fail_the_check() {
    let c = ok("# Record: A\n- reason\n");
    assert_eq!(c.records["A"].fields[0].ty, Type::Text);
    assert_eq!(c.warnings.iter().filter(|w| w.code == "W001").count(), 1);
}

#[test]
fn unknown_field_type_suggests_a_close_one() {
    let e = first_error("# Record: A\n- a: mony\n");
    assert!(
        e.contains("Unknown type \"mony\". Did you mean \"money\"?"),
        "{e}"
    );
    let e = first_error("# Record: B\n- id: text\n\n# Record: A\n- b: Bb\n");
    assert!(
        e.contains("Unknown type \"Bb\". Did you mean \"B\"?"),
        "{e}"
    );
}

#[test]
fn duplicate_field_names_are_an_error() {
    let e = first_error("# Record: A\n- a: text\n- a: int\n");
    assert!(
        e.contains("The record \"A\" has the field \"a\" twice."),
        "{e}"
    );
}

// ------------------------------------------------------------------ units (5)

#[test]
fn date_plus_duration_is_a_date_and_money_arithmetic_is_money() {
    let c = ok(&rule(
        "today > due + 14 days and amount * 2 - amount / 3 > 5 EUR",
    ));
    assert!(c
        .blocks
        .iter()
        .any(|b| b.label == "r" && b.effect.is_pure()));
}

#[test]
fn money_plus_duration_is_an_error_with_a_caret_under_the_whole_sum() {
    let e = errors(&rule("amount + 3 days > 5 EUR"));
    assert_eq!(
        e,
        "t.agent.md:9\n  - when: amount + 3 days > 5 EUR\n          ^^^^^^^^^^^^^^^\n  Cannot add a duration to money.\n  Money only combines with money; durations combine with dates and other durations."
    );
}

#[test]
fn subtracting_unlike_units() {
    let e = first_error(&rule("amount - 3 days > 5 EUR"));
    assert!(e.contains("Cannot subtract a duration from money."), "{e}");
    let e = first_error(&rule("due - amount > 3 days"));
    assert!(e.contains("Cannot subtract money from a date."), "{e}");
}

#[test]
fn money_times_money_and_money_by_text() {
    let e = first_error(&rule("amount * amount > 1 EUR"));
    assert!(e.contains("Cannot multiply money by money."), "{e}");
    assert!(
        e.contains("Money can only be multiplied or divided by a whole number."),
        "{e}"
    );
    let e = first_error(&rule("amount / \"x\" > 1 EUR"));
    assert!(e.contains("Cannot divide money by text."), "{e}");
}

#[test]
fn comparing_unlike_things() {
    let e = first_error(&rule("amount > 3"));
    assert!(e.contains("Cannot compare money with a number."), "{e}");
    let e = first_error(&rule("due = status"));
    assert!(e.contains("Cannot compare a date with text."), "{e}");
}

#[test]
fn money_in_two_written_currencies() {
    let e = first_error(&rule("amount > 3 EUR + 4 USD"));
    assert!(e.contains("Cannot add USD money and EUR money."), "{e}");
    assert!(e.contains("explicit conversion"), "{e}");
}

#[test]
fn date_minus_date_is_a_duration() {
    let c = ok(&rule("today - due > 14 days"));
    assert!(c.warnings.iter().all(|w| w.code != "E203"));
}

#[test]
fn and_or_not_need_yes_no_values() {
    let e = first_error(&rule("amount and status = \"x\""));
    assert!(
        e.contains("\"and\" needs a yes/no condition, but this is money."),
        "{e}"
    );
    let e = first_error(&rule("not amount"));
    assert!(e.contains("\"not\" needs a yes/no condition"), "{e}");
}

#[test]
fn a_rule_condition_must_be_yes_no() {
    let e = first_error(&rule("amount"));
    assert!(
        e.contains("A rule's \"when:\" must be a yes/no condition, but this is money."),
        "{e}"
    );
}

// ------------------------------------------------------------------ names and fields

#[test]
fn unknown_field_in_a_rule_suggests_the_field() {
    let e = errors(&rule("amout > 100 EUR"));
    assert_eq!(
        e,
        "t.agent.md:9\n  - when: amout > 100 EUR\n          ^^^^^\n  Unknown field \"amout\" on Invoice. Did you mean \"amount\"?"
    );
}

#[test]
fn unknown_field_after_a_dot() {
    let e = first_error(&func("text", "- return invoice.statsu"));
    assert!(
        e.contains("Unknown field \"statsu\" on Invoice. Did you mean \"status\"?"),
        "{e}"
    );
}

#[test]
fn field_access_on_a_non_record() {
    let e = first_error(&func("text", "- return n.x"));
    assert!(
        e.contains("\"n\" is a number, which has no fields, so \".x\" cannot be read."),
        "{e}"
    );
}

#[test]
fn unknown_name_lists_close_names() {
    let e = first_error(&func("int", "- return nn"));
    assert!(
        e.contains("Unknown name \"nn\". Did you mean \"n\"?"),
        "{e}"
    );
}

#[test]
fn the_rule_record_is_also_available_by_name() {
    ok(&rule("invoice.amount > 5 EUR and amount > 1 EUR"));
}

// ------------------------------------------------------------------ maybe (6)

#[test]
fn returning_first_where_from_a_non_maybe_function() {
    let e = errors("# Record: Invoice\n- id: text\n- status: text\n\n# Function: find_invoice\n- in: invoices: list of Invoice, wanted: text\n- out: Invoice\n- effect: pure\n  - return first invoice in invoices where invoice.id = wanted\n");
    assert!(
        e.contains("This can find nothing, but the function says it returns \"Invoice\"."),
        "{e}"
    );
}

#[test]
fn returning_first_where_from_a_maybe_function_is_fine() {
    ok("# Record: Invoice\n- id: text\n\n# Function: find_invoice\n- in: invoices: list of Invoice, wanted: text\n- out: maybe Invoice\n- effect: pure\n  - return first invoice in invoices where invoice.id = wanted\n");
}

#[test]
fn a_maybe_field_cannot_be_used_in_arithmetic_or_comparisons() {
    let src = "# Record: I\n- id: text\n- s: maybe int\n\n# Function: f\n- in: i: I\n- out: int\n- effect: pure\n  - return i.s + 1\n";
    let e = first_error(src);
    assert!(
        e.contains("This value can be empty (maybe int), so it cannot be used in arithmetic yet."),
        "{e}"
    );
    let src = "# Record: I\n- id: text\n- s: maybe int\n\n# Rule: r\n- for: I\n- when: s > 3\n";
    let e = first_error(src);
    assert!(e.contains("so it cannot be compared yet."), "{e}");
}

#[test]
fn a_maybe_field_can_be_checked_with_is_not_empty_in_a_condition() {
    ok("# Record: I\n- id: text\n- s: maybe int\n\n# Rule: r\n- for: I\n- when: s is not empty and s > 3\n");
}

#[test]
fn if_some_unwraps() {
    ok("# Record: I\n- id: text\n- s: maybe int\n\n# Function: f\n- in: i: I\n- out: int\n- effect: pure\n  - if i.s is some v:\n    - return v + 1\n  - return 0\n");
}

#[test]
fn guard_not_empty_unwraps_for_the_rest_of_the_block() {
    ok("# Record: I\n- id: text\n\n# Function: f\n- in: items: list of I, w: text\n- out: text\n- effect: pure\n  - set found = first x in items where x.id = w\n  - guard: found is not empty\n  - return found.id\n");
}

#[test]
fn a_maybe_record_has_no_fields_until_checked() {
    let e = first_error("# Record: I\n- id: text\n\n# Function: f\n- in: items: list of I, w: text\n- out: text\n- effect: pure\n  - set found = first x in items where x.id = w\n  - return found.id\n");
    assert!(
        e.contains("\"found\" can be empty here, so \"id\" cannot be read from it yet."),
        "{e}"
    );
    assert!(e.contains("guard: found is not empty"), "{e}");
}

#[test]
fn assigning_again_forgets_what_was_checked() {
    let e = first_error("# Record: I\n- id: text\n\n# Function: f\n- in: items: list of I, w: text\n- out: text\n- effect: pure\n  - set found = first x in items where x.id = w\n  - guard: found is not empty\n  - set found = first x in items where x.id = \"b\"\n  - return found.id\n");
    assert!(e.contains("\"found\" can be empty here"), "{e}");
}

#[test]
fn a_maybe_value_in_text_is_an_error() {
    let src = "# Record: I\n- id: text\n- s: maybe text\n\n# Function: f\n- in: i: I\n- out: text\n- effect: pure\n  - return \"s is {i.s}\"\n";
    let e = first_error(src);
    assert!(e.contains("so it cannot be written into text yet."), "{e}");
}

#[test]
fn is_some_on_a_plain_value() {
    let e = first_error(&func(
        "int",
        "- if n is some v:\n    - return v\n  - return 0",
    ));
    assert!(
        e.contains("is never empty, so \"is some\" is not needed."),
        "{e}"
    );
}

// ------------------------------------------------------------------ calls and returns

#[test]
fn return_type_mismatch() {
    let e = first_error(&func("int", "- return invoice.status"));
    assert!(
        e.contains("This returns text, but the function says it returns \"int\"."),
        "{e}"
    );
}

#[test]
fn a_plain_value_fits_a_maybe_result() {
    ok(&func("maybe int", "- return n"));
}

#[test]
fn call_arity_and_argument_types() {
    let src = format!(
        "{}\n# Function: g\n- out: int\n- effect: pure\n  - return f(1)\n",
        func("int", "- return n")
    );
    let e = first_error(&src);
    assert!(
        e.contains("\"f\" takes 3 values (invoice, n, tag), but 1 was given."),
        "{e}"
    );
    let src = format!("{}\n# Function: g\n- in: i: Invoice\n- out: int\n- effect: pure\n  - return f(i, \"x\", \"t\")\n", func("int", "- return n"));
    let e = first_error(&src);
    assert!(
        e.contains("\"n\" of \"f\" must be int, but this is text."),
        "{e}"
    );
}

#[test]
fn unknown_function_suggests() {
    let src = format!(
        "{}\n# Function: g\n- out: int\n- effect: pure\n  - return ff(1)\n",
        func("int", "- return n")
    );
    let e = first_error(&src);
    assert!(
        e.contains("Unknown function \"ff\". Did you mean \"f\"?"),
        "{e}"
    );
}

#[test]
fn return_outside_a_function_and_stop_inside_one() {
    let e = first_error("# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- return 1\n");
    assert!(
        e.contains("\"return\" only works inside a function."),
        "{e}"
    );
    let e = first_error(&func("int", "- stop\n  - return n"));
    assert!(
        e.contains("\"stop\" ends a block, but this is a function."),
        "{e}"
    );
}

#[test]
fn a_function_with_out_must_return() {
    let e = first_error("# Function: f\n- out: int\n- effect: pure\n  - set x = 1\n");
    assert!(
        e.contains("The function \"f\" says it returns a value but never does."),
        "{e}"
    );
}

#[test]
fn set_keeps_the_type_of_a_variable() {
    let e = first_error(&func("int", "- set x = 1\n  - set x = \"a\"\n  - return x"));
    assert!(e.contains("\"x\" holds int, but this is text."), "{e}");
}

#[test]
fn expression_types_are_recorded_for_later_passes() {
    let src = rule("today > due + 14 days");
    let c = ok(&src);
    let tys: Vec<String> = c.expr_types.values().map(|t| t.to_string()).collect();
    assert!(
        tys.contains(&"bool".to_string()) && tys.contains(&"date".to_string()),
        "{tys:?}"
    );
    assert!(tys.contains(&"duration".to_string()));
}

#[test]
fn constants_have_types() {
    let c = ok("# Constant: grace\n- value: 14 days\n\n# Constant: limit\n- value: 800\n\n# Constant: fee\n- value: 12.50 EUR\n\n# Constant: both\n- value: fee + fee\n");
    assert_eq!(c.constants["grace"].ty, Type::Duration);
    assert_eq!(c.constants["limit"].ty, Type::Int);
    assert_eq!(c.constants["fee"].ty, Type::Money);
    assert_eq!(c.constants["both"].ty, Type::Money);
}

#[test]
fn a_constant_defined_in_terms_of_itself() {
    let e = first_error("# Constant: a\n- value: b\n\n# Constant: b\n- value: a\n");
    assert!(e.contains("is defined in terms of itself"), "{e}");
}

#[test]
fn record_literals_and_for_each_over_non_lists() {
    let e = first_error("# Agent: a\n- model: claude\n- tools: log\n\n## On start\nfor each x in 5:\n  - log: \"x\"\n");
    assert!(
        e.contains("\"for each\" needs a list, but this is a number."),
        "{e}"
    );
}

#[test]
fn rule_filters_check_the_record() {
    let src = "# Record: Invoice\n- id: text\n\n# Record: Lead\n- id: text\n\n# Rule: r\n- for: Lead\n- when: id = \"a\"\n\n# Agent: a\n- model: claude\n- tools: crm, log\n\n## On start\nfor each r invoice in crm.invoices:\n  - log: \"x\"\n";
    let e = first_error(src);
    assert!(
        e.contains("The rule \"r\" is about Lead, but the list holds Invoice."),
        "{e}"
    );
}

#[test]
fn unknown_rule_suggests() {
    let src = "# Record: Invoice\n- id: text\n\n# Rule: overdue\n- for: Invoice\n- when: id = \"a\"\n\n# Agent: a\n- model: claude\n- tools: crm, log\n\n## On start\nfor each overdu invoice in crm.invoices:\n  - log: \"x\"\n";
    let e = first_error(src);
    assert!(
        e.contains("Unknown rule \"overdu\". Did you mean \"overdue\"?"),
        "{e}"
    );
}

#[test]
fn model_answers_are_text_or_the_named_record() {
    let src = "# Record: Q\n- score: int\n\n# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- ask model: \"hi\" -> t\n- ask model: \"hi\" as Q -> q\n- guard: q matches Q\n- log: \"{t} {q.score}\"\n";
    ok(src);
    let bad = "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- ask model: \"hi\" as Qq -> q\n";
    let e = first_error(bad);
    assert!(e.contains("Unknown type \"Qq\""), "{e}");
}
