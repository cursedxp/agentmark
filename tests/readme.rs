//! The README cannot rot (PROMPT step 11). Its code blocks are run against the built binary,
//! offline and with the mock model:
//! - every ```agentmark block is a complete file and must `check` clean;
//! - every ```console block is a list of commands (`$ agentmark …`, `NAME=value` prefixes allowed)
//!   followed by the output fragments that must appear (a trailing `…` cuts a line short, a line
//!   of only `…` is skipped). Directives: `# exit N` (the expected exit code of the command above,
//!   default 0) and `# in FILE: replace "A" with "B"` (the edit the reader makes by hand).
//!   The words `app` and `out` stand for scratch folders; ```console app starts with `app` = a copy
//!   of examples/05-invoice-service without its sha2 binding. `$PWD` is the repository root.
//! - the ```rust file=rust/connect.rs block is dropped into a copy of example 01 and must compile
//!   and run up to the model call.

mod support;

use std::path::{Path, PathBuf};
use std::process::Output;

use support::*;

struct Block {
    info: String,
    body: String,
    line: usize,
}

fn blocks() -> Vec<Block> {
    let text = std::fs::read_to_string(format!("{ROOT}/README.md")).unwrap();
    let mut out = Vec::new();
    let mut open: Option<Block> = None;
    for (i, l) in text.lines().enumerate() {
        match (&mut open, l.strip_prefix("```")) {
            (None, Some(info)) => open = Some(Block { info: info.trim().to_string(), body: String::new(), line: i + 1 }),
            (Some(_), Some(_)) => out.push(open.take().unwrap()),
            (Some(b), None) => {
                b.body.push_str(l);
                b.body.push('\n');
            }
            (None, None) => {}
        }
    }
    assert!(open.is_none(), "README.md: a code block is not closed");
    out
}

fn with_info<'a>(all: &'a [Block], first_word: &str) -> Vec<&'a Block> {
    all.iter().filter(|b| b.info.split_whitespace().next() == Some(first_word)).collect()
}

/// Cargo-building commands run under the build lock.
fn run_command(mut cmd: std::process::Command, builds: bool) -> Output {
    if builds {
        cmd.output_locked()
    } else {
        cmd.output().unwrap()
    }
}

#[test]
fn agentmark_blocks_check_clean() {
    let all = blocks();
    let found = with_info(&all, "agentmark");
    assert!(!found.is_empty(), "README.md has no ```agentmark block");
    for b in found {
        let d = temp("readme-agentmark");
        std::fs::copy(format!("{ROOT}/examples/01-invoice-chaser/tools.toml"), d.join("tools.toml")).unwrap();
        write(&d, "block.agent.md", &b.body);
        let o = run_command(agentmark(&["check", s(&d.join("block.agent.md"))]), false);
        assert!(o.status.success(), "README.md:{}: the agentmark block does not check:\n{}{}", b.line, stdout(&o), stderr(&o));
        assert!(stdout(&o).contains("ok: 0 errors, 0 warnings."), "README.md:{}: not clean:\n{}", b.line, stdout(&o));
    }
}

/// A command word that names a scratch place.
fn place(word: &str, dir: &Path) -> String {
    for name in ["app", "out"] {
        if word == name || word.starts_with(&format!("{name}/")) {
            return format!("{}/{word}", s(dir));
        }
    }
    word.to_string()
}

fn head(t: &str) -> String {
    t.chars().take(2500).collect()
}

