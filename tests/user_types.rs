//! User-defined types (SPEC 22.1, step 20): `# Enum`, `# Type` aliases and `match`, through every
//! pass: parser, semantic check, formatter, lint, IR and the generated program.
//!
//! The shared program is `tests/user_types/shapes.agent.md`.

mod common;
mod support;

use std::path::Path;
use std::process::Command;

use serde_json::json;

use agentmark::diag::render_all;
use agentmark::fmt::{format_source, is_canonical};
use agentmark::ir::{self, ExprKind as IX, Pattern, StmtKind as IS};
use agentmark::project::Project;
use agentmark::sema::{self, Type};
use agentmark::syntax::ast::*;
use agentmark::syntax::parser::parse_file;

use support::*;

const SHAPES: &str = include_str!("user_types/shapes.agent.md");

const SH: &str = "# Enum: Shape\n- Circle: radius: int\n- Rect: width: int, height: int\n- Empty\n\n";

fn parse(src: &str) -> File {
    parse_file("t.agent.md", src).unwrap_or_else(|d| panic!("did not parse:\n{}", render_all(&d, src)))
}

fn parse_errors(src: &str) -> String {
    match parse_file("t.agent.md", src) {
        Ok(_) => panic!("expected a syntax error in:\n{src}"),
        Err(d) => render_all(&d, src),
    }
}

/// A function body wrapped around `lines` (each already indented by two spaces).
fn func(pre: &str, params: &str, effect: &str, body: &str) -> String {
    format!("{pre}# Function: f\n- in: {params}\n- effect: {effect}\n{body}")
}

fn use_it(src: &str) -> String {
    // a command that calls `f`, so the check has no unused-declaration warning to talk about
    format!("{src}\n# Command: go\n\n## Run\n- stop\n")
}

// ---------------------------------------------------------------------------------- parsing

#[test]
fn an_enum_lists_variants_with_and_without_fields() {
    let f = parse(SH);
    let Decl::Enum(e) = &f.decls[0] else { panic!("{:?}", f.decls[0]) };
    assert_eq!(e.name.name, "Shape");
    let names: Vec<&str> = e.variants.iter().map(|v| v.name.name.as_str()).collect();
    assert_eq!(names, ["Circle", "Rect", "Empty"]);
    let rect = &e.variants[1];
    assert_eq!(rect.fields.len(), 2);
    assert_eq!(rect.fields[1].name.name, "height");
    assert!(matches!(&rect.fields[1].ty, Some(t) if t.kind == TypeKind::Named("int".into())));
    assert!(e.variants[2].fields.is_empty());
}

#[test]
fn a_type_alias_names_any_type() {
    let f = parse("# Type: Totals\n- is: list of maybe int\n");
    let Decl::TypeAlias(t) = &f.decls[0] else { panic!() };
    assert_eq!(t.name.name, "Totals");
    assert!(matches!(&t.target.kind, TypeKind::List(i) if matches!(i.kind, TypeKind::Maybe(_))));
}

#[test]
fn match_branches_keep_their_order_qualifier_and_binding() {
    let src = func(
        SH,
        "shape: Shape",
        "pure",
        "  - match shape:\n    - Circle c:\n      - stop\n    - Shape.Empty:\n      - stop\n    - otherwise:\n      - stop\n",
    );
    let f = parse(&src);
    let Decl::Function(func) = &f.decls[1] else { panic!() };
    let StmtKind::Match { scrutinee, arms, otherwise } = &func.body[0].kind else { panic!("{:?}", func.body) };
    assert!(matches!(&scrutinee.kind, ExprKind::Path(p) if p[0].name == "shape"));
    assert_eq!(arms.len(), 2);
    assert_eq!(arms[0].pattern.name, "Circle");
    assert_eq!(arms[0].bind.as_ref().unwrap().name, "c");
    assert!(arms[0].qualifier.is_none());
    assert_eq!(arms[1].qualifier.as_ref().unwrap().name, "Shape");
    assert_eq!(arms[1].pattern.name, "Empty");
    assert!(arms[1].bind.is_none());
    assert!(otherwise.is_some());
}

#[test]
fn variant_values_are_a_name_with_braces_or_a_plain_path() {
    let src = func(
        SH,
        "n: int",
        "pure",
        "  - set a = Circle { radius: 3 }\n  - set b = Shape.Rect { width: 1, height: n }\n  - set c = Shape.Empty\n  - set d = Empty\n",
    );
    let f = parse(&src);
    let Decl::Function(func) = &f.decls[1] else { panic!() };
    let value = |i: usize| match &func.body[i].kind {
        StmtKind::Set { expr, .. } => expr.kind.clone(),
        k => panic!("{k:?}"),
    };
    let ExprKind::Variant { qualifier, name, fields } = value(0) else { panic!() };
    assert!(qualifier.is_none());
    assert_eq!((name.name.as_str(), fields.len()), ("Circle", 1));
    let ExprKind::Variant { qualifier, name, fields } = value(1) else { panic!() };
    assert_eq!(qualifier.unwrap().name, "Shape");
    assert_eq!((name.name.as_str(), fields.len()), ("Rect", 2));
    assert!(matches!(value(2), ExprKind::Path(p) if p.len() == 2));
    assert!(matches!(value(3), ExprKind::Path(p) if p.len() == 1));
}

#[test]
fn the_fixture_parses_to_a_stable_tree() {
    let f = parse_file("tests/user_types/shapes.agent.md", SHAPES).unwrap();
    insta::with_settings!({ snapshot_path => "snapshots" }, {
        insta::assert_yaml_snapshot!("user_types__shapes", f);
    });
}

#[test]
fn syntax_mistakes_in_the_new_forms_say_what_to_write() {
    let body = |m: &str| func(SH, "s: Shape", "local", &format!("  - {m}\n"));
    let e = parse_errors(&body("match s\n    - Empty:\n      - stop"));
    assert!(e.contains("This line is missing the \":\" at its end."), "{e}");
    let e = parse_errors(&body("match s:\n  - stop"));
    assert!(e.contains("A \"match\" needs branches indented under it"), "{e}");
    let e = parse_errors(&body("match s:\n    - otherwise:\n      - stop\n    - Empty:\n      - stop"));
    assert!(e.contains("\"otherwise:\" must be the last branch of a \"match\"."), "{e}");
    let e = parse_errors(&body("match s:\n    - Empty:\n      - stop\n    - otherwise:\n      - stop\n    - otherwise:\n      - stop"));
    assert!(e.contains("A \"match\" has only one \"otherwise:\" branch."), "{e}");
    let e = parse_errors(&body("match s:\n    - Empty\n      - stop"));
    assert!(e.contains("expected \":\""), "{e}");
    let e = parse_errors("# Enum: Shape\n- Circle: radius\n");
    assert!(e.contains("expected \":\""), "{e}");
    let e = parse_errors("# Type: Totals\n");
    assert!(e.contains("The type \"Totals\" has no \"is\" line."), "{e}");
    let e = parse_errors("# Enum: Shape\n- A\n\n## Block\n");
    assert!(e.contains("An Enum has no \"##\" blocks."), "{e}");
}

