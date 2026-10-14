//! Opaque library handles (SPEC 22.4, step 23): the binder binds a library's own types and the
//! methods around them, sema holds `handle of <crate>.<Type>` to ownership rules, and an
//! incremental hash (`new` -> `update` x2 -> `finalize`) builds and prints the right digest.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentmark::binder::core::{run_tiers, CheckRequest, Tier, TypeChecker};
use agentmark::binder::rust::{read_crate_with, CargoChecker};
use agentmark::binder::{bind, BindOptions};
use agentmark::diag::Severity;
use agentmark::project::Project;
use agentmark::sema;
use agentmark_rt::MockModel;
use chrono::NaiveDate;

use support::*;

// ------------------------------------------------------------------------------ the binder

fn fixture(c: &str) -> PathBuf {
    Path::new(ROOT).join("tests/binder").join(format!("{c}.json"))
}

fn model() -> Arc<MockModel> {
    Arc::new(MockModel::from_file(Path::new(ROOT).join("tests/binder/model.json")).unwrap())
}

struct AcceptAll;

impl TypeChecker for AcceptAll {
    fn check(&self, _: &CheckRequest<'_>) -> Result<(), String> {
        Ok(())
    }
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(f)
}

fn tiered_sha2() -> agentmark::binder::core::BoundCrate {
    let json = std::fs::read_to_string(fixture("sha2")).unwrap();
    let digest = std::fs::read_to_string(fixture("digest")).unwrap();
    let ci = read_crate_with(&json, &[digest]).unwrap();
    block_on(run_tiers(&ci, &*model(), &AcceptAll))
}

/// The names v0.2 listed under `## Not bound` for sha2: the checked-in binding of example 05 is
/// that file (generated before handles existed).
fn v02_not_bound() -> Vec<String> {
    let text = std::fs::read_to_string(format!("{ROOT}/examples/05-invoice-service/bindings/sha2.agent.md")).unwrap();
    let section = text.split("## Not bound\n").nth(1).expect("a Not bound section");
    section.lines().filter_map(|l| l.strip_prefix("- ")).filter_map(|l| l.split(':').next()).map(String::from).collect()
}

#[test]
fn rebinding_sha2_binds_at_least_half_of_what_v02_listed_as_not_bound() {
    let before = v02_not_bound();
    assert_eq!(before.len(), 81, "the v0.2 file lists 81 items it did not bind");
    let b = tiered_sha2();
    let bound_now: Vec<&str> = before.iter().map(String::as_str).filter(|n| b.items.iter().any(|i| i.name == *n)).collect();
    assert!(
        bound_now.len() * 2 >= before.len(),
        "{} of {} bound: {:?}",
        bound_now.len(),
        before.len(),
        before.iter().filter(|n| !bound_now.contains(&n.as_str())).collect::<Vec<_>>()
    );
    // 7 methods of each of the 6 hash types
    assert_eq!(bound_now.len(), 42);
    // none of them needed the model: rules wrote the bodies, the type checker confirms them
    for i in b.items.iter().filter(|i| bound_now.contains(&i.name.as_str())) {
        assert_eq!(i.tier, Tier::Direct, "{}", i.name);
    }
}

#[test]
fn every_handle_body_for_sha2_compiles_against_the_real_crate() {
    // the real `cargo check`, as `agentmark bind` runs it: nothing may be demoted for failing it
    let dir = temp("handles-bind");
    let mut o = BindOptions::new(dir.join("bindings"), model(), NaiveDate::from_ymd_opt(2026, 10, 14).unwrap());
    o.rustdoc_json = Some(fixture("sha2"));
    o.foreign_rustdoc = vec![fixture("digest")];
    o.offline = true;
    o.checker = Some(Box::new(CargoChecker::new(true)));
    let out = bind("sha2", &o).unwrap();
    assert_eq!((out.counts.direct, out.counts.assisted), (42, 1), "{:?}", out.counts);
    let text = std::fs::read_to_string(&out.path).unwrap();
    for heading in [
        "## sha256_new() -> handle of sha2.Sha256",
        "## update(self: mut handle of sha2.Sha256, data: bytes)",
        "## chain_update(self: consume handle of sha2.Sha256, data: bytes) -> handle of sha2.Sha256",
        "## finalize(self: consume handle of sha2.Sha256) -> bytes",
        "## finalize_reset(self: mut handle of sha2.Sha256) -> bytes",
        "## reset(self: mut handle of sha2.Sha256)",
    ] {
        assert!(text.contains(heading), "{heading} missing from\n{text}");
    }
    // the body of a method names the receiver `self_`
    assert!(text.contains("<sha2::Sha256 as sha2::Digest>::update(self_, data);"), "{text}");
    // the file parses and checks: methods of six types share the name `update`
    let project = Project::load(dir.to_str().unwrap());
    if let Err(d) = sema::check(&project) {
        let errors: Vec<_> = d.into_iter().filter(|x| x.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "{}", project.render(&errors));
    }
}

