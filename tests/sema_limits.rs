//! Hard limits (SPEC 18.3): nesting, statements per block, declarations per file, line length,
//! empty blocks. Every message carries the suggested fix.

mod common;
use common::*;

use agentmark::diag::Severity;
use agentmark::project::Project;
use agentmark::sema;

const HEAD: &str = "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n";

fn logs(n: usize) -> String {
    (0..n).map(|i| format!("- log: \"line {i}\"\n")).collect()
}

fn render_errors(p: &Project) -> String {
    match sema::check(p) {
        Ok(_) => String::new(),
        Err(d) => p.render(
            &d.into_iter()
                .filter(|x| x.severity == Severity::Error)
                .collect::<Vec<_>>(),
        ),
    }
}

// ------------------------------------------------------------------ statements per block

#[test]
fn twenty_five_statements_are_fine() {
    ok(&format!("{HEAD}{}", logs(25)));
}

#[test]
fn twenty_six_statements_are_an_error_with_the_fix() {
    let e = errors(&format!("{HEAD}{}", logs(26)));
    assert!(
        e.contains("The block \"On start\" has 26 statements; the limit is 25."),
        "{e}"
    );
    // the statements start on line 6; the 25th is on line 30, the last on line 31
    assert!(
        e.contains(
            "Move lines 30–31 into a \"## Do <name>\" block and call it with \"do <name>\"."
        ),
        "{e}"
    );
}

#[test]
fn nested_statements_count_too() {
    let mut body = String::from("for each i in crm.invoices:\n");
    for k in 0..25 {
        body.push_str(&format!("  - log: \"{k}\"\n"));
    }
    let src = format!("# Record: Invoice\n- id: text\n\n# Agent: a\n- model: claude\n- tools: crm, log\n\n## On start\n{body}");
    let e = errors(&src);
    assert!(e.contains("has 26 statements; the limit is 25."), "{e}");
}

#[test]
fn a_function_with_too_many_statements_is_told_to_split_into_functions() {
    let body: String = (0..26).map(|i| format!("  - set v{i} = {i}\n")).collect();
    let e = errors(&format!(
        "# Function: f\n- out: int\n- effect: pure\n{body}  - return 1\n"
    ));
    assert!(
        e.contains("The function \"f\" has 27 statements; the limit is 25."),
        "{e}"
    );
    assert!(
        e.contains("into a separate \"# Function\" and call it with \"do <name>(...)\"."),
        "{e}"
    );
}

// ------------------------------------------------------------------ nesting

const REC: &str = "# Record: Invoice\n- id: text\n\n";

#[test]
fn for_each_if_if_is_the_deepest_allowed() {
    ok(&format!("{REC}# Agent: a\n- model: claude\n- tools: crm, log\n\n## On start\nfor each i in crm.invoices:\n  - if i.id = \"a\":\n    - if i.id = \"b\":\n      - log: \"deep enough\"\n"));
}

#[test]
fn a_fourth_level_is_an_error_with_the_lines_to_move() {
    let src = format!("{REC}# Agent: a\n- model: claude\n- tools: crm, log\n\n## On start\nfor each i in crm.invoices:\n  - if i.id = \"a\":\n    - if i.id = \"b\":\n      - if i.id = \"c\":\n        - log: \"too deep\"\n        - log: \"still\"\n");
    let e = errors(&src);
    assert!(
        e.contains("Statements are nested 4 levels deep; the limit is 3."),
        "{e}"
    );
    assert!(
        e.contains(
            "Move lines 12–14 into a \"## Do <name>\" block and call it with \"do <name>\"."
        ),
        "{e}"
    );
    assert!(e.starts_with("t.agent.md:12\n"), "{e}");
    assert_eq!(e.matches("nested").count(), 1, "{e}");
}

#[test]
fn else_and_try_otherwise_count_as_the_same_or_one_more_level() {
    // if/else is one level; try … otherwise adds one
    let src = format!("{REC}# Agent: a\n- model: claude\n- tools: crm, log, http\n\n## On start\nfor each i in crm.invoices:\n  - if i.id = \"a\":\n    - log: \"x\"\n  - else:\n    - if i.id = \"b\":\n      - log: \"y\"\n    - else:\n      - log: \"z\"\n");
    ok(&src);
    let src = format!("{REC}# Agent: a\n- model: claude\n- tools: crm, log, http\n\n## On start\nfor each i in crm.invoices:\n  - if i.id = \"a\":\n    - if i.id = \"b\":\n      - try http get \"https://x.y\" -> r\n        otherwise:\n          - log: \"z\"\n");
    let e = errors(&src);
    assert!(e.contains("nested 4 levels deep"), "{e}");
}

// ------------------------------------------------------------------ declarations per file

