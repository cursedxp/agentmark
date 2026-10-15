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
    assert!(why.contains("no model answer when this file was generated"), "{why}");
}

// ------------------------------------------- lifetimes and one-instantiation generics (Q370)

/// A small crate in the shape of `sqlx::query`: `fn query<'q, DB: Database>(sql: &'q str) -> Query<'q, DB>`,
/// one database (`Sqlite`) compiled in, and a builder method on the generic `Query`.
fn mini_rustdoc() -> String {
    mini_rustdoc_with(false)
}

/// `two_databases`: a second implementor of `Database`, so `DB` has no single instantiation.
fn mini_rustdoc_with(two_databases: bool) -> String {
    let path = |id: u64, p: &str, args: serde_json::Value| serde_json::json!({"resolved_path": {"path": p, "id": id, "args": args}});
    let query_args = serde_json::json!({"angle_bracketed": {"args": [{"lifetime": "'q"}, {"type": {"generic": "DB"}}], "constraints": []}});
    let generics = serde_json::json!({"params": [
        {"name": "'q", "kind": {"lifetime": {"outlives": []}}},
        {"name": "DB", "kind": {"type": {"bounds": [{"trait_bound": {"trait": {"path": "Database", "id": 10, "args": null}, "generic_params": [], "modifier": "none"}}], "default": null, "is_synthetic": false}}},
    ], "where_predicates": []});
    let none = serde_json::json!({"params": [], "where_predicates": []});
    let generics_db = serde_json::json!({"params": [
        {"name": "DB", "kind": {"type": {"bounds": [{"trait_bound": {"trait": {"path": "Database", "id": 10, "args": null}, "generic_params": [], "modifier": "none"}}], "default": null, "is_synthetic": false}}},
    ], "where_predicates": []});
    let header = serde_json::json!({"is_unsafe": false, "is_async": false});
    let item = |id: u64, name: &str, inner: serde_json::Value| serde_json::json!({"id": id, "name": name, "visibility": "public", "docs": "Starts a query.", "inner": inner});
    let describe = item(4, "describe", serde_json::json!({"function": {"sig": {"inputs": [], "output": {"primitive": "i64"}}, "generics": generics_db, "header": header}}));
    let mut doc = serde_json::json!({
        "format_version": 61, "crate_version": "1.0.0", "root": 0,
        "index": {
            "4": describe,
            "0": item(0, "mini", serde_json::json!({"module": {"items": [10, 11, 13, 3, 4]}})),
            "10": item(10, "Database", serde_json::json!({"trait": {}})),
            "11": item(11, "Sqlite", serde_json::json!({"struct": {"impls": [12], "generics": none}})),
            "12": item(12, "", serde_json::json!({"impl": {"is_synthetic": false, "blanket_impl": null, "is_negative": false,
                "trait": {"path": "Database", "id": 10, "args": null}, "for": path(11, "Sqlite", serde_json::Value::Null), "items": [], "generics": none}})),
            "13": item(13, "Query", serde_json::json!({"struct": {"impls": [14], "generics": generics}})),
            "14": item(14, "", serde_json::json!({"impl": {"is_synthetic": false, "blanket_impl": null, "is_negative": false, "trait": null,
                "for": path(13, "Query", query_args.clone()), "items": [15], "generics": generics}})),
            "15": item(15, "bind", serde_json::json!({"function": {"sig": {"inputs": [["self", {"generic": "Self"}], ["value", {"primitive": "i64"}]], "output": {"generic": "Self"}},
                "generics": none, "header": header}})),
            "3": item(3, "query", serde_json::json!({"function": {"sig": {"inputs": [["sql", {"borrowed_ref": {"lifetime": "'q", "is_mutable": false, "type": {"primitive": "str"}}}]],
                "output": path(13, "Query", query_args)}, "generics": generics, "header": header}})),
        },
        "paths": {"10": {"path": ["mini", "Database"]}, "11": {"path": ["mini", "Sqlite"]}, "13": {"path": ["mini", "Query"]}}
    });
    if two_databases {
        let idx = doc["index"].as_object_mut().unwrap();
        idx.insert("16".into(), item(16, "Postgres", serde_json::json!({"struct": {"impls": [17], "generics": none}})));
        idx.insert("17".into(), item(17, "", serde_json::json!({"impl": {"is_synthetic": false, "blanket_impl": null, "is_negative": false,
            "trait": {"path": "Database", "id": 10, "args": null}, "for": path(16, "Postgres", serde_json::Value::Null), "items": [], "generics": none}})));
        idx["0"]["inner"]["module"]["items"].as_array_mut().unwrap().push(serde_json::json!(16));
        doc["paths"]["16"] = serde_json::json!({"path": ["mini", "Postgres"]});
    }
    doc.to_string()
}

