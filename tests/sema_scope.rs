//! Scope and project-level rules (SPEC 13.2, 18.2): a file sees only what it imports, aliases,
//! unused imports and declarations, crate imports, `# Program` and `- start:`, duplicate names,
//! import cycles, manifest files.

mod common;
use common::TOOLS;

use agentmark::diag::Severity;
use agentmark::project::Project;
use agentmark::sema::{self, Checked};

fn mem(root: &str, files: Vec<(&str, &str)>) -> Project {
    let mut all = files;
    all.push(("app/tools.toml", TOOLS));
    Project::from_memory(root, all)
}

fn run(p: &Project) -> Result<Checked, String> {
    match sema::check(p) {
        Ok(c) => Ok(c),
        Err(d) => Err(p.render(
            &d.into_iter()
                .filter(|x| x.severity == Severity::Error)
                .collect::<Vec<_>>(),
        )),
    }
}

fn warn(p: &Project) -> String {
    let c = run(p).unwrap_or_else(|e| panic!("{e}"));
    p.render(&c.warnings)
}

const INVOICE: &str = "# Record: Invoice\n- id: text\n- amount: money\n";

// ------------------------------------------------------------------ imports

#[test]
fn a_file_sees_only_what_it_imports() {
    let p = mem(
        "app",
        vec![
            (
                "app/main.agent.md",
                "# Function: f\n- in: i: Invoice\n- out: text\n- touches: nothing\n- steps:\n  1. return i.id\n",
            ),
            ("app/invoice.agent.md", INVOICE),
        ],
    );
    let e = run(&p).unwrap_err();
    assert!(e.contains("Unknown type \"Invoice\"."), "{e}");
}

#[test]
fn imports_are_not_transitive() {
    let p = mem(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", "# Uses\n- [mid](./mid.agent.md)\n\n# Function: f\n- in: i: Invoice\n- out: text\n- touches: nothing\n- steps:\n  1. return i.id\n"),
            ("app/mid.agent.md", "# Uses\n- [invoice](./invoice.agent.md)\n\n# Function: g\n- in: i: Invoice\n- out: text\n- touches: nothing\n- steps:\n  1. return i.id\n"),
            ("app/invoice.agent.md", INVOICE),
        ],
    );
    let e = run(&p).unwrap_err();
    assert!(e.contains("app/main.agent.md:"), "{e}");
    assert!(e.contains("Unknown type \"Invoice\"."), "{e}");
}

#[test]
fn an_import_makes_names_visible() {
    let p = mem(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", "# Uses\n- [invoice](./invoice.agent.md)\n\n# Function: f\n- in: i: Invoice\n- out: text\n- touches: nothing\n- steps:\n  1. return i.id\n\n# Function: g\n- in: i: Invoice\n- out: text\n- touches: nothing\n- steps:\n  1. return f(i)\n"),
            ("app/invoice.agent.md", INVOICE),
        ],
    );
    let c = run(&p).unwrap_or_else(|e| panic!("{e}"));
    assert!(c.records.contains_key("Invoice"));
}

#[test]
fn an_alias_prefixes_every_name_of_the_module() {
    // (names in type positions cannot be written with a prefix, OPEN_QUESTIONS Q37, so the record
    // comes in through a plain import)
    let main = "# Uses\n- [billing](./billing/) as billing\n- [invoice](./invoice.agent.md)\n\n# Function: f\n- in: i: Invoice\n- out: bool\n- touches: nothing\n- steps:\n  1. return billing.overdue(i)\n";
    let fns = "# Uses\n- [invoice](../invoice.agent.md)\n\n# Function: overdue\n- in: i: Invoice\n- out: bool\n- touches: nothing\n- steps:\n  1. return i.amount > 5 EUR\n";
    let p = mem(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", main),
            ("app/invoice.agent.md", INVOICE),
            ("app/billing/fns.agent.md", fns),
        ],
    );
    let c = run(&p).unwrap_or_else(|e| panic!("{e}"));
    assert!(c.functions.contains_key("overdue"));
    // without the prefix the name is unknown, and the error points at the right one
    let bad = main.replace("billing.overdue(i)", "overdue(i)");
    let p = mem(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", &bad),
            ("app/invoice.agent.md", INVOICE),
            ("app/billing/fns.agent.md", fns),
        ],
    );
    let e = run(&p).unwrap_err();
    assert!(e.contains("Unknown function \"overdue\"."), "{e}");
    assert!(e.contains("Did you mean \"billing.overdue\"?"), "{e}");
}

