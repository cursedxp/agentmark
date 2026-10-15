//! Workspace awareness: project roots, unsaved buffers over the disk, other files' diagnostics.

mod common;

use common::{at, example, project, range_of, uri, Client};
use lsp_types::request::HoverRequest;
use lsp_types::{HoverParams, TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams};

fn read(p: &std::path::Path) -> String {
    std::fs::read_to_string(p).unwrap()
}

#[test]
fn columns_are_utf16_units_after_umlauts_and_emoji() {
    let (_dir, root) = project(&[
        ("agentmark.toml", "name = \"p\"\n"),
        ("tools.toml", "[log]\nverbs = []\neffect = \"local\"\n\n[crm]\nverbs = [\"get\"]\nevents = [\"lead\"]\neffect = \"network\"\n"),
        ("records/lead.agent.md", "# Record: Lead\n- name: text\n"),
        (
            "main.agent.md",
            "# Uses\n- [lead](./records/lead.agent.md)\n\n# Agent: greeter\nGreets.\n\n- model: mock\n- tools: log, crm\n\n## When crm receives lead\n1. log: \"Grüße 😀 {lead.nme}\"\n",
        ),
    ]);
    let file = root.join("main.agent.md");
    let text = read(&file);
    let mut c = Client::start(None);
    c.open(&file, &text, 1);
    let d = c.diagnostics(&file, Some(1));
    let hit = d.iter().find(|d| d.message.contains("nme")).unwrap_or_else(|| panic!("{d:?}"));
    // `nme` starts after 'Grüße 😀 {lead.' — the emoji is two UTF-16 units
    assert_eq!(hit.range, range_of(&text, "nme"), "{hit:?}");
    assert!(hit.message.contains("Did you mean \"name\"?"), "{}", hit.message);
    c.shutdown();
}

#[test]
fn an_unsaved_buffer_of_an_imported_file_changes_the_diagnostics_of_the_importer() {
    let (_dir, root) = example("01-invoice-chaser");
    let rule = root.join("rules/overdue.agent.md");
    let rec = root.join("records/invoice.agent.md");
    let (rule_text, rec_text) = (read(&rule), read(&rec));
    let mut c = Client::start(None);
    c.open(&rule, &rule_text, 1);
    assert!(c.diagnostics(&rule, Some(1)).is_empty());

    // the record is open with an unsaved rename of `due`
    c.open(&rec, &rec_text.replace("- due: date", "- due_at: date"), 1);
    let d = c.diagnostics(&rule, None);
    assert!(!d.is_empty(), "the rule no longer finds `due`");
    assert!(d.iter().any(|d| d.message.contains("due")), "{d:?}");
    assert_eq!(read(&rec), rec_text, "disk unchanged");

    // closing the buffer drops the override: the disk version is checked again
    c.close(&rec);
    assert!(c.diagnostics(&rule, None).is_empty());
    c.shutdown();
}

#[test]
fn diagnostics_reach_files_that_are_not_open_and_are_cleared_when_fixed() {
    let (_dir, root) = example("01-invoice-chaser");
    let agent = root.join("agents/invoice-chaser.agent.md");
    let rule = root.join("rules/overdue.agent.md");
    let mut c = Client::start(None);
    c.open(&agent, &read(&agent), 1);
    assert!(c.diagnostics(&agent, Some(1)).is_empty());
    // the open agent file breaks the rule file (which nobody opened) by an unsaved edit of the record
    let rec = root.join("records/invoice.agent.md");
    c.open(&rec, &read(&rec).replace("- due: date", "- due_at: date"), 1);
    assert!(!c.diagnostics(&rule, None).is_empty());
    c.change(&rec, &read(&rec), 2);
    assert!(c.diagnostics(&rule, None).is_empty(), "cleared once the cause is gone");
    c.shutdown();
}

#[test]
fn a_file_without_an_agentmark_toml_is_a_single_file_project() {
    let (_dir, root) = project(&[("tools.toml", "[log]\nverbs = []\neffect = \"local\"\n")]);
    let file = root.join("solo.agent.md");
    let text = "# Agent: solo\nSays hi.\n\n- model: mock\n- tools: log\n\n## On start\n1. log: \"hi\"\n";
    let mut c = Client::start(None);
    // the file does not exist on disk at all: it is only a buffer
    c.open(&file, text, 1);
    assert!(c.diagnostics(&file, Some(1)).is_empty());
    c.change(&file, &text.replace("- log:", "- lug:"), 2);
    let d = c.diagnostics(&file, Some(2));
    assert!(d.iter().any(|d| d.message.contains("lug")), "{d:?}");
    c.shutdown();
}

