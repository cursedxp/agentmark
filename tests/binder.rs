//! Binder tests (PROMPT step 8): sha2, chrono and reqwest rustdoc fixtures, a canned
//! `MockModel`, a fake type checker for most tests and the real `cargo check` for a few.

use agentmark::binder::core::{
    freshness, run_tiers, BoundCrate, CheckRequest, Freshness, Tier, TypeChecker,
};
use agentmark::binder::rust::{read_crate, CargoChecker, FetchSpec};
use agentmark::binder::{bind, doc_cheatsheet, BindOptions, Regenerated};
use agentmark::syntax::ast::{Decl, Effect, File};
use agentmark_rt::MockModel;
use chrono::NaiveDate;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CRATES: [(&str, &str); 3] = [("sha2", "0.10.9"), ("chrono", "0.4.45"), ("reqwest", "0.12.28")];

fn fixture(c: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/binder").join(format!("{c}.json"))
}

fn model() -> Arc<MockModel> {
    Arc::new(MockModel::from_file(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/binder/model.json")).unwrap())
}

/// Accepts every body except those of the named items.
struct FakeChecker {
    reject: Vec<&'static str>,
}

impl TypeChecker for FakeChecker {
    fn check(&self, req: &CheckRequest<'_>) -> Result<(), String> {
        if self.reject.contains(&req.name) {
            Err(format!("error[E0308]: mismatched types in {}", req.name))
        } else {
            Ok(())
        }
    }
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("agentmark-binder-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn opts(dir: &Path, c: &str, reject: Vec<&'static str>) -> BindOptions {
    let mut o = BindOptions::new(dir, model(), NaiveDate::from_ymd_opt(2026, 10, 14).unwrap());
    o.rustdoc_json = Some(fixture(c));
    o.checker = Some(Box::new(FakeChecker { reject }));
    o.foreign_rustdoc = vec![fixture("digest")];
    o.offline = true;
    o
}

fn parse(path: &Path) -> File {
    let text = std::fs::read_to_string(path).unwrap();
    match agentmark::syntax::parser::parse_file(&path.display().to_string(), &text) {
        Ok(f) => f,
        Err(d) => panic!("{}", agentmark::diag::render_all(&d, &text)),
    }
}

fn binding(f: &File) -> &agentmark::syntax::ast::Binding {
    f.decls
        .iter()
        .find_map(|d| if let Decl::Binding(b) = d { Some(b) } else { None })
        .expect("a # Binding declaration")
}

fn tiered(c: &str, reject: Vec<&'static str>) -> (agentmark::binder::core::CrateItems, BoundCrate) {
    let json = std::fs::read_to_string(fixture(c)).unwrap();
    let foreign: Vec<String> = if c == "sha2" {
        vec![std::fs::read_to_string(fixture("digest")).unwrap()]
    } else {
        vec![]
    };
    let ci = agentmark::binder::rust::read_crate_with(&json, &foreign).unwrap();
    let bound = futures_block(run_tiers(&ci, &*model(), &FakeChecker { reject }));
    (ci, bound)
}

fn futures_block<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(f)
}

#[test]
fn every_public_item_lands_in_exactly_one_tier() {
    for (c, v) in CRATES {
        let (ci, b) = tiered(c, vec![]);
        assert_eq!(ci.version, v);
        // One placement per item plus one per listed non-callable (trait, foreign re-export).
        assert_eq!(b.placements.len(), ci.items.len() + ci.extra_not_bound.len(), "{c}");
        assert_eq!(b.counts.direct + b.counts.assisted + b.counts.skipped, b.placements.len(), "{c}");
        // Bound and not-bound names partition the input: nothing dropped, nothing twice.
        let bound: HashSet<&str> = b.items.iter().map(|i| i.name.as_str()).collect();
        let skipped: HashSet<&str> = b.not_bound.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(bound.len(), b.items.len(), "{c}: duplicate bound name");
        assert_eq!(skipped.len(), b.not_bound.len(), "{c}: duplicate not-bound name");
        assert!(bound.is_disjoint(&skipped), "{c}");
        let all: HashSet<&str> = ci
            .items
            .iter()
            .map(|i| i.name.as_str())
            .chain(ci.extra_not_bound.iter().map(|e| e.name.as_str()))
            .collect();
        assert_eq!(all.len(), ci.items.len() + ci.extra_not_bound.len(), "{c}: names not unique");
        assert_eq!(bound.union(&skipped).copied().collect::<HashSet<_>>(), all, "{c}");
        assert_eq!(b.placements.iter().filter(|(_, t)| *t == Tier::Skipped).count(), b.not_bound.len(), "{c}");
    }
}

#[test]
fn chrono_uses_all_three_tiers() {
    let (_, b) = tiered("chrono", vec![]);
    assert!(b.counts.direct >= 50, "{:?}", b.counts);
    assert_eq!(b.counts.assisted, 6, "six canned bodies in model.json: {:?}", b.counts);
    assert!(b.counts.skipped > 100);
    let direct = b.items.iter().find(|i| i.name == "naive_date_from_ymd_opt").unwrap();
    assert_eq!(direct.tier, Tier::Direct);
    assert_eq!(
        direct.sig.heading(&direct.name),
        "naive_date_from_ymd_opt(year: int, month: int, day: int) -> maybe date"
    );
}

#[test]
fn generated_files_parse_as_unverified_bindings_with_doc_lines() {
    for (c, v) in CRATES {
        let dir = scratch(c);
        let out = bind(c, &opts(&dir, c, vec![])).unwrap();
        assert!(out.generated, "{c}");
        assert_eq!(out.reason, Some(Regenerated::Missing));
        assert_eq!(out.path, dir.join(format!("{c}.agent.md")));
        let text = std::fs::read_to_string(&out.path).unwrap();
        assert!(
            text.starts_with(&format!("> generated by agentmark bind \u{2014} {c} {v} \u{2014} 2026-10-14\n> edit freely; re-run `agentmark bind {c} --force` to regenerate\n# Binding: {c}\n")),
            "{c}: {}",
            text.lines().take(4).collect::<Vec<_>>().join(" | ")
        );
        let f = parse(&out.path);
        let b = binding(&f);
        assert_eq!(b.crate_name.name, c);
        assert_eq!(b.effect.value, Effect::Unverified, "{c}");
        assert!(b.proposed_effect.is_some(), "{c}");
        assert_eq!(b.items.len(), out.counts.bound(), "{c}");
        assert_eq!(b.not_bound.len(), out.counts.skipped, "{c}");
        assert_eq!(out.bound, b.items.len());
        for it in &b.items {
            let doc = it.doc.as_deref().unwrap_or_else(|| panic!("{c}: {} has no doc line", it.name.name));
            assert!(!doc.trim().is_empty() && !doc.contains('\n'), "{c}: {}", it.name.name);
        }
        for n in &b.not_bound {
            assert!(n.reason.as_deref().is_some_and(|r| !r.is_empty()), "{c}: {} has no reason", n.item);
        }
    }
}

#[test]
fn proposed_effects_follow_the_heuristics() {
    let want = [("sha2", Effect::Pure), ("chrono", Effect::Pure), ("reqwest", Effect::Network)];
    for (c, e) in want {
        let dir = scratch(&format!("eff-{c}"));
        let out = bind(c, &opts(&dir, c, vec![])).unwrap();
        let f = parse(&out.path);
        assert_eq!(binding(&f).proposed_effect.as_ref().unwrap().value, e, "{c}");
    }
}

#[test]
fn doc_lines_come_from_the_rustdoc_first_sentence() {
    let dir = scratch("docline");
    let out = bind("chrono", &opts(&dir, "chrono", vec![])).unwrap();
    let f = parse(&out.path);
    let b = binding(&f);
    let it = b.items.iter().find(|i| i.name.name == "naive_date_from_ymd_opt").unwrap();
    assert!(it.doc.as_deref().unwrap().starts_with("Makes a new"), "{:?}", it.doc);
    assert!(!it.doc.as_deref().unwrap().contains("]("), "links are flattened: {:?}", it.doc);
}

#[test]
fn a_body_that_fails_the_type_check_is_skipped_with_the_reason() {
    let dir = scratch("typecheck");
    let out = bind("chrono", &opts(&dir, "chrono", vec!["date_time_to_utc"])).unwrap();
    assert_eq!(out.counts.assisted, 5);
    let f = parse(&out.path);
    let b = binding(&f);
    assert!(b.items.iter().all(|i| i.name.name != "date_time_to_utc"));
    let n = b.not_bound.iter().find(|n| n.item == "date_time_to_utc").expect("listed as not bound");
    let r = n.reason.as_deref().unwrap();
    assert!(r.contains("failed type-check") && r.contains("mismatched types"), "{r}");
}

#[test]
fn an_item_the_model_has_no_body_for_is_skipped_not_dropped() {
    let (_, b) = tiered("chrono", vec![]);
    let (_, why) = b.not_bound.iter().find(|(n, _)| n == "date_time_hour").expect("no canned body");
    assert!(why.contains("mock model has no response for date_time_hour"), "{why}");
}

#[test]
fn fenced_model_answers_are_unwrapped() {
    let (_, b) = tiered("chrono", vec![]);
    let y = b.items.iter().find(|i| i.name == "date_time_year").unwrap();
    assert_eq!(y.body, "Ok(i64::from(chrono::Datelike::year(&date_time)))");
    assert_eq!(y.tier, Tier::Assisted);
}

#[test]
fn skip_reasons_cover_the_spec_cases() {
    let (_, b) = tiered("reqwest", vec![]);
    let reasons: String = b.not_bound.iter().map(|(_, r)| r.as_str()).collect::<Vec<_>>().join("\n");
    assert!(reasons.contains("lifetimes in the public API"));
    assert!(reasons.contains("associated types"));
    assert!(reasons.contains("mutates its receiver"));
    assert!(reasons.contains("has no AgentMark type"));
    let (_, b) = tiered("sha2", vec![]);
    assert!(b.not_bound.iter().any(|(_, r)| r.contains("another crate")), "foreign re-exports are listed");
    let (_, b) = tiered("chrono", vec![]);
    assert!(b.not_bound.iter().any(|(n, r)| n == "trait_datelike" && r.contains("trait")));
}

#[test]
fn cache_hit_leaves_the_file_alone() {
    let dir = scratch("cache");
    let first = bind("sha2", &opts(&dir, "sha2", vec![])).unwrap();
    let before = std::fs::read_to_string(&first.path).unwrap();
    let again = bind("sha2", &opts(&dir, "sha2", vec![])).unwrap();
    assert!(first.generated && !again.generated);
    assert_eq!(again.reason, None);
    assert_eq!(again.counts.skipped, first.counts.skipped);
    assert_eq!(again.bound, first.bound);
    assert_eq!(std::fs::read_to_string(&again.path).unwrap(), before);
}

#[test]
fn a_binding_for_another_version_is_stale_and_rebinding_re_marks_unverified() {
    let dir = scratch("stale");
    let first = bind("sha2", &opts(&dir, "sha2", vec![])).unwrap();
    // Pretend the file was generated for 0.9.0 and a human approved it.
    let old = std::fs::read_to_string(&first.path)
        .unwrap()
        .replace("sha2 0.10.9", "sha2 0.9.0")
        .replace("- effect: unverified", "- effect: pure");
    std::fs::write(&first.path, &old).unwrap();
    assert_eq!(
        freshness(&first.path, "sha2", "0.10.9").unwrap(),
        Freshness::Stale { bound_version: "0.9.0".into() }
    );
    assert_eq!(freshness(&first.path, "sha2", "0.9.0").unwrap(), Freshness::Fresh);
    assert_eq!(freshness(&dir.join("nope.agent.md"), "x", "1").unwrap(), Freshness::Missing);

    // plain `bind` keeps the version of the header (no silent jump to the newest release) ...
    let kept = bind("sha2", &opts(&dir, "sha2", vec![])).unwrap();
    assert!(!kept.generated && kept.version == "0.9.0", "{kept:?}");
    assert_eq!(std::fs::read_to_string(&first.path).unwrap(), old, "and writes nothing");
    // ... moving needs `--version <v>` (or `--latest`)
    let mut o = opts(&dir, "sha2", vec![]);
    o.version = Some("0.10.9".into());
    let again = bind("sha2", &o).unwrap();
    assert!(again.generated);
    assert_eq!(again.reason, Some(Regenerated::Stale { from: "0.9.0".into() }));
    assert_eq!(again.effect, "unverified");
    let f = parse(&again.path);
    assert_eq!(binding(&f).effect.value, Effect::Unverified);
    assert!(std::fs::read_to_string(&again.path).unwrap().contains("sha2 0.10.9"));
    // The approved old file is kept next to the new one.
    assert_eq!(std::fs::read_to_string(dir.join("sha2.agent.md.prev")).unwrap(), old);
}

#[test]
fn an_approved_binding_is_never_overwritten_without_force() {
    let dir = scratch("force");
    let first = bind("chrono", &opts(&dir, "chrono", vec![])).unwrap();
    let approved = std::fs::read_to_string(&first.path).unwrap().replace("- effect: unverified", "- effect: pure");
    std::fs::write(&first.path, &approved).unwrap();

    let kept = bind("chrono", &opts(&dir, "chrono", vec![])).unwrap();
    assert!(!kept.generated);
    assert_eq!(kept.effect, "pure");
    assert_eq!(std::fs::read_to_string(&first.path).unwrap(), approved);

    let mut o = opts(&dir, "chrono", vec![]);
    o.force = true;
    let forced = bind("chrono", &o).unwrap();
    assert!(forced.generated);
    assert_eq!(forced.reason, Some(Regenerated::Forced));
    assert_eq!(forced.effect, "unverified");
    assert!(std::fs::read_to_string(&first.path).unwrap().contains("- effect: unverified"));
}

#[test]
fn a_hand_written_binding_without_header_is_left_alone() {
    let dir = scratch("hand");
    let p = dir.join("sha2.agent.md");
    let hand = "# Binding: sha2\n- crate: sha2\n- effect: pure\n";
    std::fs::write(&p, hand).unwrap();
    let out = bind("sha2", &opts(&dir, "sha2", vec![])).unwrap();
    assert!(!out.generated);
    assert_eq!(out.effect, "pure");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), hand);
}

