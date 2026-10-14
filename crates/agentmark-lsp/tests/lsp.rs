//! One test per capability, driven over the protocol with `lsp-types` messages (no editor).

mod common;

use common::{at, example, range_of, uri, Client};
use lsp_types::request::{
    Completion, Formatting, GotoDefinition, HoverRequest,
};
use lsp_types::{
    CompletionParams, CompletionResponse,
    DiagnosticSeverity, DocumentFormattingParams, FormattingOptions, GotoDefinitionParams, GotoDefinitionResponse,
    Hover, HoverContents, HoverParams, NumberOrString, PartialResultParams, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, WorkDoneProgressParams,
};
use std::path::Path;

fn tdpp(path: &Path, pos: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams { text_document: TextDocumentIdentifier { uri: uri(path) }, position: pos }
}

fn complete(c: &mut Client, path: &Path, pos: Position) -> Vec<String> {
    let r = c.request::<Completion>(CompletionParams {
        text_document_position: tdpp(path, pos),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    });
    match r {
        Some(CompletionResponse::Array(items)) => items.into_iter().map(|i| i.label).collect(),
        Some(CompletionResponse::List(l)) => l.items.into_iter().map(|i| i.label).collect(),
        None => Vec::new(),
    }
}

fn definition(c: &mut Client, path: &Path, pos: Position) -> Vec<lsp_types::Location> {
    let r = c.request::<GotoDefinition>(GotoDefinitionParams {
        text_document_position_params: tdpp(path, pos),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    });
    match r {
        Some(GotoDefinitionResponse::Array(v)) => v,
        Some(GotoDefinitionResponse::Scalar(l)) => vec![l],
        _ => Vec::new(),
    }
}

fn hover(c: &mut Client, path: &Path, pos: Position) -> Option<String> {
    let h: Option<Hover> = c.request::<HoverRequest>(HoverParams {
        text_document_position_params: tdpp(path, pos),
        work_done_progress_params: WorkDoneProgressParams::default(),
    });
    h.map(|h| match h.contents {
        HoverContents::Markup(m) => m.value,
        other => panic!("unexpected hover {other:?}"),
    })
}

fn line_of(text: &str, needle: &str) -> u32 {
    text.lines().position(|l| l.contains(needle)).unwrap() as u32
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap()
}

// ---------------------------------------------------------------------------------- diagnostics

#[test]
fn diagnostics_use_the_compiler_messages_and_ranges_and_follow_the_buffer() {
    let (_dir, root) = example("01-invoice-chaser");
    let file = root.join("agents/invoice-chaser.agent.md");
    let good = read(&file);
    let mut c = Client::start(None);

    // clean file: nothing to report
    c.open(&file, &good, 1);
    assert!(c.diagnostics(&file, Some(1)).is_empty());

    // an unsaved edit with a typo: the buffer wins over the disk
    let bad = good.replace("email send customer:", "email send custmer:");
    c.change(&file, &bad, 2);
    let d = c.diagnostics(&file, Some(2));
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].severity, Some(DiagnosticSeverity::ERROR));
    assert!(d[0].message.contains("Unknown target \"custmer\""), "{}", d[0].message);
    assert!(d[0].message.contains("Did you mean \"customer\"?"), "{}", d[0].message);
    assert_eq!(d[0].source.as_deref(), Some("agentmark"));
    assert!(matches!(&d[0].code, Some(NumberOrString::String(_))));
    // the range is the misspelled word
    let word = range_of(&bad, "custmer");
    assert_eq!(d[0].range, word);
    assert_eq!(read(&file), good, "the file on disk was not touched");

    // fixed again, and saved
    c.change(&file, &good, 3);
    c.save(&file);
    assert!(c.diagnostics(&file, Some(3)).is_empty());
    c.shutdown();
}

// ---------------------------------------------------------------------------------- completion

