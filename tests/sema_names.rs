//! v0.4 names that carry meaning (SPEC 23.6, 23.8, 23.10): the type of an input or output written
//! as a name (`- in: sales`), the record named last in a rule heading, and `file read … into`.

mod common;
use common::*;

use agentmark::project::Project;
use agentmark::sema;

const RECORDS: &str = "# Record: Sale\n- region: text\n- amount: money\n\n# Record: RegionTotal\n- region: text\n- revenue: money\n\n# Record: Category\n- name: text\n\n# Record: Box\n- id: text\n\n# Enum: Status\n- Open\n- Closed\n\n# Type: Totals\n- is: map of text to money\n\n";

/// The names and types of the inputs of a function written with `- in: {inputs}`.
fn ins(inputs: &str) -> Vec<(String, String)> {
    let src = format!("{RECORDS}# Function: f\n- in: {inputs}\n- out: int\n- touches: nothing\n- steps:\n  1. return 1\n");
    let p = project(&src);
    let c = sema::check(&p).unwrap_or_else(|d| panic!("{}", p.render(&d)));
    c.functions["f"].params.iter().map(|x| (x.name.clone(), x.ty.to_string())).collect()
}

// ------------------------------------------------------------------ names as types (23.6)

#[test]
fn a_singular_record_name_is_that_record() {
    assert_eq!(ins("sale"), [("sale".into(), "Sale".into())]);
    assert_eq!(ins("region total"), [("region_total".into(), "RegionTotal".into())]);
    assert_eq!(ins("Sale"), [("Sale".into(), "Sale".into())]);
}

#[test]
fn regular_plurals_are_lists() {
    // +s
    assert_eq!(ins("sales"), [("sales".into(), "list of Sale".into())]);
    // +es
    assert_eq!(ins("boxes"), [("boxes".into(), "list of Box".into())]);
    // y -> ies
    assert_eq!(ins("categories"), [("categories".into(), "list of Category".into())]);
}

#[test]
fn a_name_of_several_words_matches_the_longest_trailing_record() {
    assert_eq!(ins("region totals"), [("region_totals".into(), "list of RegionTotal".into())]);
    // `old` and `new` are only part of the name; the last word matches Sale
    assert_eq!(
        ins("old sales, new sales"),
        [("old_sales".into(), "list of Sale".into()), ("new_sales".into(), "list of Sale".into())]
    );
    // the longest ending wins: `RegionTotal`, not a record called `Total`
    let src = format!("{RECORDS}# Record: Total\n- n: int\n\n# Function: f\n- in: region totals\n- out: int\n- touches: nothing\n- steps:\n  1. return count of region_totals\n");
    let c = ok(&src);
    assert_eq!(c.functions["f"].params[0].ty.to_string(), "list of RegionTotal");
}

#[test]
fn case_and_underscores_do_not_matter_when_matching() {
    assert_eq!(ins("region_totals"), [("region_totals".into(), "list of RegionTotal".into())]);
    assert_eq!(ins("REGION totals"), [("REGION_totals".into(), "list of RegionTotal".into())]);
}

#[test]
fn enums_and_aliases_match_like_records() {
    assert_eq!(ins("status"), [("status".into(), "Status".into())]);
    assert_eq!(ins("totals"), [("totals".into(), "map of text to money".into())]);
    // `Totals` is the alias itself, so it is not made a list
    assert_eq!(ins("old status"), [("old_status".into(), "Status".into())]);
}

#[test]
fn the_last_word_decides_when_no_type_matches() {
    assert_eq!(
        ins("due_date, paid_at, price, retry_count, qty"),
        [
            ("due_date".into(), "date".into()),
            ("paid_at".into(), "date".into()),
            ("price".into(), "money".into()),
            ("retry_count".into(), "int".into()),
            ("qty".into(), "int".into()),
        ]
    );
}

#[test]
fn a_written_type_is_never_guessed() {
    // `sales` would be a list of Sale, but the author wrote a type
    assert_eq!(ins("sales: text"), [("sales".into(), "text".into())]);
    assert_eq!(ins("path: text, sale: list of Sale"), [("path".into(), "text".into()), ("sale".into(), "list of Sale".into())]);
}

