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
    let text = "# Constant: a\n- value: 14 days   \n";
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