// ---------------------------------------------------------------------------------- semantics

fn sema_errors(src: &str) -> String {
    common::errors(src)
}

#[test]
fn the_fixture_checks_with_no_warnings() {
    let c = common::ok(SHAPES);
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    assert_eq!(c.enums.keys().cloned().collect::<Vec<_>>(), ["Menu", "Shape"]);
    assert_eq!(c.enums["Shape"].variants[1].fields[1].ty, Type::Int);
    assert_eq!(c.aliases["Entries"].ty.to_string(), "list of Entry");
}

#[test]
fn an_alias_is_the_type_it_names() {
    let c = common::ok(&use_it(
        "# Type: Names\n- is: list of text\n\n# Function: f\n- in: a: Names\n- out: list of text\n- effect: pure\n  - return a\n",
    ));
    let f = &c.functions["f"];
    assert_eq!(f.params[0].ty.to_string(), "list of text");
    assert_eq!(f.out.as_ref().unwrap(), &f.params[0].ty);
}

#[test]
fn a_record_may_hold_an_enum_and_an_alias_of_one() {
    let src = format!("{SH}# Type: Pic\n- is: Shape\n\n# Record: Slide\n- title: text\n- shape: Shape\n- other: maybe Pic\n");
    let c = common::ok(&use_it(&src));
    assert_eq!(c.records["Slide"].fields[1].ty, Type::Enum("Shape".into()));
    assert_eq!(c.records["Slide"].fields[2].ty.to_string(), "maybe Shape");
}

#[test]
fn an_enum_cannot_hold_itself_without_a_list() {
    let e = sema_errors("# Enum: Tree\n- Leaf\n- Node: left: Tree, right: Tree\n");
    assert!(e.contains("The variant \"Node\" of Tree holds a Tree inside itself, which would never end."), "{e}");
    assert!(e.contains("Keep the children in a list instead, like \"left: list of Tree\"."), "{e}");
    // through a maybe, a record or another enum
    let e = sema_errors("# Enum: Tree\n- Leaf\n- Node: next: maybe Tree\n");
    assert!(e.contains("holds a Tree inside itself"), "{e}");
    let e = sema_errors("# Record: Box\n- tree: Tree\n\n# Enum: Tree\n- Leaf\n- Node: inner: Box\n");
    assert!(e.contains("holds a Tree inside itself"), "{e}");
    let e = sema_errors("# Enum: A\n- X: b: B\n\n# Enum: B\n- Y: a: A\n");
    assert!(e.contains("The variant \"X\" of A holds a A inside itself") && e.contains("The variant \"Y\" of B holds a B inside itself"), "{e}");
    // a list keeps its elements on the heap
    common::ok(&use_it("# Enum: Tree\n- Leaf\n- Node: children: list of Tree\n\n# Function: f\n- in: t: Tree\n- out: int\n- effect: pure\n  - return 0\n"));
}

#[test]
fn type_declarations_that_make_no_sense_are_errors() {
    let e = sema_errors("# Record: Shape\n- id: text\n\n# Enum: Shape\n- A\n");
    assert!(e.contains("The enum \"Shape\" has the same name as the record in t.agent.md:1."), "{e}");
    let e = sema_errors("# Enum: Shape\n");
    assert!(e.contains("The enum \"Shape\" has no variants."), "{e}");
    let e = sema_errors("# Enum: Shape\n- A\n- A\n");
    assert!(e.contains("The enum \"Shape\" has the variant \"A\" twice."), "{e}");
    let e = sema_errors("# Enum: Shape\n- A: x: int, x: int\n");
    assert!(e.contains("The variant \"A\" has the field \"x\" twice."), "{e}");
    let e = sema_errors("# Enum: Shape\n- A: kind: text\n");
    assert!(e.contains("A variant cannot have a field called \"kind\""), "{e}");
    let e = sema_errors("# Enum: Shape\n- A: x: Shap\n");
    assert!(e.contains("Unknown type \"Shap\". Did you mean \"Shape\"?"), "{e}");
    let e = sema_errors("# Type: A\n- is: B\n\n# Type: B\n- is: list of A\n");
    assert!(e.contains("The type \"A\" is defined in terms of itself."), "{e}");
}

fn in_function(body: &str) -> String {
    func(SH, "s: Shape", "local", body)
}

#[test]
fn a_variant_value_names_the_mistake() {
    let f = |expr: &str| format!("{SH}# Function: f\n- out: Shape\n- effect: pure\n  - return {expr}\n");
    let e = sema_errors(&f("Circel { radius: 1 }"));
    assert!(e.contains("Unknown variant \"Circel\". Did you mean \"Circle\"?"), "{e}");
    let e = sema_errors(&f("Emtpy"));
    assert!(e.contains("Unknown name \"Emtpy\". Did you mean \"Empty\"?"), "{e}");
    let e = sema_errors(&f("Shape.Emtpy"));
    assert!(e.contains("Shape has no variant \"Emtpy\". Did you mean \"Empty\"?"), "{e}");
    let e = sema_errors(&f("Shap.Empty"));
    assert!(e.contains("Unknown name \"Shap\". Did you mean \"Shape\"?"), "{e}");
    let e = sema_errors(&f("Rect { width: 1 }"));
    assert!(e.contains("The variant \"Rect\" needs a value for: \"height\"."), "{e}");
    let e = sema_errors(&f("Rect { height: 1 }"));
    assert!(e.contains("The variant \"Rect\" needs a value for: \"width\"."), "{e}");
    let e = sema_errors(&f("Circle { radious: 1 }"));
    assert!(e.contains("The variant \"Circle\" has no field \"radious\". Did you mean \"radius\"?"), "{e}");
    assert!(!e.contains("needs a value"), "one typo, one error:\n{e}");
    let e = sema_errors(&f("Circle { radius: \"x\" }"));
    assert!(e.contains("The field \"radius\" of Circle is int, but this is text."), "{e}");
    let e = sema_errors(&f("Circle"));
    assert!(e.contains("The variant \"Circle\" of Shape has fields, so it is written with them."), "{e}");
    assert!(e.contains("Write \"Circle { radius: … }\"."), "{e}");
    let e = sema_errors(&f("Empty { x: 1 }"));
    assert!(e.contains("The variant \"Empty\" has no fields."), "{e}");
    let e = sema_errors(&f("Circle { radius: 1, radius: 2 }"));
    assert!(e.contains("The field \"radius\" is given twice."), "{e}");
    // a value of the wrong type for the function's `out`
    let e = sema_errors(&format!("{SH}# Function: f\n- out: int\n- effect: pure\n  - return Shape.Empty\n"));
    assert!(e.contains("This returns Shape, but the function says it returns \"int\"."), "{e}");
}

