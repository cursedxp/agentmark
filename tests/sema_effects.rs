//! Effect rules (SPEC 8, 13.1, 15.3, 16, 17, 23.7): purity of rules and guards, functions against
//! what their `- touches:` line promises, summaries, Rust blocks and bindings, parallel conflicts.

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

/// A function `f` whose steps are `steps` (one per entry, numbered here) and then `return invoice.id`.
/// `touches` is the text after `- touches:`; empty leaves the line out.
fn function(touches: &str, steps: &[&str]) -> String {
    let line = if touches.is_empty() { String::new() } else { format!("- touches: {touches}\n") };
    let mut body = String::new();
    for (i, s) in steps.iter().enumerate() {
        body.push_str(&format!("  {}. {s}\n", i + 1));
    }
    body.push_str(&format!("  {}. return invoice.id\n", steps.len() + 1));
    format!("{PRE}# Function: f\n- in: invoice: Invoice\n- out: text\n{line}- steps:\n{body}")
}

// ------------------------------------------------------------------ rules and guards are pure

#[test]
fn a_rule_may_not_call_a_network_function() {
    let src = format!("{PRE}# Function: live\n- in: invoice: Invoice\n- out: text\n- touches: crm\n- steps:\n  1. return invoice.status\n\n# Rule: r Invoice\n- when: live(invoice) = \"open\"\n");
    let e = errors(&src);
    assert!(
        e.contains("A rule must be pure, but \"live\" has the effect network."),
        "{e}"
    );
    assert!(e.contains("Rules are pure checks: they may only read the record's own fields and call pure functions."), "{e}");
}

#[test]
fn a_rule_may_not_call_a_local_function_either() {
    let src = format!("{PRE}# Function: noisy\n- in: invoice: Invoice\n- out: text\n- touches: log\n- steps:\n  1. return invoice.status\n\n# Rule: r Invoice\n- when: noisy(invoice) = \"open\"\n");
    let e = first_error(&src);
    assert!(
        e.contains("A rule must be pure, but \"noisy\" has the effect local."),
        "{e}"
    );
}

#[test]
fn a_rule_may_call_a_pure_function() {
    let src = format!("{PRE}# Function: low\n- in: invoice: Invoice\n- out: text\n- touches: nothing\n- steps:\n  1. return invoice.status\n\n# Rule: r Invoice\n- when: low(invoice) = \"open\"\n");
    ok(&src);
}

#[test]
fn a_rule_may_not_read_a_tool_source() {
    let src = format!("{PRE}# Rule: r Invoice\n- when: count of crm.invoices > 3\n");
    let e = first_error(&src);
    assert!(
        e.contains("A rule must be pure, but \"crm.invoices\" has the effect network."),
        "{e}"
    );
}

#[test]
fn a_guard_must_be_pure() {
    let src = format!("{PRE}# Function: live\n- in: invoice: Invoice\n- out: text\n- touches: crm\n- steps:\n  1. return invoice.status\n\n# Agent: a\n- model: claude\n- tools: crm, log\n\n## Every 5 minutes\n1. for each i in crm.invoices:\n   1. guard: live(i) = \"open\"\n");
    let e = first_error(&src);
    assert!(
        e.contains("A guard must be pure, but \"live\" has the effect network."),
        "{e}"
    );
    assert!(e.contains("Guards are pure checks"), "{e}");
}

#[test]
fn the_else_branch_of_a_guard_may_have_effects() {
    ok(&agent("1. guard: 1 = 1 else log: \"nope\"\n"));
}

#[test]
fn a_constant_must_be_pure_and_cannot_use_names_from_elsewhere() {
    let e = first_error("# Constant: c\n- value: nothere\n");
    assert!(e.contains("Unknown name \"nothere\""), "{e}");
}

// ------------------------------------------------------------------ functions against their `- touches:`

#[test]
fn a_function_that_touches_nothing_may_not_log() {
    let e = errors(&function("nothing", &["log: \"x\""]));
    assert!(
        e.contains("The function \"f\" says \"- touches: nothing\", but this step touches log."),
        "{e}"
    );
    assert!(
        e.contains("Add log to the \"- touches:\" line, leave the line out to let the checker work it out, or move this step out of the function."),
        "{e}"
    );
}

#[test]
fn the_message_names_everything_the_step_touches_but_the_line_leaves_out() {
    let e = first_error(&function("log", &["email send address \"a@b.c\": \"x\""]));
    assert!(
        e.contains("says \"- touches: log\", but this step touches email."),
        "{e}"
    );
}

#[test]
fn a_function_that_promises_no_ai_may_not_ask_a_model() {
    let e = first_error(&function("email", &["ask model: \"hi\" into t"]));
    assert!(
        e.contains("says \"- touches: email\", but this step touches AI."),
        "{e}"
    );
}

