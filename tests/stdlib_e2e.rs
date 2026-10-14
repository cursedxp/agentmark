//! The built-in library, end to end (SPEC 22.3): `tests/stdlib/tour.agent.md` uses every module and every
//! verb of the `file` tool. The real `agentmark` builds it, cargo compiles it, and each command runs in a
//! fresh working directory (a copy of `tests/stdlib/input/` plus `model.json`). The trace it prints and the
//! files it leaves behind are compared with what is written down here.

mod support;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use support::*;

fn build_tour() -> (Tmp, PathBuf) {
    let out = temp("stdlib-e2e");
    let src = format!("{ROOT}/tests/stdlib/tour.agent.md");
    let o = agentmark(&["build", &src, "--target", "rust", "-o", s(&out)]).output_locked();
    assert!(o.status.success(), "agentmark build failed:\n{}", stderr(&o));
    let bin = build_bin(&out, "tour");
    (out, bin)
}

/// Runs `command args…` in a fresh copy of the input folder; returns the trace and the folder.
fn run(bin: &Path, command: &[&str], stdin: &str) -> (Vec<Value>, Tmp) {
    let (trace, work, status) = run_status(bin, command, stdin);
    assert!(status.success(), "{command:?} failed ({status}) with trace {trace:#?}");
    (trace, work)
}

fn run_status(bin: &Path, command: &[&str], stdin: &str) -> (Vec<Value>, Tmp, std::process::ExitStatus) {
    let work = temp("stdlib-work");
    copy_dir(Path::new(&format!("{ROOT}/tests/stdlib/input")), &work);
    write(&work, "model.json", r#"{"today": "2026-10-14", "responses": []}"#);
    let mut child = Command::new(bin)
        .args(["--mock", "--fixtures", ".", "--trace", "-"])
        .args(command)
        .current_dir(&work)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let o = child.wait_with_output().unwrap();
    (parse_lines(&stdout(&o)), work, o.status)
}

fn prints(trace: &[Value]) -> Vec<String> {
    trace.iter().filter(|l| l["kind"] == "print").map(|l| l["message"].as_str().unwrap().to_string()).collect()
}

fn file(verb: &str, path: &str) -> Value {
    json!({"kind": "file", "verb": verb, "path": path})
}

#[test]
fn the_tour_program_behaves() {
    let (_out, bin) = build_tour();

    // ---- pure functions: nothing touches a file
    let (trace, _w) = run(&bin, &["strings", "  hello  "], "");
    assert!(trace.iter().all(|l| l["kind"] == "print"), "pure library calls leave only prints: {trace:#?}");
    assert_eq!(
        prints(&trace),
        [
            "HELLO|abc|3",
            "[ab   ] [----] a+b+c",
            "4 parts, 2 lines",
            "true true false",
            "true <b>bold</b>",
            "found 4711",
            "2 groups",
            "no captures",
        ]
    );
    let (trace, _w) = run(&bin, &["values", "-"], "");
    assert!(trace.iter().all(|l| l["kind"] == "print"), "pure library calls leave only prints: {trace:#?}");
    assert_eq!(
        prints(&trace),
        [
            "42 50 42 42",
            "1240 42.00",
            "12.35 EUR 12.40 EUR 20.00 EUR",
            "2026-10-03 03 October 2026 11",
            "not a date",
            "not a number",
            "photos/a.jpg a.tar.gz a.tar",
            "gz p",
            "2026-10-14 00:00",
        ]
    );

    // ---- csv and json, standard input
    let notes = std::fs::read_to_string(format!("{ROOT}/tests/stdlib/input/notes.json")).unwrap();
    let (trace, work) = run(&bin, &["data", "t1"], &notes);
    assert_eq!(
        trace,
        [
            file("read", "sales.csv"),
            json!({"kind": "print", "message": "EU Widget 120.00 EUR 01.10.2026"}),
            json!({"kind": "print", "message": "US Gadget 5.50 EUR 30.09.2026"}),
            file("write", "sales-copy.csv"),
            json!({"kind": "print", "message": "1 first #home"}),
            json!({"kind": "print", "message": "2 second"}),
            json!({"kind": "print", "message": r#"[{"id":1,"title":"first","tag":"home"},{"id":2,"title":"second","tag":null}]"#}),
            json!({"kind": "print", "message": r#"{"id":1,"name":"t1"}"#}),
            json!({"kind": "print", "message": "seven region,product,amount,sold\nEU,Widget,120.00 EUR,2026-10-01\nUS,Gadget,5.50 EUR,2026-09-30\n"}),
            json!({"kind": "print", "message": "t1: a sales file needs amount and sold columns"}),
        ]
    );
    assert_eq!(
        std::fs::read_to_string(work.join("sales-copy.csv")).unwrap(),
        "region,product,amount,sold\nEU,Widget,120.00 EUR,2026-10-01\nUS,Gadget,5.50 EUR,2026-09-30\n"
    );
    let (trace, _w) = run(&bin, &["data", "t2"], "not json");
    assert_eq!(trace.last().unwrap(), &json!({"kind": "print", "message": "t2: standard input is not a list of notes"}));

    // ---- the file tool
    let (trace, work) = run(&bin, &["files", "archive"], "");
    assert_eq!(
        trace,
        [
            file("write", "log.txt"),
            file("append", "log.txt"),
            file("read", "log.txt"),
            json!({"kind": "print", "message": "12 characters"}),
            file("exists", "log.txt"),
            file("list", "archive"),
            json!({"kind": "print", "message": "archive: a.txt"}),
            json!({"kind": "print", "message": "archive: b.txt"}),
            json!({"kind": "file", "verb": "rename", "path": "log.txt", "to": "archive/log-1.txt"}),
            file("exists", "log.txt"),
            json!({"kind": "print", "message": "true false"}),
            file("delete", "archive/log-1.txt"),
            file("read", "missing.txt"),
            json!({"kind": "print", "message": "missing.txt is not there"}),
        ]
    );
    assert!(!work.join("log.txt").exists() && !work.join("archive/log-1.txt").exists());
    assert!(work.join("archive/a.txt").exists());

    // ---- a pattern built at run time is checked when it runs
    let (trace, _w) = run(&bin, &["pattern", "b+"], "");
    assert_eq!(prints(&trace), ["true"]);
    let (trace, _w, status) = run_status(&bin, &["pattern", "("], "");
    assert!(!status.success());
    assert_eq!(trace.len(), 1, "{trace:#?}");
    assert_eq!(trace[0]["kind"], "error");
    assert_eq!(trace[0]["agent"], "pattern");
    let message = trace[0]["message"].as_str().unwrap();
    assert!(message.starts_with("\"(\" is not a valid pattern: "), "{message}");
}
