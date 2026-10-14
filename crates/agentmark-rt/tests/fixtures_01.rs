use agentmark_rt::*;
use serde_json::{json, Value};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/01-invoice-chaser")
}

#[tokio::test]
async fn mock_model_loads_and_matches_longest_prefix() {
    let mm = MockModel::from_file(fixtures().join("model.json")).unwrap();
    assert_eq!(mm.today().unwrap().to_string(), "2026-10-13");
    let r = mm
        .complete(&ModelRequest::new("Write a short, polite payment reminder to Müller GmbH"))
        .await
        .unwrap();
    assert_eq!(r.matched_prefix.as_deref(), Some("Write a short, polite payment reminder"));
    assert!(r.text.starts_with("Dear Müller GmbH"));
    assert!(matches!(
        mm.complete(&ModelRequest::new("Something else")).await,
        Err(RtError::NoMockResponse { .. })
    ));
}

#[tokio::test]
async fn mock_model_longest_prefix_wins() {
    let mm = MockModel::new(
        None,
        vec![("Write".into(), "short".into()), ("Write a letter".into(), "long".into()), ("Wr".into(), "shortest".into())],
    );
    let r = mm.complete(&ModelRequest::new("Write a letter to Bob")).await.unwrap();
    assert_eq!((r.text.as_str(), r.matched_prefix.as_deref()), ("long", Some("Write a letter")));
    let r = mm.complete(&ModelRequest::new("Write me")).await.unwrap();
    assert_eq!(r.text, "short");
}

#[tokio::test]
async fn mock_tools_load_fixture_files() {
    let t = MockTools::load(fixtures()).unwrap();
    let all = t.call("crm", "list", &json!({"source": "invoices"})).await.unwrap();
    assert_eq!(all.as_array().unwrap().len(), 3);
    assert_eq!(all[0]["amount"], "1200.00 EUR");
    let one = t.call("crm", "get", &json!({"source": "invoices", "id": "INV-2"})).await.unwrap();
    assert_eq!(one["customer"], "Schmidt AG");
    assert_eq!(t.call("crm", "get", &json!({"source": "invoices", "id": "nope"})).await.unwrap(), Value::Null);
    assert_eq!(t.call("crm", "update", &json!({"source": "invoices", "id": "INV-1"})).await.unwrap(), Value::Null);
    assert_eq!(t.call("email", "send", &json!({})).await.unwrap(), Value::Null);
    assert!(matches!(
        t.call("crm", "list", &json!({"source": "missing"})).await,
        Err(RtError::MissingFixture { .. })
    ));
}

/// Reproduce expected.trace.jsonl line by line through the typed helpers.
#[tokio::test]
async fn reproduces_expected_trace() {
    let ctx = Ctx::mock(fixtures(), Trace::memory()).unwrap();
    assert_eq!(ctx.clock.today().to_string(), "2026-10-13");
    let ctx = ctx.enter("invoice-chaser", "Every day at 09:00");

    let invoices = ctx.tools.call("crm", "list", &json!({"source": "invoices"})).await.unwrap();
    let inv = &invoices[0];
    let text = ctx
        .ask_model("Write a short, polite payment reminder for Müller GmbH", Some(json!({"id": inv["id"]})))
        .await
        .unwrap();
    ctx.trace_guard("no amounts changed in reminder", no_amounts_changed(&text, inv)).unwrap();
    ctx.trace_guard("reminder length < 800", text.chars().count() < 800).unwrap();
    ctx.trace_tool_email("send", "customer", "ap@mueller.de").unwrap();
    ctx.trace_tool_crm("update", "invoices", "INV-1", Some(json!({"status": "reminded"}))).unwrap();
    ctx.trace_tool_log("Reminded Müller GmbH about 1200.00 EUR").unwrap();

    let expected = std::fs::read_to_string(fixtures().join("expected.trace.jsonl")).unwrap();
    let want: Vec<Value> = expected.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let got: Vec<Value> = ctx.trace.lines().iter().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(got.len(), want.len());
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "trace line {}", i + 1);
    }
    // key order and exact bytes also match the fixture
    let exact: Vec<String> = expected.lines().map(String::from).collect();
    assert_eq!(ctx.trace.lines(), exact);
}

#[test]
fn mock_ctx_requires_today() {
    let tmp = tempfile::Builder::new().prefix("agentmark-rt-notoday-").tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    std::fs::write(dir.join("model.json"), r#"{"responses":[]}"#).unwrap();
    let e = Ctx::mock(&dir, Trace::memory()).err().expect("must fail without today");
    assert!(e.to_string().contains("today"), "{e}");
    std::fs::remove_file(dir.join("model.json")).unwrap();
    assert!(Ctx::mock(&dir, Trace::memory()).is_err()); // no model.json at all
}
