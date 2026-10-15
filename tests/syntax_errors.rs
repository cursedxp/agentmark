//! Regression tests for the wording of syntax errors (SPEC 10: plain language, a caret under the
//! offending word, a suggestion when one is within edit distance 2). Each test compares the whole
//! rendered message.

use agentmark::diag::render_all;
use agentmark::syntax::parser::parse_file;

fn render(src: &str) -> String {
    match parse_file("t.agent.md", src) {
        Ok(_) => panic!("expected a syntax error in:\n{src}"),
        Err(d) => render_all(&d, src),
    }
}

/// One statement inside `## On start` of a small agent (so the statement is on line 5).
fn stmt(line: &str) -> String {
    render(&format!(
        "# Agent: a\n- model: claude\n\n## On start\n1. {line}\n"
    ))
}

fn heading(h: &str) -> String {
    render(&format!("# Agent: a\n- model: claude\n\n{h}\n1. stop\n"))
}

#[test]
fn money_with_too_many_decimals() {
    assert_eq!(
        stmt("12.505 EUR into x"),
        "t.agent.md:5\n  1. 12.505 EUR into x\n     ^^^^^^^^^^\n  EUR has 2 decimal places, so \"12.505 EUR\" cannot be written exactly. AgentMark never rounds money silently."
    );
}

#[test]
fn decimal_without_currency() {
    assert_eq!(
        stmt("3.5 into x"),
        "t.agent.md:5\n  1. 3.5 into x\n     ^^^\n  Decimal numbers need a currency, like \"12.50 EUR\". AgentMark has no floating point numbers."
    );
}

#[test]
fn impossible_date() {
    assert_eq!(
        stmt("2026-13-01 into d"),
        "t.agent.md:5\n  1. 2026-13-01 into d\n     ^^^^^^^^^^\n  \"2026-13-01\" is not a real date. Dates are written year-month-day, like 2026-10-13."
    );
}

#[test]
fn impossible_time_in_a_heading() {
    assert_eq!(
        heading("## Every day at 25:00"),
        "t.agent.md:4\n  ## Every day at 25:00\n                  ^^^^^\n  A time looks like 09:00 (hours 00-23, minutes 00-59)."
    );
}

#[test]
fn http_status_message_is_a_sentence() {
    assert_eq!(
        stmt("respond 999"),
        "t.agent.md:5\n  1. respond 999\n             ^^^\n  Expected an HTTP status such as 200 or 404."
    );
}

#[test]
fn misspelled_trigger_words() {
    assert_eq!(
        heading("## Evry day at 09:00"),
        "t.agent.md:4\n  ## Evry day at 09:00\n     ^^^^\n  Unknown block heading \"Evry\". Did you mean \"Every\"?\n  A block starts with one of: Every, When, On, Do, Rust."
    );
    assert_eq!(
        heading("## Wehn crm receives lead"),
        "t.agent.md:4\n  ## Wehn crm receives lead\n     ^^^^\n  Unknown block heading \"Wehn\". Did you mean \"When\"?\n  A block starts with one of: Every, When, On, Do, Rust."
    );
}

#[test]
fn misspelled_statement_words() {
    assert_eq!(
        stmt("ask modle: \"hi\" into x"),
        "t.agent.md:5\n  1. ask modle: \"hi\" into x\n         ^^^^^\n  Unknown word \"modle\" after \"ask\". The statement is \"ask model: ...\". Did you mean \"model\"?"
    );
    assert_eq!(
        stmt("respnd 200 x"),
        "t.agent.md:5\n  1. respnd 200 x\n     ^^^^^^\n  Unknown statement \"respnd\". Did you mean \"respond\"?"
    );
}

#[test]
fn misspelled_duration_units() {
    assert_eq!(
        heading("## Every 5 minuts"),
        "t.agent.md:4\n  ## Every 5 minuts\n             ^^^^^^\n  Unknown time unit \"minuts\". Did you mean \"minutes\"?"
    );
    assert_eq!(
        stmt("today + 5 minuts into d"),
        "t.agent.md:5\n  1. today + 5 minuts into d\n               ^^^^^^\n  Unknown time unit \"minuts\". Did you mean \"minutes\"?"
    );
}

