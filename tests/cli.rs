//! Process-level tests of the `agentmark` binary (PROMPT step 7 part 2, step 8 wiring).

use std::path::Path;
use std::process::{Command, Output};

mod support;

use serde_json::Value;
use support::{copy_dir, Tmp};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn agentmark(args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_agentmark"));
    c.args(args).current_dir(ROOT);
    // tests never talk to a real model or to the network
    c.env("TMPDIR", support::compiler_tmp()).env_remove("AGENTMARK_MODEL")
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

fn temp(tag: &str) -> Tmp {
    support::temp(&format!("cli-{tag}"))
}

/// Example 05 without its approved sha2 binding (the crate is then unbound: a warning).
fn unbound_05(tag: &str) -> Tmp {
    let d = temp(tag);
    copy_dir(Path::new("examples/05-invoice-service"), &d);
    std::fs::remove_file(d.join("bindings/sha2.agent.md")).unwrap();
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
        ("examples/01-invoice-chaser", "invoice-chaser / Every day at 09:00: touches AI, crm, email, log\n"),
        ("examples/02-lead-qualifier", "lead-qualifier / When crm receives lead: touches AI, crm, email, log\n"),
        ("examples/03-support-triage", "support-triage / Every 15 minutes: touches AI, crm, http, log\n"),
        ("examples/04-daily-report", "daily-report / Every Monday at 08:00: touches AI, crm, email, log\n"),
        ("examples/05-invoice-service", "api / POST /invoices: touches state\n"),
        ("examples/06-sales-report", "sales-report: touches files, print\n"),
        ("examples/06-sales-report", "region_totals: touches nothing\n"),
    ];
    for (path, line) in cases {
        let o = run(&["check", path]);
        assert_eq!(code(&o), 0, "{path}: {}{}", out(&o), err(&o));
        assert!(out(&o).contains("What each part touches\n"), "{path}:\n{}", out(&o));
        assert!(out(&o).contains(&format!("  {line}")), "{path}:\n{}", out(&o));
        assert!(!out(&o).contains("network(") && !out(&o).contains("local(") && !out(&o).contains("effect"), "no internal classes: {path}:\n{}", out(&o));
        assert!(out(&o).contains("ok: 0 errors"), "{path}");
    }
}

#[test]
fn check_lists_rust_blocks_and_prints_warnings_in_the_spec_10_format() {
    let o = run(&["check", "examples/05-invoice-service"]);
    let so = out(&o);
    assert_eq!(code(&o), 0, "{so}{}", err(&o));
    assert!(so.contains("Rust blocks (trusted, not checked by AgentMark)"), "{so}");
    assert!(so.contains("fingerprint in invoice_fingerprint: says it touches nothing (examples/05-invoice-service/functions/invoice-fingerprint.agent.md:16)"), "{so}");
    // without the binding: file:line, the source line, a caret, the plain-language message
    let d = unbound_05("check05");
    let o = run(&["check", s(&d)]);
    let se = err(&o);
    let f = format!("{}/functions/invoice-fingerprint.agent.md", s(&d));
    assert!(se.contains(&format!("{f}:4\n  # Import: crate sha2\n")), "{se}");
    assert!(se.contains("The crate \"sha2\" has no binding yet"), "{se}");
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

const BAD_AGENT: &str = "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n1. lgo: \"hi\"\n";

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
    assert!(out(&o).contains("1 error, 0 warnings: not ok."), "{}", out(&o));
}

#[test]
fn check_missing_path_is_an_error() {
    for cmd in ["check", "fmt", "lint", "doc"] {
        let o = run(&[cmd, "does/not/exist"]);
        if cmd == "doc" {
            assert_eq!(code(&o), 2, "doc treats an unknown name as a library");
        } else {
            assert_eq!(code(&o), 2, "{cmd}");
            assert!(err(&o).contains("cannot find \"does/not/exist\""), "{cmd}: {}", err(&o));
        }
    }
}

#[test]
fn a_file_that_is_not_agentmark_source_is_refused_plainly() {
    for cmd in ["check", "fmt", "lint", "doc"] {
        let o = run(&[cmd, "tests/errors/tools.toml"]);
        assert_eq!(code(&o), 2, "{cmd}");
        assert!(err(&o).contains("is not an AgentMark file"), "{cmd}: {}", err(&o));
        assert_eq!(err(&o).lines().count(), 1, "one message: {}", err(&o));
    }
}

