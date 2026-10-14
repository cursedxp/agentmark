//! Process-level tests of the `agentmark` binary (PROMPT step 7 part 2, step 8 wiring).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn agentmark(args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_agentmark"));
    c.args(args).current_dir(ROOT);
    // tests never talk to a real model or to the network
    c.env_remove("AGENTMARK_MODEL")
        .env_remove("AGENTMARK_RUSTDOC_JSON")
        .env_remove("AGENTMARK_RUSTDOC_JSON_DIR")
        .env_remove("ANTHROPIC_API_KEY");
    c
}

fn run(args: &[&str]) -> Output {
    agentmark(args).output().unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap()
}

fn temp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("agentmark-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

// ---------------------------------------------------------------------------------- check

#[test]
fn check_every_example_prints_its_effect_summary() {
    let cases = [
        (
            "examples/01-invoice-chaser",
            "invoice-chaser / Every day at 09:00: model, network(crm, email), local(log)\n",
        ),
        (
            "examples/02-lead-qualifier.agent.md",
            "lead-qualifier / When crm receives lead: model, network(crm, email), local(log)\n",
        ),
        (
            "examples/03-support-triage.agent.md",
            "support-triage / Every 15 minutes: model, network(crm, http), local(log)\n",
        ),
        (
            "examples/04-daily-report.agent.md",
            "daily-report / Every Monday at 08:00: model, network(crm, email), local(log)\n",
        ),
        ("examples/05-invoice-service.agent.md", "api / POST /invoices: local(state)\n"),
    ];
    for (path, line) in cases {
        let o = run(&["check", path]);
        assert_eq!(code(&o), 0, "{path}: {}{}", out(&o), err(&o));
        assert!(out(&o).contains(line), "{path}:\n{}", out(&o));
        assert!(out(&o).contains("ok: 0 errors"), "{path}");
    }
}

#[test]
fn check_lists_rust_blocks_and_prints_warnings_in_the_spec_10_format() {
    let o = run(&["check", "examples/05-invoice-service.agent.md"]);
    let (so, se) = (out(&o), err(&o));
    assert!(so.contains("Rust blocks (trusted, not checked by AgentMark)"), "{so}");
    assert!(so.contains("fingerprint in net_amount: declared pure (examples/05-invoice-service.agent.md:"), "{so}");
    // unused/unbound warnings: file:line, the source line, a caret
    assert!(se.contains("examples/05-invoice-service.agent.md:4\n  # Import: crate sha2\n"), "{se}");
    assert!(se.contains("The crate \"sha2\" has no binding yet."), "{se}");
    assert!(!se.to_lowercase().contains("rustc"));
}

#[test]
fn check_shows_inferred_field_types() {
    let d = temp("infer");
    write(&d, "a.agent.md", "# Record: Order\n- amount\n- due_date\n- note: text\n");
    let o = run(&["check", s(&d)]);
    assert!(out(&o).contains("Field types guessed from names"), "{}", out(&o));
    assert!(out(&o).contains("Order.amount: money (from the field name;"), "{}", out(&o));
    assert!(out(&o).contains("Order.due_date: date"), "{}", out(&o));
    assert!(!out(&o).contains("Order.note"), "written types are not guessed");
}

const BAD_AGENT: &str = "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- lgo: \"hi\"\n";

#[test]
fn check_errors_exit_1_with_did_you_mean_and_no_rust() {
    let d = temp("bad");
    write(&d, "tools.toml", "[log]\nverbs = []\neffect = \"local\"\n");
    write(&d, "a.agent.md", BAD_AGENT);
    let o = run(&["check", s(&d)]);
    assert_eq!(code(&o), 1);
    let se = err(&o);
    assert!(se.contains("a.agent.md:6"), "{se}");
    assert!(se.contains("Did you mean \"log\"?"), "{se}");
    assert!(out(&o).contains("1 errors, 0 warnings: not ok."), "{}", out(&o));
}

#[test]
fn check_missing_path_is_an_error() {
    let o = run(&["check", "does/not/exist"]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("Cannot find"), "{}", err(&o));
}

fn keys(v: &Value) -> Vec<&str> {
    let mut k: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    k.sort();
    k
}

#[test]
fn check_json_has_the_documented_keys() {
    let o = run(&["check", "examples/01-invoice-chaser", "--json"]);
    assert_eq!(code(&o), 0);
    let v: Value = serde_json::from_str(&out(&o)).expect("one JSON document on stdout");
    assert_eq!(
        keys(&v),
        ["diagnostics", "inferred_fields", "ok", "path", "rust_blocks", "units", "unverified_bindings"]
    );
    assert_eq!(v["ok"], true);
    let units = v["units"].as_array().unwrap();
    let agent = units.iter().find(|u| u["name"] == "invoice-chaser").unwrap();
    assert_eq!(agent["kind"], "agent");
    assert_eq!(keys(&agent["blocks"][0]), ["effects", "file", "label", "line"]);
    let first = &agent["blocks"][0];
    assert_eq!(first["label"], "Every day at 09:00");
    assert_eq!(first["effects"]["class"], "model");
    assert_eq!(first["effects"]["summary"], "model, network(crm, email), local(log)");
    assert_eq!(first["effects"]["tools"]["network"], serde_json::json!(["crm", "email"]));
    assert_eq!(first["effects"]["tools"]["local"], serde_json::json!(["log"]));
    assert_eq!(first["effects"]["tools"]["model"], true);
    assert!(units.iter().any(|u| u["name"] == "overdue" && u["kind"] == "rule"));
}

#[test]
fn check_json_reports_rust_blocks_and_diagnostics() {
    let o = run(&["check", "examples/05-invoice-service.agent.md", "--json"]);
    let v: Value = serde_json::from_str(&out(&o)).unwrap();
    let rb = &v["rust_blocks"][0];
    assert_eq!((rb["name"].as_str(), rb["effect"].as_str()), (Some("fingerprint"), Some("pure")));
    assert_eq!(keys(rb), ["effect", "file", "line", "name", "parent"]);
    let d = &v["diagnostics"][0];
    assert_eq!(d["severity"], "warning");
    assert_eq!(keys(d), ["code", "column", "did_you_mean", "file", "help", "length", "line", "message", "severity"]);
}

#[test]
fn check_json_of_a_broken_project_still_parses() {
    let d = temp("badjson");
    write(&d, "tools.toml", "[log]\nverbs = []\neffect = \"local\"\n");
    write(&d, "a.agent.md", BAD_AGENT);
    let o = run(&["check", s(&d), "--json"]);
    assert_eq!(code(&o), 1);
    let v: Value = serde_json::from_str(&out(&o)).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["diagnostics"][0]["severity"], "error");
    assert_eq!(v["diagnostics"][0]["did_you_mean"], "log");
    assert_eq!(v["units"], serde_json::json!([]));
}