#[test]
fn misspelled_type_words() {
    assert_eq!(
        render("# Record: R\n- x: lst of int\n"),
        "t.agent.md:2\n  - x: lst of int\n       ^^^\n  Unknown type \"lst\". Did you mean \"list\"?"
    );
    assert_eq!(
        stmt("read body as lst of int into x"),
        "t.agent.md:5\n  1. read body as lst of int into x\n                  ^^^\n  Unknown type \"lst\". Did you mean \"list\"?"
    );
}

#[test]
fn a_second_value_after_a_complete_statement() {
    assert_eq!(
        stmt("emial send: \"x\" \"y\""),
        "t.agent.md:5\n  1. emial send: \"x\" \"y\"\n                     ^^^\n  I did not expect `\"y\"` here."
    );
}

#[test]
fn names_cannot_start_with_an_underscore() {
    assert_eq!(
        stmt("1 into _x"),
        "t.agent.md:5\n  1. 1 into _x\n            ^^\n  Names must start with a letter, but \"_x\" starts with an underscore."
    );
}

#[test]
fn control_lines_need_their_colon() {
    let help =
        "A line that has statements under it ends with a colon, like \"for each row in rows:\".";
    assert_eq!(
        render("# Agent: a\n- model: claude\n\n## On start\n1. for each x in y\n   1. stop\n"),
        format!("t.agent.md:5\n  1. for each x in y\n                    ^\n  This line is missing the \":\" at its end.\n  {help}")
    );
    assert_eq!(
        stmt("for each x in y"),
        format!("t.agent.md:5\n  1. for each x in y\n                    ^\n  This line is missing the \":\" at its end.\n  {help}")
    );
    assert!(stmt("if x = 1").contains("missing the \":\""));
}

#[test]
fn no_message_contains_internal_grammar_words() {
    let probes = [
        "12.505 EUR into x",
        "3.5 into x",
        "ask modle: \"x\" into y",
        "emial send: \"x\" \"y\"",
        "guard: x is not emptyy",
        "1 into",
        "crm update invoice: = 3",
        "if x is some:",
    ];
    for p in probes {
        let out = stmt(p);
        for bad in [
            "identifier",
            "inline whitespace",
            "whitespace",
            "or or",
            "\"\"",
            "found \"\"",
        ] {
            assert!(!out.contains(bad), "{p}: message contains {bad:?}:\n{out}");
        }
        assert!(
            out.lines()
                .nth(3)
                .unwrap()
                .trim_start()
                .chars()
                .next()
                .unwrap()
                .is_uppercase(),
            "{p}:\n{out}"
        );
    }
}

// ---- errors later in the line are not blamed on the first word

#[test]
fn guard_errors_point_at_the_real_problem() {
    assert_eq!(
        stmt("guard: x is not emptyy"),
        "t.agent.md:5\n  1. guard: x is not emptyy\n                     ^^^^^^\n  Expected \"empty\" here, but found \"emptyy\". Did you mean \"empty\"?"
    );
    assert_eq!(
        stmt("guard: x ="),
        "t.agent.md:5\n  1. guard: x =\n               ^\n  The line ends too early: expected a value after \"=\"."
    );
    assert_eq!(
        stmt("guard: x lenght < 3"),
        "t.agent.md:5\n  1. guard: x lenght < 3\n              ^^^^^^\n  Expected \"length\" here, but found \"lenght\". Did you mean \"length\"?"
    );
}

#[test]
fn print_and_tool_calls_are_not_unknown_statements() {
    assert_eq!(
        stmt("print: \"x\" y"),
        "t.agent.md:5\n  1. print: \"x\" y\n                ^\n  I did not expect \"y\" here."
    );
    assert_eq!(
        stmt("crm update invoice: status ="),
        "t.agent.md:5\n  1. crm update invoice: status =\n                                 ^\n  The line ends too early: expected a value after \"=\"."
    );
}

#[test]
fn misspelled_words_inside_statements() {
    assert_eq!(
        stmt("if x is somee s:"),
        "t.agent.md:5\n  1. if x is somee s:\n             ^^^^^\n  Expected \"some\" here, but found \"somee\". Did you mean \"some\"?"
    );
    assert_eq!(
        stmt("add x too y"),
        "t.agent.md:5\n  1. add x too y\n           ^^^\n  Expected \"to\" here, but found \"too\". Did you mean \"to\"?"
    );
    assert_eq!(
        stmt("if a and:"),
        "t.agent.md:5\n  1. if a and:\n             ^\n  Expected a value after \"and\", but found \":\"."
    );
}

