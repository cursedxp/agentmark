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
    let t = good.replace("4. email send customer:", "4. email ");
    c.change(&file, &t, 2);
    let labels = complete(&mut c, &file, at(&t, "4. email ", 0, 9));
    assert_eq!(labels, vec!["send"]);
    let t = good.replace("4. email send customer:", "4. email send ");
    c.change(&file, &t, 3);
    let labels = complete(&mut c, &file, at(&t, "4. email send ", 0, 14));
    assert!(labels.contains(&"customer".to_string()) && labels.contains(&"address".to_string()), "{labels:?}");
    let t = good.replace("5. crm update invoice:", "5. crm ");
    c.change(&file, &t, 4);
    let labels = complete(&mut c, &file, at(&t, "5. crm ", 0, 7));
    assert_eq!(labels, vec!["list", "get", "update"]);

    // the statement start lists the tools of the manifest
    let t = good.replace("5. crm update invoice: status = \"reminded\"", "5. ");
    c.change(&file, &t, 5);
    let labels = complete(&mut c, &file, at(&t, "5. \n", 0, 3));
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
    let t = good.replace("into reminder", "as In into reminder");
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
    let t = good.replace("1. sha2 sha256_digest", "1. sha2 ");
    c.open(&file, &t, 1);
    let labels = complete(&mut c, &file, at(&t, "1. sha2 ", 0, 8));
    assert!(labels.contains(&"sha256_digest".to_string()), "{labels:?}");

    let main = root.join("services/api.agent.md");
    let svc = read(&main).replace("add inv to state.invoices", "add inv to state.");
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
    let t = good.replace("4. email send customer:", "4. print: text.");
    c.change(&file, &t, 2);
    let labels = complete(&mut c, &file, at(&t, "print: text.", 0, 12));
    assert!(labels.contains(&"split".to_string()) && labels.contains(&"starts_with".to_string()), "{labels:?}");
    assert_eq!(labels.len(), agentmark::stdlib::functions_of("text").count());
    let t = good.replace("4. email send customer:", "4. print: text.tr");
    c.change(&file, &t, 3);
    assert_eq!(complete(&mut c, &file, at(&t, "print: text.tr", 0, 15)), vec!["trim"]);

    // `file ` lists its verbs; a statement start offers the tool itself
    let t = good.replace("4. email send customer:", "4. file ");
    c.change(&file, &t, 4);
    let labels = complete(&mut c, &file, at(&t, "4. file ", 0, 8));
    assert_eq!(labels, ["read", "write", "append", "list", "rename", "exists", "delete"]);
    let t = good.replace("5. crm update invoice: status = \"reminded\"", "5. fi");
    c.change(&file, &t, 5);
    assert!(complete(&mut c, &file, at(&t, "5. fi", 0, 5)).contains(&"file".to_string()));
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
    assert_eq!(l[0].range.start.line, line_of(&read(&root.join("constants.agent.md")), "# Constant: max_reminder_length"));

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
    let l = definition(&mut c, &rule, at(&rt, "overdue Invoice", 0, 10));
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
    assert_eq!((l[0].uri.clone(), l[0].range.start.line), (uri(&file), line_of(&text, "into digest")));
    // a Rust block
    let l = definition(&mut c, &file, at(&text, "fingerprint(digest)", 0, 3));
    assert_eq!((l[0].uri.clone(), l[0].range.start.line), (uri(&file), line_of(&text, "## Rust: fingerprint")));
    // a binding -> its file under bindings/
    let l = definition(&mut c, &file, at(&text, "sha2 sha256", 0, 2));
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
    assert!(h.contains("Touches: `AI, crm, email, log`") && h.contains("Has the model write one reminder"), "{h}");
    let h = hover(&mut c, &file, at(&text, "## Every day at 09:00", 0, 4)).unwrap();
    assert!(h.contains("Touches: `AI, crm, email, log`") && h.contains("Finds every overdue invoice"), "{h}");
    let h = hover(&mut c, &file, at(&text, "# Agent: invoice-chaser", 0, 3)).unwrap();
    assert!(h.contains("Touches: `AI, crm, email, log`") && h.contains("Chases unpaid invoices"), "{h}");

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
    assert!(h.contains("Touches: `nothing`") && h.contains("Unpaid and more than the grace period"), "{h}");

    // a path in a statement
    let h = hover(&mut c, &file, at(&text, "length < max_reminder_length", 0, 12)).unwrap();
    assert!(h.contains("int"), "{h}");
    c.shutdown();
}

