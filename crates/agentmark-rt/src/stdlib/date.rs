//! `date.*` (SPEC 22.3). Patterns are either chrono/strftime (`%Y-%m-%d`, `%Y%m%d`; used when the pattern
//! contains a `%`) or the readable form: `YYYY` `YY` `MMMM` `MMM` `MM` `DD` `dddd` `ddd` `HH` `mm` `ss`,
//! any other character as it is, `[text]` for letters that must not be read as a token (Q341).

use crate::RtError;
use chrono::format::{Item, StrftimeItems};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use std::fmt::Write;

/// The chrono pattern for `pattern`.
pub fn to_strftime(pattern: &str) -> String {
    if pattern.contains('%') {
        return pattern.to_string();
    }
    const TOKENS: &[(&str, &str)] = &[
        ("YYYY", "%Y"),
        ("YY", "%y"),
        ("MMMM", "%B"),
        ("MMM", "%b"),
        ("MM", "%m"),
        ("DD", "%d"),
        ("dddd", "%A"),
        ("ddd", "%a"),
        ("HH", "%H"),
        ("mm", "%M"),
        ("ss", "%S"),
    ];
    let mut out = String::new();
    let mut rest = pattern;
    'outer: while !rest.is_empty() {
        if let Some(inner) = rest.strip_prefix('[') {
            if let Some(end) = inner.find(']') {
                out.push_str(&inner[..end].replace('%', "%%"));
                rest = &inner[end + 1..];
                continue;
            }
        }
        for (tok, chrono_tok) in TOKENS {
            if let Some(r) = rest.strip_prefix(tok) {
                out.push_str(chrono_tok);
                rest = r;
                continue 'outer;
            }
        }
        let c = rest.chars().next().expect("rest is not empty");
        if c == '%' {
            out.push_str("%%");
        } else {
            out.push(c);
        }
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// `Ok` when `pattern` is a usable pattern (the compiler checks literal patterns with this).
pub fn check(pattern: &str) -> Result<(), String> {
    let f = to_strftime(pattern);
    if StrftimeItems::new(&f).any(|i| matches!(i, Item::Error)) {
        return Err(format!(
            "\"{pattern}\" is not a date pattern. Use YYYY MM DD HH mm ss (for example \"DD.MM.YYYY HH:mm\") or % codes (\"%Y-%m-%d\")."
        ));
    }
    Ok(())
}

/// A value `date.format` can write: a date (time of day 00:00) or a datetime.
pub trait DateLike {
    /// As a date and time of day.
    fn naive(&self) -> NaiveDateTime;
}

impl DateLike for NaiveDate {
    fn naive(&self) -> NaiveDateTime {
        self.and_hms_opt(0, 0, 0).expect("midnight is valid")
    }
}

impl DateLike for DateTime<Utc> {
    fn naive(&self) -> NaiveDateTime {
        self.naive_utc()
    }
}

/// `date.format(d, pattern)`; a pattern that is not valid is returned as it is.
pub fn format<D: DateLike>(d: &D, pattern: &str) -> String {
    if check(pattern).is_err() {
        return pattern.to_string();
    }
    let mut s = String::new();
    match write!(s, "{}", d.naive().format(&to_strftime(pattern))) {
        Ok(()) => s,
        Err(_) => pattern.to_string(),
    }
}

/// `date.parse(t, pattern)`: the date `t` is written as (any time of day in it is dropped).
pub fn parse(t: &str, pattern: &str) -> Result<NaiveDate, RtError> {
    let fail = || RtError::Lib(format!("\"{t}\" is not a date written like \"{pattern}\""));
    check(pattern).map_err(RtError::Lib)?;
    let f = to_strftime(pattern);
    NaiveDate::parse_from_str(t.trim(), &f)
        .or_else(|_| NaiveDateTime::parse_from_str(t.trim(), &f).map(|d| d.date()))
        .map_err(|_| fail())
}

/// `date.days_between(a, b)`: days from `a` to `b` (negative when `b` is earlier).
pub fn days_between(a: NaiveDate, b: NaiveDate) -> i64 {
    (b - a).num_days()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn readable_patterns_map_to_chrono() {
        assert_eq!(to_strftime("YYYY-MM-DD"), "%Y-%m-%d");
        assert_eq!(to_strftime("DD.MM.YYYY HH:mm"), "%d.%m.%Y %H:%M");
        assert_eq!(to_strftime("dddd, MMMM DD"), "%A, %B %d");
        assert_eq!(to_strftime("[Day] DD"), "Day %d");
        assert_eq!(to_strftime("%Y%m%d"), "%Y%m%d");
    }

    #[test]
    fn format_both_languages() {
        let x = d(2026, 10, 3);
        assert_eq!(format(&x, "YYYY-MM-DD"), "2026-10-03");
        assert_eq!(format(&x, "%Y-%m-%d"), "2026-10-03");
        assert_eq!(format(&x, "DD.MM.YYYY HH:mm"), "03.10.2026 00:00");
        assert_eq!(format(&x, "dddd MMMM DD"), "Saturday October 03");
        let dt = x.and_hms_opt(14, 5, 9).unwrap().and_utc();
        assert_eq!(format(&dt, "HH:mm:ss"), "14:05:09");
        assert_eq!(format(&x, "%Q"), "%Q");
    }

    #[test]
    fn parse_dates() {
        assert_eq!(parse("20261003", "%Y%m%d").unwrap(), d(2026, 10, 3));
        assert_eq!(parse("03.10.2026", "DD.MM.YYYY").unwrap(), d(2026, 10, 3));
        assert_eq!(parse("03.10.2026 14:15", "DD.MM.YYYY HH:mm").unwrap(), d(2026, 10, 3));
        let e = parse("20261399", "%Y%m%d").unwrap_err().to_string();
        assert_eq!(e, "\"20261399\" is not a date written like \"%Y%m%d\"");
        assert!(parse("2026-02-30", "YYYY-MM-DD").is_err());
        assert!(parse("x", "%Q").is_err());
    }

    #[test]
    fn days() {
        assert_eq!(days_between(d(2026, 10, 1), d(2026, 10, 14)), 13);
        assert_eq!(days_between(d(2026, 10, 14), d(2026, 10, 1)), -13);
    }

    #[test]
    fn check_patterns() {
        assert!(check("YYYY-MM-DD").is_ok());
        assert!(check("%Y").is_ok());
        assert!(check("%Q").is_err());
    }
}
