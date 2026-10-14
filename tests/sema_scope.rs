//! Scope and project-level rules (SPEC 13.2, 18.2): a file sees only what it imports, aliases,
//! unused imports and declarations, crate imports, `# Run`, duplicate names, import cycles,
//! manifest files.

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
                "# Function: f\n- in: i: Invoice\n- out: text\n- effect: pure\n  - return i.id\n",
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
            ("app/main.agent.md", "# Import: ./mid.agent.md\n\n# Function: f\n- in: i: Invoice\n- out: text\n- effect: pure\n  - return i.id\n"),
            ("app/mid.agent.md", "# Import: ./invoice.agent.md\n\n# Function: g\n- in: i: Invoice\n- out: text\n- effect: pure\n  - return i.id\n"),
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
            ("app/main.agent.md", "# Import: ./invoice.agent.md\n\n# Function: f\n- in: i: Invoice\n- out: text\n- effect: pure\n  - return i.id\n\n# Function: g\n- in: i: Invoice\n- out: text\n- effect: pure\n  - return f(i)\n"),
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
    let main = "# Import: ./invoice.agent.md\n# Import: ./billing as billing\n\n# Function: f\n- in: i: Invoice\n- out: bool\n- effect: pure\n  - return billing.overdue(i)\n";
    let fns = "# Import: ../invoice.agent.md\n\n# Function: overdue\n- in: i: Invoice\n- out: bool\n- effect: pure\n  - return i.amount > 5 EUR\n";
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
            ("app/main.agent.md", "# Import: ./rules/*.agent.md\n# Import: ./invoice.agent.md\n\n# Function: f\n- in: i: Invoice\n- out: bool\n- effect: pure\n  - return a(i) and b(i)\n"),
            ("app/invoice.agent.md", INVOICE),
            ("app/rules/a.agent.md", "# Import: ../invoice.agent.md\n\n# Function: a\n- in: i: Invoice\n- out: bool\n- effect: pure\n  - return true\n"),
            ("app/rules/b.agent.md", "# Import: ../invoice.agent.md\n\n# Function: b\n- in: i: Invoice\n- out: bool\n- effect: pure\n  - return true\n"),
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
                "# Import: ./b.agent.md\n\n# Record: A\n- id: text\n",
            ),
            (
                "app/b.agent.md",
                "# Import: ./a.agent.md\n\n# Record: B\n- id: text\n",
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
                "# Import: ./invoice.agent.md\n\n# Record: Invoice\n- id: text\n",
            ),
            ("app/invoice.agent.md", INVOICE),
        ],
    );
    let e = run(&p).unwrap_err();
    assert!(
        e.contains("The record \"Invoice\" is already declared in app/main.agent.md:3."),
        "{e}"
    );
}

// ------------------------------------------------------------------ unused

#[test]
fn unused_imports_are_warnings() {
    let p = mem(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", "# Import: ./invoice.agent.md\n# Import: ./other.agent.md\n\n# Function: f\n- in: i: Invoice\n- out: text\n- effect: pure\n  - return i.id\n\n# Function: g\n- out: text\n- effect: pure\n  - return f(i)\n"),
            ("app/invoice.agent.md", INVOICE),
            ("app/other.agent.md", "# Record: Other\n- id: text\n"),
        ],
    );
    // `g` uses an unknown `i`: an error, so check the warnings through a variant without it
    let _ = p;
    let p = mem(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", "# Import: ./invoice.agent.md\n# Import: ./other.agent.md\n\n# Function: f\n- in: i: Invoice\n- out: text\n- effect: pure\n  - return i.id\n"),
            ("app/invoice.agent.md", INVOICE),
            ("app/other.agent.md", "# Record: Other\n- id: text\n"),
        ],
    );
    let w = warn(&p);
    assert!(
        w.contains("app/main.agent.md:2\n  # Import: ./other.agent.md"),
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
        "# Record: Lonely\n- id: text\n\n# Constant: unused_limit\n- value: 5\n\n# Function: idle\n- out: int\n- effect: pure\n  - return 1\n\n# Rule: r\n- for: Lonely\n- when: id = \"x\"\n",
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
        "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- log: \"x\"\n",
        TOOLS,
    );
    assert_eq!(warn(&p), "");
}

// ------------------------------------------------------------------ crate imports

#[test]
fn a_crate_import_without_a_binding_file_is_known_but_unbound() {
    let p = Project::from_str(
        "t.agent.md",
        "# Import: crate sha2\n\n# Function: host\n- in: s: text\n- out: text\n- effect: pure\n  - return f(s)\n\n## Rust: f -> text\n- in: s: text\n- effect: pure\n  ```rust\n  use sha2::{Digest, Sha256};\n  format!(\"{:x}\", Sha256::digest(s.as_bytes()))\n  ```\n",
        TOOLS,
    );
    let w = warn(&p);
    assert!(w.contains("The crate \"sha2\" has no binding yet."), "{w}");
    assert!(w.contains("agentmark bind sha2"), "{w}");
    assert!(!w.contains("Unused import"), "{w}");
}

#[test]
fn a_crate_that_no_rust_block_mentions_is_unused() {
    let p = Project::from_str("t.agent.md", "# Import: crate sha2\n\n# Record: A\n- id: text\n\n# Function: f\n- in: a: A\n- out: text\n- effect: pure\n  - return a.id\n", TOOLS);
    let w = warn(&p);
    assert!(
        w.contains("Unused import: the crate \"sha2\" is not used in this file."),
        "{w}"
    );
}

// ------------------------------------------------------------------ # Run

#[test]
fn run_items_must_be_imported_agents_services_or_commands() {
    let main = "# Import: ./agent.agent.md\n\n# Run\n- invoice-chaser\n- ghost\n";
    let agent =
        "# Agent: invoice-chaser\n- model: claude\n- tools: log\n\n## On start\n- log: \"x\"\n";
    let p = mem(
        "app",
        vec![("app/main.agent.md", main), ("app/agent.agent.md", agent)],
    );
    let e = run(&p).unwrap_err();
    assert!(
        e.contains("Unknown agent, service or command \"ghost\"."),
        "{e}"
    );
    assert!(!e.contains("\"invoice-chaser\""), "{e}");
}

#[test]
fn run_suggests_close_names() {
    let main = "# Import: ./agent.agent.md\n\n# Run\n- invoice-chaser\n";
    let agent =
        "# Agent: invoice-chasr\n- model: claude\n- tools: log\n\n## On start\n- log: \"x\"\n";
    let p = mem(
        "app",
        vec![("app/main.agent.md", main), ("app/agent.agent.md", agent)],
    );
    let e = run(&p).unwrap_err();
    assert!(
        e.contains(
            "Unknown agent, service or command \"invoice-chaser\". Did you mean \"invoice-chasr\"?"
        ),
        "{e}"
    );
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
                "# Import: ./b.agent.md\n\n# Record: A\n- id: text\n",
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
        "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- log: \"x\"\n",
        "",
    );
    let e = run(&p).unwrap_err();
    assert!(e.contains("Unknown tool \"log\"."), "{e}");
}

// ------------------------------------------------------------------ real projects

#[test]
fn the_example_folder_and_single_files_load_the_right_manifest() {
    let folder = Project::load("examples/01-invoice-chaser");
    let single = Project::load("examples/02-lead-qualifier.agent.md");
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
