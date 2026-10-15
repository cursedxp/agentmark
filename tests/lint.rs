//! Lint tests (SPEC 18.4): one positive and one negative case per rule, `[lint] deny` handling,
//! the rendered form of a finding, and the real examples.

use agentmark::diag::{render_all, Diagnostic, Severity};
use agentmark::lint::{lint, lint_file, lint_source, LintConfig, RULES};
use agentmark::syntax::ast::File;
use agentmark::syntax::parser::parse_file;

fn parse(path: &str, src: &str) -> File {
    parse_file(path, src).unwrap_or_else(|d| panic!("did not parse:\n{}", render_all(&d, src)))
}

fn findings(src: &str, rule: &str) -> Vec<Diagnostic> {
    let file = parse("t.agent.md", src);
    lint_file(&file, &LintConfig::default())
        .into_iter()
        .filter(|d| d.code == rule)
        .collect()
}

/// Wrap numbered steps in a block of a documented agent, so only the rule under test can fire.
/// The block itself has no description (see `agent_with_block_doc`).
fn agent(body: &str) -> String {
    format!("# Agent: a\nTest agent.\n\n- model: claude\n- tools: log, crm\n\n## Do go\n{body}\n")
}

/// The same, with a description paragraph under the `## Do go` heading.
fn agent_with_block_doc(body: &str) -> String {
    format!("# Agent: a\nTest agent.\n\n- model: claude\n- tools: log, crm\n\n## Do go\nWhat this block does.\n\n{body}\n")
}

// ------------------------------------------------------------------ long-file

#[test]
fn long_file_fires_over_150_lines() {
    let mut src = String::from("# Record: Big\n");
    for i in 0..150 {
        src.push_str(&format!("- field{i}: text\n"));
    }
    // 151 lines through the source-aware entry point
    let d = lint_source("big.agent.md", &src, &LintConfig::default()).unwrap();
    assert_eq!(d.iter().filter(|d| d.code == "long-file").count(), 1);
    assert_eq!(d[0].severity, Severity::Warning);
    // and through the AST-only one
    assert_eq!(findings(&src, "long-file").len(), 1);
}

#[test]
fn long_file_is_quiet_at_150_lines() {
    let mut src = String::from("# Record: Big\n");
    for i in 0..149 {
        src.push_str(&format!("- field{i}: text\n"));
    }
    assert_eq!(src.lines().count(), 150);
    let d = lint_source("ok.agent.md", &src, &LintConfig::default()).unwrap();
    assert!(d.iter().all(|d| d.code != "long-file"));
    assert!(findings(&src, "long-file").is_empty());
}

// ------------------------------------------------------------------ magic-value

#[test]
fn magic_value_fires_for_a_repeated_amount_duration_or_url() {
    let src = agent(
        "1. guard: invoice.amount > 100 EUR\n2. guard: invoice.total < 100 EUR\n3. guard: today > due + 14 days\n4. guard: today > paid + 14 days\n5. http get \"https://api.example.com/rates\" into a\n6. http get \"https://api.example.com/rates\" into b",
    );
    let d = findings(&src, "magic-value");
    assert_eq!(d.len(), 3, "{d:?}");
    assert!(d[0].message.contains("amount 100 EUR is written 2 times"));
    assert!(d[1].message.contains("duration 14 days"));
    assert!(d[2].message.contains("URL https://api.example.com/rates"));
}

#[test]
fn magic_value_is_quiet_for_single_uses_zero_and_constants() {
    let src = "# Constant: limit\n- value: 100 EUR\n\n# Constant: limit2\n- value: 100 EUR\n\n# Agent: a\nTest agent.\n\n- model: claude\n- tools: log\n\n## Do go\n1. guard: invoice.amount > 100 EUR\n2. guard: invoice.amount > 0 EUR\n3. guard: invoice.total > 0 EUR\n4. http get \"https://api.example.com/rates\" into r\n";
    assert!(findings(src, "magic-value").is_empty());
}

// ------------------------------------------------------------------ unnamed-guard

const LONG_GUARD: &str = "1. guard: invoice.customer is not empty and invoice.amount > 0 EUR and invoice.status != \"paid\"";