#[test]
fn out_names_work_the_same_way() {
    for (written, want) in [("region totals", "list of RegionTotal"), ("sale", "Sale"), ("status", "Status")] {
        let src = format!("{RECORDS}# Function: f\n- in: x: {want}\n- out: {written}\n- touches: nothing\n- steps:\n  1. return x\n");
        let c = ok(&src);
        assert_eq!(c.functions["f"].out.as_ref().unwrap().to_string(), want);
    }
}

#[test]
fn a_name_that_tells_nothing_is_e230_in_an_input() {
    let src = "# Function: f\n- in: vat\n- out: int\n- touches: nothing\n- steps:\n  1. return 1\n";
    let e = first_error(src);
    assert!(e.contains("Could not tell what \"vat\" is. Write its type: \"vat: <type>\"."), "{e}");
    assert!(e.contains("A name works when it is a record's name (\"sales\" for a list of Sale) or ends in a word such as date, amount or count; otherwise write \"vat: text\" (or int, money, …)."), "{e}");
    assert!(e.starts_with("t.agent.md:2\n"), "{e}");
    // several words are joined with `_` in the advice
    let e = first_error("# Function: f\n- in: tax rate\n- out: int\n- touches: nothing\n- steps:\n  1. return 1\n");
    assert!(e.contains("Could not tell what \"tax rate\" is. Write its type: \"tax_rate: <type>\"."), "{e}");
}

#[test]
fn a_name_that_tells_nothing_is_e230_in_an_out_line() {
    let src = "# Function: f\n- in: n: int\n- out: verdict\n- touches: nothing\n- steps:\n  1. return n\n";
    let e = first_error(src);
    assert!(e.contains("Could not tell what \"verdict\" is. Write its type: \"- out: <type>\"."), "{e}");
    assert!(e.contains("Write the type instead, for example \"- out: text\" or \"- out: list of Sale\"."), "{e}");
}

#[test]
fn a_record_visible_only_through_uses_is_matched() {
    let p = Project::from_memory(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", "# Uses\n- [Sale](./sale.agent.md)\n\n# Function: total\nAdds up the sales.\n\n- in: sales\n- out: money\n- touches: nothing\n- steps:\n  1. return sum of amount in sales\n"),
            ("app/sale.agent.md", "# Record: Sale\n- region: text\n- amount: money\n"),
            ("app/tools.toml", ""),
        ],
    );
    let c = sema::check(&p).unwrap_or_else(|d| panic!("{}", p.render(&d)));
    assert_eq!(c.functions["total"].params[0].ty.to_string(), "list of Sale");
    // without the `# Uses` line the file cannot see the record, so the name tells nothing
    let p = Project::from_memory(
        "app/main.agent.md",
        vec![
            ("app/main.agent.md", "# Function: total\nAdds up the sales.\n\n- in: sales\n- out: int\n- touches: nothing\n- steps:\n  1. return 1\n"),
            ("app/sale.agent.md", "# Record: Sale\n- region: text\n- amount: money\n"),
            ("app/tools.toml", ""),
        ],
    );
    let e = p.render(&sema::check(&p).unwrap_err());
    assert!(e.contains("Could not tell what \"sales\" is."), "{e}");
}

// ------------------------------------------------------------------ rules (23.8)

#[test]
fn a_rule_names_its_record_last_in_the_heading_in_any_case() {
    let src = format!("{RECORDS}# Rule: big Sale\nA sale of 100 EUR or more.\n\n- when: amount >= 100 EUR\n\n# Rule: small sale\nA small sale.\n\n- when: amount < 10 EUR\n\n# Rule: LARGE SALE\nA large sale.\n\n- when: amount >= 1000 EUR\n");
    let c = project_checked(&src);
    for name in ["big", "small", "LARGE"] {
        assert!(c.declarations.iter().any(|d| d.name == name && d.kind == agentmark::sema::Kind::Rule), "{name}");
    }
}

#[test]
fn a_rule_name_may_have_several_words() {
    let src = format!("{RECORDS}# Rule: very big Sale\nA very big sale.\n\n- when: amount >= 1000 EUR\n\n# Function: count_them\nCounts them.\n\n- in: sales\n- out: int\n- touches: nothing\n- steps:\n  1. for each very_big sale in sales:\n     1. sale.region into r\n  2. return 1\n");
    let c = project_checked(&src);
    assert!(c.declarations.iter().any(|d| d.name == "very_big" && d.kind == agentmark::sema::Kind::Rule));
}

