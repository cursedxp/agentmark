//! Model providers and the `[models]` table (PROMPT step 14, SPEC 19): generated programs and the
//! binder pick their model from `- model:` lines, `AGENTMARK_ENV` and `AGENTMARK_MODEL`. Every
//! provider is a local test server that records the request and replies with a recorded body; no
//! live calls.

mod support;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use serde_json::{json, Value};
use support::*;

#[derive(Debug)]
struct Seen {
    path: String,
    headers: HashMap<String, String>,
    body: Value,
}

/// Serve the replies to successive connections; the receiver yields each request seen.
fn serve(replies: Vec<String>) -> (String, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for reply in replies {
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
            let headers = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string())).collect();
            let body = serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or(Value::Null);
            tx.send(Seen { path, headers, body }).unwrap();
            let http = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}", reply.len());
            sock.write_all(http.as_bytes()).unwrap();
        }
    });
    (url, rx)
}

fn chat(text: &str) -> String {
    json!({"id":"chatcmpl-1","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":text},"finish_reason":"stop"}]}).to_string()
}

/// Two agents on two providers, writing what each answers.
const TWO: &str = "# Agent: cloud\n- model: openai:gpt-4o-mini\n- tools: log\n\n## On start\n- ask model: \"Q cloud\" -> a\n- log: \"cloud says {a}\"\n\n# Agent: office\n- model: local:kat-coder\n- tools: log\n\n## On start\n- ask model: \"Q office\" -> b\n- log: \"office says {b}\"\n";

/// A project with `TWO`, the `[models]` table pointing the local provider at `local_url`.
fn project(tag: &str, local_url: &str) -> (Tmp, String) {
    let d = temp(tag);
    write(&d, "agentmark.toml", &format!("name = \"two\"\n\n[models]\nproduction = \"claude\"\ntest = \"mock\"\ndev = \"local:dev-model\"\n\n[models.local]\nbase_url = \"{local_url}/v1\"\n"));
    write(&d, "two.agent.md", TWO);
    write(&d, "tools.toml", &std::fs::read_to_string(format!("{ROOT}/tests/errors/tools.toml")).unwrap());
    let file = d.join("two.agent.md").to_string_lossy().to_string();
    (d, file)
}

fn run_real(file: &str, env: &[(&str, &str)]) -> std::process::Output {
    let mut c = agentmark(&["run", file]);
    c.env_remove("AGENTMARK_ENV").env_remove("OPENAI_API_KEY").env_remove("OPENAI_BASE_URL").env_remove("AGENTMARK_LOCAL_BASE_URL");
    for (k, v) in env {
        c.env(k, v);
    }
    c.output_locked()
}

#[test]
fn each_agent_reaches_the_provider_of_its_model_line() {
    let (openai, orx) = serve(vec![chat("hello from openai")]);
    let (local, lrx) = serve(vec![chat("hello from local")]);
    let (_d, file) = project("two-providers", &local);
    let o = run_real(&file, &[("OPENAI_API_KEY", "sk-test"), ("OPENAI_BASE_URL", &format!("{openai}/v1"))]);
    assert!(o.status.success(), "{}", stderr(&o));
    let e = stderr(&o);
    assert!(e.contains("cloud says hello from openai") && e.contains("office says hello from local"), "{e}");
    let o = orx.recv().unwrap();
    assert_eq!(o.path, "/v1/chat/completions");
    assert_eq!(o.headers["authorization"], "Bearer sk-test");
    assert_eq!(o.body["model"], "gpt-4o-mini");
    assert!(o.body["messages"][0]["content"].as_str().unwrap().starts_with("Q cloud"));
    let l = lrx.recv().unwrap();
    assert_eq!(l.path, "/v1/chat/completions");
    assert_eq!(l.body["model"], "kat-coder");
    assert!(!l.headers.contains_key("authorization"));
}

