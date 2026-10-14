//! Layer 3 -> Layer 2 lowering (DESIGN 4.1). See [`agent_lower`].

pub mod agent_lower;

pub use agent_lower::{agent_lower, guard_text, is_synthetic, walk_expr_mut, Lowered};
