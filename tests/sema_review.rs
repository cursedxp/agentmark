//! Regressions and interface checks from the step-3 review: name resolution order, partial record
//! literals, the resolution tables later passes consume, inference, payload types.

mod common;
use common::*;

use agentmark::project::Project;
use agentmark::sema::{self, CallKind, Resolution, ToolKind, Type};

// ------------------------------------------------------------------ names versus tools

const LEAD: &str = "# Record: Lead\n- id: text\n- email: text\n\n";

#[test]
fn a_field_named_like_a_tool_is_a_field_in_a_rule() {
    let c = ok(&format!("{LEAD}# Rule: r\n- for: Lead\n- when: email = \"x\"\n"));
    assert!(c.resolutions.values().any(|r| matches!(
        r,
        Resolution::Field { record, field, .. } if record == "Lead" && field == "email"
    )));
}

#[test]
fn a_field_named_like_a_tool_is_a_field_in_a_loop() {
    let src = format!("{LEAD}# Agent: a\n- model: claude\n- tools: crm, email, log\n\n## On start\nfor each l in crm.leads:\n  - log: \"{{email}} {{l.email}}\"\n");
    ok(&src);
}

#[test]
fn a_tool_source_still_needs_the_dot_and_a_bare_tool_is_not_a_value() {
    let src = format!("{LEAD}# Agent: a\n- model: claude\n- tools: crm, log\n\n## On start\nfor each l in crm.leads:\n  - log: \"x\"\n  - set n = crm\n");
    let e = first_error(&src);
    assert!(e.contains("\"crm\" is a tool, not a value."), "{e}");
}

#[test]
fn a_variable_wins_over_a_field_and_a_constant_loses_to_a_field() {
    let src = format!("{LEAD}# Constant: id\n- value: 1\n\n# Function: f\n- in: l: Lead, email: int\n- out: int\n- effect: pure\n  - return email\n\n# Rule: r\n- for: Lead\n- when: id = \"x\"\n");
    ok(&src);
}

// ------------------------------------------------------------------ record literals

const INV: &str = "# Record: Invoice\n- id: text\n- amount: money\n- note: maybe text\n\n";

fn ret(lit: &str) -> String {
    format!("{INV}# Function: f\n- out: Invoice\n- effect: pure\n  - return {lit}\n")
}

#[test]
fn a_record_literal_must_give_every_field_that_is_not_maybe() {
    ok(&ret("{ id: \"a\", amount: 1 EUR }"));
    ok(&ret("{ id: \"a\", amount: 1 EUR, note: \"n\" }"));
    let e = first_error(&ret("{ id: \"a\" }"));
    assert!(e.contains("Missing: amount."), "{e}");
}

#[test]
fn misspelled_keys_get_a_suggestion() {
    let e = first_error(&ret("{ idd: \"a\", amount: 1 EUR }"));
    assert!(e.contains("Invoice has no field \"idd\". Did you mean \"id\"?"), "{e}");
    assert!(e.contains("Missing: id."), "{e}");
}

#[test]
fn partial_literals_are_rejected_in_payloads_and_arguments() {
    let src = format!("{INV}# Record: Lead\n- id: text\n\n# Agent: a\n- model: claude\n- tools: crm, log\n\n## On start\n- crm update invoice: {{ id: \"x\" }}\n");
    let e = errors(&src);
    assert!(e.contains("The tool crm takes a Invoice here"), "{e}");
    assert!(e.contains("Missing: amount."), "{e}");
}

// ------------------------------------------------------------------ resolution tables

fn example_01() -> sema::Checked {
    sema::check(&Project::load("examples/01-invoice-chaser")).unwrap()
}

#[test]
fn resolutions_say_what_every_name_stands_for() {
    let c = example_01();
    let has = |f: &dyn Fn(&Resolution) -> bool| c.resolutions.values().any(f);
    assert!(has(&|r| matches!(r, Resolution::Call { kind: CallKind::DoBlock, decl } if decl == "remind")));
    assert!(has(&|r| matches!(r, Resolution::Call { kind: CallKind::Rule, decl } if decl == "overdue")));
    assert!(has(&|r| matches!(r, Resolution::Source { tool, source, record }
        if tool == "crm" && source == "invoices" && record.as_deref() == Some("Invoice"))));
    assert!(has(&|r| matches!(r, Resolution::Const { name } if name == "max_reminder_length")));
    assert!(has(&|r| matches!(r, Resolution::Const { name } if name == "grace_period")));
    assert!(has(&|r| matches!(r, Resolution::Field { var: Some(v), record, field }
        if v == "invoice" && record == "Invoice" && field == "customer")));
    assert!(has(&|r| matches!(r, Resolution::Var { name, .. } if name == "reminder")));
}

