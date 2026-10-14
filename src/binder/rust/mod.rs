//! Rust target of the binder: rustdoc JSON in, [`CrateItems`](crate::binder::core::CrateItems)
//! out, plus the `cargo check` type checker for assisted bodies.

pub mod checker;
pub mod fetch;
pub mod merge;
pub mod rustdoc;
pub mod types;

pub use checker::{rust_fn_header, CargoChecker};
pub use fetch::{ScratchGuard, fetch_dep_rustdoc_json, fetch_rustdoc_json, fetch_rustdoc_json_with, resolve_version, FetchSpec};
pub use rustdoc::{foreign_crates, reexported_crates, read_crate, read_crate_with, SUPPORTED_FORMAT};