#[test]
fn a_function_that_lists_what_it_touches_may_do_it() {
    ok(&function(
        "AI, email, log",
        &["ask model: \"hi\" into t", "email send address \"a@b.c\": \"{t}\"", "log: \"x\""],
    ));
}

#[test]
fn without_a_touches_line_the_function_is_not_limited_and_its_touches_are_computed() {
    let c = ok(&function(
        "",
        &["ask model: \"hi\" into t", "email send address \"a@b.c\": \"{t}\"", "log: \"x\""],
    ));
    let f = c.blocks.iter().find(|b| b.label == "f").unwrap();
    assert_eq!(f.declared, None);
    assert_eq!(f.effect.touches(), "AI, email, log");
    let c = ok(&function("", &[]));
    let f = c.blocks.iter().find(|b| b.label == "f").unwrap();
    assert_eq!(f.effect.touches(), "nothing");
}

#[test]
fn the_error_lands_on_the_offending_step_not_the_loop_around_it() {
    let src = format!("{PRE}# Function: f\n- in: invoices: list of Invoice\n- out: int\n- touches: nothing\n- steps:\n  1. for each i in invoices:\n     1. log: \"x\"\n  2. return 1\n");
    let e = first_error(&src);
    assert!(e.starts_with("t.agent.md:11\n       1. log: \"x\""), "{e}");
}

#[test]
fn only_the_first_excess_is_reported_per_function() {
    let e = errors(&function("nothing", &["log: \"a\"", "log: \"b\""]));
    assert_eq!(e.matches("says \"- touches: nothing\"").count(), 1, "{e}");
}

#[test]
fn a_function_calling_a_function_that_touches_email_touches_email_too() {
    let src = format!("{PRE}# Function: inner\n- in: invoice: Invoice\n- out: text\n- steps:\n  1. email send address \"a@b.c\": \"x\"\n  2. return invoice.id\n\n# Function: outer\n- in: invoice: Invoice\n- out: text\n- touches: log\n- steps:\n  1. return inner(invoice)\n");
    let e = first_error(&src);
    assert!(e.contains("The function \"outer\" says \"- touches: log\", but this step touches email."), "{e}");
}

#[test]
fn callers_see_the_touches_a_callee_lists_even_when_its_body_touches_less() {
    // `inner` lists email but its body touches nothing; callers still see email
    let src = format!("{PRE}# Function: inner\n- in: invoice: Invoice\n- out: text\n- touches: email\n- steps:\n  1. return invoice.id\n\n# Function: outer\n- in: invoice: Invoice\n- out: text\n- touches: email\n- steps:\n  1. return inner(invoice)\n");
    let c = ok(&src);
    let outer = c.blocks.iter().find(|b| b.label == "outer").unwrap();
    assert_eq!(outer.effect.to_string(), "network(email)");
    assert_eq!(outer.effect.touches(), "email");
    assert_eq!(outer.declared, Some(Effect::Network));
    // and a caller that does not list it is told
    let src = src.replace("- out: text\n- touches: email\n- steps:\n  1. return inner", "- out: text\n- touches: nothing\n- steps:\n  1. return inner");
    let e = first_error(&src);
    assert!(e.contains("says \"- touches: nothing\", but this step touches email."), "{e}");
}

#[test]
fn an_unknown_word_after_touches_is_refused_with_the_known_ones() {
    let e = first_error(&function("emial", &[]));
    assert!(e.contains("\"touches:\" names \"emial\", which is nothing this program can touch."), "{e}");
    assert!(e.contains("Did you mean \"email\"?"), "{e}");
    assert!(e.contains("Use nothing, AI, files, print, input, state, or the name of a tool or library."), "{e}");
}

#[test]
fn nothing_stands_alone() {
    let e = first_error(&function("nothing, log", &[]));
    assert!(
        e.contains("\"nothing\" stands alone: a part that touches something does not touch nothing."),
        "{e}"
    );
}

#[test]
fn the_old_effect_line_is_refused_and_shows_the_new_form() {
    let src = "# Function: f\n- out: int\n- effect: pure\n- steps:\n  1. return 1\n";
    let e = first_error(src);
    assert!(e.contains("\"effect:\" was removed. Write: - touches: nothing"), "{e}");
}

#[test]
fn recursion_is_allowed() {
    let src = "# Function: down\n- in: n: int\n- out: int\n- touches: nothing\n- steps:\n  1. if n = 0:\n     1. return 0\n  2. return down(n - 1)\n";
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
    let src = "# Function: a\n- in: n: int\n- out: int\n- touches: nothing\n- steps:\n  1. return b(n)\n\n# Function: b\n- in: n: int\n- out: int\n- touches: nothing\n- steps:\n  1. return a(n)\n";
    ok(src);
}

