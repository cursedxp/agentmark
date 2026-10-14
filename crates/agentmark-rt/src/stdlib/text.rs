//! `text.*` (SPEC 22.3). Lengths and widths count characters, not bytes.

/// `text.split(t, sep)`: the pieces between `sep` (every character when `sep` is empty).
pub fn split(t: &str, sep: &str) -> Vec<String> {
    if sep.is_empty() {
        return t.chars().map(String::from).collect();
    }
    t.split(sep).map(String::from).collect()
}

/// `text.lines(t)`: like Rust `str::lines` (no empty entry for a final newline, `\r\n` accepted).
pub fn lines(t: &str) -> Vec<String> {
    t.lines().map(String::from).collect()
}

/// `text.trim(t)`: without leading and trailing white space.
pub fn trim(t: &str) -> String {
    t.trim().to_string()
}

/// `text.lower(t)`.
pub fn lower(t: &str) -> String {
    t.to_lowercase()
}

/// `text.upper(t)`.
pub fn upper(t: &str) -> String {
    t.to_uppercase()
}

/// `text.replace(t, from, to)`: every occurrence (nothing changes when `from` is empty).
pub fn replace(t: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return t.to_string();
    }
    t.replace(from, to)
}

/// `text.contains(t, part)`.
pub fn contains(t: &str, part: &str) -> bool {
    t.contains(part)
}

/// `text.starts_with(t, p)`.
pub fn starts_with(t: &str, p: &str) -> bool {
    t.starts_with(p)
}

/// `text.ends_with(t, p)`.
pub fn ends_with(t: &str, p: &str) -> bool {
    t.ends_with(p)
}

/// `text.length(t)`: number of characters.
pub fn length(t: &str) -> i64 {
    t.chars().count() as i64
}

/// `text.pad(t, width)`: spaces added on the right up to `width` characters; longer text is kept whole.
pub fn pad(t: &str, width: i64) -> String {
    let have = t.chars().count();
    let want = usize::try_from(width).unwrap_or(0);
    let mut s = t.to_string();
    if have < want {
        s.extend(std::iter::repeat_n(' ', want - have));
    }
    s
}

/// `text.repeat(t, n)`: `n` copies (nothing for `n` below 1).
pub fn repeat(t: &str, n: i64) -> String {
    t.repeat(usize::try_from(n).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_lines() {
        assert_eq!(split("a,b,,c", ","), ["a", "b", "", "c"]);
        assert_eq!(split("abc", ""), ["a", "b", "c"]);
        assert_eq!(split("", ","), [""]);
        assert_eq!(lines("a\n\nb\r\nc\n"), ["a", "", "b", "c"]);
        assert!(lines("").is_empty());
    }

    #[test]
    fn simple_transforms() {
        assert_eq!(trim("  x \n"), "x");
        assert_eq!(lower("ÄBc"), "äbc");
        assert_eq!(upper("straße"), "STRASSE");
        assert_eq!(replace("aXbXc", "X", "--"), "a--b--c");
        assert_eq!(replace("abc", "", "x"), "abc");
    }

    #[test]
    fn tests_on_text() {
        assert!(contains("hello", "ell") && !contains("hello", "z"));
        assert!(starts_with("hello", "he") && ends_with("hello", "lo"));
        assert!(contains("abc", "") && starts_with("abc", ""));
    }

    #[test]
    fn length_pad_repeat() {
        assert_eq!(length("äöü"), 3);
        assert_eq!(pad("ab", 5), "ab   ");
        assert_eq!(pad("abcdef", 3), "abcdef");
        assert_eq!(pad("äb", 4), "äb  ");
        assert_eq!(pad("a", -2), "a");
        assert_eq!(repeat("-", 4), "----");
        assert_eq!(repeat("ab", 0), "");
        assert_eq!(repeat("ab", -3), "");
    }
}