#[test]
fn answers_keep_coming_from_the_last_good_check_while_another_file_is_broken() {
    let (_dir, root) = example("01-invoice-chaser");
    let agent = root.join("agents/invoice-chaser.agent.md");
    let rule = root.join("rules/overdue.agent.md");
    let (agent_text, rule_text) = (read(&agent), read(&rule));
    let mut c = Client::start(None);
    c.open(&rule, &rule_text, 1);
    c.open(&agent, &agent_text, 1);
    c.diagnostics(&agent, Some(1));
    // the agent file is now unparsable; the rule file is untouched
    c.change(&agent, &format!("{agent_text}\n\t tab\n"), 2);
    assert!(!c.diagnostics(&agent, Some(2)).is_empty());
    let h = c
        .request::<HoverRequest>(HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri(&rule) },
                position: at(&rule_text, "# Rule: overdue", 0, 3),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .expect("hover still answers");
    assert!(format!("{h:?}").contains("pure"));
    c.shutdown();
}

#[test]
fn the_server_asks_the_client_to_watch_the_project_files() {
    let (_dir, root) = example("01-invoice-chaser");
    let file = root.join("constants.agent.md");
    let mut c = Client::start(None);
    c.open(&file, &read(&file), 1);
    c.diagnostics(&file, Some(1));
    assert_eq!(c.registrations, 1);
    c.shutdown();
}

#[test]
fn a_broken_agentmark_toml_is_reported_on_that_file() {
    let (_dir, root) = example("01-invoice-chaser");
    let toml = root.join("agentmark.toml");
    let file = root.join("constants.agent.md");
    let mut c = Client::start(None);
    c.open(&file, &read(&file), 1);
    c.diagnostics(&file, Some(1));
    c.open(&toml, "name = \"x\"\n[lsp]\nmodel_action = true\n", 1);
    let d = c.diagnostics(&toml, Some(1));
    assert!(d.iter().any(|d| d.message.contains("Unknown key \"model_action\"")), "{d:?}");
    assert_eq!(d[0].range.start.line, 2);
    c.shutdown();
}

#[test]
fn hover_and_definition_survive_an_error_in_the_file_being_edited() {
    use lsp_types::request::GotoDefinition;
    use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, PartialResultParams};
    let (_dir, root) = example("01-invoice-chaser");
    let agent = root.join("agents/invoice-chaser.agent.md");
    let good = read(&agent);
    let mut c = Client::start(None);
    c.open(&agent, &good, 1);
    assert!(c.diagnostics(&agent, Some(1)).is_empty());

    for (version, broken) in [
        // line N broken in place (a syntax error), later lines keep their numbers
        (2, good.replace("- guard: reminder length < max_reminder_length", "- guard: reminder length <")),
        // a line inserted at the top, and a semantic error further down: every line moves by one
        (3, format!("> {}", good.replace("- email send customer:", "- email send custmer:"))),
    ] {
        c.change(&agent, &broken, version);
        assert!(!c.diagnostics(&agent, Some(version)).is_empty());
        let heading = at(&broken, "## Do remind", 0, 4);
        let h = c
            .request::<HoverRequest>(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri(&agent) },
                    position: heading,
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .expect("hover on a heading while the file has an error");
        assert!(format!("{h:?}").contains("model, network(email, crm), local(log)"), "{h:?}");

        // a rule of another file, used in this one
        let def = |c: &mut Client, pos| {
            c.request::<GotoDefinition>(GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams { text_document: TextDocumentIdentifier { uri: uri(&agent) }, position: pos },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            })
        };
        let Some(GotoDefinitionResponse::Array(l)) = def(&mut c, at(&broken, "overdue invoice", 0, 2)) else { panic!("no definition") };
        assert_eq!(l[0].uri, uri(&root.join("rules/overdue.agent.md")));
        // a `## Do` block in this very file, at its new line
        let Some(GotoDefinitionResponse::Array(l)) = def(&mut c, at(&broken, "do remind", 0, 4)) else { panic!("no definition") };
        assert_eq!((l[0].uri.clone(), l[0].range.start.line), (uri(&agent), heading.line));
    }
    c.shutdown();
}

#[test]
fn requests_for_other_uri_schemes_are_answered_with_null() {
    let mut c = Client::start(None);
    let v = c.request_value(
        "textDocument/hover",
        serde_json::json!({"textDocument": {"uri": "untitled:Untitled-1"}, "position": {"line": 0, "character": 0}}),
    );
    assert!(v.is_null());
    let v = c.request_value(
        "textDocument/completion",
        serde_json::json!({"textDocument": {"uri": "untitled:Untitled-1"}, "position": {"line": 0, "character": 0}}),
    );
    assert!(v.is_null());
    c.notify_value("textDocument/didOpen", serde_json::json!({"textDocument": {"uri": "untitled:Untitled-1", "languageId": "agentmark", "version": 1, "text": "x"}}));
    c.shutdown();
}

#[test]
fn config_errors_underline_the_line_not_one_character() {
    let (_dir, root) = example("01-invoice-chaser");
    let toml = root.join("agentmark.toml");
    let mut c = Client::start(None);
    c.open(&toml, "name = \"x\"\n[lsp]\nmodel_actions = \"yes\"\n", 1);
    let d = c.diagnostics(&toml, Some(1));
    let hit = d.iter().find(|d| d.message.contains("true or false")).unwrap_or_else(|| panic!("{d:?}"));
    assert_eq!(hit.range, range_of("name = \"x\"\n[lsp]\nmodel_actions = \"yes\"\n", "model_actions = \"yes\""));
    c.shutdown();
}