#[test]
fn tool_calls_say_which_variable_and_field_they_read() {
    let c = example_01();
    let email = c.tool_calls.values().find(|t| t.tool == "email").unwrap();
    assert_eq!(email.kind, ToolKind::Manifest);
    assert_eq!(email.verb.as_deref(), Some("send"));
    assert_eq!(email.target_word.as_deref(), Some("customer"));
    assert_eq!(email.record.as_deref(), Some("Invoice"));
    assert_eq!(email.var.as_deref(), Some("invoice"));
    // Q4: customer_email wins over customer
    assert_eq!(email.field.as_deref(), Some("customer_email"));
    let crm = c.tool_calls.values().find(|t| t.tool == "crm").unwrap();
    assert_eq!(crm.record.as_deref(), Some("Invoice"));
    assert_eq!(crm.var.as_deref(), Some("invoice"));
    assert_eq!(crm.field.as_deref(), Some("id"));
    let log = c.tool_calls.values().find(|t| t.tool == "log").unwrap();
    assert_eq!(log.target_word, None);
}

#[test]
fn bindings_are_resolved_as_bindings() {
    let main = "# Import: crate printpdf\n\n# Function: make\n- in: t: text\n- out: bytes\n- effect: local\n  - pdf render t -> b\n  - return b\n";
    let binding = "# Binding: pdf\n- crate: printpdf\n- effect: local\n\n## render(title: text) -> bytes\n  ```rust\n  Ok(Vec::new())\n  ```\n";
    let p = Project::from_memory("app/main.agent.md", vec![("app/main.agent.md", main), ("app/bindings/printpdf.agent.md", binding)]);
    let c = sema::check(&p).unwrap_or_else(|d| panic!("{}", p.render(&d)));
    let t = c.tool_calls.values().next().unwrap();
    assert_eq!((t.tool.as_str(), t.kind), ("pdf", ToolKind::Binding));
    assert!(c.resolutions.values().any(
        |r| matches!(r, Resolution::Call { kind: CallKind::Binding, decl } if decl == "pdf")
    ));
}

#[test]
fn calls_are_resolved_to_declared_names_even_through_an_alias() {
    let main = "# Import: ./billing as billing\n\n# Function: f\n- out: int\n- effect: pure\n  - return billing.five()\n";
    let fns = "# Function: five\n- out: int\n- effect: pure\n  - return 5\n";
    let p = Project::from_memory("app/main.agent.md", vec![("app/main.agent.md", main), ("app/billing/fns.agent.md", fns)]);
    let c = sema::check(&p).unwrap_or_else(|d| panic!("{}", p.render(&d)));
    assert!(c.resolutions.values().any(|r| matches!(r, Resolution::Call { kind: CallKind::Function, decl } if decl == "five")));
}

#[test]
fn unwraps_say_where_a_maybe_value_is_read_as_present() {
    let c = sema::check(&Project::load("examples/05-invoice-service.agent.md")).unwrap();
    // `found.id` after `guard: found is not empty` unwraps segment 0; `respond 200 found` too
    assert!(c.unwraps.values().any(|v| v == &vec![0]), "{:?}", c.unwraps);
    let src = "# Record: I\n- id: text\n- s: maybe int\n\n# Function: f\n- in: i: I\n- out: int\n- effect: pure\n  - if i.s is some v:\n    - return i.s\n  - return 0\n";
    let c = ok(src);
    assert!(c.unwraps.values().any(|v| v == &vec![1]), "{:?}", c.unwraps);
}

#[test]
fn every_expression_has_a_type_and_an_effect() {
    let c = example_01();
    assert!(!c.expr_types.is_empty());
    for k in c.expr_types.keys() {
        assert!(c.expr_effects.contains_key(k), "{k:?}");
    }
    let crm_reads: Vec<_> = c
        .expr_effects
        .values()
        .filter(|e| e.to_string() == "network(crm)")
        .collect();
    assert_eq!(crm_reads.len(), 1);
}

