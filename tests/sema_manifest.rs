//! Tool manifest rules (SPEC 4.3, 7, 2.3): unknown tool / verb / target / source / event, tools
//! an agent does not list, payload fields, results.

mod common;
use common::*;

const PRE: &str = "# Record: Invoice\n- id: text\n- customer: text\n- status: text\n- score: maybe int\n\n# Record: Lead\n- id: text\n\n";

fn agent(tools: &str, body: &str) -> String {
    format!("{PRE}# Agent: a\n- model: claude\n- tools: {tools}\n\n## On start\n{body}")
}

fn first(tools: &str, body: &str) -> String {
    first_error(&agent(tools, body))
}

#[test]
fn unknown_tool_is_reported_once_with_a_suggestion() {
    let e = errors(&agent("email, log", "- emial send customer: \"x\"\n"));
    assert_eq!(e.matches("Unknown tool").count(), 1, "{e}");
    assert!(
        e.contains("Unknown tool \"emial\". Did you mean \"email\"?"),
        "{e}"
    );
}

#[test]
fn unknown_tool_in_the_agent_tools_line() {
    let e = first("email, lgo", "- log: \"x\"\n");
    assert!(
        e.contains("Unknown tool \"lgo\". Did you mean \"log\"?"),
        "{e}"
    );
    assert!(e.contains("Tools are declared in tools.toml"), "{e}");
}

#[test]
fn unknown_verb_target_and_unlisted_tool() {
    let e = first("email, log", "- email sned customer: \"x\"\n");
    assert!(
        e.contains("Unknown verb \"sned\" for tool email. Did you mean \"send\"?"),
        "{e}"
    );
    let e = first("email, log", "- email send custmer: \"x\"\n");
    assert!(
        e.contains("Unknown target \"custmer\" for tool email. Did you mean \"customer\"?"),
        "{e}"
    );
    let e = first("email, log", "- crm update invoice: status = \"x\"\n");
    assert!(
        e.contains("The tool \"crm\" is not listed in the tools of agent \"a\"."),
        "{e}"
    );
    assert!(
        e.contains("Add it to the \"- tools:\" line, for example \"- tools: email, log, crm\"."),
        "{e}"
    );
}

#[test]
fn a_tool_that_needs_a_verb_and_one_that_takes_none() {
    let e = first("email, log", "- email: \"x\"\n");
    assert!(e.contains("The tool email needs a verb."), "{e}");
    assert!(e.contains("Verbs of email: send."), "{e}");
    let e = first("email, log", "- log send: \"x\"\n");
    assert!(
        e.contains("The tool log has no verbs, but \"send\" was given."),
        "{e}"
    );
}

#[test]
fn a_tool_without_targets_does_not_take_a_target_word() {
    let e = first("http", "- http post nowhere: \"x\"\n");
    assert!(
        e.contains("The tool http has no targets, so \"nowhere\" is not expected here."),
        "{e}"
    );
}

#[test]
fn crm_targets_are_singular_sources() {
    ok(&agent(
        "crm, log",
        "for each i in crm.invoices:\n  - crm update invoice: status = \"x\"\n",
    ));
    let e = first(
        "crm, log",
        "for each i in crm.invoices:\n  - crm update invoce: status = \"x\"\n",
    );
    assert!(
        e.contains("Unknown target \"invoce\" for tool crm. Did you mean \"invoice\"?"),
        "{e}"
    );
}

#[test]
fn unknown_source_suggests() {
    let e = first("crm, log", "for each i in crm.invoces:\n  - log: \"x\"\n");
    assert!(
        e.contains("Unknown source \"invoces\" for tool crm. Did you mean \"invoices\"?"),
        "{e}"
    );
}

#[test]
fn reading_a_source_needs_the_tool_in_the_tools_line() {
    let e = first("log", "for each i in crm.invoices:\n  - log: \"x\"\n");
    assert!(
        e.contains("The tool \"crm\" is not listed in the tools of agent \"a\"."),
        "{e}"
    );
}

#[test]
fn a_source_needs_a_record_of_the_singular_name() {
    let e = first("crm, log", "for each t in crm.tickets:\n  - log: \"x\"\n");
    assert!(
        e.contains(
            "\"crm.tickets\" holds tickets, but no record \"Ticket\" is visible in this file."
        ),
        "{e}"
    );
    assert!(
        e.contains("Declare \"# Record: Ticket\" or import the file that does."),
        "{e}"
    );
}

#[test]
fn a_tool_name_alone_is_not_a_value() {
    let e = first("crm, log", "- set x = crm\n");
    assert!(e.contains("\"crm\" is a tool, not a value."), "{e}");
}

