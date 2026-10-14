//! Tools written in AgentMark and the default command, end to end (SPEC 22.5, 22.6):
//! `tests/user_tools/shop/` has no `tools.toml` and no `rust/connect.rs`. The real `agentmark` builds it, cargo
//! compiles it, and the program runs in a fresh working directory with its command's arguments given
//! positionally, as `--name value` and as a flag. What it traces and writes is compared with what is written
//! down here.

mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};

use support::*;

/// The project is copied first, so nothing a build creates lands in the repository. Returns the
/// source copy, the generated project and the program.
fn build_shop() -> (Tmp, Tmp, PathBuf) {
    let src = temp("shop-src");
    copy_dir(Path::new(&format!("{ROOT}/tests/user_tools/shop")), &src);
    let out = temp("shop-out");
    let o = agentmark(&["build", s(&src), "--target", "rust", "-o", s(&out)]).output_locked();
    assert!(o.status.success(), "agentmark build failed:\n{}", stderr(&o));
    assert!(!src.join("rust").exists(), "a project without external tools gets no rust/connect.rs");
    let bin = build_bin(&out, "shop");
    (src, out, bin)
}

fn run_in(bin: &Path, work: &Path, args: &[&str]) -> Output {
    Command::new(bin).args(["--trace", "-"]).args(args).current_dir(work).output().unwrap()
}

fn tool(tool: &str, verb: &str, args: Value) -> Value {
    json!({"kind": "tool", "tool": tool, "verb": verb, "args": args})
}

fn append() -> Value {
    json!({"kind": "file", "verb": "append", "path": "ledger.txt"})
}