// ---------------------------------------------------------------------------------- fmt

#[test]
fn fmt_check_fails_on_a_messy_file_and_passes_after_fmt() {
    let d = temp("fmt");
    std::fs::copy(Path::new(ROOT).join("tests/fmt/messy.agent.md"), d.join("messy.agent.md")).unwrap();
    let before = std::fs::read_to_string(d.join("messy.agent.md")).unwrap();
    let o = run(&["fmt", "--check", s(&d)]);
    assert_eq!(code(&o), 1);
    assert!(out(&o).contains("not canonical:"), "{}", out(&o));
    assert!(out(&o).contains("messy.agent.md"));
    assert_eq!(std::fs::read_to_string(d.join("messy.agent.md")).unwrap(), before, "--check changes nothing");

    let o = run(&["fmt", s(&d)]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("formatted"));
    assert_ne!(std::fs::read_to_string(d.join("messy.agent.md")).unwrap(), before);

    let o = run(&["fmt", "--check", s(&d)]);
    assert_eq!(code(&o), 0, "{}", out(&o));
    assert!(out(&o).contains("1 files are canonical"));
}

#[test]
fn fmt_reports_syntax_errors_and_leaves_the_file() {
    let d = temp("fmtbad");
    write(&d, "a.agent.md", "# Agent: a\n\t- model: claude\n");
    let o = run(&["fmt", s(&d)]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("a.agent.md:2"), "{}", err(&o));
    assert_eq!(std::fs::read_to_string(d.join("a.agent.md")).unwrap(), "# Agent: a\n\t- model: claude\n");
}

