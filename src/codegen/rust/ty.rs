//! IR types -> Rust types, and the defaults codegen needs.

use super::names::{value_ident, Names};
use crate::ir::Type;

/// The Rust spelling of a type. `result of T` is `T` here: every generated function returns
/// `Result<_, AgentError>`, so the "result" is the function's own.
pub fn rust_type(t: &Type, n: &Names) -> String {
    match t {
        Type::Text => "String".into(),
        Type::Int => "i64".into(),
        Type::Money => "Money".into(),
        Type::Date => "NaiveDate".into(),
        Type::DateTime => "DateTime<Utc>".into(),
        Type::Duration => "Duration".into(),
        Type::Bool => "bool".into(),
        Type::Bytes => "Vec<u8>".into(),
        Type::List(i) => format!("Vec<{}>", rust_type(i, n)),
        Type::Map(k, v) => format!("std::collections::BTreeMap<{}, {}>", rust_type(k, n), rust_type(v, n)),
        Type::Maybe(i) => format!("Option<{}>", rust_type(i, n)),
        Type::Result(i) => rust_type(i, n),
        Type::Record(r) => n.record(r),
        Type::Handle(h) => n.handle_rust.get(h).cloned().unwrap_or_else(|| handle_path(h)),
        Type::Object(_) | Type::Unknown => "serde_json::Value".into(),
    }
}

/// The Rust path of a handle type: `sha2.Sha256` -> `sha2::Sha256`, `my-crate.a.B` -> `my_crate::a::B`.
pub fn handle_path(h: &str) -> String {
    let mut parts = h.split('.');
    let krate = parts.next().unwrap_or("").replace('-', "_");
    std::iter::once(krate).chain(parts.map(String::from)).collect::<Vec<_>>().join("::")
}

/// Is the Rust value `Copy`? (Then a place expression is used by value without `.clone()`.)
pub fn is_copy(t: &Type) -> bool {
    match t {
        Type::Int | Type::Money | Type::Date | Type::DateTime | Type::Duration | Type::Bool => true,
        Type::Maybe(i) => is_copy(i),
        Type::Result(i) => is_copy(i),
        _ => false,
    }
}

/// A value of the type for "nothing happened": what a failed `try` leaves behind (Q94).
pub fn default_expr(t: &Type, n: &Names) -> String {
    match t {
        Type::Text => "String::new()".into(),
        Type::Int => "0".into(),
        Type::Money => "Money::new(0, Currency::EUR)".into(),
        Type::Date => "NaiveDate::default()".into(),
        Type::DateTime => "DateTime::<Utc>::default()".into(),
        Type::Duration => "Duration::zero()".into(),
        Type::Bool => "false".into(),
        Type::Bytes | Type::List(_) => "Vec::new()".into(),
        Type::Map(..) => "std::collections::BTreeMap::new()".into(),
        Type::Maybe(_) => "None".into(),
        Type::Result(i) => default_expr(i, n),
        Type::Record(r) => format!("{}::empty()", n.record(r)),
        // sema stops every path that would need one (E366)
        Type::Handle(_) => "unreachable!(\"a handle has no default\")".into(),
        Type::Object(_) | Type::Unknown => "serde_json::Value::Null".into(),
    }
}

/// The JSON spelling of a type for the answer-format hint of `ask model … as Record`.
pub fn schema_word(t: &Type) -> String {
    match t {
        Type::Text => "text".into(),
        Type::Int => "int".into(),
        Type::Money => "money like \\\"12.50 EUR\\\"".into(),
        Type::Date => "date like \\\"2026-10-13\\\"".into(),
        Type::DateTime => "datetime".into(),
        Type::Duration => "duration".into(),
        Type::Bool => "bool".into(),
        Type::Bytes => "bytes".into(),
        Type::List(i) => format!("list of {}", schema_word(i)),
        Type::Map(k, v) => format!("object from {} to {}", schema_word(k), schema_word(v)),
        Type::Maybe(i) => format!("{} or null", schema_word(i)),
        Type::Result(i) => schema_word(i),
        Type::Record(r) => r.clone(),
        Type::Handle(h) => format!("handle of {h}"),
        Type::Object(_) | Type::Unknown => "any".into(),
    }
}

/// Struct field identifier and, when the JSON name differs, the serde rename.
pub fn field_ident(name: &str) -> (String, Option<String>) {
    let id = value_ident(name);
    if id == name {
        (id, None)
    } else {
        (id, Some(name.to_string()))
    }
}
