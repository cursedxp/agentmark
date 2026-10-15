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
    /// `> a note`
    Comment,
    /// `- a property or field` (a `*` or `+` bullet is read the same way, SPEC 23.1)
    Item,
    /// `1. a step`: the number as written (SPEC 23.3)
    Numbered(u32),
    /// A bare line ending in `:` inside a step (`otherwise:`, `else:`), or a bare `stop`
    Control,
    /// A line of the description paragraph directly under a heading (SPEC 23.2)
    Paragraph,
    /// The opening ```` ```rust ```` line; carries the language tag.
    FenceOpen(String),
    /// A line inside a fence, kept verbatim (minus the fence's own indentation).
    FenceBody,
    FenceClose,
    Blank,
}

impl LineKind {
    /// A list item, bulleted or numbered.
    pub fn is_item(&self) -> bool {
        matches!(self, LineKind::Item | LineKind::Numbered(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    /// 1-based line number.
    pub line_no: u32,
    /// Indentation in spaces (a step under `1.` is indented 3, a property under `- ` 2).
    pub indent: u32,
    pub kind: LineKind,
    /// Content with the marker removed: heading text after `# `, comment text after `>`, item text
    /// after `- ` or `1. `, the trimmed text of a control or paragraph line, the raw text of a
    /// fence body line.
    pub text: String,
    /// 1-based character column where `text` starts on the source line. For an item this is one
    /// past the column its children start at.
    pub col: u32,
    /// Byte offset of the start of this source line in the file.
    pub byte_start: usize,
}

pub const E_TAB: &str = "E001";
pub const E_INDENT: &str = "E002";
pub const E_FENCE: &str = "E003";
pub const E_LINE: &str = "E004";

/// `1. text` -> (number, byte length of the marker including the space).
fn numbered(rest: &str) -> Option<(u32, usize)> {
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    let after = &rest[digits..];
    if after == "." {
        return Some((rest[..digits].parse().ok()?, digits + 1));
    }
    let tail = after.strip_prefix(". ")?;
    let pad = tail.len() - tail.trim_start().len();
    Some((rest[..digits].parse().ok()?, digits + 2 + pad))
}

/// Classify every line of `src`. Problems (tabs, unclosed fences, lines that are not part of the
/// language) are collected, not fatal, so one run reports them all.
///
/// Prose is the description of the heading above it (SPEC 23.2): it may only follow a heading,
/// with blank lines and further paragraphs in between, and before anything else.
pub fn lex(file: &str, src: &str) -> (Vec<Line>, Vec<Diagnostic>) {
    let mut lines = Vec::new();
    let mut diags = Vec::new();
    // Inside a fence: (indent in spaces of the opening line, line number of the opening line).
    let mut fence: Option<(usize, u32)> = None;
    let mut byte_start = 0usize;
    // Directly under a heading, where a description paragraph may stand.
    let mut in_description = false;

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
                    indent: spaces as u32,
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
            indent: indent_spaces as u32,
            kind,
            text: text.to_string(),
            col: text_col as u32 + 1,
            byte_start: this_start,
        };
        let char_col = |byte: usize| raw[..byte].chars().count();

        // a blank line keeps a description open (a second paragraph may follow)
        if rest.trim().is_empty() {
            lines.push(mk(LineKind::Blank, 0, "", 0));
            continue;
        }

        // Notes are free text: tabs and odd indentation inside them do not matter.
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
                .with_help("Indent with spaces: 3 under a numbered step, 2 under a \"- \" line."),
            );
            in_description = false;
            continue;
        }

        let rest_t = rest.trim_end();
        let was_description = std::mem::replace(&mut in_description, false);
        if rest_t.starts_with("```") {
            let lang = rest_t.trim_start_matches('`').trim().to_string();
            lines.push(mk(LineKind::FenceOpen(lang), spaces, rest_t, spaces));
            fence = Some((spaces, line_no));
            continue;
        }

