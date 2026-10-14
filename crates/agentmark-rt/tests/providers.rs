//! Each provider against a recorded HTTP exchange served by a local test server (no live calls):
//! the request shape (path, headers, body) is asserted and the recorded reply is parsed.

use agentmark_rt::{
    AnthropicModel, Driver, LocalModel, MockModel, ModelClient, ModelRequest, OpenAiModel, ProviderConfig, Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

/// What the server saw.
#[derive(Debug)]
struct Seen {
    path: String,
    headers: HashMap<String, String>,
    body: Value,
}

/// Serve `replies` (status, body) to successive connections on a free port; the receiver yields
/// each request as it was seen. The thread ends after the last reply.
fn serve(replies: Vec<(u16, String)>) -> (String, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for (status, reply_body) in replies {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let (head_end, len) = loop {
                let n = sock.read(&mut chunk).unwrap();
                buf.extend_from_slice(&chunk[..n]);
                if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..i]).to_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                        .unwrap_or(0);
                    break (i + 4, len);
                }
            };
            while buf.len() < head_end + len {
                let n = sock.read(&mut chunk).unwrap();
                buf.extend_from_slice(&chunk[..n]);
            }
            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
            let mut lines = head.lines();
            let path = lines.next().unwrap().split_whitespace().nth(1).unwrap().to_string();
            let headers = lines
                .filter_map(|l| l.split_once(':'))
                .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
                .collect();
            let body = serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or(Value::Null);
            tx.send(Seen { path, headers, body }).unwrap();
            let reply = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply_body}",
                reply_body.len()
            );
            sock.write_all(reply.as_bytes()).unwrap();
        }
    });
    (url, rx)
}

const ANTHROPIC_REPLY: &str = r#"{"id":"msg_01","type":"message","role":"assistant","model":"claude-haiku-4-5","content":[{"type":"text","text":"Hello "},{"type":"text","text":"there"}],"stop_reason":"end_turn","usage":{"input_tokens":9,"output_tokens":3}}"#;
const OPENAI_REPLY: &str = r#"{"id":"chatcmpl-1","object":"chat.completion","model":"gpt-4o-2024-08-06","choices":[{"index":0,"message":{"role":"assistant","content":"Bonjour"},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":1,"total_tokens":10}}"#;
const LOCAL_REPLY: &str = r#"{"id":"chatcmpl-77","object":"chat.completion","model":"kat-coder","choices":[{"index":0,"message":{"role":"assistant","content":"local says hi"},"finish_reason":"stop"}]}"#;

#[tokio::test]
async fn anthropic_posts_a_messages_request_and_parses_the_text_blocks() {
    let (url, rx) = serve(vec![(200, ANTHROPIC_REPLY.into())]);
    let m = AnthropicModel::new("sk-ant-test", "claude-sonnet-4-5").with_base_url(url);
    let mut req = ModelRequest::new("Say hi");
    req.model = Some("claude-haiku-4-5".into());
    req.max_tokens = Some(50);
    let r = m.complete(&req).await.unwrap();
    assert_eq!(r.text, "Hello there");
    let seen = rx.recv().unwrap();
    assert_eq!(seen.path, "/v1/messages");
    assert_eq!(seen.headers["x-api-key"], "sk-ant-test");
    assert_eq!(seen.headers["anthropic-version"], "2023-06-01");
    assert_eq!(
        seen.body,
        json!({"model":"claude-haiku-4-5","max_tokens":50,"messages":[{"role":"user","content":"Say hi"}]})
    );
}

#[tokio::test]
async fn openai_posts_a_chat_completions_request_and_parses_the_first_choice() {
    let (url, rx) = serve(vec![(200, OPENAI_REPLY.into())]);
    let m = OpenAiModel::new("sk-test", "gpt-4o").with_base_url(format!("{url}/v1"));
    let mut req = ModelRequest::new("Say hi in French");
    req.max_tokens = Some(20);
    let r = m.complete(&req).await.unwrap();
    assert_eq!(r.text, "Bonjour");
    assert_eq!(r.matched_prefix, None);
    let seen = rx.recv().unwrap();
    assert_eq!(seen.path, "/v1/chat/completions");
    assert_eq!(seen.headers["authorization"], "Bearer sk-test");
    assert_eq!(
        seen.body,
        json!({"model":"gpt-4o","messages":[{"role":"user","content":"Say hi in French"}],"max_completion_tokens":20})
    );
}