#[test]
fn rustdoc_json_for_another_version_is_refused() {
    let dir = scratch("version-mismatch");
    let mut o = opts(&dir, "sha2", vec![]);
    o.version = Some("9.9.9".into());
    let e = bind("sha2", &o).unwrap_err().to_string();
    assert!(e.contains("0.10.9") && e.contains("9.9.9"), "{e}");
}

#[test]
fn cheat_sheet_lists_items_trust_and_not_bound() {
    let dir = scratch("cheat");
    let out = bind("chrono", &opts(&dir, "chrono", vec![])).unwrap();
    let s = doc_cheatsheet(&out.path).unwrap();
    assert!(s.starts_with("chrono 0.4.45 (bound 2026-10-14)\n"), "{s}");
    assert!(s.contains("effect: unverified (proposed: pure;"), "{s}");
    assert!(s.contains("chrono.naive_date_from_ymd_opt(year: int, month: int, day: int) -> maybe date"), "{s}");
    assert!(s.contains("Not bound"), "{s}");
}

// ------------------------------------------------------------ real cargo

#[test]
fn real_cargo_check_accepts_a_good_body_and_rejects_a_bad_one() {
    use agentmark::binder::core::{Param, Sig, Ty};
    let sig = Sig {
        params: vec![Param { name: "data".into(), ty: Ty::Bytes }],
        ret: Some(Ty::Bytes),
    };
    let checker = CargoChecker::new(true);
    let req = |body: &'static str| CheckRequest { crate_name: "sha2", version: "0.10.9", name: "sha256", sig: &sig, body };
    checker
        .check(&req("use sha2::{Digest, Sha256};\nOk(Sha256::digest(&data).to_vec())"))
        .unwrap();
    let e = checker.check(&req("Ok(data.len())")).unwrap_err();
    assert!(e.contains("mismatched types") || e.contains("E0308"), "{e}");
    let e = checker.check(&req("Ok(sha2::Sha257::digest(&data).to_vec())")).unwrap_err();
    assert!(e.contains("Sha257"), "{e}");
}

