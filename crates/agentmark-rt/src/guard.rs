use crate::money::{Currency, Money};
use serde_json::Value;

/// Extract every money amount (`1200.00 EUR`, `-3.5 GBP`) appearing in free text.
pub fn extract_money(text: &str) -> Vec<Money> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() || (i > 0 && (chars[i - 1].is_ascii_digit() || chars[i - 1].is_alphanumeric())) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
            i += 1;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
        }
        let num_end = i;
        if i < chars.len() && chars[i] == ' ' {
            let code: String = chars[i + 1..].iter().take(3).collect();
            let after = chars.get(i + 4);
            if code.chars().count() == 3 && Currency::from_code(&code).is_some() && !after.is_some_and(|c| c.is_alphanumeric()) {
                let negative = start > 0 && chars[start - 1] == '-' && (start < 2 || !chars[start - 2].is_alphanumeric());
                let num: String = chars[start..num_end].iter().collect();
                let s = format!("{}{num} {code}", if negative { "-" } else { "" });
                if let Ok(m) = s.parse::<Money>() {
                    out.push(m);
                }
                i += 4;
                continue;
            }
        }
    }
    out
}

fn collect_money(v: &Value, out: &mut Vec<Money>) {
    match v {
        Value::String(s) => out.extend(extract_money(s)),
        Value::Array(a) => a.iter().for_each(|x| collect_money(x, out)),
        Value::Object(o) => o.values().for_each(|x| collect_money(x, out)),
        _ => {}
    }
}

/// Guard helper: true when every money amount in `output_text` also appears in `input_json`.
pub fn no_amounts_changed(output_text: &str, input_json: &Value) -> bool {
    let mut allowed = Vec::new();
    collect_money(input_json, &mut allowed);
    extract_money(output_text).iter().all(|m| allowed.contains(m))
}
