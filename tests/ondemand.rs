//! The on-demand binding tier (PROMPT step 13, SPEC 15.2 last row): a plain statement against a
//! `## Not bound` item of sha2 makes the binder write the item with a canned `MockModel`.

mod support;

use std::path::Path;
use std::process::Output;
use support::{agentmark, bind_env, s, stderr, stdout, temp, write, OutputLocked, Tmp};

const MAIN: &str = "# Import: ./fingerprint.agent.md\n\n# Run\n- fingerprint\n";
const USES_SHA224: &str = "# Import: crate sha2\n\n> Prints a fingerprint.\n# Command: fingerprint\n- args: payload: bytes\n\n## Run\n- sha2 sha224_digest payload -> digest\n- print: \"done\"\n";

const GOOD_BODY: &str = "Ok(<sha2::Sha224 as sha2::Digest>::digest(&data).to_vec())";

fn model_json(response: &str) -> String {
    serde_json::json!({
        "today": "2026-10-14",
        "responses": [{ "prompt_prefix": "on-demand sha2 sha224_digest\n", "response": response }]
    })
    .to_string()
}

/// A project whose sha2 binding was generated (the canned model has no answer for `sha224_digest`,
/// so it is listed under `## Not bound`) and approved, plus the model file for the on-demand tier.
fn project(tag: &str, response: &str) -> (Tmp, String) {
    let d = temp(tag);
    write(&d, "agentmark.toml", "name = \"p\"\n");
    write(&d, "main.agent.md", MAIN);
    write(&d, "fingerprint.agent.md", USES_SHA224);
    let mut c = agentmark(&["bind", "sha2", "--project", s(&d)]);
    bind_env(&mut c);
    let o = c.output().unwrap();
    assert!(o.status.success(), "{}{}", stdout(&o), stderr(&o));
    let file = d.join("bindings/sha2.agent.md");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("- sha224_digest: "), "sha224_digest must start out not bound");
    std::fs::write(&file, text.replace("- effect: unverified", "- effect: pure")).unwrap();
    let model = d.join("on-demand-model.json");
    std::fs::write(&model, model_json(response)).unwrap();
    let model = s(&model).to_string();
    (d, model)
}

fn run(args: &[&str], model: &str) -> Output {
    let mut c = agentmark(args);
    bind_env(&mut c);
    c.env("AGENTMARK_MODEL", format!("mock:{model}"));
    c.output_locked()
}

fn binding_of(d: &Path) -> String {
    std::fs::read_to_string(d.join("bindings/sha2.agent.md")).unwrap()
}

#[test]
fn a_valid_body_is_saved_unverified_check_lists_it_and_the_build_goes_on() {
    let (d, model) = project("ondemand-ok", GOOD_BODY);
    let before = binding_of(&d);
    assert!(before.contains("- effect: pure"));

    // check writes the item, says so, and lists the line to approve
    let o = run(&["check", s(&d)], &model);
    let (out, err) = (stdout(&o), stderr(&o));
    assert!(err.contains("binding sha2: writing sha224_digest on demand\u{2026} done"), "{err}");
    assert!(out.contains("Libraries nobody approved yet"), "{out}");
    assert!(out.contains("bindings/sha2.agent.md:5: change \"- effect: unverified\" to \"- effect: pure\""), "{out}");
    assert_eq!(o.status.code(), Some(1), "unapproved until the line is changed: {out}{err}");

    // the file: the item moved out of Not bound, the effect is unverified again
    let after = binding_of(&d);
    assert!(after.contains("- effect: unverified"), "{after}");
    assert!(after.contains("## sha224_digest(data: bytes) -> bytes\n  ```rust\n  "), "{after}");
    assert!(after.contains(GOOD_BODY), "{after}");
    assert!(after.contains("Written on demand; not reviewed yet."), "{after}");
    assert!(!after.contains("- sha224_digest:"), "{after}");
    assert!(after.contains("- sha256_new:"), "the other items stay listed");
    assert!(d.join("bindings/sha2.agent.md.prev").exists(), "the approved file is kept");

    // a second check does not ask the model again
    let o = run(&["check", s(&d), "--allow-unverified"], &model);
    assert_eq!(o.status.code(), Some(0), "{}{}", stdout(&o), stderr(&o));
    assert!(!stderr(&o).contains("on demand"), "{}", stderr(&o));

    // build: refused while unapproved, goes through with --allow-unverified (and compiles)
    let out_dir = d.join("out");
    let o = run(&["build", s(&d), "-o", s(&out_dir)], &model);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("not approved yet"), "{}", stderr(&o));
    assert!(!out_dir.exists());
    let o = run(&["build", s(&d), "-o", s(&out_dir), "--allow-unverified"], &model);
    assert_eq!(o.status.code(), Some(0), "{}{}", stdout(&o), stderr(&o));
    assert!(out_dir.join("Cargo.toml").exists());
}