/// The direct tier writes Rust by rules; compile every direct chrono body once so a wrong rule
/// shows up here and not in a user's build.
#[test]
fn every_direct_chrono_body_compiles() {
    let (ci, b) = tiered("chrono", vec![]);
    let reqs: Vec<CheckRequest> = b
        .items
        .iter()
        .filter(|i| i.tier == Tier::Direct)
        .map(|i| CheckRequest { crate_name: "chrono", version: &ci.version, name: &i.name, sig: &i.sig, body: &i.body })
        .collect();
    assert!(reqs.len() > 50);
    CargoChecker::new(true).with_features(false, &[]).check_all(&reqs).unwrap();
}

#[test]
fn fetch_builds_rustdoc_json_when_nightly_is_installed() {
    if !agentmark::binder::rust::fetch::nightly_available() {
        eprintln!("skipped: no nightly toolchain");
        return;
    }
    let mut spec = FetchSpec::new("sha2", Some("0.10.9"));
    spec.offline = true;
    let p = agentmark::binder::rust::fetch_rustdoc_json_with(&spec).unwrap();
    let ci = read_crate(&std::fs::read_to_string(p).unwrap()).unwrap();
    assert_eq!((ci.name.as_str(), ci.version.as_str()), ("sha2", "0.10.9"));
}