#[test]
fn a_handle_owns_what_it_borrows_and_has_one_instantiation() {
    let ci = read_crate_with(&mini_rustdoc(), &[]).unwrap();
    // `Query<'q, DB>` is written with `'static` and the only database there is
    assert_eq!(ci.handle_types.get("mini.Query").map(String::as_str), Some("mini::Query<'static, mini::Sqlite>"));
    let canned = serde_json::json!({"responses": [
        {"prompt_prefix": "bind mini::query\n", "response": "Ok(mini::query(Box::leak(sql.into_boxed_str())))"},
        {"prompt_prefix": "bind mini::Query::bind\n", "response": "Ok(self_.bind(value))"},
    ]});
    let dir = temp("handles-lifetimes");
    write(&dir, "model.json", &canned.to_string());
    let model = MockModel::from_file(dir.join("model.json")).unwrap();
    let b = block_on(run_tiers(&ci, &model, &AcceptAll));
    // not skipped for the lifetime or the type parameter: the assisted tier writes the glue
    let query = b.items.iter().find(|i| i.name == "query").expect("query is bound");
    assert_eq!(query.tier, Tier::Assisted);
    assert_eq!(query.sig.heading("query"), "query(sql: text) -> handle of mini.Query");
    let bind_it = b.items.iter().find(|i| i.name == "query_bind").expect("bind is bound");
    assert_eq!(bind_it.sig.heading(bind_it.heading.as_deref().unwrap()), "bind(self: consume handle of mini.Query, value: int) -> handle of mini.Query");
    // the model is told what the handle is
    let item = ci.items.iter().find(|i| i.name == "query").unwrap();
    let agentmark::binder::core::Shape::Assisted { sig, why } = &item.shape else { panic!("assisted") };
    let prompt = agentmark::binder::core::pipeline::assist_prompt(&ci, item, sig, why);
    assert!(prompt.contains("The handle `mini.Query` is the Rust type `mini::Query<'static, mini::Sqlite>`"), "{prompt}");
    assert!(why.contains("the handle owns what the Rust value borrows"), "{why}");
    // the binding file says what the handle is, and reads back
    let text = agentmark::binder::core::render_binding(
        &b,
        "rust",
        agentmark::binder::core::ProposedEffect::Pure,
        "2026-10-14",
    );
    assert!(text.contains("## Handle: mini.Query\n  ```rust\n  mini::Query<'static, mini::Sqlite>\n  ```\n"), "{text}");
    // regression (review 1): `fmt` must not lose the `## Handle:` block
    assert_eq!(agentmark::fmt::format_source(&text, "mini.agent.md").unwrap(), text);
    let p = Project::from_memory("app/main.agent.md", vec![("app/main.agent.md", "# Import: crate mini\n"), ("app/bindings/mini.agent.md", &text), ("app/tools.toml", "")]);
    let checked = match sema::check(&p) {
        Ok(c) => c,
        Err(d) => {
            let errors: Vec<_> = d.into_iter().filter(|x| x.severity == Severity::Error).collect();
            assert!(errors.is_empty(), "{}", p.render(&errors));
            panic!("no checked result")
        }
    };
    assert_eq!(checked.bindings[0].handles.get("mini.Query").map(String::as_str), Some("mini::Query<'static, mini::Sqlite>"));
}

