//! `regex.*` (SPEC 22.3), Rust `regex` syntax (no look-around, `$1` in replacements). Patterns are
//! compiled once and kept. A pattern that does not compile is an error with a plain message.

use crate::RtError;
use ::regex::Regex;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn compiled(pattern: &str) -> Result<Regex, RtError> {
    static CACHE: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = cache.get(pattern) {
        return Ok(r.clone());
    }
    let r = Regex::new(pattern).map_err(|e| RtError::Lib(describe(pattern, &e)))?;
    cache.insert(pattern.to_string(), r.clone());
    Ok(r)
}

fn describe(pattern: &str, e: &::regex::Error) -> String {
    let why = match e {
        ::regex::Error::Syntax(s) => s.lines().last().unwrap_or("").trim_start_matches("error: ").to_string(),
        other => other.to_string(),
    };
    format!("\"{pattern}\" is not a valid pattern: {why}")
}

/// `Ok` when `pattern` compiles, else the plain message (the compiler checks literal patterns with this).
pub fn check(pattern: &str) -> Result<(), String> {
    Regex::new(pattern).map(|_| ()).map_err(|e| describe(pattern, &e))
}

/// `regex.matches(t, pattern)`: does the pattern match anywhere in `t`?
pub fn matches(t: &str, pattern: &str) -> Result<bool, RtError> {
    Ok(compiled(pattern)?.is_match(t))
}

/// `regex.find(t, pattern)`: the first match.
pub fn find(t: &str, pattern: &str) -> Result<Option<String>, RtError> {
    Ok(compiled(pattern)?.find(t).map(|m| m.as_str().to_string()))
}

/// `regex.captures(t, pattern)`: the capture groups of the first match, without the whole match
/// (a group that took no part is empty text).
pub fn captures(t: &str, pattern: &str) -> Result<Option<Vec<String>>, RtError> {
    Ok(compiled(pattern)?.captures(t).map(|c| {
        c.iter().skip(1).map(|g| g.map(|m| m.as_str().to_string()).unwrap_or_default()).collect()
    }))
}

/// `regex.replace(t, pattern, with)`: every match replaced; `$1` is group 1 (`$$` a dollar sign).
pub fn replace(t: &str, pattern: &str, with: &str) -> Result<String, RtError> {
    Ok(compiled(pattern)?.replace_all(t, with).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_find_captures() {
        assert!(matches("IMG_2026", "^IMG_[0-9]+$").unwrap());
        assert!(!matches("img", "^IMG").unwrap());
        assert_eq!(find("a12b345", "[0-9]+").unwrap().as_deref(), Some("12"));
        assert_eq!(find("abc", "[0-9]+").unwrap(), None);
        let c = captures("IMG_20261003_141500.jpg", "^IMG_([0-9]+)_([0-9]+)[.]jpg$").unwrap();
        assert_eq!(c, Some(vec!["20261003".to_string(), "141500".to_string()]));
        assert_eq!(captures("x", "y(z)").unwrap(), None);
        assert_eq!(captures("a", "a(b)?").unwrap(), Some(vec![String::new()]));
        assert_eq!(captures("abc", "abc").unwrap(), Some(vec![]));
    }

    #[test]
    fn replace_uses_groups() {
        assert_eq!(replace("**bold** and **more**", "[*][*]([a-z]+)[*][*]", "<b>$1</b>").unwrap(), "<b>bold</b> and <b>more</b>");
        assert_eq!(replace("a.b", "[.]", "$$").unwrap(), "a$b");
    }

    #[test]
    fn bad_pattern_is_a_plain_error() {
        let e = matches("x", "(").unwrap_err().to_string();
        assert!(e.starts_with("\"(\" is not a valid pattern: "), "{e}");
        assert!(check("[a-z]+").is_ok());
        assert!(check("(").is_err());
    }
}
