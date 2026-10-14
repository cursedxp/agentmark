//! Formatter tests (SPEC 18.6): idempotence, AST equivalence, comment preservation, a stored
//! snapshot of a deliberately messy file, and `fmt --check` semantics.

mod support;

use std::path::{Path, PathBuf};

use agentmark::diag::render_all;
use agentmark::fmt::{format_source, is_canonical};
use agentmark::syntax::lexer::{lex, LineKind};
use agentmark::syntax::parser::parse_file;

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
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

fn fmt_ok(src: &str) -> String {
    match format_source(src, "t.agent.md") {
        Ok(s) => s,
        Err(d) => panic!("did not parse:\n{}", render_all(&d, src)),
    }
}

/// The AST as JSON without anything that legitimately changes when a file is re-printed: spans,
/// the verbatim heading text of a block, and the source line of Rust code.
fn ast_json(src: &str) -> serde_json::Value {
    let file = parse_file("t.agent.md", src)
        .unwrap_or_else(|d| panic!("did not parse:\n{}", render_all(&d, src)));
    let mut v = serde_json::to_value(&file).unwrap();
    strip(&mut v);
    // Imports are sorted by fmt; the order in the source is not part of the meaning.
    if let Some(serde_json::Value::Array(a)) = v.get_mut("imports") {
        a.sort_by_key(|x| x.to_string());
    }
    v
}

/// Path of the first difference between two JSON values (for a readable failure).
fn first_diff(a: &serde_json::Value, b: &serde_json::Value, path: &str) -> Option<String> {
    use serde_json::Value::*;
    match (a, b) {
        (Object(x), Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => {
                        if let Some(d) = first_diff(p, q, &format!("{path}.{k}")) {
                            return Some(d);
                        }
                    }
                    (p, q) => return Some(format!("{path}.{k}: {p:?} vs {q:?}")),
                }
            }
            None
        }
        (Array(x), Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (p, q))| first_diff(p, q, &format!("{path}[{i}]"))),
        _ if a == b => None,
        _ => Some(format!("{path}: {a} vs {b}")),
    }
}

fn strip(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(m) => {
            m.remove("span");
            m.remove("label");
            if m.contains_key("code") {
                m.remove("line");
            }
            m.values_mut().for_each(strip);
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(strip),
        _ => {}
    }
}

fn comment_lines(src: &str) -> Vec<String> {
    let (lines, _) = lex("t.agent.md", src);
    let mut v: Vec<String> = lines
        .into_iter()
        .filter(|l| l.kind == LineKind::Comment)
        .map(|l| l.text)
        .collect();
    v.sort();
    v
}

/// The three properties every formatted file must have.
fn check_properties(src: &str) -> String {
    let once = fmt_ok(src);
    assert_eq!(fmt_ok(&once), once, "fmt is not idempotent");
    let (before, after) = (ast_json(src), ast_json(&once));
    if let Some(d) = first_diff(&before, &after, "") {
        panic!("fmt changed the AST at {d}\n--- output ---\n{once}");
    }
    assert_eq!(
        comment_lines(src),
        comment_lines(&once),
        "fmt lost or changed a comment"
    );
    assert!(once.ends_with('\n') && !once.ends_with("\n\n"));
    assert!(once.lines().all(|l| l == l.trim_end()), "trailing whitespace");
    once
}

#[test]
fn every_example_formats_idempotently_and_keeps_its_ast() {
    for path in example_files() {
        let src = std::fs::read_to_string(&path).unwrap();
        check_properties(&src);
    }
}

/// The examples are what people copy: every file outside `bindings/` (generated, Q111) is already
/// in the canonical style, which is what `agentmark fmt --check` asks.
#[test]
fn every_example_file_outside_bindings_is_canonical() {
    for path in example_files() {
        if path.components().any(|c| c.as_os_str() == "bindings") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        assert!(is_canonical(&src, "t.agent.md").unwrap(), "{} is not canonical: run `agentmark fmt`", path.display());
    }
}

#[test]
fn messy_file_matches_the_stored_snapshot() {
    let src = std::fs::read_to_string("tests/fmt/messy.agent.md").unwrap();
    let once = check_properties(&src);
    insta::with_settings!({ snapshot_path => "snapshots" }, {
        insta::assert_snapshot!("messy", once);
    });
}

#[test]
fn check_mode_is_true_only_for_canonical_input() {
    let messy = std::fs::read_to_string("tests/fmt/messy.agent.md").unwrap();
    assert!(!is_canonical(&messy, "m.agent.md").unwrap());
    let canon = fmt_ok(&messy);
    assert!(is_canonical(&canon, "m.agent.md").unwrap());
}

#[test]
fn invalid_source_is_reported_not_formatted() {
    let err = format_source("# Record: A\n\t- x: text\n", "t.agent.md").unwrap_err();
    assert_eq!(err[0].code, "E001");
}