/// Accepts everything, except that the named items go through the real `cargo check`.
struct RealFor {
    names: &'static [&'static str],
    real: CargoChecker,
}

impl TypeChecker for RealFor {
    fn check(&self, req: &CheckRequest<'_>) -> Result<(), String> {
        if self.names.contains(&req.name) {
            self.real.check(req)
        } else {
            Ok(())
        }
    }
}

#[test]
fn async_library_methods_are_assisted_and_their_bodies_await() {
    // `reqwest::get` and `Response::text` are `async`: the model writes the body, which awaits,
    // and the real compiler accepts it as the body of the binding's `async fn`
    let canned = serde_json::json!({"responses": [
        {"prompt_prefix": "bind reqwest::get\n", "response": "Ok(reqwest::get(&url).await?)"},
        {"prompt_prefix": "bind reqwest::Response::text\n", "response": "Ok(self_.text().await?)"},
    ]});
    let file = temp("handles-async");
    write(&file, "model.json", &canned.to_string());
    let model = MockModel::from_file(file.join("model.json")).unwrap();
    let ci = agentmark::binder::rust::read_crate(&std::fs::read_to_string(fixture("reqwest")).unwrap()).unwrap();
    let checker = RealFor { names: &["get", "response_text"], real: CargoChecker::new(true).with_features(false, &[]) };
    let b = block_on(run_tiers(&ci, &model, &checker));
    let get = b.items.iter().find(|i| i.name == "get").expect("get is bound");
    assert_eq!(get.tier, Tier::Assisted);
    assert_eq!(get.sig.heading("get"), "get(url: text) -> handle of reqwest.Response");
    let text = b.items.iter().find(|i| i.name == "response_text").expect("text is bound");
    assert_eq!(text.sig.heading(text.heading.as_deref().unwrap()), "text(self: consume handle of reqwest.Response) -> text");
    // the rest of the crate: handles bound by rules, what needs the model and has no canned answer is listed
    assert!(b.counts.direct >= 40, "{:?}", b.counts);
    let (_, why) = b.not_bound.iter().find(|(n, _)| n == "response_upgrade").expect("listed");
    assert!(why.contains("mock model has no response"), "{why}");
}

// ------------------------------------------------------------------------------ sema

const HASH_BINDING: &str = "> generated by agentmark bind \u{2014} sha2 0.10.9 \u{2014} 2026-10-14
# Binding: sha2
- crate: sha2
- effect: pure

> Create a new hasher.
## sha256_new() -> handle of sha2.Sha256
  ```rust
  Ok(<sha2::Sha256 as sha2::Digest>::new())
  ```

> Create a new hasher with prefix.
## sha224_new() -> handle of sha2.Sha224
  ```rust
  Ok(<sha2::Sha224 as sha2::Digest>::new())
  ```

> Process input data.
## update(self: mut handle of sha2.Sha256, data: bytes)
  ```rust
  <sha2::Sha256 as sha2::Digest>::update(self_, data);
  Ok(())
  ```

> Process input data.
## update(self: mut handle of sha2.Sha224, data: bytes)
  ```rust
  <sha2::Sha224 as sha2::Digest>::update(self_, data);
  Ok(())
  ```

> Retrieve result and consume the hasher.
## finalize(self: consume handle of sha2.Sha256) -> bytes
  ```rust
  Ok(<sha2::Sha256 as sha2::Digest>::finalize(self_).to_vec())
  ```

