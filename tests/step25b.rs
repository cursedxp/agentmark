//! Step 25b: gaps found while the v0.3 examples ran end to end — negative numbers, collections in
//! text, `expect_status` in `requests.json`, imports of generated code.

mod support;

use std::path::Path;
use std::process::Command;

use agentmark::diag::Severity;
use agentmark::fmt::{format_source, is_canonical};
use agentmark::project::Project;
use agentmark::sema;

use support::*;

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

fn command(body: &str) -> String {
    format!("# Command: c\n- args: n: int = 3\n\n## Run\n{body}\n")
}

#[test]
fn a_list_a_map_or_a_record_cannot_be_written_into_text() {
    let e = errors_of(&command("- set xs = list of 1, 2, 3\n- print: \"{xs}\""));
    assert!(e.contains("A value of type list of int cannot be written into text as a whole."), "{e}");
    assert!(e.contains("join"), "{e}");
    let e = errors_of(&command("- set m = map of \"a\" to 1\n- print: \"{m}\""));
    assert!(e.contains("A value of type map of text to int cannot be written into text as a whole."), "{e}");
    assert!(e.contains("for each key, value"), "{e}");
    let src = "# Record: Sale\n- region: text\n\n".to_string() + &command("- set s = Sale { region: \"EU\" }\n- print: \"{s}\"");
    let e = errors_of(&src);
    assert!(e.contains("cannot be written into text as a whole"), "{e}");
    // what is allowed stays allowed
    assert_eq!(errors_of(&command("- set xs = list of \"a\", \"b\"\n- set joined = join xs with \", \"\n- print: \"{joined} {count of xs}\"")), "");
}

#[test]
fn negative_numbers_parse_check_run_and_format() {
    let src = command(
        "- set neg = -5\n- set price = -12.50 EUR\n- set r = number.round(1234, -2)\n- set back = -n\n- set grouped = -(n + 1)\n- set twice = -(-n)\n- set explicit = n * -1\n- set both = neg - -n\n- print: \"{neg} {price} {r} {back} {grouped} {twice} {explicit} {both} {n - 1}\"",
    );
    assert_eq!(errors_of(&src), "");
    let formatted = format_source(&src, "t.agent.md").unwrap();
    // the source form of a negation survives `fmt`; a written `n * -1` stays as written (Q419)
    for kept in ["= -n\n", "= -(n + 1)\n", "= -(-n)\n", "= n * -1\n", "= neg - -n\n"] {
        assert!(formatted.contains(kept), "{kept:?} in {formatted}");
    }
    assert!(formatted.contains("= -5") && formatted.contains("= -12.50 EUR") && formatted.contains("-2)"), "{formatted}");
    assert!(is_canonical(&formatted, "t.agent.md").unwrap());
    // `a - 1` is still a subtraction, `a -1` is not accepted as one
    assert!(errors_of(&command("- set k = n -1\n- print: \"{k}\"")).len() > 0);

    let d = temp("neg");
    write(&d, "agentmark.toml", "name = \"neg\"\nversion = \"0.1.0\"\ntargets = [\"rust\"]\n");
    write(&d, "main.agent.md", "# Import: ./commands/c.agent.md\n\n# Run\n- c\n");
    write(&d, "commands/c.agent.md", &src);
    let o = agentmark(&["build", s(&d), "-o", s(&d.join("out")), "--no-check"]).output_locked();
    assert!(o.status.success(), "{}", stderr(&o));
    let bin = build_bin(&d.join("out"), "neg");
    let run = |args: &[&str]| Command::new(&bin).args(args).output().unwrap();
    assert_eq!(stdout(&run(&[])), "-5 -12.50 EUR 1200 -3 -4 3 -3 -2 2\n");
    assert_eq!(stdout(&run(&["--n", "-7"])), "-5 -12.50 EUR 1200 7 6 -7 7 -12 -8\n");
}

#[test]
fn a_request_may_expect_a_status() {
    let d = temp("status");
    let service = format!("{ROOT}/tests/collections/service");
    let o = agentmark(&["build", &service, "-o", s(&d.join("out")), "--no-check"]).output_locked();
    assert!(o.status.success(), "{}", stderr(&o));
    let bin = build_bin(&d.join("out"), "tally");
    let fx = d.join("fx");
    copy_dir(Path::new(&format!("{ROOT}/tests/collections/service-fx")), &fx);
    let run = |requests: &str| {
        write(&fx, "requests.json", requests);
        Command::new(&bin).current_dir(&fx).args(["--once", "--mock", "--fixtures", "."]).output().unwrap()
    };
    let good = run(r#"[{"method":"POST","path":"/count/zoe","expect_status":200},{"method":"GET","path":"/nowhere","expect_status":404}]"#);
    assert!(good.status.success(), "{}", stderr(&good));
    let bad = run(r#"[{"method":"POST","path":"/count/zoe","expect_status":201}]"#);
    assert_eq!(bad.status.code(), Some(1), "{}", stderr(&bad));
    assert!(stderr(&bad).contains("expected status 201, got 200"), "{}", stderr(&bad));
}
