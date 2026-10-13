use agentmark_rt::{AnthropicModel, ModelRequest};
use serde_json::json;

#[test]
fn builds_request_body() {
    let m = AnthropicModel::new("k", "claude-test");
    let body = m.request_body(&ModelRequest::new("hello"));
    assert_eq!(
        body,
        json!({"model":"claude-test","max_tokens":1024,"messages":[{"role":"user","content":"hello"}]})
    );
    let mut r = ModelRequest::new("hi");
    r.max_tokens = Some(50);
    r.model = Some("other".into());
    assert_eq!(m.request_body(&r)["model"], "other");
    assert_eq!(m.request_body(&r)["max_tokens"], 50);
}

#[test]
fn parses_response_text() {
    let v = json!({"content":[{"type":"text","text":"Hello "},{"type":"text","text":"world"}]});
    assert_eq!(AnthropicModel::parse_response(&v).unwrap(), "Hello world");
    assert!(AnthropicModel::parse_response(&json!({"error":{"message":"x"}})).is_err());
}

#[test]
fn from_env_requires_key() {
    // do not touch the real env var of the host: the helper takes the lookup result
    assert!(AnthropicModel::from_key_result(Err(std::env::VarError::NotPresent), "m").is_err());
    assert!(AnthropicModel::from_key_result(Ok("k".into()), "m").is_ok());
}
