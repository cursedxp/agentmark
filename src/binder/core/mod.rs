//! Target-agnostic half of the binder (SPEC 15.2): the item model, the tier pipeline, the
//! binding-file writer, staleness, and effect proposals. Nothing in here knows about Rust,
//! rustdoc or cargo; a target (`binder::rust`) fills in [`CrateItems`] and a [`TypeChecker`].

pub mod effects;
pub mod model;
pub mod ondemand;
pub mod pipeline;
pub mod typecheck;
pub mod writer;

pub use effects::{propose_effect, EffectHints, ProposedEffect};
pub use model::{CrateItems, Item, NotBoundEntry, Param, Shape, Sig, Ty};
pub use pipeline::{run_tiers, BoundCrate, BoundItem, Counts, Tier};
pub use typecheck::{CheckRequest, TypeChecker};
pub use writer::{add_on_demand, freshness, parse_header, render_binding, render_item, Freshness, Header};