// ------------------------------------------------------------------ summaries

#[test]
fn every_statement_and_block_has_an_effect_set() {
    let src =
        agent("1. ask model: \"hi\" into t\n2. email send address \"a@b.c\": \"{t}\"\n3. log: \"x\"\n");
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
    let src = format!("{PRE}# Agent: a\n- model: claude\n- tools: email, crm, log\n\n## Every 5 minutes\n1. for each i in crm.invoices:\n   1. do tell\n\n## Do tell\n1. email send address \"a@b.c\": \"x\"\n");
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
    let src = "# Record: Invoice\n- id: text\n\n# Record: Lead\n- id: text\n\n# Agent: a\n- model: claude\n- tools: crm, log\n\n## Every 5 minutes\n1. for each i in crm.invoices:\n   1. do show\n2. for each l in crm.leads:\n   1. do show\n\n## Do show\n1. log: \"{id}\"\n";
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
    let w = warnings("# Record: R\n- id: text\n\n# Agent: a\n- model: claude\n- tools: log\n\n## Do lonely\n1. log: \"{id}\"\n");
    assert!(w.contains("Unused action: no block calls \"do lonely\"."), "{w}");
}

#[test]
fn an_uncalled_do_block_still_reports_names_that_no_record_has() {
    let e = first_error("# Record: R\n- id: text\n\n# Agent: a\n- model: claude\n- tools: log\n\n## Do lonely\n1. log: \"{whatever}\"\n");
    assert!(e.contains("Unknown name \"whatever\""), "{e}");
}

#[test]
fn do_with_a_result_variable_from_a_function() {
    let src = "# Function: five\n- out: int\n- touches: nothing\n- steps:\n  1. return 5\n\n# Agent: a\n- model: claude\n- tools: log\n\n## On start\n1. do five into n\n2. log: \"{n}\"\n";
    ok(src);
}