#[test]
fn unnamed_guard_fires_for_a_long_guard_without_comment() {
    let d = findings(&agent(LONG_GUARD), "unnamed-guard");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("characters long"));
    // also behind `try`, once
    let src = agent(&LONG_GUARD.replace("1. guard", "1. try guard"));
    assert_eq!(findings(&src, "unnamed-guard").len(), 1);
}

#[test]
fn unnamed_guard_is_quiet_with_a_note_a_description_or_a_short_condition() {
    // a `>` note above the guard
    let noted = agent(&format!("> only bill customers with a real, open amount\n{LONG_GUARD}"));
    assert!(findings(&noted, "unnamed-guard").is_empty());
    // a description on the block (Q447)
    assert!(findings(&agent_with_block_doc(LONG_GUARD), "unnamed-guard").is_empty());
    // a short condition needs neither
    assert!(findings(&agent("1. guard: invoice.amount > 5 EUR"), "unnamed-guard").is_empty());
}

#[test]
fn unnamed_guard_is_satisfied_by_the_description_of_a_function_or_command() {
    let long = "invoice.customer is not empty and invoice.amount > 0 EUR and invoice.status != \"paid\"";
    for kind in ["Function", "Command"] {
        let body = |doc: &str| {
            format!("# {kind}: check\n{doc}- in: invoice: Invoice\n- steps:\n  1. guard: {long}\n")
        };
        assert_eq!(findings(&body(""), "unnamed-guard").len(), 1, "{kind} without description");
        assert!(findings(&body("Checks one invoice.\n\n"), "unnamed-guard").is_empty(), "{kind} with description");
    }
}

// ------------------------------------------------------------------ duplicate-rule

fn rule_file(path: &str, name: &str, when: &str) -> File {
    parse(path, &format!("# Rule: {name} Invoice\n- when: {when}\n"))
}

#[test]
fn duplicate_rule_fires_across_files_even_when_written_differently() {
    let a = rule_file("rules/open.agent.md", "open", "status != \"paid\"");
    let b = rule_file("rules/unpaid.agent.md", "unpaid", "status   !=\"paid\"");
    let d = lint(&[a, b], &LintConfig::default());
    let d: Vec<_> = d.iter().filter(|d| d.code == "duplicate-rule").collect();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].file, "rules/unpaid.agent.md");
    assert!(d[0].message.contains("\"open\"") && d[0].message.contains("rules/open.agent.md:1"));
}

#[test]
fn duplicate_rule_is_quiet_for_different_conditions() {
    let a = rule_file("a.agent.md", "open", "status != \"paid\"");
    let b = rule_file("b.agent.md", "paid", "status = \"paid\"");
    let c = rule_file("c.agent.md", "late", "today > due + 14 days");
    let d = rule_file("d.agent.md", "later", "today > due + 15 days");
    assert!(lint(&[a, b, c, d], &LintConfig::default())
        .iter()
        .all(|d| d.code != "duplicate-rule"));
}

// ------------------------------------------------------------------ prompt-in-loop

#[test]
fn prompt_in_loop_fires_without_a_guard_on_the_result() {
    let src = agent(
        "1. for each t in crm.tickets:\n   1. ask model with t: \"Classify\" into c\n   2. log: \"{c}\"",
    );
    let d = findings(&src, "prompt-in-loop");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("\"c\""));
    // a guard on something else does not count, and a nested ask is reported once
    let nested = agent(
        "1. for each t in crm.tickets:\n   1. guard: t.id is not empty\n   2. if t.open:\n      1. for each u in t.items:\n         1. ask model: \"x\" into c",
    );
    assert_eq!(findings(&nested, "prompt-in-loop").len(), 1);
}

#[test]
fn prompt_in_loop_is_quiet_with_a_guard_or_outside_a_loop() {
    let guarded = agent(
        "1. for each t in crm.tickets:\n   1. ask model with t: \"Classify\" into c\n   2. guard: c is not empty\n   3. log: \"{c}\"",
    );
    assert!(findings(&guarded, "prompt-in-loop").is_empty());
    let outside = agent("1. ask model: \"Summarise\" into s\n2. for each t in crm.tickets:\n   1. log: \"{s}\"");
    assert!(findings(&outside, "prompt-in-loop").is_empty());
}

// ------------------------------------------------------------------ missing-doc

