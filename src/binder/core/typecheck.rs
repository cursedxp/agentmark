//! The gate between the model and the binding file: a body is only accepted when the target's
//! compiler agrees it fits the declared signature (SPEC 15.2, assisted tier).

use super::model::Sig;

pub struct CheckRequest<'a> {
    pub crate_name: &'a str,
    pub version: &'a str,
    /// Item name (the binding heading).
    pub name: &'a str,
    pub sig: &'a Sig,
    pub body: &'a str,
}

/// Type-checks one generated body. Behind a trait so tests can use both the real
/// `cargo check` and a fake. `Err` carries the compiler's complaint, kept as the reason an
/// item landed under `## Not bound`.
pub trait TypeChecker: Send + Sync {
    fn check(&self, req: &CheckRequest<'_>) -> Result<(), String>;
}
