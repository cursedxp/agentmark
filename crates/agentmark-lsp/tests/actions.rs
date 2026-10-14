//! Code actions: one test per lint rule with a mechanical fix, the rules without one, and the
//! model-backed actions with the flag off and on.

mod common;

use std::path::Path;
use std::sync::Arc;

use agentmark_rt::MockModel;
use common::{project, range_of, uri, Client};
use lsp_types::request::{CodeActionRequest, CodeActionResolveRequest, ExecuteCommand};
use lsp_types::{
    CodeAction, CodeActionContext, CodeActionOrCommand, CodeActionParams, Diagnostic, ExecuteCommandParams,
    PartialResultParams, Range, TextDocumentIdentifier, WorkDoneProgressParams,
};

const TOOLS: &str = "[email]\nverbs = [\"send\"]\ntargets = [\"customer\", \"address\"]\neffect = \"network\"\n\n[crm]\nverbs = [\"list\", \"get\", \"update\"]\nsources = [\"invoices\", \"leads\"]\nevents = [\"lead\"]\neffect = \"network\"\n\n[http]\nverbs = [\"get\", \"post\"]\neffect = \"network\"\n\n[log]\nverbs = []\neffect = \"local\"\n";
const INVOICE: &str = "# Record: Invoice\n- id: text\n- customer: text\n- customer_email: text\n- amount: money\n- due: date\n- status: text\n";

fn files<'a>(main: &'a str, toml: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![("agentmark.toml", toml), ("tools.toml", TOOLS), ("records/invoice.agent.md", INVOICE), ("main.agent.md", main)]
}

fn actions(c: &mut Client, path: &Path, range: Range, diags: Vec<Diagnostic>) -> Vec<CodeAction> {
    let r = c.request::<CodeActionRequest>(CodeActionParams {
        text_document: TextDocumentIdentifier { uri: uri(path) },
        range,
        context: CodeActionContext { diagnostics: diags, only: None, trigger_kind: None },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    });
    r.unwrap_or_default()
        .into_iter()
        .map(|a| match a {
            CodeActionOrCommand::CodeAction(a) => a,
            CodeActionOrCommand::Command(c) => panic!("unexpected bare command {c:?}"),
        })
        .collect()
}

fn problems_of(path: &Path, text: &str) -> (bool, Vec<String>) {
    // the checker and the linter of the compiler library, on what is on disk
    std::fs::write(path, text).unwrap();
    let root = path.parent().unwrap().to_string_lossy().to_string();
    let p = agentmark::project::Project::load(&root);
    let ok = agentmark::sema::check(&p).is_ok();
    let cfg = agentmark::lint::LintConfig::new(p.config.lint_deny.iter().cloned());
    let files: Vec<_> = p.files.iter().map(|f| (&f.ast, f.text.as_str())).collect();
    (ok, agentmark::lint::lint_with_sources(&files, &cfg).into_iter().map(|d| d.code.to_string()).collect())
}

/// Open `main`, take the action for the finding `rule`, apply its edit, and return the new text.
/// Asserts that the project checked before and after and that the finding is gone.
fn fix(main: &str, toml: &str, rule: &str, title_has: &str) -> String {
    let (_dir, root) = project(&files(main, toml));
    let file = root.join("main.agent.md");
    let (ok_before, lint_before) = problems_of(&file, main);
    assert!(ok_before, "the fixture must check");
    assert!(lint_before.iter().any(|c| c == rule), "{rule} must fire: {lint_before:?}");

    let mut c = Client::start(None);
    c.open(&file, main, 1);
    let diags = c.diagnostics(&file, Some(1));
    let d = diags.iter().find(|d| d.code == Some(lsp_types::NumberOrString::String(rule.into()))).unwrap_or_else(|| panic!("{rule} in {diags:?}")).clone();
    let acts = actions(&mut c, &file, d.range, vec![d.clone()]);
    let a = acts.iter().find(|a| a.title.contains(title_has)).unwrap_or_else(|| panic!("{title_has} in {:?}", acts.iter().map(|a| &a.title).collect::<Vec<_>>()));
    assert_eq!(a.kind, Some(lsp_types::CodeActionKind::QUICKFIX));
    let edits = a.edit.as_ref().unwrap().changes.as_ref().unwrap().get(&uri(&file)).unwrap();
    let fixed = agentmark_lsp::apply(main, edits);

    // the editor applies it: the finding disappears from the published diagnostics
    c.change(&file, &fixed, 2);
    let after = c.diagnostics(&file, Some(2));
    assert!(!after.iter().any(|d| d.code == Some(lsp_types::NumberOrString::String(rule.into()))), "{rule} still there: {after:?}\n{fixed}");
    c.shutdown();
    let (ok_after, lint_after) = problems_of(&file, &fixed);
    assert!(ok_after, "the fixed project still checks:\n{fixed}");
    assert!(!lint_after.iter().any(|c| c == rule), "{lint_after:?}\n{fixed}");
    fixed
}

