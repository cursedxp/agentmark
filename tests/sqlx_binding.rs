//! The real `sqlx` 0.8 binding of example 08 (SPEC 22.4, Q411, Q412): a facade crate whose items are
//! re-exports, a generic pool alias, generic query handles with lifetimes. The rustdoc JSON is a trimmed
//! fixture (`tests/binder/regen.sh`), the model answers are canned (`tests/binder/sqlx-model.json`);
//! the type check is the real `cargo check --offline` against sqlx, so these tests need sqlx 0.8.6 in
//! the local cargo cache (example 08 needs it to build too): see `sqlx_in_the_cache`.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentmark::binder::core::{CheckRequest, Param, Sig, TypeChecker};
use agentmark::binder::rust::CargoChecker;
use agentmark::binder::{bind, BindOptions};
use agentmark_rt::MockModel;
use chrono::NaiveDate;

use support::*;

fn fixture(c: &str) -> PathBuf {
    Path::new(ROOT).join("tests/binder").join(format!("{c}.json"))
}

/// sqlx is a hard requirement of example 08, so a missing crate fails the test with the fix.
fn sqlx_in_the_cache() {
    let c = CargoChecker::new(true).with_features(true, &["sqlite", "runtime-tokio"]);
    let sig = Sig { params: Vec::<Param>::new(), ret: None };
    if let Err(e) = c.check(&CheckRequest { crate_name: "sqlx", version: "0.8.6", name: "probe", sig: &sig, body: "Ok(())" }) {
        panic!(
            "sqlx 0.8.6 is not in the offline cargo cache ({e}).\nFetch it once with network access: \
             `cd examples/08-notes-api && cargo fetch --manifest-path <a scratch crate depending on sqlx 0.8 with features sqlite, runtime-tokio>`, \
             or build example 08 once online (`agentmark build examples/08-notes-api -o /tmp/n8`)."
        );
    }
}

fn generate(dir: &Path) -> String {
    let model = Arc::new(MockModel::from_file(fixture("sqlx-model")).unwrap());
    let mut o = BindOptions::new(dir, model, NaiveDate::from_ymd_opt(2026, 10, 14).unwrap());
    o.rustdoc_json = Some(fixture("sqlx"));
    o.foreign_rustdoc = vec![fixture("sqlx_core"), fixture("sqlx_sqlite")];
    o.crate_features.insert("sqlx".into(), (true, vec!["sqlite".into(), "runtime-tokio".into()]));
    o.offline = true;
    let out = bind("sqlx", &o).unwrap();
    assert_eq!((out.counts.assisted, out.generated), (8, true), "{:?}", out.counts);
    std::fs::read_to_string(out.path).unwrap()
}

/// The file checked in with example 08 is what the binder makes of sqlx, with the effect approved.
#[test]
fn example_08_binding_is_what_the_binder_makes_of_sqlx() {
    sqlx_in_the_cache();
    let d = temp("sqlx-bind");
    let made = generate(&d);
    let kept = std::fs::read_to_string(format!("{ROOT}/examples/08-notes-api/bindings/sqlx.agent.md")).unwrap();
    assert_eq!(kept, made.replace("- effect: unverified", "- effect: local"));
}

#[test]
fn the_items_example_08_calls_are_bound_as_the_example_writes_them() {
    sqlx_in_the_cache();
    let d = temp("sqlx-heads");
    let made = generate(&d);
    for want in [
        "## sqlite_pool_connect(url: text) -> handle of sqlx.SqlitePool",
        "## query(sql: text) -> handle of sqlx.query.Query",
        "## query_scalar(sql: text) -> handle of sqlx.query.QueryScalar",
        "## bind(self: consume handle of sqlx.query.Query, value: text) -> handle of sqlx.query.Query",
        "## bind(self: consume handle of sqlx.query.QueryScalar, value: text) -> handle of sqlx.query.QueryScalar",
        "## execute(self: consume handle of sqlx.query.Query, executor: handle of sqlx.SqlitePool) -> handle of sqlx.sqlite.SqliteQueryResult",
        "## fetch_one(self: consume handle of sqlx.query.QueryScalar, executor: handle of sqlx.SqlitePool) -> text",
        "## fetch_optional(self: consume handle of sqlx.query.QueryScalar, executor: handle of sqlx.SqlitePool) -> maybe text",
        "## last_insert_rowid(self: handle of sqlx.sqlite.SqliteQueryResult) -> int",
        "## rows_affected(self: handle of sqlx.sqlite.SqliteQueryResult) -> int",
    ] {
        assert!(made.lines().any(|l| l == want), "missing {want}");
    }
    // the instantiation is Sqlite, never the type-erased Any driver
    assert!(made.contains("sqlx::query::Query<'static, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'static>>"));
    assert!(made.contains("sqlx::query::QueryScalar<'static, sqlx::Sqlite, String, sqlx::sqlite::SqliteArguments<'static>>"));
    // `u64` rows_affected converts with a checked conversion
    assert!(made.contains("i64::try_from(self_.rows_affected())?"));
}