#[test]
fn a_record_is_built_field_by_field() {
    let rec = "# Record: Sale\n- region: text\n- amount: money\n- note: maybe text\n\n";
    let f = |expr: &str| format!("{rec}# Function: f\n- out: Sale\n- effect: pure\n  - return {expr}\n");
    let c = common::ok(&use_it(&f("Sale { region: \"EU\", amount: 120.00 EUR }")));
    assert!(c.expr_types.values().any(|t| *t == Type::Record("Sale".into())));
    let e = sema_errors(&f("Sale { region: \"EU\" }"));
    assert!(e.contains("The record \"Sale\" needs a value for: \"amount\"."), "{e}");
    let e = sema_errors(&f("Sale { regoin: \"EU\", amount: 1 EUR }"));
    assert!(e.contains("The record \"Sale\" has no field \"regoin\". Did you mean \"region\"?"), "{e}");
    let e = sema_errors(&f("Sale { region: 1, amount: 1 EUR }"));
    assert!(e.contains("The field \"region\" of Sale is text, but this is a number."), "{e}");
    let e = sema_errors(&f("Sal { region: \"EU\" }"));
    assert!(e.contains("Unknown variant \"Sal\". Did you mean \"Sale\"?"), "{e}");
}

#[test]
fn maybe_values_are_written_some_and_none() {
    let src = "# Function: pick\n- in: n: int\n- out: maybe int\n- effect: pure\n  - if n > 0:\n    - return some n\n  - return none\n\n# Function: show\n- in: n: int\n- out: text\n- effect: pure\n  - if pick(n) is some v:\n    - return \"got {v}\"\n  - return \"nothing\"\n";
    let c = common::ok(&use_it(src));
    assert_eq!(c.functions["pick"].out.as_ref().unwrap().to_string(), "maybe int");
    assert!(is_canonical(src, "t.agent.md").unwrap());
    let e = sema_errors("# Function: f\n- out: maybe int\n- effect: pure\n  - return some \"x\"\n");
    assert!(e.contains("This returns"), "{e}");
    let (out, _) = run_command(
        "maybes",
        &format!("{src}\n# Command: demo\n- args: n: int\n\n## Run\n- print: show(n)\n- print: show(0)\n"),
        &[],
        json!({"n": 4}),
    );
    assert_eq!(out, "got 4\nnothing\n");
}

#[test]
fn try_works_with_and_without_do_and_branch_names_may_be_left_out() {
    let base = "# Function: half\n- in: n: int\n- out: result of int\n- effect: pure\n  - return n / 2\n\n";
    for call in ["try half(n) -> h", "try do half(n) -> h"] {
        let src = format!("{base}# Function: f\n- in: n: int\n- out: int\n- effect: pure\n  - {call}\n    otherwise:\n      - set h = 0\n  - return h\n");
        let c = common::ok(&use_it(&src));
        assert!(c.functions.contains_key("f"));
        // the formatter keeps the author's spelling
        assert_eq!(format_source(&src, "t.agent.md").unwrap(), src);
    }
    // a result that is not used: `try f(x)` with nothing after it
    for call in ["try half(n)", "try do half(n)"] {
        let src = format!("{base}# Function: f\n- in: n: int\n- effect: pure\n  - {call}\n");
        common::ok(&use_it(&src));
        assert_eq!(format_source(&src, "t.agent.md").unwrap(), src);
    }
    let src = format!("{base}# Function: g\n- effect: local\n  - match half(4):\n    - ok:\n      - print: \"fine\"\n    - error:\n      - print: \"broken\"\n\n# Enum: Shape\n- Circle: radius: int\n- Empty\n\n# Function: h\n- in: s: Shape\n- effect: local\n  - match s:\n    - Circle:\n      - print: \"round\"\n    - Empty:\n      - print: \"none\"\n");
    common::ok(&use_it(&src));
    // `some:` needs no name either
    common::ok(&use_it("# Function: f\n- in: n: maybe int\n- effect: local\n  - match n:\n    - some:\n      - print: \"x\"\n    - none:\n      - print: \"y\"\n"));
}

const STORED: &str = "# Function: read_count\n- in: path: text\n- out: result of int\n- effect: local\n  - try read file path as int -> n\n  - return n\n\n";

#[test]
fn a_result_can_be_stored_and_matched() {
    let src = format!("{STORED}> Reads a count.\n# Command: demo\n- args: path: text\n\n## Run\n- set parsed = read_count(path)\n- match parsed:\n  - ok total:\n    - print: \"count {{total}}\"\n  - error:\n    - print: \"unreadable\"\n");
    let c = common::ok(&src);
    assert!(c.expr_types.values().any(|t| matches!(t, Type::Result(_))));
    assert!(is_canonical(&src, "t.agent.md").unwrap());
    let d = temp("stored-result");
    write(&d, "n.json", "41");
    let (out, _) = run_command("stored-ok", &src, &[], json!({"path": s(&d.join("n.json"))}));
    assert_eq!(out, "count 41\n");
    let (out, _) = run_command("stored-bad", &src, &[], json!({"path": s(&d.join("missing.json"))}));
    assert_eq!(out, "unreadable\n");
}

#[test]
fn a_stored_result_can_only_be_matched_once_and_nowhere_else() {
    let head = format!("{STORED}# Command: demo\n- args: path: text\n\n## Run\n- set parsed = read_count(path)\n");
    let e = sema_errors(&format!("{head}- set again = parsed\n- print: \"x\"\n"));
    assert!(e.contains("\"parsed\" holds a result, which can only be matched."), "{e}");
    assert!(!e.contains("works on an enum"), "no cascade:\n{e}");
    let e = sema_errors(&format!("{head}- print: \"{{parsed}}\"\n"));
    assert!(e.contains("holds a result, which can only be matched."), "{e}");
    let twice = "- match parsed:\n  - ok v:\n    - print: \"a\"\n  - error:\n    - print: \"b\"\n";
    let e = sema_errors(&format!("{head}{twice}{twice}"));
    assert!(e.contains("\"parsed\" was used up on line"), "{e}");
    let e = sema_errors(&format!("{head}- match parsed:\n  - ok v:\n    - print: \"a\"\n"));
    assert!(e.contains("does not handle the case \"error\" of a \"result\"."), "{e}");
}

