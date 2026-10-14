//! Rust codegen, build/run/trace (PROMPT steps 6 and 7): the generated programs are built with cargo
//! and judged by their traces, never by their text.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// All generated projects share one target folder, so dependencies compile once.
fn gen_target() -> String {
    format!("{ROOT}/target/gen")
}

fn agentmark(args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_agentmark"));
    c.args(args).current_dir(ROOT);
    c.env("CARGO_TARGET_DIR", gen_target())
        .env("AGENTMARK_OFFLINE", "1")
        .env_remove("AGENTMARK_MODEL")
        .env_remove("AGENTMARK_RUSTDOC_JSON")
        .env_remove("AGENTMARK_RUSTDOC_JSON_DIR")
        .env_remove("ANTHROPIC_API_KEY");
    c
}

fn bind_env(c: &mut Command) {
    c.env("AGENTMARK_MODEL", format!("mock:{ROOT}/tests/binder/model.json"))
        .env("AGENTMARK_RUSTDOC_JSON", format!("{ROOT}/tests/binder/sha2.json"))
        .env("AGENTMARK_RUSTDOC_JSON_DIR", format!("{ROOT}/tests/binder"));
}

fn temp(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("agentmark-codegen-{tag}-{}-{n}", std::process::id()));
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

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn parse_lines(text: &str) -> Vec<Value> {
    text.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON: {l}: {e}"))).collect()
}

/// `agentmark trace <path> --mock --fixtures <dir>`: the trace lines.
fn trace_of(path: &str, fixtures: &Path) -> Vec<Value> {
    let o = agentmark(&["trace", path, "--mock", "--fixtures", s(fixtures)]).output().unwrap();
    assert!(o.status.success(), "trace failed:\n{}", stderr(&o));
    parse_lines(&stdout(&o))
}

fn tools_toml() -> String {
    std::fs::read_to_string(format!("{ROOT}/examples/tools.toml")).unwrap()
}

/// A single-file project (`name.agent.md` + tools.toml) in a temp folder; returns the file path.
fn single(tag: &str, name: &str, source: &str) -> (PathBuf, PathBuf) {
    let d = temp(tag);
    write(&d, name, source);
    write(&d, "tools.toml", &tools_toml());
    (d.clone(), d.join(name))
}

fn model_json(responses: &[(&str, &str)]) -> String {
    json!({
        "today": "2026-10-13",
        "responses": responses.iter().map(|(p, r)| json!({"prompt_prefix": p, "response": r})).collect::<Vec<_>>()
    })
    .to_string()
}

// ---------------------------------------------------------------------------------- 01

#[test]
fn example_01_reproduces_the_expected_trace() {
    let expected = parse_lines(&std::fs::read_to_string(format!("{ROOT}/fixtures/01-invoice-chaser/expected.trace.jsonl")).unwrap());
    let got = trace_of(&format!("{ROOT}/examples/01-invoice-chaser"), Path::new(&format!("{ROOT}/fixtures/01-invoice-chaser")));
    assert_eq!(got.len(), expected.len(), "got:\n{got:#?}");
    for (i, (g, e)) in got.iter().zip(&expected).enumerate() {
        assert_eq!(g, e, "trace line {}", i + 1);
    }
}