#[test]
fn a_type_parameter_with_two_implementors_has_no_instantiation_and_other_generics_keep_v02() {
    // regression (review 4): one implementor makes `DB` obvious for a handle item only
    let one = read_crate_with(&mini_rustdoc(), &[]).unwrap();
    let describe = one.items.iter().find(|i| i.name == "describe").unwrap();
    assert!(matches!(&describe.shape, agentmark::binder::core::Shape::Assisted { why, .. } if why.contains("generic")), "{:?}", describe.shape);
    // two implementors: the handle item is not bound, and nothing is recorded for the Rust type
    let two = read_crate_with(&mini_rustdoc_with(true), &[]).unwrap();
    let query = two.items.iter().find(|i| i.name == "query").unwrap();
    assert!(matches!(&query.shape, agentmark::binder::core::Shape::Skipped { .. }), "{:?}", query.shape);
    assert!(two.handle_types.is_empty(), "{:?}", two.handle_types);
}

#[test]
fn a_handle_block_for_a_handle_no_item_uses_is_an_error() {
    let text = "# Binding: mini\n- crate: mini\n- effect: pure\n\n## Handle: mini.Nothing\n  ```rust\n  mini::Nothing\n  ```\n";
    let p = Project::from_memory("app/main.agent.md", vec![("app/main.agent.md", "# Import: crate mini\n"), ("app/bindings/mini.agent.md", text), ("app/tools.toml", "")]);
    let e = match sema::check(&p) {
        Ok(_) => String::new(),
        Err(d) => p.render(&d.into_iter().filter(|x| x.severity == Severity::Error).collect::<Vec<_>>()),
    };
    assert!(e.contains("No item of this binding uses the handle \"mini.Nothing\"."), "{e}");
}

#[test]
fn crates_in_agentmark_toml_carry_features() {
    let toml = "name = \"p\"\n\n[crates]\nsqlx = { version = \"0.8\", features = [\"sqlite\", \"runtime-tokio\"] }\nserde = \"1\"\n";
    let p = Project::from_memory("app/main.agent.md", vec![("app/main.agent.md", "# Import: crate sqlx\n"), ("app/agentmark.toml", toml), ("app/tools.toml", "")]);
    let c = &p.config.crates;
    assert_eq!(c.len(), 2);
    let sqlx = c.iter().find(|c| c.name == "sqlx").unwrap();
    assert_eq!((sqlx.version.as_deref(), sqlx.features.as_slice(), sqlx.default_features), (Some("0.8"), ["sqlite".to_string(), "runtime-tokio".to_string()].as_slice(), true));
    assert!(p.load_diagnostics.is_empty() || !p.load_diagnostics.iter().any(|d| d.message.contains("agentmark.toml")), "{:?}", p.load_diagnostics);
    // a misspelled key is an error with a suggestion
    let bad = "[crates]\nsqlx = { version = \"0.8\", feature = [\"sqlite\"] }\n";
    let p = Project::from_memory("app/main.agent.md", vec![("app/main.agent.md", "# Import: crate sqlx\n"), ("app/agentmark.toml", bad), ("app/tools.toml", "")]);
    let msg = p.load_diagnostics.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("\n");
    assert!(msg.contains("Cannot read \"feature\" of the crate \"sqlx\""), "{msg}");
}

// ------------------------------------------------------------------------------ sema

