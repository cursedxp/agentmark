//! `agentmark-lsp`: a language server for AgentMark over stdio, built on the `agentmark`
//! compiler library (SPEC 20.3). See `docs/internals.md` for the design.

pub mod actions;
pub mod complete;
pub mod diagnostics;
pub mod index;
pub mod nav;
pub mod server;
pub mod text;
pub mod uri;
pub mod workspace;

use std::error::Error;
use std::sync::Arc;

use agentmark_rt::{AnthropicModel, MockModel, ModelClient};
use lsp_server::Connection;

pub use actions::{apply, EXPLAIN_PROMPT, GUARD_PROMPT};

/// Serve one client on `conn` until it sends `exit`. `model` backs the optional model actions
/// (they stay off unless the project's `agentmark.toml` says `[lsp] model_actions = true`).
pub fn serve(conn: Connection, model: Option<Arc<dyn ModelClient>>) -> Result<(), Box<dyn Error + Sync + Send>> {
    server::Server::new(conn, model).run()
}

/// The model of a real session: `AGENTMARK_MODEL=mock:<model.json>` (like generated programs), else
/// Anthropic when `ANTHROPIC_API_KEY` is set, else none.
pub fn model_from_env() -> Option<Arc<dyn ModelClient>> {
    if let Some(path) = std::env::var("AGENTMARK_MODEL").ok().and_then(|v| v.strip_prefix("mock:").map(String::from)) {
        return MockModel::from_file(path).ok().map(|m| Arc::new(m) as Arc<dyn ModelClient>);
    }
    AnthropicModel::from_env("claude-sonnet-4-5").ok().map(|m| Arc::new(m) as Arc<dyn ModelClient>)
}
