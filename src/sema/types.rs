//! Types (SPEC 6) and the rules that need only types: assignability and field-type inference
//! (SPEC 2.1).

use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum Type {
    Text,
    Int,
    Money,
    Date,
    DateTime,
    Duration,
    Bool,
    Bytes,
    List(Box<Type>),
    /// `map of K to V` (SPEC 22.2): iteration and printing follow the key order.
    Map(Box<Type>, Box<Type>),
    Maybe(Box<Type>),
    /// `result of T` (SPEC 14).
    Result(Box<Type>),
    /// A declared record, by its (project-unique) name.
    Record(String),
    /// A declared `# Enum`, by its (project-unique) name (SPEC 22.1).
    Enum(String),
    /// What a `match` branch on an enum binds: the fields of one variant, as `(enum, variant)`.
    /// Only its fields can be read (`c.radius`); it is not a value of the enum.
    Variant(String, String),
    /// A record literal `{ id: x }`: field names and types, in written order.
    Object(Vec<(String, Type)>),
    /// `handle of <crate>.<Type>` (SPEC 22.4): a library value AgentMark cannot look into, by its
    /// dotted name (`sha2.Sha256`). It moves, and cannot be printed, compared, serialized or traced.
    Handle(String),
    /// The type of something that already produced an error; compatible with everything so one
    /// mistake is reported once.
    Unknown,
}

/// The built-in type names, in the order used for did-you-mean.
pub const BUILTIN_TYPES: [&str; 8] = [
    "text", "int", "money", "date", "datetime", "duration", "bool", "bytes",
];

impl Type {
    pub fn builtin(name: &str) -> Option<Type> {
        Some(match name {
            "text" => Type::Text,
            "int" => Type::Int,
            "money" => Type::Money,
            "date" => Type::Date,
            "datetime" => Type::DateTime,
            "duration" => Type::Duration,
            "bool" => Type::Bool,
            "bytes" => Type::Bytes,
            _ => return None,
        })
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, Type::Unknown)
    }

    pub fn maybe(self) -> Type {
        Type::Maybe(Box::new(self))
    }

    pub fn list(self) -> Type {
        Type::List(Box::new(self))
    }

    /// `T` for `maybe T`, the type itself otherwise.
    pub fn unwrap_maybe(&self) -> &Type {
        match self {
            Type::Maybe(t) => t,
            t => t,
        }
    }

    pub fn is_handle(&self) -> bool {
        matches!(self, Type::Handle(_))
    }

    pub fn is_maybe(&self) -> bool {
        matches!(self, Type::Maybe(_))
    }

    /// "money", "a duration", "a list of Invoice": how a type reads inside a sentence.
    pub fn with_article(&self) -> String {
        match self {
            Type::Money => "money".into(),
            Type::Text => "text".into(),
            Type::Int => "a number".into(),
            Type::Duration => "a duration".into(),
            Type::Date => "a date".into(),
            Type::DateTime => "a datetime".into(),
            Type::Bool => "a yes/no value".into(),
            t => t.to_string(),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Text => f.write_str("text"),
            Type::Int => f.write_str("int"),
            Type::Money => f.write_str("money"),
            Type::Date => f.write_str("date"),
            Type::DateTime => f.write_str("datetime"),
            Type::Duration => f.write_str("duration"),
            Type::Bool => f.write_str("bool"),
            Type::Bytes => f.write_str("bytes"),
            Type::List(t) => write!(f, "list of {t}"),
            Type::Map(k, v) => write!(f, "map of {k} to {v}"),
            Type::Maybe(t) => write!(f, "maybe {t}"),
            Type::Result(t) => write!(f, "result of {t}"),
            Type::Record(n) | Type::Enum(n) => f.write_str(n),
            Type::Variant(e, v) => write!(f, "{e}.{v}"),
            Type::Handle(n) => write!(f, "handle of {n}"),
            Type::Object(fields) => {
                let inner: Vec<String> = fields.iter().map(|(n, t)| format!("{n}: {t}")).collect();
                write!(f, "{{ {} }}", inner.join(", "))
            }
            Type::Unknown => f.write_str("?"),
        }
    }
}