#[test]
fn completion_offers_records_rules_tools_verbs_constants_and_fields() {
    let (_dir, root) = example("01-invoice-chaser");
    let file = root.join("agents/invoice-chaser.agent.md");
    let good = read(&file);
    let mut c = Client::start(None);
    c.open(&file, &good, 1);

    // a tool word is followed by its verbs, then its targets
    let t = good.replace("- email send customer:", "- email ");
    c.change(&file, &t, 2);
    let labels = complete(&mut c, &file, at(&t, "- email ", 0, 8));
    assert_eq!(labels, vec!["send"]);
    let t = good.replace("- email send customer:", "- email send ");
    c.change(&file, &t, 3);
    let labels = complete(&mut c, &file, at(&t, "- email send ", 0, 13));
    assert!(labels.contains(&"customer".to_string()) && labels.contains(&"address".to_string()), "{labels:?}");
    let t = good.replace("- crm update invoice:", "- crm ");
    c.change(&file, &t, 4);
    let labels = complete(&mut c, &file, at(&t, "- crm ", 0, 6));
    assert_eq!(labels, vec!["list", "get", "update"]);

    // the statement start lists the tools of the manifest
    let t = good.replace("- crm update invoice: status = \"reminded\"", "- ");
    c.change(&file, &t, 5);
    let labels = complete(&mut c, &file, at(&t, "\n- \n", 0, 3));
    for want in ["email", "crm", "log", "guard:", "ask model"] {
        assert!(labels.contains(&want.to_string()), "{want} in {labels:?}");
    }

    // a rule after `for each`
    let t = good.replace("for each overdue invoice", "for each ov");
    c.change(&file, &t, 6);
    assert_eq!(complete(&mut c, &file, at(&t, "for each ov", 0, 11)), vec!["overdue"]);

    // a constant in an expression
    let t = good.replace("reminder length < max_reminder_length", "reminder length < max");
    c.change(&file, &t, 7);
    assert_eq!(complete(&mut c, &file, at(&t, "< max", 0, 5)), vec!["max_reminder_length"]);

    // a record after `as`
    let t = good.replace("-> reminder", "as In -> reminder");
    c.change(&file, &t, 8);
    assert_eq!(complete(&mut c, &file, at(&t, "as In", 0, 5)), vec!["Invoice"]);

    // fields after `x.`, also while the line is half typed
    let t = good.replace("{customer} about {amount}", "{invoice.}");
    c.change(&file, &t, 9);
    let labels = complete(&mut c, &file, at(&t, "{invoice.", 0, 9));
    assert_eq!(labels, vec!["id", "customer", "customer_email", "amount", "due", "status"]);
    let t = good.replace("{customer} about {amount}", "{invoice.cu");
    c.change(&file, &t, 10);
    let labels = complete(&mut c, &file, at(&t, "{invoice.cu", 0, 11));
    assert_eq!(labels, vec!["customer", "customer_email"]);

    // the cursor in the middle of a word that is already there (regression: the rest of the word
    // used to be glued to the path before the dot and made the check fail)
    let t = good.replace("{customer} about {amount}", "{invoice.customer_email} {amount}");
    c.change(&file, &t, 12);
    let labels = complete(&mut c, &file, at(&t, "{invoice.cu", 0, 11));
    assert_eq!(labels, vec!["customer", "customer_email"]);

    // no completion inside the plain text of a string
    let t = good.replace("Reminded", "Remin");
    c.change(&file, &t, 11);
    assert!(complete(&mut c, &file, at(&t, "Remin", 0, 5)).is_empty());
    c.shutdown();
}

#[test]
fn completion_offers_the_items_of_a_binding_and_the_sources_of_a_tool() {
    let (_dir, root) = example("05-invoice-service");
    let file = root.join("functions/invoice-fingerprint.agent.md");
    let good = read(&file);
    let mut c = Client::start(None);
    let t = good.replace("- sha2 sha256_digest", "- sha2 ");
    c.open(&file, &t, 1);
    let labels = complete(&mut c, &file, at(&t, "- sha2 ", 0, 7));
    assert!(labels.contains(&"sha256_digest".to_string()), "{labels:?}");

    let main = root.join("services/api.agent.md");
    let svc = read(&main).replace("append inv to state.invoices", "append inv to state.");
    c.open(&main, &svc, 1);
    let labels = complete(&mut c, &main, at(&svc, "to state.", 0, 9));
    assert_eq!(labels, vec!["invoices"], "fields of the state record");

    let (_d2, root2) = example("01-invoice-chaser");
    let f2 = root2.join("agents/invoice-chaser.agent.md");
    let t2 = read(&f2).replace("crm.invoices", "crm.");
    c.open(&f2, &t2, 1);
    let labels = complete(&mut c, &f2, at(&t2, "crm.", 0, 4));
    assert!(labels.contains(&"invoices".to_string()), "{labels:?}");
    c.shutdown();
}

