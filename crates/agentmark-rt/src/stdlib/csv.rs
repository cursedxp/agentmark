//! `csv.*` (SPEC 22.3). `csv.parse(t) as list of T`: the first row names the fields of `T`; cells are read
//! as the field's type (`12.50 EUR` money, `2026-10-13` date, whole numbers, `true`/`false`; an empty cell
//! is an empty `maybe`). Surrounding white space in a cell is ignored.

use crate::RtError;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

/// `csv.parse(t) as list of T`.
pub fn parse<T: DeserializeOwned>(t: &str) -> Result<Vec<T>, RtError> {
    let mut reader = ::csv::ReaderBuilder::new().trim(::csv::Trim::All).from_reader(t.as_bytes());
    let mut rows = Vec::new();
    for row in reader.deserialize::<T>() {
        rows.push(row.map_err(|e| RtError::Lib(describe(&e)))?);
    }
    Ok(rows)
}

fn describe(e: &::csv::Error) -> String {
    match e.kind() {
        ::csv::ErrorKind::Deserialize { pos, err } => {
            let line = pos.as_ref().map(|p| p.line()).unwrap_or(0);
            let msg = match err.kind() {
                ::csv::DeserializeErrorKind::Message(m) => m.clone(),
                other => other.to_string(),
            };
            let field = err.field().map(|f| format!(", column {}", f + 1)).unwrap_or_default();
            format!("line {line}{field}: {msg}")
        }
        ::csv::ErrorKind::UnequalLengths { pos, expected_len, len } => {
            let line = pos.as_ref().map(|p| p.line()).unwrap_or(0);
            format!("line {line}: the row has {len} cells, the header has {expected_len}")
        }
        _ => format!("the text is not valid CSV: {e}"),
    }
}

fn cell(v: &Value) -> Result<String, RtError> {
    Ok(match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => return Err(RtError::Lib(format!("a CSV cell holds a single value, not {other}"))),
    })
}

/// `csv.encode(rows)`: a header row with the field names of the first row, then one line per row.
pub fn encode<T: Serialize + ?Sized>(rows: &T) -> Result<String, RtError> {
    let bad = |m: String| RtError::Lib(format!("cannot write this as CSV: {m}"));
    let Value::Array(items) = serde_json::to_value(rows).map_err(|e| bad(e.to_string()))? else {
        return Err(bad("it is not a list".into()));
    };
    let mut w = ::csv::WriterBuilder::new().from_writer(Vec::new());
    let first = match items.first() {
        None => return Ok(String::new()),
        Some(Value::Object(first)) => first,
        Some(_) => return Err(bad("the rows are not records".into())),
    };
    let header: Vec<&String> = first.keys().collect();
    w.write_record(&header).map_err(|e| bad(e.to_string()))?;
    for item in &items {
        let Value::Object(m) = item else { return Err(bad("the rows are not records".into())) };
        let cells = header.iter().map(|k| cell(m.get(*k).unwrap_or(&Value::Null))).collect::<Result<Vec<_>, _>>()?;
        w.write_record(&cells).map_err(|e| bad(e.to_string()))?;
    }
    let bytes = w.into_inner().map_err(|e| bad(e.to_string()))?;
    String::from_utf8(bytes).map_err(|e| bad(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Money;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Sale {
        region: String,
        qty: i64,
        amount: Money,
        shipped: Option<chrono::NaiveDate>,
        rush: bool,
    }

    const TEXT: &str = "region,qty,amount,shipped,rush\nEU, 3 ,120.00 EUR,2026-10-01,true\n\"US, west\",007,\"1,5 EUR\",,false\n";

    #[test]
    fn header_row_names_the_fields() {
        let ok = "region,qty,amount,shipped,rush\nEU,3,120.00 EUR,2026-10-01,true\nUS,7,5.50 EUR,,false\n";
        let rows: Vec<Sale> = parse(ok).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].region, "EU");
        assert_eq!(rows[0].amount.minor, 12000);
        assert_eq!(rows[1].shipped, None);
        assert!(!rows[1].rush);
        // quoting and leading zeros
        let e = parse::<Sale>(TEXT).unwrap_err().to_string();
        assert!(e.starts_with("line 3"), "{e}");
    }

    #[test]
    fn quoted_cells_and_text_numbers() {
        #[derive(serde::Deserialize, Debug)]
        struct Row {
            name: String,
            code: String,
        }
        let rows: Vec<Row> = parse("name,code\n\"Smith, J\",007\n").unwrap();
        assert_eq!(rows[0].name, "Smith, J");
        assert_eq!(rows[0].code, "007");
        assert!(parse::<Row>("").unwrap().is_empty());
    }

    #[test]
    fn errors_name_line_and_column() {
        let e = parse::<Sale>("region,qty,amount,shipped,rush\nEU,x,1.00 EUR,,true\n").unwrap_err().to_string();
        assert!(e.starts_with("line 2, column 2: "), "{e}");
        let e = parse::<Sale>("region,qty\nEU,1\n").unwrap_err().to_string();
        assert!(e.contains("missing field"), "{e}");
        let e = parse::<Sale>("region,qty,amount,shipped,rush\nEU,1\n").unwrap_err().to_string();
        assert!(e.contains("line 2"), "{e}");
    }

    #[test]
    fn encodes_records() {
        let rows = vec![
            Sale { region: "EU".into(), qty: 3, amount: Money::new(12000, crate::Currency::EUR), shipped: None, rush: true },
            Sale { region: "US, west".into(), qty: 1, amount: Money::new(550, crate::Currency::EUR), shipped: None, rush: false },
        ];
        let text = encode(&rows).unwrap();
        assert_eq!(text, "region,qty,amount,shipped,rush\nEU,3,120.00 EUR,,true\n\"US, west\",1,5.50 EUR,,false\n");
        let back: Vec<Sale> = parse(&text).unwrap();
        assert_eq!(back, rows);
        assert_eq!(encode(&Vec::<Sale>::new()).unwrap(), "");
        assert!(encode(&vec![vec![1]]).is_err());
    }
}