const PLAIN: &str = "name = \"p\"\n";

#[test]
fn magic_value_money_is_extracted_into_a_constant() {
    let main = "# Import: ./records/invoice.agent.md\n\n> Chases.\n# Agent: chaser\n- model: mock\n- tools: log, crm\n\n## Every day at 09:00\n- for each invoice in crm.invoices:\n  - guard: invoice.amount > 100.00 EUR\n  - guard: invoice.amount < 100.00 EUR else stop\n  - log: \"big\"\n";
    let fixed = fix(main, PLAIN, "magic-value", "Extract the amount");
    assert!(fixed.contains("# Constant: fixed_amount\n- value: 100.00 EUR\n"), "{fixed}");
    assert_eq!(fixed.matches("fixed_amount").count(), 3, "{fixed}");
}

#[test]
fn magic_value_duration_and_url_are_extracted_too() {
    let main = "# Import: ./records/invoice.agent.md\n\n> Pings.\n# Agent: pinger\n- model: mock\n- tools: http, log\n\n## Every 30 minutes\n- http post \"https://example.com/hook\": { id: \"a\" }\n- http post \"https://example.com/hook\": { id: \"b\" }\n\n## Every 30 minutes\n- log: \"x\"\n";
    // two blocks with the same heading are not valid; keep one trigger and use durations as defaults
    let main = main.replace("\n## Every 30 minutes\n- log: \"x\"\n", "");
    let fixed = fix(&main, PLAIN, "magic-value", "Extract the URL");
    assert!(fixed.contains("# Constant: fixed_url\n- value: \"https://example.com/hook\"\n"), "{fixed}");
    assert!(fixed.contains("\"{fixed_url}\""), "{fixed}");
}

#[test]
fn magic_value_duration_is_extracted_even_twice_on_one_line() {
    let main = "# Import: ./records/invoice.agent.md\n\n> Chases.\n# Agent: chaser\n- model: mock\n- tools: log, crm\n\n## Every day at 09:00\n- for each invoice in crm.invoices:\n  - guard: invoice.due + 14 days > today and invoice.due + 14 days < today + 90 days\n  - guard: invoice.due + 14 days > today\n  - log: \"x\"\n";
    let fixed = fix(main, PLAIN, "magic-value", "Extract the duration");
    assert!(fixed.contains("# Constant: fixed_duration\n- value: 14 days\n"), "{fixed}");
    assert_eq!(fixed.matches("fixed_duration").count(), 4, "{fixed}");
}

#[test]
fn unnamed_guard_gets_a_comment_stub_above_it() {
    let main = "# Import: ./records/invoice.agent.md\n\n> Chases.\n# Agent: chaser\n- model: mock\n- tools: log, crm\n\n## Every day at 09:00\n- for each invoice in crm.invoices:\n  - guard: invoice.amount > 5 EUR and invoice.status != \"paid\" and invoice.customer != \"nobody\"\n  - log: \"x\"\n";
    let fixed = fix(main, PLAIN, "unnamed-guard", "comment above this guard");
    assert!(fixed.contains("  > TODO: say what this guard protects against.\n  - guard: invoice.amount"), "{fixed}");
}