#[test]
fn do_of_an_unknown_action_suggests() {
    let src = "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n1. do remnd\n\n## Do remind\n1. log: \"x\"\n";
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
fn with_rust(touches: &str, rest: &str) -> String {
    format!("{PRE}# Function: host\n- in: s: text\n- out: text\n- touches: nothing\n- steps:\n  1. return s\n\n## Rust: shout -> text\n- in: s: text\n- touches: {touches}\n\n```rust\ns.to_uppercase()\n```\n\n{rest}")
}

#[test]
fn rust_blocks_carry_what_they_say_they_touch() {
    let src = with_rust(
        "email",
        "# Function: f\n- in: s: text\n- out: text\n- steps:\n  1. return shout(s)\n",
    );
    let c = ok(&src);
    assert_eq!(c.rust_blocks.len(), 1);
    assert_eq!(c.rust_blocks[0].sig.effect, Effect::Network);
    assert_eq!(c.rust_blocks[0].parent.as_deref(), Some("host"));
    let f = c.blocks.iter().find(|b| b.label == "f").unwrap();
    assert_eq!(f.effect.to_string(), "network(email)");
    assert_eq!(f.effect.touches(), "email");
}

#[test]
fn a_rust_block_must_say_what_it_touches() {
    let src = "# Function: host\n- in: s: text\n- out: text\n- touches: nothing\n- steps:\n  1. return shout(s)\n\n## Rust: shout -> text\n- in: s: text\n\n```rust\ns.to_uppercase()\n```\n";
    let e = first_error(src);
    assert!(e.contains("The Rust block \"shout\" does not say what it touches."), "{e}");
    assert!(e.contains("Add \"- touches: nothing\" (or files, print, AI, or the tools it uses)"), "{e}");
}

#[test]
fn a_rust_block_with_the_old_effect_line_is_refused() {
    let src = "# Function: host\n- in: s: text\n- out: text\n- touches: nothing\n- steps:\n  1. return shout(s)\n\n## Rust: shout -> text\n- in: s: text\n- effect: pure\n\n```rust\ns.to_uppercase()\n```\n";
    let e = first_error(src);
    assert!(e.contains("\"effect:\" was removed."), "{e}");
}

#[test]
fn a_function_that_touches_nothing_may_not_call_a_rust_block_that_touches_email() {
    let src = with_rust(
        "email",
        "# Function: f\n- in: s: text\n- out: text\n- touches: nothing\n- steps:\n  1. return shout(s)\n",
    );
    let e = first_error(&src);
    assert!(
        e.contains("says \"- touches: nothing\", but this step touches email."),
        "{e}"
    );
}

#[test]
fn a_rule_may_call_a_rust_block_that_touches_nothing() {
    let src = with_rust(
        "nothing",
        "# Rule: r Invoice\n- when: shout(id) = \"A\"\n",
    );
    ok(&src);
}

#[test]
fn unverified_bindings_count_as_unapproved_and_are_listed() {
    let main = "# Import: crate printpdf\n\n# Function: make\n- in: t: text\n- out: bytes\n- steps:\n  1. pdf render t into b\n  2. return b\n";
    let binding = "# Binding: pdf\n- crate: printpdf\n- effect: unverified\n- proposed-effect: local\n\n## render(title: text) -> bytes\n```rust\nOk(Vec::new())\n```\n";
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
fn a_function_that_uses_an_unverified_binding_cannot_claim_to_touch_nothing() {
    let main = "# Import: crate printpdf\n\n# Function: make\n- in: t: text\n- out: bytes\n- touches: nothing\n- steps:\n  1. pdf render t into b\n  2. return b\n";
    let binding = "# Binding: pdf\n- crate: printpdf\n- effect: unverified\n\n## render(title: text) -> bytes\n```rust\nOk(Vec::new())\n```\n";
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
        out.contains("but this step touches pdf.") && out.contains("A library nobody approved yet (pdf)"),
        "{out}"
    );
}

#[test]
fn an_approved_binding_is_trusted_with_its_effect() {
    let main = "# Import: crate printpdf\n\n# Function: make\n- in: t: text\n- out: bytes\n- steps:\n  1. pdf render t into b\n  2. return b\n";
    let binding = "# Binding: pdf\n- crate: printpdf\n- effect: local\n\n## render(title: text) -> bytes\n```rust\nOk(Vec::new())\n```\n";
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
    let binding = "# Binding: pdf\n- crate: printpdf\n- effect: local\n\n## render(title: text) -> bytes\n```rust\nOk(Vec::new())\n```\n";
    let run = |stmt: &str| {
        let main = format!("# Import: crate printpdf\n\n# Function: make\n- in: t: text\n- out: bytes\n- steps:\n  1. {stmt}\n  2. return b\n");
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
    assert!(run("pdf rendr t into b")
        .contains("Unknown verb \"rendr\" for binding pdf. Did you mean \"render\"?"));
    assert!(run("pdf render t t into b").contains("takes 1 value (title), but 2 were given."));
    assert!(run("pdf").contains("The binding pdf needs a verb."));
}

// ------------------------------------------------------------------ parallel

#[test]
fn parallel_statements_must_not_write_the_same_variable() {
    let src = agent("1. parallel:\n   1. ask model: \"a\" into r\n   2. ask model: \"b\" into r\n");
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
        e.starts_with("t.agent.md:12\n     2. ask model: \"b\" into r"),
        "{e}"
    );
}

#[test]
fn parallel_with_distinct_variables_is_fine_and_defines_them_afterwards() {
    let src = agent("1. parallel:\n   1. ask model: \"a\" into r1\n   2. ask model: \"b\" into r2\n2. log: \"{r1} {r2}\"\n");
    let c = ok(&src);
    let b = c.blocks.iter().find(|b| b.label == "On start").unwrap();
    assert_eq!(b.effect.to_string(), "model, local(log)");
}

#[test]
fn parallel_conflicts_through_nested_statements_and_into() {
    let src = agent(
        "1. 0 into total\n2. parallel:\n   1. 1 into total\n   2. if 1 = 1:\n      1. 2 into total\n",
    );
    let e = first_error(&src);
    assert!(e.contains("both write \"total\""), "{e}");
}

// ------------------------------------------------------------ text -> bytes for binding arguments (Q151)

const HASH_BINDING: &str = "# Binding: hashing\n- crate: hashing\n- effect: pure\n\n## digest(data: bytes) -> bytes\n```rust\nOk(data)\n```\n";

fn hash_call(param: &str, arg: &str) -> Result<sema::Checked, String> {
    let main = format!(
        "# Import: crate hashing\n\n# Function: make\n- in: {param}\n- out: bytes\n- touches: nothing\n- steps:\n  1. hashing digest {arg} into b\n  2. return b\n"
    );
    let p = Project::from_memory(
        "app/main.agent.md",
        vec![("app/main.agent.md", main.as_str()), ("app/bindings/hashing.agent.md", HASH_BINDING)],
    );
    sema::check(&p).map_err(|d| p.render(&d))
}

#[test]
fn a_binding_taking_bytes_accepts_text() {
    let c = hash_call("t: text", "t").unwrap();
    assert_eq!(c.text_to_bytes.len(), 1);
    let c = hash_call("t: text", "\"{t}!\"").unwrap();
    assert_eq!(c.text_to_bytes.len(), 1);
}

#[test]
fn a_binding_taking_bytes_still_rejects_other_types() {
    let e = hash_call("m: money", "m").unwrap_err();
    assert!(e.contains("must be bytes, but this is money"), "{e}");
    let e = hash_call("n: int", "n").unwrap_err();
    assert!(e.contains("must be bytes"), "{e}");
}