const HASH_BINDING: &str = "# Binding: sha2\ngenerated by agentmark bind — sha2 0.10.9 — 2026-10-14\n\n- crate: sha2\n- effect: pure\n\n## sha256_new() -> handle of sha2.Sha256\nCreate a new hasher.\n\n  ```rust\n  Ok(<sha2::Sha256 as sha2::Digest>::new())\n  ```\n\n## sha224_new() -> handle of sha2.Sha224\nCreate a new hasher with prefix.\n\n  ```rust\n  Ok(<sha2::Sha224 as sha2::Digest>::new())\n  ```\n\n## update(self: mut handle of sha2.Sha256, data: bytes)\nProcess input data.\n\n  ```rust\n  <sha2::Sha256 as sha2::Digest>::update(self_, data);\n  Ok(())\n  ```\n\n## update(self: mut handle of sha2.Sha224, data: bytes)\nProcess input data.\n\n  ```rust\n  <sha2::Sha224 as sha2::Digest>::update(self_, data);\n  Ok(())\n  ```\n\n## finalize(self: consume handle of sha2.Sha256) -> bytes\nRetrieve result and consume the hasher.\n\n  ```rust\n  Ok(<sha2::Sha256 as sha2::Digest>::finalize(self_).to_vec())\n  ```\n\n## finalize_reset(self: mut handle of sha2.Sha256) -> bytes\nRetrieve result and reset.\n\n  ```rust\n  Ok(<sha2::Sha256 as sha2::Digest>::finalize_reset(self_).to_vec())\n  ```\n\n## chain_update(self: consume handle of sha2.Sha256, data: bytes) -> handle of sha2.Sha256\nAdd data to a copy of this hasher.\n\n  ```rust\n  Ok(<sha2::Sha256 as sha2::Digest>::chain_update(self_, data))\n  ```\n\n## same_state(self: handle of sha2.Sha256, other: handle of sha2.Sha256) -> bool\nWhether both hashers are the same.\n\n  ```rust\n  Ok(false)\n  ```\n\n## absorb(self: mut handle of sha2.Sha256, other: consume handle of sha2.Sha256)\nCombine two.\n\n  ```rust\n  Ok(())\n  ```\n";

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

const HEAD: &str = "# Import: crate sha2\n\n# Command: go\nHashes.\n\n- in: word: text\n- steps:\n";

fn prog(lines: &[&str]) -> String {
    // top-level lines are numbered here; indented lines are the numbered steps under them
    let mut n = 0;
    let body: String = lines
        .iter()
        .map(|l| {
            if l.starts_with(' ') {
                format!("  {l}\n")
            } else {
                n += 1;
                format!("  {n}. {l}\n")
            }
        })
        .collect();
    format!("{HEAD}{body}")
}

#[test]
fn a_hasher_is_fed_in_pieces_and_finished_once() {
    ok(&prog(&[
        "sha2 sha256_new into hasher",
        "hasher update word",
        "hasher update \"more\"",
        "hasher finalize into digest",
        "print: \"hashed\"",
    ]));
}

#[test]
fn methods_with_one_name_belong_to_their_handle_type() {
    ok(&prog(&[
        "sha2 sha256_new into a",
        "sha2 sha224_new into b",
        "a update word",
        "b update word",
    ]));
    // `finalize` exists on Sha256 only in this binding
    let e = errors_of(&prog(&["sha2 sha224_new into b", "b finalize into d"]));
    assert!(e.contains("The handle \"b\" (sha2.Sha224) has no method \"finalize\"."), "{e}");
}

#[test]
fn using_a_handle_after_a_consuming_call_names_the_line_that_used_it_up() {
    let e = errors_of(&prog(&[
        "sha2 sha256_new into hasher",
        "hasher finalize into digest",
        "hasher update \"late\"",
    ]));
    // line 9 is `hasher finalize into digest`: the steps start on line 8
    assert!(e.contains("app/main.agent.md:10\n    3. hasher update \"late\"\n       ^^^^^^"), "{e}");
    assert!(e.contains("\"hasher\" was used up on line 9 (\"hasher finalize into digest\")"), "{e}");
    assert_eq!(e.matches("was used up").count(), 1, "{e}");
}

