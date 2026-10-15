//! Step 25b: gaps found while the v0.3 examples ran end to end — negative numbers, collections in
//! text, `expect_status` in `requests.json`, imports of generated code.

mod support;

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
    format!("# Command: c\nDoes things.\n\n- in: n: int = 3\n- steps:\n{body}\n")
}

#[test]
fn a_list_a_map_or_a_record_cannot_be_written_into_text() {
    let e = errors_of(&command("  1. list of 1, 2, 3 into xs\n  2. print: \"{xs}\""));
    assert!(e.contains("A value of type list of int cannot be written into text as a whole."), "{e}");
    assert!(e.contains("join"), "{e}");
    let e = errors_of(&command("  1. map of \"a\" to 1 into m\n  2. print: \"{m}\""));
    assert!(e.contains("A value of type map of text to int cannot be written into text as a whole."), "{e}");
    assert!(e.contains("for each key, value"), "{e}");
    let src = "# Record: Sale\n- region: text\n\n".to_string() + &command("  1. Sale { region: \"EU\" } into s\n  2. print: \"{s}\"");
    let e = errors_of(&src);
    assert!(e.contains("cannot be written into text as a whole"), "{e}");
    // what is allowed stays allowed
    assert_eq!(errors_of(&command("  1. list of \"a\", \"b\" into xs\n  2. join xs with \", \" into joined\n  3. print: \"{joined} {count of xs}\"")), "");
}

#[test]
fn negative_numbers_parse_check_run_and_format() {
    let src = command(
        "  1. -5 into neg\n  2. -12.50 EUR into price\n  3. number.round(1234, -2) into r\n  4. -n into back\n  5. -(n + 1) into grouped\n  6. -(-n) into twice\n  7. n * -1 into explicit\n  8. neg - -n into both\n  9. print: \"{neg} {price} {r} {back} {grouped} {twice} {explicit} {both} {n - 1}\"",
    );
    assert_eq!(errors_of(&src), "");
    let formatted = format_source(&src, "t.agent.md").unwrap();
    // the source form of a negation survives `fmt`; a written `n * -1` stays as written (Q419)
    for kept in ["-n into back\n", "-(n + 1) into grouped\n", "-(-n) into twice\n", "n * -1 into explicit\n", "neg - -n into both\n"] {
        assert!(formatted.contains(kept), "{kept:?} in {formatted}");
    }
    assert!(formatted.contains("-5 into neg") && formatted.contains("-12.50 EUR into price") && formatted.contains("-2)"), "{formatted}");
    assert!(is_canonical(&formatted, "t.agent.md").unwrap());
    // `a - 1` is still a subtraction, `a -1` is not accepted as one
    assert!(errors_of(&command("  1. n -1 into k\n  2. print: \"{k}\"")).len() > 0);

    let d = temp("neg");
    write(&d, "agentmark.toml", "name = \"neg\"\nversion = \"0.1.0\"\ntargets = [\"rust\"]\n");
    write(&d, "main.agent.md", "# Program: c\n\n# Uses\n- [c](./commands/c.agent.md)\n");
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
    // A small project written here: a counter service whose state holds a map.
    let service = d.join("service");
    write(&service, "agentmark.toml", "name = \"tally\"\nversion = \"0.1.0\"\ntargets = [\"rust\"]\n");
    write(&service, "main.agent.md", "# Program: tally\nA counter service.\n\n# Uses\n- [tally](./services/tally.agent.md)\n");
    write(
        &service,
        "records/tally-state.agent.md",
        "# Record: TallyState\nWhat the service remembers: how often each name was counted.\n\n- counts: map of text to int\n- calls: list of text\n",
    );
    write(
        &service,
        "services/tally.agent.md",
        "# Uses\n- [TallyState](../records/tally-state.agent.md)\n\n# Service: tally\nCounts names; the counts live in the service state and are served in key order.\n\n- listen: 0.0.0.0:8080\n- state: TallyState\n\n## POST /count/{name}\n1. if state.counts at name is some seen:\n   1. put seen + 1 at name in state.counts\n   else:\n   1. put 1 at name in state.counts\n2. add name to state.calls\n3. respond 200 state.counts\n",
    );
    let o = agentmark(&["build", s(&service), "-o", s(&d.join("out")), "--no-check"]).output_locked();
    assert!(o.status.success(), "{}", stderr(&o));
    let bin = build_bin(&d.join("out"), "tally");
    let fx = d.join("fx");
    write(&fx, "model.json", "{\"today\":\"2026-10-14\",\"responses\":[]}\n");
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