#[test]
fn imports_sort_local_files_then_folders_then_crates() {
    let out = fmt_ok(
        "# Import: crate zed\n# Import: ./b as b\n# Import: ./z.agent.md\n# Import: ./a.agent.md\n# Import: crate alpha\n\n# Record: A\n- x: text\n",
    );
    assert!(out.starts_with(
        "# Import: ./a.agent.md\n# Import: ./z.agent.md\n# Import: ./b as b\n# Import: crate alpha\n# Import: crate zed\n\n# Record: A\n"
    ));
}

#[test]
fn money_and_durations_have_one_spelling() {
    let out = fmt_ok(
        "# Constant: a\n- value: 12.5 EUR\n\n# Constant: b\n- value: 1200 JPY\n\n# Constant: c\n- value: 14 EUR\n\n# Constant: d\n- value: 1 days\n",
    );
    assert!(out.contains("- value: 12.50 EUR\n"));
    assert!(out.contains("- value: 1200 JPY\n"));
    assert!(out.contains("- value: 14 EUR\n"));
    assert!(out.contains("- value: 1 day\n"));
}

#[test]
fn expressions_keep_exactly_the_parentheses_precedence_needs() {
    let cases = [
        ("(a + b) * c", "(a + b) * c"),
        ("(a * b) + c", "a * b + c"),
        ("a - (b - c)", "a - (b - c)"),
        ("(a - b) - c", "a - b - c"),
        ("not (a and b)", "not (a and b)"),
        ("(not a) and b", "not a and b"),
        ("a and (b or c)", "a and (b or c)"),
        ("(a and b) or c", "a and b or c"),
        ("a and (b and c)", "a and (b and c)"),
        ("(a = b) = c", "(a = b) = c"),
        ("(a + b) length > 3", "(a + b) length > 3"),
        ("(first x in xs where x.id = 1) = y", "(first x in xs where x.id = 1) = y"),
        ("first x in xs where x.id = 1 and x.ok", "first x in xs where x.id = 1 and x.ok"),
        ("f(a,b)+g()", "f(a, b) + g()"),
        ("{id:a.id,n:1}", "{ id: a.id, n: 1 }"),
        ("\"a \\{b\\} {c} \\\"q\\\"\"", "\"a \\{b} {c} \\\"q\\\"\""),
    ];
    for (input, want) in cases {
        let src = format!("# Function: f\n- effect: pure\n  - set v = {input}\n");
        let out = fmt_ok(&src);
        let want_line = format!("  - set v = {want}\n");
        assert!(out.contains(&want_line), "{input:?} printed as\n{out}");
        check_properties(&src);
    }
}

#[test]
fn comments_the_parser_drops_are_written_back_where_they_were() {
    let src = "\
# Agent: a
- model: claude
> after the last property

## Do x
- if a:
  - log: \"1\"
> above else
- else:
  - log: \"2\"
> end of block

# Run
- a
> between run items
- b
";
    let out = check_properties(src);
    let expected = "\
# Agent: a
- model: claude
> after the last property

## Do x
- if a:
  - log: \"1\"
> above else
- else:
  - log: \"2\"
> end of block

# Run
- a
> between run items
- b
";
    assert_eq!(out, expected);
}

#[test]
fn a_comment_before_the_first_heading_stays_apart_from_the_file_doc() {
    let src = "> file doc\n\n> loose note\n\n# Record: A\n- x: text\n";
    let out = check_properties(src);
    assert_eq!(out, src);
}

#[test]
fn a_file_with_only_a_doc_or_nothing_formats() {
    assert_eq!(fmt_ok(""), "");
    assert_eq!(fmt_ok("\n\n"), "");
    assert_eq!(fmt_ok("> just a note\n\n\n"), "> just a note\n");
}

#[test]
fn fenced_rust_is_verbatim() {
    let src = "\
# Function: f
- effect: pure

## Rust: g -> text
- effect: pure
  ```rust
  fn x() {
      let   y = 1;   // spaces   kept

  }
  ```
";
    let out = check_properties(src);
    assert!(out.contains("  fn x() {\n      let   y = 1;   // spaces   kept\n\n  }\n"));
}

#[test]
fn blank_lines_inside_fenced_rust_are_not_collapsed() {
    let src = "\
# Function: f
- effect: pure

## Rust: g -> text
- effect: pure
  ```rust
  let a = 1;


  let s = \"line one


  line four\";
  format!(\"{a}{s}\")
  ```
";
    let out = check_properties(src);
    assert_eq!(out, src);
}

#[test]
fn a_comment_between_not_bound_and_its_first_item_stays_below_the_heading() {
    let src = "\
# Binding: pdf
- crate: printpdf
- effect: unverified

## Not bound
> these need a Rust block
- save_to_stream: needs a lifetime
";
    let out = check_properties(src);
    assert_eq!(out, src);
}

#[test]
fn a_comment_above_otherwise_or_else_lines_up_with_it() {
    let src = "\
> Doc.
# Agent: a
- model: claude

## Do go
- try http get \"https://x.example\" -> r
  > what happens when it fails
  otherwise:
    - log: \"no\"
- if a:
  - log: \"1\"
  > trailing in if
> above else
- else:
  - log: \"2\"
";
    let out = check_properties(src);
    assert_eq!(out, src);
}
