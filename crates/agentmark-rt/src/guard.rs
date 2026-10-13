use crate::money::{Currency, Money};
use serde_json::Value;

fn symbol_currency(c: char) -> Option<Currency> {
    match c {
        '€' => Some(Currency::EUR),
        '$' => Some(Currency::USD),
        '£' => Some(Currency::GBP),
        '¥' => Some(Currency::JPY),
        _ => None,
    }
}

/// Code of three upper-case letters at `chars[at..]`, standing alone (no letter/digit around it).
fn code_at(chars: &[char], at: usize) -> Option<Currency> {
    if at + 3 > chars.len() {
        return None;
    }
    if at > 0 && chars[at - 1].is_alphanumeric() {
        return None;
    }
    if chars.get(at + 3).is_some_and(|c| c.is_alphanumeric()) {
        return None;
    }
    Currency::from_code(&chars[at..at + 3].iter().collect::<String>())
}

/// Parse a digit token joined by `,`/`.` exactly; `None` when ambiguous or too precise.
fn parse_token(token: &str, negative: bool, currency: Currency) -> Option<Money> {
    let (int_part, frac) = match (token.contains(','), token.contains('.')) {
        (false, false) => (token.to_string(), ""),
        (false, true) => {
            let (i, f) = token.split_once('.')?;
            if f.contains('.') {
                return None;
            }
            (i.to_string(), f)
        }
        (true, true) => {
            let (i, f) = token.rsplit_once('.')?;
            if f.contains(',') || i.contains('.') {
                return None;
            }
            let mut groups = i.split(',');
            let first = groups.next()?;
            if first.is_empty() || first.len() > 3 || !groups.all(|g| g.len() == 3) {
                return None;
            }
            (i.replace(',', ""), f)
        }
        (true, false) => return None,
    };
    if frac.len() > currency.exponent() as usize || (token.ends_with('.')) {
        return None;
    }
    let s = if frac.is_empty() { format!("{int_part} {currency}") } else { format!("{int_part}.{frac} {currency}") };
    let m: Money = s.parse().ok()?;
    Some(if negative { Money::new(-m.minor, m.currency) } else { m })
}

/// Every amount in the text: `Ok` when parsed exactly, `Err(())` when a currency-marked number is ambiguous.
fn scan(text: &str) -> Vec<Result<Money, ()>> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() || (i > 0 && chars[i - 1].is_ascii_digit()) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() {
            if chars[i].is_ascii_digit() {
                i += 1;
            } else if (chars[i] == ',' || chars[i] == '.') && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
                i += 1;
            } else {
                break;
            }
        }
        let end = i;
        // marker after: optional single space, then code or symbol
        let mut after = None;
        let mut j = end;
        if chars.get(j) == Some(&' ') {
            j += 1;
        }
        if let Some(c) = code_at(&chars, j) {
            after = Some((c, j + 3));
        } else if let Some(c) = chars.get(j).copied().and_then(symbol_currency) {
            after = Some((c, j + 1));
        }
        // marker before: symbol, or code, with optional single space between
        let mut before = None;
        let mut p = start;
        if p > 0 && chars[p - 1] == ' ' {
            p -= 1;
        }
        if p > 0 {
            if let Some(c) = symbol_currency(chars[p - 1]) {
                before = Some((c, p - 1));
            } else if p >= 3 {
                if let Some(c) = code_at(&chars, p - 3) {
                    before = Some((c, p - 3));
                }
            }
        }
        let (currency, marker_start, next) = match (after, before) {
            (Some((c, e)), _) => (c, start, e),
            (None, Some((c, s))) => (c, s, end),
            (None, None) => continue,
        };
        let neg_at = if before.is_some() && after.is_none() { marker_start } else { start };
        let negative = neg_at > 0 && chars[neg_at - 1] == '-' && (neg_at < 2 || !chars[neg_at - 2].is_alphanumeric());
        let token: String = chars[start..end].iter().collect();
        out.push(parse_token(&token, negative, currency).ok_or(()));
        i = next.max(end);
    }
    out
}

/// Extract every money amount (`1200.00 EUR`, `EUR 1,300.00`, `€5`) that parses exactly; ambiguous ones are skipped.
pub fn extract_money(text: &str) -> Vec<Money> {
    scan(text).into_iter().flatten().collect()
}

fn collect_money(v: &Value, out: &mut Vec<Money>) {
    match v {
        Value::String(s) => out.extend(extract_money(s)),
        Value::Array(a) => a.iter().for_each(|x| collect_money(x, out)),
        Value::Object(o) => o.values().for_each(|x| collect_money(x, out)),
        _ => {}
    }
}

/// Guard helper: true only when every currency-marked number in `output_text` parses exactly and also appears in `input_json`.
///
/// Fails closed: an amount that cannot be parsed exactly (ambiguous separators, too many decimals) makes it false.
pub fn no_amounts_changed(output_text: &str, input_json: &Value) -> bool {
    let mut allowed = Vec::new();
    collect_money(input_json, &mut allowed);
    scan(output_text).iter().all(|m| matches!(m, Ok(m) if allowed.contains(m)))
}
