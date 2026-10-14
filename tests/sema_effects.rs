//! Effect rules (SPEC 8, 13.1, 15.3, 16, 17): purity of rules and guards, functions against their
//! declared effect, summaries, Rust blocks and bindings, parallel conflicts.

mod common;
use common::*;

use agentmark::project::Project;
use agentmark::sema::{self, BlockKind, EffectSet};
use agentmark::syntax::ast::Effect;

const PRE: &str = "# Record: Invoice\n- id: text\n- status: text\n\n";

fn agent(body: &str) -> String {
    format!(
        "{PRE}# Agent: a\n- model: claude\n- tools: email, crm, log, http\n\n## On start\n{body}"
    )
}

fn function(effect: &str, body: &str) -> String {
    format!("{PRE}# Function: f\n- in: invoice: Invoice\n- out: text\n- effect: {effect}\n  {body}\n  - return invoice.id\n")
}

// ------------------------------------------------------------------ rules and guards are pure

#[test]
fn a_rule_may_not_call_a_network_function() {
    let src = format!("{PRE}# Function: live\n- in: invoice: Invoice\n- out: text\n- effect: network\n  - return invoice.status\n\n# Rule: r\n- for: Invoice\n- when: live(invoice) = \"open\"\n");
    let e = errors(&src);
    assert!(
        e.contains("A rule must be pure, but \"live\" has the effect network."),
        "{e}"
    );
    assert!(e.contains("Rules are pure checks: they may only read the record's own fields and call pure functions."), "{e}");
}

#[test]
fn a_rule_may_not_call_a_local_function_either() {
    let src = format!("{PRE}# Function: noisy\n- in: invoice: Invoice\n- out: text\n- effect: local\n  - return invoice.status\n\n# Rule: r\n- for: Invoice\n- when: noisy(invoice) = \"open\"\n");
    let e = first_error(&src);
    assert!(
        e.contains("A rule must be pure, but \"noisy\" has the effect local."),
        "{e}"
    );
}

#[test]
fn a_rule_may_call_a_pure_function() {
    let src = format!("{PRE}# Function: low\n- in: invoice: Invoice\n- out: text\n- effect: pure\n  - return invoice.status\n\n# Rule: r\n- for: Invoice\n- when: low(invoice) = \"open\"\n");
    ok(&src);
}

#[test]
fn a_rule_may_not_read_a_tool_source() {
    let src = format!("{PRE}# Rule: r\n- for: Invoice\n- when: count of crm.invoices > 3\n");
    let e = first_error(&src);
    assert!(
        e.contains("A rule must be pure, but \"crm.invoices\" has the effect network."),
        "{e}"
    );
}

#[test]
fn a_guard_must_be_pure() {
    let src = format!("{PRE}# Function: live\n- in: invoice: Invoice\n- out: text\n- effect: network\n  - return invoice.status\n\n# Agent: a\n- model: claude\n- tools: crm, log\n\n## Every 5 minutes\nfor each i in crm.invoices:\n  - guard: live(i) = \"open\"\n");
    let e = first_error(&src);
    assert!(
        e.contains("A guard must be pure, but \"live\" has the effect network."),
        "{e}"
    );
    assert!(e.contains("Guards are pure checks"), "{e}");
}

#[test]
fn the_else_branch_of_a_guard_may_have_effects() {
    ok(&agent("- guard: 1 = 1 else log: \"nope\"\n"));
}

#[test]
fn a_constant_must_be_pure_and_cannot_use_names_from_elsewhere() {
    let e = first_error("# Constant: c\n- value: nothere\n");
    assert!(e.contains("Unknown name \"nothere\""), "{e}");
}

// ------------------------------------------------------------------ functions against their declared effect

#[test]
fn a_pure_function_may_not_log() {
    let e = errors(&function("pure", "- log: \"x\""));
    assert!(
        e.contains(
            "The function \"f\" says \"- effect: pure\", but this line has the effect local (log)."
        ),
        "{e}"
    );
    assert!(
        e.contains(
            "Change the function to \"- effect: local\", or move this line out of the function."
        ),
        "{e}"
    );
}

#[test]
fn a_local_function_may_not_email() {
    let e = first_error(&function("local", "- email send address \"a@b.c\": \"x\""));
    assert!(
        e.contains("says \"- effect: local\", but this line has the effect network (email)."),
        "{e}"
    );
}

#[test]
fn a_network_function_may_not_ask_a_model() {
    let e = first_error(&function("network", "- ask model: \"hi\" -> t"));
    assert!(
        e.contains("says \"- effect: network\", but this line has the effect model."),
        "{e}"
    );
}

