//! The built-in library in the checker (SPEC 22.3): typing, effects, and the plain errors
//! E340-E347 (unknown function, `result` without `try`, `as` types, bad literal patterns, the
//! `file` statement).

mod common;
use common::*;

use agentmark::sema::Type;

/// A command with `body` as its `## Run` block.
fn run(body: &str) -> String {
    let indented: String = body.lines().map(|l| format!("{l}\n")).collect();
    format!("# Record: Sale\n- region: text\n- amount: money\n\n# Command: c\n- args: name: text\n\n## Run\n{indented}")
}

/// A pure function returning `out` with `body` in it.
fn func(out: &str, body: &str) -> String {
    let lines: String = body.lines().map(|l| format!("  - {l}\n")).collect();
    format!("# Function: f\n- in: s: text, n: int, m: money, d: date\n- out: {out}\n- effect: pure\n{lines}")
}

fn ty_of(src: &str, text: &str) -> String {
    let c = ok(src);
    let at = src.find(text).unwrap_or_else(|| panic!("{text} not in source"));
    let line = src[..at].matches('\n').count() as u32 + 1;
    let col = (at - src[..at].rfind('\n').map_or(0, |i| i + 1)) as u32 + 1;
    let (_, t) = c
        .expr_types
        .iter()
        .find(|(k, _)| k.span.line == line && k.span.col == col && k.span.len as usize == text.len())
        .unwrap_or_else(|| panic!("no expression {text} at {line}:{col}"));
    t.to_string()
}

// ------------------------------------------------------------------ typing

#[test]
fn every_function_has_the_type_of_its_table_row() {
    let cases = [
        ("text.split(s, \",\")", "list of text"),
        ("text.lines(s)", "list of text"),
        ("text.trim(s)", "text"),
        ("text.lower(s)", "text"),
        ("text.upper(s)", "text"),
        ("text.replace(s, \"a\", \"b\")", "text"),
        ("text.contains(s, \"a\")", "bool"),
        ("text.starts_with(s, \"a\")", "bool"),
        ("text.ends_with(s, \"a\")", "bool"),
        ("text.length(s)", "int"),
        ("text.pad(s, 4)", "text"),
        ("text.repeat(s, n)", "text"),
        ("regex.matches(s, \"a\")", "bool"),
        ("regex.find(s, \"a\")", "maybe text"),
        ("regex.captures(s, \"(a)\")", "maybe list of text"),
        ("regex.replace(s, \"a\", \"b\")", "text"),
        ("number.round(n, 0)", "int"),
        ("number.round(m, 1)", "money"),
        ("number.min(n, 3)", "int"),
        ("number.max(m, 3.00 EUR)", "money"),
        ("number.abs(n)", "int"),
        ("number.format(n, 2)", "text"),
        ("money.format(m)", "text"),
        ("date.format(d, \"YYYY\")", "text"),
        ("date.format(now, \"YYYY\")", "text"),
        ("date.days_between(d, today)", "int"),
        ("path.join(s, \"x\")", "text"),
        ("path.name(s)", "text"),
        ("path.stem(s)", "text"),
        ("path.extension(s)", "text"),
        ("path.parent(s)", "text"),
        ("json.encode(m)", "text"),
    ];
    for (call, want) in cases {
        let src = func(want, &format!("return {call}"));
        assert_eq!(ty_of(&src, call), want, "{call}");
    }
}

#[test]
fn calls_that_can_fail_are_results_after_try_and_give_the_inner_type() {
    let src = run("- try number.parse(name) -> n\n- try money.parse(name) -> m\n- try date.parse(name, \"YYYY-MM-DD\") -> d\n- print: \"{n} {money.format(m)} {date.format(d, \"YYYY\")}\"");
    let c = ok(&src);
    let types: Vec<String> = c.expr_types.iter().filter(|(k, _)| k.span.line >= 9 && k.span.line <= 11).map(|(_, t)| t.to_string()).collect();
    for want in ["result of int", "result of money", "result of date"] {
        assert!(types.iter().any(|t| t == want), "{want} in {types:?}");
    }
}

#[test]
fn decoding_takes_the_type_after_as() {
    let src = run("- try json.parse(name) as Sale -> one\n- try do csv.parse(name) as list of Sale -> rows\n- try csv.parse(name) as list of Sale -> again\n- print: \"{one.region} {count of rows} {count of again}\"");
    ok(&src);
}

#[test]
fn a_decoded_value_has_the_declared_type() {
    let src = run("- try json.parse(name) as list of Sale -> rows\n- print: \"{count of rows}\"");
    let c = ok(&src);
    assert!(c.expr_types.values().any(|t| *t == Type::Result(Box::new(Type::List(Box::new(Type::Record("Sale".into())))))));
}

// ------------------------------------------------------------------ effects

#[test]
fn library_calls_are_pure_and_the_file_tool_is_local() {
    let c = ok(&func("text", "return text.upper(s)"));
    assert!(c.blocks.iter().all(|b| b.effect.is_pure()));
    let c = ok(&run("- try file read name -> t\n- file write \"o.txt\": t\n- read input -> typed\n- print: typed"));
    let shown = c.blocks.iter().find(|b| b.effect.local.contains(&"file".to_string())).expect("a block with the file effect").effect.to_string();
    assert_eq!(shown, "local(file, input, print)");
}

