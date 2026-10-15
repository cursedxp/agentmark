//! The built-in library in the checker (SPEC 22.3): typing, effects, and the plain errors
//! E340-E347 (unknown function, `result` without `try`, `as` types, bad literal patterns, the
//! `file` statement).

mod common;
use common::*;

use agentmark::sema::Type;

/// Numbered steps: the top-level lines of `body` are numbered, indented lines are the steps
/// under them, as written.
fn steps(body: &str) -> String {
    let mut out = String::new();
    let mut n = 0;
    for line in body.lines() {
        if line.starts_with(' ') {
            out.push_str(&format!("  {line}\n"));
        } else {
            n += 1;
            out.push_str(&format!("  {n}. {line}\n"));
        }
    }
    out
}

/// A command with `body` as its steps (the first step is on line 8).
fn run(body: &str) -> String {
    format!("# Record: Sale\n- region: text\n- amount: money\n\n# Command: c\n- in: name: text\n- steps:\n{}", steps(body))
}

/// A function returning `out` with `body` as its steps; it touches nothing.
fn func(out: &str, body: &str) -> String {
    format!("# Function: f\n- in: s: text, n: int, m: money, d: date\n- out: {out}\n- touches: nothing\n- steps:\n{}", steps(body))
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
    let src = run("try number.parse(name) into n\ntry money.parse(name) into m\ntry date.parse(name, \"YYYY-MM-DD\") into d\nprint: \"{n} {money.format(m)} {date.format(d, \"YYYY\")}\"");
    let c = ok(&src);
    let types: Vec<String> = c.expr_types.iter().filter(|(k, _)| k.span.line >= 8 && k.span.line <= 10).map(|(_, t)| t.to_string()).collect();
    for want in ["result of int", "result of money", "result of date"] {
        assert!(types.iter().any(|t| t == want), "{want} in {types:?}");
    }
}

#[test]
fn decoding_takes_the_type_after_as() {
    let src = run("try json.parse(name) as Sale into one\ntry do csv.parse(name) as list of Sale into rows\ntry csv.parse(name) as list of Sale into again\nprint: \"{one.region} {count of rows} {count of again}\"");
    ok(&src);
}

#[test]
fn a_decoded_value_has_the_declared_type() {
    let src = run("try json.parse(name) as list of Sale into rows\nprint: \"{count of rows}\"");
    let c = ok(&src);
    assert!(c.expr_types.values().any(|t| *t == Type::Result(Box::new(Type::List(Box::new(Type::Record("Sale".into())))))));
}

// ------------------------------------------------------------------ effects

#[test]
fn library_calls_are_pure_and_the_file_tool_is_local() {
    let c = ok(&func("text", "return text.upper(s)"));
    assert!(c.blocks.iter().all(|b| b.effect.is_pure()));
    let c = ok(&run("try file read name into t\nfile write \"o.txt\": t\nread input into typed\nprint: typed"));
    let shown = c.blocks.iter().find(|b| b.effect.local.contains(&"file".to_string())).expect("a block with the file effect").effect.to_string();
    assert_eq!(shown, "local(file, input, print)");
}

#[test]
fn a_function_that_touches_nothing_cannot_use_the_file_tool() {
    let e = first_error(&func("text", "file delete s\nreturn s"));
    assert!(e.contains("says \"- touches: nothing\"") && e.contains("this step touches files."), "{e}");
}

#[test]
fn library_calls_are_fine_in_a_rule() {
    ok("# Record: Invoice\n- id: text\n- status: text\n\n# Rule: shouty Invoice\n- when: text.upper(status) = \"LATE\" and regex.matches(id, \"^INV\")\n");
}

// ------------------------------------------------------------------ errors

#[test]
fn an_unknown_function_names_the_module_and_suggests() {
    let e = first_error(&func("text", "return text.uper(s)"));
    assert!(e.contains("The module \"text\" has no function \"uper\"."), "{e}");
    assert!(e.contains("Did you mean \"upper\"?"), "{e}");
}

