//! Diagnostics: spans, the `Diagnostic` type, did-you-mean, and the SPEC 10 renderer.
//!
//! A rendered diagnostic looks like this (SPEC 10):
//!
//! ```text
//! examples/invoice-chaser.agent.md:12
//!   - email send custmer: "..."
//!                ^^^^^^^
//!   Unknown target "custmer" for tool email. Did you mean "customer"?
//! ```
//!
//! Every pass reports mistakes in the author's source through this module, never as a Rust
//! compiler error.

use serde::Serialize;

/// A region on one source line. `line` and `col` are 1-based; `col` and `len` count characters
/// (not bytes), so they are right for text containing e.g. umlauts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Default)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub len: u32,
}

impl Span {
    pub fn new(line: u32, col: u32, len: u32) -> Self {
        Span { line, col, len }
    }

    /// The span from the start of `self` to the end of `other` (both on the same line).
    pub fn to(self, other: Span) -> Span {
        let end = other.col + other.len;
        Span {
            line: self.line,
            col: self.col,
            len: end.saturating_sub(self.col),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Severity {
    Error,
    Warning,
}

/// One problem found in a source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// Stable code, e.g. `E001` (tabs). Not shown in the rendered text; used by `check --json`.
    pub code: &'static str,
    pub severity: Severity,
    /// Path of the file as the author knows it (used in the `file:line` header).
    pub file: String,
    pub span: Span,
    /// Plain-language explanation of what is wrong.
    pub message: String,
    /// `Did you mean "x"?` candidate, when one is within edit distance 2.
    pub did_you_mean: Option<String>,
    /// Optional extra line of advice (e.g. the suggested fix for a hard limit).
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &'static str, file: &str, span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Error,
            file: file.to_string(),
            span,
            message: message.into(),
            did_you_mean: None,
            help: None,
        }
    }

    pub fn warning(code: &'static str, file: &str, span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            ..Diagnostic::error(code, file, span, message)
        }
    }

    pub fn with_did_you_mean(mut self, candidate: Option<String>) -> Self {
        self.did_you_mean = candidate;
        self
    }

    /// Convenience: look `word` up among `candidates` and attach the best match.
    pub fn suggesting<'a>(self, word: &str, candidates: impl IntoIterator<Item = &'a str>) -> Self {
        let hit = did_you_mean(word, candidates);
        self.with_did_you_mean(hit)
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Render in the SPEC 10 format. `source` is the full text of the file the span points into.
    pub fn render(&self, source: &str) -> String {
        let line_text = source
            .lines()
            .nth(self.span.line.saturating_sub(1) as usize)
            .unwrap_or("")
            .trim_end();
        // The caret line mirrors tabs so the carets stay under the token in any terminal.
        let pad: String = line_text
            .chars()
            .take(self.span.col.saturating_sub(1) as usize)
            .map(|c| if c == '\t' { '\t' } else { ' ' })
            .collect();
        let carets = "^".repeat(self.span.len.max(1) as usize);
        let mut out = format!(
            "{}:{}\n  {}\n  {}{}\n  {}",
            self.file, self.span.line, line_text, pad, carets, self.message
        );
        if let Some(c) = &self.did_you_mean {
            out.push_str(&format!(" Did you mean \"{c}\"?"));
        }
        if let Some(h) = &self.help {
            out.push_str(&format!("\n  {h}"));
        }
        out
    }
}