#[test]
fn the_shop_builds_without_tools_toml_and_runs_its_default_command() {
    let (_src, _out, bin) = build_shop();

    // positional arguments, in declared order
    let work = temp("shop-work");
    let o = run_in(&bin, &work, &["Hello World", "2"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        parse_lines(&stdout(&o)),
        [
            tool("slugger", "slug", json!({"title": "Hello World"})),
            tool("slugger", "shout", json!({"word": "hello-world"})),
            tool("ledger", "add", json!({"entry": "HELLO-WORLD x2"})),
            append(),
            tool("ledger", "add_twice", json!({"entry": "hello-world", "mark": "again"})),
            tool("ledger", "add", json!({"entry": "hello-world"})),
            append(),
            tool("ledger", "add", json!({"entry": "again hello-world"})),
            append(),
            json!({"kind": "print", "message": "wrote hello-world"}),
        ]
    );
    assert_eq!(
        std::fs::read_to_string(work.join("ledger.txt")).unwrap(),
        "HELLO-WORLD x2\nhello-world\nagain hello-world\n"
    );

    // `--name value`, in any order, and the default of `count`
    let work = temp("shop-work");
    let o = run_in(&bin, &work, &["--count", "5", "--title", "A  b!"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(std::fs::read_to_string(work.join("ledger.txt")).unwrap(), "A-B- x5\na-b-\nagain a-b-\n");
    let work = temp("shop-work");
    assert!(run_in(&bin, &work, &["Menu"]).status.success());
    assert_eq!(std::fs::read_to_string(work.join("ledger.txt")).unwrap(), "MENU x1\nmenu\nagain menu\n");

    // a mix of both, and the flag
    let work = temp("shop-work");
    let o = run_in(&bin, &work, &["Menu", "--dry-run"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        parse_lines(&stdout(&o)),
        [tool("slugger", "slug", json!({"title": "Menu"})), json!({"kind": "print", "message": "would write menu"})]
    );
    assert!(!work.join("ledger.txt").exists(), "a dry run writes nothing");
    let work = temp("shop-work");
    assert!(run_in(&bin, &work, &["--dry_run", "--title", "x"]).status.success(), "the underscore spelling works too");

    // a missing argument is a usage error, not a crash
    let work = temp("shop-work");
    let o = run_in(&bin, &work, &[]);
    assert_eq!(o.status.code(), Some(2));
    let e = stderr(&o);
    assert!(e.contains("the argument <TITLE> is required"), "{e}");
    assert!(e.contains("Usage:"), "{e}");
    assert!(stdout(&o).is_empty());
}

#[test]
fn agentmark_run_passes_the_arguments_after_the_double_dash() {
    let src = temp("shop-run");
    copy_dir(Path::new(&format!("{ROOT}/tests/user_tools/shop")), &src);
    let work = temp("shop-run-work");
    let o = agentmark(&["trace", s(&src), "--", "Ünï Code", "--count", "3"]).current_dir(&work).output_locked();
    assert!(o.status.success(), "{}", stderr(&o));
    let trace = parse_lines(&stdout(&o));
    assert_eq!(trace.first().unwrap(), &tool("slugger", "slug", json!({"title": "Ünï Code"})));
    assert_eq!(trace.last().unwrap()["kind"], "print");
    assert_eq!(std::fs::read_to_string(work.join("ledger.txt")).unwrap().lines().next().unwrap(), "-N-CODE x3");
    assert!(!src.join("rust").exists());
}

#[test]
fn a_verb_that_can_fail_is_called_with_try() {
    let src = temp("doubler-src");
    copy_dir(Path::new(&format!("{ROOT}/tests/user_tools/doubler")), &src);
    let out = temp("doubler-out");
    let o = agentmark(&["build", s(&src), "--target", "rust", "-o", s(&out)]).output_locked();
    assert!(o.status.success(), "agentmark build failed:\n{}", stderr(&o));
    let bin = build_bin(&out, "doubler");
    let work = temp("doubler-work");
    let o = run_in(&bin, &work, &["21"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        parse_lines(&stdout(&o)),
        [tool("conv", "double", json!({"word": "21"})), json!({"kind": "print", "message": "double 42"})]
    );
    let o = run_in(&bin, &work, &["abc"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        parse_lines(&stdout(&o)),
        [tool("conv", "double", json!({"word": "abc"})), json!({"kind": "print", "message": "not a number: abc"})]
    );
}

#[test]
fn positional_and_named_values_mix_and_wrong_ones_are_usage_errors() {
    let (_src, _out, bin) = build_shop();
    // a named argument leaves the positional values to the others, in declared order
    let work = temp("shop-mix");
    assert!(run_in(&bin, &work, &["--title", "Named First", "4"]).status.success());
    assert_eq!(std::fs::read_to_string(work.join("ledger.txt")).unwrap(), "NAMED-FIRST x4\nnamed-first\nagain named-first\n");
    let work = temp("shop-mix");
    assert!(run_in(&bin, &work, &["Hello", "--count", "7"]).status.success());
    assert!(std::fs::read_to_string(work.join("ledger.txt")).unwrap().starts_with("HELLO x7\n"));

    // too many values, or one that is given by name and by position: a usage error, nothing runs
    for args in [&["a", "2", "3"][..], &["Hello", "2", "--count", "3"], &["--title", "Other", "2", "x"]] {
        let work = temp("shop-bad");
        let o = run_in(&bin, &work, args);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(stderr(&o).contains("unexpected value"), "{args:?}: {}", stderr(&o));
        assert!(stderr(&o).contains("Usage:"), "{}", stderr(&o));
        assert!(!work.join("ledger.txt").exists(), "{args:?}");
    }
    // a value of the wrong type
    let work = temp("shop-bad");
    let o = run_in(&bin, &work, &["Hello", "many"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("\"many\" is not a valid value for <COUNT>"), "{}", stderr(&o));
}

#[test]
fn a_default_command_without_required_arguments_recursion_and_aliased_tools_build_warning_free() {
    let src = temp("calc-src");
    copy_dir(Path::new(&format!("{ROOT}/tests/user_tools/calc")), &src);
    let out = temp("calc-out");
    let o = agentmark(&["build", s(&src), "--target", "rust", "-o", s(&out)]).output_locked();
    assert!(o.status.success(), "agentmark build failed:\n{}", stderr(&o));
    let bin = build_bin(&out, "calc"); // asserts: no warning, so no unused `missing_argument`
    let work = temp("calc-work");
    let o = run_in(&bin, &work, &[]);
    assert!(o.status.success(), "{}", stderr(&o));
    let prints: Vec<String> = parse_lines(&stdout(&o)).iter().filter(|l| l["kind"] == "print").map(|l| l["message"].as_str().unwrap().to_string()).collect();
    assert_eq!(prints, ["n! = 120", "120"]);
    let o = run_in(&bin, &work, &["6", "six"]);
    let prints: Vec<String> = parse_lines(&stdout(&o)).iter().filter(|l| l["kind"] == "print").map(|l| l["message"].as_str().unwrap().to_string()).collect();
    assert_eq!(prints, ["six! = 720", "720"]);
    // the recursive verb is traced at every level
    let trace = parse_lines(&stdout(&run_in(&bin, &work, &["3"])));
    let calls: Vec<&Value> = trace.iter().filter(|l| l["kind"] == "tool" && l["tool"] == "maths").collect();
    assert_eq!(calls.iter().map(|l| l["args"]["n"].as_i64().unwrap()).collect::<Vec<_>>(), [3, 2, 1]);
}

#[test]
fn arguments_named_a_and_rest_and_money_and_date_arguments_work() {
    let src = temp("odd-src");
    copy_dir(Path::new(&format!("{ROOT}/tests/user_tools/odd")), &src);
    let out = temp("odd-out");
    let o = agentmark(&["build", s(&src), "--target", "rust", "-o", s(&out)]).output_locked();
    assert!(o.status.success(), "agentmark build failed:\n{}", stderr(&o));
    let bin = build_bin(&out, "odd");
    let work = temp("odd-work");
    let say = |args: &[&str]| {
        let o = run_in(&bin, &work, args);
        assert!(o.status.success(), "{args:?}: {}", stderr(&o));
        parse_lines(&stdout(&o))[0]["message"].as_str().unwrap().to_string()
    };
    assert_eq!(say(&["x", "y"]), "x y 1.50 EUR 2026-10-14");
    assert_eq!(say(&["x", "y", "--price", "3.00 EUR", "--day", "2026-01-02"]), "x y 3.00 EUR 2026-01-02");
    assert_eq!(say(&["--rest", "r", "--a", "q", "12.25 EUR"]), "q r 12.25 EUR 2026-10-14");
    let o = run_in(&bin, &work, &["x", "y", "not money"]);
    assert_eq!(o.status.code(), Some(2));

    // the same types as arguments of a subcommand (several commands)
    let src = temp("odd2-src");
    write(&src, "agentmark.toml", "name = \"odd2\"\nversion = \"0.1.0\"\ntargets = [\"rust\"]\n");
    write(&src, "main.agent.md", "# Import: ./one.agent.md\n# Import: ./two.agent.md\n\n# Run\n- one\n- two\n");
    write(&src, "one.agent.md", "# Command: one\n- args: price: money, day: date\n\n## Run\n- print: \"{price} {day}\"\n");
    write(&src, "two.agent.md", "# Command: two\n\n## Run\n- print: \"two\"\n");
    let out = temp("odd2-out");
    let o = agentmark(&["build", s(&src), "--target", "rust", "-o", s(&out)]).output_locked();
    assert!(o.status.success(), "{}", stderr(&o));
    let bin = build_bin(&out, "odd2");
    let o = run_in(&bin, &work, &["one", "2.00 EUR", "2026-02-03"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(parse_lines(&stdout(&o))[0]["message"], "2.00 EUR 2026-02-03");
}
