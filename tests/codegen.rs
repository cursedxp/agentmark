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

/// A unique temp folder, removed when dropped (so reruns never meet leftovers).
struct Tmp(tempfile::TempDir);

impl std::ops::Deref for Tmp {
    type Target = Path;
    fn deref(&self) -> &Path {
        self.0.path()
    }
}

fn temp(tag: &str) -> Tmp {
    Tmp(tempfile::Builder::new().prefix(&format!("agentmark-codegen-{tag}-")).tempdir().unwrap())
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
fn single(tag: &str, name: &str, source: &str) -> (Tmp, PathBuf) {
    let d = temp(tag);
    write(&d, name, source);
    write(&d, "tools.toml", &tools_toml());
    let file = d.join(name);
    (d, file)
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
    let o = agentmark(&["build", &format!("{ROOT}/examples/01-invoice-chaser"), "--target", "rust", "-o", s(&out), "--no-check"]).output().unwrap();
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

// ---------------------------------------------------------------------------------- fixtures

/// Every fixture folder (and its `variants/<case>/`) with an `expected.trace.jsonl`: the program's
/// trace must equal it, line by line.
fn fixture_dirs() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut names: Vec<_> = std::fs::read_dir(format!("{ROOT}/fixtures")).unwrap().flatten().map(|e| e.path()).collect();
    names.sort();
    for dir in names.into_iter().filter(|p| p.is_dir()) {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        if dir.join("expected.trace.jsonl").is_file() {
            out.push((name.clone(), dir.clone()));
        }
        let mut vs: Vec<_> = std::fs::read_dir(dir.join("variants")).into_iter().flatten().flatten().map(|e| e.path()).collect();
        vs.sort();
        for v in vs.into_iter().filter(|v| v.join("expected.trace.jsonl").is_file()) {
            out.push((name.clone(), v));
        }
    }
    out
}

#[test]
fn every_example_reproduces_the_expected_trace_of_every_fixture() {
    let fixtures = fixture_dirs();
    assert!(fixtures.len() >= 10, "{fixtures:?}");
    for (example, dir) in fixtures {
        let want = parse_lines(&std::fs::read_to_string(dir.join("expected.trace.jsonl")).unwrap());
        // a variant is self-contained; a plain `requests.json` etc. only exists where it applies
        let got = trace_of(&format!("{ROOT}/examples/{example}"), &dir);
        assert_eq!(got, want, "{example} with {}", dir.display());
    }
}

#[test]
fn a_missing_model_answer_is_reported_not_swallowed() {
    let fx = temp("fx04b");
    write(&fx, "crm.invoices.json", "[]");
    write(&fx, "model.json", &model_json(&[]));
    let o = agentmark(&["trace", &format!("{ROOT}/examples/04-daily-report"), "--mock", "--fixtures", s(&fx)]).output().unwrap();
    // the block fails, the run says so, and the trace records the error
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    let t = parse_lines(&stdout(&o));
    assert_eq!(t.last().unwrap()["kind"], "error");
    assert_eq!(t.last().unwrap()["block"], "Every Monday at 08:00");
}

// ---------------------------------------------------------------------------------- 05

/// Example 05 as it is in the repository (its sha2 binding is approved), built with cargo.
fn built_05() -> (Tmp, PathBuf) {
    let d = temp("p05");
    let o = agentmark(&["build", &format!("{ROOT}/examples/05-invoice-service"), "-o", s(&d.join("out")), "--no-check"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("binding sha2: cached"), "{}", stderr(&o));
    let cargo = std::fs::read_to_string(d.join("out/Cargo.toml")).unwrap();
    assert!(cargo.contains("sha2 = \"=0.10"), "{cargo}");
    for f in ["src/service_api.rs", "src/command_import_csv.rs"] {
        assert!(d.join("out").join(f).is_file(), "{f}");
    }
    let b = Command::new("cargo")
        .args(["build", "--offline", "--manifest-path"])
        .arg(d.join("out/Cargo.toml"))
        .env("CARGO_TARGET_DIR", gen_target())
        .output()
        .unwrap();
    assert!(b.status.success(), "{}", stderr(&b));
    assert!(!stderr(&b).contains("warning"), "{}", stderr(&b));
    let bin = gen_bin("invoice-service");
    (d, bin)
}

#[test]
fn build_binds_missing_crates_and_refuses_unapproved_ones() {
    // the repository copy of 05 without its binding: bound from the canned rustdoc, then refused
    let d = temp("bind05");
    copy_dir(Path::new(&format!("{ROOT}/examples/05-invoice-service")), &d);
    std::fs::remove_file(d.join("bindings/sha2.agent.md")).unwrap();
    let out = temp("out-bind05");
    let mut c = agentmark(&["build", s(&d), "-o", s(&out.join("p")), "--no-check"]);
    bind_env(&mut c);
    let o = c.output().unwrap();
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    let e = stderr(&o);
    assert!(e.contains("binding sha2: generated"), "{e}");
    assert!(e.contains("unverified") && e.contains("- effect: pure"), "{e}");
    assert!(!out.join("p").exists(), "nothing is written for a refused build");
    // approving the one line is all it takes
    let p = d.join("bindings/sha2.agent.md");
    let t = std::fs::read_to_string(&p).unwrap().replace("- effect: unverified", "- effect: pure");
    std::fs::write(p, t).unwrap();
    let o = agentmark(&["build", s(&d), "-o", s(&out.join("p")), "--no-check"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("binding sha2: cached"));
}

fn copy_dir(from: &Path, to: &Path) {
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let t = to.join(e.file_name());
        if e.path().is_dir() {
            std::fs::create_dir_all(&t).unwrap();
            copy_dir(&e.path(), &t);
        } else {
            std::fs::copy(e.path(), t).unwrap();
        }
    }
}

fn gen_bin(name: &str) -> PathBuf {
    PathBuf::from(gen_target()).join("debug").join(name)
}


#[test]
fn a_response_that_differs_from_expect_body_fails_the_run() {
    let (d, bin) = built_05();
    let fx = d.join("fx");
    copy_dir(Path::new(&format!("{ROOT}/fixtures/05-invoice-service")), &{
        std::fs::create_dir_all(&fx).unwrap();
        fx.clone()
    });
    let ok = Command::new(&bin).args(["--once", "--mock", "--fixtures", s(&fx)]).output().unwrap();
    assert!(ok.status.success(), "{}", stderr(&ok));
    write(&fx, "requests.json", r#"[{"method":"GET","path":"/invoices/INV-1","expect_body":{"id":"nope"}}]"#);
    let bad = Command::new(&bin).args(["--once", "--mock", "--fixtures", s(&fx)]).output().unwrap();
    assert_eq!(bad.status.code(), Some(1));
    assert!(stderr(&bad).contains("expected body"), "{}", stderr(&bad));
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
    copy_dir(Path::new(&format!("{ROOT}/fixtures/05-invoice-service")), &{
        std::fs::create_dir_all(&fx).unwrap();
        fx.clone()
    });
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
    assert_eq!(body["net"], "972.00 EUR");
    assert_eq!(body["fingerprint"], "fd8ffe6cba7f7fef9fd8eae9a4c87754b9cc91e6cb3543a95537b1eedfee2847");
    let t = parse_lines(&std::fs::read_to_string(trace).unwrap());
    assert_eq!(t.last().unwrap(), &json!({"kind":"http","method":"GET","path":"/invoices/INV-1/net","status":200}));
}

#[test]
fn command_line_runs_the_command_with_its_arguments() {
    let (d, bin) = built_05();
    let fx = d.join("fx");
    copy_dir(Path::new(&format!("{ROOT}/fixtures/05-invoice-service")), &{
        std::fs::create_dir_all(&fx).unwrap();
        fx.clone()
    });
    let run = |file: &str| {
        let o = Command::new(&bin).args(["--mock", "--fixtures", s(&fx), "--trace", "-", "import-csv", file]).output().unwrap();
        assert!(o.status.success(), "{}", stderr(&o));
        parse_lines(&stdout(&o))
    };
    assert_eq!(run("nope.csv"), vec![json!({"kind":"print","message":"Could not read nope.csv"})]);
    assert_eq!(run("invoices.csv")[0]["kind"], "print");
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

// ---------------------------------------------------------------------------------- review fixes

fn cargo_build_in(out: &Path) -> Output {
    Command::new("cargo")
        .args(["build", "--offline", "--manifest-path"])
        .arg(out.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", gen_target())
        .output()
        .unwrap()
}

#[test]
fn every_example_compiles_without_a_single_warning() {
    for name in ["01-invoice-chaser", "02-lead-qualifier", "03-support-triage", "04-daily-report", "05-invoice-service"] {
        let out = temp("nowarn");
        let o = agentmark(&["build", &format!("{ROOT}/examples/{name}"), "-o", s(&out), "--no-check"]).output().unwrap();
        assert!(o.status.success(), "{name}: {}", stderr(&o));
        let b = cargo_build_in(&out);
        assert!(b.status.success(), "{name}: {}", stderr(&b));
        assert!(!stderr(&b).contains("warning"), "{name} must compile warning free:\n{}", stderr(&b));
    }
}

const BROKEN_SEMICOLON: &str = "# Record: Invoice\n- id: text\n\n# Agent: shouter\n- model: claude\n- tools: log\n\n## Rust: shout -> text\n- in: invoice: Invoice\n- effect: pure\n  ```rust\n  let loud = invoice.id.to_uppercase();\n  loud;\n  ```\n\n## On start\n- set loud = shout({ id: \"a\" })\n- log: \"hi {loud}\"\n";

#[test]
fn a_rust_block_ending_in_a_semicolon_is_the_authors_mistake_not_an_ice() {
    let (d, file) = single("semicolon", "semi.agent.md", BROKEN_SEMICOLON);
    let fx = d.join("fx");
    write(&fx, "model.json", &model_json(&[]));
    let o = agentmark(&["run", s(&file), "--mock", "--fixtures", s(&fx)]).output().unwrap();
    let e = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{e}");
    assert!(e.contains("semi.agent.md:8"), "points at the block heading:\n{e}");
    assert!(e.contains("does not compile"), "{e}");
    assert!(!e.contains("internal compiler error"), "{e}");
}

#[test]
fn build_checks_the_generated_project_and_reports_user_rust_plainly() {
    let (d, file) = single("buildcheck", "semi.agent.md", BROKEN_SEMICOLON);
    let out = temp("buildcheck-out");
    let o = agentmark(&["build", s(&file), "-o", s(&out)]).output().unwrap();
    let e = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{e}");
    assert!(e.contains("semi.agent.md:8") && e.contains("does not compile"), "{e}");
    assert!(!e.contains("error[E"), "no raw rustc output: {e}");
    // --no-check only writes the files
    let o = agentmark(&["build", s(&file), "-o", s(&out), "--no-check"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    drop(d);
}

const SEEN: &str = "# Record: Invoice\n- id: text\n- customer: text\n- status: text\n\n# Agent: seen\n- model: claude\n- tools: crm, log\n\n## On start\n- for each invoice in crm.invoices:\n  - crm update invoice: status = \"seen\"\n  - log: \"saw {invoice.id}\"\n";

const CONNECT: &str = r#"//! A user's connect.rs: an in-memory CRM.
use std::sync::Mutex;

use agentmark_rt::RtError;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{Connected, Crm, Email, Http};

static STORE: Mutex<Vec<Value>> = Mutex::new(Vec::new());

struct Memory;

#[async_trait]
impl Crm for Memory {
    async fn list(&self, _args: Value) -> Result<Value, RtError> {
        let mut rows = STORE.lock().unwrap();
        if rows.is_empty() {
            rows.push(json!({"id": "A-1", "customer": "Acme", "status": "open"}));
            rows.push(json!({"id": "A-2", "customer": "Bolt", "status": "open"}));
        }
        Ok(Value::Array(rows.clone()))
    }
    async fn get(&self, _args: Value) -> Result<Value, RtError> {
        Ok(Value::Null)
    }
    async fn update(&self, args: Value) -> Result<Value, RtError> {
        for r in STORE.lock().unwrap().iter_mut() {
            if r["id"] == args["id"] {
                r["status"] = args["set"]["status"].clone();
            }
        }
        Ok(Value::Null)
    }
}

struct Unused;

#[async_trait]
impl Email for Unused {
    async fn send(&self, _args: Value) -> Result<Value, RtError> {
        Ok(Value::Null)
    }
}

#[async_trait]
impl Http for Unused {
    async fn get(&self, _args: Value) -> Result<Value, RtError> {
        Ok(Value::Null)
    }
    async fn post(&self, _args: Value) -> Result<Value, RtError> {
        Ok(Value::Null)
    }
}

pub fn connect() -> Connected {
    Connected { crm: Box::new(Memory), email: Box::new(Unused), http: Box::new(Unused) }
}
"#;

#[test]
fn connect_rs_lives_in_the_project_and_build_and_run_use_it() {
    let (d, file) = single("connect", "seen-real.agent.md", SEEN);
    let out = temp("connect-out");
    let build = || agentmark(&["build", s(&file), "-o", s(&out), "--no-check"]).output().unwrap();
    // first build: the stub is created in the PROJECT, copied into the output
    let first = build();
    assert!(first.status.success());
    assert!(stderr(&first).contains("rust/connect.rs"), "{}", stderr(&first));
    let stub = std::fs::read_to_string(d.join("rust/connect.rs")).unwrap();
    assert!(stub.contains("not connected yet: implement Crm::list in rust/connect.rs"), "{stub}");
    assert!(stub.contains("never changes"), "the header says the file is the user's");
    assert_eq!(std::fs::read_to_string(out.join("src/connect.rs")).unwrap(), stub);

    // the user fills in the project file; building again copies it and never rewrites it
    std::fs::write(d.join("rust/connect.rs"), CONNECT).unwrap();
    assert!(build().status.success());
    assert_eq!(std::fs::read_to_string(d.join("rust/connect.rs")).unwrap(), CONNECT);
    assert_eq!(std::fs::read_to_string(out.join("src/connect.rs")).unwrap(), CONNECT);

    // deleting the output loses nothing: the next build brings the implementation back
    std::fs::remove_dir_all(&*out).unwrap();
    assert!(build().status.success());
    assert_eq!(std::fs::read_to_string(out.join("src/connect.rs")).unwrap(), CONNECT);

    let b = cargo_build_in(&out);
    assert!(b.status.success(), "{}", stderr(&b));
    assert!(!stderr(&b).contains("warning"), "{}", stderr(&b));
    let trace = out.join("trace.jsonl");
    let o = Command::new(gen_bin("seen-real")).args(["--once", "--trace", s(&trace)]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let t = parse_lines(&std::fs::read_to_string(trace).unwrap());
    let updates: Vec<&Value> = t.iter().filter(|l| l["verb"] == "update").collect();
    assert_eq!(updates.len(), 2, "{t:#?}");
    assert_eq!(updates[0], &json!({"kind":"tool","tool":"crm","verb":"update","source":"invoices","id":"A-1","set":{"status":"seen"}}));
    assert_eq!(t.last().unwrap(), &json!({"kind":"tool","tool":"log","message":"saw A-2"}));
}

#[test]
fn a_real_run_uses_the_projects_connect_rs() {
    let (d, file) = single("connectrun", "seen-run.agent.md", SEEN);
    write(&d, "rust/connect.rs", CONNECT);
    let o = agentmark(&["run", s(&file)]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let e = stderr(&o);
    assert!(e.contains("saw A-1") && e.contains("saw A-2"), "the in-memory CRM answered:\n{e}");
    assert_eq!(std::fs::read_to_string(d.join("rust/connect.rs")).unwrap(), CONNECT, "run never rewrites it");
    // trace uses it too
    let t = agentmark(&["trace", s(&file)]).output().unwrap();
    assert!(t.status.success(), "{}", stderr(&t));
    assert_eq!(parse_lines(&stdout(&t)).iter().filter(|l| l["verb"] == "update").count(), 2);
}

#[test]
fn a_mock_run_does_not_create_the_projects_connect_rs() {
    let (d, file) = single("connectmock", "seen-mock.agent.md", SEEN);
    let fx = d.join("fx");
    write(&fx, "model.json", &model_json(&[]));
    write(&fx, "crm.invoices.json", r#"[{"id":"A-1","customer":"Acme","status":"open"}]"#);
    let o = agentmark(&["run", s(&file), "--mock", "--fixtures", s(&fx)]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!d.join("rust").exists(), "mock runs leave the project alone");
}

#[test]
fn a_real_run_without_connecting_says_what_to_implement() {
    let (d, file) = single("unconnected", "seen-stub.agent.md", SEEN);
    let o = agentmark(&["run", s(&file)]).output().unwrap();
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("not connected yet: implement Crm::list in rust/connect.rs"), "{}", stderr(&o));
    assert!(d.join("rust/connect.rs").is_file(), "the stub was created for the author to fill in");
}

#[test]
fn every_agent_asks_the_model_of_its_own_model_line() {
    let src = "# Agent: small\n- model: claude:haiku\n- tools: log\n\n## On start\n- ask model: \"Q small\" -> a\n- log: \"{a}\"\n\n# Agent: big\n- model: claude:opus\n- tools: log\n\n## On start\n- ask model: \"Q big\" -> b\n- log: \"{b}\"\n";
    let (d, file) = single("drivers", "two.agent.md", src);
    let fx = d.join("fx");
    write(
        &fx,
        "model.json",
        r#"{"today":"2026-10-13","responses":[
            {"prompt_prefix":"Q small","response":"S","model":"claude-haiku-4-5"},
            {"prompt_prefix":"Q big","response":"B","model":"claude-opus-4-1"}]}"#,
    );
    let t = trace_of(s(&file), &fx);
    let logs: Vec<&str> = t.iter().filter_map(|l| l["message"].as_str()).collect();
    assert_eq!(logs, vec!["S", "B"], "{t:#?}");
}

#[test]
fn build_warns_about_memory_file_and_serialized_parallel_blocks() {
    let src = "# Agent: a\n- model: claude\n- tools: log\n- memory: file\n\n## On start\n- parallel:\n  - log: \"x\"\n";
    let (d, file) = single("notes", "a.agent.md", src);
    let o = agentmark(&["build", s(&file), "-o", s(&d.join("out")), "--no-check"]).output().unwrap();
    let e = stderr(&o);
    assert!(o.status.success(), "{e}");
    assert!(e.contains("`memory: file` is not supported in v0.1"), "{e}");
    assert!(e.contains("this `parallel:` runs in order"), "{e}");
    assert!(e.contains("only event entries") || !e.contains("event"), "{e}");
}

#[test]
fn a_failing_command_leaves_an_error_line_in_the_trace() {
    let src = "# Command: slurp\n- args: path: text\n\n## Run\n- read file path as text -> t\n- print: \"got {t}\"\n";
    let (d, file) = single("cmderr", "slurp.agent.md", src);
    let fx = d.join("fx");
    write(&fx, "model.json", &model_json(&[]));
    write(&fx, "command.json", r#"{"command":"slurp","args":{"path":"missing.txt"}}"#);
    let o = agentmark(&["trace", s(&file), "--mock", "--fixtures", s(&fx)]).output().unwrap();
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    let t = parse_lines(&stdout(&o));
    assert_eq!(t.len(), 1, "{t:#?}");
    assert_eq!(t[0]["kind"], "error");
    assert_eq!(t[0]["agent"], "slurp");
    assert_eq!(t[0]["block"], "Run");
}

// ---------------------------------------------------------------------------------- result of T

const RESULT_FN: &str = "# Function: read_count\n- in: path: text\n- out: result of int\n- effect: local\n  - try read file path as int -> n\n  - return n\n\n# Command: check\n- args: path: text\n\n## Run\n- try do read_count(path) -> n\n  otherwise:\n    - print: \"unreadable\"\n    - stop\n- print: \"count {n}\"\n";

#[test]
fn a_result_function_returns_ok_and_its_failure_reaches_the_caller() {
    let (d, file) = single("resultfn", "count.agent.md", RESULT_FN);
    let fx = d.join("fx");
    write(&fx, "model.json", &model_json(&[]));
    write(&d, "n.json", "41");
    let run = |path: &str| {
        write(&fx, "command.json", &json!({"command": "check", "args": {"path": path}}).to_string());
        let o = agentmark(&["run", s(&file), "--mock", "--fixtures", s(&fx)]).output().unwrap();
        assert!(o.status.success(), "{}", stderr(&o));
        format!("{}{}", stdout(&o), stderr(&o))
    };
    // success: the caller sees a plain int
    assert!(run(s(&d.join("n.json"))).contains("count 41"));
    // the `try` inside the function has no otherwise: the error leaves the function, the caller's
    // otherwise branch takes it
    let e = run(s(&d.join("missing.json")));
    assert!(e.contains("unreadable") && !e.contains("count"), "{e}");
}

#[test]
fn a_result_functions_failure_without_otherwise_is_traced_once_by_the_caller() {
    let src = RESULT_FN.replace("  otherwise:\n    - print: \"unreadable\"\n    - stop\n", "");
    let (d, file) = single("resultfn2", "count2.agent.md", &src);
    let fx = d.join("fx");
    write(&fx, "model.json", &model_json(&[]));
    write(&fx, "command.json", &json!({"command": "check", "args": {"path": s(&d.join("missing.json"))}}).to_string());
    let o = agentmark(&["trace", s(&file), "--mock", "--fixtures", s(&fx)]).output().unwrap();
    let t = parse_lines(&stdout(&o));
    assert_eq!(t.len(), 1, "{t:#?}");
    assert_eq!(t[0]["kind"], "error");
}