#[test]
fn counts_are_pluralized() {
    let d = temp("plural");
    write(&d, "tools.toml", "[log]\nverbs = []\neffect = \"local\"\n");
    write(&d, "a.agent.md", BAD_AGENT);
    assert!(out(&run(&["check", s(&d)])).contains("1 error, 0 warnings: not ok."));
    let l = lint_project("plural-lint", "name = \"p\"\n");
    assert!(out(&run(&["lint", s(&l)])).contains("lint: 0 errors, 1 warning in 1 file."), "{}", out(&run(&["lint", s(&l)])));
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
        ["diagnostics", "inferred_fields", "ok", "path", "rust_blocks", "schema_version", "units", "unverified_bindings"]
    );
    assert_eq!(v["ok"], true);
    assert_eq!(v["schema_version"], 1);
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

/// A project that has every kind of nested entry docs/check-json.md describes: a warning (an unused
/// import), an inferred field, a Rust block. Self-contained, so it does not depend on `examples/`.
const JSON_DOC_PROJECT: &str = "# Uses\n- [spare](./spare.agent.md)\n\n# Record: Order\n- amount\n- id: text\n\n# Agent: shouter\n- model: claude\n- tools: log\n\n## Rust: shout -> text\n- in: order: Order\n- touches: nothing\n\n```rust\norder.id.to_uppercase()\n```\n\n## On start\n1. shout({ id: \"a\", amount: 5 EUR }) into loud\n2. log: \"hi {loud}\"\n";

#[test]
fn check_json_nested_entries_have_the_documented_keys() {
    let d = temp("jsondoc");
    write(&d, "tools.toml", "[log]\nverbs = []\neffect = \"local\"\n");
    write(&d, "a.agent.md", JSON_DOC_PROJECT);
    write(&d, "spare.agent.md", "# Record: Spare\n- id: text\n");
    let o = run(&["check", s(&d), "--json"]);
    let v: Value = serde_json::from_str(&out(&o)).unwrap_or_else(|e| panic!("{e}: {}{}", out(&o), err(&o)));
    assert_eq!(v["ok"], true, "{v}");
    let rb = &v["rust_blocks"][0];
    assert_eq!((rb["name"].as_str(), rb["effect"].as_str()), (Some("shout"), Some("pure")));
    assert_eq!(keys(rb), ["effect", "file", "line", "name", "parent"]);
    let d = v["diagnostics"].as_array().unwrap().iter().find(|d| d["severity"] == "warning").expect("the unused import is a warning");
    assert_eq!(keys(d), ["code", "column", "did_you_mean", "file", "help", "length", "line", "message", "severity"]);
    assert!(d["message"].as_str().unwrap().starts_with("Unused"), "{d}");
    let f = &v["inferred_fields"][0];
    assert_eq!(keys(f), ["fallback", "field", "file", "line", "record", "type"]);
    assert_eq!((f["record"].as_str(), f["field"].as_str(), f["type"].as_str()), (Some("Order"), Some("amount"), Some("money")));
    let unit = &v["units"][0];
    assert_eq!(keys(unit), ["blocks", "effects", "file", "kind", "name"]);
    assert_eq!(keys(&unit["effects"]), ["class", "summary", "tools"]);
    assert_eq!(keys(&unit["effects"]["tools"]), ["local", "model", "network", "unverified"]);
}

#[test]
fn check_json_of_example_05_has_no_diagnostics() {
    let o = run(&["check", "examples/05-invoice-service", "--json"]);
    assert_eq!(code(&o), 0, "{}{}", out(&o), err(&o));
    let v: Value = serde_json::from_str(&out(&o)).unwrap();
    assert_eq!(v["diagnostics"], serde_json::json!([]));
}