// ------------------------------------------------------------ review fixes

#[test]
fn sha2_hashing_comes_from_the_reexported_digest_trait() {
    let (ci, b) = tiered("sha2", vec![]);
    let it = ci.items.iter().find(|i| i.name == "sha256_digest").expect("sha256_digest");
    assert_eq!(it.path, "<sha2::Sha256 as sha2::Digest>::digest");
    assert_eq!(it.summary, "Compute hash of `data`.");
    let bound = b.items.iter().find(|i| i.name == "sha256_digest").expect("bound");
    assert_eq!(bound.tier, Tier::Assisted);
    assert_eq!(bound.sig.heading(&bound.name), "sha256_digest(data: bytes) -> bytes");
    // Receiver-less methods only; `self` methods are one aggregated line.
    assert!(b.not_bound.iter().any(|(n, r)| n == "sha256_new" && r.contains("no AgentMark type")));
    assert!(!b.items.iter().any(|i| i.name.contains("finalize") || i.name.ends_with("_update")));
    // ... each one its own entry, never a list (SPEC 15.2)
    assert!(!b.not_bound.iter().any(|(n, _)| n.contains("methods_with_self")));
    for m in ["update", "chain_update", "finalize", "finalize_into", "finalize_reset", "finalize_into_reset", "reset"] {
        let name = format!("sha256_{m}");
        let (_, r) = b.not_bound.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("{name} listed"));
        assert!(r.contains("takes `self`") && r.contains(&format!("Digest::{m} on sha2::Sha256")), "{r}");
    }
    // Without digest's rustdoc the re-export stays a listed non-callable.
    let j = std::fs::read_to_string(fixture("sha2")).unwrap();
    assert_eq!(agentmark::binder::rust::foreign_crates(&j).unwrap(), vec!["digest".to_string()]);
    assert!(read_crate(&j).unwrap().items.iter().all(|i| i.name != "sha256_digest"));
}

#[test]
fn the_canned_sha256_digest_body_really_compiles() {
    let (ci, b) = tiered("sha2", vec![]);
    let it = b.items.iter().find(|i| i.name == "sha256_digest").unwrap();
    CargoChecker::new(true)
        .check(&CheckRequest { crate_name: "sha2", version: &ci.version, name: &it.name, sig: &it.sig, body: &it.body })
        .unwrap();
    let dir = scratch("sha-file");
    let out = bind("sha2", &opts(&dir, "sha2", vec![])).unwrap();
    assert_eq!(out.counts.assisted, 1);
    let f = parse(&out.path);
    let item = binding(&f).items.iter().find(|i| i.name.name == "sha256_digest").unwrap();
    assert_eq!(item.doc.as_deref(), Some("Compute hash of `data`."));
}

#[test]
fn reexport_names_carry_the_source_path() {
    let (ci, _) = tiered("sha2", vec![]);
    let names: Vec<&str> = ci.extra_not_bound.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"reexport_digest") && names.contains(&"reexport_digest_digest"), "{names:?}");
    let r = &ci.extra_not_bound.iter().find(|e| e.name == "reexport_digest_digest").unwrap().reason;
    assert!(!r.contains("bind that crate to use it"), "{r}");
}

