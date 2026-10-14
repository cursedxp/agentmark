//! Collection words (SPEC 22.2, step 21): parsing, typing, formatting, and behavior of the
//! generated program. The behavior tests build `tests/collections/project` with cargo and judge
//! the program by what it prints (`tests/collections/expected.stdout`), never by generated source.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use agentmark::diag::{render_all, Severity};
use agentmark::fmt::{format_source, is_canonical};
use agentmark::project::Project;
use agentmark::sema;
use agentmark::syntax::ast::{ExprKind, StmtKind};
use agentmark::syntax::collections::{CollExpr, CollStmt};
use agentmark::syntax::parser::parse_file;

use support::*;

fn project_dir() -> PathBuf {
    PathBuf::from(format!("{ROOT}/tests/collections/project"))
}

fn files_of(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            files_of(&p, out);
        } else if p.to_string_lossy().ends_with(".agent.md") {
            out.push(p);
        }
    }
}

/// The errors `check` reports for a single-file program, rendered as the user sees them.
fn errors_of(src: &str) -> String {
    let project = Project::from_str("t.agent.md", src, "");
    match sema::check(&project) {
        Ok(_) => String::new(),
        Err(diags) => {
            let errors: Vec<_> = diags.into_iter().filter(|d| d.severity == Severity::Error).collect();
            project.render(&errors)
        }
    }
}

// ---------------------------------------------------------------------------------- the project

#[test]
fn the_project_is_canonical_and_checks_clean() {
    let mut files = Vec::new();
    files_of(&project_dir(), &mut files);
    assert_eq!(files.len(), 8, "{files:?}");
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        assert!(is_canonical(&src, "t.agent.md").unwrap(), "{} is not in the canonical style", f.display());
    }
    let project = Project::load(project_dir().to_str().unwrap());
    match sema::check(&project) {
        Ok(c) => assert!(c.warnings.is_empty(), "{:?}", c.warnings),
        Err(d) => panic!("{}", project.render(&d)),
    }
}

/// `command` is `None` for a program whose one command is its default (SPEC 22.6: no subcommand name).
fn run_bin(bin: &Path, fx: &Path, command: Option<&str>) -> std::process::Output {
    Command::new(bin).args(["--mock", "--fixtures", s(fx), "--trace", "-"]).args(command).output().unwrap()
}

