//! Tools written in AgentMark (SPEC 22.5): parsing, the checker's errors E380 to E386 and the effect rules
//! (a verb is checked like a function), formatting, the lint walkers and the IR.

mod common;
use common::*;

use agentmark::fmt::format_source;
use agentmark::project::Project;
use agentmark::sema::{self, BlockKind};

const SLUGGER: &str = "# Tool: slugger
- effect: pure

## slug(title: text) -> text
- set lowered = text.lower(title)
- return regex.replace(lowered, \"[^a-z0-9]+\", \"-\")
";

const LEDGER: &str = "# Tool: ledger
- effect: local

## add(entry: text)
- file append \"ledger.txt\": entry

## size(entry: text) -> int
- return text.length(entry)
";

/// `SLUGGER` and `LEDGER` followed by a command whose `## Run` is `body`.
fn with_command(body: &str) -> String {
    let lines: String = body.lines().map(|l| format!("{l}\n")).collect();
    format!("{SLUGGER}\n{LEDGER}\n# Command: c\n- args: title: text\n\n## Run\n{lines}")
}

/// The error codes of `src`, in the order they are reported.
fn codes(src: &str) -> Vec<&'static str> {
    let p = project(src);
    match sema::check(&p) {
        Ok(_) => panic!("expected errors in:\n{src}"),
        Err(d) => d.iter().filter(|x| x.severity == agentmark::diag::Severity::Error).map(|x| x.code).collect(),
    }
}

/// `src` has an error with `code` whose rendered text contains every piece of `pieces`.
fn expect(src: &str, code: &str, pieces: &[&str]) {
    let p = project(src);
    let Err(d) = sema::check(&p) else { panic!("expected errors in:\n{src}") };
    let mine: Vec<_> = d.iter().filter(|x| x.code == code).cloned().collect();
    assert!(!mine.is_empty(), "no {code} in {:?}:\n{}", codes(src), p.render(&d));
    let text = p.render(&mine);
    for piece in pieces {
        assert!(text.contains(piece), "{code} does not say {piece:?}:\n{text}");
    }
}

// ------------------------------------------------------------------ what is accepted

#[test]
fn a_tool_is_called_as_a_statement_and_a_pure_one_as_an_expression() {
    let c = ok(&with_command(
        "- set s = slugger.slug(title)\n- ledger add s\n- ledger size \"a b\" -> n\n- slugger slug title -> t\n- print: \"{n} {t}\"",
    ));
    // each verb is a block of its own, tagged as a tool verb
    let verbs: Vec<(&str, &str, String)> = c
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Tool)
        .map(|b| (b.unit.as_str(), b.label.as_str(), b.effect.to_string()))
        .collect();
    assert_eq!(
        verbs,
        [
            ("slugger", "slug", "pure".to_string()),
            ("ledger", "add", "local(file)".to_string()),
            ("ledger", "size", "pure".to_string()),
        ]
    );
    // the command that calls them has the effect of what they do, not of their names
    assert_eq!(c.unit_effect("c").to_string(), "local(file, ledger, print)");
}

#[test]
fn a_tool_that_declares_more_than_its_body_does_still_has_its_declared_effect_at_the_call() {
    let src = "# Tool: audit
- effect: network

## note(entry: text)
- print: entry

# Command: c
- args: title: text

## Run
- audit note title
";
    let c = ok(src);
    assert_eq!(c.unit_effect("c").to_string(), "network(audit), local(print)");
}

#[test]
fn a_library_module_name_still_means_the_library_unless_the_tool_has_that_verb() {
    // a tool called `text` with a verb `shout`: `text.lower` is the library's
    let src = "# Tool: text
- effect: pure

## shout(word: text) -> text
- return text.upper(word)

# Command: c
- args: title: text

## Run
- print: \"{text.lower(title)} {text.shout(title)}\"
";
    ok(src);
}