#[test]
fn prompt_in_loop_gets_a_guard_on_the_answer() {
    let main = "# Import: ./records/invoice.agent.md\n\n> Chases.\n# Agent: chaser\n- model: mock\n- tools: email, crm\n\n## Every day at 09:00\n- for each invoice in crm.invoices:\n  - ask model with invoice: \"Write a reminder.\" -> reminder\n  - email send customer: \"{reminder}\"\n";
    let fixed = fix(main, PLAIN, "prompt-in-loop", "guard on \"reminder\"");
    assert!(fixed.contains("-> reminder\n  - guard: reminder is not empty\n  - email send"), "{fixed}");
}

#[test]
fn prompt_in_loop_with_a_record_answer_gets_a_matches_guard() {
    let main = "# Import: ./records/invoice.agent.md\n\n> Chases.\n# Agent: chaser\n- model: mock\n- tools: crm, log\n\n## Every day at 09:00\n- for each invoice in crm.invoices:\n  - ask model with invoice: \"Classify it.\" as Invoice -> verdict\n  - log: \"done\"\n";
    let fixed = fix(main, PLAIN, "prompt-in-loop", "guard on \"verdict\"");
    assert!(fixed.contains("  - guard: verdict matches Invoice\n"), "{fixed}");
}

#[test]
fn missing_doc_gets_a_description_stub_above_the_heading() {
    let main = "# Import: ./records/invoice.agent.md\n\n# Agent: chaser\n- model: mock\n- tools: log\n\n## On start\n- log: \"hi\"\n";
    let fixed = fix(main, PLAIN, "missing-doc", "description above this agent");
    assert!(fixed.contains("> TODO: describe what this agent does.\n# Agent: chaser\n"), "{fixed}");
}

#[test]
fn rules_without_a_mechanical_fix_offer_none() {
    // vague-name needs a name only the author knows; long-file and duplicate-rule need a decision
    let main = "# Import: ./records/invoice.agent.md\n\n> Chases.\n# Agent: chaser\n- model: mock\n- tools: log, crm\n\n## Every day at 09:00\n- for each invoice in crm.invoices:\n  - ask model with invoice: \"Write.\" -> result\n  - guard: result is not empty\n  - log: \"{result}\"\n";
    let (_dir, root) = project(&files(main, PLAIN));
    let file = root.join("main.agent.md");
    let mut c = Client::start(None);
    c.open(&file, main, 1);
    let diags = c.diagnostics(&file, Some(1));
    let vague = diags.iter().find(|d| d.code == Some(lsp_types::NumberOrString::String("vague-name".into()))).expect("vague-name fires");
    assert!(actions(&mut c, &file, vague.range, vec![vague.clone()]).is_empty());
    c.shutdown();
}

#[test]
fn a_did_you_mean_suggestion_is_a_quick_fix() {
    let main = "# Import: ./records/invoice.agent.md\n\n> Chases.\n# Agent: chaser\n- model: mock\n- tools: email, crm\n\n## Every day at 09:00\n- for each invoice in crm.invoices:\n  - email send custmer: \"hello\"\n";
    let (_dir, root) = project(&files(main, PLAIN));
    let file = root.join("main.agent.md");
    let mut c = Client::start(None);
    c.open(&file, main, 1);
    let d = c.diagnostics(&file, Some(1));
    let err = d.iter().find(|d| d.message.contains("custmer")).unwrap();
    let acts = actions(&mut c, &file, err.range, vec![err.clone()]);
    let a = acts.iter().find(|a| a.title == "Change to \"customer\"").expect("quick fix");
    let edits = a.edit.as_ref().unwrap().changes.as_ref().unwrap().get(&uri(&file)).unwrap();
    assert_eq!(agentmark_lsp::apply(main, edits), main.replace("custmer", "customer"));
    assert_eq!(edits[0].range, range_of(main, "custmer"));
    c.shutdown();
}

// ---------------------------------------------------------------------------------- model actions

const AGENT: &str = "# Import: ./records/invoice.agent.md\n\n> Chases.\n# Agent: chaser\n- model: mock\n- tools: email, crm\n\n## Every day at 09:00\n- for each invoice in crm.invoices:\n  - ask model with invoice: \"Write a reminder.\" -> reminder\n  - guard: reminder is not empty\n  - email send customer: \"{reminder}\"\n";

