use agentmark_rt::{parse_model_json, read_data, Ctx, Money, Trace};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize, PartialEq)]
struct Q {
    score: i64,
    reason: String,
}

#[test]
fn parses_plain_fenced_and_wrapped_json() {
    let want = Q { score: 7, reason: "ok".into() };
    assert_eq!(parse_model_json::<Q>(r#"{"score":7,"reason":"ok"}"#).unwrap(), want);
    assert_eq!(parse_model_json::<Q>("```json\n{\"score\":7,\"reason\":\"ok\"}\n```").unwrap(), want);
    assert_eq!(parse_model_json::<Q>("Sure! {\"score\":7,\"reason\":\"ok\"} Hope it helps.").unwrap(), want);
}

#[test]
fn rejects_json_that_does_not_fit() {
    let e = parse_model_json::<Q>(r#"{"score":"high"}"#).unwrap_err().to_string();
    assert!(e.contains("does not fit"), "{e}");
    assert!(parse_model_json::<Q>("no json here").is_err());
}

#[derive(Debug, Deserialize, PartialEq)]
struct Row {
    id: String,
    amount: Money,
    qty: i64,
    paid: bool,
}

#[test]
fn reads_csv_with_quotes_and_typed_cells() {
    let dir = tempdir();
    let p = dir.join("rows.csv");
    std::fs::write(&p, "id,amount,qty,paid\r\n\"A,1\",12.50 EUR,3,true\n\nB2,\"1200.00 EUR\",4,false\n").unwrap();
    let rows: Vec<Row> = read_data(p.to_str().unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "A,1");
    assert_eq!(rows[0].amount.to_string(), "12.50 EUR");
    assert_eq!((rows[0].qty, rows[0].paid), (3, true));
    assert_eq!(rows[1].amount.to_string(), "1200.00 EUR");
}

#[test]
fn csv_cells_stay_text_when_the_type_wants_text() {
    #[derive(Deserialize)]
    struct T {
        id: String,
    }
    let dir = tempdir();
    let p = dir.join("t.csv");
    std::fs::write(&p, "id\n007\n").unwrap();
    let rows: Vec<T> = read_data(p.to_str().unwrap()).unwrap();
    assert_eq!(rows[0].id, "007");
}

#[test]
fn reads_json_and_reports_bad_files() {
    let dir = tempdir();
    let p = dir.join("r.json");
    std::fs::write(&p, r#"[{"score":1,"reason":"x"}]"#).unwrap();
    let rows: Vec<Q> = read_data(p.to_str().unwrap()).unwrap();
    assert_eq!(rows[0].score, 1);
    assert!(read_data::<Vec<Q>>("/no/such/file.json").is_err());
}

fn tempdir() -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!("agentmark-rt-support-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[tokio::test]
async fn ask_model_about_adds_context_and_traces_id() {
    let dir = tempdir();
    std::fs::write(
        dir.join("model.json"),
        r#"{"today":"2026-10-13","responses":[{"prompt_prefix":"Write","response":"hi"}]}"#,
    )
    .unwrap();
    let trace = Trace::memory();
    let ctx = Ctx::mock(&dir, trace.clone()).unwrap().enter("a", "b");
    let out = ctx
        .ask_model_about("Write it.", Some(&json!({"id": "INV-1", "x": 1})), Some("Answer with JSON."))
        .await
        .unwrap();
    assert_eq!(out, "hi");
    let line: serde_json::Value = serde_json::from_str(&trace.lines()[0]).unwrap();
    assert_eq!(line["prompt_prefix"], "Write");
    assert_eq!(line["context"], json!({"id": "INV-1"}));
    // a subject without id is reported whole; no subject = no context
    ctx.ask_model_about("Write it.", Some(&json!("plain")), None).await.unwrap();
    ctx.ask_model_about("Write it.", None, None).await.unwrap();
    let l: Vec<serde_json::Value> = trace.lines().iter().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(l[1]["context"], json!("plain"));
    assert!(l[2].get("context").is_none());
}