#[test]
fn one_of_each_kind_per_file_is_fine() {
    let src = "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- log: \"x\"\n\n# Service: s\n\n## GET /x\n- respond 200 \"ok\"\n\n# Command: c\n\n## Run\n- print: \"x\"\n";
    ok(src);
}

#[test]
fn two_agents_in_one_file_are_an_error() {
    let src = "# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- log: \"x\"\n\n# Agent: b\n- model: claude\n- tools: log\n\n## On start\n- log: \"y\"\n";
    let e = first_error(src);
    assert!(
        e.contains("A file may hold only one agent, but \"b\" is the second (the first is \"a\")."),
        "{e}"
    );
    assert!(
        e.contains(
            "Move \"b\" into its own file, for example \"agents/b.agent.md\", and import it."
        ),
        "{e}"
    );
}

#[test]
fn records_and_rules_may_share_a_single_file_project() {
    ok("# Record: A\n- id: text\n\n# Record: B\n- id: text\n\n# Rule: r\n- for: A\n- when: id = \"x\"\n\n# Rule: s\n- for: B\n- when: id = \"y\"\n");
}

#[test]
fn in_a_folder_project_records_and_rules_need_their_folders() {
    let rec = |n: &str| format!("# Record: {n}\n- id: text\n");
    let p = Project::from_memory(
        "app",
        vec![
            ("app/records/all.agent.md", &format!("{}\n{}", rec("A"), rec("B"))),
            ("app/other/two.agent.md", &format!("{}\n{}", rec("C"), rec("D"))),
            ("app/rules/both.agent.md", "# Import: ../records/all.agent.md\n\n# Rule: r\n- for: A\n- when: id = \"x\"\n\n# Rule: s\n- for: A\n- when: id = \"y\"\n"),
            ("app/mixed.agent.md", "# Import: ./records/all.agent.md\n\n# Record: E\n- id: text\n\n# Rule: t\n- for: B\n- when: id = \"x\"\n"),
        ],
    );
    let e = render_errors(&p);
    assert!(
        e.contains("Several records in one file are only allowed in a \"records/\" folder."),
        "{e}"
    );
    assert!(
        e.contains("Move \"D\" into its own file, for example \"records/D.agent.md\"."),
        "{e}"
    );
    assert!(e.contains("Records and rules cannot share a file."), "{e}");
    // records/all and rules/both are fine
    assert!(!e.contains("app/records/all.agent.md"), "{e}");
    assert!(!e.contains("app/rules/both.agent.md"), "{e}");
}

// ------------------------------------------------------------------ line length

#[test]
fn a_line_of_120_characters_is_fine_121_is_not() {
    // "  - set a = " is 12 characters; digits and " + 1" groups fill the rest
    let line = |digits: usize| format!("  - set a = {}{}", "1".repeat(digits), " + 1".repeat(26));
    assert_eq!(line(4).chars().count(), 120);
    assert_eq!(line(5).chars().count(), 121);
    let wrap = |l: &str| {
        format!("# Function: f\n- out: int\n- effect: pure\n{l}\n  - return a\n# Function: g\n- out: int\n- effect: pure\n  - return f()\n")
    };
    ok(&wrap(&line(4)));
    let e = errors(&wrap(&line(5)));
    assert!(
        e.contains("This line is 121 characters long; the limit is 120."),
        "{e}"
    );
    assert!(
        e.contains("Split the statement into shorter ones, or use shorter names."),
        "{e}"
    );
    // the caret starts at column 121
    let caret_line = e.lines().nth(2).unwrap();
    assert_eq!(caret_line.find('^'), Some(2 + 120), "{e}");
}

#[test]
fn the_text_of_a_string_literal_does_not_count() {
    let prose = "word ".repeat(40);
    ok(&format!("{HEAD}- log: \"{prose}\"\n"));
}

#[test]
fn long_comment_lines_count_in_full() {
    let comment = format!("> {}", "c".repeat(130));
    let e = errors(&format!("{comment}\n\n{HEAD}- log: \"x\"\n"));
    assert!(e.contains("characters long; the limit is 120."), "{e}");
}

// ------------------------------------------------------------------ empty blocks

#[test]
fn an_empty_block_is_an_error() {
    let e = errors(&format!("{HEAD}"));
    assert!(
        e.contains("The block \"On start\" has no statements."),
        "{e}"
    );
    assert!(e.contains("An empty block hides what was meant."), "{e}");
}

#[test]
fn an_empty_function_and_an_empty_route_are_errors_too() {
    let e = errors("# Function: f\n- effect: pure\n");
    assert!(e.contains("The function \"f\" has no statements."), "{e}");
    let e = errors("# Service: s\n\n## GET /x\n");
    assert!(e.contains("The route \"GET /x\" has no statements."), "{e}");
}