fn tiny_rustdoc() -> String {
    let f = |id: u64, name: &str, ty: serde_json::Value| {
        serde_json::json!({"id": id, "name": name, "visibility": "public", "docs": "", "inner": {"function": {
            "sig": {"inputs": [["p", ty]], "output": null},
            "generics": {"params": [], "where_predicates": []},
            "header": {"is_unsafe": false, "is_async": false}}}})
    };
    let path = |id: u64, p: &str| serde_json::json!({"resolved_path": {"path": p, "id": id, "args": null}});
    serde_json::json!({
        "format_version": 61, "crate_version": "1.0.0", "root": 0,
        "index": {
            "0": {"id": 0, "name": "demo", "visibility": "public", "docs": "", "inner": {"module": {"items": [1, 2, 3]}}},
            "1": f(1, "read", serde_json::json!({"borrowed_ref": {"lifetime": null, "is_mutable": false, "type": path(10, "Path")}})),
            "2": f(2, "connect", path(11, "TcpStream")),
            "3": f(3, "lookup", path(12, "SocketAddr")),
        },
        "paths": {
            "10": {"path": ["std", "path", "Path"]},
            "11": {"path": ["std", "net", "tcp", "TcpStream"]},
            "12": {"path": ["core", "net", "socket_addr", "SocketAddr"]},
        }
    })
    .to_string()
}

#[test]
fn effect_heuristic_sees_std_paths_in_signatures() {
    use agentmark::binder::core::{propose_effect, ProposedEffect};
    let ci = read_crate(&tiny_rustdoc()).unwrap();
    let sig = |n: &str| ci.items.iter().find(|i| i.name == n).unwrap().source_sig.clone();
    assert!(sig("read").contains("&std::path::Path"), "{}", sig("read"));
    let eff = |names: &[&str]| {
        let sigs: Vec<String> = names.iter().map(|n| sig(n)).collect();
        propose_effect(&ci.hints, sigs.iter().map(String::as_str))
    };
    assert_eq!(eff(&["read"]), ProposedEffect::Local);
    assert_eq!(eff(&["connect"]), ProposedEffect::Network);
    assert_eq!(eff(&["lookup"]), ProposedEffect::Network);
}

#[test]
fn reader_items_equal_public_functions_in_the_fixture_index() {
    // Independent count from the raw JSON: every function in the index is either a module-level
    // public function or a method of an impl. Documented exclusions (Q72): non-public inherent
    // methods; blanket/synthetic/negative impls and std-plumbing trait impls (already absent
    // from the trimmed fixtures, asserted below).
    for (c, _) in CRATES {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(fixture(c)).unwrap()).unwrap();
        let index = v["index"].as_object().unwrap();
        let kind = |i: &serde_json::Value| i["inner"].as_object().and_then(|o| o.keys().next().cloned()).unwrap_or_default();
        let mut method_of: HashSet<u64> = HashSet::new();
        let mut counted = 0;
        for i in index.values().filter(|i| kind(i) == "impl") {
            let imp = &i["inner"]["impl"];
            assert!(imp["is_synthetic"] != true && imp["blanket_impl"].is_null() && imp["is_negative"] != true, "{c}");
            let inherent = imp["trait"].is_null();
            for m in imp["items"].as_array().unwrap() {
                let m = m.as_u64().unwrap();
                let Some(f) = index.get(&m.to_string()) else { continue };
                if kind(f) != "function" {
                    continue;
                }
                method_of.insert(m);
                if !inherent || f["visibility"] == "public" {
                    counted += 1;
                }
            }
        }
        for (id, f) in index {
            if kind(f) == "function" && !method_of.contains(&id.parse().unwrap()) {
                assert_eq!(f["visibility"], "public", "{c}");
                counted += 1;
            }
        }
        let ci = read_crate(&std::fs::read_to_string(fixture(c)).unwrap()).unwrap();
        assert_eq!(ci.items.len(), counted, "{c}");
    }
}

#[test]
fn a_body_that_escapes_its_function_is_refused_before_cargo_runs() {
    use agentmark::binder::core::{Sig, Ty};
    assert!(agentmark::binder::rust::checker::balanced("Ok(\"}\".to_string()) // }").is_ok());
    assert!(agentmark::binder::rust::checker::balanced("let c = '}'; let r = r#\"}\"#; Ok(1)").is_ok());
    assert!(agentmark::binder::rust::checker::balanced("Ok(1) }").is_err());
    let sig = Sig { params: vec![], ret: Some(Ty::Int) };
    let e = CargoChecker::new(true)
        .check(&CheckRequest {
            crate_name: "sha2",
            version: "0.10.9",
            name: "x",
            sig: &sig,
            body: "Ok(1) } pub fn y() { ",
        })
        .unwrap_err();
    assert!(e.contains("balanced"), "{e}");
}