#[test]
fn missing_doc_fires_for_agent_service_and_command_without_description() {
    let src = "# Agent: a\n- model: claude\n\n## Do go\n1. stop\n\n# Service: s\n- listen: 0.0.0.0:80\n\n## GET /x\n1. respond 200\n\n# Command: c\n- steps:\n  1. stop\n";
    let d = findings(src, "missing-doc");
    assert_eq!(d.len(), 3, "{d:?}");
    assert!(d[0].message.contains("agent \"a\""));
    assert!(d[1].message.contains("service \"s\""));
    assert!(d[2].message.contains("command \"c\""));
}

#[test]
fn missing_doc_wants_a_paragraph_under_the_heading_not_a_note_above_it() {
    // a `>` line is a note, not a description
    let src = "# Agent: a\n> This is only a note.\n\n- model: claude\n\n## Do go\n1. stop\n";
    assert_eq!(findings(src, "missing-doc").len(), 1);
    // several paragraphs under the heading are one description
    let src = "# Agent: a\nDoes things.\n\nAnd more things.\n\n- model: claude\n\n## Do go\n1. stop\n";
    assert!(findings(src, "missing-doc").is_empty());
}

#[test]
fn missing_doc_is_quiet_with_a_description_and_for_other_declarations() {
    let src = "# Agent: a\nDoes things.\n\n- model: claude\n\n## Do go\n1. stop\n\n# Record: R\n- x: text\n";
    assert!(findings(src, "missing-doc").is_empty());
}

// ------------------------------------------------------------------ vague-name

#[test]
fn vague_name_fires_once_per_name_per_declaration() {
    let src = "# Record: thing\n- data: text\n\n# Function: f\n- in: x: int\n- touches: nothing\n- steps:\n  1. x into result\n  2. result + 1 into result\n  3. return result\n";
    let d = findings(src, "vague-name");
    let names: Vec<&str> = d.iter().map(|d| d.message.split('"').nth(1).unwrap()).collect();
    assert_eq!(names, vec!["thing", "data", "x", "result"], "{d:?}");
}

#[test]
fn vague_name_is_quiet_for_descriptive_names() {
    let src = agent("1. 1 into invoice_rows\n2. for each ticket in crm.tickets:\n   1. log: \"{ticket}\"");
    assert!(findings(&src, "vague-name").is_empty());
}

// ------------------------------------------------------------------ deny and config

#[test]
fn denied_rules_become_errors_and_the_rest_stay_warnings() {
    let src = "# Agent: a\n- model: claude\n\n## Do go\n1. 1 into data\n";
    let file = parse("t.agent.md", src);
    let cfg = LintConfig::new(["missing-doc"]);
    let d = lint_file(&file, &cfg);
    let sev = |rule: &str| d.iter().find(|d| d.code == rule).unwrap().severity;
    assert_eq!(sev("missing-doc"), Severity::Error);
    assert_eq!(sev("vague-name"), Severity::Warning);
}

#[test]
fn config_reads_the_lint_table_of_agentmark_toml() {
    let toml = "name = \"p\"\n\n[lint]\ndeny = [\"prompt-in-loop\", \"missing-doc\"]\n\n[models]\ntest = \"mock\"\n";
    let cfg = LintConfig::from_toml_str(toml, "agentmark.toml").unwrap();
    assert!(cfg.denies("prompt-in-loop") && cfg.denies("missing-doc") && !cfg.denies("long-file"));
    assert_eq!(
        LintConfig::from_toml_str("name = \"p\"\n", "agentmark.toml").unwrap(),
        LintConfig::default()
    );
}