#[test]
fn completion_offers_the_built_in_library_and_the_file_tool() {
    let (_dir, root) = example("01-invoice-chaser");
    let file = root.join("agents/invoice-chaser.agent.md");
    let good = read(&file);
    let mut c = Client::start(None);
    c.open(&file, &good, 1);

    // `text.` lists the functions of the module, with the same names as the language table
    let t = good.replace("- email send customer:", "- set shout = text.");
    c.change(&file, &t, 2);
    let labels = complete(&mut c, &file, at(&t, "= text.", 0, 14));
    assert!(labels.contains(&"split".to_string()) && labels.contains(&"starts_with".to_string()), "{labels:?}");
    assert_eq!(labels.len(), agentmark::stdlib::functions_of("text").count());
    let t = good.replace("- email send customer:", "- set shout = text.tr");
    c.change(&file, &t, 3);
    assert_eq!(complete(&mut c, &file, at(&t, "= text.tr", 0, 16)), vec!["trim"]);

    // `file ` lists its verbs; a statement start offers the tool itself
    let t = good.replace("- email send customer:", "- file ");
    c.change(&file, &t, 4);
    let labels = complete(&mut c, &file, at(&t, "- file ", 0, 7));
    assert_eq!(labels, ["read", "write", "append", "list", "rename", "exists", "delete"]);
    let t = good.replace("- crm update invoice: status = \"reminded\"", "- fi");
    c.change(&file, &t, 5);
    assert!(complete(&mut c, &file, at(&t, "- fi", 0, 4)).contains(&"file".to_string()));
    c.shutdown();
}

// ---------------------------------------------------------------------------------- definition

#[test]
fn go_to_definition_follows_imports_constants_variables_and_tools() {
    let (_dir, root) = example("01-invoice-chaser");
    let file = root.join("agents/invoice-chaser.agent.md");
    let text = read(&file);
    let mut c = Client::start(None);
    c.open(&file, &text, 1);

    // a rule used as a filter -> its declaration in another file
    let l = definition(&mut c, &file, at(&text, "overdue invoice", 0, 2));
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].uri, uri(&root.join("rules/overdue.agent.md")));
    let rule_text = read(&root.join("rules/overdue.agent.md"));
    assert_eq!(l[0].range, range_of(&rule_text, "overdue").clone());

    // a constant -> constants.agent.md
    let l = definition(&mut c, &file, at(&text, "max_reminder_length", 0, 3));
    assert_eq!(l[0].uri, uri(&root.join("constants.agent.md")));
    assert_eq!(l[0].range.start.line, 3);

    // the record a `## Do` block receives -> the `for each` that calls it
    let l = definition(&mut c, &file, at(&text, "with invoice", 0, 6));
    assert_eq!(l.len(), 1, "{l:?}");
    assert_eq!((l[0].uri.clone(), l[0].range.start.line), (uri(&file), line_of(&text, "for each overdue invoice")));
    let loop_line = text.lines().nth(line_of(&text, "for each overdue invoice") as usize).unwrap();
    assert_eq!(l[0].range.start.character as usize, loop_line.find("invoice in").unwrap());

    // a `## Do` action
    let l = definition(&mut c, &file, at(&text, "do remind", 0, 4));
    assert_eq!(l[0].uri, uri(&file));
    assert_eq!(l[0].range.start.line, text.lines().position(|x| x.starts_with("## Do remind")).unwrap() as u32);

    // a tool -> its table in tools.toml
    let l = definition(&mut c, &file, at(&text, "- tools: email", 0, 10));
    assert_eq!(l[0].uri, uri(&root.join("tools.toml")));

    // an import line -> the imported file
    let l = definition(&mut c, &file, at(&text, "../records/invoice.agent.md", 0, 5));
    assert_eq!(l[0].uri, uri(&root.join("records/invoice.agent.md")));

    // a bare field -> the record's field
    let rule = root.join("rules/overdue.agent.md");
    let rt = read(&rule);
    c.open(&rule, &rt, 1);
    let l = definition(&mut c, &rule, at(&rt, "due +", 0, 1));
    assert_eq!(l[0].uri, uri(&root.join("records/invoice.agent.md")));
    // a record name -> the record
    let l = definition(&mut c, &rule, at(&rt, "for: Invoice", 0, 7));
    assert_eq!(l[0].uri, uri(&root.join("records/invoice.agent.md")));
    c.shutdown();
}