#[test]
fn build_writes_the_item_itself_when_check_did_not() {
    let (d, model) = project("ondemand-build", GOOD_BODY);
    let out_dir = d.join("out");
    let o = run(&["build", s(&d), "-o", s(&out_dir), "--allow-unverified"], &model);
    assert_eq!(o.status.code(), Some(0), "{}{}", stdout(&o), stderr(&o));
    assert!(stderr(&o).contains("binding sha2: writing sha224_digest on demand\u{2026} done"), "{}", stderr(&o));
    assert!(binding_of(&d).contains("## sha224_digest(data: bytes) -> bytes"));
}

#[test]
fn invalid_rust_twice_stops_with_the_documented_error_and_changes_nothing() {
    let (d, model) = project("ondemand-bad", "Ok(this_function_does_not_exist(&data))");
    let before = binding_of(&d);
    let out_dir = d.join("out");
    let o = run(&["build", s(&d), "-o", s(&out_dir), "--allow-unverified"], &model);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{}{err}", stdout(&o));

    // two plain log lines, one per attempt
    assert!(err.contains("binding sha2: writing sha224_digest on demand\u{2026} failed\n"), "{err}");
    assert!(err.contains("binding sha2: writing sha224_digest on demand (try 2 of 2)\u{2026} failed\n"), "{err}");
    assert_eq!(err.matches("writing sha224_digest on demand").count(), 2, "{err}");

    // the SPEC 10 error at the author's statement
    let file = d.join("fingerprint.agent.md");
    let expected = format!(
        "{}:8\n  - sha2 sha224_digest payload -> digest\n         ^^^^^^^^^^^^^\n  \
         Could not bind \"sha224_digest\" of sha2 on demand: the code the model wrote did not pass the type check, twice.\n  \
         Nothing was changed. Use a ## Rust block for \"sha224_digest\", or bind it by hand in bindings/sha2.agent.md.\n",
        file.display()
    );
    assert!(err.contains(&expected), "{err}\n-- expected --\n{expected}");
    assert!(err.contains("1 error: not built."), "{err}");
    assert!(!err.contains("error[E"), "no rustc output reaches the author: {err}");

    // never a broken build: nothing written, the binding is byte for byte what it was
    assert!(!out_dir.exists());
    assert_eq!(binding_of(&d), before);
    assert!(!d.join("bindings/sha2.agent.md.prev").exists());

    // check says the same
    let o = run(&["check", s(&d)], &model);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("Could not bind \"sha224_digest\" of sha2 on demand"), "{}", stderr(&o));
    assert_eq!(binding_of(&d), before);
}

#[test]
fn check_json_reports_the_failure_as_a_diagnostic() {
    let (d, model) = project("ondemand-json", "not rust at all (((");
    let o = run(&["check", s(&d), "--json"], &model);
    assert_eq!(o.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["diagnostics"][0]["code"], "E220");
    assert_eq!(v["diagnostics"][0]["line"], 8);
}

// ---- the tier as a library call, with a fake type checker ----

