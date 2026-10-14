//! Every `.agent.md` under `examples/` must parse, and its AST is snapshotted (YAML, spans
//! included) so any change to the parser's output is a visible diff.

mod support;

use std::path::{Path, PathBuf};

use agentmark::diag::render_all;
use agentmark::syntax::parser::parse_file;

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
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
    for n in ["06-sales-report", "07-file-renamer", "08-notes-api", "09-markdown-to-html", "10-log-analyzer"] {
        for f in ["agentmark.toml", "main.agent.md"] {
            assert!(Path::new("examples").join(n).join(f).is_file(), "examples/{n}/{f} is missing");
        }
    }
    assert_eq!(example_files().len(), 61);
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