#[test]
fn config_errors_are_plain_and_suggest_the_rule() {
    let toml = "[lint]\ndeny = [\"missing-doc\", \"magic-valu\"]\n";
    let d = LintConfig::from_toml_str(toml, "agentmark.toml").unwrap_err();
    assert_eq!(d.span.line, 2);
    assert_eq!(d.did_you_mean.as_deref(), Some("magic-value"));
    assert!(d.render(toml).starts_with("agentmark.toml:2\n"));

    let d = LintConfig::from_toml_str("[lint]\ndeny = \"missing-doc\"\n", "agentmark.toml").unwrap_err();
    assert!(d.message.contains("list of rule names"));
    assert_eq!((d.span.line, d.span.col), (2, 8));
    // the unknown name is underlined, not the first "deny" in the file
    let toml = "# deny nothing\n[lint]\ndeny = [\"missing-doc\", \"magic-valu\"]\n";
    let d = LintConfig::from_toml_str(toml, "agentmark.toml").unwrap_err();
    assert_eq!((d.span.line, d.span.col, d.span.len), (3, 25, 10));
    let d = LintConfig::from_toml_str("name = 1\nlint = 5\n", "agentmark.toml").unwrap_err();
    assert!(d.message.contains("[lint] must be a table"));
    assert_eq!((d.span.line, d.span.col), (2, 8));
    let d = LintConfig::from_toml_str("[lint\n", "agentmark.toml").unwrap_err();
    assert_eq!(d.span.line, 1);
}

#[test]
fn every_rule_id_is_known() {
    for r in RULES {
        assert!(LintConfig::from_toml_str(&format!("[lint]\ndeny = [\"{r}\"]\n"), "t").is_ok());
    }
}

// ------------------------------------------------------------------ format and examples

#[test]
fn a_finding_is_rendered_in_the_spec_10_shape_with_its_rule_id() {
    let src = "# Agent: a\n- model: claude\n\n## Do go\n1. stop\n";
    let file = parse("agents/a.agent.md", src);
    let d = lint_file(&file, &LintConfig::default());
    let text = render_all(&d, src);
    assert_eq!(
        text,
        "agents/a.agent.md:1\n  # Agent: a\n    ^^^^^^^^\n  The agent \"a\" has no description. [missing-doc]\n  Write what it does as a plain paragraph directly under its heading."
    );
}

#[test]
fn the_invoice_chaser_project_is_clean_under_its_own_deny_list() {
    let toml = std::fs::read_to_string("examples/01-invoice-chaser/agentmark.toml").unwrap();
    let cfg = LintConfig::from_toml_str(&toml, "agentmark.toml").unwrap();
    assert!(cfg.denies("prompt-in-loop") && cfg.denies("missing-doc"));
    let mut files = Vec::new();
    for rel in [
        "main.agent.md",
        "constants.agent.md",
        "agents/invoice-chaser.agent.md",
        "records/invoice.agent.md",
        "rules/overdue.agent.md",
    ] {
        let path = format!("examples/01-invoice-chaser/{rel}");
        let src = std::fs::read_to_string(&path).unwrap();
        files.push(parse(&path, &src));
    }
    let d = lint(&files, &cfg);
    assert!(d.is_empty(), "{d:?}");
}

#[test]
fn the_support_triage_example_guards_its_model_answer_inside_the_loop() {
    let src = std::fs::read_to_string("examples/03-support-triage/agents/support-triage.agent.md").unwrap();
    let file = parse("examples/03-support-triage/agents/support-triage.agent.md", &src);
    let d = lint_file(&file, &LintConfig::new(["prompt-in-loop"]));
    assert!(d.iter().all(|d| d.code != "prompt-in-loop"));
}

#[test]
fn vague_name_checks_first_in_variables_and_route_parameters() {
    let src = "# Service: s\nSvc.\n\n- listen: 0.0.0.0:80\n\n## GET /things/{x}\n1. first thing in state.things where thing.id = x into found\n2. respond 200 found\n";
    let d = findings(src, "vague-name");
    let at: Vec<(String, u32, u32)> =
        d.iter().map(|d| (d.message.split('\"').nth(1).unwrap().to_string(), d.span.line, d.span.col)).collect();
    assert_eq!(at, vec![("x".to_string(), 5, 17), ("thing".to_string(), 6, 21)], "{d:?}");
}

#[test]
fn long_file_counts_real_lines_when_sources_are_given() {
    use agentmark::lint::lint_with_sources;
    let mut src = String::from("# Record: R\n- a: text\n");
    for _ in 0..160 {
        src.push_str("> trailing comment\n");
    }
    let file = parse("r.agent.md", &src);
    // the AST ends at line 2, so the AST-only count misses it
    assert!(lint_file(&file, &LintConfig::default()).is_empty());
    let d = lint_with_sources(&[(&file, &src)], &LintConfig::default());
    assert_eq!(d.iter().filter(|d| d.code == "long-file").count(), 1);
}