// ---------------------------------------------------------------------------------- enums

#[test]
fn variants_and_enums_have_definitions_hovers_and_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().canonicalize().unwrap().join("shapes.agent.md");
    let text = "# Enum: Shape\n- Circle: radius: int\n- Empty\n\n# Function: area\n- in: shape: Shape\n- out: int\n- touches: nothing\n- steps:\n  1. match shape:\n     - Circle c:\n       1. return c.radius\n     - Empty:\n       1. return 0\n\n# Command: go\nPrints an area.\n\n- steps:\n  1. print: \"{area(Circle { radius: 3 })}\"\n";
    let mut c = Client::start(None);
    c.open(&file, text, 1);
    let d = c.diagnostics(&file, Some(1));
    assert!(d.is_empty(), "{d:?}");

    // a variant value -> the variant's line in the enum
    let l = definition(&mut c, &file, at(text, "Circle { radius: 3 }", 0, 2));
    assert_eq!((l[0].uri.clone(), l[0].range.start.line), (uri(&file), line_of(text, "- Circle:")));
    // hover shows the variant and its fields
    let h = hover(&mut c, &file, at(text, "Circle { radius: 3 }", 0, 2)).expect("hover on a variant");
    assert!(h.contains("Shape.Circle") && h.contains("radius: int"), "{h}");
    // the branch's name is a variable of the match
    let l = definition(&mut c, &file, at(text, "c.radius", 0, 0));
    assert_eq!(l[0].range.start.line, line_of(text, "- Circle c:"));

    // statements offer the variants (values and branches) and `none`
    let typing = text.replace("1. print: \"{area(Circle { radius: 3 })}\"\n", "1. return \n");
    c.change(&file, &typing, 2);
    let names = complete(&mut c, &file, at(&typing, "1. return \n", 0, 10));
    c.change(&file, text, 3);
    assert!(names.iter().any(|n| n == "Circle") && names.iter().any(|n| n == "Empty"), "{names:?}");

    // a match that misses a variant is reported with the compiler's message, on the match line
    let bad = text.replace("     - Empty:\n       1. return 0\n", "");
    c.change(&file, &bad, 4);
    let d = c.diagnostics(&file, Some(4));
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].message.contains("does not handle the variant \"Empty\" of Shape."), "{}", d[0].message);
    assert_eq!(d[0].range.start.line, line_of(&bad, "1. match shape:"));
    c.shutdown();
}

// ---------------------------------------------------------------------------------- formatting

#[test]
fn formatting_returns_the_canonical_text_as_one_edit() {
    let (_dir, root) = example("01-invoice-chaser");
    let file = root.join("constants.agent.md");
    let messy = "# Constant: grace_period\nHow long.\n\n\n* value: 14 days\n\n# Constant: max_reminder_length\nHow many.\n\n+ value: 800\n";
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
    c.change(&file, "# Constant\n- value 14\n", 3);
    assert_eq!(fmt(&mut c), None);
    c.shutdown();
}

// ---------------------------------------------------------------------------------- v0.4 syntax

/// A small project: a record, a rule, a function, a command and a tool-using agent.
fn v04_project() -> (tempfile::TempDir, std::path::PathBuf) {
    common::project(&[
        ("agentmark.toml", "name = \"p\"\n"),
        ("tools.toml", "[log]\nverbs = []\neffect = \"local\"\n\n[crm]\nverbs = [\"get\"]\nsources = [\"invoices\"]\neffect = \"network\"\n"),
        ("records/sale.agent.md", "# Record: Sale\nOne sale.\n\n- region: text\n- amount: money\n"),
        ("rules/big.agent.md", "# Uses\n- [Sale](../records/sale.agent.md)\n\n# Rule: big Sale\nA sale of 100 EUR or more.\n\n- when: amount >= 100 EUR\n"),
        ("commands/other.agent.md", "# Command: other\nDoes something else.\n\n- steps:\n  1. print: \"other\"\n"),
        (
            "main.agent.md",
            "# Program: sales\nReads a sales file.\n\n- start: [sales-report](#command-sales-report)\n\n# Uses\n- [Sale](./records/sale.agent.md)\n- [every rule](./rules/*.agent.md)\n- [other](./commands/other.agent.md)\n\n# Function: total\nAdds up sales.\n\n- in: sales\n- out: money\n- touches: nothing\n- steps:\n  1. return sum of amount in sales\n\n# Command: sales-report\nReads a sales file.\n\n- in: path: text\n- steps:\n  1. try file read path into sales\n     otherwise:\n     1. stop\n  2. print: \"{total(sales)}\"\n  3. for each big sale in sales:\n     1. print: \"{sale.region}\"\n",
        ),
    ])
}