fn run_console(b: &Block) {
    let d = temp("readme-console");
    if b.info.split_whitespace().nth(1) == Some("app") {
        copy_dir(Path::new(&format!("{ROOT}/examples/05-invoice-service")), &d.join("app"));
        std::fs::remove_file(d.join("app/bindings/sha2.agent.md")).unwrap();
    }
    let scrub = |t: &str| t.replace(&format!("{}/", s(&d)), "").replace(&format!("{ROOT}/"), "");

    // (command line, expected exit, expected fragments)
    let mut steps: Vec<(String, i32, Vec<String>)> = Vec::new();
    let mut edits: Vec<(usize, String)> = Vec::new();
    for l in b.body.lines() {
        if let Some(cmd) = l.strip_prefix("$ ") {
            steps.push((cmd.to_string(), 0, vec![]));
        } else if let Some(n) = l.strip_prefix("# exit ") {
            steps.last_mut().expect("`# exit` before any command").1 = n.trim().parse().unwrap();
        } else if l.starts_with("# in ") {
            edits.push((steps.len(), l.to_string()));
        } else if !l.trim().is_empty() && l.trim() != "…" {
            let frag = l.trim_end().trim_end_matches('…').to_string();
            steps.last_mut().expect("output before any command").2.push(frag);
        }
    }
    assert!(!steps.is_empty(), "README.md:{}: console block without a command", b.line);

    for (i, (cmd, code, frags)) in steps.iter().enumerate() {
        // the hand edits that happen after the previous command
        for (_, e) in edits.iter().filter(|(at, _)| *at == i) {
            apply_edit(e, &d, b.line);
        }
        let mut env = Vec::new();
        let mut words = cmd.split_whitespace().peekable();
        while let Some(w) = words.peek() {
            match w.split_once('=') {
                Some((k, v)) if !k.is_empty() && !k.contains('/') => {
                    env.push((k.to_string(), v.replace("$PWD", ROOT)));
                    words.next();
                }
                _ => break,
            }
        }
        assert_eq!(words.next(), Some("agentmark"), "README.md:{}: `{cmd}` must start with agentmark", b.line);
        let args: Vec<String> = words.map(|w| place(w, &d)).collect();
        let args: Vec<&str> = args.iter().map(|a| a.as_str()).collect();
        let mut c = agentmark(&args);
        for (k, v) in &env {
            c.env(k, v);
        }
        let o = run_command(c, matches!(args[0], "run" | "trace" | "build"));
        let got = scrub(&format!("{}{}", stdout(&o), stderr(&o)));
        assert_eq!(o.status.code(), Some(*code), "README.md:{}: `{cmd}` exited differently\n{}", b.line, head(&got));
        for f in frags {
            assert!(got.contains(f.as_str()), "README.md:{}: `{cmd}` did not print:\n  {f}\nit printed:\n{}", b.line, head(&got));
        }
    }
}

/// `# in FILE: replace "A" with "B"`.
fn apply_edit(directive: &str, dir: &Path, line: usize) {
    let rest = directive.strip_prefix("# in ").unwrap();
    let (file, what) = rest.split_once(": replace ").unwrap_or_else(|| panic!("README.md:{line}: bad directive {directive}"));
    let quoted: Vec<&str> = what.split('"').collect();
    assert!(quoted.len() == 5, "README.md:{line}: bad directive {directive}");
    let path = PathBuf::from(place(file, dir));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(quoted[1]), "README.md:{line}: {file} has no line {:?}", quoted[1]);
    std::fs::write(&path, text.replacen(quoted[1], quoted[3], 1)).unwrap();
}

#[test]
fn console_blocks_run() {
    let all = blocks();
    let found = with_info(&all, "console");
    assert!(found.len() >= 8, "README.md should show every command, found {} console blocks", found.len());
    for b in found {
        run_console(b);
    }
    // the commands only read the repository
    let status = std::process::Command::new("git").args(["status", "--porcelain", "examples", "fixtures"]).current_dir(ROOT).output().unwrap();
    assert_eq!(stdout(&status), "", "README commands changed files in the repository");
}

#[test]
fn every_command_is_shown() {
    let all = blocks();
    let shown: String = with_info(&all, "console").iter().map(|b| b.body.clone()).collect();
    for c in ["check", "build", "run", "trace", "bind", "fmt", "lint", "doc"] {
        assert!(shown.contains(&format!("agentmark {c} ")) || shown.contains(&format!(" agentmark {c} ")), "README.md shows no `agentmark {c}` command");
    }
}

#[test]
fn connect_rs_of_the_readme_compiles_and_runs() {
    let all = blocks();
    let b = all.iter().find(|b| b.info == "rust file=rust/connect.rs").expect("README.md has no ```rust file=rust/connect.rs block");
    let d = temp("readme-connect");
    let project = d.join("chaser");
    copy_dir(Path::new(&format!("{ROOT}/examples/01-invoice-chaser")), &project);
    write(&project, "rust/connect.rs", &b.body);
    // a real run (no --mock) reads the invoices through the README's MemoryCrm, then asks the real
    // model, which cannot be reached without a key: the run gets that far. The build cache of the
    // run goes to a TMPDIR of its own, so nothing is left behind.
    let cache = temp("readme-connect-cache");
    let mut c = agentmark(&["run", s(&project)]);
    c.env("TMPDIR", s(&cache));
    let o = c.output_locked();
    let err = stderr(&o);
    assert!(!err.contains("not connected yet"), "stub still in use:\n{err}");
    assert!(!err.contains("error["), "README connect.rs does not compile:\n{err}");
    assert!(err.contains("environment variable ANTHROPIC_API_KEY is not set"), "the run did not reach the model call:\n{err}");
}