#[test]
fn a_pure_tool_of_an_imported_folder_is_called_with_its_module_name() {
    let p = Project::from_memory(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", "# Import: ./tools as tools\n\n# Command: c\n- args: title: text\n\n## Run\n- print: \"{tools.slugger.slug(title)}\"\n"),
            ("app/tools/slugger.agent.md", SLUGGER),
            ("app/tools/ledger.agent.md", LEDGER),
        ],
    );
    match sema::check(&p) {
        Ok(c) => assert!(c.blocks.iter().any(|b| b.kind == BlockKind::Tool && b.unit == "slugger")),
        Err(d) => panic!("{}", p.render(&d)),
    }
}

#[test]
fn an_agent_lists_a_user_tool_like_any_other() {
    let src = "# Tool: ledger
- effect: local

## add(entry: text)
- file append \"ledger.txt\": entry

# Agent: scribe
- model: claude
- tools: ledger

## On start
- ledger add \"started\"
";
    ok(src);
    let unlisted = src.replace("- tools: ledger", "- tools: log");
    expect(&unlisted, "E208", &["The tool \"ledger\" is not listed in the tools of agent \"scribe\"."]);
}

// ------------------------------------------------------------------ the new errors

#[test]
fn e380_a_tool_name_taken_by_tools_toml_or_file() {
    for (name, other) in [("email", "the tool \"email\" of tools.toml"), ("file", "the built-in file tool")] {
        let src = format!("# Tool: {name}\n- effect: pure\n\n## x() -> int\n- return 1\n");
        expect(&src, "E380", &[&format!("The tool \"{name}\" has the same name as {other}."), "^^^^"]);
    }
}

#[test]
fn e381_the_same_verb_twice() {
    let src = "# Tool: t\n- effect: pure\n\n## x() -> int\n- return 1\n\n## x() -> int\n- return 2\n";
    expect(src, "E381", &["The tool \"t\" has the verb \"x\" twice.", "t.agent.md:7"]);
}

#[test]
fn e382_an_expression_needs_a_pure_tool() {
    expect(
        &with_command("- print: \"{ledger.size(title)}\""),
        "E382",
        &[
            "The tool \"ledger\" has the effect local, so \"ledger.size(...)\" cannot be used inside an expression.",
            "Call it as a statement: \"- ledger size ... -> value\".",
        ],
    );
}

#[test]
fn e383_an_unknown_or_missing_verb_with_a_suggestion() {
    expect(&with_command("- slugger slog title -> t"), "E383", &["Unknown verb \"slog\" for tool slugger.", "Did you mean \"slug\"?"]);
    expect(&with_command("- print: \"{slugger.slog(title)}\""), "E383", &["Unknown verb \"slog\" for tool slugger.", "Did you mean \"slug\"?"]);
    expect(&with_command("- ledger"), "E383", &["The tool ledger needs a verb.", "Verbs of ledger: add, size."]);
}

#[test]
fn e384_a_tool_takes_values_not_assignments() {
    expect(&with_command("- ledger add: entry = title"), "E384", &["The tool \"ledger\" takes values, not \"field = value\" assignments."]);
}

#[test]
fn e385_a_verb_without_a_result_has_no_value() {
    expect(&with_command("- ledger add title -> r"), "E385", &["The verb \"add\" of \"ledger\" returns nothing, so there is no result to store."]);
    let src = "# Tool: t\n- effect: pure\n\n## x()\n- stop\n\n# Command: c\n\n## Run\n- print: \"{t.x()}\"\n";
    expect(src, "E385", &["The verb \"t.x\" returns nothing, so it cannot be used as a value."]);
}

#[test]
fn e386_a_verb_that_says_it_returns_a_value_must_return_one() {
    let src = "# Tool: t\n- effect: pure\n\n## x(a: int) -> int\n- set b = a + 1\n";
    expect(src, "E386", &["The verb \"x\" of the tool \"t\" says it returns a value but never does.", "t.agent.md:4"]);
}