/// A function that only borrows the handles it is given leaves them with the caller (Q414).
#[test]
fn a_function_that_only_borrows_a_handle_leaves_it_with_the_caller() {
    let peek = "# Import: crate sha2\n\n# Function: peek\nCompares.\n\n- in: h: handle of sha2.Sha256, g: handle of sha2.Sha256\n- out: bool\n- touches: nothing\n- steps:\n  1. h same_state g into same\n  2. return same\n\n";
    let go = "# Command: go\nHashes.\n\n- in: word: text\n- steps:\n  1. sha2 sha256_new into a\n  2. sha2 sha256_new into b\n  3. do peek(a, b) into same\n  4. a update word\n  5. b finalize into digest\n";
    ok(&format!("{peek}{go}"));
    // a function that uses the handle up takes it from the caller
    let eat = "# Import: crate sha2\n\n# Function: eat\nEats.\n\n- in: h: handle of sha2.Sha256\n- out: bytes\n- touches: nothing\n- steps:\n  1. h finalize into d\n  2. return d\n\n";
    let e = errors_of(&format!("{eat}# Command: go\nHashes.\n\n- in: word: text\n- steps:\n  1. sha2 sha256_new into a\n  2. do eat(a) into d\n  3. a update word\n"));
    assert!(e.contains("\"a\" was used up on line"), "{e}");
}

#[test]
fn passing_a_handle_on_uses_it_up_too() {
    let e = errors_of(
        "# Import: crate sha2\n\n# Function: feed\nFeeds.\n\n- in: h: handle of sha2.Sha256\n- touches: nothing\n- steps:\n  1. h update \"x\"\n\n# Command: go\nHashes.\n\n- in: word: text\n- steps:\n  1. sha2 sha256_new into hasher\n  2. do feed(hasher)\n  3. hasher update word\n",
    );
    assert!(e.contains("\"hasher\" was used up on line 17"), "{e}");
    let e = errors_of(&prog(&["sha2 sha256_new into a", "a into b", "a update word"]));
    assert!(e.contains("\"a\" was used up on line 9 (\"a into b\")"), "{e}");
    // a handle can be handed on and handed back
    ok(
        "# Import: crate sha2\n\n# Function: feed\nFeeds.\n\n- in: h: handle of sha2.Sha256\n- out: handle of sha2.Sha256\n- touches: nothing\n- steps:\n  1. h update \"x\"\n  2. return h\n\n# Command: go\nHashes.\n\n- in: word: text\n- steps:\n  1. sha2 sha256_new into hasher\n  2. do feed(hasher) into back\n  3. back finalize into digest\n",
    );
}

#[test]
fn a_handle_used_up_in_one_branch_may_be_used_up_afterwards() {
    let e = errors_of(&prog(&[
        "sha2 sha256_new into hasher",
        "if word = \"a\":",
        "   1. hasher finalize into early",
        "hasher update word",
    ]));
    assert!(e.contains("\"hasher\" was used up on line 10"), "{e}");
    // a branch that leaves contributes nothing
    ok(&prog(&[
        "sha2 sha256_new into hasher",
        "if word = \"a\":",
        "   1. hasher finalize into early",
        "   1. stop",
        "hasher update word",
    ]));
}

#[test]
fn a_handle_that_lives_outside_a_loop_cannot_be_used_up_inside_it() {
    let e = errors_of(&format!(
        "{HEAD}  1. sha2 sha256_new into hasher\n  2. for each part in list_of_parts:\n     1. hasher finalize into d\n"
    ));
    assert!(e.contains("is used up here, but this line is inside a loop"), "{e}");
    // made inside the loop, it is a new one every round
    let e = errors_of(
        "# Import: crate sha2\n\n# Command: go\nHashes.\n\n- in: parts: list of text\n- steps:\n  1. for each part in parts:\n     1. sha2 sha256_new into hasher\n     2. hasher update part\n     3. hasher finalize into d\n",
    );
    assert!(e.is_empty(), "{e}");
}