/// "What are the fields of record R" (name and type, in order), if R exists.
pub type FieldsOf<'a> = dyn Fn(&str) -> Option<Vec<(String, Type)>> + 'a;

/// Can a value of type `from` be stored where `to` is expected?
/// `field_of` answers "what are the fields of record R" for record literals.
pub fn assignable(
    from: &Type,
    to: &Type,
    field_of: &FieldsOf,
) -> bool {
    if from.is_unknown() || to.is_unknown() || from == to {
        return true;
    }
    match (from, to) {
        // a plain value can always be stored in a `maybe` slot
        (f, Type::Maybe(t)) if !f.is_maybe() => assignable(f, t, field_of),
        (Type::Maybe(f), Type::Maybe(t)) => assignable(f, t, field_of),
        (Type::List(f), Type::List(t)) => assignable(f, t, field_of),
        (Type::Map(fk, fv), Type::Map(tk, tv)) => {
            assignable(fk, tk, field_of) && assignable(fv, tv, field_of)
        }
        (Type::Object(fields), Type::Record(r)) => match field_of(r) {
            // every key must exist with a fitting type, and every field that is not `maybe`
            // must be given
            Some(rec) => {
                fields.iter().all(|(n, t)| {
                    rec.iter()
                        .find(|(rn, _)| rn == n)
                        .is_some_and(|(_, rt)| assignable(t, rt, field_of))
                }) && rec
                    .iter()
                    .all(|(rn, rt)| rt.is_maybe() || fields.iter().any(|(n, _)| n == rn))
            }
            None => true,
        },
        (Type::Object(a), Type::Object(b)) => b.iter().all(|(n, t)| {
            a.iter()
                .find(|(an, _)| an == n)
                .is_some_and(|(_, at)| assignable(at, t, field_of))
        }),
        (Type::Result(f), Type::Result(t)) => assignable(f, t, field_of),
        _ => false,
    }
}

/// SPEC 2.1: the type of a field written without one. Returns the type and whether a name rule
/// matched (`false` = the fallback `text`, which is a warning).
///
/// The rules say "names ending in `date`, `at`, `on`"; read letter by letter, `reason` or `format`
/// would become dates. OPEN_QUESTIONS Q32: the suffix must be a whole word of the name
/// (`due_date`, `created_at`, `paid_on`, `date`), words being separated by `_` or `-`.
pub fn infer_field_type(name: &str) -> (Type, bool) {
    let last = last_word(name);
    match last.as_str() {
        "date" | "at" | "on" => (Type::Date, true),
        "amount" | "price" | "total" | "fee" => (Type::Money, true),
        "count" | "qty" | "number" => (Type::Int, true),
        _ => (Type::Text, false),
    }
}

/// Last word of a name, words split at `_`, `-` and lower-to-upper case changes
/// (`createdAt` -> `at`, `due_date` -> `date`).
fn last_word(name: &str) -> String {
    let mut word = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if c == '_' || c == '-' {
            word.clear();
            prev_lower = false;
        } else if c.is_uppercase() && prev_lower {
            word.clear();
            word.extend(c.to_lowercase());
            prev_lower = false;
        } else {
            word.extend(c.to_lowercase());
            prev_lower = c.is_lowercase() || c.is_ascii_digit();
        }
    }
    word
}

