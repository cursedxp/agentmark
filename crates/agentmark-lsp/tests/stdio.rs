//! The real binary over stdin/stdout: what the VS Code client will launch.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn frame(v: &serde_json::Value) -> Vec<u8> {
    let body = v.to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

fn read_message(r: &mut impl BufRead) -> serde_json::Value {
    let mut len = 0usize;
    loop {
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some(n) = line.strip_prefix("Content-Length:") {
            len = n.trim().parse().unwrap();
        }
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).unwrap();
    serde_json::from_slice(&buf).unwrap()
}

#[test]
fn the_binary_initializes_serves_a_request_and_exits_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("c.agent.md");
    let text = "# Constant: a\n- value: 14 days\n";
    std::fs::write(&file, text).unwrap();
    let uri = agentmark_lsp::uri::uri_from_path(&file.to_string_lossy());

    let mut child = Command::new(env!("CARGO_BIN_EXE_agentmark-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let send = |stdin: &mut std::process::ChildStdin, v: serde_json::Value| stdin.write_all(&frame(&v)).unwrap();

    send(&mut stdin, serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}));
    let init = read_message(&mut out);
    let caps = &init["result"]["capabilities"];
    assert_eq!(caps["documentFormattingProvider"], true);
    assert_eq!(caps["hoverProvider"], true);
    assert_eq!(caps["definitionProvider"], true);
    assert!(caps["completionProvider"].is_object() && caps["codeActionProvider"].is_object());
    assert_eq!(init["result"]["serverInfo"]["name"], "agentmark-lsp");
    send(&mut stdin, serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/formatting","params":{"textDocument":{"uri":uri.as_str()},"options":{"tabSize":2,"insertSpaces":true}}}),
    );
    let fmt = read_message(&mut out);
    assert_eq!(fmt["id"], 2);
    assert_eq!(fmt["result"][0]["newText"], "# Constant: a\n- value: 14 days\n");
    send(&mut stdin, serde_json::json!({"jsonrpc":"2.0","id":3,"method":"shutdown"}));
    assert_eq!(read_message(&mut out)["id"], 3);
    send(&mut stdin, serde_json::json!({"jsonrpc":"2.0","method":"exit"}));
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn exit_without_shutdown_is_exit_code_1() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_agentmark-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    stdin.write_all(&frame(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}))).unwrap();
    read_message(&mut out);
    stdin.write_all(&frame(&serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}))).unwrap();
    stdin.write_all(&frame(&serde_json::json!({"jsonrpc":"2.0","method":"exit"}))).unwrap();
    drop(stdin);
    assert_eq!(child.wait().unwrap().code(), Some(1));
}

#[test]
fn the_binary_picks_the_model_like_the_binder_does() {
    // no AGENTMARK_MODEL, no key: AGENTMARK_ENV=test names the canned model in the project's [models]
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.json");
    let explain = agentmark_lsp::EXPLAIN_PROMPT;
    std::fs::write(&model, serde_json::json!({ "responses": [{ "prompt_prefix": explain, "response": "It says hi." }] }).to_string()).unwrap();
    std::fs::write(
        dir.path().join("agentmark.toml"),
        format!("name = \"p\"\n\n[lsp]\nmodel_actions = true\n\n[models]\ntest = \"mock:{}\"\n", model.display()),
    )
    .unwrap();
    let file = dir.path().join("main.agent.md");
    let text = "# Agent: a\nSays.\n\n- model: mock\n- tools: log\n\n## On start\n1. log: \"hi\"\n";
    std::fs::write(&file, text).unwrap();
    let uri = agentmark_lsp::uri::uri_from_path(&file.to_string_lossy());

    let run = |env: Option<&str>| -> serde_json::Value {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_agentmark-lsp"));
        cmd.env_remove("AGENTMARK_MODEL").env_remove("AGENTMARK_ENV").env_remove("ANTHROPIC_API_KEY");
        if let Some(e) = env {
            cmd.env("AGENTMARK_ENV", e);
        }
        let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let send = |stdin: &mut std::process::ChildStdin, v: serde_json::Value| stdin.write_all(&frame(&v)).unwrap();
        send(&mut stdin, serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}));
        read_message(&mut out);
        send(&mut stdin, serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
        send(&mut stdin, serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri.as_str(),"languageId":"agentmark","version":1,"text":text}}}));
        send(
            &mut stdin,
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"workspace/executeCommand","params":{"command":"agentmark.explainBlock","arguments":[uri.as_str(), 5]}}),
        );
        // diagnostics / showMessage notifications may come first
        let reply = loop {
            let m = read_message(&mut out);
            if m["id"] == 2 {
                break m;
            }
        };
        send(&mut stdin, serde_json::json!({"jsonrpc":"2.0","id":3,"method":"shutdown"}));
        loop {
            if read_message(&mut out)["id"] == 3 {
                break;
            }
        }
        send(&mut stdin, serde_json::json!({"jsonrpc":"2.0","method":"exit"}));
        drop(stdin);
        child.wait().unwrap();
        reply
    };

    let ok = run(Some("test"));
    assert_eq!(ok["result"], "It says hi.", "{ok}");
    // without the environment nothing picks a model (Claude, no key)
    let none = run(None);
    assert!(none["error"]["message"].as_str().unwrap_or("").contains("no model is configured"), "{none}");
}