#[test]
fn closed_word_properties_suggest() {
    assert_eq!(
        render("# Agent: a\n- model: claude\n- memory: fiel\n"),
        "t.agent.md:3\n  - memory: fiel\n            ^^^^\n  Unknown memory setting \"fiel\". Did you mean \"file\"?"
    );
}

#[test]
fn dates_must_exist_on_the_calendar() {
    assert!(stmt("2026-02-30 into d").contains("\"2026-02-30\" is not a real date"));
    assert!(stmt("2026-04-31 into d").contains("is not a real date"));
    assert!(stmt("2025-02-29 into d").contains("is not a real date"));
    let ok = |d: &str| {
        parse_file(
            "t.agent.md",
            &format!("# Agent: a\n- model: claude\n\n## On start\n1. {d} into d\n"),
        )
        .is_ok()
    };
    assert!(ok("2024-02-29") && ok("2000-02-29") && ok("2026-12-31"));
}

#[test]
fn diagnostics_are_sorted_by_position() {
    let src = "# Agent: a\n- modle: claude\n\n## On start\n1. sett x = 1\n2. 2 into _y\n\n# Record: R\n- x: lst of int\n";
    let diags = parse_file("t.agent.md", src).unwrap_err();
    let pos: Vec<(u32, u32)> = diags.iter().map(|d| (d.span.line, d.span.col)).collect();
    let mut sorted = pos.clone();
    sorted.sort();
    assert_eq!(pos, sorted);
    assert!(pos.len() >= 3);
}

#[test]
fn a_tool_statement_inside_a_condition_is_not_a_typo_of_and() {
    // `send` is within edit distance 2 of `and`, but suggesting the word that stands right before is
    // nonsense: say what is wrong instead
    let e = stmt("guard: status != \"paid\" and email send customer");
    assert!(e.contains("I did not expect \"send\" here: \"email send\" looks like a statement"), "{e}");
    assert!(!e.contains("Did you mean"), "{e}");
    assert!(!e.contains("Expected \"and\""), "{e}");
}

#[test]
fn typos_of_and_or_still_get_did_you_mean() {
    for (src, want) in [
        ("guard: ready adn done", "and"),
        ("guard: ready nad done", "and"),
        ("guard: ready an done", "and"),
        ("guard: a or b orr c", "or"),
    ] {
        let e = stmt(src);
        assert!(e.contains(&format!("Did you mean \"{want}\"?")), "{src}:\n{e}");
        assert!(!e.contains("looks like a statement"), "{src}:\n{e}");
    }
    assert!(stmt("guard: status != \"paid\" and email send customer").contains("looks like a statement"));
}

// ---------------------------------------------------------------------------------------------
// v0.4 (SPEC 23.13): every removed form is an error that shows the new form.
// ---------------------------------------------------------------------------------------------

/// The error codes of a source, in order.
fn codes(src: &str) -> Vec<String> {
    match parse_file("t.agent.md", src) {
        Ok(_) => panic!("expected a syntax error in:\n{src}"),
        Err(d) => d.iter().map(|d| d.code.to_string()).collect(),
    }
}

/// One step of a small command, the way v0.3 would have written it.
fn cmd_step(step: &str) -> String {
    format!("# Command: c\nDoes things.\n\n- steps:\n  {step}\n")
}

#[test]
fn e060_run_and_start_headings_were_removed() {
    for h in ["# Run", "# Start"] {
        let src = format!("{h}\n- chaser\n");
        assert_eq!(codes(&src), ["E060"], "{h}");
        let out = render(&src);
        assert!(out.contains(&format!("\"{h}\" was removed")), "{out}");
        assert!(out.contains("# Program: <name>") && out.contains("- start: <command>"), "{out}");
    }
}

#[test]
fn e061_run_block_of_a_command_was_removed() {
    let src = "# Command: c\nDoes things.\n\n- in: p: text\n\n## Run\n1. stop\n";
    assert_eq!(codes(src), ["E061"]);
    let out = render(src);
    assert!(out.contains("\"## Run\" was removed"), "{out}");
    assert!(out.contains("under \"- steps:\" as a numbered list"), "{out}");
    assert!(out.contains("- steps:\n      1. print"), "{out}");
}