#[test]
fn a_handle_cannot_be_printed_compared_saved_or_sent() {
    for (line, what) in [
        ("print: hasher", "print"),
        ("print: \"state {hasher}\"", "interpolate"),
        ("hasher = hasher into same", "compare"),
        ("ask model with hasher: \"hi\" into answer", "ask"),
    ] {
        let e = errors_of(&prog(&["sha2 sha256_new into hasher", line]));
        assert!(
            e.contains("is handle of sha2.Sha256, a library value that cannot be printed, compared, saved, sent or traced"),
            "{what}: {e}"
        );
    }
    let e = errors_of("# Import: crate sha2\n\n# Record: Doc\n- name: text\n- hasher: handle of sha2.Sha256\n");
    assert!(e.contains("A record is saved and sent as JSON, so it cannot have a field of handle of sha2.Sha256"), "{e}");
}

#[test]
fn handles_do_not_nest_in_lists_or_maybes() {
    let e = errors_of("# Import: crate sha2\n\n# Function: f\nTakes.\n\n- in: hs: list of handle of sha2.Sha256\n- touches: nothing\n- steps:\n  1. stop\n");
    assert!(e.contains("A handle cannot be in a list: handle of sha2.Sha256."), "{e}");
    let e = errors_of("# Import: crate sha2\n\n# Function: f\nTakes.\n\n- in: h: maybe handle of sha2.Sha256\n- touches: nothing\n- steps:\n  1. stop\n");
    assert!(e.contains("A handle cannot be in a maybe"), "{e}");
}

#[test]
fn an_unknown_handle_type_is_suggested_from_the_bindings() {
    let e = errors_of("# Import: crate sha2\n\n# Function: f\nTakes.\n\n- in: h: handle of sha2.Sha265\n- touches: nothing\n- steps:\n  1. stop\n");
    assert!(e.contains("No binding declares the handle type \"sha2.Sha265\"."), "{e}");
    assert!(e.contains("Did you mean \"sha2.Sha256\"?"), "{e}");
}

#[test]
fn borrow_modes_belong_to_bindings_and_arguments_must_be_variables() {
    let e = errors_of("# Import: crate sha2\n\n# Function: f\nTakes.\n\n- in: h: mut handle of sha2.Sha256\n- touches: nothing\n- steps:\n  1. stop\n");
    assert!(e.contains("\"mut\" in front of \"handle of\" only belongs in a binding's parameter list"), "{e}");
    // `same_state` borrows both handles: the same one twice, or a new value, is refused
    let e = errors_of(&prog(&["sha2 sha256_new into a", "a same_state a into same"]));
    assert!(e.contains("\"a\" is given twice in one call."), "{e}");
    ok(&prog(&["sha2 sha256_new into a", "sha2 sha256_new into b", "a same_state b into same", "print: \"{same}\""]));
    // `absorb` uses up its second handle
    let e = errors_of(&prog(&["sha2 sha256_new into a", "sha2 sha256_new into b", "a absorb b", "b update word"]));
    assert!(e.contains("\"b\" was used up on line 10"), "{e}");
}

#[test]
fn a_parallel_block_cannot_use_one_handle_twice() {
    let e = errors_of(&prog(&[
        "sha2 sha256_new into a",
        "sha2 sha256_new into b",
        "parallel:",
        "   1. a update \"x\"",
        "   1. b update \"y\"",
    ]));
    assert!(e.is_empty(), "{e}");
    let e = errors_of(&prog(&["sha2 sha256_new into a", "parallel:", "   1. a update \"x\"", "   1. a update \"y\""]));
    assert!(e.contains("both use the handle \"a\" (the first is on line 10)"), "{e}");
}

#[test]
fn a_misspelled_handle_variable_is_suggested() {
    let e = errors_of(&prog(&["sha2 sha256_new into hasher", "hashr update word"]));
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
        let words = h.split(|c: char| !(c.is_alphanumeric() || c == '.' || c == '_'));
        assert!(!words.into_iter().any(|w| w == "chrono.NaiveDate" || w == "chrono.DateTime"), "{h}");
    }
}

const MK: &str = "# Import: crate sha2\n\n# Function: mk\nMakes one.\n\n- out: handle of sha2.Sha256\n- touches: nothing\n- steps:\n  1. sha2 sha256_new into h\n  2. return h\n\n";

