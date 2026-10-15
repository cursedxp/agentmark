//! Positions. The compiler counts 1-based lines and columns in characters; LSP counts 0-based lines
//! and UTF-16 code units.

use agentmark::diag::Span;
use lsp_types::{Position, Range};

/// The text of line `n` (0-based) without its line ending.
pub fn line_text(text: &str, n: usize) -> &str {
    text.split('\n').nth(n).unwrap_or("").trim_end_matches('\r')
}

/// The text of a statement line without its list marker: `1. print: "x"` and `- print: "x"`
/// both give `print: "x"`.
pub fn strip_marker(s: &str) -> &str {
    let t = s.trim_start();
    if let Some(r) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("+ ")) {
        return r;
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        if let Some(r) = t[digits..].strip_prefix(". ") {
            return r;
        }
    }
    t
}

/// Is this line a numbered step (`1. print: "x"`)?
pub fn is_numbered(s: &str) -> bool {
    let t = s.trim_start();
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    digits > 0 && t[digits..].starts_with(". ")
}

fn utf16(s: &str) -> u32 {
    s.chars().map(|c| c.len_utf16() as u32).sum()
}

/// LSP position of the character `col` (0-based, in characters) of line `line` (0-based).
pub fn position(text: &str, line: usize, col: usize) -> Position {
    let l = line_text(text, line);
    let before: String = l.chars().take(col).collect();
    Position::new(line as u32, utf16(&before))
}

/// The range a compiler [`Span`] covers in `text`.
pub fn span_range(text: &str, span: Span) -> Range {
    let line = span.line.saturating_sub(1) as usize;
    let col = span.col.saturating_sub(1) as usize;
    Range::new(position(text, line, col), position(text, line, col + span.len as usize))
}

/// (line, column in characters), both 0-based, of an LSP position.
pub fn char_pos(text: &str, pos: Position) -> (usize, usize) {
    let l = line_text(text, pos.line as usize);
    let mut units = 0u32;
    let mut chars = 0usize;
    for c in l.chars() {
        if units >= pos.character {
            break;
        }
        units += c.len_utf16() as u32;
        chars += 1;
    }
    (pos.line as usize, chars)
}

/// The position just after the last character of `text`.
pub fn end_position(text: &str) -> Position {
    let line = text.split('\n').count().saturating_sub(1);
    position(text, line, usize::MAX / 2)
}

/// Does the (1-based, character) span contain the (0-based, character) position?
pub fn span_contains(span: Span, line: usize, col: usize) -> bool {
    span.line as usize == line + 1
        && (span.col as usize).saturating_sub(1) <= col
        && col < (span.col as usize).saturating_sub(1) + span.len.max(1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn umlauts_and_astral_chars() {
        let t = "- log: \"Grüße 😀 {x}\"";
        let r = span_range(t, Span::new(1, 18, 3));
        // "- log: \"Grüße 😀 " is 17 chars, the emoji counts two UTF-16 units
        assert_eq!((r.start.character, r.end.character), (18, 21));
        assert_eq!(char_pos(t, r.start), (0, 17));
    }
}