#[tokio::test]
async fn openai_without_a_token_limit_sends_none() {
    let (url, rx) = serve(vec![(200, OPENAI_REPLY.into())]);
    let m = OpenAiModel::new("k", "gpt-4o").with_base_url(url);
    m.complete(&ModelRequest::new("x")).await.unwrap();
    let seen = rx.recv().unwrap();
    assert_eq!(seen.path, "/chat/completions");
    assert!(seen.body.get("max_completion_tokens").is_none() && seen.body.get("max_tokens").is_none());
}

#[tokio::test]
async fn local_posts_to_the_configured_base_url_without_a_key() {
    let (url, rx) = serve(vec![(200, LOCAL_REPLY.into())]);
    let m = LocalModel::new(format!("{url}/v1/"), "kat-coder", None);
    let mut req = ModelRequest::new("hello");
    req.max_tokens = Some(7);
    let r = m.complete(&req).await.unwrap();
    assert_eq!(r.text, "local says hi");
    let seen = rx.recv().unwrap();
    assert_eq!(seen.path, "/v1/chat/completions");
    assert!(!seen.headers.contains_key("authorization"));
    assert_eq!(seen.body, json!({"model":"kat-coder","messages":[{"role":"user","content":"hello"}],"max_tokens":7}));
}

#[tokio::test]
async fn http_errors_and_odd_replies_are_reported_with_the_status() {
    let (url, _rx) = serve(vec![(401, r#"{"error":{"message":"bad key"}}"#.into()), (200, r#"{"choices":[]}"#.into())]);
    let m = OpenAiModel::new("k", "gpt-4o").with_base_url(url);
    let e = m.complete(&ModelRequest::new("x")).await.unwrap_err().to_string();
    assert!(e.contains("401") && e.contains("bad key"), "{e}");
    let e = m.complete(&ModelRequest::new("x")).await.unwrap_err().to_string();
    assert!(e.contains("unexpected response"), "{e}");
}

#[test]
fn content_parts_are_joined() {
    let v = json!({"choices":[{"message":{"content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]}}]});
    assert_eq!(OpenAiModel::parse_response(&v).unwrap(), "ab");
    let v = json!({"choices":[{"message":{"content":null}}]});
    assert!(OpenAiModel::parse_response(&v).is_err());
}

// ---------------------------------------------------------------------------------- router

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + Send + Sync + 'static {
    let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    move |k| m.get(k).cloned()
}

#[tokio::test]
async fn the_router_sends_each_agent_to_the_provider_of_its_model_line() {
    let (openai, orx) = serve(vec![(200, OPENAI_REPLY.into())]);
    let (local, lrx) = serve(vec![(200, LOCAL_REPLY.into())]);
    let (claude, crx) = serve(vec![(200, ANTHROPIC_REPLY.into())]);
    let cfg = ProviderConfig::new(&[], Some(&format!("{local}/v1")));
    let r = Router::new(
        cfg,
        env(&[
            ("OPENAI_API_KEY", "sk-o"),
            ("OPENAI_BASE_URL", &format!("{openai}/v1")),
            ("ANTHROPIC_API_KEY", "sk-a"),
            ("ANTHROPIC_BASE_URL", &claude),
        ]),
    )
    .unwrap();
    let ask = |id: &str| {
        let mut q = ModelRequest::new("p");
        q.model = Some(id.into());
        q
    };
    assert_eq!(r.complete(&ask("openai:gpt-4o-mini")).await.unwrap().text, "Bonjour");
    assert_eq!(r.complete(&ask("local:kat-coder")).await.unwrap().text, "local says hi");
    assert_eq!(r.complete(&ask("claude-haiku-4-5")).await.unwrap().text, "Hello there");
    assert_eq!(orx.recv().unwrap().body["model"], "gpt-4o-mini");
    assert_eq!(lrx.recv().unwrap().body["model"], "kat-coder");
    assert_eq!(crx.recv().unwrap().body["model"], "claude-haiku-4-5");
}

#[tokio::test]
async fn a_missing_key_is_reported_at_the_first_question() {
    let r = Router::new(ProviderConfig::default(), env(&[])).unwrap();
    let mut q = ModelRequest::new("p");
    q.model = Some("openai:gpt-4o".into());
    assert_eq!(r.complete(&q).await.unwrap_err().to_string(), "environment variable OPENAI_API_KEY is not set");
    q.model = Some("claude-sonnet-4-5".into());
    assert_eq!(r.complete(&q).await.unwrap_err().to_string(), "environment variable ANTHROPIC_API_KEY is not set");
}

fn envs() -> ProviderConfig {
    ProviderConfig::new(&[("production", "claude"), ("test", "mock"), ("dev", "local:kat-coder")], Some("http://127.0.0.1:1/v1"))
}

#[tokio::test]
async fn agentmark_env_replaces_every_agents_model_with_the_mapped_driver() {
    let (local, lrx) = serve(vec![(200, LOCAL_REPLY.into())]);
    let cfg = ProviderConfig { local_base_url: Some(format!("{local}/v1")), ..envs() };
    let r = Router::new(cfg, env(&[("AGENTMARK_ENV", "dev"), ("OPENAI_API_KEY", "unused")])).unwrap();
    assert_eq!(r.forced(), Some(&Driver::Local("kat-coder".into())));
    let mut q = ModelRequest::new("p");
    q.model = Some("openai:gpt-4o".into());
    assert_eq!(r.complete(&q).await.unwrap().text, "local says hi");
    assert_eq!(lrx.recv().unwrap().body["model"], "kat-coder");
}

#[tokio::test]
async fn the_test_environment_maps_to_the_mock_model() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("model.json"), r#"{"today":"2026-10-13","responses":[{"prompt_prefix":"Q","response":"canned"}]}"#).unwrap();
    let dir = d.path().to_str().unwrap().to_string();
    let r = Router::new(envs(), env(&[("AGENTMARK_ENV", "test"), ("AGENTMARK_FIXTURES", &dir)])).unwrap();
    let mut q = ModelRequest::new("Q1");
    q.model = Some("openai:gpt-4o".into());
    let a = r.complete(&q).await.unwrap();
    assert_eq!((a.text.as_str(), a.matched_prefix.as_deref()), ("canned", Some("Q")));
    assert_eq!(r.mock_today().unwrap().to_string(), "2026-10-13");
}

#[test]
fn environment_selection_rules() {
    // unset: every agent keeps its own model line
    assert!(Router::new(envs(), env(&[])).unwrap().forced().is_none());
    // production without an entry is the default: no override
    assert!(Router::new(ProviderConfig::new(&[("test", "mock")], None), env(&[("AGENTMARK_ENV", "production")])).unwrap().forced().is_none());
    // no [models] at all: AGENTMARK_ENV means nothing
    assert!(Router::new(ProviderConfig::default(), env(&[("AGENTMARK_ENV", "dev")])).unwrap().forced().is_none());
    // an unknown environment is named, with the known ones
    let e = Router::new(envs(), env(&[("AGENTMARK_ENV", "staging")])).unwrap_err().to_string();
    assert!(e.contains("staging") && e.contains("dev, production, test"), "{e}");
    // AGENTMARK_MODEL wins over AGENTMARK_ENV
    let r = Router::new(envs(), env(&[("AGENTMARK_ENV", "dev"), ("AGENTMARK_MODEL", "openai:gpt-4o"), ("OPENAI_API_KEY", "k")])).unwrap();
    assert_eq!(r.forced(), Some(&Driver::OpenAi("gpt-4o".into())));
    // a mapped driver that is not supported is an error, not a silent fallback
    let bad = ProviderConfig::new(&[("prod", "azure:x")], None);
    assert!(Router::new(bad, env(&[("AGENTMARK_ENV", "prod")])).unwrap_err().to_string().contains("not supported in this version"));
}

#[tokio::test]
async fn plain_claude_keeps_an_agents_own_claude_model_but_replaces_other_providers() {
    let (claude, crx) = serve(vec![(200, ANTHROPIC_REPLY.into()), (200, ANTHROPIC_REPLY.into())]);
    let r = Router::new(
        envs(),
        env(&[("AGENTMARK_ENV", "production"), ("ANTHROPIC_API_KEY", "k"), ("ANTHROPIC_BASE_URL", &claude)]),
    )
    .unwrap();
    for (asked, sent) in [("claude-haiku-4-5", "claude-haiku-4-5"), ("openai:gpt-4o", "claude-sonnet-4-5")] {
        let mut q = ModelRequest::new("p");
        q.model = Some(asked.into());
        r.complete(&q).await.unwrap();
        assert_eq!(crx.recv().unwrap().body["model"], sent);
    }
}

#[test]
fn mock_files_load_from_the_driver_path() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("m.json");
    std::fs::write(&f, r#"{"responses":[]}"#).unwrap();
    assert!(MockModel::from_file(&f).is_ok());
    let spec = format!("mock:{}", f.display());
    assert!(Router::new(ProviderConfig::default(), env(&[("AGENTMARK_MODEL", &spec)])).is_ok());
    let missing = format!("mock:{}/nope.json", d.path().display());
    assert!(Router::new(ProviderConfig::default(), env(&[("AGENTMARK_MODEL", &missing)])).is_err());
}
