use agentmark_rt::*;
use serde_json::json;

fn one(f: impl FnOnce(&Trace) -> Result<(), RtError>) -> String {
    let t = Trace::memory();
    f(&t).unwrap();
    let l = t.lines();
    assert_eq!(l.len(), 1);
    l[0].clone()
}

#[test]
fn model_line() {
    assert_eq!(
        one(|t| t.model("a", "b", "pre", None)),
        r#"{"kind":"model","agent":"a","block":"b","prompt_prefix":"pre"}"#
    );
    assert_eq!(
        one(|t| t.model("a", "b", "pre", Some(json!({"id": "X"})))),
        r#"{"kind":"model","agent":"a","block":"b","prompt_prefix":"pre","context":{"id":"X"}}"#
    );
}

#[test]
fn guard_lines() {
    assert_eq!(one(|t| t.guard("a", "g", true)), r#"{"kind":"guard","agent":"a","name":"g","result":"pass"}"#);
    assert_eq!(one(|t| t.guard("a", "g", false)), r#"{"kind":"guard","agent":"a","name":"g","result":"fail"}"#);
}

#[test]
fn tool_lines() {
    assert_eq!(
        one(|t| t.tool_email("send", "customer", "x@y.de")),
        r#"{"kind":"tool","tool":"email","verb":"send","target":"customer","to":"x@y.de"}"#
    );
    assert_eq!(
        one(|t| t.tool_crm("update", "invoices", "INV-1", Some(json!({"status": "paid"})))),
        r#"{"kind":"tool","tool":"crm","verb":"update","source":"invoices","id":"INV-1","set":{"status":"paid"}}"#
    );
    assert_eq!(
        one(|t| t.tool_crm("get", "invoices", "INV-1", None)),
        r#"{"kind":"tool","tool":"crm","verb":"get","source":"invoices","id":"INV-1"}"#
    );
    assert_eq!(
        one(|t| t.tool_http("post", "https://x.test/h", Some(json!({"a": 1})))),
        r#"{"kind":"tool","tool":"http","verb":"post","url":"https://x.test/h","body":{"a":1}}"#
    );
    assert_eq!(
        one(|t| t.tool_http("get", "https://x.test/h", None)),
        r#"{"kind":"tool","tool":"http","verb":"get","url":"https://x.test/h"}"#
    );
    assert_eq!(one(|t| t.tool_log("hi \"there\"")), r#"{"kind":"tool","tool":"log","message":"hi \"there\""}"#);
}

#[test]
fn http_print_error_lines() {
    assert_eq!(one(|t| t.http("GET", "/x", 200)), r#"{"kind":"http","method":"GET","path":"/x","status":200}"#);
    assert_eq!(one(|t| t.print("hello")), r#"{"kind":"print","message":"hello"}"#);
    assert_eq!(
        one(|t| t.error("a", "b", "boom")),
        r#"{"kind":"error","agent":"a","block":"b","message":"boom"}"#
    );
}

#[test]
fn file_sink_appends_lines() {
    let dir = std::env::temp_dir().join(format!("agentmark-rt-trace-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.jsonl");
    let _ = std::fs::remove_file(&path);
    let t = Trace::to_file(&path).unwrap();
    t.print("a").unwrap();
    t.print("b").unwrap();
    drop(t);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"kind\":\"print\",\"message\":\"a\"}\n{\"kind\":\"print\",\"message\":\"b\"}\n");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ctx_helpers_use_labels() {
    let ctx = Ctx::new(Trace::memory(), std::sync::Arc::new(MockModel::new(None, vec![])), std::sync::Arc::new(MockTools::empty()), Clock::system())
        .enter("unit-x", "Entry y");
    ctx.trace_guard("g", true).unwrap();
    ctx.trace_error("bad").unwrap();
    ctx.trace_model("p", None).unwrap();
    assert_eq!(ctx.trace.lines()[0], r#"{"kind":"guard","agent":"unit-x","name":"g","result":"pass"}"#);
    assert_eq!(ctx.trace.lines()[1], r#"{"kind":"error","agent":"unit-x","block":"Entry y","message":"bad"}"#);
    assert_eq!(ctx.trace.lines()[2], r#"{"kind":"model","agent":"unit-x","block":"Entry y","prompt_prefix":"p"}"#);
}

struct Failing;
impl std::io::Write for Failing {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe closed"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn write_errors_are_returned_not_swallowed() {
    let t = Trace::from_writer(Box::new(Failing));
    let e = t.print("x").unwrap_err();
    assert!(matches!(e, RtError::Trace(ref m) if m.contains("pipe closed")), "{e}");
    let ctx = Ctx::new(t, std::sync::Arc::new(MockModel::new(None, vec![])), std::sync::Arc::new(MockTools::empty()), Clock::system());
    assert!(ctx.trace_tool_log("x").is_err());
}
