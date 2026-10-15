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
            // `- start: name` is printed as the link `[name](#command-name)`: the same command.
            if let Some(serde_json::Value::String(t)) = m.get_mut("target") {
                if let Some(rest) = t.strip_prefix("#command-") {
                    *t = rest.to_string();
                }
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
        "# Import: crate zed\n# Uses\n- [z](./z.agent.md)\n- [b](./b/) as b\n- [a](./a.agent.md)\n\n# Import: crate alpha\n\n# Record: A\n- x: text\n",
    );
    assert!(out.starts_with(
        "# Uses\n- [a](./a.agent.md)\n- [z](./z.agent.md)\n- [b](./b/) as b\n\n# Import: crate alpha\n# Import: crate zed\n\n# Record: A\n"
    ), "{out}");
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
        let src = format!("# Function: f\n- touches: nothing\n- steps:\n  1. return {input}\n");
        let out = fmt_ok(&src);
        let want_line = format!("  1. return {want}\n");
        assert!(out.contains(&want_line), "{input:?} printed as\n{out}");
        check_properties(&src);
    }
}

#[test]
fn comments_the_parser_drops_are_written_back_where_they_were() {
    let src = "# Program: a\n\n# Agent: a\n- model: claude\n> after the last property\n> after the last property\n\n## Do x\n1. if a:\n   1. log: \"1\"\n   > above else\n   else:\n   1. log: \"2\"\n> end of block\n";
    let out = check_properties(src);
    let expected = "# Program: a\n\n# Agent: a\n- model: claude\n> after the last property\n> after the last property\n\n## Do x\n1. if a:\n   1. log: \"1\"\n   > above else\n   else:\n   1. log: \"2\"\n> end of block\n";
    assert_eq!(out, expected);
}

#[test]
fn a_comment_before_the_first_heading_stays_apart_from_the_file_doc() {
    let src = "> file doc\n> loose note\n\n# Record: A\n- x: text\n";
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
    let src = "# Function: f\n- touches: nothing\n- steps:\n  1. return g()\n\n## Rust: g -> text\n- touches: nothing\n\n```rust\nfn x() {\n    let   y = 1;   // spaces   kept\n\n}\n```\n";
    let out = check_properties(src);
    assert!(out.contains("```rust\nfn x() {\n    let   y = 1;   // spaces   kept\n\n}\n```\n"));
}

#[test]
fn blank_lines_inside_fenced_rust_are_not_collapsed() {
    let src = "# Function: f\n- touches: nothing\n- steps:\n  1. return g()\n\n## Rust: g -> text\n- touches: nothing\n\n```rust\nlet a = 1;\n\nlet s = \"line one\n\nline four\";\nformat!(\"{a}{s}\")\n```\n";
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
    let src = "# Agent: a\nDoc.\n\n- model: claude\n\n## Do go\n1. try http get \"https://x.example\" into r\n   otherwise:\n   1. log: \"no\"\n2. if a:\n   1. log: \"1\"\n   > trailing in if\n   > above else\n   else:\n   1. log: \"2\"\n";
    let out = check_properties(src);
    assert_eq!(out, src);
}

// ---------------------------------------------------------------------------------------------
// v0.4 (SPEC 23): numbered steps, descriptions, `# Program` / `# Uses`, `add`.
// ---------------------------------------------------------------------------------------------

#[test]
fn steps_are_renumbered_at_every_level() {
    let src = "# Command: c\nDoes things.\n\n- steps:\n  1. print: \"a\"\n  1. for each x in xs:\n     1. print: \"b\"\n     1. for each y in ys:\n        1. print: \"c\"\n        1. print: \"d\"\n  1. print: \"e\"\n";
    let out = check_properties(src);
    assert_eq!(
        out,
        "# Command: c\nDoes things.\n\n- steps:\n  1. print: \"a\"\n  2. for each x in xs:\n     1. print: \"b\"\n     2. for each y in ys:\n        1. print: \"c\"\n        2. print: \"d\"\n  3. print: \"e\"\n"
    );
}

#[test]
fn children_are_reindented_to_the_text_of_their_parent() {
    // Lenient input: any deeper indentation means "under it"; under `10.` the text starts at 4.
    let mut src = String::from("# Command: c\nDoes things.\n\n- steps:\n");
    for i in 0..9 {
        src.push_str(&format!("  1. print: \"{i}\"\n"));
    }
    src.push_str("  1. for each x in xs:\n         1. print: \"a\"\n         1. print: \"b\"\n  1. print: \"z\"\n");
    let out = check_properties(&src);
    assert!(out.contains("  9. print: \"8\"\n  10. for each x in xs:\n      1. print: \"a\"\n      2. print: \"b\"\n  11. print: \"z\"\n"), "{out}");
    // Under `1.` it is 3 spaces, however deep the input was.
    let out = fmt_ok("# Command: c\nDoes things.\n\n- steps:\n  1. for each x in xs:\n          1. print: \"a\"\n");
    assert!(out.contains("  1. for each x in xs:\n     1. print: \"a\"\n"), "{out}");
    // A top-level list (agent block) has no bullet before it.
    let out = fmt_ok("# Agent: a\nDoc.\n\n- model: claude\n\n## Do x\n1. for each x in xs:\n      1. log: \"a\"\n");
    assert!(out.contains("1. for each x in xs:\n   1. log: \"a\"\n"), "{out}");
}