#[test]
fn a_misspelled_module_suggests_the_whole_name() {
    let e = first_error(&func("text", "return txt.upper(s)"));
    assert!(e.contains("Unknown function \"txt.upper\"") && e.contains("Did you mean \"text.upper\"?"), "{e}");
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
    let e = first_error(&run("regex.find(name, \"a\") into found\nprint: text.upper(found)"));
    assert!(e.contains("must be text, but this is maybe text") && e.contains("is some"), "{e}");
}

#[test]
fn a_result_without_try_says_how_to_write_it() {
    let e = first_error(&run("print: \"{number.parse(name)}\""));
    assert!(e.contains("\"number.parse\" can fail (it returns \"result of int\")") && e.contains("try number.parse"), "{e}");
    let e = first_error(&run("file read name into t"));
    assert!(e.contains("\"file read\" can fail"), "{e}");
    // stored without `try`, the result itself is kept and must be matched (Q300, Q457)
    let e = first_error(&run("number.parse(name) into n"));
    assert!(e.contains("The result in \"n\" is never matched, so its failure would be lost."), "{e}");
    assert!(e.contains("Match it with \"match n:\"") && e.contains("or use \"try ... into value\" instead."), "{e}");
}

#[test]
fn json_and_csv_parse_need_their_type() {
    let e = first_error(&run("try json.parse(name) into x"));
    assert!(e.contains("needs to know what to read") && e.contains("json.parse(body) as Note"), "{e}");
    let e = first_error(&run("try do csv.parse(name) as Sale into x"));
    assert!(e.contains("reads a list of records"), "{e}");
    let e = first_error(&run("try do csv.parse(name) as list of text into x"));
    assert!(e.contains("csv.parse"), "{e}");
    let e = first_error(&run("try json.parse(name) as Nope into x"));
    assert!(e.contains("Nope"), "{e}");
}

#[test]
fn a_literal_pattern_that_cannot_work_is_checked_now() {
    let e = first_error(&func("bool", "return regex.matches(s, \"(\")"));
    assert!(e.contains("\"(\" is not a valid pattern"), "{e}");
    let e = first_error(&func("text", "return date.format(d, \"%Q\")"));
    assert!(e.contains("is not a date pattern"), "{e}");
    // a constant holding a pattern is looked through
    let src = format!("# Constant: bad\n- value: \"[\"\n{}", func("bool", "return regex.matches(s, bad)"));
    let e = first_error(&src);
    assert!(e.contains("is not a valid pattern"), "{e}");
    ok(&func("bool", "return regex.matches(s, \"^[a-z]+$\")"));
    ok(&func("text", "return date.format(d, \"%Y-%m-%d\")"));
}

#[test]
fn the_file_statement_has_a_shape_per_verb() {
    let e = first_error(&run("file raed name"));
    assert!(e.contains("no verb \"raed\"") && e.contains("Did you mean \"read\"?"), "{e}");
    let e = first_error(&run("file rename name"));
    assert!(e.contains("file rename <old> to <new>"), "{e}");
    let e = first_error(&run("file write name"));
    assert!(e.contains("needs the text after a colon"), "{e}");
    let e = first_error(&run("file write name to \"x\""));
    assert!(e.contains("not after \"to\""), "{e}");
    let e = first_error(&run("file delete name: \"x\""));
    assert!(e.contains("takes only a path"), "{e}");
    let e = first_error(&run("file list name"));
    assert!(e.contains("name it with \"into <name>\""), "{e}");
    let e = first_error(&run("file delete name into gone"));
    assert!(e.contains("gives nothing back"), "{e}");
    let e = first_error(&run("file delete 3"));
    assert!(e.contains("A file path must be text"), "{e}");
}

#[test]
fn file_results_have_their_types() {
    let c = ok(&run("file list name into names\nfile exists name into there\ntry file read name into t\nif there:\n   1. print: \"{count of names} {text.length(t)}\""));
    assert!(c.expr_types.values().any(|t| t.to_string() == "list of text"));
}