        if let Some(h) = rest_t.strip_prefix("## ") {
            if spaces > 0 {
                diags.push(heading_indented(file, line_no, spaces));
                continue;
            }
            lines.push(mk(LineKind::Heading2, 0, h.trim(), 3));
            in_description = true;
        } else if let Some(h) = rest_t.strip_prefix("# ") {
            if spaces > 0 {
                diags.push(heading_indented(file, line_no, spaces));
                continue;
            }
            lines.push(mk(LineKind::Heading1, 0, h.trim(), 2));
            in_description = true;
        } else if rest_t == "-"
            || rest_t.starts_with("- ")
            || rest_t.starts_with("* ")
            || rest_t.starts_with("+ ")
        {
            let body = rest_t[1..].trim_start();
            let off = rest_t.len() - body.len();
            lines.push(mk(LineKind::Item, spaces, body, spaces + off));
        } else if let Some((n, width)) = numbered(rest_t) {
            let body = rest_t[width.min(rest_t.len())..].trim_start();
            let off = rest_t.len() - body.len();
            lines.push(mk(LineKind::Numbered(n), spaces, body, spaces + off));
        } else if was_description && spaces == 0 {
            lines.push(mk(LineKind::Paragraph, 0, rest_t, 0));
            in_description = true;
        } else if rest_t.ends_with(':') || rest_t == "stop" {
            lines.push(mk(LineKind::Control, spaces, rest_t, spaces));
        } else if rest_t.starts_with("for each ")
            || rest_t.starts_with("if ")
            || matches!(rest_t, "else" | "otherwise" | "parallel")
        {
            let col = spaces + rest_t.chars().count() + 1;
            diags.push(
                Diagnostic::error(
                    E_LINE,
                    file,
                    Span::new(line_no, col as u32, 1),
                    "This line is missing the \":\" at its end.",
                )
                .with_help(
                    "A line that has steps under it ends with a colon, like \"for each row in rows:\".",
                ),
            );
        } else {
            diags.push(
                Diagnostic::error(
                    E_LINE,
                    file,
                    Span::new(line_no, spaces as u32 + 1, rest_t.chars().count() as u32),
                    "This line is not part of the language: steps start with a number (\"1. \"), properties with \"- \" and headings with \"# \" or \"## \".",
                )
                .with_help("A description is the paragraph directly under a heading; a note for people starts with \">\"."),
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
        let src = "# Agent: a\nChases invoices.\n\n- model: claude\n\n## Do x\n1. for each y in z:\n   1. stop\n   otherwise:\n> note\n";
        use LineKind::*;
        assert_eq!(
            kinds(src),
            vec![
                Heading1, Paragraph, Blank, Item, Blank, Heading2, Numbered(1), Numbered(1), Control,
                Comment, Blank
            ]
        );
    }

    #[test]
    fn a_description_may_have_several_paragraphs() {
        use LineKind::*;
        assert_eq!(
            kinds("# Record: A\nFirst line\nsecond line:\n\nSecond paragraph.\n- x: text\n"),
            vec![Heading1, Paragraph, Paragraph, Blank, Paragraph, Item, Blank]
        );
    }

    #[test]
    fn prose_after_a_list_is_e004() {
        let (_, diags) = lex("t", "# Record: A\n- x: text\nThis is a paragraph.\n");
        assert_eq!(diags[0].code, E_LINE);
    }

    #[test]
    fn measures_indent_in_spaces_and_text_columns() {
        let (lines, _) = lex("t", "   1. add x to y\n12. stop\n  * z\n");
        assert_eq!(lines[0].indent, 3);
        assert_eq!(lines[0].kind, LineKind::Numbered(1));
        assert_eq!(lines[0].text, "add x to y");
        assert_eq!(lines[0].col, 7);
        assert_eq!(lines[1].kind, LineKind::Numbered(12));
        assert_eq!(lines[1].col, 5);
        assert_eq!(lines[2].kind, LineKind::Item);
        assert_eq!(lines[2].text, "z");
    }

    #[test]
    fn star_and_plus_bullets_are_items_and_two_digit_steps_start_their_text_at_four() {
        let (lines, diags) = lex("t", "* a: 1\n+ b: 2\n- c: 3\n  10. stop\n     1. print: \"x\"\n");
        assert!(diags.is_empty(), "{diags:?}");
        let kinds: Vec<_> = lines.iter().map(|l| l.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                LineKind::Item,
                LineKind::Item,
                LineKind::Item,
                LineKind::Numbered(10),
                LineKind::Numbered(1),
                LineKind::Blank
            ]
        );
        assert_eq!(lines[3].indent, 2);
        // the text of `  10. stop` starts at column 7: its children go 6 spaces in
        assert_eq!(lines[3].col, 7);
        assert_eq!(lines[4].indent, 5);
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
    fn indented_heading_is_e002() {
        let (_, diags) = lex("t", "# Agent: a\n  ## Do x\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, E_INDENT);
    }

    #[test]
    fn unclosed_fence_is_e003() {
        let (_, diags) = lex("t", "```rust\nfn x() {}\n");
        assert_eq!(diags[0].code, E_FENCE);
        assert_eq!(diags[0].span.line, 1);
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