#[test]
fn a_verb_body_may_not_exceed_the_effect_of_its_tool() {
    let src = "# Tool: t
- effect: pure

## write(entry: text)
- print: entry
";
    expect(
        src,
        "E206",
        &[
            "The tool \"t\" says \"- effect: pure\", but this line has the effect local (print).",
            "Change the tool to \"- effect: local\", or move this line out of the tool.",
            "t.agent.md:5",
        ],
    );
    // a tool statement inside a verb counts like any other statement
    let src = format!("{LEDGER}\n# Tool: pure_one\n- effect: pure\n\n## go(a: text)\n- ledger add a\n");
    expect(&src, "E206", &["has the effect local (file)"]);
}

#[test]
fn values_are_checked_against_the_parameters_of_the_verb() {
    expect(&with_command("- ledger add title title"), "E209", &["\"ledger add\" takes 1 value (entry), but 2 were given."]);
    expect(&with_command("- set n = 3\n- ledger add n"), "E203", &["\"entry\" of \"ledger add\" must be text, but this is a number."]);
}

#[test]
fn a_verb_that_can_fail_needs_try_and_cannot_be_used_in_an_expression() {
    let tool = "# Tool: conv\n- effect: pure\n\n## double(word: text) -> result of int\n- try number.parse(word) -> n\n- return n * 2\n\n";
    let bare = format!("{tool}# Command: c\n- args: w: text\n\n## Run\n- conv double w -> n\n- print: \"{{n}}\"\n");
    expect(&bare, "E209", &["can fail (it returns \"result of int\"), so it has to be called with \"try\"."]);
    let inline = format!("{tool}# Command: c\n- args: w: text\n\n## Run\n- print: \"{{conv.double(w)}}\"\n");
    expect(&inline, "E209", &["The verb \"conv.double\" can fail", "so it cannot be used inside an expression."]);
    let fine = format!("{tool}# Command: c\n- args: w: text\n\n## Run\n- try conv double w -> n\n- print: \"{{n}}\"\n");
    ok(&fine);
}

#[test]
fn parse_errors_name_what_is_missing() {
    let e = errors("# Tool: t\n\n## x()\n- stop\n");
    assert!(e.contains("The tool \"t\" does not say what effect it has."), "{e}");
    let e = errors("# Tool: t\n- effect: pure\n");
    assert!(e.contains("The tool \"t\" has no verbs."), "{e}");
    let e = errors("# Tool: t\n- effect: pure\n- model: claude\n\n## x()\n- stop\n");
    assert!(e.contains("model"), "{e}");
    let e = errors("# Tool: t\n- effect: pure\n\n## x(a: text) -> text\n- stop\n```rust\na\n```\n");
    assert!(e.contains("A verb is either statements or one fenced rust block, not both."), "{e}");
}

#[test]
fn an_unused_tool_is_a_warning_like_an_unused_function() {
    let w = warnings(SLUGGER);
    assert!(w.contains("Unused tool: nothing refers to \"slugger\"."), "{w}");
}

// ------------------------------------------------------------------ formatting and linting

const CANONICAL: &str = "> Words for file names.
# Tool: slugger
- effect: pure

