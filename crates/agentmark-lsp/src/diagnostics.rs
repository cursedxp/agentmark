//! Compiler and lint findings as LSP diagnostics, with the SPEC 10 wording.

use std::collections::BTreeMap;

use agentmark::diag::{Diagnostic, Severity};
use lsp_types::{DiagnosticSeverity, NumberOrString, Range};
use serde_json::json;

use crate::text::span_range;
use crate::workspace::Analysis;

/// The text of a message: the plain-language sentence, the suggestion and the help line, as
/// `agentmark check` prints them (without the source line and caret, the editor draws those).
pub fn message(d: &Diagnostic) -> String {
    let mut out = d.message.clone();
    if let Some(c) = &d.did_you_mean {
        out.push_str(&format!(" Did you mean \"{c}\"?"));
    }
    if let Some(h) = &d.help {
        out.push_str(&format!("\n{h}"));
    }
    out
}

pub fn range(a: &Analysis, d: &Diagnostic) -> Range {
    let text = a.text(&d.file).unwrap_or("");
    // config errors point at a line, not at a word: underline what is on the line
    if !d.file.ends_with(".agent.md") && d.span.len <= 1 {
        let line = crate::text::line_text(text, d.span.line.saturating_sub(1) as usize);
        let lead = line.chars().take_while(|c| c.is_whitespace()).count() as u32;
        let len = line.trim().chars().count() as u32;
        if len > 0 {
            return span_range(text, agentmark::diag::Span::new(d.span.line, lead + 1, len));
        }
    }
    span_range(text, d.span)
}

pub fn to_lsp(a: &Analysis, d: &Diagnostic, source: &str) -> lsp_types::Diagnostic {
    lsp_types::Diagnostic {
        range: range(a, d),
        severity: Some(match d.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(d.code.to_string())),
        source: Some(source.to_string()),
        message: message(d),
        data: d.did_you_mean.as_ref().map(|c| json!({ "did_you_mean": c })),
        ..Default::default()
    }
}

/// Every diagnostic of the project by file path: the compiler's messages (source `agentmark`)
/// and the lint findings (source `agentmark-lint`).
pub fn by_file(a: &Analysis) -> BTreeMap<String, Vec<lsp_types::Diagnostic>> {
    let mut out: BTreeMap<String, Vec<lsp_types::Diagnostic>> = BTreeMap::new();
    for d in &a.compiler {
        out.entry(d.file.clone()).or_default().push(to_lsp(a, d, "agentmark"));
    }
    for d in &a.lint {
        out.entry(d.file.clone()).or_default().push(to_lsp(a, d, "agentmark-lint"));
    }
    out
}
