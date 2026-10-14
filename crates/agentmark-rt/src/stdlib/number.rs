//! `number.*` (SPEC 22.3). `int` is a 64-bit whole number; `round`, `min`, `max` and `abs` also take
//! `money` (Q340).

use crate::{Money, RtError};

/// A value `number.min`, `max`, `abs` and `round` work on: `int` and `money`.
pub trait Num: Sized {
    /// The smaller of two values (two amounts must have the same currency).
    fn smaller(a: Self, b: Self) -> Result<Self, RtError>;
    /// The larger of two values.
    fn larger(a: Self, b: Self) -> Result<Self, RtError>;
    /// The value without its sign.
    fn absolute(self) -> Result<Self, RtError>;
    /// Rounded to `places` decimal places of the whole unit (half away from zero).
    fn rounded(self, places: i64) -> Result<Self, RtError>;
}

/// Round `v` to a multiple of `10^drop` (half away from zero).
fn round_to_power(v: i64, drop: u32) -> Result<i64, RtError> {
    if drop == 0 {
        return Ok(v);
    }
    let unit = 10i128.checked_pow(drop).ok_or_else(overflow)?;
    let v = v as i128;
    let mut q = v / unit;
    if (v % unit).abs() * 2 >= unit {
        q += if v < 0 { -1 } else { 1 };
    }
    i64::try_from(q * unit).map_err(|_| overflow())
}

fn overflow() -> RtError {
    RtError::Lib("the number is too large".into())
}

impl Num for i64 {
    fn smaller(a: i64, b: i64) -> Result<i64, RtError> {
        Ok(a.min(b))
    }
    fn larger(a: i64, b: i64) -> Result<i64, RtError> {
        Ok(a.max(b))
    }
    fn absolute(self) -> Result<i64, RtError> {
        self.checked_abs().ok_or_else(overflow)
    }
    fn rounded(self, places: i64) -> Result<i64, RtError> {
        // a whole number has no decimal places: only negative `places` (tens, hundreds) change it
        if places >= 0 {
            return Ok(self);
        }
        round_to_power(self, u32::try_from(places.unsigned_abs()).unwrap_or(u32::MAX))
    }
}

impl Num for Money {
    fn smaller(a: Money, b: Money) -> Result<Money, RtError> {
        Ok(if a.try_cmp(&b)?.is_le() { a } else { b })
    }
    fn larger(a: Money, b: Money) -> Result<Money, RtError> {
        Ok(if a.try_cmp(&b)?.is_ge() { a } else { b })
    }
    fn absolute(self) -> Result<Money, RtError> {
        Ok(Money::new(self.minor.checked_abs().ok_or_else(overflow)?, self.currency))
    }
    fn rounded(self, places: i64) -> Result<Money, RtError> {
        let exp = i64::from(self.currency.exponent());
        let drop = exp - places;
        if drop <= 0 {
            return Ok(self);
        }
        Ok(Money::new(round_to_power(self.minor, u32::try_from(drop).unwrap_or(u32::MAX))?, self.currency))
    }
}

/// `number.round(x, places)`.
pub fn round<T: Num>(x: T, places: i64) -> Result<T, RtError> {
    x.rounded(places)
}

/// `number.min(a, b)`.
pub fn min<T: Num>(a: T, b: T) -> Result<T, RtError> {
    T::smaller(a, b)
}

/// `number.max(a, b)`.
pub fn max<T: Num>(a: T, b: T) -> Result<T, RtError> {
    T::larger(a, b)
}

/// `number.abs(x)`.
pub fn abs<T: Num>(x: T) -> Result<T, RtError> {
    x.absolute()
}

/// `number.parse(t)`: a whole number, optionally signed, white space around it ignored.
pub fn parse(t: &str) -> Result<i64, RtError> {
    let s = t.trim();
    s.strip_prefix('+')
        .unwrap_or(s)
        .parse::<i64>()
        .map_err(|_| RtError::Lib(format!("\"{t}\" is not a whole number")))
}

/// `number.format(x, places)`: the number written with exactly `places` decimals (`format(12, 2)` is `12.00`).
pub fn format(x: i64, places: i64) -> String {
    match usize::try_from(places) {
        Ok(p) if p > 0 => format!("{x}.{}", "0".repeat(p)),
        _ => x.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Currency;

    fn eur(s: &str) -> Money {
        format!("{s} EUR").parse().unwrap()
    }

    #[test]
    fn ints() {
        assert_eq!(min(3, 5).unwrap(), 3);
        assert_eq!(max(3, 5).unwrap(), 5);
        assert_eq!(abs(-7).unwrap(), 7);
        assert!(abs(i64::MIN).is_err());
        assert_eq!(round(1234, 2).unwrap(), 1234);
        assert_eq!(round(1234, -1).unwrap(), 1230);
        assert_eq!(round(1235, -1).unwrap(), 1240);
        assert_eq!(round(-1235, -1).unwrap(), -1240);
        assert_eq!(round(1250, -2).unwrap(), 1300);
    }

    #[test]
    fn money() {
        assert_eq!(min(eur("5.00"), eur("3.50")).unwrap(), eur("3.50"));
        assert_eq!(max(eur("5.00"), eur("3.50")).unwrap(), eur("5.00"));
        assert_eq!(abs(eur("-2.25")).unwrap(), eur("2.25"));
        assert_eq!(round(eur("12.34"), 1).unwrap(), eur("12.30"));
        assert_eq!(round(eur("12.35"), 1).unwrap(), eur("12.40"));
        assert_eq!(round(eur("12.50"), 0).unwrap(), eur("13.00"));
        assert_eq!(round(eur("12.50"), 2).unwrap(), eur("12.50"));
        let usd = Money::new(100, Currency::USD);
        let e = min(eur("1.00"), usd).unwrap_err().to_string();
        assert!(e.contains("currency mismatch"), "{e}");
    }

    #[test]
    fn parse_and_format() {
        assert_eq!(parse(" 42 ").unwrap(), 42);
        assert_eq!(parse("-7").unwrap(), -7);
        assert_eq!(parse("+7").unwrap(), 7);
        assert_eq!(parse("4x").unwrap_err().to_string(), "\"4x\" is not a whole number");
        assert!(parse("").is_err() && parse("1.5").is_err());
        assert_eq!(format(12, 2), "12.00");
        assert_eq!(format(12, 0), "12");
        assert_eq!(format(-3, 1), "-3.0");
        assert_eq!(format(5, -1), "5");
    }
}
