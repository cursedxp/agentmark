//! `money.*` (SPEC 22.3).

use crate::{Money, RtError};

/// `money.parse(t)`: an amount like `12.50 EUR` (at most the currency's decimals, white space around ignored).
pub fn parse(t: &str) -> Result<Money, RtError> {
    t.trim()
        .parse::<Money>()
        .map_err(|_| RtError::Lib(format!("\"{t}\" is not an amount like \"12.50 EUR\"")))
}

/// `money.format(m)`: the amount written like a literal, `12.50 EUR`.
pub fn format(m: Money) -> String {
    m.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_format_round_trip() {
        let m = parse(" 1200.5 USD ").unwrap();
        assert_eq!(format(m), "1200.50 USD");
        assert_eq!(format(parse("-0.05 EUR").unwrap()), "-0.05 EUR");
        assert_eq!(format(parse("500 JPY").unwrap()), "500 JPY");
    }

    #[test]
    fn bad_amounts_are_plain_errors() {
        assert_eq!(parse("12").unwrap_err().to_string(), "\"12\" is not an amount like \"12.50 EUR\"");
        assert!(parse("12.505 EUR").is_err());
        assert!(parse("12 XYZ").is_err());
    }
}