#[test]
fn a_model_function_may_do_everything() {
    ok(&function(
        "model",
        "- ask model: \"hi\" -> t\n  - email send address \"a@b.c\": \"{t}\"\n  - log: \"x\"",
    ));
}

#[test]
fn the_error_lands_on_the_offending_line_not_the_loop_around_it() {
    let src = format!("{PRE}# Function: f\n- in: invoices: list of Invoice\n- out: int\n- effect: pure\n  - for each i in invoices:\n    - log: \"x\"\n  - return 1\n");
    let e = first_error(&src);
    assert!(e.starts_with("t.agent.md:10\n      - log: \"x\""), "{e}");
}

#[test]
fn only_the_first_excess_is_reported_per_function() {
    let e = errors(&function("pure", "- log: \"a\"\n  - log: \"b\""));
    assert_eq!(e.matches("says \"- effect: pure\"").count(), 1, "{e}");
}

#[test]
fn a_function_calling_a_network_function_inherits_the_effect() {
    let src = format!("{PRE}# Function: inner\n- in: invoice: Invoice\n- out: text\n- effect: network\n  - email send address \"a@b.c\": \"x\"\n  - return invoice.id\n\n# Function: outer\n- in: invoice: Invoice\n- out: text\n- effect: local\n  - return inner(invoice)\n");
    let e = first_error(&src);
    assert!(e.contains("The function \"outer\" says \"- effect: local\", but this line has the effect network (email)."), "{e}");
}

#[test]
fn a_declared_effect_is_a_floor_for_callers() {
    // `inner` says network but its body is pure; callers still see network(inner)
    let src = format!("{PRE}# Function: inner\n- in: invoice: Invoice\n- out: text\n- effect: network\n  - return invoice.id\n\n# Function: outer\n- in: invoice: Invoice\n- out: text\n- effect: network\n  - return inner(invoice)\n");
    let c = ok(&src);
    let outer = c.blocks.iter().find(|b| b.label == "outer").unwrap();
    assert_eq!(outer.effect.to_string(), "network(inner)");
    assert_eq!(outer.declared, Some(Effect::Network));
}

#[test]
fn recursion_is_allowed() {
    let src = "# Function: down\n- in: n: int\n- out: int\n- effect: pure\n  - if n = 0:\n    - return 0\n  - return down(n - 1)\n";
    let c = ok(src);
    assert!(c
        .blocks
        .iter()
        .find(|b| b.label == "down")
        .unwrap()
        .effect
        .is_pure());
}

#[test]
fn mutual_recursion_is_allowed() {
    let src = "# Function: a\n- in: n: int\n- out: int\n- effect: pure\n  - return b(n)\n\n# Function: b\n- in: n: int\n- out: int\n- effect: pure\n  - return a(n)\n";
    ok(src);
}

// ------------------------------------------------------------------ summaries

#[test]
fn every_statement_and_block_has_an_effect_set() {
    let src =
        agent("- ask model: \"hi\" -> t\n- email send address \"a@b.c\": \"{t}\"\n- log: \"x\"\n");
    let c = ok(&src);
    let b = c.blocks.iter().find(|b| b.label == "On start").unwrap();
    assert_eq!(b.kind, BlockKind::Entry);
    assert_eq!(b.effect.to_string(), "model, network(email), local(log)");
    // per statement
    let mut per: Vec<String> = c
        .stmt_effects
        .iter()
        .filter(|(k, _)| k.span.line >= 10)
        .map(|(k, e)| format!("{}:{}", k.span.line, e))
        .collect();
    per.sort();
    assert_eq!(per, ["10:model", "11:network(email)", "12:local(log)"]);
}

#[test]
fn a_do_action_contributes_its_effect_to_the_caller() {
    let src = format!("{PRE}# Agent: a\n- model: claude\n- tools: email, crm, log\n\n## Every 5 minutes\nfor each i in crm.invoices:\n  - do tell\n\n## Do tell\n- email send address \"a@b.c\": \"x\"\n");
    let c = ok(&src);
    let entry = c
        .blocks
        .iter()
        .find(|b| b.label == "Every 5 minutes")
        .unwrap();
    assert_eq!(entry.effect.to_string(), "network(crm, email)");
    let action = c.blocks.iter().find(|b| b.label == "Do tell").unwrap();
    assert_eq!(action.kind, BlockKind::Action);
    assert_eq!(action.effect.to_string(), "network(email)");
    assert_eq!(action.param, Some(("i".to_string(), "Invoice".to_string())));
    assert_eq!(c.unit_effect("a").to_string(), "network(crm, email)");
}

