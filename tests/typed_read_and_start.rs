//! SPEC 23.10 and 23.4, end to end: `file read … into <records>` reads a CSV file into records, and
//! a program with exactly one command runs it without a subcommand name (with several commands, also
//! when `- start:` names one, the commands stay subcommands, Q441). The real `agentmark` builds the
//! programs, cargo compiles them, and the traces and outputs are compared with what is written here.

mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};

use support::*;

const TYPED_READ: &str = r#"# Program: reader
Reads files into records and into text.

# Record: Sale
One line of a sales file.

- region: text
- amount: money

# Command: sales
Prints the sales of a CSV file.

- in: path: text
- steps:
  1. try file read path into sales
     otherwise:
     1. print: "Could not read {path}"
     2. stop
  2. for each sale in sales:
     1. print: "{sale.region} {sale.amount}"
  3. print: "{count of sales} sales"

# Command: content
Prints how long a file is, read as text.

- in: path: text
- steps:
  1. try file read path into content
     otherwise:
     1. print: "Could not read {path}"
     2. stop
  2. print: "{text.length(content)} characters"
"#;

fn build(tag: &str, source: &str) -> (Tmp, PathBuf) {
    let src = temp(&format!("{tag}-src"));
    // the file's name is the program's package name: distinct per test, because the tests share cargo's target folder
    let name = tag.replace('-', "_");
    write(&src, &format!("{name}.agent.md"), source);
    let out = temp(&format!("{tag}-out"));
    let o = agentmark(&["build", s(&src.join(format!("{name}.agent.md"))), "--target", "rust", "-o", s(&out)]).output_locked();
    assert!(o.status.success(), "agentmark build failed:\n{}", stderr(&o));
    let bin = build_bin(&out, &name);
    (out, bin)
}

fn run_in(bin: &Path, work: &Path, args: &[&str]) -> Output {
    Command::new(bin).args(["--mock", "--fixtures", ".", "--trace", "-"]).args(args).current_dir(work).output().unwrap()
}

fn work_with(files: &[(&str, &str)]) -> Tmp {
    let d = temp("typed-work");
    write(&d, "model.json", r#"{"today": "2026-10-14", "responses": []}"#);
    for (name, text) in files {
        write(&d, name, text);
    }
    d
}

fn read(path: &str) -> Value {
    json!({"kind": "file", "verb": "read", "path": path})
}

fn print(m: &str) -> Value {
    json!({"kind": "print", "message": m})
}

#[test]
fn a_csv_file_is_read_into_records_and_everything_else_goes_to_otherwise() {
    let (_out, bin) = build("typed-read", TYPED_READ);
    let work = work_with(&[
        ("sales.csv", "region,amount\nEU,120.00 EUR\nUS,5.50 EUR\n"),
        ("UPPER.CSV", "region,amount\nAsia,1.00 EUR\n"),
        ("sales.txt", "region,amount\nEU,120.00 EUR\n"),
        ("broken.csv", "region\nEU\n"),
        ("not-money.csv", "region,amount\nEU,plenty\n"),
    ]);

    // a `.csv` file: its header row names the fields; the read is traced like any file read
    let o = run_in(&bin, &work, &["sales", "sales.csv"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        parse_lines(&stdout(&o)),
        [read("sales.csv"), print("EU 120.00 EUR"), print("US 5.50 EUR"), print("2 sales")]
    );
    // the extension is matched in any case
    let o = run_in(&bin, &work, &["sales", "UPPER.CSV"]);
    assert_eq!(parse_lines(&stdout(&o)), [read("UPPER.CSV"), print("Asia 1.00 EUR"), print("1 sales")]);

    // a name that does not end in `.csv`, a file that does not exist, a CSV with a missing column and
    // one with a value of the wrong type all take the same `otherwise`; the read is still traced
    for bad in ["sales.txt", "missing.csv", "broken.csv", "not-money.csv"] {
        let o = run_in(&bin, &work, &["sales", bad]);
        assert!(o.status.success(), "{bad}: {}", stderr(&o));
        assert_eq!(parse_lines(&stdout(&o)), [read(bad), print(&format!("Could not read {bad}"))], "{bad}");
    }

    // `into content` is not a record: the file is text, whatever its name
    for (file, chars) in [("sales.csv", 40), ("sales.txt", 28)] {
        let o = run_in(&bin, &work, &["content", file]);
        assert!(o.status.success(), "{}", stderr(&o));
        assert_eq!(parse_lines(&stdout(&o)), [read(file), print(&format!("{chars} characters"))], "{file}");
    }
    let o = run_in(&bin, &work, &["content", "missing.txt"]);
    assert_eq!(parse_lines(&stdout(&o)), [read("missing.txt"), print("Could not read missing.txt")]);
}

const ONE_COMMAND: &str = "# Program: greeter\nSays hello.\n\n# Command: greet\nGreets someone.\n\n- in: who: text = \"world\"\n- steps:\n  1. print: \"hello {who}\"\n";

#[test]
fn a_program_with_exactly_one_command_runs_it_without_a_subcommand_name() {
    let (_out, bin) = build("start-one", ONE_COMMAND);
    let work = work_with(&[]);
    let o = run_in(&bin, &work, &["Ada"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(parse_lines(&stdout(&o)), [print("hello Ada")]);
    let o = run_in(&bin, &work, &[]);
    assert_eq!(parse_lines(&stdout(&o)), [print("hello world")]);
    let o = run_in(&bin, &work, &["--who", "Bob"]);
    assert_eq!(parse_lines(&stdout(&o)), [print("hello Bob")]);
}

const START_NAMES_ONE: &str = "# Program: tools\nTwo small commands.\n\n- start: [greet](#command-greet)\n\n# Command: greet\nGreets someone.\n\n- in: who: text\n- steps:\n  1. print: \"hello {who}\"\n\n# Command: leave\nSays goodbye.\n\n- in: who: text\n- steps:\n  1. print: \"bye {who}\"\n";

#[test]
fn start_with_several_commands_keeps_the_subcommands() {
    let (_out, bin) = build("start-many", START_NAMES_ONE);
    let work = work_with(&[]);
    let o = run_in(&bin, &work, &["greet", "Ada"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(parse_lines(&stdout(&o)), [print("hello Ada")]);
    let o = run_in(&bin, &work, &["leave", "Ada"]);
    assert_eq!(parse_lines(&stdout(&o)), [print("bye Ada")]);
    // no subcommand name: a usage error, nothing runs (Q441: `start:` does not pick a default yet)
    let o = run_in(&bin, &work, &["Ada"]);
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert!(stdout(&o).is_empty(), "{}", stdout(&o));
}

#[test]
fn start_must_name_a_command_of_the_program() {
    let src = temp("start-bad");
    write(&src, "main.agent.md", &ONE_COMMAND.replace("Says hello.\n", "Says hello.\n\n- start: [nothing](#command-nothing)\n"));
    let o = agentmark(&["check", s(&src.join("main.agent.md"))]).output_locked();
    assert_eq!(o.status.code(), Some(1), "{}{}", stdout(&o), stderr(&o));
    assert!(stderr(&o).contains("E214") || stderr(&o).contains("nothing"), "{}", stderr(&o));
}