#[test]
fn reading_state_is_pure_and_appending_is_local_state() {
    let c = sema::check(&Project::load("examples/05-invoice-service.agent.md")).unwrap();
    let get = c.blocks.iter().find(|b| b.label == "GET /invoices/{id}").unwrap();
    assert!(get.effect.is_pure());
    let post = c.blocks.iter().find(|b| b.label == "POST /invoices").unwrap();
    assert_eq!(post.effect.to_string(), "local(state)");
}

// ------------------------------------------------------------------ inference

#[test]
fn camel_case_names_are_inferred_and_listed() {
    let src = "# Record: A\n- createdAt\n- totalAmount\n- retryCount\n- notes\n- dueDate: text\n";
    let c = ok(src);
    let tys: Vec<String> = c.records["A"].fields.iter().map(|f| f.ty.to_string()).collect();
    assert_eq!(tys, ["date", "money", "int", "text", "text"]);
    let listed: Vec<(&str, bool)> = c.inferred_fields.iter().map(|f| (f.field.as_str(), f.fallback)).collect();
    assert_eq!(listed, [("createdAt", false), ("totalAmount", false), ("retryCount", false), ("notes", true)]);
    assert_eq!(c.warnings.iter().filter(|w| w.code == "W001").count(), 1);
}

// ------------------------------------------------------------------ Do blocks, payloads, services

#[test]
fn do_callers_may_use_different_variable_names_for_the_same_record() {
    let src = "# Record: Invoice\n- id: text\n\n# Agent: a\n- model: claude\n- tools: crm, log\n\n## Every 5 minutes\nfor each i in crm.invoices:\n  - do show\nfor each j in crm.invoices:\n  - do show\n\n## Do show\n- log: \"{id}\"\n";
    let c = ok(src);
    let b = c.blocks.iter().find(|b| b.label == "Do show").unwrap();
    assert_eq!(b.param, Some(("i".to_string(), "Invoice".to_string())));
}

#[test]
fn text_tools_need_text_payloads() {
    let src = "# Agent: a\n- model: claude\n- tools: email, log\n\n## On start\n- email send address \"a@b.c\": 5\n";
    let e = first_error(src);
    assert!(e.contains("The tool email takes text here, but this is a number."), "{e}");
    let e = first_error("# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- log: 5\n");
    assert!(e.contains("The tool log takes text here"), "{e}");
}

#[test]
fn binding_arguments_are_type_checked() {
    let binding = "# Binding: pdf\n- crate: printpdf\n- effect: local\n\n## render(title: text) -> bytes\n  ```rust\n  Ok(Vec::new())\n  ```\n";
    let main = "# Import: crate printpdf\n\n# Function: make\n- in: n: int\n- out: bytes\n- effect: local\n  - pdf render n -> b\n  - return b\n";
    let p = Project::from_memory("app/main.agent.md", vec![("app/main.agent.md", main), ("app/bindings/printpdf.agent.md", binding)]);
    let d = sema::check(&p).unwrap_err();
    let out = p.render(&d.into_iter().filter(|x| x.severity == agentmark::diag::Severity::Error).collect::<Vec<_>>());
    assert!(out.contains("\"title\" of \"pdf render\" must be text, but this is a number."), "{out}");
}

#[test]
fn only_one_service_may_have_a_state() {
    let src = "# Record: S\n- n: int\n\n# Service: a\n- state: S\n\n## GET /x\n- respond 200 state\n\n# Service: b\n- state: S\n\n## GET /y\n- respond 200 state\n";
    let e = first_error(src);
    assert!(e.contains("Only one service may have a \"state:\" (\"a\" in t.agent.md:5 has one)"), "{e}");
}

#[test]
fn types_still_display_for_literals() {
    assert_eq!(Type::Object(vec![("a".into(), Type::Text)]).to_string(), "{ a: text }");
}

// ------------------------------------------------------------------ tool targets (round 2)

const REC: &str = "# Record: Lead\n- id: text\n- score: maybe int\n\n# Record: Invoice\n- id: text\n- status: text\n\n";

fn agent(body: &str) -> String {
    format!("{REC}# Agent: a\n- model: claude\n- tools: email, crm, log\n\n## On start\n{body}")
}

