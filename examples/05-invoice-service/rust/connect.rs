//! Connect the tools to the real world.
//!
//! This file is yours: it lives in your project as `rust/connect.rs` (beside agentmark.toml);
//! `agentmark build` creates it once, when it is missing, and never changes it again. Every
//! build copies it to `src/connect.rs` of the generated project, so edit THIS one. Implement the traits of `tools.rs` for your systems and return them from `connect()`.
//! A real run (without --mock) uses what `connect()` returns. If tools.toml gains a tool, add it here.

use agentmark_rt::RtError;
use async_trait::async_trait;
use serde_json::Value;

use crate::tools::{Connected, Crm, Email, Http};

/// A tool nobody has connected yet: every call says what to implement.
struct NotConnected;

#[async_trait]
impl Crm for NotConnected {
    async fn list(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Crm::list in rust/connect.rs".into()))
    }
    async fn get(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Crm::get in rust/connect.rs".into()))
    }
    async fn update(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Crm::update in rust/connect.rs".into()))
    }
}

#[async_trait]
impl Email for NotConnected {
    async fn send(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Email::send in rust/connect.rs".into()))
    }
}

#[async_trait]
impl Http for NotConnected {
    async fn get(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Http::get in rust/connect.rs".into()))
    }
    async fn post(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Http::post in rust/connect.rs".into()))
    }
}

/// The tools of a real run.
pub fn connect() -> Connected {
    Connected {
        crm: Box::new(NotConnected),
        email: Box::new(NotConnected),
        http: Box::new(NotConnected),
    }
}