#[test]
fn a_do_block_sees_the_loop_record_but_two_different_records_are_an_error() {
    let src = "# Record: Invoice\n- id: text\n\n# Record: Lead\n- id: text\n\n# Agent: a\n- model: claude\n- tools: crm, log\n\n## Every 5 minutes\nfor each i in crm.invoices:\n  - do show\nfor each l in crm.leads:\n  - do show\n\n## Do show\n- log: \"{id}\"\n";
    let e = first_error(src);
    assert!(
        e.contains(
            "The block \"show\" works on \"i\" (Invoice), but here it is called with \"l\" (Lead)."
        ),
        "{e}"
    );
}

#[test]
fn an_uncalled_do_block_is_a_warning_not_an_error() {
    // we do not know which record it works on, so a name that is a field of some record passes
    let w = warnings("# Record: R\n- id: text\n\n# Agent: a\n- model: claude\n- tools: log\n\n## Do lonely\n- log: \"{id}\"\n");
    assert!(w.contains("Unused action: no block calls \"do lonely\"."), "{w}");
}

#[test]
fn an_uncalled_do_block_still_reports_names_that_no_record_has() {
    let e = first_error("# Record: R\n- id: text\n\n# Agent: a\n- model: claude\n- tools: log\n\n## Do lonely\n- log: \"{whatever}\"\n");
    assert!(e.contains("Unknown name \"whatever\""), "{e}");
}

#[test]
fn do_with_a_result_variable_from_a_function() {
    let src = "# Function: five\n- out: int\n- effect: pure\n  - return 5\n\n# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- do five -> n\n- log: \"{n}\"\n";
    ok(src);
}

#[test]
fn do_of_an_unknown_action_suggests() {
    let src = "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- do remnd\n\n## Do remind\n- log: \"x\"\n";
    let e = first_error(src);
    assert!(
        e.contains("Unknown action or function \"remnd\". Did you mean \"remind\"?"),
        "{e}"
    );
}

#[test]
fn display_format_is_the_spec_summary() {
    let mut e = EffectSet::pure();
    e.add(Effect::Model, "model");
    e.add(Effect::Network, "crm");
    e.add(Effect::Local, "log");
    assert_eq!(e.to_string(), "model, network(crm), local(log)");
}

// ------------------------------------------------------------------ Rust blocks and bindings

/// A function `host` with a `## Rust: shout` block under it (Rust blocks hoist to module level).
fn with_rust(effect: &str, rest: &str) -> String {
    format!("{PRE}# Function: host\n- in: s: text\n- out: text\n- effect: pure\n  - return s\n\n## Rust: shout -> text\n- in: s: text\n- effect: {effect}\n  ```rust\n  s.to_uppercase()\n  ```\n\n{rest}")
}

#[test]
fn rust_blocks_carry_their_declared_effect() {
    let src = with_rust(
        "network",
        "# Function: f\n- in: s: text\n- out: text\n- effect: network\n  - return shout(s)\n",
    );
    let c = ok(&src);
    assert_eq!(c.rust_blocks.len(), 1);
    assert_eq!(c.rust_blocks[0].sig.effect, Effect::Network);
    assert_eq!(c.rust_blocks[0].parent.as_deref(), Some("host"));
    let f = c.blocks.iter().find(|b| b.label == "f").unwrap();
    assert_eq!(f.effect.to_string(), "network(shout)");
}

#[test]
fn a_pure_function_may_not_call_a_network_rust_block() {
    let src = with_rust(
        "network",
        "# Function: f\n- in: s: text\n- out: text\n- effect: pure\n  - return shout(s)\n",
    );
    let e = first_error(&src);
    assert!(
        e.contains("says \"- effect: pure\", but this line has the effect network (shout)."),
        "{e}"
    );
}

#[test]
fn a_rule_may_call_a_pure_rust_block() {
    let src = with_rust(
        "pure",
        "# Rule: r\n- for: Invoice\n- when: shout(id) = \"A\"\n",
    );
    ok(&src);
}

#[test]
fn unverified_bindings_count_as_unapproved_and_are_listed() {
    let main = "# Import: crate printpdf\n\n# Function: make\n- in: t: text\n- out: bytes\n- effect: network\n  - pdf render t -> b\n  - return b\n";
    let binding = "# Binding: pdf\n- crate: printpdf\n- effect: unverified\n- proposed-effect: local\n\n## render(title: text) -> bytes\n  ```rust\n  Ok(Vec::new())\n  ```\n";
    let p = Project::from_memory(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", main),
            ("app/bindings/printpdf.agent.md", binding),
        ],
    );
    let c = sema::check(&p).unwrap_or_else(|d| panic!("{}", p.render(&d)));
    assert_eq!(c.unverified_bindings().count(), 1);
    let w = p.render(&c.warnings);
    assert!(w.contains("The binding \"pdf\" is unverified"), "{w}");
    assert!(
        w.contains("Approve it by changing the line to \"- effect: local\""),
        "{w}"
    );
    let make = c.blocks.iter().find(|b| b.label == "make").unwrap();
    assert_eq!(make.effect.to_string(), "unverified(pdf)");
}