#[test]
fn email_customer_needs_a_record_in_front_of_the_block() {
    let e = first_error(&agent("- email send customer: \"x\"\n"));
    assert!(
        e.contains("\"customer\" needs a record in front of this block with a \"customer_email\" or \"customer\" field."),
        "{e}"
    );
    assert!(e.contains("address"), "{e}");
}

#[test]
fn email_customer_needs_a_field_on_that_record() {
    let e = first_error(&agent("for each l in crm.leads:\n  - email send customer: \"x\"\n"));
    assert!(e.contains("\"customer\" needs a record in front of this block"), "{e}");
    // a record with only `customer` works, and `customer_email` wins when both exist
    let src = "# Record: Lead\n- id: text\n- customer: text\n\n# Agent: a\n- model: claude\n- tools: email, crm\n\n## On start\nfor each l in crm.leads:\n  - email send customer: \"x\"\n";
    let c = ok(src);
    assert_eq!(c.tool_calls.values().next().unwrap().field.as_deref(), Some("customer"));
}

#[test]
fn a_literal_address_needs_no_record() {
    ok(&agent("- email send address \"a@b.c\": \"x\"\n"));
}

#[test]
fn a_crm_target_with_two_candidate_variables_is_ambiguous() {
    let e = first_error(&agent(
        "- crm get invoice \"x\" -> b\n- crm get invoice \"y\" -> a2\n- crm update invoice: status = \"z\"\n",
    ));
    assert!(e.contains("Which invoice? \"a2\" or \"b\" are both in scope."), "{e}");
}

#[test]
fn a_crm_target_uses_the_one_matching_variable_or_the_one_in_front() {
    let c = ok(&agent("- crm get invoice \"x\" -> b\n- crm update invoice: status = \"z\"\n"));
    let upd = c.tool_calls.values().find(|t| t.verb.as_deref() == Some("update")).unwrap();
    assert_eq!((upd.var.as_deref(), upd.field.as_deref()), (Some("b"), Some("id")));
    // an explicit target value means no variable is looked up
    let get = c.tool_calls.values().find(|t| t.verb.as_deref() == Some("get")).unwrap();
    assert_eq!((get.var.as_deref(), get.field.as_deref()), (None, None));
    // the loop record is in front of the block and wins over another Invoice variable
    let c = ok(&agent(
        "- crm get invoice \"x\" -> b\nfor each i in crm.invoices:\n  - crm update invoice: status = \"z\"\n",
    ));
    let upd = c.tool_calls.values().find(|t| t.verb.as_deref() == Some("update")).unwrap();
    assert_eq!(upd.var.as_deref(), Some("i"));
}

#[test]
fn crm_update_without_a_record_in_scope_is_an_error() {
    let e = first_error(&agent("- crm update invoice: amount = 1 EUR\n"));
    assert!(
        e.contains("\"invoice\" needs an Invoice in scope (a loop over invoices, or a \"-> x\" result), or write its id: \"crm update invoice x: ...\""),
        "{e}"
    );
}

#[test]
fn crm_update_with_an_explicit_id_needs_no_variable() {
    let c = ok(&agent("- crm update invoice \"x\": status = \"y\"\n"));
    let upd = c.tool_calls.values().next().unwrap();
    assert_eq!((upd.var.as_deref(), upd.field.as_deref()), (None, None));
}

#[test]
fn the_ambiguity_help_shows_how_to_choose() {
    let e = first_error(&agent(
        "- crm get invoice \"x\" -> b\n- crm get invoice \"y\" -> a2\n- crm update invoice: status = \"z\"\n",
    ));
    assert!(e.contains("Name the one you mean, for example \"crm update invoice a2: ...\"."), "{e}");
}

#[test]
fn a_payload_typo_does_not_hide_the_missing_record() {
    let e = errors(&agent("- crm update invoice: statuss = \"1\"\n"));
    assert!(e.contains("Unknown field \"statuss\" on Invoice"), "{e}");
    assert!(e.contains("\"invoice\" needs an Invoice in scope"), "{e}");
    let e = errors(&agent("- email send customer: 5\n"));
    assert!(e.contains("\"customer\" needs a record in front of this block"), "{e}");
}

// ------------------------------------------------------------------ narrowing of bare fields

const EVENT: &str = "# Record: Lead\n- id: text\n- score: maybe int\n\n# Agent: a\n- model: claude\n- tools: crm, log\n\n## When crm receives lead\n";