> Lower case, dashes between words.
## slug(title: text) -> text
- set lowered = text.lower(title)
- return regex.replace(lowered, \"[^a-z0-9]+\", \"-\")

## shout(word: text) -> text
  ```rust
  word.to_uppercase()
  ```

> Adds a line.
# Tool: ledger
- effect: local

## add(entry: text, mark: text = \"-\")
- file append \"ledger.txt\": \"{mark} {entry}\"
";

#[test]
fn fmt_keeps_a_canonical_tool_and_makes_a_messy_one_canonical() {
    assert_eq!(format_source(CANONICAL, "t.agent.md").unwrap(), CANONICAL);
    let messy = "> Words for file names.\n#   Tool:   slugger\n-   effect:    pure\n\n\n> Lower case, dashes between words.\n##   slug( title: text )   ->   text\n-   set lowered = text.lower( title )\n-   return regex.replace( lowered,\"[^a-z0-9]+\",   \"-\" )\n\n## shout(word: text) -> text\n```rust\nword.to_uppercase()\n```\n\n> Adds a line.\n# Tool: ledger\n- effect: local\n## add(entry: text, mark: text = \"-\")\n- file append \"ledger.txt\": \"{mark} {entry}\"\n";
    let once = format_source(messy, "t.agent.md").unwrap();
    assert_eq!(format_source(&once, "t.agent.md").unwrap(), once, "fmt twice is fmt once");
    assert_eq!(once, CANONICAL);
}

#[test]
fn lint_looks_inside_the_verbs_of_a_tool() {
    let src = "# Tool: t
- effect: pure

## shape(data: text) -> text
- return data
";
    let diags = agentmark::lint::lint_source("t.agent.md", src, &agentmark::lint::LintConfig::default()).unwrap();
    assert!(diags.iter().any(|d| d.message.contains("data")), "{diags:#?}");
}

// ------------------------------------------------------------------ the IR

#[test]
fn every_verb_is_a_function_of_the_ir_and_shows_in_the_effect_summary() {
    let p = project(&with_command("- ledger add title\n- print: \"{slugger.slug(title)}\""));
    let c = sema::check(&p).unwrap_or_else(|d| panic!("{}", p.render(&d)));
    let prog = agentmark::ir::lower(&p, &c);
    let names: Vec<&str> = prog.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["slugger__slug", "ledger__add", "ledger__size"]);
    let summary: Vec<(String, String, String)> = agentmark::ir::effect_summary(&prog)
        .into_iter()
        .map(|(u, l, e)| (u, l, e.to_string()))
        .collect();
    assert!(summary.contains(&("ledger".into(), "add".into(), "local(file)".into())), "{summary:#?}");
    // a call is traced as a tool before the body runs
    let first = &prog.functions[1].body[0];
    assert!(matches!(
        &first.kind,
        agentmark::ir::StmtKind::Effect { call: agentmark::ir::EffectCall::Trace(agentmark::ir::TraceEvent::ToolCall { tool, verb, .. }), .. }
            if tool == "ledger" && verb == "add"
    ));
}

// ------------------------------------------------------------------ review fixes

#[test]
fn e387_the_default_command_cannot_have_an_argument_named_like_a_program_flag() {
    let one = "# Command: c\n- args: mock: bool = false, fixtures: text\n\n## Run\n- print: \"x\"\n\n# Run\n- c\n";
    expect(one, "E387", &["The argument \"mock\" would be the flag \"--mock\"", "Rename the argument"]);
    assert_eq!(codes(one), ["E387", "E387"]);
    // not the default (two commands): the names are subcommand arguments
    let two = "# Command: c\n- args: mock: bool = false\n\n## Run\n- print: \"x\"\n\n# Command: d\n\n## Run\n- print: \"y\"\n\n# Run\n- c\n- d\n";
    ok(two);
}

#[test]
fn check_lists_a_rust_verb_with_the_trusted_code() {
    let c = ok(&format!("{SLUGGER}\n# Tool: caps\n- effect: pure\n\n## shout(word: text) -> text\n```rust\nword.to_uppercase()\n```\n"));
    let r = c.rust_blocks.iter().find(|r| r.name == "caps.shout").expect("listed");
    assert_eq!(r.parent.as_deref(), Some("caps"));
    assert_eq!(r.sig.effect, agentmark::syntax::ast::Effect::Pure);
}