#[test]
fn a_function_that_uses_an_unverified_binding_cannot_claim_to_be_pure() {
    let main = "# Import: crate printpdf\n\n# Function: make\n- in: t: text\n- out: bytes\n- effect: pure\n  - pdf render t -> b\n  - return b\n";
    let binding = "# Binding: pdf\n- crate: printpdf\n- effect: unverified\n\n## render(title: text) -> bytes\n  ```rust\n  Ok(Vec::new())\n  ```\n";
    let p = Project::from_memory(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", main),
            ("app/bindings/printpdf.agent.md", binding),
        ],
    );
    let d = sema::check(&p).expect_err("must fail");
    let out = p.render(
        &d.into_iter()
            .filter(|x| x.severity == agentmark::diag::Severity::Error)
            .collect::<Vec<_>>(),
    );
    assert!(
        out.contains("but this line has the effect unverified (pdf)."),
        "{out}"
    );
}

#[test]
fn an_approved_binding_is_trusted_with_its_effect() {
    let main = "# Import: crate printpdf\n\n# Function: make\n- in: t: text\n- out: bytes\n- effect: local\n  - pdf render t -> b\n  - return b\n";
    let binding = "# Binding: pdf\n- crate: printpdf\n- effect: local\n\n## render(title: text) -> bytes\n  ```rust\n  Ok(Vec::new())\n  ```\n";
    let p = Project::from_memory(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", main),
            ("app/bindings/printpdf.agent.md", binding),
        ],
    );
    let c = sema::check(&p).unwrap_or_else(|d| panic!("{}", p.render(&d)));
    assert_eq!(c.unverified_bindings().count(), 0);
    assert_eq!(c.bindings[0].effect, Effect::Local);
    assert_eq!(c.bindings[0].items[0].name, "render");
}

#[test]
fn binding_verbs_and_value_counts_are_checked() {
    let binding = "# Binding: pdf\n- crate: printpdf\n- effect: local\n\n## render(title: text) -> bytes\n  ```rust\n  Ok(Vec::new())\n  ```\n";
    let run = |stmt: &str| {
        let main = format!("# Import: crate printpdf\n\n# Function: make\n- in: t: text\n- out: bytes\n- effect: local\n  {stmt}\n  - return b\n");
        let p = Project::from_memory(
            "app/main.agent.md",
            vec![
                ("app/main.agent.md", main.as_str()),
                ("app/bindings/printpdf.agent.md", binding),
            ],
        );
        match sema::check(&p) {
            Ok(_) => String::new(),
            Err(d) => p.render(
                &d.into_iter()
                    .filter(|x| x.severity == agentmark::diag::Severity::Error)
                    .collect::<Vec<_>>(),
            ),
        }
    };
    assert!(run("- pdf rendr t -> b")
        .contains("Unknown verb \"rendr\" for binding pdf. Did you mean \"render\"?"));
    assert!(run("- pdf render t t -> b").contains("takes 1 value (title), but 2 were given."));
    assert!(run("- pdf -> b").contains("The binding pdf needs a verb."));
}

// ------------------------------------------------------------------ parallel

#[test]
fn parallel_statements_must_not_write_the_same_variable() {
    let src = agent("- parallel:\n  - ask model: \"a\" -> r\n  - ask model: \"b\" -> r\n");
    let e = errors(&src);
    assert!(
        e.contains(
            "Two statements of this \"parallel\" block both write \"r\" (the first is on line 11)."
        ),
        "{e}"
    );
    assert!(
        e.contains("Give each statement its own variable, or run them one after the other."),
        "{e}"
    );
    assert!(
        e.starts_with("t.agent.md:12\n    - ask model: \"b\" -> r"),
        "{e}"
    );
}

#[test]
fn parallel_with_distinct_variables_is_fine_and_defines_them_afterwards() {
    let src = agent("- parallel:\n  - ask model: \"a\" -> r1\n  - ask model: \"b\" -> r2\n- log: \"{r1} {r2}\"\n");
    let c = ok(&src);
    let b = c.blocks.iter().find(|b| b.label == "On start").unwrap();
    assert_eq!(b.effect.to_string(), "model, local(log)");
}

#[test]
fn parallel_conflicts_through_nested_statements_and_set() {
    let src = agent(
        "- set total = 0\n- parallel:\n  - set total = 1\n  - if 1 = 1:\n    - set total = 2\n",
    );
    let e = first_error(&src);
    assert!(e.contains("both write \"total\""), "{e}");
}