#[test]
fn committed_example_connect_stubs_equal_a_freshly_generated_stub() {
    for name in ["01-invoice-chaser", "02-lead-qualifier", "03-support-triage", "04-daily-report", "05-invoice-service"] {
        let d = temp(&format!("stub-{name}"));
        copy_dir(&Path::new("examples").join(name), &d);
        let committed = std::fs::read_to_string(d.join("rust/connect.rs")).unwrap_or_else(|_| panic!("{name} has no rust/connect.rs"));
        std::fs::remove_dir_all(d.join("rust")).unwrap();
        let out_dir = temp(&format!("stub-out-{name}"));
        let o = run(&["build", s(&d), "-o", s(&out_dir), "--no-check"]);
        assert_eq!(code(&o), 0, "{}", err(&o));
        let fresh = std::fs::read_to_string(d.join("rust/connect.rs")).unwrap();
        assert_eq!(committed, fresh, "{name}: regenerate with a build after deleting examples/{name}/rust");
    }
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

/// Legal v0.4 in a non-canonical shape: spacing, `*` bullets, step numbers. (The formatter's own
/// rules are tested in tests/fmt.rs; this only needs something `fmt` has to change.)
const MESSY: &str = "# Program:   messy\nA deliberately messy file.\n\n# Record:   Invoice\nAn invoice.\n\n*  customer:   text\n* amount\n\n# Command: show\nShows it.\n\n- in:  invoice:Invoice\n- steps:\n   1.   print:   \"x\"\n   1.   print: \"y\"\n";

#[test]
fn fmt_check_fails_on_a_messy_file_and_passes_after_fmt() {
    let d = temp("fmt");
    write(&d, "messy.agent.md", MESSY);
    let before = std::fs::read_to_string(d.join("messy.agent.md")).unwrap();
    let o = run(&["fmt", "--check", s(&d)]);
    assert_eq!(code(&o), 1);
    assert!(out(&o).contains("not canonical:"), "{}", out(&o));
    assert!(out(&o).contains("messy.agent.md"));
    assert!(out(&o).contains("1 of 1 file is not canonical; run \"agentmark fmt\" to fix it."), "grammar: {}", out(&o));
    assert_eq!(std::fs::read_to_string(d.join("messy.agent.md")).unwrap(), before, "--check changes nothing");

    let o = run(&["fmt", s(&d)]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("formatted"));
    assert_ne!(std::fs::read_to_string(d.join("messy.agent.md")).unwrap(), before);

    let o = run(&["fmt", "--check", s(&d)]);
    assert_eq!(code(&o), 0, "{}", out(&o));
    assert!(out(&o).contains("1 file is canonical"));
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

const UNDOCUMENTED: &str = "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n1. log: \"hi\"\n";

fn lint_project(tag: &str, toml: &str) -> Tmp {
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
    assert!(out(&o).contains("lint: 0 errors, 1 warning"), "{}", out(&o));

    let d = lint_project("lint-deny", "name = \"p\"\n\n[lint]\ndeny = [\"missing-doc\"]\n");
    let o = run(&["lint", s(&d)]);
    assert_eq!(code(&o), 1);
    assert!(out(&o).contains("lint: 1 error, 0 warnings"), "{}", out(&o));
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
    // the title is the `# Program` name, its description comes first
    assert!(
        t.starts_with("# invoice-chaser\n\nChases unpaid invoices once a day with a polite reminder. It never changes an amount.\n\n## Agents\n"),
        "{t}"
    );
    assert!(t.contains("## Agents\n\n### invoice-chaser\n"), "{t}");
    assert!(t.contains("Chases unpaid invoices once a day. Never changes amounts; only sends reminders."), "{t}");
    assert!(t.contains("- touches: AI, crm, email, log\n"), "{t}");
    assert!(
        t.contains("invoice-chaser / Every day at 09:00: touches AI, crm, email, log\n\nFinds every overdue invoice and reminds its customer.\n"),
        "{t}"
    );
    assert!(t.contains("invoice-chaser / Do remind: touches AI, crm, email, log\n"), "{t}");
    assert!(t.contains("## Records\n\n### Invoice\n\nAn invoice as stored in the CRM.\n\n- id: text\n"), "{t}");
    assert!(t.contains("- amount: money"), "{t}");
    assert!(t.contains("## Rules\n\n### overdue\n"), "{t}");
    assert!(t.contains("overdue: touches nothing\n"), "{t}");
    assert!(!t.contains("can touch") && !t.contains("network(") && !t.contains("local("), "no internal classes:\n{t}");
}

#[test]
fn doc_of_a_service_lists_routes_with_their_effects() {
    let o = run(&["doc", "examples/05-invoice-service"]);
    let t = out(&o);
    assert!(t.contains("## Services\n\n### api\n"), "{t}");
    assert!(t.contains("- touches: state\n"), "{t}");
    assert!(t.contains("api / POST /invoices: touches state\n\nAdds the invoice in the request body."), "{t}");
    assert!(t.contains("api / GET /invoices/{id}: touches nothing\n"), "{t}");
    assert!(t.contains("## Commands\n\n### import-csv\n"), "{t}");
    assert!(t.contains("import-csv: touches files, print, state\n"), "{t}");
}

#[test]
fn doc_of_a_command_example_says_what_the_command_touches() {
    let o = run(&["doc", "examples/06-sales-report"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let t = out(&o);
    assert!(t.starts_with("# sales-report\n\nReads a sales file and writes a revenue report, region by region.\n\n## Commands\n"), "{t}");
    assert!(
        t.contains("### sales-report\n\nReads a sales CSV file and writes a revenue report per region, biggest region first.\n\nsales-report: touches files, print\n"),
        "{t}"
    );
    assert!(t.contains("### region_totals\n\nAdds up the sales of every region, biggest revenue first.\n\nregion_totals: touches nothing\n"), "{t}");
}

#[test]
fn check_of_an_agent_and_of_a_command_example_reads_as_sentences() {
    // an agent: one line per trigger and per Do block, AI first, then the tools alphabetically
    let o = run(&["check", "examples/01-invoice-chaser"]);
    assert_eq!(
        out(&o),
        "What each part touches\n  invoice-chaser / Every day at 09:00: touches AI, crm, email, log\n  invoice-chaser / Do remind: touches AI, crm, email, log\n  overdue: touches nothing\n\nok: 0 errors, 0 warnings.\n"
    );
    // a command and its functions: the `file` tool is written `files`
    let o = run(&["check", "examples/06-sales-report"]);
    assert_eq!(
        out(&o),
        "What each part touches\n  sales-report: touches files, print\n  region_totals: touches nothing\n  table_row: touches nothing\n\nok: 0 errors, 0 warnings.\n"
    );
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

#[test]
fn doc_of_an_unknown_name_says_how_to_bind() {
    let d = temp("docnone");
    let o = run(&["doc", "sha2", "--project", s(&d)]);
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("agentmark bind sha2"), "{}", err(&o));
}

// ---------------------------------------------------------------------------------- bind

/// `bind sha2` against the checked-in rustdoc fixtures and the canned model.
fn bind(project: &Path, extra: &[&str]) -> Output {
    let mut args = vec!["bind", "sha2", "--project", s(project)];
    args.extend_from_slice(extra);
    agentmark(&args)
        .env("AGENTMARK_MODEL", format!("mock:{ROOT}/tests/binder/model.json"))
        .env("AGENTMARK_RUSTDOC_JSON", format!("{ROOT}/tests/binder/sha2.json"))
        .env("AGENTMARK_RUSTDOC_JSON_DIR", format!("{ROOT}/tests/binder"))
        .env("AGENTMARK_OFFLINE", "1")
        .output()
        .unwrap()
}

const USES_SHA2: &str = "# Import: crate sha2\n\n# Command: fingerprint\nPrints a fingerprint.\n\n- in: payload: bytes\n- steps:\n  1. sha2 sha256_digest payload into digest\n  2. print: \"done\"\n";

fn sha2_project(tag: &str) -> Tmp {
    let d = temp(tag);
    write(&d, "agentmark.toml", "name = \"p\"\n");
    write(&d, "main.agent.md", "# Program: fingerprint\n\n# Uses\n- [fingerprint](./fingerprint.agent.md)\n");
    write(&d, "fingerprint.agent.md", USES_SHA2);
    d
}

#[test]
fn bind_generates_then_serves_from_cache() {
    let d = sha2_project("bind");
    let first = bind(&d, &[]);
    assert_eq!(code(&first), 0, "{}{}", out(&first), err(&first));
    assert!(out(&first).contains("sha2 0.10.9: generated"), "{}", out(&first));
    assert!(out(&first).contains("42 direct, 1 assisted, "), "{}", out(&first));
    assert!(out(&first).contains("change the line \"- effect: unverified\" to \"- effect: pure\""), "{}", out(&first));
    let file = d.join("bindings/sha2.agent.md");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("## sha256_digest(data: bytes) -> bytes"));

    let second = bind(&d, &[]);
    assert_eq!(code(&second), 0);
    assert!(out(&second).contains("sha2 0.10.9: cached"), "{}", out(&second));
    assert!(!out(&second).contains("generated"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);

    let forced = bind(&d, &["--force"]);
    assert!(out(&forced).contains("generated") && out(&forced).contains("(forced)"), "{}", out(&forced));
}

#[test]
fn bind_keeps_the_header_version_until_asked_to_move() {
    let d = sha2_project("bindpin");
    assert_eq!(code(&bind(&d, &[])), 0);
    let file = d.join("bindings/sha2.agent.md");
    let text = std::fs::read_to_string(&file).unwrap().replace("sha2 0.10.9", "sha2 0.10.8");
    std::fs::write(&file, &text).unwrap();
    // plain bind and --force stay on 0.10.8 (--force regenerates that version when the docs for it exist)
    let o = bind(&d, &[]);
    assert!(out(&o).contains("sha2 0.10.8: cached"), "{}", out(&o));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    // moving is explicit
    let o = bind(&d, &["--latest"]);
    assert!(out(&o).contains("sha2 0.10.9: generated"), "{}{}", out(&o), err(&o));
    let o = bind(&d, &["--version", "0.10.9", "--latest"]);
    assert_eq!(code(&o), 2, "--version and --latest exclude each other");
    let o = run(&["bind", "--all", "--version", "0.10.9", "--project", s(&d)]);
    assert_eq!(code(&o), 2);
}

#[test]
fn bind_all_refreshes_every_binding_and_keeps_approved_ones() {
    let d = sha2_project("bindall");
    assert_eq!(code(&bind(&d, &[])), 0);
    let file = d.join("bindings/sha2.agent.md");
    let approved = std::fs::read_to_string(&file).unwrap().replace("- effect: unverified", "- effect: pure");
    std::fs::write(&file, &approved).unwrap();
    let o = agentmark(&["bind", "--all", "--project", s(&d)])
        .env("AGENTMARK_MODEL", format!("mock:{ROOT}/tests/binder/model.json"))
        .env("AGENTMARK_RUSTDOC_JSON", format!("{ROOT}/tests/binder/sha2.json"))
        .env("AGENTMARK_RUSTDOC_JSON_DIR", format!("{ROOT}/tests/binder"))
        .env("AGENTMARK_OFFLINE", "1")
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("sha2 0.10.9: cached"), "{}", out(&o));
    assert!(out(&o).contains("Effect: pure (approved)"), "{}", out(&o));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), approved);
}

#[test]
fn bind_all_without_bindings_says_so() {
    let d = temp("bindnone");
    let o = agentmark(&["bind", "--all", "--project", s(&d)])
        .env("AGENTMARK_MODEL", format!("mock:{ROOT}/tests/binder/model.json"))
        .output()
        .unwrap();
    assert_eq!(code(&o), 0);
    assert!(out(&o).contains("No bindings"), "{}", out(&o));
}

#[test]
fn bind_without_a_model_or_a_name_is_a_usage_error() {
    let d = temp("bindmodel");
    let o = run(&["bind", "sha2", "--project", s(&d)]);
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("ANTHROPIC_API_KEY"), "{}", err(&o));
    let o = run(&["bind"]);
    assert_eq!(code(&o), 2);
    let o = agentmark(&["bind", "sha2"]).env("AGENTMARK_MODEL", "openai:gpt").env_remove("OPENAI_API_KEY").output().unwrap();
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("OPENAI_API_KEY is not set"), "{}", err(&o));
    let o = agentmark(&["bind", "sha2"]).env("AGENTMARK_MODEL", "azure:x").output().unwrap();
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("\"azure\" is not supported in this version"), "{}", err(&o));
}

