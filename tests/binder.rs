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
    o.checker = Box::new(FakeChecker { reject });
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
    let ci = read_crate(&json).unwrap();
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
    assert!(why.contains("model gave no body"), "{why}");
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
        freshness(&first.path, "0.10.9").unwrap(),
        Freshness::Stale { bound_version: "0.9.0".into() }
    );
    assert_eq!(freshness(&first.path, "0.9.0").unwrap(), Freshness::Fresh);
    assert_eq!(freshness(&dir.join("nope.agent.md"), "1").unwrap(), Freshness::Missing);

    let again = bind("sha2", &opts(&dir, "sha2", vec![])).unwrap();
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
    CargoChecker::new(true).check_all(&reqs).unwrap();
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