> Retrieve result and reset.
## finalize_reset(self: mut handle of sha2.Sha256) -> bytes
  ```rust
  Ok(<sha2::Sha256 as sha2::Digest>::finalize_reset(self_).to_vec())
  ```

> Add data to a copy of this hasher.
## chain_update(self: consume handle of sha2.Sha256, data: bytes) -> handle of sha2.Sha256
  ```rust
  Ok(<sha2::Sha256 as sha2::Digest>::chain_update(self_, data))
  ```

> Whether both hashers are the same.
## same_state(self: handle of sha2.Sha256, other: handle of sha2.Sha256) -> bool
  ```rust
  Ok(false)
  ```

> Combine two.
## absorb(self: mut handle of sha2.Sha256, other: consume handle of sha2.Sha256)
  ```rust
  Ok(())
  ```
";

fn project(main: &str) -> Project {
    Project::from_memory(
        "app/main.agent.md",
        vec![("app/main.agent.md", main), ("app/bindings/sha2.agent.md", HASH_BINDING), ("app/tools.toml", "")],
    )
}

/// Every error of the program, rendered as `check` prints it.
fn errors_of(main: &str) -> String {
    let p = project(main);
    match sema::check(&p) {
        Ok(_) => String::new(),
        Err(d) => p.render(&d.into_iter().filter(|x| x.severity == Severity::Error).collect::<Vec<_>>()),
    }
}

fn ok(main: &str) {
    let e = errors_of(main);
    assert!(e.is_empty(), "{e}");
}

const HEAD: &str = "# Import: crate sha2\n\n> Hashes.\n# Command: go\n- args: word: text\n\n## Run\n";

fn prog(lines: &[&str]) -> String {
    let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
    format!("{HEAD}{body}")
}

#[test]
fn a_hasher_is_fed_in_pieces_and_finished_once() {
    ok(&prog(&[
        "- sha2 sha256_new -> hasher",
        "- hasher update word",
        "- hasher update \"more\"",
        "- hasher finalize -> digest",
        "- print: \"hashed\"",
    ]));
}

#[test]
fn methods_with_one_name_belong_to_their_handle_type() {
    ok(&prog(&[
        "- sha2 sha256_new -> a",
        "- sha2 sha224_new -> b",
        "- a update word",
        "- b update word",
    ]));
    // `finalize` exists on Sha256 only in this binding
    let e = errors_of(&prog(&["- sha2 sha224_new -> b", "- b finalize -> d"]));
    assert!(e.contains("The handle \"b\" (sha2.Sha224) has no method \"finalize\"."), "{e}");
}

#[test]
fn using_a_handle_after_a_consuming_call_names_the_line_that_used_it_up() {
    let e = errors_of(&prog(&[
        "- sha2 sha256_new -> hasher",
        "- hasher finalize -> digest",
        "- hasher update \"late\"",
    ]));
    // line 8 is `hasher finalize -> digest` (6 header lines, one blank, `## Run`)
    assert!(e.contains("app/main.agent.md:10\n  - hasher update \"late\"\n    ^^^^^^"), "{e}");
    assert!(e.contains("\"hasher\" was used up on line 9 (\"hasher finalize -> digest\")"), "{e}");
    assert_eq!(e.matches("was used up").count(), 1, "{e}");
}

#[test]
fn passing_a_handle_on_uses_it_up_too() {
    let e = errors_of(
        "# Import: crate sha2\n\n> Feeds.\n# Function: feed\n- in: h: handle of sha2.Sha256\n- effect: pure\n  - h update \"x\"\n\n> Hashes.\n# Command: go\n- args: word: text\n\n## Run\n- sha2 sha256_new -> hasher\n- do feed(hasher)\n- hasher update word\n",
    );
    assert!(e.contains("\"hasher\" was used up on line 15"), "{e}");
    let e = errors_of(&prog(&["- sha2 sha256_new -> a", "- set b = a", "- a update word"]));
    assert!(e.contains("\"a\" was used up on line 9 (\"set b = a\")"), "{e}");
    // a handle can be handed on and handed back
    ok(
        "# Import: crate sha2\n\n> Feeds.\n# Function: feed\n- in: h: handle of sha2.Sha256\n- out: handle of sha2.Sha256\n- effect: pure\n  - h update \"x\"\n  - return h\n\n> Hashes.\n# Command: go\n- args: word: text\n\n## Run\n- sha2 sha256_new -> hasher\n- do feed(hasher) -> back\n- back finalize -> digest\n",
    );
}