#[test]
fn build_writes_the_spec_layout_and_the_project_compiles_warning_free() {
    let out = temp("layout");
    let o = agentmark(&["build", &format!("{ROOT}/examples/01-invoice-chaser"), "--target", "rust", "-o", s(&out)]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    for f in ["Cargo.toml", "src/main.rs", "src/records.rs", "src/rules.rs", "src/tools.rs", "src/bindings.rs", "src/agent_invoice_chaser.rs"] {
        assert!(out.join(f).is_file(), "missing {f}");
    }
    let agent = std::fs::read_to_string(out.join("src/agent_invoice_chaser.rs")).unwrap();
    assert!(agent.contains("// Effects: model, network(crm, email), local(log)"), "{agent}");
    assert!(agent.contains("agents/invoice-chaser.agent.md:15"), "source-map comments name the .agent.md line");
    assert!(std::fs::read_to_string(out.join("Cargo.toml")).unwrap().contains("agentmark-rt"));
    let b = Command::new("cargo")
        .args(["build", "--offline", "--manifest-path"])
        .arg(out.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", gen_target())
        .output()
        .unwrap();
    assert!(b.status.success(), "{}", stderr(&b));
    assert!(!stderr(&b).contains("warning"), "generated code must be warning free:\n{}", stderr(&b));
}

#[test]
fn run_prints_the_log_and_trace_prints_json_lines_only() {
    let fx = format!("{ROOT}/fixtures/01-invoice-chaser");
    let o = agentmark(&["run", &format!("{ROOT}/examples/01-invoice-chaser"), "--mock", "--fixtures", &fx]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("Reminded Müller GmbH about 1200.00 EUR"));
    let t = agentmark(&["trace", &format!("{ROOT}/examples/01-invoice-chaser"), "--mock", "--fixtures", &fx]).output().unwrap();
    assert_eq!(parse_lines(&stdout(&t)).len(), 6);
}

#[test]
fn mock_without_fixtures_is_asked_for_them() {
    let o = agentmark(&["run", &format!("{ROOT}/examples/01-invoice-chaser"), "--mock"]).output().unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("--fixtures"), "{}", stderr(&o));
}

// ---------------------------------------------------------------------------------- 02-04

#[test]
fn example_02_lead_qualifier_runs_once_per_lead() {
    let fx = temp("fx02");
    write(&fx, "crm.leads.json", r#"[{"id":"L1","company":"Acme","message":"hi","email":"a@acme.com"},{"id":"L2","company":"Tiny","message":"hm","email":"t@tiny.com"}]"#);
    write(&fx, "model.json", &model_json(&[("Score this inbound lead", r#"{"score": 9, "reason": "big budget"}"#)]));
    let t = trace_of(&format!("{ROOT}/examples/02-lead-qualifier.agent.md"), &fx);
    let kinds: Vec<String> = t.iter().map(|l| format!("{}:{}", l["kind"].as_str().unwrap(), l["tool"].as_str().unwrap_or(l["name"].as_str().unwrap_or("")))).collect();
    assert_eq!(t.len(), 8, "{kinds:?}");
    assert_eq!(t[0]["context"], json!({"id": "L1"}));
    assert_eq!(t[1]["name"], "q matches Qualification");
    assert_eq!(t[1]["result"], "pass");
    assert_eq!(t[2], json!({"kind":"tool","tool":"crm","verb":"update","source":"leads","id":"L1","set":{"score":9}}));
    assert_eq!(t[3], json!({"kind":"tool","tool":"email","verb":"send","target":"address","to":"sales@optimotion.dev"}));
    assert_eq!(t[4]["context"], json!({"id": "L2"}));
}

#[test]
fn example_02_a_model_answer_that_does_not_fit_fails_the_matches_guard() {
    let fx = temp("fx02b");
    write(&fx, "crm.leads.json", r#"[{"id":"L1","company":"Acme","message":"hi","email":"a@acme.com"}]"#);
    write(&fx, "model.json", &model_json(&[("Score this inbound lead", "I think it is a good lead")]));
    let t = trace_of(&format!("{ROOT}/examples/02-lead-qualifier.agent.md"), &fx);
    assert_eq!(t.len(), 2, "{t:#?}");
    assert_eq!(t[1]["result"], "fail");
}

#[test]
fn example_03_support_triage_stops_the_whole_block_on_a_triaged_ticket() {
    let fx = temp("fx03");
    write(&fx, "crm.tickets.json", r#"[{"id":"T1","subject":"s","body":"b"},{"id":"T2","subject":"s","body":"b","priority":"low"},{"id":"T3","subject":"s","body":"b"}]"#);
    write(&fx, "model.json", &model_json(&[("Classify priority", r#"{"priority":"high","category":"billing"}"#)]));
    let t = trace_of(&format!("{ROOT}/examples/03-support-triage.agent.md"), &fx);
    // T1 is triaged, T2 already has a priority: `stop` ends the block (Q3), T3 is not looked at
    assert_eq!(t.len(), 5, "{t:#?}");
    assert_eq!(t[2]["id"], "T1");
    assert_eq!(t[3], json!({"kind":"tool","tool":"http","verb":"post","url":"https://hooks.example.com/alert","body":{"ticket":"T1","category":"billing"}}));
    assert_eq!(t[4], json!({"kind":"tool","tool":"log","message":"Triaged T1 as high/billing"}));
}

#[test]
fn example_03_an_unknown_priority_fails_the_guard_and_stops() {
    let fx = temp("fx03b");
    write(&fx, "crm.tickets.json", r#"[{"id":"T1","subject":"s","body":"b"}]"#);
    write(&fx, "model.json", &model_json(&[("Classify priority", r#"{"priority":"urgent","category":"other"}"#)]));
    let t = trace_of(&format!("{ROOT}/examples/03-support-triage.agent.md"), &fx);
    assert_eq!(t.len(), 2);
    assert_eq!(t[1]["result"], "fail");
}

#[test]
fn example_04_daily_report_counts_with_the_rules() {
    let fx = temp("fx04");
    write(&fx, "crm.invoices.json", r#"[{"id":"I1","customer":"A","amount":"10.00 EUR","due":"2026-01-01","status":"open"},{"id":"I2","customer":"B","amount":"5.00 EUR","due":"2026-10-12","status":"open"},{"id":"I3","customer":"C","amount":"5.00 EUR","due":"2026-01-01","status":"paid"}]"#);
    write(&fx, "model.json", &model_json(&[("Write a two-sentence Monday summary. Open invoices: 2. Overdue: 1.", "Two are open, one is late.")]));
    let t = trace_of(&format!("{ROOT}/examples/04-daily-report.agent.md"), &fx);
    assert_eq!(t[0]["prompt_prefix"], "Write a two-sentence Monday summary. Open invoices: 2. Overdue: 1.");
    assert_eq!(t[1], json!({"kind":"guard","agent":"daily-report","name":"summary is not empty","result":"pass"}));
    assert_eq!(t[3], json!({"kind":"tool","tool":"log","message":"Report sent: 2 open, 1 overdue"}));
}

#[test]
fn a_missing_model_answer_is_reported_not_swallowed() {
    let fx = temp("fx04b");
    write(&fx, "crm.invoices.json", "[]");
    write(&fx, "model.json", &model_json(&[]));
    let o = agentmark(&["trace", &format!("{ROOT}/examples/04-daily-report.agent.md"), "--mock", "--fixtures", s(&fx)]).output().unwrap();
    // the block fails, the run says so, and the trace records the error
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    let t = parse_lines(&stdout(&o));
    assert_eq!(t.last().unwrap()["kind"], "error");
    assert_eq!(t.last().unwrap()["block"], "Every Monday at 08:00");
}

// ---------------------------------------------------------------------------------- 05

/// Example 05 in a temp folder, with the sha2 binding generated from the canned rustdoc.
fn project_05() -> (PathBuf, PathBuf) {
    let d = temp("p05");
    std::fs::copy(format!("{ROOT}/examples/05-invoice-service.agent.md"), d.join("05-invoice-service.agent.md")).unwrap();
    write(&d, "tools.toml", &tools_toml());
    (d.clone(), d.join("05-invoice-service.agent.md"))
}

fn approve_sha2(dir: &Path) {
    let p = dir.join("bindings/sha2.agent.md");
    let t = std::fs::read_to_string(&p).unwrap().replace("- effect: unverified", "- effect: pure");
    std::fs::write(p, t).unwrap();
}

#[test]
fn build_binds_imported_crates_and_refuses_unapproved_ones() {
    let (d, file) = project_05();
    let mut c = agentmark(&["build", s(&file), "-o", s(&d.join("out"))]);
    bind_env(&mut c);
    let o = c.output().unwrap();
    assert_eq!(o.status.code(), Some(1));
    let e = stderr(&o);
    assert!(e.contains("binding sha2: generated"), "{e}");
    assert!(e.contains("not approved") && e.contains("--allow-unverified"), "{e}");
    assert!(!d.join("out").exists(), "nothing is written for a refused build");

    // approved: builds, and the second build finds the cached binding
    approve_sha2(&d);
    let o = agentmark(&["build", s(&file), "-o", s(&d.join("out"))]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("binding sha2: cached"), "{}", stderr(&o));
    let cargo = std::fs::read_to_string(d.join("out/Cargo.toml")).unwrap();
    assert!(cargo.contains("sha2 = \"=0.10"), "{cargo}");
    for f in ["src/service_api.rs", "src/command_import_csv.rs"] {
        assert!(d.join("out").join(f).is_file(), "{f}");
    }
}

#[test]
fn allow_unverified_builds_for_development() {
    let (d, file) = project_05();
    let mut c = agentmark(&["build", s(&file), "-o", s(&d.join("out")), "--allow-unverified"]);
    bind_env(&mut c);
    let o = c.output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
}

fn built_05() -> (PathBuf, PathBuf) {
    let (d, file) = project_05();
    let mut c = agentmark(&["build", s(&file), "-o", s(&d.join("out")), "--allow-unverified"]);
    bind_env(&mut c);
    assert!(c.output().unwrap().status.success());
    let b = Command::new("cargo")
        .args(["build", "--offline", "--manifest-path"])
        .arg(d.join("out/Cargo.toml"))
        .env("CARGO_TARGET_DIR", gen_target())
        .output()
        .unwrap();
    assert!(b.status.success(), "{}", stderr(&b));
    assert!(!stderr(&b).contains("warning"), "{}", stderr(&b));
    let bin = gen_bin("agentmark-05-invoice-service");
    (d, bin)
}

fn gen_bin(name: &str) -> PathBuf {
    PathBuf::from(gen_target()).join("debug").join(name)
}

const STATE: &str = r#"[{"id":"INV-1","customer":"Acme","amount":"100.00 EUR","due":"2026-01-01","status":"open"}]"#;

#[test]
fn service_answers_requests_from_the_fixture_and_traces_them() {
    let (d, bin) = built_05();
    let fx = d.join("fx");
    write(&fx, "model.json", &model_json(&[]));
    write(&fx, "state.invoices.json", STATE);
    write(
        &fx,
        "requests.json",
        r#"[{"method":"GET","path":"/invoices/INV-1"},{"method":"GET","path":"/invoices/NOPE"},
            {"method":"POST","path":"/invoices","body":{"id":"INV-2","customer":"B","amount":"5.00 EUR","due":"2026-02-02","status":"open"}},
            {"method":"GET","path":"/invoices/INV-2"},
            {"method":"POST","path":"/invoices","body":{"id":"INV-3","customer":"B","amount":"0.00 EUR","due":"2026-02-02","status":"open"}}]"#,
    );
    let o = Command::new(bin).args(["--once", "--mock", "--fixtures", s(&fx), "--trace", "-"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let t = parse_lines(&stdout(&o));
    let http: Vec<(String, String, u64)> = t
        .iter()
        .filter(|l| l["kind"] == "http")
        .map(|l| (l["method"].as_str().unwrap().into(), l["path"].as_str().unwrap().into(), l["status"].as_u64().unwrap()))
        .collect();
    assert_eq!(
        http,
        vec![
            ("GET".into(), "/invoices/INV-1".into(), 200),
            ("GET".into(), "/invoices/NOPE".into(), 404),
            ("POST".into(), "/invoices".into(), 201),
            ("GET".into(), "/invoices/INV-2".into(), 200),
            ("POST".into(), "/invoices".into(), 400),
        ]
    );
    // the guard line comes before the http line of its request
    assert_eq!(t[0]["kind"], "guard");
    assert_eq!(t[0]["agent"], "api");
    assert_eq!(t[1]["kind"], "http");
}

fn http_get(addr: &str, path: &str) -> String {
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

#[test]
fn service_serves_a_real_get() {
    let (d, bin) = built_05();
    let fx = d.join("fx");
    write(&fx, "model.json", &model_json(&[]));
    write(&fx, "state.invoices.json", STATE);
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let addr = format!("127.0.0.1:{port}");
    let trace = d.join("trace.jsonl");
    let mut child = Command::new(bin)
        .args(["--mock", "--fixtures", s(&fx), "--serve", &addr, "--trace", s(&trace)])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut reply = String::new();
    for _ in 0..50 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if std::net::TcpStream::connect(&addr).is_ok() {
            reply = http_get(&addr, "/invoices/INV-1/net");
            break;
        }
    }
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    let body: Value = serde_json::from_str(reply.split("\r\n\r\n").nth(1).unwrap().trim()).unwrap();
    assert_eq!(body["id"], "INV-1");
    assert_eq!(body["net"], "81.00 EUR");
    assert_eq!(body["fingerprint"].as_str().unwrap().len(), 64);
    let t = parse_lines(&std::fs::read_to_string(trace).unwrap());
    assert_eq!(t.last().unwrap(), &json!({"kind":"http","method":"GET","path":"/invoices/INV-1/net","status":200}));
}

#[test]
fn command_reads_a_csv_and_reports_a_missing_file() {
    let (d, bin) = built_05();
    let fx = d.join("fx");
    write(&fx, "model.json", &model_json(&[]));
    write(&fx, "rows.csv", "id,customer,amount,due,status\nA1,X,10.00 EUR,2026-01-01,open\nA2,Y,20.00 EUR,2026-01-02,open\n");
    let run = |file: &str, extra: &[&str]| {
        let mut args = vec!["--mock", "--fixtures", s(&fx), "--trace", "-", "import-csv", file];
        args.extend_from_slice(extra);
        let o = Command::new(&bin).args(&args).output().unwrap();
        assert!(o.status.success(), "{}", stderr(&o));
        parse_lines(&stdout(&o))
    };
    assert_eq!(run("rows.csv", &[]), vec![json!({"kind":"print","message":"Imported 2 rows"})]);
    assert_eq!(run("nope.csv", &[]), vec![json!({"kind":"print","message":"Could not read nope.csv"})]);
    // command.json drives the same command in a --once mock run
    write(&fx, "command.json", r#"{"command":"import-csv","args":{"path":"rows.csv","dry_run":true}}"#);
    let o = Command::new(&bin).args(["--once", "--mock", "--fixtures", s(&fx), "--trace", "-"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(parse_lines(&stdout(&o)), vec![json!({"kind":"print","message":"Imported 2 rows"})]);
}

// ---------------------------------------------------------------------------------- names, errors

#[test]
fn colliding_names_get_distinct_rust_names() {
    let src = r#"# Record: Invoice
- id: text
- customer: text

# Function: invoice_chaser__remind
- in: invoice: Invoice
- out: text
- effect: pure
  - return "user {invoice.id}"

# Function: net-amount
- in: invoice: Invoice
- out: text
- effect: pure
  - return "dash"

# Function: net_amount
- in: invoice: Invoice
- out: text
- effect: pure
  - return "underscore"

# Agent: invoice-chaser
- model: claude
- tools: log

## Every 5 minutes
- set a = invoice_chaser__remind(first_invoice())
- log: "never"

## Do remind
- log: "do block"
"#;
    // keep the program simple: only the agent and the colliding names matter
    let src = src.replace("- set a = invoice_chaser__remind(first_invoice())\n- log: \"never\"\n", "- do remind\n").replace(
        "## Every 5 minutes\n- do remind",
        "## Every 5 minutes\n- do remind\n- do invoice_chaser__remind({ id: \"x\", customer: \"y\" }) -> a\n- do net_amount({ id: \"x\", customer: \"y\" }) -> b\n- do net-amount({ id: \"x\", customer: \"y\" }) -> c\n- log: \"{a} {b} {c}\"",
    );
    let (d, file) = single("names", "names.agent.md", &src);
    let o = agentmark(&["check", s(&file)]).output().unwrap();
    assert!(o.status.success(), "the test source must be valid:\n{}\n{}", stdout(&o), stderr(&o));
    let fx = d.join("fx");
    write(&fx, "model.json", &model_json(&[]));
    let t = trace_of(s(&file), &fx);
    let msgs: Vec<&str> = t.iter().filter_map(|l| l["message"].as_str()).collect();
    assert_eq!(msgs, vec!["do block", "user x underscore dash"]);
}

#[test]
fn a_broken_rust_block_is_reported_at_its_agent_md_line() {
    let src = "# Record: Invoice\n- id: text\n\n# Agent: shouter\n- model: claude\n- tools: log\n\n## Rust: shout -> text\n- in: invoice: Invoice\n- effect: pure\n  ```rust\n  let loud = invoice.id.to_uppercase();\n  loud.len_of_it()\n  ```\n\n## On start\n- set loud = shout({ id: \"a\" })\n- log: \"hi {loud}\"\n";
    let (d, file) = single("rustblock", "shout.agent.md", src);
    let o = agentmark(&["check", s(&file)]).output().unwrap();
    assert!(o.status.success(), "the test source must be valid:\n{}\n{}", stdout(&o), stderr(&o));
    let fx = d.join("fx");
    write(&fx, "model.json", &model_json(&[]));
    let o = agentmark(&["run", s(&file), "--mock", "--fixtures", s(&fx)]).output().unwrap();
    let e = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{e}");
    assert!(e.contains("shout.agent.md:13"), "points at the line of the mistake:\n{e}");
    assert!(e.contains("loud.len_of_it()"), "shows the source line:\n{e}");
    assert!(e.contains("does not compile"), "{e}");
    assert!(!e.contains("internal compiler error"), "{e}");
}