mod library {
    use super::*;
    use agentmark::binder::core::ondemand::{prompt, Request};
    use agentmark::binder::core::{add_on_demand, BoundItem, CheckRequest, ProposedEffect, Sig, Tier, Ty, TypeChecker};
    use agentmark::binder::{bind, bind_on_demand, BindOptions, OnDemandError, OnDemandUse};
    use agentmark_rt::MockModel;
    use chrono::NaiveDate;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn fixture(c: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/binder").join(format!("{c}.json"))
    }

    /// Fails the first `fail` checks, accepts the rest.
    struct Flaky {
        fail: usize,
        seen: Arc<AtomicUsize>,
    }

    impl TypeChecker for Flaky {
        fn check(&self, _: &CheckRequest<'_>) -> Result<(), String> {
            if self.seen.fetch_add(1, Ordering::SeqCst) < self.fail {
                Err("error[E0308]: mismatched types".into())
            } else {
                Ok(())
            }
        }
    }

    fn opts(dir: &Path, model_json: &str, fail: usize) -> (BindOptions, Arc<AtomicUsize>) {
        let seen = Arc::new(AtomicUsize::new(0));
        let m = dir.join("model.json");
        std::fs::write(&m, model_json).unwrap();
        let mut o = BindOptions::new(
            dir.join("bindings"),
            Arc::new(MockModel::from_file(&m).unwrap()),
            NaiveDate::from_ymd_opt(2026, 10, 14).unwrap(),
        );
        o.rustdoc_json = Some(fixture("sha2"));
        o.foreign_rustdoc = vec![fixture("digest")];
        o.offline = true;
        o.checker = Some(Box::new(Flaky { fail, seen: seen.clone() }));
        (o, seen)
    }

