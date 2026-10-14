//! Layer 3 -> Layer 2 lowering (DESIGN 4.1). See [`agent_lower`].

pub mod agent_lower;

pub use agent_lower::{agent_lower, is_synthetic, sanitize, walk_expr_mut, Lowered};
