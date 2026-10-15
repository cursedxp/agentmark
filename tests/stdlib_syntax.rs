//! Syntax of the built-in library (SPEC 22.3): the parser reads `module.fn(args)`, `json.parse(t) as T`,
//! `try [do] <call> -> var`, `file <verb> …` and `read input -> t`; the formatter prints them back the way
//! they were written and is idempotent; the linter walks into them.

use agentmark::diag::render_all;
use agentmark::fmt::format_source;
use agentmark::syntax::ast::*;
use agentmark::syntax::parser::parse_file;

fn parse_body(stmts: &str) -> Vec<Stmt> {
    let src = format!("# Command: c\nDoes things.\n\n- in: p: text\n- steps:\n{stmts}");
    match parse_file("t.agent.md", &src) {
        Ok(f) => match f.decls.into_iter().next() {
            Some(Decl::Command(c)) => c.body,
            other => panic!("not a command: {other:?}"),
        },
        Err(d) => panic!("did not parse:\n{}", render_all(&d, &src)),
    }
}

fn first(stmts: &str) -> StmtKind {
    parse_body(stmts).remove(0).kind
}

fn fails(stmts: &str) -> bool {
    let src = format!("# Command: c\nDoes things.\n\n- in: p: text\n- steps:\n{stmts}");
    parse_file("t.agent.md", &src).is_err()
}

#[test]
fn a_module_call_is_a_dotted_call() {
    let StmtKind::Set { expr, .. } = first("  1. text.split(p, \",\") into x\n") else { panic!() };
    let ExprKind::Call { func, args } = expr.kind else { panic!() };
    assert_eq!(func.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["text", "split"]);
    assert_eq!(args.len(), 2);
}

#[test]
fn as_follows_only_json_parse_and_csv_parse() {
    let StmtKind::Set { expr, .. } = first("  1. json.parse(p) as list of Note into x\n") else { panic!() };
    let ExprKind::Decode { call, ty } = expr.kind else { panic!("{expr:?}") };
    assert!(matches!(call.kind, ExprKind::Call { .. }));
    assert!(matches!(ty.kind, TypeKind::List(_)));
    assert!(fails("  1. text.trim(p) as text into x\n"));
}

#[test]
fn try_takes_a_library_call_with_or_without_do() {
    for (src, via_do) in [("  1. try number.parse(p) into n\n", false), ("  1. try do number.parse(p) into n\n", true)] {
        let StmtKind::Try { stmt, otherwise } = first(src) else { panic!() };
        assert!(otherwise.is_none());
        let StmtKind::CallInto { into, via_do: got, .. } = stmt.kind else { panic!() };
        assert_eq!((into.name.as_str(), got), ("n", via_do));
    }
    let StmtKind::Try { stmt, .. } = first("  1. try csv.parse(p) as list of Sale into rows\n") else { panic!() };
    let StmtKind::CallInto { call, .. } = stmt.kind else { panic!() };
    assert!(matches!(call.kind, ExprKind::Decode { .. }));
    // a user function after `do` is still a `do` statement
    let StmtKind::Try { stmt, .. } = first("  1. try do mine(p) into n\n") else { panic!() };
    assert!(matches!(stmt.kind, StmtKind::Do { .. }));
    // a library call with nothing to store is not a statement
    assert!(fails("  1. number.parse(p)\n"));
}

#[test]
fn file_statements_take_expressions() {
    let StmtKind::File { verb, path, arg, into, .. } = first("  1. file rename path.join(p, \"a\") to path.join(p, \"b\")\n") else { panic!() };
    assert_eq!(verb.name, "rename");
    assert!(matches!(path.kind, ExprKind::Call { .. }));
    assert!(matches!(arg, Some(FileArg::To(_))) && into.is_none());
    let StmtKind::File { arg, .. } = first("  1. file write \"out.txt\": \"{p}\\n\"\n") else { panic!() };
    assert!(matches!(arg, Some(FileArg::Content(_))));
    let StmtKind::Try { stmt, .. } = first("  1. try file read p into content\n") else { panic!() };
    let StmtKind::File { into: Some(i), .. } = stmt.kind else { panic!() };
    assert_eq!(i.name, "content");
    let StmtKind::File { into: Some(_), arg: None, .. } = first("  1. file list p into names\n") else { panic!() };
    assert!(fails("  1. file\n"));
}

#[test]
fn read_input_stores_text() {
    let StmtKind::ReadInput { into } = first("  1. read input into typed\n") else { panic!() };
    assert_eq!(into.name, "typed");
    // the older forms are untouched
    assert!(matches!(first("  1. read file p as text into t\n"), StmtKind::ReadFile { .. }));
}

#[test]
fn the_formatter_keeps_the_spelling_and_is_idempotent() {
    let src = "# Command: c\nDoes things.\n\n- in: p: text\n- steps:\n  1. try file read p into content\n  2. try do csv.parse(content) as list of Sale into rows\n  3. try number.parse(p) into n\n  4. read input into typed\n  5. file write path.join(p, \"o.txt\"): json.encode(rows)\n  6. file rename p to \"x\"\n  7. json.parse(typed) as Note into d\n";
    let once = format_source(src, "t.agent.md").unwrap_or_else(|d| panic!("{}", render_all(&d, src)));
    assert_eq!(once, src, "already canonical");
    let messy = src.replace("6. file rename p to \"x\"", "6.   file   rename   p   to   \"x\"").replace("as list of Sale", "as list  of  Sale");
    assert_eq!(format_source(&messy, "t.agent.md").unwrap(), src);
}

#[test]
fn the_linter_sees_names_introduced_by_the_new_statements() {
    // `data` is a vague name: it must be found whichever statement introduces it
    for stmt in ["  1. read input into data\n", "  1. try number.parse(p) into data\n", "  1. file list p into data\n"] {
        let src = format!("# Command: c\nDoes things.\n\n- in: p: text\n- steps:\n{stmt}  2. print: \"{{data}}\"\n");
        let file = parse_file("t.agent.md", &src).unwrap();
        let findings = agentmark::lint::lint_file(&file, &agentmark::lint::LintConfig::default());
        assert!(findings.iter().any(|d| d.code == "vague-name"), "{stmt}: {findings:?}");
    }
}

#[test]
fn the_reference_lists_every_function_of_the_table_and_every_file_verb() {
    let reference = std::fs::read_to_string("docs/reference.md").unwrap();
    let start = reference.find("### 22.3").expect("docs/reference.md has a 22.3 section");
    let section = &reference[start..];
    let section = &section[..section.find("\n## Appendix").unwrap_or(section.len())];
    for f in agentmark::stdlib::FUNCS {
        let ps: Vec<&str> = f.params.iter().map(|p| p.0).collect();
        let sig = format!("{}({})", f.name, ps.join(", "));
        assert!(section.contains(&sig), "the 22.3 table in docs/reference.md lacks {}.{sig}", f.module);
    }
    for m in agentmark::stdlib::MODULES {
        assert!(section.contains(&format!("| `{m}` |")), "module {m} is not in the table");
    }
    for v in agentmark::stdlib::FILE_VERBS {
        assert!(section.contains(&format!("`file {} ", v.verb)), "file {} is not in the verb table", v.verb);
    }
}
