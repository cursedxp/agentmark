//! Helpers shared by the semantic-check tests: check a source text against the example manifest.
#![allow(dead_code)]

use agentmark::diag::Severity;
use agentmark::project::Project;
use agentmark::sema::{self, Checked};

pub const TOOLS: &str = include_str!("../../examples/tools.toml");

pub fn project(src: &str) -> Project {
    Project::from_str("t.agent.md", src, TOOLS)
}

/// Rendered errors of `src` (warnings left out). Panics if the source checks.
pub fn errors(src: &str) -> String {
    let p = project(src);
    match sema::check(&p) {
        Ok(_) => panic!("expected errors in:\n{src}"),
        Err(d) => {
            let e: Vec<_> = d
                .into_iter()
                .filter(|x| x.severity == Severity::Error)
                .collect();
            assert!(!e.is_empty(), "only warnings in:\n{src}");
            p.render(&e)
        }
    }
}

/// The checked project; panics with the rendered errors if there are any.
pub fn ok(src: &str) -> Checked {
    let p = project(src);
    match sema::check(&p) {
        Ok(c) => c,
        Err(d) => panic!("expected no errors, got:\n{}\nin:\n{src}", p.render(&d)),
    }
}

/// Rendered warnings of a source that checks.
pub fn warnings(src: &str) -> String {
    let p = project(src);
    let c = sema::check(&p).unwrap_or_else(|d| panic!("errors:\n{}", p.render(&d)));
    p.render(&c.warnings)
}

/// The first rendered error block, for compact assertions.
pub fn first_error(src: &str) -> String {
    errors(src).split("\n\n").next().unwrap().to_string()
}