#[test]
fn a_glob_import_brings_every_matching_file() {
    let p = mem(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", "# Uses\n- [*](./rules/*.agent.md)\n- [invoice](./invoice.agent.md)\n\n# Function: f\n- in: i: Invoice\n- out: bool\n- touches: nothing\n- steps:\n  1. return a(i) and b(i)\n"),
            ("app/invoice.agent.md", INVOICE),
            ("app/rules/a.agent.md", "# Uses\n- [invoice](../invoice.agent.md)\n\n# Function: a\n- in: i: Invoice\n- out: bool\n- touches: nothing\n- steps:\n  1. return true\n"),
            ("app/rules/b.agent.md", "# Uses\n- [invoice](../invoice.agent.md)\n\n# Function: b\n- in: i: Invoice\n- out: bool\n- touches: nothing\n- steps:\n  1. return true\n"),
        ],
    );
    run(&p).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn an_import_cycle_is_an_error_that_prints_the_cycle() {
    let p = mem(
        "app/a.agent.md",
        vec![
            (
                "app/a.agent.md",
                "# Uses\n- [b](./b.agent.md)\n\n# Record: A\n- id: text\n",
            ),
            (
                "app/b.agent.md",
                "# Uses\n- [a](./a.agent.md)\n\n# Record: B\n- id: text\n",
            ),
        ],
    );
    let e = run(&p).unwrap_err();
    assert!(
        e.contains("Import cycle: app/a.agent.md -> app/b.agent.md -> app/a.agent.md."),
        "{e}"
    );
}

#[test]
fn duplicate_declarations_across_files() {
    let p = mem(
        "app/main.agent.md",
        vec![
            (
                "app/main.agent.md",
                "# Uses\n- [invoice](./invoice.agent.md)\n\n# Record: Invoice\n- id: text\n",
            ),
            ("app/invoice.agent.md", INVOICE),
        ],
    );
    let e = run(&p).unwrap_err();
    assert!(
        e.contains("The record \"Invoice\" is already declared in app/main.agent.md:4."),
        "{e}"
    );
}

// ------------------------------------------------------------------ unused

#[test]
fn unused_imports_are_warnings() {
    let p = mem(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", "# Uses\n- [invoice](./invoice.agent.md)\n- [other](./other.agent.md)\n\n# Function: f\n- in: i: Invoice\n- out: text\n- touches: nothing\n- steps:\n  1. return i.id\n\n# Function: g\n- out: text\n- touches: nothing\n- steps:\n  1. return f(i)\n"),
            ("app/invoice.agent.md", INVOICE),
            ("app/other.agent.md", "# Record: Other\n- id: text\n"),
        ],
    );
    // `g` uses an unknown `i`: an error, so check the warnings through a variant without it
    let _ = p;
    let p = mem(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", "# Uses\n- [invoice](./invoice.agent.md)\n- [other](./other.agent.md)\n\n# Function: f\n- in: i: Invoice\n- out: text\n- touches: nothing\n- steps:\n  1. return i.id\n"),
            ("app/invoice.agent.md", INVOICE),
            ("app/other.agent.md", "# Record: Other\n- id: text\n"),
        ],
    );
    let w = warn(&p);
    assert!(
        w.contains("app/main.agent.md:3\n  - [other](./other.agent.md)"),
        "{w}"
    );
    assert!(
        w.contains("Unused import: nothing from \"./other.agent.md\" is used in this file."),
        "{w}"
    );
    assert!(!w.contains("nothing from \"./invoice.agent.md\""), "{w}");
}

#[test]
fn unused_declarations_are_warnings() {
    let p = Project::from_str(
        "t.agent.md",
        "# Record: Lonely\n- id: text\n\n# Constant: unused_limit\n- value: 5\n\n# Function: idle\n- out: int\n- touches: nothing\n- steps:\n  1. return 1\n\n# Rule: r Lonely\n- when: id = \"x\"\n",
        TOOLS,
    );
    let w = warn(&p);
    assert!(
        w.contains("Unused constant: nothing refers to \"unused_limit\"."),
        "{w}"
    );
    assert!(
        w.contains("Unused function: nothing refers to \"idle\"."),
        "{w}"
    );
    assert!(w.contains("Unused rule: nothing refers to \"r\"."), "{w}");
    // the record is used by the rule, so it is not reported
    assert!(!w.contains("Unused record"), "{w}");
}

#[test]
fn agents_services_and_commands_are_never_unused() {
    let p = Project::from_str(
        "t.agent.md",
        "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n1. log: \"x\"\n",
        TOOLS,
    );
    assert_eq!(warn(&p), "");
}