#[test]
fn bind_reports_missing_library_metadata_in_plain_words() {
    let d = temp("bindmeta");
    let o = agentmark(&["bind", "sha2", "--project", s(&d)])
        .env("AGENTMARK_MODEL", format!("mock:{ROOT}/tests/binder/model.json"))
        .env("AGENTMARK_RUSTDOC_JSON", d.join("nothing.json"))
        .output()
        .unwrap();
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("cannot read the interface of \"sha2\""), "{}", err(&o));
}

#[test]
fn check_loads_the_binding_and_lists_it_until_approved() {
    let d = sha2_project("bindcheck");
    assert_eq!(code(&bind(&d, &[])), 0);

    // unverified: listed with the one line to approve, check fails ...
    let o = run(&["check", s(&d)]);
    assert_eq!(code(&o), 1, "{}{}", out(&o), err(&o));
    let t = out(&o);
    assert!(t.contains("Libraries nobody approved yet"), "{t}");
    assert!(t.contains("sha2 (library sha2): "), "{t}");
    assert!(t.contains("bindings/sha2.agent.md:6: change \"- effect: unverified\" to \"- effect: pure\""), "{t}");
    assert!(t.contains("1 unapproved library"), "{t}");
    // ... the statement `- sha2 sha256_digest payload -> digest` type-checked: no errors
    assert!(!err(&o).contains("error"), "{}", err(&o));

    // --allow-unverified lets development go on
    let o = run(&["check", s(&d), "--allow-unverified"]);
    assert_eq!(code(&o), 0);
    assert!(out(&o).contains("Libraries nobody approved yet"));

    // json lists it too
    let o = run(&["check", s(&d), "--json"]);
    assert_eq!(code(&o), 1);
    let v: Value = serde_json::from_str(&out(&o)).unwrap();
    let u = &v["unverified_bindings"][0];
    assert_eq!((u["name"].as_str(), u["crate"].as_str(), u["proposed_effect"].as_str()), (Some("sha2"), Some("sha2"), Some("pure")));
    assert_eq!(u["line"], 6);

    // approving the line makes it a trusted pure call
    let file = d.join("bindings/sha2.agent.md");
    let approved = std::fs::read_to_string(&file).unwrap().replace("- effect: unverified", "- effect: pure");
    std::fs::write(&file, approved).unwrap();
    let o = run(&["check", s(&d)]);
    assert_eq!(code(&o), 0, "{}{}", out(&o), err(&o));
    assert!(!out(&o).contains("nobody approved"));
    assert!(out(&o).contains("fingerprint: touches print"), "{}", out(&o));
}