#[test]
fn agentmark_env_picks_the_mapped_driver_for_every_agent() {
    // dev = "local:dev-model": both agents ask the local server, the OpenAI one is never called
    let (local, lrx) = serve(vec![chat("one"), chat("two")]);
    let (_d, file) = project("env-dev", &local);
    let o = run_real(&file, &[("AGENTMARK_ENV", "dev")]);
    assert!(o.status.success(), "{}", stderr(&o));
    let e = stderr(&o);
    assert!(e.contains("cloud says one") && e.contains("office says two"), "{e}");
    for _ in 0..2 {
        assert_eq!(lrx.recv().unwrap().body["model"], "dev-model");
    }
}

#[test]
fn the_test_environment_runs_on_the_mock_model_without_network() {
    let (_d, file) = project("env-test", "http://127.0.0.1:1");
    let dir = std::path::Path::new(&file).parent().unwrap().join("fx");
    write(&dir, "model.json", r#"{"today":"2026-10-13","responses":[{"prompt_prefix":"Q cloud","response":"canned a"},{"prompt_prefix":"Q office","response":"canned b"}]}"#);
    let o = run_real(&file, &[("AGENTMARK_ENV", "test"), ("AGENTMARK_FIXTURES", dir.to_str().unwrap())]);
    assert!(o.status.success(), "{}", stderr(&o));
    let e = stderr(&o);
    assert!(e.contains("cloud says canned a") && e.contains("office says canned b"), "{e}");
}

#[test]
fn agentmark_model_wins_over_agentmark_env() {
    let (local, lrx) = serve(vec![chat("x"), chat("y")]);
    let (_d, file) = project("model-wins", &local);
    let o = run_real(&file, &[("AGENTMARK_ENV", "test"), ("AGENTMARK_MODEL", "local:forced")]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(lrx.recv().unwrap().body["model"], "forced");
}

#[test]
fn an_environment_that_is_not_in_the_table_is_named() {
    let (_d, file) = project("env-unknown", "http://127.0.0.1:1");
    let o = run_real(&file, &[("AGENTMARK_ENV", "staging")]);
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("AGENTMARK_ENV=\"staging\" is not in [models]"), "{}", stderr(&o));
}

#[test]
fn the_local_base_url_comes_from_agentmark_toml_and_the_environment_overrides_it() {
    let (local, lrx) = serve(vec![chat("x")]);
    let src = "# Agent: office\n- model: local:kat-coder\n- tools: log\n\n## On start\n- ask model: \"Q office\" -> b\n- log: \"{b}\"\n";
    let d = temp("local-url");
    write(&d, "agentmark.toml", "name = \"l\"\n\n[models.local]\nbase_url = \"http://127.0.0.1:1/v1\"\n");
    write(&d, "l.agent.md", src);
    write(&d, "tools.toml", &std::fs::read_to_string(format!("{ROOT}/tests/errors/tools.toml")).unwrap());
    let file = d.join("l.agent.md").to_string_lossy().to_string();
    let o = run_real(&file, &[]);
    assert!(!o.status.success(), "port 1 refuses the connection");
    let o = run_real(&file, &[("AGENTMARK_LOCAL_BASE_URL", &format!("{local}/v1"))]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(lrx.recv().unwrap().path, "/v1/chat/completions");
}

// ---------------------------------------------------------------------------------- check

#[test]
fn check_rejects_unsupported_providers_at_the_model_line() {
    let d = temp("check-azure");
    write(&d, "a.agent.md", "# Agent: a\n- model: azure:dep\n- tools: log\n\n## On start\n- log: \"x\"\n");
    write(&d, "tools.toml", &std::fs::read_to_string(format!("{ROOT}/tests/errors/tools.toml")).unwrap());
    let o = agentmark(&["check", d.join("a.agent.md").to_str().unwrap()]).output().unwrap();
    assert_eq!(o.status.code(), Some(1));
    let e = stderr(&o) + &stdout(&o);
    assert!(e.contains("a.agent.md:2") && e.contains("The model provider \"azure\" is not supported in this version."), "{e}");
}

#[test]
fn check_accepts_every_supported_driver() {
    let d = temp("check-ok");
    write(&d, "a.agent.md", "# Agent: a\n- model: openai:gpt-4o\n- tools: log\n\n## On start\n- log: \"x\"\n\n# Agent: b\n- model: local:kat-coder\n- tools: log\n\n## On start\n- log: \"y\"\n\n# Agent: c\n- model: claude:haiku\n- tools: log\n\n## On start\n- log: \"z\"\n\n# Agent: d\n- model: mock\n- tools: log\n\n## On start\n- log: \"w\"\n");
    write(&d, "tools.toml", &std::fs::read_to_string(format!("{ROOT}/tests/errors/tools.toml")).unwrap());
    let o = agentmark(&["check", d.join("a.agent.md").to_str().unwrap()]).output().unwrap();
    assert!(o.status.success(), "{}{}", stderr(&o), stdout(&o));
}

#[test]
fn the_models_table_is_validated_in_agentmark_toml() {
    let bad = |tag: &str, toml: &str| {
        let d = temp(tag);
        write(&d, "agentmark.toml", toml);
        write(&d, "a.agent.md", "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- log: \"x\"\n");
        write(&d, "tools.toml", &std::fs::read_to_string(format!("{ROOT}/tests/errors/tools.toml")).unwrap());
        let o = agentmark(&["check", d.to_str().unwrap()]).output().unwrap();
        (o.status.code(), stderr(&o) + &stdout(&o))
    };
    let (c, e) = bad("models-azure", "[models]\nproduction = \"azure:x\"\n");
    assert_eq!(c, Some(1));
    assert!(e.contains("agentmark.toml:2") && e.contains("production = \"azure:x\": The model provider \"azure\" is not supported in this version."), "{e}");
    let (_, e) = bad("models-url", "[models.local]\nbase_url = \"localhost\"\n");
    assert!(e.contains("must be a web address"), "{e}");
    let (_, e) = bad("models-key", "[models.local]\nurl = \"http://x\"\n");
    assert!(e.contains("Unknown key \"url\" under [models.local]"), "{e}");
    let (c, e) = bad("models-ok", "[models]\nproduction = \"claude\"\ntest = \"mock\"\ndev = \"local:kat-coder\"\n\n[models.local]\nbase_url = \"http://localhost:11434/v1\"\n");
    assert_eq!(c, Some(0), "{e}");
}

// ---------------------------------------------------------------------------------- binder

#[test]
fn the_binder_uses_the_models_table_too() {
    // AGENTMARK_ENV=test -> the canned model named by the table; no AGENTMARK_MODEL, no key
    let d = temp("bind-env");
    write(&d, "agentmark.toml", &format!("name = \"p\"\n\n[models]\ntest = \"mock:{ROOT}/tests/binder/model.json\"\n"));
    write(&d, "main.agent.md", "# Import: ./fingerprint.agent.md\n\n# Run\n- fingerprint\n");
    write(&d, "fingerprint.agent.md", "# Import: crate sha2\n\n> Prints a fingerprint.\n# Command: fingerprint\n- args: payload: bytes\n\n## Run\n- sha2 sha256_digest payload -> digest\n- print: \"done\"\n");
    let o = agentmark(&["bind", "sha2", "--project", d.to_str().unwrap()])
        .env("AGENTMARK_ENV", "test")
        .env("AGENTMARK_RUSTDOC_JSON", format!("{ROOT}/tests/binder/sha2.json"))
        .env("AGENTMARK_RUSTDOC_JSON_DIR", format!("{ROOT}/tests/binder"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}{}", stderr(&o), stdout(&o));
    assert!(d.join("bindings/sha2.agent.md").is_file());
}
