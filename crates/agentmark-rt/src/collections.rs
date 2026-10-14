//! Helpers of the collection words (SPEC 22.2): ordering, money sums, `unique`.
//!
//! Generated programs call these by full path, `agentmark_rt::collections::sorted(…)`.

use std::cmp::Ordering;

use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::error::RtError;
use crate::money::{Currency, Money};

/// A value `sort`, `min` and `max` can put in order.
///
/// Only money can fail: two amounts in different currencies have no order, and the collection
/// words report that as an error instead of guessing.
pub trait SortKey {
    /// The order of two keys. Only meaningful after [`SortKey::comparable`] accepted them.
    fn compare(&self, other: &Self) -> Ordering;

    /// Can `self` be ordered against `other`?
    fn comparable(&self, _other: &Self) -> Result<(), RtError> {
        Ok(())
    }
}

macro_rules! ord_key {
    ($($t:ty),*) => {$(
        impl SortKey for $t {
            fn compare(&self, other: &Self) -> Ordering {
                self.cmp(other)
            }
        }
    )*};
}

ord_key!(i64, String, bool, NaiveDate, DateTime<Utc>, Duration);

impl SortKey for Money {
    fn compare(&self, other: &Self) -> Ordering {
        self.minor.cmp(&other.minor)
    }

    fn comparable(&self, other: &Self) -> Result<(), RtError> {
        self.try_cmp(other).map(|_| ()).map_err(RtError::from)
    }
}

/// Put `(key, value)` pairs in the order of their keys and return the values. The sort is stable:
/// equal keys keep their original order, also when `descending`.
pub fn sorted<K: SortKey, T>(mut keyed: Vec<(K, T)>, descending: bool) -> Result<Vec<T>, RtError> {
    if let Some((first, _)) = keyed.first() {
        for (k, _) in &keyed {
            first.comparable(k)?;
        }
    }
    keyed.sort_by(|a, b| {
        let o = a.0.compare(&b.0);
        if descending {
            o.reverse()
        } else {
            o
        }
    });
    Ok(keyed.into_iter().map(|(_, v)| v).collect())
}

/// The smallest (or largest) value; the first of equal ones. `None` for an empty list.
pub fn extreme<K: SortKey>(values: Vec<K>, max: bool) -> Result<Option<K>, RtError> {
    let mut best: Option<K> = None;
    for v in values {
        best = match best {
            None => Some(v),
            Some(b) => {
                b.comparable(&v)?;
                let o = v.compare(&b);
                if (max && o == Ordering::Greater) || (!max && o == Ordering::Less) {
                    Some(v)
                } else {
                    Some(b)
                }
            }
        };
    }
    Ok(best)
}

/// The sum of amounts of one currency. Mixed currencies are an error; an empty list is `0 EUR`
/// (an empty list has no currency, so the default one is used).
pub fn sum_money(values: Vec<Money>) -> Result<Money, RtError> {
    let mut total: Option<Money> = None;
    for v in values {
        total = Some(match total {
            None => v,
            Some(t) => t.checked_add(v)?,
        });
    }
    Ok(total.unwrap_or(Money::new(0, Currency::EUR)))
}

/// Every value once, in the order of its first occurrence. Values are compared by their JSON
/// form (every AgentMark value has one), so the cost is linear, not quadratic; a value that
/// cannot be serialised is compared with `==` against the ones kept so far.
pub fn unique<T: PartialEq + serde::Serialize>(values: Vec<T>) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<T> = Vec::with_capacity(values.len());
    for v in values {
        match serde_json::to_string(&v) {
            Ok(key) => {
                if seen.insert(key) {
                    out.push(v);
                }
            }
            Err(_) => {
                if !out.contains(&v) {
                    out.push(v);
                }
            }
        }
    }
    out
}