#[test]
fn check_types_binding_calls_against_the_bound_signature() {
    let d = sha2_project("bindtype");
    assert_eq!(code(&bind(&d, &[])), 0);
    write(&d, "fingerprint.agent.md", &USES_SHA2.replace("payload: bytes", "payload: int"));
    let o = run(&["check", s(&d), "--allow-unverified"]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("\"data\" of \"sha2 sha256_digest\" must be bytes, but this is a number."), "{}", err(&o));
    // text is accepted where a binding takes bytes (Q151)
    write(&d, "fingerprint.agent.md", &USES_SHA2.replace("payload: bytes", "payload: text"));
    let o = run(&["check", s(&d), "--allow-unverified"]);
    assert_eq!(code(&o), 0, "{}{}", out(&o), err(&o));
}

#[test]
fn doc_of_a_crate_prints_the_cheat_sheet() {
    let d = sha2_project("bindsheet");
    assert_eq!(code(&bind(&d, &[])), 0);
    let o = run(&["doc", "sha2", "--project", s(&d)]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("sha256_digest"), "{}", out(&o));
}

#[test]
fn generated_bindings_pass_lint_and_fmt_are_left_alone() {
    let d = sha2_project("bindlint");
    assert_eq!(code(&bind(&d, &[])), 0);
    let file = d.join("bindings/sha2.agent.md");
    let before = std::fs::read_to_string(&file).unwrap();
    assert_eq!(code(&run(&["lint", s(&d)])), 0);
    assert_eq!(code(&run(&["fmt", s(&d)])), 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn review_fixes_text_layout() {
    // effects are indented like the other sections
    let o = run(&["check", "examples/01-invoice-chaser"]);
    assert!(out(&o).contains("\n  invoice-chaser / Every day at 09:00: touches AI,"), "{}", out(&o));
    // folder doc: title is the `# Program` name; a part that touches nothing says so
    let o = run(&["doc", "examples/05-invoice-service"]);
    assert!(out(&o).starts_with("# invoice-service\n"), "{}", out(&o));
    assert!(out(&o).contains("net_amount: touches nothing"), "{}", out(&o));
    // the author's doc comes before the generated line of a rule
    let d = temp("ruledoc");
    write(&d, "records/order.agent.md", "# Record: Order\n- amount: money\n");
    write(&d, "rules/big.agent.md", "# Uses\n- [order](../records/order.agent.md)\n\n# Rule: big Order\nBig orders.\n\n- when: amount > 5 EUR\n");
    let o = run(&["doc", s(&d)]);
    assert!(out(&o).contains("### big\n\nBig orders.\n\nApplies to Order.\n\nbig: touches nothing\n"), "{}{}", out(&o), err(&o));
}

#[test]
fn fmt_leaves_no_temp_files() {
    let d = temp("fmtatomic");
    write(&d, "messy.agent.md", MESSY);
    assert_eq!(code(&run(&["fmt", s(&d)])), 0);
    let names: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names.len(), 1, "{names:?}");
}

#[test]
fn bind_all_conflicts_with_a_name_and_refuses_misnamed_files() {
    assert_eq!(code(&run(&["bind", "sha2", "--all"])), 2);
    let d = sha2_project("bindmisnamed");
    assert_eq!(code(&bind(&d, &[])), 0);
    let dir = d.join("bindings");
    std::fs::rename(dir.join("sha2.agent.md"), dir.join("hash.agent.md")).unwrap();
    let o = agentmark(&["bind", "--all", "--project", s(&d)])
        .env("AGENTMARK_MODEL", format!("mock:{ROOT}/tests/binder/model.json"))
        .env("AGENTMARK_RUSTDOC_JSON", format!("{ROOT}/tests/binder/sha2.json"))
        .env("AGENTMARK_RUSTDOC_JSON_DIR", format!("{ROOT}/tests/binder"))
        .env("AGENTMARK_OFFLINE", "1")
        .output()
        .unwrap();
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("not named \"sha2.agent.md\""), "{}", err(&o));
    assert!(!dir.join("sha2.agent.md").exists(), "no second file");
}

