//! The README cannot rot (PROMPT step 11). Its code blocks are run against the built binary,
//! offline and with the mock model:
//! - every ```agentmark block is a complete file and must `check` clean;
//! - every ```console block is a list of commands (`$ agentmark …`, `NAME=value` prefixes allowed)
//!   followed by the output fragments that must appear (a trailing `…` cuts a line short, a line
//!   of only `…` is skipped). Directives: `# exit N` (the expected exit code of the command above,
//!   default 0) and `# in FILE: replace "A" with "B"` (the edit the reader makes by hand).
//!   The words `app` and `out` stand for scratch folders; ```console app starts with `app` = a copy
//!   of examples/05-invoice-service without its sha2 binding. `$PWD` is the repository root.
//!   ```console connect: `app` = a copy of example 01 with the ```rust file=rust/connect.rs block.
//! The blocks come from README.md and from docs/connect.md (the page about `rust/connect.rs`).
//! - the ```rust http block (with the ```toml agentmark.toml block appended to agentmark.toml) must
//!   build: `[rust.dependencies]` brings in reqwest, nothing calls a network.
//! - the ```rust trait block must appear verbatim in the generated `src/tools.rs`.

mod support;

use std::path::{Path, PathBuf};
use std::process::Output;

use support::*;

struct Block {
    file: &'static str,
    info: String,
    body: String,
    line: usize,
}

/// The pages whose fenced blocks are tested here.
const PAGES: [&str; 2] = ["README.md", "docs/connect.md"];

fn blocks() -> Vec<Block> {
    PAGES.iter().flat_map(|p| blocks_of(p)).collect()
}

fn blocks_of(file: &'static str) -> Vec<Block> {
    let text = std::fs::read_to_string(format!("{ROOT}/{file}")).unwrap();
    let mut out = Vec::new();
    let mut open: Option<Block> = None;
    for (i, l) in text.lines().enumerate() {
        match (&mut open, l.strip_prefix("```")) {
            (None, Some(info)) => open = Some(Block { file, info: info.trim().to_string(), body: String::new(), line: i + 1 }),
            (Some(_), Some(_)) => out.push(open.take().unwrap()),
            (Some(b), None) => {
                b.body.push_str(l);
                b.body.push('\n');
            }
            (None, None) => {}
        }
    }
    assert!(open.is_none(), "{file}: a code block is not closed");
    out
}

impl Block {
    fn at(&self) -> String {
        format!("{}:{}", self.file, self.line)
    }
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
        assert!(o.status.success(), "{}: the agentmark block does not check:\n{}{}", b.at(), stdout(&o), stderr(&o));
        assert!(stdout(&o).contains("ok: 0 errors, 0 warnings."), "{}: not clean:\n{}", b.at(), stdout(&o));
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
    std::fs::create_dir_all(d.join("tmp")).unwrap();
    match b.info.split_whitespace().nth(1) {
        Some("app") => {
            copy_dir(Path::new(&format!("{ROOT}/examples/05-invoice-service")), &d.join("app"));
            std::fs::remove_file(d.join("app/bindings/sha2.agent.md")).unwrap();
        }
        Some("connect") => {
            copy_dir(Path::new(&format!("{ROOT}/examples/01-invoice-chaser")), &d.join("app"));
            write(&d.join("app"), "rust/connect.rs", &readme_block("rust file=rust/connect.rs").body);
        }
        _ => {}
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
    assert!(!steps.is_empty(), "{}: console block without a command", b.at());

    for (i, (cmd, code, frags)) in steps.iter().enumerate() {
        // the hand edits that happen after the previous command
        for (_, e) in edits.iter().filter(|(at, _)| *at == i) {
            apply_edit(e, &d, &b.at());
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
        assert_eq!(words.next(), Some("agentmark"), "{}: `{cmd}` must start with agentmark", b.at());
        let args: Vec<String> = words.map(|w| place(w, &d)).collect();
        let args: Vec<&str> = args.iter().map(|a| a.as_str()).collect();
        let mut c = agentmark(&args);
        // build caches of run/trace go under the block's own folder, removed with it
        c.env("TMPDIR", d.join("tmp"));
        for (k, v) in &env {
            c.env(k, v);
        }
        let o = run_command(c, matches!(args[0], "run" | "trace" | "build"));
        let got = scrub(&format!("{}{}", stdout(&o), stderr(&o)));
        assert_eq!(o.status.code(), Some(*code), "{}: `{cmd}` exited differently\n{}", b.at(), head(&got));
        for f in frags {
            assert!(got.contains(f.as_str()), "{}: `{cmd}` did not print:\n  {f}\nit printed:\n{}", b.at(), head(&got));
        }
    }
}

/// `# in FILE: replace "A" with "B"`.
fn apply_edit(directive: &str, dir: &Path, at: &str) {
    let rest = directive.strip_prefix("# in ").unwrap();
    let (file, what) = rest.split_once(": replace ").unwrap_or_else(|| panic!("{at}: bad directive {directive}"));
    let quoted: Vec<&str> = what.split('"').collect();
    assert!(quoted.len() == 5, "{at}: bad directive {directive}");
    let path = PathBuf::from(place(file, dir));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(quoted[1]), "{at}: {file} has no line {:?}", quoted[1]);
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

fn readme_block(info: &str) -> Block {
    let b = blocks().into_iter().find(|b| b.info == info);
    b.unwrap_or_else(|| panic!("README.md has no ```{info} block"))
}

/// A copy of example 01 in a scratch folder.
fn project_01(d: &Path) -> PathBuf {
    let project = d.join("chaser");
    copy_dir(Path::new(&format!("{ROOT}/examples/01-invoice-chaser")), &project);
    project
}

#[test]
fn http_backed_connect_rs_builds_with_rust_dependencies() {
    let d = temp("readme-http");
    let project = project_01(&d);
    write(&project, "rust/connect.rs", &readme_block("rust http").body);
    let toml = std::fs::read_to_string(project.join("agentmark.toml")).unwrap();
    write(&project, "agentmark.toml", &format!("{toml}\n{}", readme_block("toml agentmark.toml").body));
    let out = d.join("out");
    let o = agentmark(&["build", s(&project), "-o", s(&out)]).output_locked();
    assert!(o.status.success(), "README http connect.rs does not build:\n{}{}", stdout(&o), stderr(&o));
    let cargo = std::fs::read_to_string(out.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("reqwest = "), "[rust.dependencies] was not merged:\n{cargo}");
}

#[test]
fn the_trait_block_is_what_build_generates() {
    let d = temp("readme-trait");
    let project = project_01(&d);
    let out = d.join("out");
    let o = agentmark(&["build", s(&project), "-o", s(&out), "--no-check"]).output();
    assert!(o.map(|o| o.status.success()).unwrap(), "build failed");
    let tools = std::fs::read_to_string(out.join("src/tools.rs")).unwrap();
    let block = readme_block("rust");
    assert!(tools.contains(block.body.trim_end()), "the trait in {} is not in the generated tools.rs:\n{tools}", block.at());
}