#[test]
fn e062_args_was_renamed_to_in_and_the_message_quotes_the_line() {
    let src = "# Command: c\nDoes things.\n\n- args: path: text, dry_run: bool = false\n- steps:\n  1. stop\n";
    assert_eq!(codes(src), ["E062"]);
    assert_eq!(
        render(src),
        "t.agent.md:4\n  - args: path: text, dry_run: bool = false\n    ^^^^\n  \"args:\" was renamed. Write: - in: path: text, dry_run: bool = false\n  A command's inputs are written like a function's."
    );
}

#[test]
fn e063_effect_is_removed_on_functions_tools_and_rust_blocks() {
    let function = "# Function: f\nDoes things.\n\n- out: int\n- effect: pure\n- steps:\n  1. return 1\n";
    assert_eq!(codes(function), ["E063"]);
    // `pure` suggests `touches: nothing`
    assert!(render(function).contains("\"effect:\" was removed. Write: - touches: nothing"), "{}", render(function));

    let tool = "# Tool: t\nDoes things.\n\n- effect: network\n\n## slug(title: text) -> text\n1. return title\n";
    assert_eq!(codes(tool), ["E063"]);
    assert!(render(tool).contains("- touches:"), "{}", render(tool));

    let rust = "# Function: f\nDoes things.\n\n- steps:\n  1. return g()\n\n## Rust: g -> text\n- effect: pure\n\n```rust\nString::new()\n```\n";
    assert_eq!(codes(rust), ["E063"]);

    // bindings keep `- effect:` (Q430)
    let binding = "# Binding: pdf\nRenders documents.\n\n- crate: printpdf\n- effect: unverified\n";
    assert!(parse_file("t.agent.md", binding).is_ok());
}

#[test]
fn e064_set_was_removed_and_the_message_quotes_the_new_form() {
    let src = cmd_step("1. set by_region = group sales by region");
    assert_eq!(codes(&src), ["E064"]);
    let out = render(&src);
    assert!(out.contains("\"set\" was removed. Write: group sales by region into by_region"), "{out}");
    assert!(render(&cmd_step("1. set x = 1")).contains("Write: 1 into x"));
}

#[test]
fn e065_arrow_naming_was_replaced_by_into() {
    for (old, new) in [
        ("file read path -> content", "file read path into content"),
        ("ask model with invoice: \"Remind\" -> reminder", "ask model with invoice: \"Remind\" into reminder"),
        ("do net_amount(invoice, 19) -> n", "do net_amount(invoice, 19) into n"),
        ("read body as Invoice -> inv", "read body as Invoice into inv"),
    ] {
        let src = cmd_step(&format!("1. {old}"));
        assert_eq!(codes(&src), ["E065"], "{old}");
        let out = render(&src);
        assert!(out.contains(&format!("\"->\" was replaced by \"into\". Write: {new}")), "{out}");
    }
}

#[test]
fn e065_does_not_fire_for_arrows_in_text_or_signatures() {
    // inside quoted text
    assert!(parse_file("t.agent.md", &cmd_step("1. print: \"a -> b\"")).is_ok());
    // in a Rust block heading and a tool verb heading ("returns")
    let rust = "# Function: f\nDoes things.\n\n- steps:\n  1. return g()\n\n## Rust: g -> text\n- touches: nothing\n\n```rust\nString::new()\n```\n";
    assert!(parse_file("t.agent.md", rust).is_ok());
    let tool = "# Tool: t\nDoes things.\n\n## slug(title: text) -> text\n1. return title\n";
    assert!(parse_file("t.agent.md", tool).is_ok());
}

#[test]
fn e066_append_was_renamed_to_add_with_and_without_a_record_literal() {
    let plain = cmd_step("1. append x to list");
    assert_eq!(codes(&plain), ["E066"]);
    assert!(render(&plain).contains("\"append\" was renamed. Write: add x to list"), "{}", render(&plain));
    let record = cmd_step("1. append RegionTotal { region: region } to totals");
    assert_eq!(codes(&record), ["E066"]);
    assert!(
        render(&record).contains("Write: add RegionTotal to totals: with one bullet per field under it"),
        "{}",
        render(&record)
    );
}

#[test]
fn e067_for_in_a_rule_moves_the_record_into_the_heading() {
    let src = "# Rule: big\nA big sale.\n\n- for: Sale\n- when: amount > 1 EUR\n";
    assert_eq!(codes(src), ["E067"]);
    let out = render(src);
    assert!(out.contains("\"for:\" was removed. Put the record last in the heading: # Rule: <name> Sale"), "{out}");
}

