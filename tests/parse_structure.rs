//! Behavior of the parser on specific constructs (the snapshots show everything; these tests say
//! what each construct is supposed to mean).

use agentmark::diag::{render_all, Span};
use agentmark::syntax::ast::*;
use agentmark::syntax::parser::parse_file;

fn parse(src: &str) -> File {
    match parse_file("t.agent.md", src) {
        Ok(f) => f,
        Err(d) => panic!("unexpected errors:\n{}", render_all(&d, src)),
    }
}

fn errors(src: &str) -> String {
    match parse_file("t.agent.md", src) {
        Ok(_) => panic!("expected an error"),
        Err(d) => render_all(&d, src),
    }
}

fn read(path: &str) -> File {
    let src = std::fs::read_to_string(path).unwrap();
    parse(&src)
}

fn agent(f: &File) -> &Agent {
    f.decls
        .iter()
        .find_map(|d| {
            if let Decl::Agent(a) = d {
                Some(a)
            } else {
                None
            }
        })
        .expect("an agent")
}

#[test]
fn invoice_chaser_agent_file() {
    let f = read("examples/01-invoice-chaser/agents/invoice-chaser.agent.md");
    assert_eq!(f.imports.len(), 3);
    assert!(
        matches!(&f.imports[0].kind, ImportKind::File { path, alias: None } if path == "../constants.agent.md")
    );
    let a = agent(&f);
    assert_eq!(a.name.name, "invoice-chaser");
    assert_eq!(
        a.doc.as_deref(),
        Some("Chases unpaid invoices once a day. Never changes amounts; only sends reminders.")
    );
    assert_eq!(a.model.value, "claude");
    assert_eq!(
        a.tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["email", "crm", "log"]
    );
    assert_eq!(a.blocks.len(), 2);

    let every = &a.blocks[0];
    assert_eq!(every.trigger, Trigger::DailyAt { hour: 9, minute: 0 });
    assert_eq!(every.label, "Every day at 09:00");
    let StmtKind::ForEach {
        filter_rule,
        var,
        source,
        body,
    } = &every.body[0].kind
    else {
        panic!()
    };
    assert_eq!(filter_rule.as_ref().unwrap().name, "overdue");
    assert_eq!(var.name, "invoice");
    assert!(matches!(&source.kind, ExprKind::Path(p) if p.len() == 2));
    assert!(matches!(&body[0].kind, StmtKind::Do { name, .. } if name.name == "remind"));

    let remind = &a.blocks[1];
    assert!(matches!(&remind.trigger, Trigger::Do(n) if n.name == "remind"));
    let kinds: Vec<&str> = remind
        .body
        .iter()
        .map(|s| match &s.kind {
            StmtKind::AskModel { .. } => "ask",
            StmtKind::Guard { .. } => "guard",
            StmtKind::ToolCall { .. } => "tool",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["ask", "guard", "guard", "tool", "tool", "tool"]);
}

#[test]
fn statement_spans_are_file_positions() {
    let f = read("examples/01-invoice-chaser/agents/invoice-chaser.agent.md");
    let a = agent(&f);
    // line 15 is `1. for each overdue invoice in crm.invoices:` (the text starts at column 4)
    assert_eq!(a.blocks[0].body[0].span.line, 15);
    assert_eq!(a.blocks[0].body[0].span.col, 4);
    // `   1. do remind` is on line 16, text starts at column 7
    let StmtKind::ForEach { body, .. } = &a.blocks[0].body[0].kind else {
        panic!()
    };
    assert_eq!(body[0].span, Span::new(16, 7, 9));
}

#[test]
fn a_bare_for_each_control_line_keeps_its_file_position() {
    // the text of a numbered step starts after "1. "
    let f = parse("# Agent: a\n- model: claude\n\n## On start\n1. for each overdue invoice in crm.invoices:\n   1. stop\n");
    let a = agent(&f);
    let s = &a.blocks[0].body[0];
    assert!(matches!(s.kind, StmtKind::ForEach { .. }));
    assert_eq!((s.span.line, s.span.col), (5, 4));
}

#[test]
fn rule_and_constants() {
    let f = read("examples/01-invoice-chaser/rules/overdue.agent.md");
    let Decl::Rule(r) = &f.decls[0] else { panic!() };
    assert_eq!(r.for_record.name, "Invoice");
    assert_eq!(
        r.doc.as_deref(),
        Some("Unpaid and more than the grace period past due.")
    );
    assert!(matches!(
        &r.when.kind,
        ExprKind::Binary { op: BinOp::And, .. }
    ));

    let f = read("examples/01-invoice-chaser/constants.agent.md");
    let Decl::Constant(c) = &f.decls[0] else {
        panic!()
    };
    assert!(
        matches!(&c.value.kind, ExprKind::Duration(d) if d.amount == 14 && d.unit == DurationUnit::Day)
    );
    let Decl::Constant(c) = &f.decls[1] else {
        panic!()
    };
    assert!(matches!(&c.value.kind, ExprKind::Int(800)));
}

#[test]
fn program_declaration() {
    let f = read("examples/01-invoice-chaser/main.agent.md");
    let Decl::Program(r) = &f.decls[0] else { panic!() };
    assert_eq!(r.name.name, "invoice-chaser");
    assert_eq!(
        r.doc.as_deref(),
        Some("Chases unpaid invoices once a day with a polite reminder. It never changes an amount.")
    );
}

#[test]
fn record_fields_with_and_without_types() {
    let f = read("examples/02-lead-qualifier/records/lead.agent.md");
    let Decl::Record(r) = &f.decls[0] else {
        panic!()
    };
    assert_eq!(r.fields.len(), 5);
    let score = r.fields.last().unwrap();
    assert!(matches!(
        &score.ty.as_ref().unwrap().kind,
        TypeKind::Maybe(_)
    ));

    let f = parse("# Record: R\n- due_date\n- amount:\n- note: text\n");
    let Decl::Record(r) = &f.decls[0] else {
        panic!()
    };
    assert!(r.fields[0].ty.is_none() && r.fields[1].ty.is_none() && r.fields[2].ty.is_some());
}

#[test]
fn lead_qualifier_event_trigger_and_if_else() {
    let f = read("examples/02-lead-qualifier/agents/lead-qualifier.agent.md");
    let a = agent(&f);
    let b = &a.blocks[0];
    assert!(
        matches!(&b.trigger, Trigger::OnEvent { tool, event } if tool.name == "crm" && event.name == "lead")
    );
    let StmtKind::AskModel {
        with,
        as_record,
        into,
        ..
    } = &b.body[0].kind
    else {
        panic!()
    };
    assert!(with.is_some());
    assert_eq!(as_record.as_ref().unwrap().name, "Qualification");
    assert_eq!(into.name, "q");
    let StmtKind::If { cond, then, else_ } = &b.body[3].kind else {
        panic!()
    };
    assert!(matches!(&cond.kind, ExprKind::Binary { op: BinOp::Ge, .. }));
    assert_eq!(then.len(), 1);
    assert_eq!(else_.len(), 1);
}

#[test]
fn support_triage_nesting_and_stop() {
    let f = read("examples/03-support-triage/agents/support-triage.agent.md");
    let a = agent(&f);
    assert!(
        matches!(&a.blocks[0].trigger, Trigger::Every(d) if d.amount == 15 && d.unit == DurationUnit::Minute)
    );
    let StmtKind::ForEach {
        filter_rule, body, ..
    } = &a.blocks[0].body[0].kind
    else {
        panic!()
    };
    assert!(filter_rule.is_none());
    let StmtKind::If { cond, then, .. } = &body[0].kind else {
        panic!()
    };
    assert!(matches!(&cond.kind, ExprKind::IsNotEmpty(_)));
    assert!(matches!(&then[0].kind, StmtKind::Stop));
    let StmtKind::If { then, .. } = &body[4].kind else {
        panic!()
    };
    let StmtKind::ToolCall {
        tool,
        verb,
        target,
        payload,
        ..
    } = &then[0].kind
    else {
        panic!()
    };
    assert_eq!(
        (tool.name.as_str(), verb.as_ref().unwrap().name.as_str()),
        ("http", "post")
    );
    assert!(matches!(&target[0], TargetPart::Value(_)));
    assert!(
        matches!(payload, Some(Payload::Value(e)) if matches!(&e.kind, ExprKind::Record(f) if f.len() == 2))
    );
}

#[test]
fn daily_report_weekly_trigger_and_counts() {
    let f = read("examples/04-daily-report/agents/daily-report.agent.md");
    let a = agent(&f);
    assert!(matches!(
        &a.blocks[0].trigger,
        Trigger::Weekly {
            day: Weekday::Monday,
            hour: 8,
            minute: 0
        }
    ));
    let StmtKind::Set { var, expr } = &a.blocks[0].body[0].kind else {
        panic!()
    };
    assert_eq!(var.name, "open_count");
    assert!(
        matches!(&expr.kind, ExprKind::CountOf { rule: Some(r), noun: Some(n), .. } if r.name == "open" && n.name == "invoices")
    );
    let StmtKind::ToolCall { target, .. } = &a.blocks[0].body[4].kind else {
        panic!()
    };
    assert!(matches!(&target[0], TargetPart::Word(w) if w.name == "address"));
}

#[test]
fn invoice_service_layer_two_and_one() {
    // the function file: a crate import, a Function and the `## Rust:` block hoisted right after it
    let f = read("examples/05-invoice-service/functions/invoice-fingerprint.agent.md");
    assert!(
        matches!(&f.imports[1].kind, ImportKind::Crate { name, alias: None } if name.name == "sha2")
    );
    let names: Vec<&str> = f
        .decls
        .iter()
        .map(|d| match d {
            Decl::Function(_) => "function",
            Decl::RustBlock(_) => "rust",
            _ => "other",
        })
        .collect();
    assert_eq!(names, ["function", "rust"]);
    let Decl::RustBlock(r) = &f.decls[1] else {
        panic!()
    };
    assert_eq!(r.name.name, "fingerprint");
    assert_eq!(r.parent.as_ref().unwrap().name, "invoice_fingerprint");
    assert_eq!(r.touches.words[0].name, "nothing");
    assert_eq!(r.params[0].name.name, "digest");
    assert_eq!(r.rust.code, "digest.iter().map(|b| format!(\"{:02x}\", b)).collect::<String>()");
    assert_eq!(r.rust.line, 23);

    let f = read("examples/05-invoice-service/functions/net-amount.agent.md");
    let Decl::Function(func) = &f.decls[0] else {
        panic!()
    };
    assert_eq!(func.params.len(), 2);
    assert_eq!(func.touches.as_ref().unwrap().words[0].name, "nothing");
    assert!(matches!(&func.out.as_ref().unwrap().kind, TypeKind::Named(n) if n == "money"));
    assert_eq!(func.body.len(), 1);
    assert!(matches!(&func.body[0].kind, StmtKind::Return(_)));

    let f = read("examples/05-invoice-service/services/api.agent.md");
    let Decl::Service(s) = &f.decls[0] else {
        panic!()
    };
    assert_eq!(s.listen.as_ref().unwrap().value, "0.0.0.0:8080");
    assert_eq!(s.state.as_ref().unwrap().name, "AppState");
    assert_eq!(s.routes.len(), 3);
    assert_eq!(
        (s.routes[0].method.as_str(), s.routes[0].path.as_str()),
        ("GET", "/invoices/{id}")
    );
    assert_eq!(s.routes[0].path_params, ["id"]);
    assert!(
        matches!(&s.routes[0].body[0].kind, StmtKind::Set { expr, .. } if matches!(&expr.kind, ExprKind::FirstIn { .. }))
    );
    let StmtKind::Guard { else_, .. } = &s.routes[0].body[1].kind else {
        panic!()
    };
    assert!(matches!(
        &else_.as_ref().unwrap().kind,
        StmtKind::Respond {
            status: 404,
            body: None
        }
    ));
    assert!(matches!(
        &s.routes[2].body[0].kind,
        StmtKind::ReadBody { .. }
    ));

    let f = read("examples/05-invoice-service/commands/import-csv.agent.md");
    let Decl::Command(c) = &f.decls[0] else {
        panic!()
    };
    assert_eq!(c.args.len(), 2);
    assert!(c.args[1].default.is_some());
    // try ... otherwise: with its own two-statement body
    let StmtKind::Try { stmt, otherwise } = &c.body[0].kind else {
        panic!()
    };
    assert!(matches!(&stmt.kind, StmtKind::ReadFile { .. }));
    let otherwise = otherwise.as_ref().unwrap();
    assert!(matches!(&otherwise[0].kind, StmtKind::Print(_)));
    assert!(matches!(&otherwise[1].kind, StmtKind::Stop));
    let StmtKind::ForEach { body, .. } = &c.body[1].kind else {
        panic!()
    };
    assert!(matches!(&body[0].kind, StmtKind::If { .. }));
}

#[test]
fn imports_of_every_shape() {
    let f = parse(
        "# Uses\n- [invoice](./records/invoice.agent.md)\n- [rules](./billing/rules.agent.md) as billing\n- [every rule](./rules/*.agent.md)\n- [billing](./billing/) as billing\n\n# Import: crate reqwest as http_client\n# Import: crate serde_json\n",
    );
    let kinds: Vec<&ImportKind> = f.imports.iter().map(|i| &i.kind).collect();
    assert!(matches!(kinds[0], ImportKind::File { path, alias: None } if path == "./records/invoice.agent.md"));
    assert!(matches!(kinds[1], ImportKind::File { alias: Some(a), .. } if a.name == "billing"));
    assert!(matches!(kinds[2], ImportKind::Glob { pattern } if pattern == "./rules/*.agent.md"));
    assert!(
        matches!(kinds[3], ImportKind::Folder { path, alias } if path == "./billing" && alias.name == "billing")
    );
    // the link text is a label for people
    let labels: Vec<Option<&str>> = f.imports.iter().map(|i| i.label.as_deref()).collect();
    assert_eq!(labels, [Some("invoice"), Some("rules"), Some("every rule"), Some("billing"), None, None]);
    assert!(
        matches!(kinds[4], ImportKind::Crate { alias: Some(a), .. } if a.name == "http_client")
    );
    assert!(
        matches!(kinds[5], ImportKind::Crate { name, alias: None } if name.name == "serde_json")
    );
}

#[test]
fn function_body_lives_under_the_effect_line() {
    let f = parse(
        "# Function: net_amount\n- in: invoice: Invoice, vat: int\n- out: money\n- touches: nothing\n- steps:\n  1. invoice.amount into gross\n  2. return gross - gross * vat / 100\n",
    );
    let Decl::Function(func) = &f.decls[0] else {
        panic!()
    };
    assert_eq!(func.body.len(), 2);
    assert!(matches!(&func.body[0].kind, StmtKind::Set { .. }));
}

#[test]
fn if_some_and_else_with_dash_form() {
    let f = parse(
        "# Agent: a\n- model: claude\n\n## On start\n1. if invoice.score is some s:\n   1. log: \"{s}\"\n   else:\n   1. log: \"unscored\"\n",
    );
    let a = agent(&f);
    assert_eq!(a.blocks[0].body.len(), 1);
    let StmtKind::IfSome {
        bind, then, else_, ..
    } = &a.blocks[0].body[0].kind
    else {
        panic!()
    };
    assert_eq!(bind.name, "s");
    assert_eq!((then.len(), else_.len()), (1, 1));
}

#[test]
fn parallel_block_and_do_with_arguments() {
    let f = parse(
        "# Command: c\n- steps:\n  1. parallel:\n     1. do fetch_rates into rates\n     2. do fetch_invoices(1, \"x\") into invoices\n",
    );
    let Decl::Command(c) = &f.decls[0] else {
        panic!()
    };
    let StmtKind::Parallel(body) = &c.body[0].kind else {
        panic!()
    };
    assert_eq!(body.len(), 2);
    assert!(matches!(&body[0].kind, StmtKind::Do { into: Some(i), .. } if i.name == "rates"));
    assert!(matches!(&body[1].kind, StmtKind::Do { args, .. } if args.len() == 2));
}

#[test]
fn binding_with_verbatim_rust_and_not_bound() {
    let src = "# Binding: pdf\ngenerated by agentmark bind — printpdf 0.7.0 — 2026-10-13\nedit freely; re-run `agentmark bind printpdf --force` to regenerate\n\n- crate: printpdf\n- effect: unverified\n- proposed-effect: local\n\n## render(doc: Document) -> bytes\nRenders a document to PDF bytes.\n\n```rust\nlet mut pdf = printpdf::PdfDocument::new(&doc.title, Mm(210.0), Mm(297.0), \"L1\");\n    // indented comment\nOk(pdf.save_to_bytes()?)\n```\n\n## Not bound\n- print_with_icc: lifetimes in the public API\n- other\n";
    let f = parse(src);
    let Decl::Binding(b) = &f.decls[0] else {
        panic!()
    };
    assert!(b.doc.as_deref().unwrap().contains("printpdf 0.7.0"));
    assert_eq!(b.crate_name.name, "printpdf");
    assert_eq!(b.effect.value, Effect::Unverified);
    assert_eq!(b.proposed_effect.as_ref().unwrap().value, Effect::Local);
    let item = &b.items[0];
    assert_eq!(item.name.name, "render");
    assert_eq!(item.params[0].ty.kind, TypeKind::Named("Document".into()));
    assert_eq!(
        item.doc.as_deref(),
        Some("Renders a document to PDF bytes.")
    );
    assert_eq!(
        item.rust.code,
        "let mut pdf = printpdf::PdfDocument::new(&doc.title, Mm(210.0), Mm(297.0), \"L1\");\n    // indented comment\nOk(pdf.save_to_bytes()?)"
    );
    assert_eq!(b.not_bound.len(), 2);
    assert_eq!(b.not_bound[0].item, "print_with_icc");
    assert_eq!(
        b.not_bound[0].reason.as_deref(),
        Some("lifetimes in the public API")
    );
    assert!(b.not_bound[1].reason.is_none());
}

#[test]
fn rust_block_inside_an_agent_is_hoisted() {
    let src = "# Agent: a\n- model: claude\n\n## Rust: hash_payload -> text\n- in: payload: text\n- touches: nothing\n\n```rust\nuse sha2::{Digest, Sha256};\nformat!(\"{:x}\", Sha256::digest(payload.as_bytes()))\n```\n";
    let f = parse(src);
    assert!(
        matches!(&f.decls[1], Decl::RustBlock(r) if r.name.name == "hash_payload" && r.parent.as_ref().unwrap().name == "a")
    );
    assert!(agent(&f).blocks.is_empty());
}

#[test]
fn doc_comment_attaches_to_the_next_heading_only_when_adjacent() {
    let f = parse("> about the file\n\n# Record: A\n- x\n\n# Record: B\nabout b\n\n- y\n");
    assert_eq!(f.doc.as_deref(), Some("about the file"));
    let Decl::Record(a) = &f.decls[0] else {
        panic!()
    };
    let Decl::Record(b) = &f.decls[1] else {
        panic!()
    };
    assert!(a.doc.is_none());
    assert_eq!(b.doc.as_deref(), Some("about b"));
}

#[test]
fn block_doc_comments() {
    let f = parse(
        "# Agent: a\n- model: claude\n\n## Every day at 07:30\nRuns every morning.\n\n1. stop\n",
    );
    assert_eq!(
        agent(&f).blocks[0].doc.as_deref(),
        Some("Runs every morning.")
    );
}

#[test]
fn headings_are_case_insensitive_identifiers_are_not() {
    let f = parse("# agent: Mixed-Case\n- model: claude\n\n## EVERY day AT 09:00\n1. stop\n");
    assert_eq!(agent(&f).name.name, "Mixed-Case");
}

#[test]
fn unicode_text_keeps_columns_in_characters() {
    let f = parse("# Agent: a\n- model: claude\n\n## On start\n1. log: \"Müller, grüß dich\"\n");
    let a = agent(&f);
    assert_eq!(a.blocks[0].body[0].span.col, 4);
    assert_eq!(
        a.blocks[0].body[0].span.len,
        "log: \"Müller, grüß dich\"".chars().count() as u32
    );
}

// ---------------------------------------------------------------- syntax errors

#[test]
fn else_without_if_is_an_error() {
    let out = errors("# Agent: a\n- model: claude\n\n## On start\n1. else:\n   1. stop\n");
    assert!(
        out.contains("\"else:\" belongs inside an \"if\" step, after its steps."),
        "{out}"
    );
}

#[test]
fn unknown_property_suggests_the_right_one() {
    let out = errors("# Agent: a\n- modle: claude\n");
    assert!(
        out.contains("Unknown property \"modle\" for an Agent. Did you mean \"model\"?"),
        "{out}"
    );
}

#[test]
fn unknown_declaration_suggests_the_right_one() {
    let out = errors("# Agnet: a\n- model: claude\n");
    assert!(
        out.contains("Unknown heading \"Agnet\". Did you mean \"Agent\"?"),
        "{out}"
    );
}

#[test]
fn missing_model_and_duplicate_property() {
    let out = errors("# Agent: a\n- tools: log\n");
    assert!(
        out.contains("The agent \"a\" has no \"model\" line."),
        "{out}"
    );
    let out = errors("# Agent: a\n- model: claude\n- model: claude\n");
    assert!(
        out.contains("The property \"model\" is given twice."),
        "{out}"
    );
}

#[test]
fn bad_statement_points_at_the_problem() {
    let out = errors("# Agent: a\n- model: claude\n\n## On start\n1. guard: x is not emptyy\n");
    assert!(out.contains("t.agent.md:5"), "{out}");
    assert!(out.contains("^^^^^^"), "{out}");
}

#[test]
fn statements_too_deeply_indented() {
    let out = errors("# Agent: a\n- model: claude\n\n## On start\n1. stop\n   1. stop\n");
    assert!(
        out.contains("Nothing can be indented under this step."),
        "{out}"
    );
}

#[test]
fn unclosed_fence_and_prose_are_reported() {
    assert!(errors("# Binding: b\n```rust\nfn x() {}\n").contains("never closed"));
    // a paragraph right under a heading is its description; prose after the lists is not allowed
    assert!(errors("# Record: R\n- x\nsome prose\n").contains("not part of the language"));
}

#[test]
fn rust_block_without_effect_or_code() {
    let out = errors("# Agent: a\n- model: claude\n\n## Rust: f -> text\n\n```rust\nx\n```\n");
    assert!(out.contains("does not say what it touches"), "{out}");
    let out = errors("# Agent: a\n- model: claude\n\n## Rust: f -> text\n- touches: nothing\n");
    assert!(
        out.contains("needs its Rust code in a fenced block"),
        "{out}"
    );
}

#[test]
fn folder_import_needs_a_name() {
    let out = errors("# Uses\n- [billing](./billing/)\n");
    assert!(out.contains("To use a folder, give it a name"), "{out}");
}

#[test]
fn several_errors_are_all_reported() {
    let out = errors("# Agent: a\n- modle: claude\n- toolz: log\n");
    assert!(out.contains("modle") && out.contains("toolz"), "{out}");
}

#[test]
fn hyphenated_rule_and_record_names_can_be_referenced() {
    let f = parse(
        "# Rule: very-overdue Big-Invoice\n- when: status = \"x\"\n\n# Agent: a\n- model: claude\n\n## On start\n1. for each very-overdue invoice in crm.invoices:\n   1. stop\n2. count of very-overdue invoices in crm.invoices into n\n3. read body as list of Big-Invoice into rows\n",
    );
    let Decl::Rule(r) = &f.decls[0] else { panic!() };
    assert_eq!(r.for_record.name, "Big-Invoice");
    let a = agent(&f);
    let StmtKind::ForEach { filter_rule, .. } = &a.blocks[0].body[0].kind else {
        panic!()
    };
    assert_eq!(filter_rule.as_ref().unwrap().name, "very-overdue");
    let StmtKind::Set { expr, .. } = &a.blocks[0].body[1].kind else {
        panic!()
    };
    assert!(
        matches!(&expr.kind, ExprKind::CountOf { rule: Some(r), .. } if r.name == "very-overdue")
    );
    assert!(matches!(
        &a.blocks[0].body[2].kind,
        StmtKind::ReadBody { .. }
    ));
}

#[test]
fn plain_comments_stay_attached_to_the_next_statement_field_and_property() {
    let src = "# Record: R\n> the id\n- id: text\n- note: text\n\n# Rule: r R\n> only open ones\n- when: note = \"x\"\n\n# Agent: a\n- model: claude\n\n## On start\n> a long condition, explained\n> over two lines\n1. guard: note = \"x\"\n2. stop\n3. for each x in y:\n   > inside the loop\n   1. stop\n";
    let f = parse(src);
    let Decl::Record(r) = &f.decls[0] else {
        panic!()
    };
    assert_eq!(r.fields[0].comments, ["the id"]);
    assert!(r.fields[1].comments.is_empty());
    let Decl::Rule(rule) = &f.decls[1] else {
        panic!()
    };
    assert_eq!(rule.prop_comments.len(), 1);
    assert_eq!(rule.prop_comments[0].key, "when");
    assert_eq!(rule.prop_comments[0].comments, ["only open ones"]);
    let body = &agent(&f).blocks[0].body;
    assert_eq!(
        body[0].comments,
        ["a long condition, explained", "over two lines"]
    );
    assert!(body[1].comments.is_empty());
    let StmtKind::ForEach { body: inner, .. } = &body[2].kind else {
        panic!()
    };
    assert_eq!(inner[0].comments, ["inside the loop"]);
    // doc comments are not repeated as plain comments
    let f = parse("# Record: R\ndoc\n\n- id\n");
    let Decl::Record(r) = &f.decls[0] else {
        panic!()
    };
    assert_eq!(r.doc.as_deref(), Some("doc"));
    assert!(r.fields[0].comments.is_empty());
}
