//! Deliberate mistakes in `tests/errors/*.agent.md`. Each file starts with `>` comment lines that
//! hold the exact message the compiler must print (SPEC 10 format); the test runs the file through
//! the front end and compares the rendered text exactly.
//!
//! Mistakes the parser can see (tabs, indentation, a missing `effect`, an unknown property) and
//! the semantic ones (unknown tool/verb/target, unlisted tool, effect violation, unit mismatch,
//! missing `maybe`, unknown field) go through the same front end: load the project, run
//! `sema::check`, render the errors. `semantic_cases_parse_cleanly` proves the parser accepts the
//! semantic cases, so their message really comes from the semantic pass (OPEN_QUESTIONS Q11).

use std::path::Path;

use agentmark::diag::{render_all, Severity};
use agentmark::project::Project;
use agentmark::sema;
use agentmark::syntax::parser::parse_file;

/// Every case: file stem, and whether it needs the semantic pass.
const CASES: &[(&str, bool)] = &[
    ("tabs", false),
    ("bad-indentation", false),
    ("indent-too-deep", false),
    ("function-missing-effect", false),
    ("unknown-property", false),
    ("unknown-tool", true),
    ("unknown-verb", true),
    ("unknown-target", true),
    ("unlisted-tool", true),
    ("effect-violation-in-rule", true),
    ("unit-mismatch", true),
    ("missing-maybe", true),
    ("unknown-field", true),
    ("unsupported-model-azure", true),
    ("unsupported-model-bedrock", true),
    ("unknown-model-provider", true),
];

fn path_of(stem: &str) -> String {
    format!("tests/errors/{stem}.agent.md")
}

/// The expected message: the leading `>` comment lines, with the `> ` marker removed.
fn expected(src: &str) -> String {
    src.lines()
        .take_while(|l| l.starts_with('>'))
        .map(|l| l.strip_prefix("> ").unwrap_or(l.trim_start_matches('>')))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Run the front end: load the file (parse + `tools.toml` next to it), then the semantic check.
/// Only errors are rendered; warnings (unused declarations in these tiny files) are not part of
/// the documented message.
fn front_end(path: &str, _src: &str) -> String {
    let project = Project::load(path);
    let diags = match sema::check(&project) {
        Ok(_) => return String::new(),
        Err(d) => d,
    };
    let errors: Vec<_> = diags
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    project.render(&errors)
}

fn check(stem: &str) {
    let path = path_of(stem);
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let want = expected(&src);
    assert!(
        !want.is_empty(),
        "{path} has no expected message in its header"
    );
    let got = front_end(&path, &src);
    assert_eq!(
        got, want,
        "\n--- {path}: rendered message differs from the header ---"
    );
}

macro_rules! case {
    ($name:ident, $stem:literal) => {
        #[test]
        fn $name() {
            check($stem);
        }
    };
    (semantic $name:ident, $stem:literal) => {
        #[test]
        fn $name() {
            check($stem);
        }
    };
}

case!(tabs, "tabs");
case!(bad_indentation, "bad-indentation");
case!(indent_too_deep, "indent-too-deep");
case!(function_missing_effect, "function-missing-effect");
case!(unknown_property, "unknown-property");
case!(semantic unknown_tool, "unknown-tool");
case!(semantic unknown_verb, "unknown-verb");
case!(semantic unknown_target, "unknown-target");
case!(semantic unlisted_tool, "unlisted-tool");
case!(semantic effect_violation_in_rule, "effect-violation-in-rule");
case!(semantic unit_mismatch, "unit-mismatch");
case!(semantic missing_maybe, "missing-maybe");
case!(semantic unknown_field, "unknown-field");
case!(semantic unsupported_model_azure, "unsupported-model-azure");
case!(semantic unsupported_model_bedrock, "unsupported-model-bedrock");
case!(semantic unknown_model_provider, "unknown-model-provider");

#[test]
fn every_error_file_is_registered_and_documented() {
    let mut on_disk: Vec<String> = std::fs::read_dir("tests/errors")
        .unwrap()
        .filter_map(|e| {
            let name = e.unwrap().file_name().to_string_lossy().to_string();
            name.strip_suffix(".agent.md").map(str::to_string)
        })
        .collect();
    on_disk.sort();
    let mut registered: Vec<String> = CASES.iter().map(|(n, _)| n.to_string()).collect();
    registered.sort();
    assert_eq!(on_disk, registered, "tests/errors/ and CASES disagree");
    assert!(CASES.len() >= 10);
    for (stem, _) in CASES {
        let src = std::fs::read_to_string(path_of(stem)).unwrap();
        let want = expected(&src);
        // file:line, source line, caret line, message
        assert!(want.lines().count() >= 4, "{stem}: header too short");
        assert!(
            want.starts_with(&path_of(stem)),
            "{stem}: header must start with the file path"
        );
    }
}

#[test]
fn semantic_cases_parse_cleanly() {
    for (stem, semantic) in CASES.iter().filter(|(_, s)| *s) {
        assert!(*semantic);
        let path = path_of(stem);
        let src = std::fs::read_to_string(&path).unwrap();
        if let Err(d) = parse_file(&path, &src) {
            panic!(
                "{path} should parse; the mistake in it is semantic:\n{}",
                render_all(&d, &src)
            );
        }
    }
}

#[test]
fn error_tests_have_a_tool_manifest() {
    assert!(Path::new("tests/errors/tools.toml").exists());
    let body = |p: &str| std::fs::read_to_string(p).unwrap().lines().skip(1).collect::<Vec<_>>().join("\n");
    assert_eq!(
        body("tests/errors/tools.toml"),
        body("examples/01-invoice-chaser/tools.toml"),
        "tests/errors/tools.toml must stay a copy of the example projects' tools.toml"
    );
}
