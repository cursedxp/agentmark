//! `json.*` (SPEC 22.3). `json.parse(t) as T` decodes into the record, list or value `T`: field names are
//! the JSON keys; money is text like `"12.50 EUR"`, dates are `"2026-10-13"`.

use crate::RtError;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// `json.parse(t) as T`.
pub fn parse<T: DeserializeOwned>(t: &str) -> Result<T, RtError> {
    serde_json::from_str(t).map_err(|e| RtError::Lib(format!("the text is not the expected JSON: {e}")))
}

/// `json.encode(v)`: compact JSON text.
pub fn encode<T: Serialize + ?Sized>(v: &T) -> Result<String, RtError> {
    serde_json::to_string(v).map_err(|e| RtError::Lib(format!("cannot write this as JSON: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Money;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Note {
        id: i64,
        title: String,
        price: Money,
        due: chrono::NaiveDate,
        tag: Option<String>,
    }

    #[test]
    fn decodes_records_and_lists() {
        let n: Note = parse(r#"{"id":1,"title":"a","price":"12.50 EUR","due":"2026-10-13"}"#).unwrap();
        assert_eq!(n.price.minor, 1250);
        assert_eq!(n.tag, None);
        let list: Vec<Note> = parse(r#"[{"id":1,"title":"a","price":"1 EUR","due":"2026-10-13","tag":"x"}]"#).unwrap();
        assert_eq!(list[0].tag.as_deref(), Some("x"));
        let ids: Vec<i64> = parse("[1, 2]").unwrap();
        assert_eq!(ids, [1, 2]);
    }

    #[test]
    fn failure_has_a_plain_message() {
        let e = parse::<Note>(r#"{"id":"x"}"#).unwrap_err().to_string();
        assert!(e.starts_with("the text is not the expected JSON: "), "{e}");
        assert!(parse::<Note>("nope").is_err());
        let m = parse::<Note>(r#"{"id":1,"title":"a","price":"12","due":"2026-10-13"}"#).unwrap_err().to_string();
        assert!(m.contains("12.50 EUR") || m.contains("invalid money"), "{m}");
    }

    #[test]
    fn encodes() {
        assert_eq!(encode(&vec![1, 2]).unwrap(), "[1,2]");
        assert_eq!(encode("a\"b").unwrap(), "\"a\\\"b\"");
        let n = Note { id: 1, title: "t".into(), price: Money::new(1250, crate::Currency::EUR), due: chrono::NaiveDate::from_ymd_opt(2026, 10, 13).unwrap(), tag: None };
        assert_eq!(encode(&n).unwrap(), r#"{"id":1,"title":"t","price":"12.50 EUR","due":"2026-10-13","tag":null}"#);
    }
}
