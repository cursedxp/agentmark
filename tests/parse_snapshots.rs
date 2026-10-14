//! Every `.agent.md` under `examples/` must parse, and its AST is snapshotted (YAML, spans
//! included) so any change to the parser's output is a visible diff.

use std::path::{Path, PathBuf};

use agentmark::diag::render_all;
use agentmark::syntax::parser::parse_file;

/// v0.3 examples (06 to 10) are authored ahead of the compiler (step 25); until their steps land they are
/// not parsed, formatted or run by the tests that pin 01 to 05.
fn v03_pending(p: &Path) -> bool {
    p.components().any(|c| {
        let c = c.as_os_str().to_string_lossy();
        ["06-", "07-", "08-", "09-", "10-"].iter().any(|n| c.starts_with(n))
    })
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if v03_pending(&p) {
            continue;
        }
        if p.is_dir() {
            collect(&p, out);
        } else if p.to_string_lossy().ends_with(".agent.md") {
            out.push(p);
        }
    }
}

fn example_files() -> Vec<PathBuf> {
    let mut v = Vec::new();
    collect(Path::new("examples"), &mut v);
    v
}

#[test]
fn every_example_is_a_project_folder_with_an_entry_point() {
    for n in ["01-invoice-chaser", "02-lead-qualifier", "03-support-triage", "04-daily-report", "05-invoice-service"] {
        for f in ["agentmark.toml", "tools.toml", "main.agent.md"] {
            assert!(Path::new("examples").join(n).join(f).is_file(), "examples/{n}/{f} is missing");
        }
    }
    // no single-file drafts remain directly under examples/ (step 9)
    assert!(std::fs::read_dir("examples").unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".agent.md")));
    assert_eq!(example_files().len(), 30);
}

#[test]
fn every_example_parses_and_matches_its_snapshot() {
    for path in example_files() {
        let rel = path.to_string_lossy().replace('\\', "/");
        let src = std::fs::read_to_string(&path).unwrap();
        let file = match parse_file(&rel, &src) {
            Ok(f) => f,
            Err(diags) => panic!("{rel} failed to parse:\n{}", render_all(&diags, &src)),
        };
        let name = rel
            .trim_start_matches("examples/")
            .trim_end_matches(".agent.md")
            .replace('/', "__");
        insta::with_settings!({ snapshot_path => "snapshots" }, {
            insta::assert_yaml_snapshot!(name, file);
        });
    }
}
