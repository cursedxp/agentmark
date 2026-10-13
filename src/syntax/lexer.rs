//! Line lexer (DESIGN 2.1).
//!
//! AgentMark is line-structured, so the lexer is a *classifier*: it decides what kind of line each
//! source line is, measures its indentation, and keeps fenced code verbatim. It does not parse
//! anything inside a line; that is the parser's job (chumsky, `parser.rs`).

use crate::diag::{Diagnostic, Span};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum LineKind {
    /// `# Agent: x`
    Heading1,
    /// `## Every day at 09:00`
    Heading2,
    /// `> a comment`
    Comment,
    /// `- a statement or property`
    Item,
    /// A bare line ending in `:` (or a bare `stop`), e.g. `for each x in y:`
    Control,
    /// The opening ```` ```rust ```` line; carries the language tag.
    FenceOpen(String),
    /// A line inside a fence, kept verbatim (minus the fence's own indentation).
    FenceBody,
    FenceClose,
    Blank,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    /// 1-based line number.
    pub line_no: u32,
    /// Indentation in 2-space levels.
    pub indent: u32,
    pub kind: LineKind,
    /// Content with the marker removed: heading text after `# `, comment text after `>`, item text
    /// after `- `, the trimmed text of a control line, the raw text of a fence body line.
    pub text: String,
    /// 1-based character column where `text` starts on the source line.
    pub col: u32,
    /// Byte offset of the start of this source line in the file.
    pub byte_start: usize,
}

pub const E_TAB: &str = "E001";
pub const E_INDENT: &str = "E002";
pub const E_FENCE: &str = "E003";
pub const E_LINE: &str = "E004";

/// Classify every line of `src`. Problems (tabs, odd indentation, unclosed fences, lines that are
/// not part of the language) are collected, not fatal, so one run reports them all.
pub fn lex(file: &str, src: &str) -> (Vec<Line>, Vec<Diagnostic>) {
    let mut lines = Vec::new();
    let mut diags = Vec::new();
    // Inside a fence: (indent in spaces of the opening line, line number of the opening line).
    let mut fence: Option<(usize, u32)> = None;
    let mut byte_start = 0usize;

    for (i, raw) in src.split('\n').enumerate() {
        let line_no = i as u32 + 1;
        let this_start = byte_start;
        byte_start += raw.len() + 1;
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let spaces = raw.len() - raw.trim_start_matches(' ').len();
        let rest = &raw[spaces..];

        if let Some((fence_indent, _)) = fence {
            if rest.trim_end() == "```" {
                lines.push(Line {
                    line_no,
                    indent: (spaces / 2) as u32,
                    kind: LineKind::FenceClose,
                    text: String::new(),
                    col: spaces as u32 + 1,
                    byte_start: this_start,
                });
                fence = None;
            } else {
                let strip = spaces.min(fence_indent);
                lines.push(Line {
                    line_no,
                    indent: 0,
                    kind: LineKind::FenceBody,
                    text: raw[strip..].trim_end().to_string(),
                    col: strip as u32 + 1,
                    byte_start: this_start,
                });
            }
            continue;
        }

        let mk = |kind: LineKind, indent_spaces: usize, text: &str, text_col: usize| Line {
            line_no,
            indent: (indent_spaces / 2) as u32,
            kind,
            text: text.to_string(),
            col: text_col as u32 + 1,
            byte_start: this_start,
        };
        let char_col = |byte: usize| raw[..byte].chars().count();

        if rest.trim().is_empty() {
            lines.push(mk(LineKind::Blank, 0, "", 0));
            continue;
        }

        // Comments are free text: tabs and odd indentation inside them do not matter.
        if let Some(body) = rest.strip_prefix('>') {
            let body = body.strip_prefix(' ').unwrap_or(body);
            let off = rest.len() - body.len();
            lines.push(mk(
                LineKind::Comment,
                spaces,
                body.trim_end(),
                char_col(spaces + off),
            ));
            continue;
        }

        if let Some(tab) = raw.find('\t') {
            let len = raw[tab..].chars().take_while(|c| *c == '\t').count();
            diags.push(
                Diagnostic::error(
                    E_TAB,
                    file,
                    Span::new(line_no, char_col(tab) as u32 + 1, len as u32),
                    "Tabs are not allowed in AgentMark files.",
                )
                .with_help("Use 2 spaces for each indentation level."),
            );
            continue;
        }

        let rest_t = rest.trim_end();
        if rest_t.starts_with("```") {
            let lang = rest_t.trim_start_matches('`').trim().to_string();
            lines.push(mk(LineKind::FenceOpen(lang), spaces, rest_t, spaces));
            fence = Some((spaces, line_no));
            continue;
        }

        if spaces % 2 != 0 {
            diags.push(
                Diagnostic::error(
                    E_INDENT,
                    file,
                    Span::new(line_no, 1, spaces as u32),
                    format!(
                        "Indentation must be 2 spaces per level, but this line is indented {spaces} spaces."
                    ),
                )
                .with_help(format!(
                    "Use {} or {} spaces.",
                    spaces - 1,
                    spaces + 1
                )),
            );
            continue;
        }

        if let Some(h) = rest_t.strip_prefix("## ") {
            if spaces > 0 {
                diags.push(heading_indented(file, line_no, spaces));
                continue;
            }
            lines.push(mk(LineKind::Heading2, 0, h.trim(), 3));
        } else if let Some(h) = rest_t.strip_prefix("# ") {
            if spaces > 0 {
                diags.push(heading_indented(file, line_no, spaces));
                continue;
            }
            lines.push(mk(LineKind::Heading1, 0, h.trim(), 2));
        } else if rest_t == "-" || rest_t.starts_with("- ") {
            let body = rest_t.strip_prefix('-').unwrap_or("").trim_start();
            let off = rest_t.len() - body.len();
            lines.push(mk(LineKind::Item, spaces, body, spaces + off));
        } else if rest_t.ends_with(':') || rest_t == "stop" {
            lines.push(mk(LineKind::Control, spaces, rest_t, spaces));
        } else {
            diags.push(
                Diagnostic::error(
                    E_LINE,
                    file,
                    Span::new(line_no, spaces as u32 + 1, rest_t.chars().count() as u32),
                    "This line is not part of the language: statements start with \"- \" and headings with \"# \" or \"## \".",
                )
                .with_help("Put explanations in a \"> comment\" line."),
            );
        }
    }

    if let Some((_, open_line)) = fence {
        diags.push(Diagnostic::error(
            E_FENCE,
            file,
            Span::new(open_line, 1, 3),
            "This code fence is never closed. Add a line with just ``` after the code.",
        ));
    }
    (lines, diags)
}