/// Complete at the `|` of `marked`, which replaces the whole of `main.agent.md`.
fn complete_marked(root: &Path, marked: &str) -> Vec<String> {
    let file = root.join("main.agent.md");
    let (line, col) = marked.split('\n').enumerate().find_map(|(i, l)| l.find('|').map(|c| (i, c))).expect("a | marks the cursor");
    let text = marked.replacen('|', "", 1);
    let mut c = Client::start(None);
    c.open(&file, &text, 1);
    c.diagnostics(&file, Some(1));
    let labels = complete(&mut c, &file, Position::new(line as u32, col as u32));
    c.shutdown();
    labels
}

#[test]
fn completion_offers_the_properties_of_each_part() {
    let (_d, root) = v04_project();
    let with = |head: &str| format!("{head}\nDescribed.\n\n- |\n");
    let cases = [
        ("# Function: f", vec!["in:", "out:", "touches:", "steps:"]),
        ("# Command: c", vec!["in:", "steps:"]),
        ("# Tool: t", vec!["touches:"]),
        ("# Rule: big Sale", vec!["when:"]),
        ("# Agent: a", vec!["model:", "tools:", "memory:", "timeout:"]),
        ("# Service: s", vec!["listen:", "state:"]),
    ];
    for (head, want) in cases {
        assert_eq!(complete_marked(&root, &with(head)), want, "{head}");
    }
    assert_eq!(complete_marked(&root, "# Program: p\nDescribed.\n\n- |\n"), vec!["start:"]);
    // a Rust block of a function has its own two properties
    let rust = "# Function: f\nD.\n\n- steps:\n  1. return 1\n\n## Rust: g -> int\nD.\n\n- |\n";
    assert_eq!(complete_marked(&root, rust), vec!["in:", "touches:"]);
}

#[test]
fn completion_offers_headings_and_keywords_of_v04() {
    let (_d, root) = v04_project();
    let labels = complete_marked(&root, "# |\n");
    for want in ["Program: ", "Uses", "Import: crate ", "Function: ", "Command: ", "Rule: "] {
        assert!(labels.contains(&want.to_string()), "{want} in {labels:?}");
    }
    assert_eq!(complete_marked(&root, "# Us|\n"), vec!["Uses"]);
    let labels = complete_marked(&root, "# Agent: a\nD.\n\n- tools: log\n\n## |\n");
    assert!(labels.contains(&"Every day at ".to_string()) && labels.contains(&"Do ".to_string()), "{labels:?}");

    // a numbered step offers the step keywords (`add`, no `set`) and the tools
    let steps = "# Command: c\nD.\n\n- steps:\n  1. |\n";
    let labels = complete_marked(&root, steps);
    for want in ["add ", "print:", "guard:", "for each ", "ask model", "stop", "log", "file"] {
        assert!(labels.contains(&want.to_string()), "{want} in {labels:?}");
    }
    assert!(!labels.iter().any(|l| l.starts_with("set") || l.starts_with("append")), "{labels:?}");
    // a nested step too
    let labels = complete_marked(&root, "# Command: c\nD.\n\n- steps:\n  1. if 1 > 0:\n     1. |\n");
    assert!(labels.contains(&"print:".to_string()), "{labels:?}");
}

#[test]
fn completion_offers_into_after_a_step_that_gives_a_result() {
    let (_d, root) = v04_project();
    let steps = |line: &str| format!("# Command: c\nD.\n\n- in: path: text\n- steps:\n  1. {line}\n");
    assert_eq!(complete_marked(&root, &steps("file read path |")), vec!["into"]);
    assert_eq!(complete_marked(&root, &steps("ask model with path: \"Say hi.\" |")), vec!["into"]);
    assert_eq!(complete_marked(&root, &steps("file read path in|")), vec!["into"]);
    // not after a step that has no result, nor when the result is already named
    assert!(complete_marked(&root, &steps("print: \"x\" |")).is_empty());
    assert!(!complete_marked(&root, &steps("file read path into |")).contains(&"into".to_string()));
}