#[test]
fn a_bare_field_is_narrowed_by_the_qualified_check_and_back() {
    ok(&format!("{EVENT}- guard: lead.score is not empty\n- log: \"{{score}}\"\n"));
    ok(&format!("{EVENT}- guard: score is not empty\n- log: \"{{lead.score}}\"\n"));
    let e = first_error(&format!("{EVENT}- log: \"{{score}}\"\n"));
    assert!(e.contains("so it cannot be written into text yet."), "{e}");
}

#[test]
fn checking_a_present_value_again_is_fine() {
    ok(&format!("{EVENT}- guard: lead.score is not empty\n- guard: lead.score is not empty\n- log: \"{{score}}\"\n"));
    ok(&format!("{EVENT}- guard: score is not empty\n- if score is not empty:\n  - log: \"{{score}}\"\n"));
}

// ------------------------------------------------------------------ declaration sites

#[test]
fn declaration_and_definition_sites_are_exported() {
    let c = example_01();
    let var = c
        .resolutions
        .values()
        .find_map(|r| match r {
            Resolution::Var { name, def } if name == "reminder" => Some(def),
            _ => None,
        })
        .unwrap();
    // `reminder` is defined by the `ask model … -> reminder` line
    assert_eq!(var.unwrap().line, 15);
    assert!(c.constants["grace_period"].span.line > 0);
    let agent = c.declarations.iter().find(|d| d.name == "invoice-chaser").unwrap();
    assert_eq!(agent.kind, agentmark::sema::Kind::Agent);
    assert!(c.files[agent.file].ends_with("invoice-chaser.agent.md"));
    assert!(c.declarations.iter().any(|d| d.name == "overdue" && d.kind == agentmark::sema::Kind::Rule));
    assert!(c.declarations.iter().any(|d| d.name == "Invoice" && d.kind == agentmark::sema::Kind::Record));
}

#[test]
fn function_signatures_know_where_they_are_declared() {
    let c = ok("# Function: five\n- out: int\n- effect: pure\n  - return 5\n\n# Function: g\n- out: int\n- effect: pure\n  - return five()\n");
    assert_eq!(c.functions["five"].span.line, 1);
    assert_eq!(c.functions["g"].span.line, 6);
}

// ------------------------------------------------------------------ result of T (step 10, Q200)

const FALLIBLE: &str = "# Function: read_count\n- in: path: text\n- out: result of int\n- effect: local\n  - try read file path as int -> n\n  - return n\n\n# Command: check\n- args: path: text\n\n## Run\n";

#[test]
fn a_result_function_accepts_a_plain_return_and_try_hands_back_the_plain_type() {
    // `return n` (an int) is accepted for `result of int`; after `try`, `n` is an int again
    ok(&format!("{FALLIBLE}- try do read_count(path) -> n\n  otherwise:\n    - stop\n- print: \"{{n + 1}}\"\n"));
    // the otherwise branch assigns the plain type too
    ok(&format!("{FALLIBLE}- try do read_count(path) -> n\n  otherwise:\n    - set n = 0\n- print: \"{{n}}\"\n"));
}

#[test]
fn a_result_function_returning_the_wrong_type_is_still_an_error() {
    let src = "# Function: f\n- in: path: text\n- out: result of int\n- effect: pure\n  - return \"x\"\n";
    let e = first_error(src);
    assert!(e.contains("This returns text, but the function says it returns \"int\"."), "{e}");
}

#[test]
fn a_result_function_must_be_called_with_try() {
    let e = first_error(&format!("{FALLIBLE}- do read_count(path) -> n\n- print: \"{{n}}\"\n"));
    assert!(e.contains("can fail (it returns \"result of int\"), so it has to be called with \"try\""), "{e}");
    assert!(e.contains("try do read_count"), "{e}");
    // and not inside an expression
    let e = first_error(&format!("{FALLIBLE}- set n = read_count(path)\n- print: \"{{n}}\"\n"));
    assert!(e.contains("can fail") && e.contains("cannot be used inside an expression"), "{e}");
}

#[test]
fn the_value_after_try_is_the_plain_type_not_a_result() {
    let e = first_error(&format!("{FALLIBLE}- try do read_count(path) -> n\n  otherwise:\n    - stop\n- set n = \"text\"\n"));
    assert!(e.contains("\"n\" holds int"), "{e}");
}
