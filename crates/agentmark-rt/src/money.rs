use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

/// Supported ISO currencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Currency {
    /// Euro (2 minor digits).
    EUR,
    /// US dollar (2 minor digits).
    USD,
    /// Pound sterling (2 minor digits).
    GBP,
    /// Swiss franc (2 minor digits).
    CHF,
    /// Japanese yen (0 minor digits).
    JPY,
}

impl Currency {
    /// Number of decimal digits in the minor unit (2 for EUR, 0 for JPY).
    pub fn exponent(self) -> u32 {
        match self {
            Currency::JPY => 0,
            _ => 2,
        }
    }

    /// The three-letter ISO code.
    pub fn code(self) -> &'static str {
        match self {
            Currency::EUR => "EUR",
            Currency::USD => "USD",
            Currency::GBP => "GBP",
            Currency::CHF => "CHF",
            Currency::JPY => "JPY",
        }
    }

    /// Parse an upper-case ISO code, `None` when unsupported.
    pub fn from_code(code: &str) -> Option<Currency> {
        Some(match code {
            "EUR" => Currency::EUR,
            "USD" => Currency::USD,
            "GBP" => Currency::GBP,
            "CHF" => Currency::CHF,
            "JPY" => Currency::JPY,
            _ => return None,
        })
    }

    fn factor(self) -> i64 {
        10i64.pow(self.exponent())
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Errors from money parsing and arithmetic.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoneyError {
    /// Two amounts of different currencies were combined or ordered.
    #[error("currency mismatch: {left} vs {right}")]
    CurrencyMismatch {
        /// Left operand currency.
        left: Currency,
        /// Right operand currency.
        right: Currency,
    },
    /// The result does not fit in 64-bit minor units.
    #[error("money amount overflow")]
    Overflow,
    /// Division by zero.
    #[error("division by zero")]
    DivisionByZero,
    /// The text is not a money amount like `1200.00 EUR`.
    #[error("invalid money amount {0:?}: expected like \"1200.00 EUR\"")]
    Parse(String),
}

/// An amount of money in integer minor units (cents) of one currency. No floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Money {
    /// Amount in minor units (cents; yen for JPY).
    pub minor: i64,
    /// The currency of the amount.
    pub currency: Currency,
}

impl Money {
    /// Build from minor units and a currency.
    pub fn new(minor: i64, currency: Currency) -> Money {
        Money { minor, currency }
    }

    /// Add two amounts of the same currency; mismatch or overflow is an error.
    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        self.same_currency(other)?;
        let minor = self.minor.checked_add(other.minor).ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }

    /// Subtract two amounts of the same currency; mismatch or overflow is an error.
    pub fn checked_sub(self, other: Money) -> Result<Money, MoneyError> {
        self.same_currency(other)?;
        let minor = self.minor.checked_sub(other.minor).ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }

    /// Multiply by an integer.
    pub fn mul_int(self, n: i64) -> Result<Money, MoneyError> {
        let minor = self.minor.checked_mul(n).ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }

    /// Divide by an integer, rounding half away from zero to the minor unit.
    pub fn div_int(self, n: i64) -> Result<Money, MoneyError> {
        self.mul_div(1, n)
    }

    /// Compute `self * num / den` exactly (128-bit intermediate), rounding half away from zero.
    pub fn mul_div(self, num: i64, den: i64) -> Result<Money, MoneyError> {
        if den == 0 {
            return Err(MoneyError::DivisionByZero);
        }
        let n = self.minor as i128 * num as i128;
        let d = den as i128;
        let mut q = n / d;
        let r = n % d;
        if r.abs() * 2 >= d.abs() {
            q += if (n < 0) != (d < 0) { -1 } else { 1 };
        }
        let minor = i64::try_from(q).map_err(|_| MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }

    /// Order two amounts; comparing different currencies is an error.
    pub fn try_cmp(&self, other: &Money) -> Result<Ordering, MoneyError> {
        self.same_currency(*other)?;
        Ok(self.minor.cmp(&other.minor))
    }

    fn same_currency(self, other: Money) -> Result<(), MoneyError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(MoneyError::CurrencyMismatch { left: self.currency, right: other.currency })
        }
    }
}

impl PartialOrd for Money {
    fn partial_cmp(&self, other: &Money) -> Option<Ordering> {
        self.try_cmp(other).ok()
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.minor < 0 { "-" } else { "" };
        let abs = self.minor.unsigned_abs();
        let exp = self.currency.exponent();
        if exp == 0 {
            write!(f, "{sign}{abs} {}", self.currency)
        } else {
            let factor = self.currency.factor() as u64;
            write!(f, "{sign}{}.{:0width$} {}", abs / factor, abs % factor, self.currency, width = exp as usize)
        }
    }
}

impl FromStr for Money {
    type Err = MoneyError;

    /// Parse `"1200.00 EUR"`, `"0 EUR"`, `"-5.5 USD"` (at most the currency's minor digits).
    fn from_str(s: &str) -> Result<Money, MoneyError> {
        let bad = || MoneyError::Parse(s.to_string());
        let (num, code) = s.split_once(' ').ok_or_else(bad)?;
        let currency = Currency::from_code(code).ok_or_else(bad)?;
        let (neg, num) = match num.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, num),
        };
        let (int_part, frac_part) = match num.split_once('.') {
            Some((i, f)) => (i, f),
            None => (num, ""),
        };
        let digits = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
        let has_dot = num.contains('.');
        if int_part.is_empty()
            || !digits(int_part)
            || !digits(frac_part)
            || (has_dot && frac_part.is_empty())
            || frac_part.len() > currency.exponent() as usize
        {
            return Err(bad());
        }
        let mut frac = frac_part.to_string();
        while frac.len() < currency.exponent() as usize {
            frac.push('0');
        }
        let major: i64 = int_part.parse().map_err(|_| bad())?;
        let frac_n: i64 = if frac.is_empty() { 0 } else { frac.parse().map_err(|_| bad())? };
        let minor = major
            .checked_mul(currency.factor())
            .and_then(|v| v.checked_add(frac_n))
            .ok_or_else(bad)?;
        Ok(Money::new(if neg { -minor } else { minor }, currency))
    }
}

impl Serialize for Money {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Money, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