#[test]
fn unverified_library_is_reported_once_in_text_and_kept_in_json_diagnostics() {
    let d = sha2_project("bindonce");
    assert_eq!(code(&bind(&d, &[])), 0);
    let o = run(&["check", s(&d)]);
    assert!(!err(&o).contains("nobody approved"), "{}", err(&o));
    assert!(out(&o).contains("Libraries nobody approved yet"));
    let v: Value = serde_json::from_str(&out(&run(&["check", s(&d), "--json"]))).unwrap();
    assert!(v["diagnostics"].as_array().unwrap().iter().any(|x| x["code"] == "W005"));
}

const PURE_FN_SHA2: &str = "# Import: crate sha2\n\n# Function: fingerprint\nHashes text.\n\n- in: s: text\n- out: bytes\n- touches: nothing\n- steps:\n  1. sha2 sha256_digest s into d\n  2. return d\n";

#[test]
fn allow_unverified_checks_effects_with_the_proposed_effect_and_warns_once() {
    let d = temp("assume");
    write(&d, "agentmark.toml", "name = \"p\"\n");
    write(&d, "main.agent.md", PURE_FN_SHA2);
    assert_eq!(code(&bind(&d, &[])), 0);
    // refused as before, and the advice is not the illegal "- effect: unverified"
    let o = run(&["check", s(&d)]);
    assert_eq!(code(&o), 1, "{}{}", out(&o), err(&o));
    let e = err(&o);
    assert!(e.contains("Approve the binding's \"- effect:\" line"), "{e}");
    assert!(!e.contains("Change the function to \"- effect: unverified\""), "{e}");
    // with the flag the proposal (pure) stands in, with one warning that says so
    let o = run(&["check", s(&d), "--allow-unverified"]);
    assert_eq!(code(&o), 0, "{}{}", out(&o), err(&o));
    let e = err(&o);
    assert_eq!(e.matches("its effect is assumed to be \"pure\" from the proposal").count(), 1, "{e}");
    assert!(e.contains("Approve it before a release build"), "{e}");
    // the JSON agrees: W008 instead of W005 for the same binding
    let v: Value = serde_json::from_str(&out(&run(&["check", s(&d), "--json", "--allow-unverified"]))).unwrap();
    let codes: Vec<&str> = v["diagnostics"].as_array().unwrap().iter().filter_map(|x| x["code"].as_str()).collect();
    assert!(codes.contains(&"W008") && !codes.contains(&"W005"), "{codes:?}");
    // build without the flag still refuses
    let o = run(&["build", s(&d), "-o", s(&d.join("out")), "--no-check"]);
    assert_eq!(code(&o), 1, "{}{}", out(&o), err(&o));
}