#[test]
fn a_sibling_else_moves_inside_its_if() {
    let src = "# Command: c\nDoes things.\n\n- steps:\n  1. if a:\n     1. print: \"yes\"\n  2. else:\n     1. print: \"no\"\n  3. print: \"after\"\n";
    let out = fmt_ok(src);
    assert_eq!(
        out,
        "# Command: c\nDoes things.\n\n- steps:\n  1. if a:\n     1. print: \"yes\"\n     else:\n     1. print: \"no\"\n  2. print: \"after\"\n"
    );
    assert_eq!(fmt_ok(&out), out);
}

#[test]
fn an_inline_record_after_add_becomes_field_bullets() {
    let src = "# Function: f\nDoc.\n\n- touches: nothing\n- steps:\n  1. add RegionTotal { region: region, orders: count of rs, revenue: 1 } to totals\n  2. add RegionTotal { region: other } to totals\n";
    let out = check_properties(src);
    assert_eq!(
        out,
        "# Function: f\nDoc.\n\n- touches: nothing\n- steps:\n  1. add RegionTotal to totals:\n     - region\n     - orders: count of rs\n     - revenue: 1\n  2. add RegionTotal to totals:\n     - region: other\n"
    );
    // Written the long way, `region: region` is written as the bare name.
    let long = fmt_ok("# Function: f\nDoc.\n\n- touches: nothing\n- steps:\n  1. add RegionTotal to totals:\n     - region: region\n");
    assert!(long.contains("     - region\n"), "{long}");
}

#[test]
fn start_is_written_as_a_link_to_a_command_of_the_same_file() {
    let src = "# Program: p\nDoc.\n\n- start:   sales-report\n\n# Command: sales-report\nDoc.\n\n- steps:\n  1. print: \"x\"\n";
    let out = check_properties(src);
    assert!(out.contains("- start: [sales-report](#command-sales-report)\n"), "{out}");
    // Already a link: unchanged.
    assert_eq!(fmt_ok(&out), out);
    // A link to a file stays a link to that file.
    let file = "# Program: p\nDoc.\n\n- start: [it](./commands/it.agent.md)\n\n# Uses\n- [it](./commands/it.agent.md)\n";
    assert_eq!(fmt_ok(file), file);
}

#[test]
fn parts_come_in_the_order_program_uses_crates_rest() {
    let src = "# Import: crate zed\n\n# Uses\n- [z](./z.agent.md)\n- [a](./a.agent.md)\n\n# Import: crate alpha\n\n# Record: A\n- x: text\n";
    let out = fmt_ok(src);
    assert_eq!(
        out,
        "# Uses\n- [a](./a.agent.md)\n- [z](./z.agent.md)\n\n# Import: crate alpha\n# Import: crate zed\n\n# Record: A\n- x: text\n"
    );
    let with_program = "# Program: p\nDoc.\n\n# Uses\n- [z](./z.agent.md)\n- [a](./a.agent.md)\n\n# Import: crate zed\n\n# Record: A\n- x: text\n";
    let out = fmt_ok(with_program);
    assert!(out.starts_with("# Program: p\nDoc.\n\n# Uses\n- [a](./a.agent.md)\n- [z](./z.agent.md)\n\n# Import: crate zed\n\n# Record: A\n"), "{out}");
}

#[test]
fn a_uses_entry_keeps_its_label_and_as_name() {
    let src = "# Uses\n-   [every rule](./rules/*.agent.md)\n- [billing rules](./billing/)   as   billing\n";
    let out = fmt_ok(src);
    assert_eq!(out, "# Uses\n- [every rule](./rules/*.agent.md)\n- [billing rules](./billing/) as billing\n");
}

#[test]
fn a_description_sits_under_its_heading_followed_by_one_blank_line() {
    let src = "# Record: Sale\nA sale.\n\n\nIt has two lines of description.\n- region: text\n\n\n- amount: money\n";
    let out = check_properties(src);
    assert_eq!(
        out,
        "# Record: Sale\nA sale.\n\nIt has two lines of description.\n\n- region: text\n- amount: money\n"
    );
    // Under a `##` heading too.
    let src = "# Agent: a\nDoc.\n- model: claude\n\n## Do x\nWhat x does.\n1. log: \"1\"\n";
    let out = fmt_ok(src);
    assert!(out.contains("## Do x\nWhat x does.\n\n1. log: \"1\"\n"), "{out}");
}

#[test]
fn star_and_plus_bullets_are_written_as_dashes() {
    let out = fmt_ok("# Record: A\nDoc.\n\n* x: text\n+ y: int\n");
    assert_eq!(out, "# Record: A\nDoc.\n\n- x: text\n- y: int\n");
}

#[test]
fn match_branches_stay_bullets_with_numbered_steps() {
    let src = "# Command: c\nDoc.\n\n- steps:\n  1. match x:\n   - ok v:\n         1. print: \"{v}\"\n   - error:\n       1. stop\n";
    let out = check_properties(src);
    assert_eq!(
        out,
        "# Command: c\nDoc.\n\n- steps:\n  1. match x:\n     - ok v:\n       1. print: \"{v}\"\n     - error:\n       1. stop\n"
    );
}

#[test]
fn every_example_formats_to_itself_even_the_generated_bindings_once() {
    // Idempotent on the canonical form of every example file (bindings included).
    for path in example_files() {
        let src = std::fs::read_to_string(&path).unwrap();
        let once = fmt_ok(&src);
        assert_eq!(fmt_ok(&once), once, "{}", path.display());
    }
}

#[test]
fn the_messy_file_is_idempotent() {
    let src = std::fs::read_to_string("tests/fmt/messy.agent.md").unwrap();
    let once = fmt_ok(&src);
    assert_eq!(fmt_ok(&once), once);
}