#[test]
fn a_none_that_nothing_types_is_an_error() {
    let e = sema_errors("# Function: f\n- effect: local\n  - set d = none\n  - print: \"x\"\n");
    assert!(e.contains("This \"none\" does not say what it is a \"none\" of."), "{e}");
    let e = sema_errors("# Function: f\n- effect: local\n  - print: \"{ a: none }\"\n");
    assert!(!e.is_empty());
    common::ok(&use_it("# Function: f\n- out: maybe int\n- effect: pure\n  - return none\n"));
}

#[test]
fn a_command_without_arguments_runs() {
    let src = "> Says hi.\n# Command: demo\n\n## Run\n- print: \"hi\"\n";
    let (out, _) = run_command("noargs", src, &[], json!({}));
    assert_eq!(out, "hi\n");
}

#[test]
fn a_record_named_like_a_rust_type_still_builds_warning_free() {
    let src = "# Record: Box\n- tree: text\n\n# Function: word_of\n- in: s: Tree\n- out: text\n- effect: pure\n  - set word = \"angular\"\n  - match s:\n    - Leaf:\n      - set word = \"leaf\"\n    - otherwise:\n      - set word = \"node\"\n  - return word\n\n# Enum: Tree\n- Leaf\n- Node: inner: Box\n\n> Serves a box.\n# Service: boxes\n- listen: 127.0.0.1:8098\n\n## GET /box\n- respond 200 { node: Node { inner: Box { tree: \"oak\" } }, word: word_of(Leaf) }\n";
    let d = temp("boxed");
    write(&d, "boxes.agent.md", src);
    let out = d.join("out");
    let o = agentmark(&["build", s(&d.join("boxes.agent.md")), "-o", s(&out), "--no-check"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    build_bin(&out, "boxes");
}

#[test]
fn a_variant_name_that_two_enums_have_needs_its_enum() {
    let both = format!("{SH}# Enum: Menu\n- Empty\n- Item: name: text\n\n");
    let src = |expr: &str| format!("{both}# Function: f\n- out: Shape\n- effect: pure\n  - return {expr}\n");
    let e = sema_errors(&src("Empty"));
    assert!(e.contains("\"Empty\" is a variant of several enums here: Menu.Empty, Shape.Empty."), "{e}");
    assert!(e.contains("Say which one you mean, for example \"Menu.Empty\"."), "{e}");
    common::ok(&use_it(&src("Shape.Empty")));
    // names that only one enum has stay bare
    common::ok(&use_it(&format!("{both}# Function: f\n- out: Menu\n- effect: pure\n  - return Item {{ name: \"tea\" }}\n")));
}

#[test]
fn variants_are_found_through_module_aliases() {
    let p = Project::from_memory(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", "# Import: ./shape.agent.md as geo\n\n# Command: go\n\n## Run\n- set a = geo.Shape.Empty\n- set b = geo.Circle { radius: 1 }\n- set c = geo.Empty\n- match c:\n  - Empty:\n    - print: \"empty\"\n  - Circle k:\n    - print: \"{k.radius}\"\n  - Rect r:\n    - print: \"{r.width}\"\n- print: \"done\"\n"),
            ("app/shape.agent.md", SH),
        ],
    );
    match sema::check(&p) {
        Ok(c) => assert!(c.warnings.iter().all(|w| w.code != "W002"), "{:?}", c.warnings),
        Err(d) => panic!("{}", p.render(&d)),
    }
}

#[test]
fn a_variable_or_constant_wins_over_a_variant_of_the_same_name() {
    let src = format!("{SH}# Constant: Empty\n- value: 7\n\n# Function: f\n- out: int\n- effect: pure\n  - return Empty\n");
    common::ok(&use_it(&src));
}

#[test]
fn match_needs_an_enum_a_maybe_or_a_result() {
    let e = sema_errors(&func("", "n: int", "local", "  - match n:\n    - Empty:\n      - print: \"x\"\n"));
    assert!(e.contains("\"match\" works on an enum, a \"maybe\" or a \"result\", but this is a number."), "{e}");
    // a result is made by a call: it cannot be passed in
    let src = "# Function: f\n- in: r: result of int\n- effect: pure\n  - stop\n";
    let e = sema_errors(src);
    assert!(e.contains("A result is made by a call and matched where it is stored; it cannot be passed in."), "{e}");
}

#[test]
fn an_unhandled_variant_is_named() {
    let e = sema_errors(&in_function("  - match s:\n    - Circle c:\n      - print: \"c\"\n    - Rect r:\n      - print: \"r\"\n"));
    assert!(
        e.contains("    - match s:\n      ^^^^^^^^\n  This \"match\" does not handle the variant \"Empty\" of Shape.\n  Add a branch for it (\"- Empty:\"), or an \"- otherwise:\" branch for everything else."),
        "{e}"
    );
    let e = sema_errors(&in_function("  - match s:\n    - Circle c:\n      - print: \"c\"\n"));
    assert!(e.contains("does not handle the variants \"Rect\" and \"Empty\" of Shape."), "{e}");
    let e = sema_errors(&func("", "n: maybe int", "local", "  - match n:\n    - some v:\n      - print: \"x\"\n"));
    assert!(e.contains("does not handle the case \"none\" of a \"maybe\"."), "{e}");
    let src = "# Function: g\n- out: result of int\n- effect: pure\n  - return 1\n\n# Function: f\n- effect: local\n  - match g():\n    - ok v:\n      - print: \"x\"\n";
    let e = sema_errors(src);
    assert!(e.contains("does not handle the case \"error\" of a \"result\"."), "{e}");
}

#[test]
fn otherwise_covers_the_rest_and_an_unreachable_one_is_a_warning() {
    let covered = in_function("  - match s:\n    - Circle c:\n      - print: \"c\"\n    - otherwise:\n      - print: \"other\"\n");
    common::ok(&use_it(&covered));
    let all = in_function("  - match s:\n    - Circle c:\n      - print: \"c\"\n    - Rect r:\n      - print: \"r\"\n    - Empty:\n      - print: \"e\"\n    - otherwise:\n      - print: \"x\"\n");
    let w = common::warnings(&use_it(&all));
    assert!(w.contains("Every case is handled above, so this \"otherwise:\" can never run."), "{w}");
}

#[test]
fn a_branch_that_does_not_fit_is_named() {
    let e = sema_errors(&in_function("  - match s:\n    - Circel c:\n      - print: \"c\"\n    - Rect r:\n      - print: \"r\"\n    - Empty:\n      - print: \"e\"\n"));
    assert!(e.contains("Shape has no variant \"Circel\". Did you mean \"Circle\"?"), "{e}");
    assert!(e.contains("The variants of Shape are: Circle, Rect, Empty."), "{e}");
    assert!(!e.contains("does not handle"), "a misspelled branch is one error:\n{e}");
    let two = format!("{SH}# Enum: Menu\n- Item: name: text\n\n");
    let e = sema_errors(&func(&two, "s: Shape", "local", "  - match s:\n    - Item i:\n      - print: \"c\"\n    - otherwise:\n      - print: \"e\"\n"));
    assert!(e.contains("\"Item\" belongs to Menu, but this \"match\" is on Shape."), "{e}");
    let e = sema_errors(&func(&two, "s: Shape", "local", "  - match s:\n    - Menu.Empty:\n      - print: \"c\"\n    - otherwise:\n      - print: \"e\"\n"));
    assert!(e.contains("This \"match\" is on Shape, but the branch names \"Menu\"."), "{e}");
    let e = sema_errors(&in_function("  - match s:\n    - Empty:\n      - print: \"c\"\n    - Empty:\n      - print: \"r\"\n    - otherwise:\n      - print: \"e\"\n"));
    assert!(e.contains("This \"match\" already has a branch for \"Empty\"."), "{e}");
    let e = sema_errors(&func("", "n: maybe int", "local", "  - match n:\n    - ok v:\n      - print: \"x\"\n    - none:\n      - print: \"y\"\n"));
    assert!(e.contains("\"ok\" is not a branch of a \"maybe\"."), "{e}");
    assert!(e.contains("The branches of a \"maybe\" are: some, none."), "{e}");
}

#[test]
fn a_branch_names_its_value_only_when_it_has_one() {
    let e = sema_errors(&in_function("  - match s:\n    - Circle c:\n      - print: \"c\"\n    - Rect r:\n      - print: \"r\"\n    - Empty e:\n      - print: \"e\"\n"));
    assert!(e.contains("The variant \"Empty\" has no fields, so there is nothing to name \"e\"."), "{e}");
    let e = sema_errors(&func("", "n: maybe int", "local", "  - match n:\n    - some v:\n      - print: \"x\"\n    - none x:\n      - print: \"y\"\n"));
    assert!(e.contains("\"none\" carries no value, so there is nothing to name \"x\"."), "{e}");
}

#[test]
fn a_branch_name_gives_the_fields_of_its_variant_and_nothing_else() {
    let e = sema_errors(&in_function("  - match s:\n    - Circle c:\n      - print: \"{c.radious}\"\n    - otherwise:\n      - stop\n"));
    assert!(e.contains("The variant \"Circle\" has no field \"radious\". Did you mean \"radius\"?"), "{e}");
    // the whole payload is not an enum value
    let src = format!("{SH}# Function: f\n- in: s: Shape\n- out: Shape\n- effect: pure\n  - match s:\n    - Circle c:\n      - return c\n    - otherwise:\n      - return s\n");
    let e = sema_errors(&src);
    assert!(e.contains("\"c\" stands for the fields of Circle, not for a value of Shape."), "{e}");
    assert!(e.contains("Read a field, like c.radius."), "{e}");
    let e = sema_errors(&in_function("  - print: \"{s}\"\n"));
    assert!(e.contains("A Shape cannot be written into text as a whole."), "{e}");
    let e = sema_errors(&in_function("  - print: \"{s.radius}\"\n"));
    assert!(e.contains("\"s\" is a Shape, which has no fields of its own, so \".radius\" cannot be read."), "{e}");
    assert!(e.contains("Use \"- match s:\" and read the fields inside the branch"), "{e}");
}

#[test]
fn the_names_of_a_matched_maybe_and_result_have_the_inner_types() {
    let src = "# Function: g\n- out: result of int\n- effect: pure\n  - return 1\n\n# Function: f\n- in: n: maybe money\n- out: money\n- effect: local\n  - match g():\n    - ok v:\n      - print: \"{v + 1}\"\n    - error e:\n      - print: \"{e}\"\n  - match n:\n    - some m:\n      - return m + 1 EUR\n    - none:\n      - return 0 EUR\n";
    let c = common::ok(&use_it(src));
    assert!(c.patterns.values().any(|p| *p == sema::Pattern::Error));
    assert!(c.patterns.values().any(|p| *p == sema::Pattern::Some));
}

#[test]
fn match_counts_for_the_hard_limits_as_one_level_and_a_branch_may_be_empty() {
    // for each -> if -> match -> if is 4 levels deep only when the match opens two; it opens one
    let body = "  - for each x in items:\n    - if x > 0:\n      - match shape:\n        - Empty:\n          - print: \"e\"\n        - otherwise:\n";
    let src = format!("{SH}# Function: f\n- in: items: list of int, shape: Shape\n- effect: local\n{body}");
    common::ok(&use_it(&src));
    let deeper = format!("{src}          - if x > 1:\n            - print: \"deep\"\n");
    let e = sema_errors(&use_it(&deeper));
    assert!(e.contains("Statements are nested 4 levels deep; the limit is 3."), "{e}");
}

#[test]
fn match_is_pure_unless_its_branches_are_not() {
    let c = common::ok(&use_it(&format!(
        "{SH}# Function: f\n- in: s: Shape\n- out: int\n- effect: pure\n  - match s:\n    - Circle c:\n      - return c.radius\n    - otherwise:\n      - return 0\n"
    )));
    assert!(c.blocks.iter().find(|b| b.unit == "f").unwrap().effect.is_pure());
    let e = sema_errors(&in_function_pure("  - match s:\n    - otherwise:\n      - print: \"x\"\n"));
    assert!(e.contains("says \"- effect: pure\", but this line has the effect local (print)"), "{e}");
}

fn in_function_pure(body: &str) -> String {
    func(SH, "s: Shape", "pure", body)
}

#[test]
fn an_unused_enum_is_reported_like_an_unused_record() {
    let w = common::warnings(&format!("{SH}# Command: go\n\n## Run\n- stop\n"));
    assert!(w.contains("Unused enum: nothing refers to \"Shape\"."), "{w}");
    let w = common::warnings("# Type: T\n- is: text\n\n# Command: go\n\n## Run\n- stop\n");
    assert!(w.contains("Unused type: nothing refers to \"T\"."), "{w}");
    // a variant value is a use of its enum
    let used = format!("{SH}# Command: go\n\n## Run\n- set s = Empty\n- print: \"x\"\n");
    assert!(!common::warnings(&used).contains("Unused enum"));
}

// ---------------------------------------------------------------------------------- formatter

#[test]
fn the_fixture_is_canonical_and_messy_new_forms_are_tidied() {
    assert!(is_canonical(SHAPES, "t.agent.md").unwrap());
    let messy = "# Enum:   Shape\n-   Circle:  radius:int\n- Rect:width:int,height:int\n-   Empty\n\n#   Type: Sizes\n-   is:   list   of   int\n\n# Function: f\n- in: s:Shape\n- effect: pure\n  - match   s:\n    -   Circle   c:\n      - return 1\n    - Shape.Rect   r:\n      - return 2\n    - otherwise:\n      - return 3\n  - set x = Circle{radius:3}\n  - set y = Shape.Rect {width:1,height:2}\n";
    let out = format_source(messy, "t.agent.md").unwrap();
    assert_eq!(
        out,
        "# Enum: Shape\n- Circle: radius: int\n- Rect: width: int, height: int\n- Empty\n\n# Type: Sizes\n- is: list of int\n\n# Function: f\n- in: s: Shape\n- effect: pure\n  - match s:\n    - Circle c:\n      - return 1\n    - Shape.Rect r:\n      - return 2\n    - otherwise:\n      - return 3\n  - set x = Circle { radius: 3 }\n  - set y = Shape.Rect { width: 1, height: 2 }\n"
    );
    // formatting is idempotent and keeps the comments
    assert_eq!(format_source(&out, "t.agent.md").unwrap(), out);
    let commented = "# Enum: Shape\n> the round one\n- Circle: radius: int\n- Empty\n\n# Function: f\n- in: s: Shape\n- effect: pure\n  - match s:\n    > first\n    - Circle c:\n      - return 1\n    > the rest\n    - otherwise:\n      - return 2\n";
    assert_eq!(format_source(commented, "t.agent.md").unwrap(), commented);
}

// ---------------------------------------------------------------------------------- IR

#[test]
fn the_ir_has_the_enums_and_resolved_matches() {
    let src = format!(
        "{SH}# Type: Pic\n- is: Shape\n\n# Function: f\n- in: s: Pic, n: maybe int\n- out: int\n- effect: pure\n  - match s:\n    - Circle c:\n      - return c.radius\n    - Rect r:\n      - return r.width\n    - Empty:\n      - set e = Rect {{ width: 1, height: 2 }}\n      - return 0\n"
    );
    let p = Project::from_str("t.agent.md", &src, "");
    let checked = sema::check(&p).unwrap_or_else(|d| panic!("{}", p.render(&d)));
    let prog = ir::lower(&p, &checked);
    assert_eq!(prog.enums.len(), 1);
    let e = &prog.enums[0];
    assert_eq!(e.name, "Shape");
    assert_eq!(e.variants.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["Circle", "Rect", "Empty"]);
    let f = &prog.functions[0];
    // the alias is gone: the parameter is a Shape
    assert_eq!(f.params[0].ty, Type::Enum("Shape".into()));
    let IS::Match { arms, otherwise, .. } = &f.body[0].kind else { panic!("{:?}", f.body[0].kind) };
    assert!(otherwise.is_none());
    assert_eq!(arms.len(), 3);
    assert_eq!(arms[0].pattern, Pattern::Variant { enum_name: "Shape".into(), variant: "Circle".into() });
    assert_eq!(arms[0].bind.as_ref().unwrap().ty, Type::Variant("Shape".into(), "Circle".into()));
    assert!(arms[2].bind.is_none());
    // the branch's name reads fields of the variant
    let IS::Return(r) = &arms[0].body[0].kind else { panic!() };
    assert!(matches!(&r.kind, IX::Field { field, .. } if field == "radius"));
    assert_eq!(r.ty, Type::Int);
    let IS::Let { value, .. } = &arms[2].body[0].kind else { panic!() };
    let IX::Variant { enum_name, variant, fields } = &value.kind else { panic!("{:?}", value.kind) };
    assert_eq!((enum_name.as_str(), variant.as_str(), fields.len()), ("Shape", "Rect", 2));
    assert_eq!(value.ty, Type::Enum("Shape".into()));
    // the match is pure when its branches are
    assert!(f.effect.is_pure());
}

// ---------------------------------------------------------------------------------- lint

#[test]
fn lint_looks_inside_match_branches() {
    let src = "# Enum: Shape\n- Circle: radius: int\n- Empty\n\n# Function: f\n- in: s: Shape\n- effect: local\n  - match s:\n    - Circle data:\n      - print: \"{data.radius}\"\n    - Empty:\n      - print: \"https://example.com/a and https://example.com/a\"\n";
    let f = parse(src);
    let findings = agentmark::lint::lint(std::slice::from_ref(&f), &agentmark::lint::LintConfig::default());
    let texts: Vec<String> = findings.iter().map(|d| d.message.clone()).collect();
    assert!(texts.iter().any(|t| t.contains("\"data\" does not say what it holds.")), "{texts:?}");
    assert!(texts.iter().any(|t| t.contains("URL")), "{texts:?}");
}

// ---------------------------------------------------------------------------------- generated program

fn run_command(tag: &str, src: &str, files: &[(&str, &str)], args: serde_json::Value) -> (String, String) {
    let d = temp(tag);
    write(&d, "prog.agent.md", src);
    for (name, text) in files {
        write(&d, name, text);
    }
    let fx = d.join("fx");
    write(&fx, "model.json", r#"{"today":"2026-10-13","responses":[]}"#);
    write(&fx, "command.json", &json!({"command": "demo", "args": args}).to_string());
    let o = agentmark(&["run", s(&d.join("prog.agent.md")), "--mock", "--fixtures", s(&fx)]).output_locked();
    assert!(o.status.success(), "{}", stderr(&o));
    (stdout(&o), stderr(&o))
}

#[test]
fn the_fixture_program_prints_what_each_match_decides() {
    let d = temp("shapes-data");
    write(
        &d,
        "entries.json",
        r#"[{"label":"a","shape":{"kind":"Circle","radius":3}},{"label":"b","shape":{"kind":"Empty"}},{"label":"c","shape":{"kind":"Rect","width":4,"height":4}}]"#,
    );
    write(&d, "count.json", "12");
    let args = json!({"entries_path": s(&d.join("entries.json")), "count_path": s(&d.join("count.json"))});
    let (out, _) = run_command("shapes", SHAPES, &[], args.clone());
    assert_eq!(
        out,
        "circle r=3\nrect 2x5\nnothing\n3.50 EUR / 0.00 EUR\n7\na: circle r=3 (round)\nb: nothing (plain)\nb is empty\nc: rect 4x4 (square)\nfound b\ncount 12\nn 12\nhit c\nz\n"
    );
    // the `error` branch of the result, and `no entries` when the JSON does not fit the enum
    write(&d, "entries.json", r#"[{"label":"a","shape":{"kind":"Triangle"}}]"#);
    let (out, _) = run_command("shapes2", SHAPES, &[], json!({"entries_path": s(&d.join("entries.json")), "count_path": s(&d.join("missing.json"))}));
    assert_eq!(out, "circle r=3\nrect 2x5\nnothing\n3.50 EUR / 0.00 EUR\n7\nno entries\n");
    write(&d, "entries.json", "[]");
    let (out, _) = run_command("shapes3", SHAPES, &[], json!({"entries_path": s(&d.join("entries.json")), "count_path": s(&d.join("missing.json"))}));
    assert!(out.ends_with("no entry b\nunreadable\nn 0\nno hit\nz\n"), "{out}");
}

/// Branches that leave their name unused, a `result` matched in a route: the generated code must
/// build without a warning (regression: the first draft named the failure `problem__err`).
const SERVICE: &str = "# Function: read_count\n- in: path: text\n- out: result of int\n- effect: local\n  - try read file path as int -> n\n  - return n\n\n# Enum: Shape\n- Circle: radius: int\n- Rect: width: int, height: int\n- Empty\n\n> Answers with shapes and measures the ones it is sent.\n# Service: shapes\n- listen: 127.0.0.1:8099\n\n## GET /shape\n- respond 200 Circle { radius: 3 }\n\n## GET /empty\n- respond 200 Shape.Empty\n\n## POST /area\n- read body as Shape -> shape\n- match shape:\n  - Circle c:\n    - respond 200 { area: c.radius * c.radius * 3 }\n  - Rect r:\n    - respond 200 { area: r.width * r.height }\n  - Empty:\n    - respond 200 { area: 0 }\n\n## POST /kind\n- read body as Shape -> shape\n- match shape:\n  - Circle c:\n    - respond 200 { kind: \"round\" }\n  - otherwise:\n    - respond 200 { kind: \"angular\" }\n\n## GET /count\n- match read_count(\"count.json\"):\n  - ok total:\n    - respond 200 { total: total }\n  - error problem:\n    - respond 404 { problem: problem }\n";

#[test]
fn enum_values_travel_as_tagged_json_through_a_service() {
    let d = temp("shapes-service");
    write(&d, "shapes.agent.md", SERVICE);
    let out = d.join("out");
    let o = agentmark(&["build", s(&d.join("shapes.agent.md")), "-o", s(&out), "--no-check"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    // generated code is warning free
    let bin = build_bin(&out, "shapes");
    let fx = d.join("fx");
    write(&fx, "model.json", r#"{"today":"2026-10-13","responses":[]}"#);
    let requests = |list: serde_json::Value| write(&fx, "requests.json", &list.to_string());
    requests(json!([
        {"method": "GET", "path": "/shape", "expect_body": {"kind": "Circle", "radius": 3}},
        {"method": "GET", "path": "/empty", "expect_body": {"kind": "Empty"}},
        {"method": "POST", "path": "/area", "body": {"kind": "Circle", "radius": 2}, "expect_body": {"area": 12}},
        {"method": "POST", "path": "/area", "body": {"kind": "Rect", "width": 2, "height": 5}, "expect_body": {"area": 10}},
        {"method": "POST", "path": "/area", "body": {"kind": "Empty"}, "expect_body": {"area": 0}},
        {"method": "POST", "path": "/kind", "body": {"kind": "Circle", "radius": 1}, "expect_body": {"kind": "round"}},
        {"method": "POST", "path": "/kind", "body": {"kind": "Empty"}, "expect_body": {"kind": "angular"}},
        {"method": "GET", "path": "/count", "expect_body": {"total": 12}}
    ]));
    write(&fx, "count.json", "12");
    let run = |bin: &Path| Command::new(bin).current_dir(&d).args(["--once", "--mock", "--fixtures", s(&fx)]).output().unwrap();
    let ok = run(&bin);
    assert!(ok.status.success(), "{}", stderr(&ok));
    // a body that is not one of the variants is a client error, not a crash
    requests(json!([{"method": "POST", "path": "/area", "body": {"kind": "Triangle"}}]));
    let bad = run(&bin);
    assert!(bad.status.success(), "{}", stderr(&bad));
    // the `error` branch of the result: no count.json in the fixtures, the route answers 404 with the message
    std::fs::remove_file(fx.join("count.json")).unwrap();
    requests(json!([{"method": "GET", "path": "/count", "expect_body": {"total": 12}}]));
    let missing = run(&bin);
    assert_eq!(missing.status.code(), Some(1), "the 404 body is not {{\"total\":12}}: {}", stderr(&missing));
    requests(json!([{"method": "GET", "path": "/count"}]));
    assert!(run(&bin).status.success());
    // and the tagged JSON the service wrote is what it is expected to read: a wrong expectation fails the run
    requests(json!([{"method": "GET", "path": "/shape", "expect_body": {"radius": 3}}]));
    let wrong = run(&bin);
    assert_eq!(wrong.status.code(), Some(1), "{}", stderr(&wrong));
}

#[test]
fn match_in_the_last_statement_of_a_function_returns_from_every_branch() {
    // every branch ends the function, and the generated code has no unreachable tail
    let src = format!(
        "{SH}# Function: size\n- in: s: Shape\n- out: int\n- effect: pure\n  - match s:\n    - Circle c:\n      - return c.radius\n    - Rect r:\n      - return r.width * r.height\n    - Empty:\n      - return 0\n\n# Command: demo\n- args: width: int\n\n## Run\n- print: \"{{size(Rect {{ width: width, height: 3 }})}}\"\n"
    );
    let d = temp("shapes-tail");
    write(&d, "prog.agent.md", &src);
    let out = d.join("out");
    let o = agentmark(&["build", s(&d.join("prog.agent.md")), "-o", s(&out), "--no-check"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let (text, _) = run_command("shapes-tail-run", &src, &[], json!({"width": 4}));
    assert_eq!(text, "12\n");
}

#[test]
fn the_documentation_page_lists_enums_and_types() {
    let d = temp("shapes-doc");
    write(&d, "shapes.agent.md", SHAPES);
    let o = agentmark(&["doc", s(&d.join("shapes.agent.md"))]).output_locked();
    let text = stdout(&o);
    assert!(text.contains("## Enums\n\n### Shape\n"), "{text}");
    assert!(text.contains("- Circle: radius: int"), "{text}");
    assert!(text.contains("- Empty\n"), "{text}");
    assert!(text.contains("## Types\n\n### Entries\n\nAnother name for `list of Entry`."), "{text}");
}

const NOTES: &str = "# Record: Note\n- id: int\n- title: text\n\n> Parses notes from JSON and says what happened.\n# Command: demo\n- args: body: text\n\n## Run\n- set parsed = json.parse(body) as list of Note\n- match parsed:\n  - ok notes:\n    - print: \"{count of notes} notes\"\n  - error problem:\n    - print: \"bad json\"\n- match number.parse(body):\n  - ok n:\n    - print: \"number {n}\"\n  - error:\n    - print: \"not a number\"\n";

#[test]
fn library_results_are_stored_and_matched_like_function_results() {
    let c = common::ok(NOTES);
    assert!(c.expr_types.values().any(|t| matches!(t, Type::Result(_))));
    assert!(is_canonical(NOTES, "t.agent.md").unwrap());
    let (out, _) = run_command("notes-ok", NOTES, &[], json!({"body": "[{\"id\":1,\"title\":\"a\"},{\"id\":2,\"title\":\"b\"}]"}));
    assert_eq!(out, "2 notes\nnot a number\n");
    let (out, _) = run_command("notes-bad", NOTES, &[], json!({"body": "12"}));
    assert_eq!(out, "bad json\nnumber 12\n");
    // the stored result can only be matched
    let e = sema_errors(&NOTES.replace("- match parsed:", "- set copy = parsed\n- match parsed:"));
    assert!(e.contains("\"parsed\" holds a result, which can only be matched."), "{e}");
}

const TOTALS: &str = "# Enum: Kind\n- Sale: amount: money\n- Refund: amount: money\n- Note\n\n# Type: Totals\n- is: map of text to money\n\n# Function: lookup\n- in: totals: Totals\n- out: maybe money\n- effect: pure\n  - return totals at \"sales\"\n\n# Function: signed\n- in: kind: Kind\n- out: money\n- effect: pure\n  - match kind:\n    - Sale s:\n      - return s.amount\n    - Refund r:\n      - return 0 EUR - r.amount\n    - Note:\n      - return 0 EUR\n\n> Adds sales and refunds per region, matching each kind inside the loops.\n# Command: demo\n\n## Run\n- set totals = empty map of text to money\n- set kinds = list of Sale { amount: 5.00 EUR }, Refund { amount: 2.00 EUR }, Note\n- for each kind in kinds with index i:\n  - match kind:\n    - Sale s:\n      - put s.amount at \"sales\" in totals\n    - Refund:\n      - put signed(kind) at \"refunds\" in totals\n    - otherwise:\n      - print: \"note {i}\"\n- for each region, total in totals:\n  - print: \"{region}: {total}\"\n- match lookup(totals):\n  - some m:\n    - print: \"sales {m}\"\n  - none:\n    - print: \"no sales\"\n";

#[test]
fn map_aliases_and_matches_inside_collection_loops() {
    let alias = "# Type: Totals\n- is: map of text to money\n\n# Function: f\n- in: totals: Totals\n- out: maybe money\n- effect: pure\n  - return totals at \"EU\"\n";
    let c = common::ok(&use_it(alias));
    assert_eq!(c.aliases["Totals"].ty.to_string(), "map of text to money");
    assert_eq!(c.functions["f"].params[0].ty.to_string(), "map of text to money");
    assert!(is_canonical(alias, "t.agent.md").unwrap());
    let c = common::ok(TOTALS);
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    let (out, _) = run_command("totals", TOTALS, &[], json!({}));
    assert_eq!(out, "note 3\nrefunds: -2.00 EUR\nsales: 5.00 EUR\nsales 5.00 EUR\n");
}

#[test]
fn a_stored_result_must_be_matched_on_every_path() {
    let head = format!("{STORED}# Command: demo\n- args: path: text, flag: bool\n\n## Run\n");
    let never = "The result in \"n\" is never matched, so its failure would be lost.";
    // straight line
    let e = sema_errors(&format!("{head}- set n = read_count(path)\n- print: \"done\"\n"));
    assert!(e.contains(never), "{e}");
    // one branch matches, the other does not
    let e = sema_errors(&format!("{head}- set n = read_count(path)\n- if flag:\n  - match n:\n    - ok:\n      - print: \"a\"\n    - error:\n      - print: \"b\"\n"));
    assert!(e.contains(never), "{e}");
    // a branch that leaves with the result unmatched
    let e = sema_errors(&format!("{head}- set n = read_count(path)\n- if flag:\n  - stop\n- match n:\n  - ok:\n    - print: \"a\"\n  - error:\n    - print: \"b\"\n"));
    assert!(e.contains(never), "{e}");
    // made in a loop and never matched
    let e = sema_errors(&format!("{head}- for each i in list of 1, 2:\n  - set n = read_count(path)\n  - print: \"x\"\n"));
    assert!(e.contains(never), "{e}");
    // matched before the loop's second round: the loop message talks about results
    let e = sema_errors(&format!("{head}- set n = read_count(path)\n- for each i in list of 1, 2:\n  - match n:\n    - ok:\n      - print: \"a\"\n    - error:\n      - print: \"b\"\n"));
    assert!(e.contains("Match \"n\" inside the loop where it is made, or after the loop."), "{e}");
    // matched on every path is fine
    common::ok(&format!("{head}- set n = read_count(path)\n- if flag:\n  - match n:\n    - ok:\n      - print: \"a\"\n    - error:\n      - print: \"b\"\n- else:\n  - match n:\n    - ok:\n      - print: \"c\"\n    - error:\n      - print: \"d\"\n"));
    common::ok(&format!("{head}- for each i in list of 1, 2:\n  - set n = read_count(path)\n  - match n:\n    - ok:\n      - print: \"a\"\n    - error:\n      - print: \"b\"\n"));
}