fn mock() -> Arc<MockModel> {
    Arc::new(MockModel::new(
        None,
        vec![
            (agentmark_lsp::EXPLAIN_PROMPT.to_string(), "It writes a reminder for every invoice and emails it.".to_string()),
            (agentmark_lsp::GUARD_PROMPT.to_string(), "- guard: reminder length < 800".to_string()),
        ],
    ))
}

fn model_titles(c: &mut Client, file: &Path, text: &str) -> Vec<String> {
    let line = text.lines().position(|l| l.contains("ask model")).unwrap() as u32;
    let r = Range::new(lsp_types::Position::new(line, 3), lsp_types::Position::new(line, 3));
    actions(c, file, r, vec![]).into_iter().map(|a| a.title).collect()
}

#[test]
fn model_actions_are_not_offered_while_the_flag_is_off() {
    for toml in [PLAIN, "name = \"p\"\n\n[lsp]\nmodel_actions = false\n"] {
        let (_dir, root) = project(&files(AGENT, toml));
        let file = root.join("main.agent.md");
        let mut c = Client::start(Some(mock()));
        c.open(&file, AGENT, 1);
        c.diagnostics(&file, Some(1));
        let titles = model_titles(&mut c, &file, AGENT);
        assert!(titles.iter().all(|t| !t.contains("model")), "{titles:?}");
        // and the command refuses
        let err = c
            .try_request(
                "workspace/executeCommand",
                serde_json::to_value(ExecuteCommandParams {
                    command: "agentmark.explainBlock".into(),
                    arguments: vec![serde_json::json!(uri(&file).as_str()), serde_json::json!(8)],
                    work_done_progress_params: WorkDoneProgressParams::default(),
                })
                .unwrap(),
            )
            .unwrap_err();
        assert!(err.contains("model actions are off"), "{err}");
        c.shutdown();
    }
}