#[test]
fn a_rule_heading_without_a_record_says_so() {
    let e = first_error(&format!("{RECORDS}# Rule: big\nA sale.\n\n- when: amount >= 100 EUR\n"));
    assert!(e.contains("big"), "{e}");
    let e = first_error(&format!("{RECORDS}# Rule: big Sael\nA sale.\n\n- when: amount >= 100 EUR\n"));
    assert!(e.contains("Sael"), "{e}");
}

/// The check passes (unused declarations of the shared records are only warnings in a real run;
/// here they are listed with the errors, so the unused ones are left out of the judgement).
fn ok_loose(src: &str) -> agentmark::sema::Checked {
    let p = project(src);
    match sema::check(&p) {
        Ok(c) => c,
        Err(d) => {
            let real: Vec<_> = d.iter().filter(|x| x.severity == agentmark::diag::Severity::Error).cloned().collect();
            assert!(real.is_empty(), "{}", p.render(&real));
            panic!("unexpected: {}", p.render(&d));
        }
    }
}

fn project_checked(src: &str) -> agentmark::sema::Checked {
    // rules nobody uses are only warnings
    ok(src)
}

// ------------------------------------------------------------------ typed file read (23.10)

fn read_src(into: &str, shown: &str) -> String {
    format!("{RECORDS}# Command: c\nReads a file.\n\n- in: path: text\n- steps:\n  1. try file read path into {into}\n     otherwise:\n     1. stop\n  2. print: \"{{{shown}}}\"\n")
}

fn read_type(into: &str, name: &str) -> String {
    // a list can only be counted, text can be shown
    let is_list = matches!(name, "sales" | "region_totals" | "categories");
    let shown = if is_list || into.contains("list of") { format!("count of {name}") } else { name.to_string() };
    let src = format!("{RECORDS}# Command: c\nReads a file.\n\n- in: path: text\n- steps:\n  1. try file read path into {into}\n     otherwise:\n     1. stop\n  2. print: \"{{{shown}}}\"\n");
    let c = ok_loose(&src);
    // a list of records is recorded as a CSV read
    c.file_reads.values().next().map(|t| t.to_string()).unwrap_or_else(|| "text".into())
}

#[test]
fn the_plural_of_a_record_reads_a_csv_into_a_list() {
    assert_eq!(read_type("sales", "sales"), "list of Sale");
    assert_eq!(read_type("region_totals", "region_totals"), "list of RegionTotal");
    assert_eq!(read_type("categories", "categories"), "list of Category");
}

#[test]
fn a_singular_or_unmatched_name_is_the_raw_text() {
    assert_eq!(read_type("sale", "sale"), "text");
    assert_eq!(read_type("content", "content"), "text");
    assert_eq!(read_type("rows", "rows"), "text");
}

#[test]
fn a_type_after_the_name_is_taken_as_written() {
    assert_eq!(read_type("content: text", "content"), "text");
    assert_eq!(read_type("sales: list of Sale", "sales"), "list of Sale");
    // the name would have been a list; the written type wins
    let src = read_src("sales: text", "sales");
    ok_loose(&src);
}

#[test]
fn a_wrong_written_type_is_an_error() {
    let e = first_error(&read_src("n: int", "n"));
    assert!(e.contains("A file is read as text or as a list of records, not as a number."), "{e}");
    assert!(e.contains("Write \"into content: text\", or \"into sales: list of Sale\" for a CSV file."), "{e}");
    let e = first_error(&read_src("rows: list of text", "count of rows"));
    assert!(e.contains("A file is read as text or as a list of records, not as list of text."), "{e}");
}

#[test]
fn a_type_after_into_on_another_verb_is_an_error() {
    let src = format!("{RECORDS}# Command: c\nLists a folder.\n\n- in: path: text\n- steps:\n  1. file list path into names: list of text\n  2. print: \"{{count of names}}\"\n");
    let e = first_error(&src);
    assert!(e.contains("Only \"file read\" takes a type after the name; \"file list\" gives list of text."), "{e}");
    assert!(e.contains("Write it like \"file list <folder> into <names>\"."), "{e}");
}