#[test]
fn completion_offers_touches_words_start_commands_and_the_verbs_of_a_tool() {
    let (_d, root) = v04_project();
    let labels = complete_marked(&root, "# Function: f\nD.\n\n- touches: |\n");
    for want in ["nothing", "files", "print", "input", "AI", "crm", "log"] {
        assert!(labels.contains(&want.to_string()), "{want} in {labels:?}");
    }
    // `- start:` offers the commands of the program
    let labels = complete_marked(&root, "# Program: p\nD.\n\n- start: |\n\n# Command: sales-report\nD.\n\n- steps:\n  1. stop\n");
    assert!(labels.contains(&"sales-report".to_string()) && labels.contains(&"other".to_string()), "{labels:?}");
    // a tool word in a numbered step is followed by its verbs, then a rule after `for each`
    assert_eq!(complete_marked(&root, "# Command: c\nD.\n\n- steps:\n  1. crm |\n"), vec!["get"]);
    assert_eq!(complete_marked(&root, "# Uses\n- [every rule](./rules/*.agent.md)\n\n# Command: c\nD.\n\n- steps:\n  1. for each bi| in sales:\n     1. stop\n"), vec!["big"]);
    // a `do` names an action: a function of the project
    let labels = complete_marked(&root, "# Function: total\nAdds.\n\n- in: sales\n- out: money\n- steps:\n  1. return 1 EUR\n\n# Command: c\nD.\n\n- steps:\n  1. do |\n");
    assert!(labels.contains(&"total".to_string()), "{labels:?}");
}

#[test]
fn completion_offers_the_files_under_uses_relative_to_this_file() {
    let (_d, root) = v04_project();
    let labels = complete_marked(&root, "# Uses\n- [rec](./rec|\n");
    assert_eq!(labels, vec!["./records/", "./records/sale.agent.md"]);
    let labels = complete_marked(&root, "# Uses\n- [sale](./records/|\n");
    assert_eq!(labels, vec!["./records/sale.agent.md"]);
    let labels = complete_marked(&root, "# Uses\n- [any](|\n");
    for want in ["./records/sale.agent.md", "./rules/big.agent.md", "./commands/other.agent.md", "./rules/", "./commands/"] {
        assert!(labels.contains(&want.to_string()), "{want} in {labels:?}");
    }
    assert!(!labels.iter().any(|l| l.contains("main.agent.md")), "{labels:?}");

    // from a file in a sub-folder the paths climb with `../`
    let file = root.join("rules/big.agent.md");
    let text = "# Uses\n- [Sale](../rec\n";
    let mut c = Client::start(None);
    c.open(&file, text, 1);
    c.diagnostics(&file, Some(1));
    let labels = complete(&mut c, &file, at(text, "../rec", 0, 6));
    assert_eq!(labels, vec!["../records/", "../records/sale.agent.md"]);
    let labels = complete(&mut c, &file, at(text, "../rec", 0, 3));
    assert!(labels.contains(&"../records/".to_string()) && labels.contains(&"../commands/other.agent.md".to_string()), "{labels:?}");
    c.shutdown();
}

#[test]
fn go_to_definition_follows_a_uses_link_and_the_start_of_a_program() {
    let (_d, root) = v04_project();
    let file = root.join("main.agent.md");
    let text = read(&file);
    let mut c = Client::start(None);
    c.open(&file, &text, 1);
    let d = c.diagnostics(&file, Some(1));
    assert!(d.is_empty(), "{d:?}");

    // a `# Uses` link -> the top of the file
    let l = definition(&mut c, &file, at(&text, "./records/sale.agent.md", 0, 4));
    assert_eq!((l[0].uri.clone(), l[0].range.start.line), (uri(&root.join("records/sale.agent.md")), 0));
    // the link text works too, and a glob goes to its first file
    let l = definition(&mut c, &file, at(&text, "[other]", 0, 2));
    assert_eq!(l[0].uri, uri(&root.join("commands/other.agent.md")));
    let l = definition(&mut c, &file, at(&text, "./rules/*.agent.md", 0, 4));
    assert_eq!(l[0].uri, uri(&root.join("rules/big.agent.md")));

    // `- start:` as the link to the heading -> the command's heading, in this very file
    let l = definition(&mut c, &file, at(&text, "#command-sales-report", 0, 5));
    assert_eq!(l.len(), 1, "{l:?}");
    assert_eq!((l[0].uri.clone(), l[0].range.start.line), (uri(&file), line_of(&text, "# Command: sales-report")));
    let l = definition(&mut c, &file, at(&text, "[sales-report]", 0, 3));
    assert_eq!(l[0].range.start.line, line_of(&text, "# Command: sales-report"));

    // the other forms: a link to the file that declares the command, and the plain name
    for start in ["[other](./commands/other.agent.md)", "[other](./commands/other.agent.md#command-other)", "other"] {
        let t = text.replace("[sales-report](#command-sales-report)", start);
        c.change(&file, &t, 2);
        let l = definition(&mut c, &file, at(&t, "- start: ", 0, 9));
        assert_eq!(l.len(), 1, "{start}: {l:?}");
        assert_eq!(l[0].uri, uri(&root.join("commands/other.agent.md")), "{start}");
    }
    c.shutdown();
}