#[test]
fn a_handle_used_up_in_one_branch_may_be_used_up_afterwards() {
    let e = errors_of(&prog(&[
        "- sha2 sha256_new -> hasher",
        "- if word = \"a\":",
        "  - hasher finalize -> early",
        "- hasher update word",
    ]));
    assert!(e.contains("\"hasher\" was used up on line 10"), "{e}");
    // a branch that leaves contributes nothing
    ok(&prog(&[
        "- sha2 sha256_new -> hasher",
        "- if word = \"a\":",
        "  - hasher finalize -> early",
        "  - stop",
        "- hasher update word",
    ]));
}

#[test]
fn a_handle_that_lives_outside_a_loop_cannot_be_used_up_inside_it() {
    let e = errors_of(&format!(
        "{HEAD}- sha2 sha256_new -> hasher\n- for each part in list_of_parts:\n  - hasher finalize -> d\n"
    ));
    assert!(e.contains("is used up here, but this line is inside a loop"), "{e}");
    // made inside the loop, it is a new one every round
    let e = errors_of(
        "# Import: crate sha2\n\n> Hashes.\n# Command: go\n- args: parts: list of text\n\n## Run\n- for each part in parts:\n  - sha2 sha256_new -> hasher\n  - hasher update part\n  - hasher finalize -> d\n",
    );
    assert!(e.is_empty(), "{e}");
}

#[test]
fn a_handle_cannot_be_printed_compared_saved_or_sent() {
    for (line, what) in [
        ("- print: hasher", "print"),
        ("- print: \"state {hasher}\"", "interpolate"),
        ("- set same = hasher = hasher", "compare"),
        ("- ask model with hasher: \"hi\" -> answer", "ask"),
    ] {
        let e = errors_of(&prog(&["- sha2 sha256_new -> hasher", line]));
        assert!(
            e.contains("is handle of sha2.Sha256, a library value that cannot be printed, compared, saved, sent or traced"),
            "{what}: {e}"
        );
    }
    let e = errors_of("# Record: Doc\n- name: text\n- hasher: handle of sha2.Sha256\n\n# Import: crate sha2\n");
    assert!(e.contains("A record is saved and sent as JSON, so it cannot have a field of handle of sha2.Sha256"), "{e}");
}

#[test]
fn handles_do_not_nest_in_lists_or_maybes() {
    let e = errors_of("# Import: crate sha2\n\n> Takes.\n# Function: f\n- in: hs: list of handle of sha2.Sha256\n- effect: pure\n  - stop\n");
    assert!(e.contains("A handle cannot be in a list: handle of sha2.Sha256."), "{e}");
    let e = errors_of("# Import: crate sha2\n\n> Takes.\n# Function: f\n- in: h: maybe handle of sha2.Sha256\n- effect: pure\n  - stop\n");
    assert!(e.contains("A handle cannot be in a maybe"), "{e}");
}

#[test]
fn an_unknown_handle_type_is_suggested_from_the_bindings() {
    let e = errors_of("# Import: crate sha2\n\n> Takes.\n# Function: f\n- in: h: handle of sha2.Sha265\n- effect: pure\n  - stop\n");
    assert!(e.contains("No binding declares the handle type \"sha2.Sha265\"."), "{e}");
    assert!(e.contains("Did you mean \"sha2.Sha256\"?"), "{e}");
}

#[test]
fn borrow_modes_belong_to_bindings_and_arguments_must_be_variables() {
    let e = errors_of("# Import: crate sha2\n\n> Takes.\n# Function: f\n- in: h: mut handle of sha2.Sha256\n- effect: pure\n  - stop\n");
    assert!(e.contains("\"mut\" in front of \"handle of\" only belongs in a binding's parameter list"), "{e}");
    // `same_state` borrows both handles: the same one twice, or a new value, is refused
    let e = errors_of(&prog(&["- sha2 sha256_new -> a", "- a same_state a -> same"]));
    assert!(e.contains("\"a\" is given twice in one call."), "{e}");
    ok(&prog(&["- sha2 sha256_new -> a", "- sha2 sha256_new -> b", "- a same_state b -> same", "- print: \"{same}\""]));
    // `absorb` uses up its second handle
    let e = errors_of(&prog(&["- sha2 sha256_new -> a", "- sha2 sha256_new -> b", "- a absorb b", "- b update word"]));
    assert!(e.contains("\"b\" was used up on line 10"), "{e}");
}