fn heading_indented(file: &str, line_no: u32, spaces: usize) -> Diagnostic {
    Diagnostic::error(
        E_INDENT,
        file,
        Span::new(line_no, 1, spaces as u32),
        "Headings must start at the beginning of the line.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<LineKind> {
        let (lines, diags) = lex("t.agent.md", src);
        assert!(diags.is_empty(), "{diags:?}");
        lines.into_iter().map(|l| l.kind).collect()
    }

    #[test]
    fn classifies_every_kind() {
        let src =
            "> doc\n# Agent: a\n- model: claude\n\n## Do x\nfor each y in z:\n  - stop\nstop\n";
        use LineKind::*;
        assert_eq!(
            kinds(src),
            vec![Comment, Heading1, Item, Blank, Heading2, Control, Item, Control, Blank]
        );
    }

    #[test]
    fn measures_indent_in_levels_and_text_columns() {
        let (lines, _) = lex("t", "  - set x = 1\n");
        assert_eq!(lines[0].indent, 1);
        assert_eq!(lines[0].text, "set x = 1");
        assert_eq!(lines[0].col, 5);
    }

    #[test]
    fn fences_are_verbatim_and_strip_the_fence_indent() {
        let src = "  ```rust\n  let x = 1;\n    nested\n\ttabbed\n  ```\n";
        let (lines, diags) = lex("t", src);
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(lines[0].kind, LineKind::FenceOpen("rust".into()));
        assert_eq!(lines[1].text, "let x = 1;");
        assert_eq!(lines[2].text, "  nested");
        assert_eq!(lines[3].text, "\ttabbed");
        assert_eq!(lines[4].kind, LineKind::FenceClose);
    }

    #[test]
    fn tab_in_indentation_is_e001_with_its_position() {
        let (_, diags) = lex("t", "# Agent: a\n\t- log: \"x\"\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, E_TAB);
        assert_eq!(diags[0].span, Span::new(2, 1, 1));
    }

    #[test]
    fn tabs_inside_comments_are_fine() {
        let (_, diags) = lex("t", "> a\tb\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn odd_indentation_is_e002() {
        let (_, diags) = lex("t", "# Agent: a\n   - log: \"x\"\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, E_INDENT);
        assert_eq!(diags[0].span, Span::new(2, 1, 3));
    }

    #[test]
    fn unclosed_fence_is_e003() {
        let (_, diags) = lex("t", "```rust\nfn x() {}\n");
        assert_eq!(diags[0].code, E_FENCE);
        assert_eq!(diags[0].span.line, 1);
    }

    #[test]
    fn prose_is_e004() {
        let (_, diags) = lex("t", "# Agent: a\nThis is a paragraph.\n");
        assert_eq!(diags[0].code, E_LINE);
    }

    #[test]
    fn crlf_line_endings_are_handled() {
        let (lines, diags) = lex("t", "# Agent: a\r\n- model: claude\r\n");
        assert!(diags.is_empty());
        assert_eq!(lines[1].text, "model: claude");
    }

    #[test]
    fn byte_offsets_point_at_line_starts() {
        let src = "# A: a\n- x: 1\n";
        let (lines, _) = lex("t", src);
        assert_eq!(&src[lines[1].byte_start..], "- x: 1\n");
    }
}