#[test]
fn unknown_event_and_missing_event_record() {
    let e = first(
        "crm, log",
        "- log: \"x\"\n\n## When crm receives leed\n- log: \"y\"\n",
    );
    assert!(
        e.contains("Unknown event \"leed\" for tool crm. Did you mean \"lead\"?"),
        "{e}"
    );
    let e = first(
        "crm, log",
        "- log: \"x\"\n\n## When crm receives ticket\n- log: \"y\"\n",
    );
    assert!(e.contains("The event \"ticket\" arrives as a record, but no record \"Ticket\" is visible in this file."), "{e}");
    let e = first(
        "email, log",
        "- log: \"x\"\n\n## When email receives message\n- log: \"y\"\n",
    );
    assert!(
        e.contains("Unknown event \"message\" for tool email."),
        "{e}"
    );
    assert!(
        e.contains("The tool email has no events in tools.toml."),
        "{e}"
    );
}

#[test]
fn an_event_block_needs_its_tool_in_the_tools_line() {
    let e = first(
        "log",
        "- log: \"x\"\n\n## When crm receives lead\n- log: \"y\"\n",
    );
    assert!(e.contains("The tool \"crm\" is not listed"), "{e}");
}

#[test]
fn the_event_record_is_in_scope_by_name_and_bare() {
    ok(&agent(
        "crm, log",
        "- log: \"x\"\n\n## When crm receives lead\n- log: \"{lead.id} {id}\"\n",
    ));
}

#[test]
fn assignments_check_field_names_and_types() {
    let e = first(
        "crm, log",
        "for each i in crm.invoices:\n  - crm update invoice: statuss = \"x\"\n",
    );
    assert!(
        e.contains("Unknown field \"statuss\" on Invoice. Did you mean \"status\"?"),
        "{e}"
    );
    let e = first(
        "crm, log",
        "for each i in crm.invoices:\n  - crm update invoice: status = 5\n",
    );
    assert!(
        e.contains("The field \"status\" of Invoice is text, but this is a number."),
        "{e}"
    );
    // an int fits a `maybe int` field
    ok(&agent(
        "crm, log",
        "for each i in crm.invoices:\n  - crm update invoice: score = 5\n",
    ));
}

#[test]
fn results_of_get_and_list_have_record_types() {
    let c = ok(&agent(
        "crm, log",
        "- crm get invoice \"x\" -> inv\n- crm list invoices -> all\n- crm update invoice: inv\n- log: \"{inv.id} {count of all}\"\n",
    ));
    let types: Vec<String> = c.expr_types.values().map(|t| t.to_string()).collect();
    assert!(types.contains(&"Invoice".to_string()), "{types:?}");
    assert!(types.contains(&"list of Invoice".to_string()), "{types:?}");
}

#[test]
fn http_results_are_text() {
    let c = ok(&agent(
        "http, log",
        "- http get \"https://x.y\" -> body\n- log: \"{body}\"\n",
    ));
    assert!(c.expr_types.values().any(|t| t.to_string() == "text"));
}

#[test]
fn the_resolved_manifest_is_part_of_the_result() {
    let c = ok(&agent("email, log", "- log: \"x\"\n"));
    assert_eq!(
        c.manifest.tool_names().collect::<Vec<_>>(),
        ["crm", "email", "http", "log"]
    );
    assert_eq!(
        c.manifest.tool("crm").unwrap().sources,
        ["invoices", "leads", "tickets"]
    );
}

#[test]
fn services_and_commands_may_use_any_manifest_tool() {
    let src = format!("{PRE}# Service: api\n- listen: 0.0.0.0:80\n\n## GET /x\n- log: \"x\"\n- respond 200 \"ok\"\n");
    ok(&src);
}

#[test]
fn respond_and_read_body_only_in_routes() {
    let e = first("log", "- respond 200 \"x\"\n");
    assert!(
        e.contains("\"respond\" only works inside a service route."),
        "{e}"
    );
    let e = first("log", "- read body as Invoice -> x\n");
    assert!(
        e.contains("\"read body\" only works inside a service route."),
        "{e}"
    );
}

#[test]
fn append_and_print_and_read_file() {
    let src = format!("{PRE}# Command: c\n- in: path: text\n- steps:\n  1. read file path as list of Invoice into rows\n  2. add \"x\" to rows\n  3. print: \"n\"\n");
    let e = first_error(&src);
    assert!(
        e.contains("This list holds Invoice, but this is text."),
        "{e}"
    );
}

#[test]
fn commands_use_the_state_of_the_services_record() {
    let src = format!("{PRE}# Record: AppState\n- invoices: list of Invoice\n\n# Service: api\n- state: AppState\n\n## GET /x\n1. respond 200 state.invoices\n\n# Command: c\n- in: n: int = 3\n- steps:\n  1. add state.invoices to state.invoices\n");
    // appending a list to a list of invoices is a type error; the point is that `state` resolves
    let e = first_error(&src);
    assert!(
        e.contains("This list holds Invoice, but this is list of Invoice."),
        "{e}"
    );
}

#[test]
fn parameter_defaults_are_type_checked() {
    let src =
        format!("{PRE}# Command: c\n- in: n: int = \"three\"\n- steps:\n  1. print: \"{{n}}\"\n");
    let e = first_error(&src);
    assert!(
        e.contains("The default of \"n\" is text, but the parameter is int."),
        "{e}"
    );
}