    /// A generated sha2 binding (no canned answers, so every assisted item is not bound).
    fn bound(tag: &str, model_json: &str, fail: usize) -> (Tmp, BindOptions, Arc<AtomicUsize>) {
        let d = temp(tag);
        let (first, _) = opts(&d, r#"{"responses":[]}"#, 0);
        bind("sha2", &first).unwrap();
        let (o, seen) = opts(&d, model_json, fail);
        (d, o, seen)
    }

    fn use_of(item: &str, arity: usize) -> OnDemandUse {
        OnDemandUse { item: item.into(), statement: format!("sha2 {item} payload -> digest"), arity }
    }

    fn lines() -> (Arc<std::sync::Mutex<Vec<String>>>, impl FnMut(&str) + Send) {
        let v = Arc::new(std::sync::Mutex::new(Vec::new()));
        let w = v.clone();
        (v, move |l: &str| w.lock().unwrap().push(l.to_string()))
    }

    #[test]
    fn the_second_attempt_is_told_what_was_wrong_and_can_succeed() {
        let model = r#"{"responses":[{"prompt_prefix":"on-demand sha2 sha224_digest\n","response":"Ok(vec![])"}]}"#;
        let (d, o, seen) = bound("ondemand-retry", model, 1);
        let (log, mut sink) = lines();
        let done = bind_on_demand("sha2", &[use_of("sha224_digest", 1)], &o, &mut sink).unwrap();
        assert_eq!(done, ["sha224_digest"]);
        assert_eq!(seen.load(Ordering::SeqCst), 2);
        assert_eq!(
            *log.lock().unwrap(),
            [
                "binding sha2: writing sha224_digest on demand\u{2026} failed",
                "binding sha2: writing sha224_digest on demand (try 2 of 2)\u{2026} done"
            ]
        );
        let text = std::fs::read_to_string(d.join("bindings/sha2.agent.md")).unwrap();
        assert!(text.contains("## sha224_digest(data: bytes) -> bytes"), "{text}");
        // the file still parses and its effect line is the unapproved one
        agentmark::syntax::parser::parse_file("sha2.agent.md", &text).unwrap_or_else(|_| panic!("{text}"));
        assert!(text.contains("- effect: unverified"));
    }

    #[test]
    fn an_item_with_no_agentmark_signature_gets_one_from_the_model() {
        // sha224_new returns a hasher object: skipped, no signature to implement
        let good = "## sha224_new(data: bytes) -> bytes\nOk(data)";
        let model = serde_json::json!({"responses":[{"prompt_prefix":"on-demand sha2 sha224_new\n","response":good}]}).to_string();
        let (d, o, _) = bound("ondemand-sig", &model, 0);
        let (_, mut sink) = lines();
        bind_on_demand("sha2", &[use_of("sha224_new", 1)], &o, &mut sink).unwrap();
        let text = std::fs::read_to_string(d.join("bindings/sha2.agent.md")).unwrap();
        assert!(text.contains("## sha224_new(data: bytes) -> bytes\n  ```rust\n  Ok(data)\n  ```"), "{text}");
        assert!(!text.contains("- sha224_new:"), "{text}");
    }

    #[test]
    fn a_signature_that_does_not_fit_the_statement_fails_twice_and_writes_nothing() {
        let wrong = "## sha224_new(a: bytes, b: bytes) -> bytes\nOk(a)";
        let model = serde_json::json!({"responses":[{"prompt_prefix":"on-demand sha2 sha224_new\n","response":wrong}]}).to_string();
        let (d, o, seen) = bound("ondemand-arity", &model, 0);
        let before = std::fs::read_to_string(d.join("bindings/sha2.agent.md")).unwrap();
        let (_, mut sink) = lines();
        let e = bind_on_demand("sha2", &[use_of("sha224_new", 1)], &o, &mut sink).unwrap_err();
        let OnDemandError::Failed(f) = e else { panic!("expected a failure") };
        assert_eq!(f.item, "sha224_new");
        assert_eq!(f.complaints.len(), 2);
        assert!(f.complaints[0].contains("1 value but the signature has 2 parameters"), "{:?}", f.complaints);
        assert_eq!(seen.load(Ordering::SeqCst), 0, "the compiler is not asked about a body for the wrong signature");
        assert_eq!(std::fs::read_to_string(d.join("bindings/sha2.agent.md")).unwrap(), before);
    }

    #[test]
    fn several_items_are_saved_together_or_not_at_all() {
        let model = serde_json::json!({"responses":[
            {"prompt_prefix":"on-demand sha2 sha224_digest\n","response":"Ok(vec![])"},
            {"prompt_prefix":"on-demand sha2 sha384_digest\n","response":"Ok(vec![])"},
        ]})
        .to_string();
        // the first item passes, both attempts for the second fail
        let (d, mut o2, _) = bound("ondemand-two", &model, 0);
        let before = std::fs::read_to_string(d.join("bindings/sha2.agent.md")).unwrap();
        o2.checker = Some(Box::new(OnlyFor("sha224_digest")));
        let (_, mut sink) = lines();
        let uses = [use_of("sha224_digest", 1), use_of("sha384_digest", 1)];
        assert!(matches!(bind_on_demand("sha2", &uses, &o2, &mut sink), Err(OnDemandError::Failed(f)) if f.item == "sha384_digest"));
        assert_eq!(std::fs::read_to_string(d.join("bindings/sha2.agent.md")).unwrap(), before);
        o2.checker = Some(Box::new(Flaky { fail: 0, seen: Arc::new(AtomicUsize::new(0)) }));
        let done = bind_on_demand("sha2", &uses, &o2, &mut sink).unwrap();
        assert_eq!(done, ["sha224_digest", "sha384_digest"]);
    }

    struct OnlyFor(&'static str);
    impl TypeChecker for OnlyFor {
        fn check(&self, r: &CheckRequest<'_>) -> Result<(), String> {
            if r.name == self.0 { Ok(()) } else { Err("no".into()) }
        }
    }

    #[test]
    fn an_item_that_is_not_listed_as_not_bound_is_left_alone() {
        let d = temp("ondemand-bound");
        let repo = std::fs::read_to_string(fixture("model")).unwrap();
        let (o, seen) = opts(&d, &repo, 0);
        bind("sha2", &o).unwrap();
        let asked_while_binding = seen.load(Ordering::SeqCst);
        let (log, mut sink) = lines();
        // already bound by the first tiers
        let done = bind_on_demand("sha2", &[use_of("sha256_digest", 1), use_of("no_such_item", 1)], &o, &mut sink).unwrap();
        assert!(done.is_empty());
        assert!(log.lock().unwrap().is_empty());
        assert_eq!(seen.load(Ordering::SeqCst), asked_while_binding);
    }

    #[test]
    fn the_prompt_names_the_statement_the_docs_and_the_last_complaint() {
        let json = std::fs::read_to_string(fixture("sha2")).unwrap();
        let foreign = vec![std::fs::read_to_string(fixture("digest")).unwrap()];
        let ci = agentmark::binder::rust::read_crate_with(&json, &foreign).unwrap();
        let req = Request { crate_name: "sha2", item: "sha224_digest", statement: "sha2 sha224_digest payload -> d", reason: "x", arity: 1 };
        let p = prompt(&ci, &req, None, Some("the code does not compile: boom"));
        assert!(p.starts_with("on-demand sha2 sha224_digest\n"));
        assert!(p.contains("sha2 sha224_digest payload -> d"));
        assert!(p.contains("Your previous answer was rejected: the code does not compile: boom"));
        assert!(p.contains("signature on the first line"));
    }

    fn item(name: &str) -> BoundItem {
        BoundItem {
            name: name.into(),
            path: format!("c::{name}"),
            summary: "Does it.".into(),
            sig: Sig { params: vec![], ret: Some(Ty::Int) },
            body: "Ok(1)".into(),
            tier: Tier::Assisted,
        }
    }

    const FILE: &str = "> generated by agentmark bind \u{2014} c 1.0.0 \u{2014} 2026-10-14\n> edit freely\n# Binding: c\n- crate: c\n- effect: pure\n- proposed-effect: pure\n\n> Old.\n## old() -> int\n  ```rust\n  Ok(0)\n  ```\n\n## Not bound\n- a: reason one\n  > continued\n- b: reason two\n- c\n";

    #[test]
    fn writer_moves_items_out_of_not_bound_and_resets_the_approval() {
        let out = add_on_demand(FILE, &[item("a")], "rust", ProposedEffect::Local);
        let f = agentmark::syntax::parser::parse_file("c.agent.md", &out).unwrap_or_else(|_| panic!("{out}"));
        assert!(out.contains("- effect: unverified\n- proposed-effect: local\n"), "{out}");
        assert!(out.contains("> Does it.\n## a() -> int\n  ```rust\n  Ok(1)\n  ```\n\n## Not bound\n- b: reason two\n- c\n"), "{out}");
        assert!(!out.contains("continued") && !out.contains("reason one"), "{out}");
        assert!(out.contains("Ok(0)"), "the old item stays");
        let agentmark::syntax::ast::Decl::Binding(b) = &f.decls[0] else { panic!() };
        assert_eq!(b.items.len(), 2);
        assert_eq!(b.not_bound.len(), 2);
    }

    #[test]
    fn writer_drops_the_not_bound_heading_when_the_last_entry_leaves() {
        let one = "# Binding: c\n- crate: c\n- effect: network\n\n## Not bound\n- a: why\n";
        let out = add_on_demand(one, &[item("a")], "rust", ProposedEffect::Pure);
        assert_eq!(
            out,
            "# Binding: c\n- crate: c\n- effect: unverified\n- proposed-effect: pure\n\n> Does it.\n## a() -> int\n  ```rust\n  Ok(1)\n  ```\n"
        );
        // a proposal never goes down: network stays network
        let again = add_on_demand(&out.replace("proposed-effect: pure", "proposed-effect: network"), &[], "rust", ProposedEffect::Pure);
        assert!(again.contains("- proposed-effect: network"));
    }
}