#[test]
fn a_parallel_block_cannot_use_one_handle_twice() {
    let e = errors_of(&prog(&[
        "- sha2 sha256_new -> a",
        "- sha2 sha256_new -> b",
        "- parallel:",
        "  - a update \"x\"",
        "  - b update \"y\"",
    ]));
    assert!(e.is_empty(), "{e}");
    let e = errors_of(&prog(&["- sha2 sha256_new -> a", "- parallel:", "  - a update \"x\"", "  - a update \"y\""]));
    assert!(e.contains("both use the handle \"a\" (the first is on line 10)"), "{e}");
}

#[test]
fn a_misspelled_handle_variable_is_suggested() {
    let e = errors_of(&prog(&["- sha2 sha256_new -> hasher", "- hashr update word"]));
    assert!(e.contains("Unknown tool \"hashr\".") && e.contains("Did you mean \"hasher\"?"), "{e}");
}

#[test]
fn types_with_an_agentmark_equivalent_stay_values_and_generic_ones_are_not_handles() {
    // regression (step 23): `NaiveDate` is a public struct of chrono, but AgentMark has `date`; and
    // `DateTime<Tz>` has a type parameter, so a path to it would need arguments
    let json = std::fs::read_to_string(fixture("chrono")).unwrap();
    let ci = read_crate_with(&json, &[]).unwrap();
    let b = block_on(run_tiers(&ci, &*model(), &AcceptAll));
    let heading = |n: &str| {
        let i = b.items.iter().find(|i| i.name == n).unwrap_or_else(|| panic!("{n} bound"));
        i.sig.heading(i.heading.as_deref().unwrap_or(&i.name))
    };
    assert_eq!(heading("fixed_offset_offset_from_utc_date"), "offset_from_utc_date(self: handle of chrono.FixedOffset, utc: date) -> handle of chrono.FixedOffset");
    assert_eq!(heading("weekday_succ"), "succ(self: handle of chrono.Weekday) -> handle of chrono.Weekday");
    for i in &b.items {
        let h = i.sig.heading(&i.name);
        assert!(!h.contains("handle of chrono.NaiveDate") && !h.contains("handle of chrono.DateTime"), "{h}");
    }
}

#[test]
fn a_try_that_makes_a_handle_must_leave_when_it_fails() {
    let e = errors_of(&prog(&["- try sha2 sha256_new -> hasher", "  otherwise:", "    - print: \"no\""]));
    assert!(e.contains("\"hasher\" is a handle, and there is none to use when this step fails."), "{e}");
    ok(&prog(&["- try sha2 sha256_new -> hasher", "  otherwise:", "    - stop", "- hasher update word"]));
}

#[test]
fn handle_types_parse_and_print_in_the_canonical_style() {
    let src = "# Import: crate sha2\n\n> Takes one and gives it back.\n# Function: f\n- in: h: handle of sha2.Sha256, n: int\n- out: handle of sha2.Sha256\n- effect: pure\n  - return h\n";
    assert_eq!(agentmark::fmt::format_source(src, "f.agent.md").unwrap(), src);
    // a binding's modes survive `fmt` too
    assert_eq!(agentmark::fmt::format_source(HASH_BINDING, "sha2.agent.md").unwrap(), HASH_BINDING);
    // the AST says what was written
    let f = agentmark::syntax::parser::parse_file("sha2.agent.md", HASH_BINDING).unwrap();
    let b = f.decls.iter().find_map(|d| if let agentmark::syntax::ast::Decl::Binding(b) = d { Some(b) } else { None }).unwrap();
    let modes: Vec<_> = b
        .items
        .iter()
        .filter(|i| i.name.name == "absorb")
        .flat_map(|i| i.params.iter())
        .map(|p| match &p.ty.kind {
            agentmark::syntax::ast::TypeKind::Handle { name, mode } => format!("{mode:?} {name}"),
            k => format!("{k:?}"),
        })
        .collect();
    assert_eq!(modes, ["Mut sha2.Sha256", "Consume sha2.Sha256"]);
}