#[test]
fn hover_on_a_heading_shows_its_description_and_what_it_touches() {
    let (_d, root) = v04_project();
    let file = root.join("main.agent.md");
    let text = read(&file);
    let mut c = Client::start(None);
    c.open(&file, &text, 1);
    c.diagnostics(&file, Some(1));
    let h = hover(&mut c, &file, at(&text, "# Function: total", 0, 4)).expect("function");
    assert!(h.contains("Adds up sales.") && h.contains("Touches: `nothing`"), "{h}");
    let h = hover(&mut c, &file, at(&text, "# Command: sales-report", 0, 4)).expect("command");
    assert!(h.contains("Reads a sales file.") && h.contains("Touches: `files, print`"), "{h}");
    assert!(!h.contains("Effects"), "{h}");
    // a call shows what the called function touches
    let h = hover(&mut c, &file, at(&text, "total(sales)", 0, 2)).expect("call");
    assert!(h.contains("Touches: `nothing`"), "{h}");
    // a record heading shows its description
    let rec = root.join("records/sale.agent.md");
    let rt = read(&rec);
    c.open(&rec, &rt, 1);
    let h = hover(&mut c, &rec, at(&rt, "# Record: Sale", 0, 11)).expect("record");
    assert!(h.contains("One sale."), "{h}");
    c.shutdown();
}

#[test]
fn removed_v03_syntax_is_reported_with_its_code_and_a_range_on_the_line() {
    let (_d, root) = v04_project();
    let file = root.join("old.agent.md");
    let cases = [
        ("E062", "# Command: c\nD.\n\n- args: path: text\n- steps:\n  1. stop\n", "- args: path: text"),
        ("E064", "# Command: c\nD.\n\n- steps:\n  1. set x = 1\n", "set x = 1"),
        ("E065", "# Command: c\nD.\n\n- in: path: text\n- steps:\n  1. file read path -> content\n", "file read path -> content"),
        ("E066", "# Command: c\nD.\n\n- steps:\n  1. empty list of int into xs\n  2. append 1 to xs\n", "append 1 to xs"),
        ("E063", "# Function: f\nD.\n\n- effect: pure\n- steps:\n  1. return 1\n", "- effect: pure"),
        ("E070", "# Command: c\nD.\n\n- steps:\n  - stop\n", "- stop"),
        ("E232", "# Function: f\nD.\n\n- touches: nonsense\n- steps:\n  1. return 1\n", "nonsense"),
        ("E230", "# Function: f\nD.\n\n- in: vat\n- steps:\n  1. return 1\n", "vat"),
    ];
    let mut c = Client::start(None);
    for (i, (code, text, needle)) in cases.iter().enumerate() {
        c.open(&file, text, i as i32 + 1);
        let d = c.diagnostics(&file, Some(i as i32 + 1));
        let hit = d
            .iter()
            .find(|d| d.code == Some(NumberOrString::String((*code).into())))
            .unwrap_or_else(|| panic!("{code} in {d:?}"));
        assert_eq!(hit.severity, Some(DiagnosticSeverity::ERROR), "{code}");
        assert_eq!(hit.range.start.line, line_of(text, needle), "{code}: {hit:?}");
        assert!(hit.range.end > hit.range.start, "{code} has a non-empty range: {hit:?}");
    }
    c.shutdown();
}