// ------------------------------------------------------------------ crate imports

#[test]
fn a_crate_import_without_a_binding_file_is_known_but_unbound() {
    let p = Project::from_str(
        "t.agent.md",
        "# Import: crate sha2\n\n# Function: host\n- in: s: text\n- out: text\n- touches: nothing\n- steps:\n  1. return f(s)\n\n## Rust: f -> text\n- in: s: text\n- touches: nothing\n\n```rust\nuse sha2::{Digest, Sha256};\nformat!(\"{:x}\", Sha256::digest(s.as_bytes()))\n```\n",
        TOOLS,
    );
    let w = warn(&p);
    assert!(w.contains("The crate \"sha2\" has no binding yet."), "{w}");
    assert!(w.contains("agentmark bind sha2"), "{w}");
    assert!(!w.contains("Unused import"), "{w}");
}

#[test]
fn a_crate_that_no_rust_block_mentions_is_unused() {
    let p = Project::from_str("t.agent.md", "# Import: crate sha2\n\n# Record: A\n- id: text\n\n# Function: f\n- in: a: A\n- out: text\n- touches: nothing\n- steps:\n  1. return a.id\n", TOOLS);
    let w = warn(&p);
    assert!(
        w.contains("Unused import: the crate \"sha2\" is not used in this file."),
        "{w}"
    );
}

// ------------------------------------------------------------------ # Program (SPEC 23.4)

const GREET: &str = "# Command: greet\nSays hello.\n\n- steps:\n  1. print: \"hello\"\n";
const BYE: &str = "# Command: bye\nSays goodbye.\n\n- steps:\n  1. print: \"bye\"\n";

fn units(c: &Checked) -> Vec<&str> {
    c.program.as_ref().unwrap().units.iter().map(|s| s.as_str()).collect()
}

#[test]
fn only_the_entry_file_has_a_program_heading() {
    let p = mem(
        "app",
        vec![
            ("app/main.agent.md", "# Program: tool\nDoes things.\n\n# Uses\n- [other](./other.agent.md)\n"),
            ("app/other.agent.md", "# Program: other\nNot the entry.\n\n# Command: greet\nSays hello.\n\n- steps:\n  1. print: \"hello\"\n"),
        ],
    );
    let e = run(&p).unwrap_err();
    assert!(e.starts_with("app/other.agent.md:1\n  # Program: other"), "{e}");
    assert!(
        e.contains("Only the entry file has a \"# Program\" heading, and this is not it: the entry file is main.agent.md."),
        "{e}"
    );
    assert!(e.contains("Move the heading, its description and \"- start:\" to the top of the entry file."), "{e}");
}

#[test]
fn start_can_be_a_link_to_the_command_heading() {
    let p = mem(
        "app",
        vec![
            ("app/main.agent.md", "# Program: tool\nDoes things.\n\n- start: [bye](#command-bye)\n\n# Uses\n- [greet](./greet.agent.md)\n- [bye](./bye.agent.md)\n"),
            ("app/greet.agent.md", GREET),
            ("app/bye.agent.md", BYE),
        ],
    );
    let c = run(&p).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(c.program.as_ref().unwrap().start.as_deref(), Some("bye"));
}

#[test]
fn start_can_be_a_link_to_a_file_that_declares_one_command() {
    let p = mem(
        "app",
        vec![
            ("app/main.agent.md", "# Program: tool\nDoes things.\n\n- start: [bye](./bye.agent.md)\n\n# Uses\n- [greet](./greet.agent.md)\n- [bye](./bye.agent.md)\n"),
            ("app/greet.agent.md", GREET),
            ("app/bye.agent.md", BYE),
        ],
    );
    let c = run(&p).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(c.program.as_ref().unwrap().start.as_deref(), Some("bye"));
}

#[test]
fn start_can_be_a_file_link_with_an_anchor() {
    let p = mem(
        "app",
        vec![
            ("app/main.agent.md", "# Program: tool\nDoes things.\n\n- start: [bye](./bye.agent.md#command-bye)\n\n# Uses\n- [greet](./greet.agent.md)\n- [bye](./bye.agent.md)\n"),
            ("app/greet.agent.md", GREET),
            ("app/bye.agent.md", BYE),
        ],
    );
    let c = run(&p).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(c.program.as_ref().unwrap().start.as_deref(), Some("bye"));
    // an anchor that names a command of another file does not match
    let main = "# Program: tool\nDoes things.\n\n- start: [bye](./greet.agent.md#command-bye)\n\n# Uses\n- [greet](./greet.agent.md)\n- [bye](./bye.agent.md)\n";
    let p = mem("app", vec![("app/main.agent.md", main), ("app/greet.agent.md", GREET), ("app/bye.agent.md", BYE)]);
    let e = run(&p).unwrap_err();
    assert!(e.contains("which is not a command of this program."), "{e}");
}