#[test]
fn a_function_that_declares_pure_cannot_use_the_file_tool() {
    let e = first_error(&func("text", "file delete s\nreturn s"));
    assert!(e.contains("says \"- effect: pure\"") && e.contains("local (file)"), "{e}");
}

#[test]
fn library_calls_are_fine_in_a_rule() {
    ok("# Record: Invoice\n- id: text\n- status: text\n\n# Rule: shouty\n- for: Invoice\n- when: text.upper(status) = \"LATE\" and regex.matches(id, \"^INV\")\n");
}

// ------------------------------------------------------------------ errors

#[test]
fn an_unknown_function_names_the_module_and_suggests() {
    let e = first_error(&func("text", "return text.uper(s)"));
    assert!(e.contains("The module \"text\" has no function \"uper\"."), "{e}");
    assert!(e.contains("Did you mean \"upper\"?"), "{e}");
}

#[test]
fn wrong_argument_counts_and_kinds_are_plain() {
    let e = first_error(&func("text", "return text.pad(s)"));
    assert!(e.contains("\"text.pad\" takes 2 values (t, width), but 1 was given."), "{e}");
    let e = first_error(&func("text", "return text.pad(s, \"4\")"));
    assert!(e.contains("\"width\" of \"text.pad\" must be a number, but this is text."), "{e}");
    let e = first_error(&func("int", "return number.max(n, m)"));
    assert!(e.contains("compares two values of the same kind"), "{e}");
    let e = first_error(&func("text", "return date.format(s, \"YYYY\")"));
    assert!(e.contains("\"d\" of \"date.format\" must be a date or datetime"), "{e}");
    let e = first_error(&func("text", "return csv.encode(s)"));
    assert!(e.contains("\"rows\" of \"csv.encode\" must be a list"), "{e}");
}

#[test]
fn a_maybe_argument_must_be_checked_first() {
    let e = first_error(&run("- set found = regex.find(name, \"a\")\n- print: text.upper(found)"));
    assert!(e.contains("must be text, but this is maybe text") && e.contains("is some"), "{e}");
}

#[test]
fn a_result_without_try_says_how_to_write_it() {
    let e = first_error(&run("- set n = number.parse(name)"));
    assert!(e.contains("\"number.parse\" can fail (it returns \"result of int\")") && e.contains("try number.parse"), "{e}");
    let e = first_error(&run("- file read name -> t"));
    assert!(e.contains("\"file read\" can fail"), "{e}");
    let e = first_error(&run("- number.parse(name) -> n"));
    assert!(e.contains("can fail"), "{e}");
}

#[test]
fn json_and_csv_parse_need_their_type() {
    let e = first_error(&run("- try json.parse(name) -> x"));
    assert!(e.contains("needs to know what to read") && e.contains("json.parse(body) as Note"), "{e}");
    let e = first_error(&run("- try do csv.parse(name) as Sale -> x"));
    assert!(e.contains("reads a list of records"), "{e}");
    let e = first_error(&run("- try do csv.parse(name) as list of text -> x"));
    assert!(e.contains("csv.parse"), "{e}");
    let e = first_error(&run("- try json.parse(name) as Nope -> x"));
    assert!(e.contains("Nope"), "{e}");
}

#[test]
fn a_literal_pattern_that_cannot_work_is_checked_now() {
    let e = first_error(&func("bool", "return regex.matches(s, \"(\")"));
    assert!(e.contains("\"(\" is not a valid pattern"), "{e}");
    let e = first_error(&func("text", "return date.format(d, \"%Q\")"));
    assert!(e.contains("is not a date pattern"), "{e}");
    // a constant holding a pattern is looked through
    let src = format!("# Constant: bad\n- value: \"[\"\n\n{}", func("bool", "return regex.matches(s, bad)"));
    let e = first_error(&src);
    assert!(e.contains("is not a valid pattern"), "{e}");
    ok(&func("bool", "return regex.matches(s, \"^[a-z]+$\")"));
    ok(&func("text", "return date.format(d, \"%Y-%m-%d\")"));
}

#[test]
fn the_file_statement_has_a_shape_per_verb() {
    let e = first_error(&run("- file raed name"));
    assert!(e.contains("no verb \"raed\"") && e.contains("Did you mean \"read\"?"), "{e}");
    let e = first_error(&run("- file rename name"));
    assert!(e.contains("file rename <old> to <new>"), "{e}");
    let e = first_error(&run("- file write name"));
    assert!(e.contains("needs the text after a colon"), "{e}");
    let e = first_error(&run("- file write name to \"x\""));
    assert!(e.contains("not after \"to\""), "{e}");
    let e = first_error(&run("- file delete name: \"x\""));
    assert!(e.contains("takes only a path"), "{e}");
    let e = first_error(&run("- file list name"));
    assert!(e.contains("name it with \"-> name\""), "{e}");
    let e = first_error(&run("- file delete name -> gone"));
    assert!(e.contains("gives nothing back"), "{e}");
    let e = first_error(&run("- file delete 3"));
    assert!(e.contains("A file path must be text"), "{e}");
}

#[test]
fn file_results_have_their_types() {
    let c = ok(&run("- file list name -> names\n- file exists name -> there\n- try file read name -> t\n- if there:\n  - print: \"{count of names} {text.length(t)}\""));
    assert!(c.expr_types.values().any(|t| t.to_string() == "list of text"));
}