// ------------------------------------------------------------------------------ end to end

/// SHA-256("hello world")
const HELLO_WORLD: &str = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

const HASH_COMMAND: &str = "# Import: ../records/invoice.agent.md
# Import: crate sha2

> Hashes two pieces of text with one hasher, fed one after the other.
# Command: hash-pieces
- args: first: text, second: text

## Run
- sha2 sha256_new -> hasher
- hasher update first
- hasher settle -> turns
- print: \"settled after {turns} turns\"
- do add_more(hasher, second) -> hasher
- hasher finalize -> digest
- print: \"{hex(digest)}\"

> Feeds text to a hasher and hands it back.
# Function: add_more
- in: h: handle of sha2.Sha256, more: text
- out: handle of sha2.Sha256
- effect: pure
  - h update more
  - return h

## Rust: hex -> text
- in: digest: bytes
- effect: pure
  ```rust
  digest.iter().map(|b| format!(\"{:02x}\", b)).collect::<String>()
  ```
";

/// Example 05 (service, records, a command) plus a command that hashes in pieces: its sha2 binding
/// is generated from the checked-in rustdoc, so it holds the handle methods.
fn hash_project(d: &Path) -> PathBuf {
    let proj = d.join("invoice-service");
    copy_dir(Path::new(&format!("{ROOT}/examples/05-invoice-service")), &proj);
    std::fs::remove_file(proj.join("bindings/sha2.agent.md")).unwrap();
    write(&proj, "commands/hash-pieces.agent.md", HASH_COMMAND);
    let main = std::fs::read_to_string(proj.join("main.agent.md")).unwrap();
    let main = main
        .replace("# Import: ./commands/import-csv.agent.md\n", "# Import: ./commands/import-csv.agent.md\n# Import: ./commands/hash-pieces.agent.md\n")
        .replace("- import-csv\n", "- import-csv\n- hash-pieces\n");
    write(&proj, "main.agent.md", &main);
    proj
}

#[test]
fn an_incremental_hash_builds_and_prints_the_right_digest() {
    let d = temp("handles-e2e");
    let proj = hash_project(&d);
    let out = d.join("out");
    let build = || {
        let mut c = agentmark(&["build", s(&proj), "-o", s(&out), "--no-check"]);
        bind_env(&mut c);
        c.output_locked()
    };
    // the first build binds the crate and stops at the approval, like any new binding
    let first = build();
    assert_eq!(first.status.code(), Some(1), "{}", stderr(&first));
    let binding = proj.join("bindings/sha2.agent.md");
    let text = std::fs::read_to_string(&binding).unwrap();
    assert!(text.contains("## update(self: mut handle of sha2.Sha256, data: bytes)"), "{text}");
    // a library method that is `async` is awaited by the generated code: a body may `.await`
    let settle = "> Waits for the runtime to run something else first.\n## settle(self: handle of sha2.Sha256) -> int\n  ```rust\n  tokio::task::yield_now().await;\n  Ok(1)\n  ```\n\n## Not bound\n";
    let text = text.replace("- effect: unverified", "- effect: pure").replace("## Not bound\n", settle);
    write(&proj, "bindings/sha2.agent.md", &text);
    let second = build();
    assert!(second.status.success(), "{}", stderr(&second));

    let bin = build_bin(&out, "invoice-service");
    let run = std::process::Command::new(&bin).args(["hash-pieces", "hello ", "world"]).current_dir(&d).output().unwrap();
    assert!(run.status.success(), "{}", stderr(&run));
    let said: Vec<String> = stdout(&run).lines().map(String::from).collect();
    assert_eq!(said, ["settled after 1 turns", HELLO_WORLD], "{}", stdout(&run));
    // pieces split elsewhere give the same digest: the hasher keeps its state between the calls
    let run = std::process::Command::new(&bin).args(["hash-pieces", "hello w", "orld"]).current_dir(&d).output().unwrap();
    assert_eq!(stdout(&run).lines().last(), Some(HELLO_WORLD));
}