#[test]
fn check_binds_a_missing_crate_like_build_does() {
    // `# Import: crate sha2` -> `check` -> approve: check runs the binder first
    let d = sha2_project("checkbinds");
    let o = agentmark(&["check", s(&d)])
        .env("AGENTMARK_MODEL", format!("mock:{ROOT}/tests/binder/model.json"))
        .env("AGENTMARK_RUSTDOC_JSON", format!("{ROOT}/tests/binder/sha2.json"))
        .env("AGENTMARK_RUSTDOC_JSON_DIR", format!("{ROOT}/tests/binder"))
        .env("AGENTMARK_OFFLINE", "1")
        .output()
        .unwrap();
    assert!(err(&o).contains("binding sha2: generated"), "{}", err(&o));
    assert!(d.join("bindings/sha2.agent.md").is_file());
    let t = out(&o);
    assert!(t.contains("Libraries nobody approved yet") && t.contains("change \"- effect: unverified\" to \"- effect: pure\""), "{t}");
    assert!(!err(&o).contains("Unknown tool") && !err(&o).contains("Unused import"), "{}", err(&o));
}

#[test]
fn binding_without_supplied_rustdoc_leaves_no_scratch_crates_behind() {
    let d = sha2_project("noscratch");
    let tmp = temp("noscratch-tmp");
    // no AGENTMARK_RUSTDOC_JSON: the binder resolves the version (and tries a rustdoc build) in
    // scratch crates under $TMPDIR; whatever the outcome, nothing may stay
    for args in [vec!["check", s(&d)], vec!["bind", "sha2", "--project", s(&d)]] {
        let _ = agentmark(&args)
            .env("TMPDIR", s(&tmp))
            .env("AGENTMARK_MODEL", format!("mock:{ROOT}/tests/binder/model.json"))
            .env("AGENTMARK_OFFLINE", "1")
            .output()
            .unwrap();
        let left: Vec<_> = std::fs::read_dir(&tmp).unwrap().flatten().map(|e| e.file_name()).collect();
        assert!(left.is_empty(), "{args:?} left {left:?}");
    }
}

#[test]
fn check_that_cannot_bind_says_how_and_reports_one_error_on_the_import_line() {
    let d = sha2_project("checknobind");
    let o = agentmark(&["check", s(&d)]).env("AGENTMARK_OFFLINE", "1").output().unwrap();
    assert_eq!(code(&o), 1);
    let e = err(&o);
    assert!(e.contains("cannot bind \"sha2\""), "{e}");
    assert!(e.contains("# Import: crate sha2") && e.contains("has no binding yet, but this file uses it"), "{e}");
    assert!(e.contains("agentmark bind sha2"), "{e}");
    // the follow-ons of the missing binding are gone
    assert!(!e.contains("Unknown tool \"sha2\""), "{e}");
    assert!(!e.contains("Unused import"), "{e}");
    assert!(out(&o).contains("1 error, 0 warnings: not ok."), "{}", out(&o));
}