#[test]
fn a_tool_of_an_aliased_import_is_called_with_its_module_in_a_statement() {
    let files = vec![
        ("app/main.agent.md", "# Import: ./tools as tools\n\n# Command: c\n- args: title: text\n\n## Run\n- tools.ledger add title\n- tools.ledger size title -> n\n- print: \"{n}\"\n"),
        ("app/tools/ledger.agent.md", LEDGER),
    ];
    let p = Project::from_memory("app/main.agent.md", files.clone());
    let c = sema::check(&p).unwrap_or_else(|d| panic!("{}", p.render(&d)));
    assert_eq!(c.unit_effect("c").to_string(), "local(file, ledger, print)");
    // a typo in the module part is an unknown tool, with a suggestion
    let bad = files[0].1.replace("tools.ledger add", "tools.ledgr add");
    let p = Project::from_memory("app/main.agent.md", vec![("app/main.agent.md", bad.as_str()), files[1]]);
    let d = sema::check(&p).unwrap_err();
    let text = p.render(&d);
    assert!(text.contains("Unknown tool \"tools.ledgr\"") && text.contains("Did you mean \"tools.ledger\"?"), "{text}");
    // and it formats as written
    let src = "# Command: c\n- args: title: text\n\n## Run\n- tools.ledger add title\n";
    assert_eq!(format_source(src, "t.agent.md").unwrap(), src);
}

#[test]
fn a_tool_cannot_share_its_name_with_an_agent_command_or_service() {
    for (kind, decl) in [
        ("agent", "# Agent: ledger\n- model: claude\n\n## On start\n- stop\n"),
        ("command", "# Command: ledger\n\n## Run\n- stop\n"),
        ("service", "# Service: ledger\n- listen: 0.0.0.0:80\n\n## GET /x\n- respond 200\n"),
    ] {
        let src = format!("{LEDGER}\n{decl}");
        expect(&src, "E380", &[&format!("The tool \"ledger\" has the same name as the {kind} \"ledger\"")]);
    }
}

#[test]
fn a_tool_named_like_a_library_module_gets_a_warning() {
    let w = warnings("# Tool: json\n- effect: pure\n\n## shout(word: text) -> text\n- return text.upper(word)\n\n# Command: c\n- args: w: text\n\n## Run\n- print: \"{json.shout(w)}\"\n");
    assert!(w.contains("The tool \"json\" has the name of a library module"), "{w}");
}

const MINI_HASH: &str = "# Binding: sha2
- crate: sha2
- effect: pure

## sha256_new() -> handle of sha2.Sha256
  ```rust
  Ok(<sha2::Sha256 as sha2::Digest>::new())
  ```

## finalize(self: consume handle of sha2.Sha256) -> bytes
  ```rust
  Ok(<sha2::Sha256 as sha2::Digest>::finalize(self_).to_vec())
  ```
";

#[test]
fn a_verb_may_take_a_handle_it_uses_up_and_the_trace_leaves_it_out() {
    let tool = "# Import: crate sha2\n\n# Tool: hashing\n- effect: pure\n\n## drain(hasher: handle of sha2.Sha256, label: text) -> bytes\n- hasher finalize -> digest\n- return digest\n\n";
    let cmd = |body: &str| format!("{tool}# Command: c\n- args: w: text\n\n## Run\n{body}");
    let load = |src: &str| {
        Project::from_memory(
            "app/main.agent.md",
            vec![("app/main.agent.md", src), ("app/bindings/sha2.agent.md", MINI_HASH), ("app/tools.toml", "")],
        )
    };
    let good = cmd("- sha2 sha256_new -> h\n- hashing drain h w -> d\n- print: \"done\"\n");
    let p = load(&good);
    let c = sema::check(&p).unwrap_or_else(|d| panic!("{}", p.render(&d)));
    let prog = agentmark::ir::lower(&p, &c);
    let f = prog.functions.iter().find(|f| f.name == "hashing__drain").unwrap();
    match &f.body[0].kind {
        agentmark::ir::StmtKind::Effect { call: agentmark::ir::EffectCall::Trace(agentmark::ir::TraceEvent::ToolCall { args, .. }), .. } => {
            assert_eq!(args.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["label"], "a handle is not traced");
        }
        other => panic!("{other:?}"),
    }
    // passing the handle to the verb used it up
    let bad = cmd("- sha2 sha256_new -> h\n- hashing drain h w -> d\n- hashing drain h w -> e\n");
    let p = load(&bad);
    let d = sema::check(&p).unwrap_err();
    let text = p.render(&d);
    assert!(text.contains("\"h\" was used up on line"), "{text}");
}
