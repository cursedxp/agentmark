//! Every `.agent.md` under `examples/` must parse, and its AST is snapshotted (YAML, spans
//! included) so any change to the parser's output is a visible diff.

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
    // 01-05 are project folders (SPEC 18.1); the old single files 02-05 may still sit next to them
    // until the code generator's tests stop referencing them.
    for n in ["01-invoice-chaser", "02-lead-qualifier", "03-support-triage", "04-daily-report", "05-invoice-service"] {
        for f in ["agentmark.toml", "tools.toml", "main.agent.md"] {
            assert!(Path::new("examples").join(n).join(f).is_file(), "examples/{n}/{f} is missing");
        }
    }
    // 30 files in the five projects, plus the legacy single files directly under examples/
    // (02-05, removed once the code generator's tests stop using them).
    let legacy = std::fs::read_dir("examples")
        .unwrap()
        .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".agent.md"))
        .count();
    assert_eq!(example_files().len(), 30 + legacy);
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