#[test]
fn a_binding_generated_for_another_crate_is_stale() {
    let dir = scratch("wrong-crate");
    let p = dir.join("sha2.agent.md");
    std::fs::write(&p, "> generated by agentmark bind \u{2014} chrono 0.10.9 \u{2014} 2026-10-14\n# Binding: sha2\n- crate: sha2\n- effect: pure\n").unwrap();
    assert_eq!(
        freshness(&p, "sha2", "0.10.9").unwrap(),
        Freshness::Stale { bound_version: "chrono 0.10.9".into() }
    );
}

#[test]
fn skipped_assisted_items_name_the_library_path() {
    let (_, b) = tiered("chrono", vec!["date_time_to_utc"]);
    let (_, r) = b.not_bound.iter().find(|(n, _)| n == "date_time_to_utc").unwrap();
    assert!(r.contains("failed type-check") && r.ends_with("(chrono::DateTime::to_utc)"), "{r}");
}

// ------------------------------------------------------------ foreign traits, follow-up

/// A crate `demo` with `pub use ext::Size` and `pub type W = ext::Wrapper`, and the JSON of
/// `ext`, whose trait `Size` has a receiver-less `fn size() -> i64` (1:1 types).
fn demo_and_ext() -> (String, String) {
    let item = |id: u64, name: &str, inner: serde_json::Value| {
        serde_json::json!({"id": id, "name": name, "visibility": "public", "docs": "Size of the thing.", "inner": inner})
    };
    let demo = serde_json::json!({
        "format_version": 61, "crate_version": "1.0.0", "root": 0,
        "index": {
            "0": item(0, "demo", serde_json::json!({"module": {"items": [1, 2]}})),
            "1": item(1, "Size", serde_json::json!({"use": {"source": "ext::Size", "name": "Size", "id": null, "is_glob": false}})),
            "2": item(2, "W", serde_json::json!({"type_alias": {"type": {"resolved_path": {"path": "ext::Wrapper", "id": 20, "args": null}}}})),
        },
        "paths": {"20": {"path": ["ext", "Wrapper"]}}
    });
    let ext = serde_json::json!({
        "format_version": 61, "crate_version": "2.0.0", "root": 0,
        "index": {
            "0": item(0, "ext", serde_json::json!({"module": {"items": []}})),
            "4": item(4, "Size", serde_json::json!({"trait": {"items": [5]}})),
            "5": item(5, "size", serde_json::json!({"function": {
                "sig": {"inputs": [], "output": {"primitive": "i64"}},
                "generics": {"params": [], "where_predicates": []},
                "header": {"is_unsafe": false, "is_async": false}}})),
        },
        "paths": {}
    });
    (demo.to_string(), ext.to_string())
}

#[test]
fn a_one_to_one_foreign_trait_method_is_assisted_never_direct() {
    use agentmark::binder::core::Shape;
    let (demo, ext) = demo_and_ext();
    let ci = agentmark::binder::rust::read_crate_with(&demo, &[ext]).unwrap();
    let it = ci.items.iter().find(|i| i.name == "w_size").expect("w_size");
    assert_eq!(it.path, "<demo::W as demo::Size>::size");
    assert!(matches!(&it.shape, Shape::Assisted { why, .. } if why.contains("another crate")), "{:?}", it.shape);
}

fn demo_opts(tag: &str) -> (PathBuf, BindOptions) {
    let (demo, _) = demo_and_ext();
    let dir = scratch(tag);
    let json = dir.join("demo-in.json");
    std::fs::write(&json, demo).unwrap();
    let mut o = opts(&dir.join("out"), "sha2", vec![]);
    o.rustdoc_json = Some(json);
    o.foreign_rustdoc = vec![];
    (dir, o)
}

#[test]
fn a_failing_dependency_rustdoc_build_is_reported_in_the_reexport_line() {
    let (_dir, mut o) = demo_opts("foreign-fail");
    o.foreign_fetch = Some(Arc::new(|c| Err(format!("cargo exploded for {c}"))));
    let out = bind("demo", &o).unwrap();
    let r = entries(&std::fs::read_to_string(&out.path).unwrap()).remove("reexport_ext_size").expect("re-export listed");
    // long reasons continue on `  > ` lines (Q110): the whole cause survives
    assert!(r.contains("re-export of `ext::Size`") && r.contains("from another crate"), "{r}");
    assert!(r.contains("cargo exploded for ext"), "{r}");
    let f = parse(&out.path);
    assert!(binding(&f).not_bound.iter().all(|n| n.item != "w_size"));
}

#[test]
fn a_dependency_json_directory_replaces_the_nightly_build() {
    let (dir, o) = demo_opts("foreign-dir");
    let (_, ext) = demo_and_ext();
    let jd = dir.join("json");
    std::fs::create_dir_all(&jd).unwrap();
    std::fs::write(jd.join("ext.json"), ext).unwrap();
    std::env::set_var("AGENTMARK_RUSTDOC_JSON_DIR", &jd);
    let out = bind("demo", &o);
    std::env::remove_var("AGENTMARK_RUSTDOC_JSON_DIR");
    let out = out.unwrap();
    let f = parse(&out.path);
    // No canned model body for it, but the item exists and was tiered (skipped by the model).
    let n = binding(&f).not_bound.iter().find(|n| n.item == "w_size").expect("w_size tiered");
    assert!(n.reason.as_deref().unwrap().contains("mock model has no response for"));
    // Without the directory the missing JSON is named in the re-export line.
    let (_d2, o2) = demo_opts("foreign-none");
    let r = entries(&std::fs::read_to_string(bind("demo", &o2).unwrap().path).unwrap())
        .remove("reexport_ext_size")
        .unwrap();
    assert!(r.contains("AGENTMARK_RUSTDOC_JSON_DIR"), "{r}");
}