/// Why an object literal does not fit a record: the missing fields and misspelled keys, as a
/// sentence to append to a message (empty when nothing specific can be said).
pub fn object_note(from: &Type, to: &Type, fields_of: &FieldsOf) -> String {
    let (Type::Object(given), Type::Record(r)) = (from, to) else {
        return String::new();
    };
    let Some(rec) = fields_of(r) else {
        return String::new();
    };
    let mut notes = Vec::new();
    let unknown: Vec<&str> = given
        .iter()
        .map(|(n, _)| n.as_str())
        .filter(|n| !rec.iter().any(|(rn, _)| rn == n))
        .collect();
    for u in unknown {
        let hit = crate::diag::did_you_mean(u, rec.iter().map(|(n, _)| n.as_str()));
        notes.push(match hit {
            Some(h) => format!("{r} has no field \"{u}\". Did you mean \"{h}\"?"),
            None => format!("{r} has no field \"{u}\"."),
        });
    }
    let missing: Vec<&str> = rec
        .iter()
        .filter(|(rn, rt)| !rt.is_maybe() && !given.iter().any(|(n, _)| n == rn))
        .map(|(n, _)| n.as_str())
        .collect();
    if !missing.is_empty() {
        notes.push(format!("Missing: {}.", missing.join(", ")));
    }
    notes.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_records(_: &str) -> Option<Vec<(String, Type)>> {
        None
    }

    #[test]
    fn display_reads_like_the_source() {
        assert_eq!(Type::Int.maybe().to_string(), "maybe int");
        assert_eq!(
            Type::Record("Invoice".into()).list().to_string(),
            "list of Invoice"
        );
    }

    #[test]
    fn inference_follows_the_spec_by_whole_words() {
        assert_eq!(infer_field_type("due_date"), (Type::Date, true));
        assert_eq!(infer_field_type("created_at"), (Type::Date, true));
        assert_eq!(infer_field_type("paid_on"), (Type::Date, true));
        assert_eq!(infer_field_type("amount"), (Type::Money, true));
        assert_eq!(infer_field_type("unit_price"), (Type::Money, true));
        assert_eq!(infer_field_type("retry_count"), (Type::Int, true));
        assert_eq!(infer_field_type("qty"), (Type::Int, true));
        assert_eq!(infer_field_type("customer"), (Type::Text, false));
        assert_eq!(infer_field_type("createdAt"), (Type::Date, true));
        assert_eq!(infer_field_type("totalAmount"), (Type::Money, true));
        assert_eq!(infer_field_type("retryCount"), (Type::Int, true));
        assert_eq!(infer_field_type("dueDate"), (Type::Date, true));
        assert_eq!(infer_field_type("Date"), (Type::Date, true));
        // letters alone are not enough
        assert_eq!(infer_field_type("reason"), (Type::Text, false));
        assert_eq!(infer_field_type("format"), (Type::Text, false));
    }

    #[test]
    fn assignability() {
        let a = |f: &Type, t: &Type| assignable(f, t, &no_records);
        assert!(a(&Type::Int, &Type::Int));
        assert!(a(&Type::Int, &Type::Int.maybe()));
        assert!(!a(&Type::Int.maybe(), &Type::Int));
        assert!(!a(&Type::Int, &Type::Money));
        assert!(a(&Type::Unknown, &Type::Money));
        assert!(a(&Type::Text.list(), &Type::Text.list()));
        assert!(!a(&Type::Text.list(), &Type::Int.list()));
    }

    #[test]
    fn record_literals_must_give_every_field_that_is_not_maybe() {
        let rec = |n: &str| {
            (n == "R").then(|| vec![("a".to_string(), Type::Text), ("b".to_string(), Type::Int.maybe())])
        };
        let only_a = Type::Object(vec![("a".into(), Type::Text)]);
        assert!(assignable(&only_a, &Type::Record("R".into()), &rec));
        let only_b = Type::Object(vec![("b".into(), Type::Int)]);
        assert!(!assignable(&only_b, &Type::Record("R".into()), &rec));
    }

    #[test]
    fn record_literals_fit_records_with_matching_fields() {
        let rec = |n: &str| {
            (n == "Invoice")
                .then(|| vec![("id".to_string(), Type::Text), ("n".to_string(), Type::Int.maybe())])
        };
        let obj = Type::Object(vec![("id".into(), Type::Text)]);
        assert!(assignable(&obj, &Type::Record("Invoice".into()), &rec));
        let bad = Type::Object(vec![("id".into(), Type::Int)]);
        assert!(!assignable(&bad, &Type::Record("Invoice".into()), &rec));
        let extra = Type::Object(vec![("nope".into(), Type::Text)]);
        assert!(!assignable(&extra, &Type::Record("Invoice".into()), &rec));
    }
}