#[test]
fn a_handle_that_is_not_a_variable_is_refused_where_nothing_takes_it() {
    // regression (review 2): the check is by type, not by the shape of the expression
    for (line, what) in [
        ("print: mk()", "print"),
        ("print: \"x {mk()}\"", "interpolation"),
        ("mk() = mk() into same", "comparison"),
        ("{ name: \"a\", h: mk() } into rec", "record literal"),
    ] {
        let e = errors_of(&format!("{MK}# Command: go\nHashes.\n\n- in: word: text\n- steps:\n  1. {line}\n"));
        assert!(e.contains("a library value that cannot be printed, compared, saved, sent or traced"), "{what}: {e}");
    }
    // handed on, it is fine
    ok(&format!("{MK}# Command: go\nHashes.\n\n- in: word: text\n- steps:\n  1. mk() into h\n  2. h update word\n"));
}

#[test]
fn a_handle_cannot_come_from_outside_the_program() {
    let e = errors_of("# Import: crate sha2\n\n# Command: go\nHashes.\n\n- in: h: handle of sha2.Sha256\n- steps:\n  1. h update \"x\"\n");
    assert!(e.contains("The command argument \"h\" comes from outside the program, and a handle cannot"), "{e}");
}

#[test]
fn a_parallel_block_cannot_lend_one_handle_to_two_statements() {
    // regression (review 6): arguments count, not only receivers
    let e = errors_of(&prog(&[
        "sha2 sha256_new into a",
        "sha2 sha256_new into b",
        "parallel:",
        "   1. a same_state b into x",
        "   1. b same_state a into y",
    ]));
    assert!(e.contains("both use the handle \"b\""), "{e}");
}

#[test]
fn a_try_that_makes_a_handle_must_leave_when_it_fails() {
    let e = errors_of(&prog(&["try sha2 sha256_new into hasher", "   1. otherwise:", "   1. print: \"no\""]));
    assert!(e.contains("\"hasher\" is a handle, and there is none to use when this step fails."), "{e}");
    ok(&prog(&["try sha2 sha256_new into hasher", "   1. otherwise:", "   1. stop", "hasher update word"]));
}