#[test]
fn fmt_of_a_missing_path_is_a_usage_error() {
    let o = run(&["fmt", "nope/nothing"]);
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("cannot find"));
}

// ---------------------------------------------------------------------------------- lint

const UNDOCUMENTED: &str = "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- log: \"hi\"\n";

fn lint_project(tag: &str, toml: &str) -> PathBuf {
    let d = temp(tag);
    write(&d, "tools.toml", "[log]\nverbs = []\neffect = \"local\"\n");
    write(&d, "agentmark.toml", toml);
    write(&d, "a.agent.md", UNDOCUMENTED);
    d
}

#[test]
fn lint_warns_by_default_and_fails_for_denied_rules() {
    let d = lint_project("lint-warn", "name = \"p\"\n");
    let o = run(&["lint", s(&d)]);
    assert_eq!(code(&o), 0, "{}{}", out(&o), err(&o));
    assert!(err(&o).contains("a.agent.md:1"), "{}", err(&o));
    assert!(out(&o).contains("lint: 0 errors, 1 warnings"), "{}", out(&o));

    let d = lint_project("lint-deny", "name = \"p\"\n\n[lint]\ndeny = [\"missing-doc\"]\n");
    let o = run(&["lint", s(&d)]);
    assert_eq!(code(&o), 1);
    assert!(out(&o).contains("lint: 1 errors, 0 warnings"), "{}", out(&o));
}

#[test]
fn lint_passes_the_invoice_chaser_example() {
    let o = run(&["lint", "examples/01-invoice-chaser"]);
    assert_eq!(code(&o), 0, "{}{}", out(&o), err(&o));
}

// ---------------------------------------------------------------------------------- doc

#[test]
fn doc_collects_descriptions_and_effects() {
    let o = run(&["doc", "examples/01-invoice-chaser"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let t = out(&o);
    assert!(t.contains("# invoice-chaser"), "{t}");
    assert!(t.contains("## Agents\n\n### invoice-chaser\n"), "{t}");
    assert!(t.contains("Chases unpaid invoices once a day. Never changes amounts; only sends reminders."), "{t}");
    assert!(t.contains("- can touch: model, network(crm, email), local(log)"), "{t}");
    assert!(t.contains("**Every day at 09:00** (can touch: model, network(crm, email), local(log))"), "{t}");
    assert!(t.contains("## Records\n\n### Invoice\n\nAn invoice as stored in the CRM.\n\n- id: text\n"), "{t}");
    assert!(t.contains("- amount: money"), "{t}");
    assert!(t.contains("## Rules\n\n### overdue\n"), "{t}");
}

#[test]
fn doc_of_a_service_lists_routes_with_their_effects() {
    let o = run(&["doc", "examples/05-invoice-service.agent.md"]);
    let t = out(&o);
    assert!(t.contains("## Services\n\n### api\n"), "{t}");
    assert!(t.contains("**POST /invoices** (can touch: local(state))"), "{t}");
    assert!(t.contains("## Commands\n\n### import-csv\n"), "{t}");
}

#[test]
fn doc_refuses_a_project_with_errors() {
    let d = temp("docbad");
    write(&d, "tools.toml", "[log]\nverbs = []\neffect = \"local\"\n");
    write(&d, "a.agent.md", BAD_AGENT);
    let o = run(&["doc", s(&d)]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("a.agent.md:6"));
}

// ---------------------------------------------------------------------------------- misc

#[test]
fn help_and_unknown_commands() {
    let o = run(&["--help"]);
    assert_eq!(code(&o), 0);
    for c in ["check", "fmt", "lint", "doc"] {
        assert!(out(&o).contains(c), "help lists {c}");
    }
    assert_eq!(code(&run(&["frobnicate"])), 2);
    assert_eq!(code(&run(&[])), 2);
}