#[test]
fn freshly_bound_files_keep_every_line_within_the_limit_and_check_clean() {
    // SPEC 18.3: the limits apply to generated files too (Q110).
    for (c, _) in CRATES {
        let dir = scratch(&format!("limit-{c}"));
        let out = bind(c, &opts(&dir, c, vec![])).unwrap();
        let text = std::fs::read_to_string(&out.path).unwrap();
        for (i, l) in text.lines().enumerate() {
            assert!(l.chars().count() <= 120, "{c}:{}: {} chars: {l}", i + 1, l.chars().count());
        }
        // still names the library path of what was not bound
        assert!(text.contains("## Not bound"), "{c}");
        let project = agentmark::project::Project::load(dir.to_str().unwrap());
        match agentmark::sema::check(&project) {
            Ok(_) => {}
            Err(d) => {
                let errors: Vec<_> =
                    d.into_iter().filter(|x| x.severity == agentmark::diag::Severity::Error).collect();
                assert!(errors.is_empty(), "{c}:\n{}", project.render(&errors));
            }
        }
    }
}

/// `## Not bound` entries of a binding file with their `  > ` continuation lines joined.
fn entries(text: &str) -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    let mut pieces: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut cur: Option<String> = None;
    let mut in_section = false;
    for l in text.lines() {
        if l.starts_with('#') {
            in_section = l.trim_end() == "## Not bound";
        } else if in_section {
            if let Some(rest) = l.strip_prefix("- ") {
                let (name, reason) = rest.split_once(':').unwrap_or((rest, ""));
                cur = Some(name.to_string());
                m.insert(name.to_string(), reason.trim().to_string());
            } else if let (Some(rest), Some(c)) = (l.strip_prefix("  > "), &cur) {
                pieces.entry(c.clone()).or_default().push(rest.to_string());
            }
        }
    }
    for (name, rest) in pieces {
        let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
        let joined = agentmark::syntax::parser::join_wrapped(&m[&name], &rest);
        m.insert(name, joined);
    }
    m
}