fn built() -> (Tmp, PathBuf) {
    let d = temp("collections");
    let o = agentmark(&["build", s(&project_dir()), "-o", s(&d.join("out")), "--no-check"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let bin = build_bin(&d.join("out"), "collections");
    (d, bin)
}

#[test]
fn every_word_gives_the_documented_result() {
    let (_d, bin) = built();
    let fx = PathBuf::from(format!("{ROOT}/tests/collections/fx"));
    let o = run_bin(&bin, &fx, Some("words"));
    assert!(o.status.success(), "{}", stderr(&o));
    let lines: Vec<String> = parse_lines(&stdout(&o))
        .iter()
        .map(|t| {
            assert_eq!(t["kind"], "print", "{t}");
            t["message"].as_str().unwrap().to_string()
        })
        .collect();
    let want = std::fs::read_to_string(format!("{ROOT}/tests/collections/expected.stdout")).unwrap();
    assert_eq!(lines.join("\n"), want.trim_end());

    // the sum of two currencies is an error in the trace, not a guess
    let o = run_bin(&bin, &fx, Some("mixed"));
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    let t = parse_lines(&stdout(&o));
    assert_eq!(t.len(), 1, "{t:#?}");
    assert_eq!(t[0]["kind"], "error");
    assert_eq!(t[0]["block"], "Run");
    assert!(t[0]["message"].as_str().unwrap().contains("currency mismatch"), "{t:#?}");
}

// ---------------------------------------------------------------------------------- parsing

const SHAPES: &str = r#"# Function: shapes
- in: rows: list of text
- effect: local
  - set a = list of 1, 2, 3
  - set b = empty list of text
  - set c = map of "EU" to 1, "US" to 2
  - set d = empty map of text to int
  - set e = rows where item != "x"
  - set f = map rows to item
  - set g = sort rows by item descending
  - set h = group rows by item
  - set i = sum of a
  - set j = sum of item in a
  - set k = count of rows where item = "x"
  - set l = min of item in a
  - set m = max of a
  - set n = first of rows
  - set o = last of rows
  - set p = take 10 from rows
  - set q = skip 2 from rows
  - set r = unique rows
  - set t = unique item in rows
  - set u = join rows with ", "
  - set v = c at "EU"
  - set w = keys of c
  - set x = values of c

# Function: statements
- in: rows: list of text, c: map of text to int
- effect: local
  - append "z" to rows
  - put 1 at "FR" in c
  - remove "US" from c
  - for each row in rows with index n:
    - set last_row = row
  - for each region, total in c:
    - set last_region = region
"#;

fn shapes() -> Vec<agentmark::syntax::ast::Stmt> {
    let file = parse_file("t.agent.md", SHAPES).unwrap_or_else(|d| panic!("{}", render_all(&d, SHAPES)));
    let bodies: Vec<_> = file
        .decls
        .iter()
        .map(|d| match d {
            agentmark::syntax::ast::Decl::Function(f) => f.body.clone(),
            _ => panic!(),
        })
        .collect();
    bodies.concat()
}

#[test]
fn every_form_parses_to_its_own_node() {
    let body = shapes();
    let coll = |i: usize| match &body[i].kind {
        StmtKind::Set { expr, .. } => match &expr.kind {
            ExprKind::Coll(c) => (**c).clone(),
            other => panic!("line {i}: {other:?}"),
        },
        other => panic!("{other:?}"),
    };
    assert!(matches!(coll(0), CollExpr::ListLit(v) if v.len() == 3));
    assert!(matches!(coll(1), CollExpr::EmptyList(_)));
    assert!(matches!(coll(2), CollExpr::MapLit(v) if v.len() == 2));
    assert!(matches!(coll(3), CollExpr::EmptyMap { .. }));
    assert!(matches!(coll(4), CollExpr::Where { .. }));
    assert!(matches!(coll(5), CollExpr::MapTo { .. }));
    assert!(matches!(coll(6), CollExpr::Sort { descending: true, .. }));
    assert!(matches!(coll(7), CollExpr::GroupBy { .. }));
    assert!(matches!(coll(8), CollExpr::Sum { expr: None, .. }));
    assert!(matches!(coll(9), CollExpr::Sum { expr: Some(_), .. }));
    assert!(matches!(coll(10), CollExpr::CountWhere { .. }));
    assert!(matches!(coll(11), CollExpr::Extreme { max: false, expr: Some(_), .. }));
    assert!(matches!(coll(12), CollExpr::Extreme { max: true, expr: None, .. }));
    assert!(matches!(coll(13), CollExpr::Edge { last: false, .. }));
    assert!(matches!(coll(14), CollExpr::Edge { last: true, .. }));
    assert!(matches!(coll(15), CollExpr::Slice { skip: false, .. }));
    assert!(matches!(coll(16), CollExpr::Slice { skip: true, .. }));
    assert!(matches!(coll(17), CollExpr::Unique { expr: None, .. }));
    assert!(matches!(coll(18), CollExpr::Unique { expr: Some(_), .. }));
    assert!(matches!(coll(19), CollExpr::Join { .. }));
    assert!(matches!(coll(20), CollExpr::At { .. }));
    assert!(matches!(coll(21), CollExpr::MapPart { values: false, .. }));
    assert!(matches!(coll(22), CollExpr::MapPart { values: true, .. }));
    assert!(matches!(body[23].kind, StmtKind::Append { .. }));
    assert!(matches!(&body[24].kind, StmtKind::Coll(CollStmt::Put { .. })));
    assert!(matches!(&body[25].kind, StmtKind::Coll(CollStmt::Remove { .. })));
    assert!(matches!(&body[26].kind, StmtKind::Coll(CollStmt::ForEachIndex { body, .. }) if body.len() == 1));
    assert!(matches!(&body[27].kind, StmtKind::Coll(CollStmt::ForEachEntry { body, .. }) if body.len() == 1));
    assert_eq!(body.len(), 28);
}

#[test]
fn the_ast_of_every_form_is_stable() {
    let file = parse_file("t.agent.md", SHAPES).unwrap();
    insta::with_settings!({ snapshot_path => "snapshots" }, {
        insta::assert_yaml_snapshot!("shapes", file);
    });
}

#[test]
fn the_shapes_check_and_are_canonical() {
    assert_eq!(errors_of(SHAPES), "");
    assert!(is_canonical(SHAPES, "t.agent.md").unwrap());
}

#[test]
fn old_forms_keep_their_meaning() {
    // `count of rows`, `count of open invoices in …` and `first x in … where …` are not collection words
    let src = "# Function: f\n- in: rows: list of text\n- effect: pure\n  - set a = count of rows\n  - set b = first x in rows where x = \"a\"\n";
    let file = parse_file("t.agent.md", src).unwrap();
    let agentmark::syntax::ast::Decl::Function(f) = &file.decls[0] else { panic!() };
    let kinds: Vec<_> = f
        .body
        .iter()
        .map(|s| match &s.kind {
            StmtKind::Set { expr, .. } => std::mem::discriminant(&expr.kind),
            _ => panic!(),
        })
        .collect();
    assert_eq!(kinds[0], std::mem::discriminant(&ExprKind::CountOf { rule: None, noun: None, source: Box::new(agentmark::syntax::ast::Expr { span: agentmark::diag::Span::new(0, 0, 0), kind: ExprKind::Today }) }));
    assert_eq!(kinds[1], std::mem::discriminant(&ExprKind::FirstIn { var: agentmark::syntax::ast::Ident { name: String::new(), span: agentmark::diag::Span::new(0, 0, 0) }, source: Box::new(agentmark::syntax::ast::Expr { span: agentmark::diag::Span::new(0, 0, 0), kind: ExprKind::Today }), cond: Box::new(agentmark::syntax::ast::Expr { span: agentmark::diag::Span::new(0, 0, 0), kind: ExprKind::Today }) }));
}

#[test]
fn a_literal_takes_every_comma_after_it_unless_parenthesised() {
    // Q320: `f(list of 1, 2, 3)` is one list argument; `f((list of 1, 2), 3)` is two arguments
    let src = "# Function: f\n- in: xs: list of int, n: int\n- out: int\n- effect: pure\n  - return n\n\n# Function: g\n- out: int\n- effect: pure\n  - return f((list of 1, 2), 3)\n";
    assert_eq!(errors_of(src), "");
    let one = src.replace("f((list of 1, 2), 3)", "f(list of 1, 2, 3)");
    assert!(errors_of(&one).contains("\"f\" takes 2 values (xs, n), but 1 was given."), "{}", errors_of(&one));
}

// ---------------------------------------------------------------------------------- formatting

#[test]
fn fmt_writes_one_canonical_spelling() {
    let messy = "# Function: f\n- in: rows: list of text\n- effect: local\n  -   set e   =  rows   where item  != \"x\"\n  - set s = sort   rows by item   descending\n  - set g = group rows by item\n  - set k = join (keys of g) with \",\"\n  - for each row in rows   with index  n:\n    - print: \"{n} {row}\"\n  - put 1   at \"a\"   in (empty map of text to int)\n";
    let want = "# Function: f\n- in: rows: list of text\n- effect: local\n  - set e = rows where item != \"x\"\n  - set s = sort rows by item descending\n  - set g = group rows by item\n  - set k = join keys of g with \",\"\n  - for each row in rows with index n:\n    - print: \"{n} {row}\"\n  - put 1 at \"a\" in empty map of text to int\n";
    let got = format_source(messy, "t.agent.md").unwrap();
    assert_eq!(got, want);
    assert_eq!(format_source(&got, "t.agent.md").unwrap(), got, "fmt is idempotent");
}

#[test]
fn parentheses_are_kept_where_the_meaning_needs_them() {
    // a form that runs to the end of the line is weaker than everything: parenthesised before more text
    let src = "# Function: f\n- in: rows: list of text\n- effect: pure\n  - set n = count of (rows where item = \"a\") + 1\n  - set m = sum of (map rows to item length) in rows\n";
    let got = format_source(src, "t.agent.md").unwrap();
    assert_eq!(format_source(&got, "t.agent.md").unwrap(), got);
    let again = parse_file("t.agent.md", &got).unwrap();
    let first = parse_file("t.agent.md", src).unwrap();
    let strip = |f: &agentmark::syntax::ast::File| serde_json::to_string(f).unwrap();
    // same tree apart from spans
    let drop_spans = |s: String| {
        let v: Value = serde_json::from_str(&s).unwrap();
        fn walk(v: &mut Value) {
            match v {
                Value::Object(m) => {
                    m.remove("span");
                    m.values_mut().for_each(walk);
                }
                Value::Array(a) => a.iter_mut().for_each(walk),
                _ => {}
            }
        }
        let mut v = v;
        walk(&mut v);
        v
    };
    assert_eq!(drop_spans(strip(&first)), drop_spans(strip(&again)));
}

// ---------------------------------------------------------------------------------- mistakes

/// What `check` says about the statements `body` (inside a function of `Sale`s): per error the line
/// within `body` (1-based), the underlined text, and the message with its help line.
fn explain(body: &str) -> Vec<(usize, String, String)> {
    let prefix = "# Record: Sale\n- region: text\n- amount: money\n\n# Function: f\n- in: sales: list of Sale\n- effect: pure\n";
    let first_body_line = prefix.lines().count() + 1;
    let out = errors_of(&format!("{prefix}{body}"));
    out.split("\n\n")
        .filter(|b| !b.is_empty())
        .map(|block| {
            let lines: Vec<&str> = block.lines().collect();
            let line: usize = lines[0].rsplit(':').next().unwrap().parse().unwrap();
            let (src, carets) = (&lines[1][2..], &lines[2][2..]);
            let start = carets.find('^').unwrap();
            let len = carets.matches('^').count();
            let underlined = src[start..start + len].to_string();
            let message = lines[3..].iter().map(|l| l.trim_start()).collect::<Vec<_>>().join("\n");
            (line + 1 - first_body_line, underlined, message)
        })
        .collect()
}

fn one(body: &str) -> (usize, String, String) {
    let mut all = explain(body);
    assert_eq!(all.len(), 1, "{all:#?}");
    all.remove(0)
}

fn says(body: &str, line: usize, under: &str, message: &str) {
    assert_eq!(one(body), (line, under.to_string(), message.to_string()), "for:\n{body}");
}

const MAP_HELP: &str = "A map is written like: map of \"EU\" to 1, \"US\" to 2 (or \"empty map of text to int\").";

#[test]
fn mistakes_are_explained_in_plain_language() {
    says(
        "  - set xs = list of 1, \"a\"\n",
        1,
        "\"a\"",
        "Every list item must be the same kind of value: the first is a number, but this is text.",
    );
    says("  - set n = 5\n  - set m = n where item = 1\n", 2, "n", "\"where\" needs a list, but this is a number.");
    says(
        "  - set ranked = sort sales by item\n",
        1,
        "item",
        "\"sort … by\" needs values it can put in order, but this is Sale.\nNumbers, money, text, dates and durations have an order; records and lists do not.",
    );
    says(
        "  - set names = list of \"a\", \"b\"\n  - set total = sum of names\n",
        2,
        "names",
        "\"sum of\" adds numbers or money, but these are text.\nSum the numbers or amounts of a field: \"sum of amount in rows\".",
    );
    says(
        "  - set text = join (list of 1, 2) with \",\"\n",
        1,
        "list of 1, 2",
        "\"join\" glues a list of text, but this is a list of int.\nTurn the values into text first, for example: map rows to \"{item.id}\"",
    );
    says(
        "  - set m = empty map of money to int\n",
        1,
        "money",
        "A map cannot be keyed by money.\nMap keys are text, numbers, dates, datetimes or yes/no values.",
    );
    says(
        "  - set xs = list of 1, 2\n  - put 1 at \"a\" in xs\n",
        2,
        "xs",
        &format!("\"put … in\" needs a map, but this is list of int.\n{MAP_HELP}"),
    );
    says(
        "  - set eu = sales where regoin = \"EU\"\n",
        1,
        "regoin",
        "Unknown field \"regoin\" on Sale. Did you mean \"region\"?",
    );
    says(
        "  - set g = group sales by amount\n",
        1,
        "amount",
        "\"group … by\" needs a key that is text, a number, a date or a yes/no value, but this is money.",
    );
    says(
        "  - set xs = list of 1, 2\n  - for each k, v in xs:\n    - set z = k\n",
        2,
        "xs",
        &format!("\"for each … , … in\" needs a map, but this is list of int.\n{MAP_HELP}"),
    );
    says("  - set xs = list of 1, 2\n  - set zs = xs at 1\n", 2, "xs", &format!("\"at\" needs a map, but this is list of int.\n{MAP_HELP}"));
    says("  - set xs = list of 1, 2\n  - set ys = take \"a\" from xs\n", 2, "\"a\"", "\"take\" needs a whole number, but this is text.");

    // two mistakes in one statement list are both reported, in order
    let both = explain("  - set ages = map of \"ann\" to 31\n  - put 1 at 5 in ages\n  - put \"old\" at \"bob\" in ages\n");
    assert_eq!(
        both,
        vec![
            (2, "5".to_string(), "The keys of this map are text, but this is a number.".to_string()),
            (3, "\"old\"".to_string(), "This map holds int, but this is text.".to_string()),
        ]
    );
}

#[test]
fn the_expression_of_a_word_must_be_pure() {
    let src = "# Function: shout\n- in: word: text\n- out: text\n- effect: local\n  - print: word\n  - return word\n\n# Function: f\n- effect: pure\n  - set names = list of \"a\", \"b\"\n  - set loud = names where shout(item) = \"a\"\n";
    let e = errors_of(src);
    assert!(e.contains("An expression inside a collection word must be pure, but \"shout\" has the effect local."), "{e}");
    assert!(e.contains("Do the effectful step in a \"for each\" loop instead."), "{e}");
    // the same call outside a word is fine in a local function
    let ok = src.replace("- effect: pure\n  - set names", "- effect: local\n  - set names").replace("names where shout(item) = \"a\"", "list of shout(\"a\")");
    assert_eq!(errors_of(&ok), "");
}

#[test]
fn item_exists_only_inside_a_words_expression() {
    let src = "# Function: f\n- out: text\n- effect: pure\n  - return \"{item}\"\n";
    assert!(errors_of(src).contains("Unknown name \"item\"."), "{}", errors_of(src));
}

#[test]
fn a_bare_field_and_item_are_the_same_element() {
    let src = "# Record: Sale\n- region: text\n- amount: money\n\n# Function: f\n- in: sales: list of Sale\n- out: int\n- effect: pure\n  - set a = sales where region = \"EU\"\n  - set b = sales where item.region = \"EU\"\n  - return count of a + count of b\n";
    assert_eq!(errors_of(src), "");
}

#[test]
fn collection_words_do_not_make_a_function_impure() {
    let src = "# Function: total\n- in: xs: list of int\n- out: int\n- effect: pure\n  - set doubled = map xs to item * 2\n  - set ys = empty list of int\n  - append 1 to ys\n  - return sum of doubled + sum of ys\n";
    assert_eq!(errors_of(src), "");
}

#[test]
fn fmt_keeps_list_literals_apart() {
    // regression: a literal inside a literal, a call or a record lost its parentheses and swallowed the next comma
    let src = "# Function: f\n- in: xs: list of int, n: int\n- out: int\n- effect: pure\n  - return n\n\n# Function: g\n- out: int\n- effect: pure\n  - set nested = list of (list of 1, 2), (list of 3), list of 4\n  - set m = map of \"a\" to (list of 1, 2), \"b\" to (list of 3)\n  - set r = { xs: (list of 1, 2), n: 3 }\n  - return f((list of 1, 2), 3)\n";
    let want = src.replace("\"b\" to (list of 3)", "\"b\" to list of 3");
    let got = format_source(src, "t.agent.md").unwrap();
    assert_eq!(got, want);
    assert_eq!(errors_of(&got), "");
}

#[test]
fn deeply_nested_words_parse_in_a_small_stack() {
    // regression: the grammar overflowed the 2 MiB stack of a debug-build test thread (Q328)
    let src = "# Function: f\n- in: rows: list of text\n- out: text\n- effect: pure\n  - return join (map (sort (rows where item != \"x\") by item) to \"{item}!\") with \",\"\n";
    assert_eq!(errors_of(src), "");
    assert!(is_canonical(src, "t.agent.md").unwrap());
}

// ---------------------------------------------------------------------------------- a map in service state

#[test]
fn a_map_in_service_state_changes_between_requests_and_is_served_in_key_order() {
    let d = temp("tally");
    let service = format!("{ROOT}/tests/collections/service");
    let o = agentmark(&["build", &service, "-o", s(&d.join("out")), "--no-check"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let bin = build_bin(&d.join("out"), "tally");
    let fx = d.join("fx");
    copy_dir(Path::new(&format!("{ROOT}/tests/collections/service-fx")), &fx);

    // every request carries the status and body it must get; a mismatch fails the run
    let o = Command::new(&bin).args(["--once", "--mock", "--fixtures", s(&fx), "--trace", "-"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let statuses: Vec<_> = parse_lines(&stdout(&o)).iter().filter(|t| t["kind"] == "http").map(|t| t["status"].clone()).collect();
    assert_eq!(statuses, vec![200; 7]);

    // the check has teeth: a wrong expectation fails
    write(&fx, "requests.json", r#"[{"method":"POST","path":"/count/zoe","expect_body":{"zoe":2}}]"#);
    let bad = Command::new(&bin).args(["--once", "--mock", "--fixtures", s(&fx)]).output().unwrap();
    assert_eq!(bad.status.code(), Some(1), "{}", stderr(&bad));
}

#[test]
fn the_service_checks_with_its_effects() {
    let project = Project::load(&format!("{ROOT}/tests/collections/service"));
    let checked = sema::check(&project).unwrap_or_else(|d| panic!("{}", project.render(&d)));
    let effects: Vec<String> = checked.blocks.iter().map(|b| format!("{} {}", b.label, b.effect)).collect();
    assert_eq!(
        effects,
        vec!["POST /count/{name} local(state)", "POST /forget/{name} local(state)", "GET /summary pure"]
    );
}

// ---------------------------------------------------------------------------------- review fixes

#[test]
fn a_variable_may_not_hide_a_field_of_the_element() {
    // before: the variable silently won and `count of sales where region = "EU"` counted nothing
    let (line, under, msg) = one("  - set region = \"US\"\n  - set n = count of sales where region = \"EU\"\n");
    assert_eq!((line, under.as_str()), (2, "region"));
    assert_eq!(
        msg,
        "\"region\" is a variable here and also a field of Sale, so it is unclear which one is meant.\nWrite \"item.region\" for the field, or rename the variable."
    );
    // both ways out are fine
    assert_eq!(explain("  - set region = \"US\"\n  - set n = count of sales where item.region = \"EU\"\n"), vec![]);
    assert_eq!(explain("  - set country = \"US\"\n  - set n = count of sales where region = country\n"), vec![]);
    // outside a word the variable is simply the variable
    assert_eq!(explain("  - set region = \"US\"\n  - for each sale in sales:\n    - set where_from = region\n"), vec![]);
}

#[test]
fn append_put_and_remove_need_a_variable() {
    let note = "Name the value first with \"set\", change that, and use it afterwards.";
    says("  - put 1 at \"a\" in (map of \"b\" to 2)\n", 1, "map of \"b\" to 2", &format!("\"put\" changes a variable, but this is not one.\n{note}"));
    says("  - remove \"a\" from (map of \"b\" to 2)\n", 1, "map of \"b\" to 2", &format!("\"remove\" changes a variable, but this is not one.\n{note}"));
    says("  - append 1 to (list of 2)\n", 1, "list of 2", &format!("\"append\" changes a variable, but this is not one.\n{note}"));
    assert_eq!(explain("  - set xs = list of 2\n  - append 1 to xs\n"), vec![]);
}

/// Parse only (the 120-character line limit is a check, not a parse, rule).
fn parse_errors(src: &str) -> String {
    match parse_file("t.agent.md", src) {
        Ok(_) => String::new(),
        Err(d) => render_all(&d, src),
    }
}

#[test]
fn a_line_nested_too_deep_is_refused_not_a_crash() {
    let deep = format!("{}1{}", "(".repeat(150), ")".repeat(150));
    let e = parse_errors(&format!("# Function: f\n- out: int\n- effect: pure\n  - return {deep}\n"));
    assert!(e.contains("This line nests more than 100 levels of parentheses or braces."), "{e}");
    assert!(e.contains("Name the inner part with \"set\" on a line of its own."), "{e}");
    // 90 levels still parse
    let ok = format!("{}1{}", "(".repeat(90), ")".repeat(90));
    assert_eq!(parse_errors(&format!("# Function: f\n- out: int\n- effect: pure\n  - return {ok}\n")), "");
}

#[test]
fn nested_words_parse_in_linear_time() {
    // regression: `unique (…)`, `sum of (…)` and `count of (…)` parsed their operand twice per level (2^depth)
    let nest = |word: &str, depth: usize| (0..depth).fold("rows".to_string(), |e, _| format!("{word} ({e})"));
    for src in [
        format!("  - return {}", nest("unique", 20)),
        format!("  - return {}", nest("sum of", 20)),
        format!("  - return {}", nest("count of", 20)),
        format!("  - return {}", nest("min of", 20)),
    ] {
        let t = std::time::Instant::now();
        assert_eq!(parse_errors(&format!("# Function: f\n- effect: pure\n{src}\n")), "");
        assert!(t.elapsed().as_secs() < 10, "{src}: took {:?}", t.elapsed());
    }
}

#[test]
fn a_sum_followed_by_more_is_still_a_sum_of_its_source() {
    // regression of the single-pass operand: `sum of a + sum of b` is `(sum of a) + (sum of b)`
    assert_eq!(errors_of("# Function: f\n- in: xs: list of int, ys: list of int\n- out: int\n- effect: pure\n  - return sum of xs + sum of ys\n"), "");
    // and the element form still works with an operator inside it
    assert_eq!(errors_of("# Function: f\n- in: xs: list of int\n- out: int\n- effect: pure\n  - return sum of item * 2 in xs\n"), "");
}

#[test]
fn unused_loop_variables_and_element_names_build_warning_free() {
    let d = temp("quiet");
    let o = agentmark(&["build", &format!("{ROOT}/tests/collections/warnings"), "-o", s(&d.join("out")), "--no-check"]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    // build_bin fails the test on any rustc warning
    let bin = build_bin(&d.join("out"), "quiet");
    let fx = d.join("fx");
    write(&fx, "model.json", r#"{"today":"2026-10-14","responses":[]}"#);
    let o = run_bin(&bin, &fx, None);
    assert!(o.status.success(), "{}", stderr(&o));
    let lines: Vec<String> = parse_lines(&stdout(&o)).iter().map(|t| t["message"].as_str().unwrap().to_string()).collect();
    assert_eq!(
        lines,
        vec!["row", "row", "row", "entry a", "again", "again", "again", "3 3 3 1"]
    );
}
