use crate::error::RtError;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// Parse a model answer as JSON into `T`: tolerates a ```json fence or prose around one object/array.
pub fn parse_model_json<T: DeserializeOwned>(text: &str) -> Result<T, RtError> {
    let body = json_slice(text);
    serde_json::from_str(body).map_err(|e| RtError::Model(format!("the model's answer does not fit the expected record: {e}")))
}

fn json_slice(text: &str) -> &str {
    let t = text.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    let t = t.trim().strip_suffix("```").unwrap_or(t).trim();
    if t.starts_with('{') || t.starts_with('[') {
        return t;
    }
    let open = t.find(['{', '[']);
    let close = t.rfind(['}', ']']);
    match (open, close) {
        (Some(a), Some(b)) if b > a => &t[a..=b],
        _ => t,
    }
}

/// Read structured data from a file: `.csv` (header row = field names) or JSON (anything else).
/// A CSV row becomes an object; cells are text first and numbers/booleans only when the target
/// type does not accept text there.
pub fn read_data<T: DeserializeOwned>(path: &str) -> Result<T, RtError> {
    let err = |m: String| RtError::Fixture { path: path.to_string(), message: m };
    let text = std::fs::read_to_string(path).map_err(|e| err(e.to_string()))?;
    if !path.to_ascii_lowercase().ends_with(".csv") {
        return serde_json::from_str(&text).map_err(|e| err(e.to_string()));
    }
    let rows = parse_csv(&text);
    let Some((header, body)) = rows.split_first() else {
        return serde_json::from_value(Value::Array(vec![])).map_err(|e| err(e.to_string()));
    };
    let objects = |coerce: bool| -> Value {
        Value::Array(
            body.iter()
                .map(|row| {
                    let mut m = Map::new();
                    for (k, v) in header.iter().zip(row) {
                        m.insert(k.trim().to_string(), if coerce { coerce_cell(v) } else { Value::String(v.clone()) });
                    }
                    Value::Object(m)
                })
                .collect(),
        )
    };
    match serde_json::from_value::<T>(objects(false)) {
        Ok(v) => Ok(v),
        Err(_) => serde_json::from_value::<T>(objects(true)).map_err(|e| err(e.to_string())),
    }
}

fn coerce_cell(v: &str) -> Value {
    let t = v.trim();
    if let Ok(n) = t.parse::<i64>() {
        return Value::from(n);
    }
    match t {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::String(v.to_string()),
    }
}

/// Minimal CSV: commas, double-quoted cells with `""` escapes, `\n` or `\r\n` line ends; blank lines are skipped.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let finish_row = |row: &mut Vec<String>, cell: &mut String, rows: &mut Vec<Vec<String>>| {
        row.push(std::mem::take(cell));
        let blank = row.len() == 1 && row[0].trim().is_empty();
        if !blank {
            rows.push(std::mem::take(row));
        } else {
            row.clear();
        }
    };
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                chars.next();
                cell.push('"');
            }
            ('"', _) => quoted = !quoted,
            (',', false) => row.push(std::mem::take(&mut cell)),
            ('\r', false) => {}
            ('\n', false) => finish_row(&mut row, &mut cell, &mut rows),
            (c, _) => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        finish_row(&mut row, &mut cell, &mut rows);
    }
    rows
}