/// Render several diagnostics, separated by one blank line.
pub fn render_all(diags: &[Diagnostic], source: &str) -> String {
    diags
        .iter()
        .map(|d| d.render(source))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Edit distance between two words (Damerau-Levenshtein: a swapped pair of letters counts as one
/// edit, which is how most typos happen).
pub fn edit_distance(a: &str, b: &str) -> usize {
    strsim::damerau_levenshtein(a, b)
}

/// The candidate closest to `word`, if it is within edit distance 2 (SPEC 10). Exact matches are
/// not suggestions. A candidate must also differ from `word` by fewer edits than `word` has
/// letters, so a one-letter name is never "corrected" into something unrelated. Ties go to the
/// first candidate listed.
pub fn did_you_mean<'a>(
    word: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let limit = 2.min(word.chars().count().saturating_sub(1));
    let mut best: Option<(usize, &str)> = None;
    for cand in candidates {
        if cand == word {
            continue;
        }
        let d = edit_distance(word, cand);
        if d <= limit && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, cand));
        }
    }
    best.map(|(_, c)| c.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn did_you_mean_finds_typo_within_distance_two() {
        let hit = did_you_mean("custmer", ["customer", "address"]);
        assert_eq!(hit.as_deref(), Some("customer"));
        assert_eq!(
            did_you_mean("emial", ["email", "crm"]).as_deref(),
            Some("email")
        );
    }

    #[test]
    fn did_you_mean_rejects_far_words_and_exact_matches() {
        assert_eq!(did_you_mean("banana", ["customer", "address"]), None);
        assert_eq!(did_you_mean("customer", ["customer"]), None);
        // distance 3 is too far
        assert_eq!(did_you_mean("custom", ["customer-x", "address"]), None);
    }

    #[test]
    fn did_you_mean_prefers_the_closest_then_the_first() {
        assert_eq!(
            did_you_mean("sned", ["send", "seed", "sent"]).as_deref(),
            Some("send")
        );
        assert_eq!(
            did_you_mean("lst", ["list", "lest"]).as_deref(),
            Some("list")
        );
        assert_eq!(did_you_mean("ge", ["get", "set"]).as_deref(), Some("get"));
    }

    #[test]
    fn did_you_mean_does_not_correct_one_letter_names() {
        assert_eq!(did_you_mean("x", ["y", "id"]), None);
    }

    #[test]
    fn span_to_covers_both() {
        let a = Span::new(3, 5, 2);
        let b = Span::new(3, 10, 4);
        assert_eq!(a.to(b), Span::new(3, 5, 9));
    }

    #[test]
    fn render_puts_carets_under_the_token() {
        let src = "# Agent: a\n- email send custmer: \"x\"\n";
        let col = "- email send ".chars().count() as u32 + 1;
        let d = Diagnostic::error(
            "E999",
            "t.agent.md",
            Span::new(2, col, 7),
            "Unknown target \"custmer\" for tool email.",
        )
        .with_did_you_mean(Some("customer".into()));
        let expected = "\
t.agent.md:2
  - email send custmer: \"x\"
               ^^^^^^^
  Unknown target \"custmer\" for tool email. Did you mean \"customer\"?";
        assert_eq!(d.render(src), expected);
    }

    #[test]
    fn render_counts_characters_not_bytes() {
        let src = "- log: \"Müller\" oops\n";
        let col = "- log: \"Müller\" ".chars().count() as u32 + 1;
        let d = Diagnostic::error(
            "E999",
            "t.agent.md",
            Span::new(1, col, 4),
            "Unexpected word.",
        );
        let out = d.render(src);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[1], "  - log: \"Müller\" oops");
        assert_eq!(lines[2], "                  ^^^^");
    }

    #[test]
    fn render_mirrors_tabs_in_the_caret_line_and_prints_help() {
        let src = "\t- log: \"x\"\n";
        let d = Diagnostic::error(
            "E001",
            "t.agent.md",
            Span::new(1, 1, 1),
            "Tabs are not allowed.",
        )
        .with_help("Use 2 spaces.");
        let out = d.render(src);
        assert_eq!(
            out,
            "t.agent.md:1\n  \t- log: \"x\"\n  ^\n  Tabs are not allowed.\n  Use 2 spaces."
        );
    }

    #[test]
    fn render_all_separates_with_blank_line() {
        let src = "a\nb\n";
        let d1 = Diagnostic::error("E1", "f", Span::new(1, 1, 1), "one");
        let d2 = Diagnostic::error("E2", "f", Span::new(2, 1, 1), "two");
        let out = render_all(&[d1, d2], src);
        assert_eq!(out, "f:1\n  a\n  ^\n  one\n\nf:2\n  b\n  ^\n  two");
    }
}