#[test]
fn long_not_bound_causes_continue_on_comment_lines_and_survive_in_full() {
    let dir = scratch("limit-full");
    let out = bind("sha2", &opts(&dir, "sha2", vec![])).unwrap();
    let text = std::fs::read_to_string(&out.path).unwrap();
    let e = entries(&text);
    let r = &e["sha224_output_size"];
    assert!(r.starts_with("assisted tier: mock model has no response for sha224_output_size"), "{r}");
    assert!(!r.contains("prompt") && !r.contains("bind <"), "no prompt fragments: {r}");
    assert!(r.ends_with("(<sha2::Sha224 as sha2::Digest>::output_size)"), "{r}");
    // a long type-check error is kept whole
    let long = "error[E0308]: mismatched types: expected `Vec<u8>`, found `GenericArray<u8, <CoreWrapper<CtVariableCoreWrapper<Sha256VarCore, U32, OidSha256>> as OutputSizeUser>::OutputSize>` at line 3";
    struct Long(&'static str);
    impl TypeChecker for Long {
        fn check(&self, req: &CheckRequest<'_>) -> Result<(), String> {
            if req.name == "sha256_digest" { Err(self.0.to_string()) } else { Ok(()) }
        }
    }
    let dir = scratch("limit-typeerr");
    let mut o = opts(&dir, "sha2", vec![]);
    o.checker = Some(Box::new(Long(Box::leak(long.to_string().into_boxed_str()))));
    let out = bind("sha2", &o).unwrap();
    let text = std::fs::read_to_string(&out.path).unwrap();
    assert!(text.lines().all(|l| l.chars().count() <= 120));
    let r = &entries(&text)["sha256_digest"];
    assert!(r.contains(long), "{r}");
    // and the file still parses
    let f = parse(&out.path);
    // the parser joins the continuation lines: the reason is the whole text
    let n = binding(&f).not_bound.iter().find(|n| n.item == "sha256_digest").unwrap();
    assert_eq!(n.reason.as_deref().unwrap(), r.as_str());
    assert!(r.ends_with("(<sha2::Sha256 as sha2::Digest>::digest)"), "{r}");
    // and so does `agentmark doc <crate>`
    let sheet = doc_cheatsheet(&out.path).unwrap();
    assert!(sheet.contains(long) && sheet.contains("::Digest>::digest)"), "{sheet}");
}

#[test]
fn generated_files_round_trip_through_fmt_byte_for_byte() {
    for (c, _) in CRATES {
        let dir = scratch(&format!("fmt-{c}"));
        let out = bind(c, &opts(&dir, c, vec![])).unwrap();
        let text = std::fs::read_to_string(&out.path).unwrap();
        assert!(text.contains("\n  > "), "{c}: has wrapped entries");
        let f = out.path.display().to_string();
        assert_eq!(agentmark::fmt::format_source(&text, &f).unwrap(), text, "{c}");
    }
}

#[test]
fn alias_lines_say_which_crate_and_no_generic_noise() {
    let dir = scratch("alias");
    let out = bind("sha2", &opts(&dir, "sha2", vec![])).unwrap();
    let e = entries(&std::fs::read_to_string(&out.path).unwrap());
    let (n, r) = e.iter().find(|(_, r)| r.contains("is a type alias for")).expect("alias entry");
    assert!(r.contains(" from crate digest; its methods live there"), "{n}: {r}");
    assert!(r.contains("<…>") && !r.contains("U32"), "{r}");
}

#[test]
fn indented_comment_lines_continue_a_not_bound_reason() {
    let src = "# Binding: lib\n- crate: lib\n- effect: pure\n\n## Not bound\n- a: first part\n  > second part\n  > third\n- b: alone\n> plain note\n- c:\n  > only continuation\n";
    let f = agentmark::syntax::parser::parse_file("lib.agent.md", src).unwrap();
    let nb = &binding(&f).not_bound;
    let r = |i: usize| nb[i].reason.as_deref();
    assert_eq!((r(0), r(1), r(2)), (Some("first part second part third"), Some("alone"), Some("only continuation")));
    // fmt joins what fits on one line and keeps the plain note
    let out = agentmark::fmt::format_source(src, "lib.agent.md").unwrap();
    assert!(out.contains("- a: first part second part third\n") && out.contains("> plain note"), "{out}");
}

#[test]
fn bind_stays_on_the_header_version_force_regenerates_it_and_latest_moves() {
    let dir = scratch("pinned");
    let first = bind("sha2", &opts(&dir, "sha2", vec![])).unwrap();
    let old = std::fs::read_to_string(&first.path).unwrap().replace("sha2 0.10.9", "sha2 0.10.8").replace("- effect: unverified", "- effect: pure");
    std::fs::write(&first.path, &old).unwrap();
    // the header is the lock: the newer release the tooling knows about (0.10.9) is not taken
    let kept = bind("sha2", &opts(&dir, "sha2", vec![])).unwrap();
    assert!(!kept.generated && kept.version == "0.10.8", "{kept:?}");
    assert_eq!(std::fs::read_to_string(&first.path).unwrap(), old);
    // `--latest` goes to what the tooling resolves (the JSON's 0.10.9) and re-marks unverified
    let mut l = opts(&dir, "sha2", vec![]);
    l.latest = true;
    let moved = bind("sha2", &l).unwrap();
    assert_eq!(moved.reason, Some(Regenerated::Stale { from: "0.10.8".into() }));
    assert_eq!(moved.effect, "unverified");
    // now the header says 0.10.9: plain bind is a cache hit, --force regenerates the SAME version
    let hit = bind("sha2", &opts(&dir, "sha2", vec![])).unwrap();
    assert!(!hit.generated);
    let mut f = opts(&dir, "sha2", vec![]);
    f.force = true;
    let forced = bind("sha2", &f).unwrap();
    assert_eq!((forced.generated, forced.reason, forced.version.as_str()), (true, Some(Regenerated::Forced), "0.10.9"));
}

/// Wrap `reason` as a "Not bound" entry, parse the file, return the lines and the parsed reason.
fn wrap_and_parse(reason: &str) -> (Vec<String>, String) {
    let entry = agentmark::binder::core::writer::not_bound_entry("item", reason);
    let text = format!("> generated by agentmark bind \u{2014} x 1.0.0 \u{2014} 2026-10-14\n# Binding: x\n- crate: x\n- effect: unverified\n\n## Not bound\n{entry}");
    let f = agentmark::syntax::parser::parse_file("x.agent.md", &text).unwrap_or_else(|d| panic!("{}", agentmark::diag::render_all(&d, &text)));
    let reason = binding(&f).not_bound[0].reason.clone().unwrap_or_default();
    (entry.lines().map(String::from).collect(), reason)
}

#[test]
fn a_word_longer_than_a_line_is_split_and_joined_back_without_a_space() {
    let path: String = (0..145).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
    let reason = format!("see {path} for details");
    let (lines, parsed) = wrap_and_parse(&reason);
    assert!(lines.len() >= 2 && lines.iter().all(|l| l.chars().count() <= 120), "{lines:?}");
    assert!(lines.iter().any(|l| l.ends_with('\\')), "the split is marked: {lines:?}");
    assert_eq!(parsed, reason, "no space invented inside the word");
}

#[test]
fn backslashes_survive_the_wrapping_also_at_line_ends() {
    // a long path with many backslashes (Windows style), and words that end in a backslash exactly where a line wraps
    let path = format!("C:{}", "\\folder".repeat(30));
    for reason in [format!("path {path} end"), format!("{} {path}", "word\\ ".repeat(40)), "a\\ ".repeat(80), format!("x{}", "\\".repeat(150))] {
        let (lines, parsed) = wrap_and_parse(&reason);
        assert!(lines.iter().all(|l| l.chars().count() <= 120), "{lines:?}");
        assert_eq!(parsed, reason.split_whitespace().collect::<Vec<_>>().join(" "));
    }
}