#[test]
fn e068_a_note_directly_above_a_heading_is_not_a_description() {
    for src in [
        "> A record.\n# Record: R\n- x: text\n",
        "# Agent: a\nDoes things.\n\n- model: claude\n\n> Runs on start.\n## On start\n1. stop\n",
    ] {
        assert_eq!(codes(src), ["E068"], "{src}");
        assert!(render(src).contains("A description goes under its heading as a plain paragraph, not in a \">\" line above it."));
    }
    // a note somewhere else, or at the top of the file apart from the first heading, is fine
    assert!(parse_file("t.agent.md", "> A note.\n\n# Record: R\n- x: text\n").is_ok());
    assert!(parse_file("t.agent.md", "# Record: R\nA record.\n\n> a note between\n- x: text\n").is_ok());
}

#[test]
fn e069_file_imports_moved_to_uses_and_crate_imports_stay() {
    let src = "# Import: ./records/invoice.agent.md\n";
    assert_eq!(codes(src), ["E069"]);
    assert!(render(src).contains("Write:\n    # Uses\n    - [invoice](./records/invoice.agent.md)"), "{}", render(src));
    let aliased = "# Import: ./billing as billing\n";
    assert_eq!(codes(aliased), ["E069"]);
    assert!(render(aliased).contains("- [billing](./billing) as billing"), "{}", render(aliased));
    assert!(parse_file("t.agent.md", "# Import: crate sha2\n").is_ok());
    assert!(parse_file("t.agent.md", "# Import: crate reqwest as http_client\n").is_ok());
}

#[test]
fn e070_a_step_written_as_a_bullet_shows_the_numbered_form() {
    let src = cmd_step("- print: \"x\"");
    assert_eq!(codes(&src), ["E070"]);
    assert!(render(&src).contains("Steps are a numbered list. Write: 1. print: \"x\""), "{}", render(&src));
    // in an agent block as well
    let agent = "# Agent: a\nDoes things.\n\n- model: claude\n\n## On start\n- log: \"hi\"\n";
    assert_eq!(codes(agent), ["E070"]);
}

#[test]
fn e071_a_property_written_as_a_numbered_item_is_an_error() {
    let src = "# Record: R\n1. x: text\n";
    assert_eq!(codes(src), ["E071"]);
    // (the agent then also lacks its `model` line)
    let src = "# Agent: a\nDoes things.\n\n1. model: claude\n";
    assert!(codes(src).iter().any(|c| c == "E071"));
}

#[test]
fn e072_program_must_be_the_first_heading() {
    let src = "# Record: R\n- x: text\n\n# Program: p\n";
    assert_eq!(codes(src), ["E072"]);
    assert!(render(src).contains("\"# Program\" must be the first heading of the file."));
    // twice
    let src = "# Program: p\n\n# Program: q\n";
    assert_eq!(codes(src), ["E072"]);
}

#[test]
fn e073_uses_after_a_declaration_or_with_an_entry_that_is_not_a_link() {
    let late = "# Record: R\n- x: text\n\n# Uses\n- [a](./a.agent.md)\n";
    assert_eq!(codes(late), ["E073"]);
    assert!(render(late).contains("\"# Uses\" goes at the top of the file"), "{}", render(late));
    let not_a_link = "# Uses\n- some text\n";
    assert_eq!(codes(not_a_link), ["E073"]);
    // after `# Program` is where it belongs
    assert!(parse_file("t.agent.md", "# Program: p\nDoes things.\n\n# Uses\n- [a](./a.agent.md)\n").is_ok());
}

#[test]
fn a_folder_in_uses_needs_a_module_name() {
    let src = "# Uses\n- [billing](./billing/)\n";
    let out = render(src);
    assert!(out.contains("To use a folder, give it a name: \"- [billing](./billing/) as <name>\"."), "{out}");
    assert!(parse_file("t.agent.md", "# Uses\n- [billing](./billing/) as billing\n").is_ok());
}

#[test]
fn removed_forms_do_not_hide_other_errors_in_the_same_file() {
    let src = "# Run\n- chaser\n\n# Command: c\nDoes things.\n\n- args: p: text\n- steps:\n  1. set x = 1\n";
    let c = codes(src);
    for want in ["E060", "E062", "E064"] {
        assert!(c.iter().any(|x| x == want), "{want} in {c:?}");
    }
}
