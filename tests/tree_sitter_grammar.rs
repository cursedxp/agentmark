//! The tree-sitter grammar in `editors/tree-sitter-agentmark/` must parse every
//! example and every `agentmark` block in the docs without ERROR nodes, and its
//! own corpus must pass (SPEC 20.5). The work is done by
//! `scripts/check-examples.sh`, which needs `npx` (it fetches tree-sitter-cli
//! from npm). Without `npx`, or without network access to fetch the CLI, the
//! test says so and passes: the grammar is an editor add-on, not part of the
//! compiler.

use std::path::PathBuf;
use std::process::Command;

fn grammar_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("editors/tree-sitter-agentmark")
}

fn have(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn tree_sitter_grammar_parses_examples_and_docs() {
    if !have("npx", &["--version"]) || !have("node", &["--version"]) || !have("bash", &["-c", "true"]) {
        eprintln!("skipped: npx/node/bash not found, so the tree-sitter grammar was not checked");
        return;
    }
    if !have("npx", &["--yes", "tree-sitter-cli@0.25", "--version"]) {
        eprintln!("skipped: tree-sitter-cli@0.25 is not available (offline?), so the grammar was not checked");
        return;
    }
    let out = Command::new("bash")
        .arg(grammar_dir().join("scripts/check-examples.sh"))
        .output()
        .expect("run check-examples.sh");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "tree-sitter grammar check failed:\n{text}");
    assert!(
        text.contains("parse without ERROR or MISSING nodes"),
        "unexpected output:\n{text}"
    );
}

#[test]
fn generated_parser_is_committed() {
    // Consumers (Zed, Neovim, Helix) build src/parser.c directly; they must not need the CLI.
    for f in ["src/parser.c", "src/grammar.json", "src/node-types.json"] {
        assert!(grammar_dir().join(f).is_file(), "{f} is missing; run `tree-sitter generate`");
    }
    for f in ["grammar.js", "queries/highlights.scm", "queries/injections.scm", "tree-sitter.json", "package.json"] {
        assert!(grammar_dir().join(f).is_file(), "{f} is missing");
    }
}