#[test]
fn handle_types_parse_and_print_in_the_canonical_style() {
    let src = "# Import: crate sha2\n\n# Function: f\nTakes one and gives it back.\n\n- in: h: handle of sha2.Sha256, n: int\n- out: handle of sha2.Sha256\n- touches: nothing\n- steps:\n  1. return h\n";
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

const HASH_COMMAND: &str = "# Uses\n- [invoice](../records/invoice.agent.md)\n\n# Import: crate sha2\n\n# Command: hash-pieces\nHashes two pieces of text with one hasher, fed one after the other.\n\n- in: first: text, second: text\n- steps:\n  1. sha2 sha256_new into hasher\n  2. hasher update first\n  3. hasher settle into turns\n  4. print: \"settled after {turns} turns\"\n  5. do add_more(hasher, second) into hasher\n  6. hasher finalize into digest\n  7. print: \"{hex(digest)}\"\n\n# Function: add_more\nFeeds text to a hasher and hands it back.\n\n- in: h: handle of sha2.Sha256, more: text\n- out: handle of sha2.Sha256\n- touches: nothing\n- steps:\n  1. h update more\n  2. return h\n\n## Rust: hex -> text\n- in: digest: bytes\n- touches: nothing\n\n```rust\ndigest.iter().map(|b| format!(\"{:02x}\", b)).collect::<String>()\n```\n";

const TWICE_COMMAND: &str = "# Import: crate sha2\n\n# Command: hash-twice\nHashes `first`; when `second` is \"again\", starts a new hasher inside the branch and hashes that too.\n\n- in: first: text, second: text\n- steps:\n  1. sha2 sha256_new into hasher\n  2. hasher update first\n  3. if second = \"again\":\n     1. hasher finalize into early\n     2. print: \"{hex2(early)}\"\n     3. sha2 sha256_new into hasher\n     4. hasher update second\n     5. hasher finalize into late\n     6. print: \"{hex2(late)}\"\n     7. stop\n  4. hasher finalize into digest\n  5. print: \"{hex2(digest)}\"\n\n## Rust: hex2 -> text\n- in: digest: bytes\n- touches: nothing\n\n```rust\ndigest.iter().map(|b| format!(\"{:02x}\", b)).collect::<String>()\n```\n";

/// Example 05 (service, records, a command) plus a command that hashes in pieces: its sha2 binding
/// is generated from the checked-in rustdoc, so it holds the handle methods.
fn hash_project(d: &Path) -> PathBuf {
    let proj = d.join("invoice-service");
    copy_dir(Path::new(&format!("{ROOT}/examples/05-invoice-service")), &proj);
    std::fs::remove_file(proj.join("bindings/sha2.agent.md")).unwrap();
    write(&proj, "commands/hash-pieces.agent.md", HASH_COMMAND);
    write(&proj, "commands/hash-twice.agent.md", TWICE_COMMAND);
    // the library is bound and built with the features `agentmark.toml` names (Q371)
    let toml = std::fs::read_to_string(proj.join("agentmark.toml")).unwrap();
    write(&proj, "agentmark.toml", &format!("{toml}\n[crates]\nsha2 = {{ default-features = false, features = [\"std\"] }}\n"));
    let main = std::fs::read_to_string(proj.join("main.agent.md")).unwrap();
    let main = main
        .replace("# Uses\n- [import-csv](./commands/import-csv.agent.md)\n", "# Uses\n- [hash-pieces](./commands/hash-pieces.agent.md)\n- [import-csv](./commands/import-csv.agent.md)\n")
        .replace("- import-csv\n", "- import-csv\n- hash-pieces\n- hash-twice\n")
        .replace("# Uses\n- [hash-pieces](./commands/hash-pieces.agent.md)\n", "# Uses\n- [hash-pieces](./commands/hash-pieces.agent.md)\n- [hash-twice](./commands/hash-twice.agent.md)\n");
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
    let settle = "## settle(self: handle of sha2.Sha256) -> int\nWaits for the runtime to run something else first.\n\n```rust\ntokio::task::yield_now().await;\nOk(1)\n```\n\n## Not bound\n";
    let text = text.replace("- effect: unverified", "- effect: pure").replace("## Not bound\n", settle);
    write(&proj, "bindings/sha2.agent.md", &text);
    let second = build();
    assert!(second.status.success(), "{}", stderr(&second));

    let cargo = std::fs::read_to_string(out.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("sha2 = { version = \"=0.10.9\", default-features = false, features = [\"std\"] }"), "{cargo}");
    let bin = build_bin(&out, "invoice-service");
    let run = std::process::Command::new(&bin).args(["hash-pieces", "hello ", "world"]).current_dir(&d).output().unwrap();
    assert!(run.status.success(), "{}", stderr(&run));
    let said: Vec<String> = stdout(&run).lines().map(String::from).collect();
    assert_eq!(said, ["settled after 1 turns", HELLO_WORLD], "{}", stdout(&run));
    // pieces split elsewhere give the same digest: the hasher keeps its state between the calls
    let run = std::process::Command::new(&bin).args(["hash-pieces", "hello w", "orld"]).current_dir(&d).output().unwrap();
    assert_eq!(stdout(&run).lines().last(), Some(HELLO_WORLD));
    // regression (review 5): a handle made again inside a branch is a new variable there; the
    // outer one was used up in the branch and is not touched afterwards
    let sha = |a: &str, b: &str| {
        let o = std::process::Command::new(&bin).args(["hash-twice", a, b]).current_dir(&d).output().unwrap();
        assert!(o.status.success(), "{}", stderr(&o));
        stdout(&o).lines().map(String::from).collect::<Vec<_>>()
    };
    let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let again = "b4c9e14061c2fd453b36700e3b0da008db2189c711ac629f0f583089164e267d";
    assert_eq!(sha("abc", "again"), [abc, again]);
    assert_eq!(sha("abc", "other"), [abc]);
}
