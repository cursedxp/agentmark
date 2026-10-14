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

use agentmark_rt::ModelClient;
use lsp_server::Connection;

pub use actions::{apply, EXPLAIN_PROMPT, GUARD_PROMPT};

/// Serve one client on `conn` until it sends `exit`. `model` backs the optional model actions
/// (they stay off unless the project's `agentmark.toml` says `[lsp] model_actions = true`).
pub fn serve(conn: Connection, model: Option<Arc<dyn ModelClient>>) -> Result<(), Box<dyn Error + Sync + Send>> {
    server::Server::new(conn, model).run()
}

/// Serve one client on `conn` with the model of a real session: for each project, the one the binder
/// uses (`AGENTMARK_MODEL`, else the `[models]` entry `AGENTMARK_ENV` names in that project's
/// `agentmark.toml`, else Claude; Q240/Q256). Resolved when a model action runs, so edits to
/// `agentmark.toml` count at once.
pub fn serve_from_env(conn: Connection) -> Result<(), Box<dyn Error + Sync + Send>> {
    server::Server::from_env(conn).run()
}