#[test]
fn start_can_be_the_plain_name() {
    let p = mem(
        "app",
        vec![
            ("app/main.agent.md", "# Program: tool\nDoes things.\n\n- start: greet\n\n# Uses\n- [greet](./greet.agent.md)\n- [bye](./bye.agent.md)\n"),
            ("app/greet.agent.md", GREET),
            ("app/bye.agent.md", BYE),
        ],
    );
    let c = run(&p).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(c.program.as_ref().unwrap().start.as_deref(), Some("greet"));
}

#[test]
fn a_start_that_is_not_a_program_command_suggests_and_lists_the_commands() {
    let p = mem(
        "app",
        vec![
            ("app/main.agent.md", "# Program: tool\nDoes things.\n\n- start: [gret](#command-gret)\n\n# Uses\n- [greet](./greet.agent.md)\n- [bye](./bye.agent.md)\n"),
            ("app/greet.agent.md", GREET),
            ("app/bye.agent.md", BYE),
        ],
    );
    let e = run(&p).unwrap_err();
    assert!(e.starts_with("app/main.agent.md:4\n"), "{e}");
    assert!(e.contains("\"- start:\" names \"gret\", which is not a command of this program."), "{e}");
    assert!(e.contains("Did you mean \"greet\"?"), "{e}");
    assert!(e.contains("Its commands are: greet, bye. A command of another file must be listed under \"# Uses\"."), "{e}");
}

#[test]
fn a_command_of_a_file_that_is_not_listed_under_uses_is_not_a_program_command() {
    let p = mem(
        "app",
        vec![
            ("app/main.agent.md", "# Program: tool\nDoes things.\n\n- start: greet\n"),
            ("app/cmds.agent.md", GREET),
        ],
    );
    let e = run(&p).unwrap_err();
    assert!(e.contains("\"- start:\" names \"greet\", which is not a command of this program."), "{e}");
    assert!(e.contains("The program has no command: leave \"- start:\" out (its agents and services all run)."), "{e}");
}

#[test]
fn the_units_of_a_program_are_the_entry_file_and_the_files_it_uses() {
    let p = mem(
        "app",
        vec![
            (
                "app/main.agent.md",
                "# Program: tool\nDoes things.\n\n- start: greet\n\n# Uses\n- [greet](./greet.agent.md)\n\n# Command: local\nRuns in the entry file.\n\n- steps:\n  1. print: \"x\"\n",
            ),
            ("app/greet.agent.md", GREET),
            ("app/bye.agent.md", BYE),
        ],
    );
    let c = run(&p).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(units(&c), ["local", "greet"]);
    let prog = c.program.unwrap();
    assert_eq!(prog.name, "tool");
    assert_eq!(prog.doc.as_deref(), Some("Does things."));
}

#[test]
fn uses_lines_that_bring_in_a_part_of_the_program_are_not_unused() {
    let p = mem(
        "app",
        vec![
            ("app/main.agent.md", "# Program: tool\nDoes things.\n\n# Uses\n- [greet](./greet.agent.md)\n- [commands](./cmds/*.agent.md)\n"),
            ("app/greet.agent.md", GREET),
            ("app/cmds/bye.agent.md", "# Command: bye\nSays goodbye.\n\n- steps:\n  1. print: \"bye\"\n"),
        ],
    );
    // two commands and no `- start:` is fine: they are subcommands
    let w = warn(&p);
    assert!(!w.contains("Unused import"), "{w}");
    assert!(!w.contains("Unused command"), "{w}");
}

#[test]
fn a_uses_line_for_a_file_without_runnable_parts_that_nothing_uses_is_still_unused() {
    let p = mem(
        "app",
        vec![
            ("app/main.agent.md", "# Program: tool\nDoes things.\n\n# Uses\n- [invoice](./invoice.agent.md)\n\n# Command: greet\nSays hello.\n\n- steps:\n  1. print: \"hello\"\n"),
            ("app/invoice.agent.md", INVOICE),
        ],
    );
    let w = warn(&p);
    assert!(w.contains("Unused import: nothing from \"./invoice.agent.md\" is used in this file."), "{w}");
}