#[test]
fn model_actions_explain_a_block_and_suggest_a_guard_when_the_flag_is_on() {
    let toml = "name = \"p\"\n\n[lsp]\nmodel_actions = true\n";
    let (_dir, root) = project(&files(AGENT, toml));
    let file = root.join("main.agent.md");
    let mut c = Client::start(Some(mock()));
    c.open(&file, AGENT, 1);
    c.diagnostics(&file, Some(1));
    let line = AGENT.lines().position(|l| l.contains("ask model")).unwrap() as u32;
    let r = Range::new(lsp_types::Position::new(line, 3), lsp_types::Position::new(line, 3));
    let acts = actions(&mut c, &file, r, vec![]);
    let explain = acts.iter().find(|a| a.title.starts_with("Explain this block")).expect("explain offered");
    let suggest = acts.iter().find(|a| a.title.starts_with("Suggest a guard")).expect("suggest offered");

    // explain: a command, answered with the model's text (and shown as a message)
    let cmd = explain.command.clone().unwrap();
    let answer: Option<serde_json::Value> = c.request::<ExecuteCommand>(ExecuteCommandParams {
        command: cmd.command,
        arguments: cmd.arguments.unwrap(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    });
    assert_eq!(answer, Some(serde_json::json!("It writes a reminder for every invoice and emails it.")));
    let shown = c.notification("window/showMessage");
    assert!(shown.params.to_string().contains("writes a reminder"));

    // suggest: resolved lazily into an edit that inserts the model's guard after the line
    assert!(suggest.edit.is_none());
    let resolved = c.request::<CodeActionResolveRequest>(suggest.clone());
    let edits = resolved.edit.unwrap().changes.unwrap().remove(&uri(&file)).unwrap();
    let new = agentmark_lsp::apply(AGENT, &edits);
    assert!(new.contains("-> reminder\n  - guard: reminder length < 800\n  - guard: reminder is not empty"), "{new}");

    // an answer that is not a guard that parses is refused, nothing is inserted
    let bad = Arc::new(MockModel::new(None, vec![(agentmark_lsp::GUARD_PROMPT.to_string(), "guard: ((".to_string())]));
    let mut c2 = Client::start(Some(bad));
    c2.open(&file, AGENT, 1);
    c2.diagnostics(&file, Some(1));
    let acts = actions(&mut c2, &file, r, vec![]);
    let suggest = acts.iter().find(|a| a.title.starts_with("Suggest a guard")).unwrap().clone();
    let err = c2.try_request("codeAction/resolve", serde_json::to_value(suggest).unwrap()).unwrap_err();
    assert!(err.contains("does not parse"), "{err}");
    c2.shutdown();
    c.shutdown();
}

#[test]
fn doc_and_comment_stubs_are_snippets_for_clients_that_can_take_them() {
    let main = "# Import: ./records/invoice.agent.md\n\n# Agent: chaser\n- model: mock\n- tools: log\n\n## On start\n- log: \"hi\"\n";
    for (snippets, want) in [(true, "> ${1:describe what this agent does}\n"), (false, "> TODO: describe what this agent does.\n")] {
        let (_dir, root) = project(&files(main, PLAIN));
        let file = root.join("main.agent.md");
        let mut c = Client::start_with(None, snippets);
        c.open(&file, main, 1);
        let diags = c.diagnostics(&file, Some(1));
        let d = diags.iter().find(|d| d.code == Some(lsp_types::NumberOrString::String("missing-doc".into()))).unwrap().clone();
        let raw = c.request_value(
            "textDocument/codeAction",
            serde_json::to_value(CodeActionParams {
                text_document: TextDocumentIdentifier { uri: uri(&file) },
                range: d.range,
                context: CodeActionContext { diagnostics: vec![], only: None, trigger_kind: None },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            })
            .unwrap(),
        );
        let a = &raw[0];
        assert!(a.get("data").is_none());
        if snippets {
            let e = &a["edit"]["documentChanges"][0]["edits"][0];
            assert_eq!(e["newText"], want);
            assert_eq!(e["insertTextFormat"], 2);
        } else {
            assert_eq!(a["edit"]["changes"][uri(&file).as_str()][0]["newText"], want);
        }
        c.shutdown();
    }
}

/// Open `file`, apply the "Change to" fix of its first Did-you-mean finding, return the new text.
fn apply_dym(root: &Path, file: &str, text: &str, want: &str) -> String {
    let path = root.join(file);
    let mut c = Client::start(None);
    c.open(&path, text, 1);
    let d = c.diagnostics(&path, Some(1));
    let err = d.iter().find(|d| d.message.contains("Did you mean")).unwrap_or_else(|| panic!("{d:?}")).clone();
    let acts = actions(&mut c, &path, err.range, vec![err.clone()]);
    let a = acts.iter().find(|a| a.title == format!("Change to \"{want}\"")).unwrap_or_else(|| panic!("{:?}", acts.iter().map(|a| &a.title).collect::<Vec<_>>()));
    let edits = a.edit.as_ref().unwrap().changes.as_ref().unwrap().get(&uri(&path)).unwrap();
    c.shutdown();
    agentmark_lsp::apply(text, edits)
}

#[test]
fn did_you_mean_in_config_files_replaces_only_the_word() {
    let main = "# Import: ./records/invoice.agent.md\n\n> Says.\n# Agent: a\n- model: mock\n- tools: log\n\n## On start\n- log: \"hi\"\n";
    // agentmark.toml: an unknown [lsp] key
    let (_d, root) = project(&files(main, PLAIN));
    let fixed = apply_dym(&root, "agentmark.toml", "name = \"p\"\n[lsp]\nmodel_action = true\n", "model_actions");
    assert_eq!(fixed, "name = \"p\"\n[lsp]\nmodel_actions = true\n");
    std::fs::write(root.join("agentmark.toml"), &fixed).unwrap();
    let p = agentmark::project::Project::load(&root.to_string_lossy());
    assert!(p.load_diagnostics.is_empty(), "{:?}", p.load_diagnostics);
    assert!(p.config.lsp_model_actions);

    // tools.toml: a misspelled effect keeps its quotes
    let bad_tools = TOOLS.replace("effect = \"local\"", "effect = \"locl\"");
    let (_d, root) = project(&files(main, PLAIN));
    let fixed = apply_dym(&root, "tools.toml", &bad_tools, "local");
    assert_eq!(fixed, TOOLS);
    std::fs::write(root.join("tools.toml"), &fixed).unwrap();
    let p = agentmark::project::Project::load(&root.to_string_lossy());
    assert!(p.load_diagnostics.is_empty(), "{:?}", p.load_diagnostics);
    assert!(agentmark::sema::check(&p).is_ok());
}