#[test]
fn go_to_definition_finds_parameters_fields_rust_blocks_and_bindings() {
    let (_dir, root) = example("05-invoice-service");
    let file = root.join("functions/invoice-fingerprint.agent.md");
    let text = read(&file);
    let mut c = Client::start(None);
    c.open(&file, &text, 1);

    // a parameter -> its `- in:` line
    let l = definition(&mut c, &file, at(&text, "{invoice.id}", 0, 1));
    assert_eq!((l[0].uri.clone(), l[0].range.start.line), (uri(&file), line_of(&text, "- in: invoice")));
    // a field of the record, in another file
    let l = definition(&mut c, &file, at(&text, "invoice.id", 0, 8));
    assert_eq!(l[0].uri, uri(&root.join("records/invoice.agent.md")));
    assert_eq!(l[0].range.start.line, line_of(&read(&root.join("records/invoice.agent.md")), "- id:"));
    // a result variable -> the `->` that introduces it
    let l = definition(&mut c, &file, at(&text, "fingerprint(digest)", 0, 14));
    assert_eq!((l[0].uri.clone(), l[0].range.start.line), (uri(&file), line_of(&text, "-> digest")));
    // a Rust block
    let l = definition(&mut c, &file, at(&text, "fingerprint(digest)", 0, 3));
    assert_eq!((l[0].uri.clone(), l[0].range.start.line), (uri(&file), line_of(&text, "## Rust: fingerprint")));
    // a binding -> its file under bindings/
    let l = definition(&mut c, &file, at(&text, "- sha2 sha256", 0, 4));
    assert_eq!(l[0].uri, uri(&root.join("bindings/sha2.agent.md")));
    // a function called from another file
    let svc = root.join("services/api.agent.md");
    let st = read(&svc);
    c.open(&svc, &st, 1);
    let l = definition(&mut c, &svc, at(&st, "net_amount(found", 0, 3));
    assert_eq!(l[0].uri, uri(&root.join("functions/net-amount.agent.md")));
    c.shutdown();
}

// ---------------------------------------------------------------------------------- hover

#[test]
fn hover_shows_the_effect_summary_of_a_block_and_the_type_of_a_field() {
    let (_dir, root) = example("01-invoice-chaser");
    let file = root.join("agents/invoice-chaser.agent.md");
    let text = read(&file);
    let mut c = Client::start(None);
    c.open(&file, &text, 1);

    let h = hover(&mut c, &file, at(&text, "## Do remind", 0, 4)).expect("hover on a heading");
    assert!(h.contains("model, network(email, crm), local(log)"), "{h}");
    let h = hover(&mut c, &file, at(&text, "## Every day at 09:00", 0, 4)).unwrap();
    assert!(h.contains("model, network(crm, email), local(log)"), "{h}");
    let h = hover(&mut c, &file, at(&text, "# Agent: invoice-chaser", 0, 3)).unwrap();
    assert!(h.contains("Effects:") && h.contains("model"), "{h}");

    // a field in the record file
    let rec = root.join("records/invoice.agent.md");
    let rt = read(&rec);
    c.open(&rec, &rt, 1);
    let h = hover(&mut c, &rec, at(&rt, "- amount: money", 0, 3)).unwrap();
    assert!(h.contains("Invoice.amount") && h.contains("money"), "{h}");
    let h = hover(&mut c, &rec, at(&rt, "- customer_email: text", 0, 3)).unwrap();
    assert!(h.contains("Invoice.customer_email"), "{h}");

    // a bare field and a constant in expressions
    let rule = root.join("rules/overdue.agent.md");
    let ru = read(&rule);
    c.open(&rule, &ru, 1);
    let h = hover(&mut c, &rule, at(&ru, "due +", 0, 1)).unwrap();
    assert!(h.contains("Invoice.due") && h.contains("date"), "{h}");
    let h = hover(&mut c, &rule, at(&ru, "grace_period", 0, 3)).unwrap();
    assert!(h.contains("duration"), "{h}");
    let h = hover(&mut c, &rule, at(&ru, "# Rule: overdue", 0, 3)).unwrap();
    assert!(h.contains("pure"), "{h}");

    // a path in a statement
    let h = hover(&mut c, &file, at(&text, "length < max_reminder_length", 0, 12)).unwrap();
    assert!(h.contains("int"), "{h}");
    c.shutdown();
}

// ---------------------------------------------------------------------------------- formatting

#[test]
fn formatting_returns_the_canonical_text_as_one_edit() {
    let (_dir, root) = example("01-invoice-chaser");
    let file = root.join("constants.agent.md");
    let messy = "# Constant: grace_period\n- value: 14 days   \n\n\n\n# Constant: max_reminder_length\n- value: 800\n";
    let mut c = Client::start(None);
    c.open(&file, messy, 1);
    let fmt = |c: &mut Client| {
        c.request::<Formatting>(DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri: uri(&file) },
            options: FormattingOptions::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
    };
    let edits = fmt(&mut c).expect("edits");
    assert_eq!(edits.len(), 1);
    let want = agentmark::fmt::format_source(messy, &file.to_string_lossy()).unwrap();
    assert_eq!(agentmark_lsp::apply(messy, &edits), want);
    assert_ne!(want, messy);

    // already canonical: no edit
    c.change(&file, &want, 2);
    assert_eq!(fmt(&mut c).expect("empty list"), vec![]);

    // a file that does not parse is left alone
    c.change(&file, "# Constant\n- value 14\n\t\n", 3);
    assert_eq!(fmt(&mut c), None);
    c.shutdown();
}