#[test]
fn with_exactly_one_command_start_can_be_left_out() {
    let p = mem(
        "app",
        vec![
            ("app/main.agent.md", "# Program: tool\nDoes things.\n\n# Uses\n- [greet](./greet.agent.md)\n"),
            ("app/greet.agent.md", GREET),
        ],
    );
    let c = run(&p).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(c.program.as_ref().unwrap().start.as_deref(), Some("greet"));
}

#[test]
fn with_several_commands_and_no_start_the_commands_are_subcommands() {
    let p = mem(
        "app",
        vec![
            ("app/main.agent.md", "# Program: tool\nDoes things.\n\n# Uses\n- [greet](./greet.agent.md)\n- [bye](./bye.agent.md)\n"),
            ("app/greet.agent.md", GREET),
            ("app/bye.agent.md", BYE),
        ],
    );
    let c = run(&p).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(c.program.as_ref().unwrap().start, None);
    assert_eq!(units(&c), ["greet", "bye"]);
}

#[test]
fn agents_and_services_of_the_used_files_run_without_a_start() {
    let agent = "# Agent: watcher\nWatches.\n\n- model: claude\n- tools: log\n\n## On start\n1. log: \"x\"\n";
    let p = mem(
        "app",
        vec![
            ("app/main.agent.md", "# Program: tool\nDoes things.\n\n# Uses\n- [watcher](./watcher.agent.md)\n"),
            ("app/watcher.agent.md", agent),
        ],
    );
    let c = run(&p).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(units(&c), ["watcher"]);
    assert_eq!(c.program.as_ref().unwrap().start, None);
}

#[test]
fn a_project_without_a_program_is_a_library() {
    let p = mem("app", vec![("app/main.agent.md", INVOICE)]);
    let c = run(&p).unwrap_or_else(|e| panic!("{e}"));
    assert!(c.program.is_none());
}

// ------------------------------------------------------------------ files that fail to load

#[test]
fn a_broken_tools_toml_is_reported_before_anything_else() {
    let p = Project::from_str(
        "t.agent.md",
        "# Record: A\n- id: text\n",
        "[email]\nverb = [\"send\"]\neffect = \"network\"\n",
    );
    let d = sema::check(&p).unwrap_err();
    let out = p.render(&d);
    assert!(
        out.starts_with("tools.toml:2\n  verb = [\"send\"]"),
        "{out}"
    );
    assert!(
        out.contains("Unknown key \"verb\" for tool \"email\". Did you mean \"verbs\"?"),
        "{out}"
    );
}

#[test]
fn a_syntax_error_in_an_imported_file_is_reported_with_that_file() {
    let p = mem(
        "app/main.agent.md",
        vec![
            (
                "app/main.agent.md",
                "# Uses\n- [b](./b.agent.md)\n\n# Record: A\n- id: text\n",
            ),
            ("app/b.agent.md", "# Record: B\n\t- id: text\n"),
        ],
    );
    let e = run(&p).unwrap_err();
    assert!(e.starts_with("app/b.agent.md:2"), "{e}");
}

#[test]
fn a_missing_tools_toml_means_no_tools() {
    let p = Project::from_str(
        "t.agent.md",
        "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n1. log: \"x\"\n",
        "",
    );
    let e = run(&p).unwrap_err();
    assert!(e.contains("Unknown tool \"log\"."), "{e}");
}

// ------------------------------------------------------------------ real projects

#[test]
fn the_example_folder_and_single_files_load_the_right_manifest() {
    let folder = Project::load("examples/01-invoice-chaser");
    let single = Project::load("examples/02-lead-qualifier");
    assert_eq!(
        folder.manifest.tools.keys().collect::<Vec<_>>(),
        single.manifest.tools.keys().collect::<Vec<_>>()
    );
    assert!(sema::check(&folder).is_ok());
    assert!(sema::check(&single).is_ok());
}

#[test]
fn check_results_name_their_files() {
    let p = Project::load("examples/01-invoice-chaser");
    let c = sema::check(&p).unwrap();
    assert_eq!(c.files.len(), 5);
    assert!(c
        .files
        .iter()
        .all(|f| f.starts_with("examples/01-invoice-chaser/")));
    let b = c.blocks.iter().find(|b| b.label == "Do remind").unwrap();
    assert!(c.files[b.file].ends_with("agents/invoice-chaser.agent.md"));
    assert_eq!(
        b.param,
        Some(("invoice".to_string(), "Invoice".to_string()))
    );
}