#[test]
fn fmt_check_counts_agree_with_the_verb() {
    let d = temp("fmtplural");
    let messy = MESSY;
    write(&d, "a.agent.md", messy);
    write(&d, "b.agent.md", messy);
    write(&d, "c.agent.md", "# Record: C\n- id: text\n");
    let o = run(&["fmt", "--check", s(&d)]);
    assert!(out(&o).contains("2 of 3 files are not canonical; run \"agentmark fmt\" to fix them."), "{}", out(&o));
    std::fs::remove_file(d.join("b.agent.md")).unwrap();
    let o = run(&["fmt", "--check", s(&d)]);
    assert!(out(&o).contains("1 of 2 files is not canonical; run \"agentmark fmt\" to fix it."), "{}", out(&o));
}

// ---------------------------------------------------------------------------------- misc

#[test]
fn help_and_unknown_commands() {
    let o = run(&["--help"]);
    assert_eq!(code(&o), 0);
    for c in ["check", "fmt", "lint", "doc", "bind"] {
        assert!(out(&o).contains(c), "help lists {c}");
    }
    assert_eq!(code(&run(&["frobnicate"])), 2);
    assert_eq!(code(&run(&[])), 2);
}

#[test]
fn doc_of_a_path_like_target_says_cannot_find() {
    for t in ["nope/sheet", "nope.agent.md"] {
        let o = run(&["doc", t]);
        assert_eq!(code(&o), 2);
        assert!(err(&o).contains(&format!("cannot find \"{t}\"")), "{}", err(&o));
    }
}

#[cfg(unix)]
#[test]
fn fmt_keeps_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let d = temp("fmtperm");
    let f = d.join("messy.agent.md");
    write(&d, "messy.agent.md", MESSY);
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert_eq!(code(&run(&["fmt", s(&d)])), 0);
    assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o640);
}

#[test]
fn dot_is_a_project_folder_for_check_and_lint() {
    let d = temp("dot");
    write(&d, "agentmark.toml", "name = \"p\"\nversion = \"0.1.0\"\ntargets = [\"rust\"]\n");
    write(&d, "tools.toml", "[log]\nverbs = []\neffect = \"local\"\n");
    write(&d, "records/a.agent.md", "# Record: A\nA.\n- id: text\n");
    for cmd in ["check", "lint"] {
        let mut c = agentmark(&[cmd, "."]);
        let o = c.current_dir(&d).output().unwrap();
        assert_eq!(code(&o), 0, "{cmd} .: {}{}", out(&o), err(&o));
        assert!(!err(&o).contains("Cannot find"), "{cmd} .: {}", err(&o));
    }
}

// ---- [rust.dependencies] (Q222) -----------------------------------------------------------

#[test]
fn rust_dependencies_that_clash_with_a_generated_one_are_a_plain_error() {
    let d = temp("deps-clash");
    copy_dir(Path::new("examples/01-invoice-chaser"), &d);
    let toml = std::fs::read_to_string(d.join("agentmark.toml")).unwrap();
    write(&d, "agentmark.toml", &format!("{toml}\n[rust.dependencies]\nserde_json = \"1\"\n"));
    let o = run(&["build", s(&d), "-o", s(&d.join("out")), "--no-check"]);
    assert_eq!(code(&o), 2, "{}", err(&o));
    assert!(err(&o).contains("[rust.dependencies] lists \"serde_json\", but the generated project already depends on it"), "{}", err(&o));
    assert!(!d.join("out").exists());
}

#[test]
fn rust_dependencies_are_merged_into_cargo_toml() {
    let d = temp("deps-merge");
    copy_dir(Path::new("examples/01-invoice-chaser"), &d);
    let toml = std::fs::read_to_string(d.join("agentmark.toml")).unwrap();
    write(&d, "agentmark.toml", &format!("{toml}\n[rust.dependencies]\nreqwest = {{ version = \"0.12\", default-features = false }}\nfastrand = \"2\"\n"));
    let o = run(&["build", s(&d), "-o", s(&d.join("out")), "--no-check"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let cargo = std::fs::read_to_string(d.join("out/Cargo.toml")).unwrap();
    assert!(cargo.contains("fastrand = \"2\"\n") && cargo.contains("reqwest = { default-features = false, version = \"0.12\" }"), "{cargo}");
}

#[test]
fn a_malformed_rust_table_is_reported_as_an_agentmark_toml_error() {
    let d = temp("deps-bad");
    copy_dir(Path::new("examples/01-invoice-chaser"), &d);
    write(&d, "agentmark.toml", "name = \"x\"\n[rust]\ndependenceis = {}\n");
    let o = run(&["check", s(&d)]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("Unknown key \"dependenceis\" under [rust] in agentmark.toml."), "{}", err(&o));
}
