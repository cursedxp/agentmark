//! AgentMark compiler library. The binary (`main.rs`) and the language server both build on this.

pub mod cli;
pub mod diag;
pub mod fmt;
pub mod lint;
pub mod syntax;
pub mod binder;
pub mod manifest;
pub mod project;
pub mod sema;
pub mod ir;
pub mod lower;
