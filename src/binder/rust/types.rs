//! Rustdoc JSON types -> a small Rust type model (`RT`), its rendering, the lift onto AgentMark
//! types, and the 1:1 conversions used by the direct tier.

use crate::binder::core::Ty;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum RT {
    Prim(String),
    Slice(Box<RT>),
    Array(Box<RT>),
    Tuple(Vec<RT>),
    Ref { inner: Box<RT>, mutable: bool },
    Ptr,
    Path { full: Vec<String>, args: Vec<RT> },
    Generic(String),
    Impl(Vec<Bound>),
    Dyn(Vec<Bound>),
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    pub name: String,
    pub args: Vec<RT>,
    /// `Output = T` of `Future<Output = T>`.
    pub output: Option<Box<RT>>,
    pub marker: bool,
}

/// Things that put an item out of reach of the first two tiers (SPEC 15.2 "skipped" row).
#[derive(Debug, Default, Clone)]
pub struct Hazards {
    pub lifetimes: bool,
    pub assoc: bool,
    pub closure: bool,
}

const MARKERS: &[&str] = &[
    "Sized", "Send", "Sync", "Unpin", "Copy", "Clone", "Debug", "UnwindSafe", "RefUnwindSafe",
];
const INTS: &[&str] = &["i8", "i16", "i32", "i64", "isize", "u8", "u16", "u32", "u64", "usize"];
/// Integers that always fit an AgentMark `int` (i64): the direct tier allows only these.
const SMALL_INTS: &[&str] = &["i8", "i16", "i32", "i64", "u8", "u16", "u32"];

pub struct TypeParser<'a> {
    pub doc: &'a Value,
    pub hz: Hazards,
    pub self_ty: Option<RT>,
}

impl<'a> TypeParser<'a> {
    pub fn new(doc: &'a Value, self_ty: Option<RT>) -> Self {
        TypeParser {
            doc,
            hz: Hazards::default(),
            self_ty,
        }
    }

    fn full_path(&self, id: Option<u64>, fallback: &str) -> Vec<String> {
        if let Some(id) = id {
            if let Some(p) = self.doc["paths"][id.to_string()]["path"].as_array() {
                let v: Vec<String> = p.iter().filter_map(|s| s.as_str().map(String::from)).collect();
                if !v.is_empty() {
                    return v;
                }
            }
        }
        fallback.split("::").map(String::from).collect()
    }

    pub fn parse(&mut self, v: &Value) -> RT {
        let Some((kind, body)) = v.as_object().and_then(|o| o.iter().next()) else {
            return RT::Other("?".into());
        };
        match kind.as_str() {
            "primitive" => RT::Prim(body.as_str().unwrap_or("?").to_string()),
            "generic" => {
                let n = body.as_str().unwrap_or("?");
                if n == "Self" {
                    if let Some(s) = &self.self_ty {
                        return s.clone();
                    }
                }
                RT::Generic(n.to_string())
            }
            "resolved_path" => {
                let id = body["id"].as_u64();
                let full = self.full_path(id, body["path"].as_str().unwrap_or("?"));
                let args = self.parse_args(&body["args"]);
                RT::Path { full, args }
            }
            "borrowed_ref" => {
                if !body["lifetime"].is_null() {
                    self.hz.lifetimes = true;
                }
                RT::Ref {
                    inner: Box::new(self.parse(&body["type"])),
                    mutable: body["is_mutable"].as_bool().unwrap_or(false),
                }
            }
            "slice" => RT::Slice(Box::new(self.parse(body))),
            "array" => RT::Array(Box::new(self.parse(&body["type"]))),
            "tuple" => RT::Tuple(
                body.as_array()
                    .map(|a| a.iter().map(|t| self.parse(t)).collect())
                    .unwrap_or_default(),
            ),
            "raw_pointer" => RT::Ptr,
            "impl_trait" => RT::Impl(self.parse_bounds(body)),
            "dyn_trait" => {
                let bounds: Vec<Value> = body["traits"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|t| serde_json::json!({ "trait_bound": { "trait": t["trait"], "modifier": "none" } }))
                            .collect()
                    })
                    .unwrap_or_default();
                RT::Dyn(self.parse_bounds(&Value::Array(bounds)))
            }
            "qualified_path" => {
                self.hz.assoc = true;
                RT::Other("<qualified path>".into())
            }
            "function_pointer" => {
                self.hz.closure = true;
                RT::Other("fn pointer".into())
            }
            _ => RT::Other("_".into()),
        }
    }

    fn parse_args(&mut self, args: &Value) -> Vec<RT> {
        let mut out = Vec::new();
        if let Some(ab) = args.get("angle_bracketed") {
            for a in ab["args"].as_array().into_iter().flatten() {
                if a.get("lifetime").is_some() {
                    self.hz.lifetimes = true;
                } else if let Some(t) = a.get("type") {
                    out.push(self.parse(t));
                }
            }
            if ab["constraints"].as_array().is_some_and(|c| !c.is_empty()) {
                self.hz.assoc = true;
            }
        } else if args.get("parenthesized").is_some() {
            self.hz.closure = true;
        }
        out
    }

    pub fn parse_bounds(&mut self, bounds: &Value) -> Vec<Bound> {
        let mut out = Vec::new();
        for b in bounds.as_array().into_iter().flatten() {
            let Some(tb) = b.get("trait_bound") else { continue };
            let t = &tb["trait"];
            let name = t["path"]
                .as_str()
                .unwrap_or("?")
                .rsplit("::")
                .next()
                .unwrap_or("?")
                .to_string();
            let mut output = None;
            if let Some(ab) = t["args"].get("angle_bracketed") {
                for c in ab["constraints"].as_array().into_iter().flatten() {
                    if c["name"] == "Output" {
                        if let Some(ty) = c["binding"]["equality"].get("type") {
                            output = Some(Box::new(self.parse(ty)));
                        }
                    }
                }
            }
            let mut tmp = t["args"].clone();
            // Constraints on a bound (`Future<Output = T>`) are handled above, not hazards.
            if let Some(ab) = tmp.get_mut("angle_bracketed") {
                ab["constraints"] = Value::Array(vec![]);
            }
            let args = self.parse_args(&tmp);
            if matches!(name.as_str(), "Fn" | "FnMut" | "FnOnce") {
                self.hz.closure = true;
            }
            let marker = MARKERS.contains(&name.as_str()) || tb["modifier"] == "maybe";
            out.push(Bound {
                name,
                args,
                output,
                marker,
            });
        }
        out
    }
}

impl RT {
    /// A path into `std`/`core`/`alloc`.
    pub fn is_std_path(&self) -> bool {
        matches!(self, RT::Path { full, .. } if is_std(full))
    }
}

pub fn last(full: &[String]) -> &str {
    full.last().map(String::as_str).unwrap_or("")
}

fn is_std(full: &[String]) -> bool {
    matches!(full.first().map(String::as_str), Some("alloc" | "core" | "std"))
}

fn is_chrono(full: &[String]) -> bool {
    full.first().map(String::as_str) == Some("chrono")
}

fn is_u8(t: &RT) -> bool {
    matches!(t, RT::Prim(p) if p == "u8")
}

fn std_path(name: &str, args: Vec<RT>) -> RT {
    RT::Path {
        full: vec!["alloc".into(), name.into()],
        args,
    }
}

/// Rust-looking text for a type; foreign paths are kept fully qualified so effect heuristics
/// can see `std::net::...`.
pub fn render(t: &RT) -> String {
    match t {
        RT::Prim(p) => p.clone(),
        RT::Slice(i) => format!("[{}]", render(i)),
        RT::Array(i) => format!("[{}; _]", render(i)),
        RT::Tuple(v) => format!("({})", v.iter().map(render).collect::<Vec<_>>().join(", ")),
        RT::Ref { inner, mutable } => format!("&{}{}", if *mutable { "mut " } else { "" }, render(inner)),
        RT::Ptr => "*const _".into(),
        RT::Path { full, args } => {
            let name = if is_std(full) { last(full).to_string() } else { full.join("::") };
            if args.is_empty() {
                name
            } else {
                format!("{name}<{}>", args.iter().map(render).collect::<Vec<_>>().join(", "))
            }
        }
        RT::Generic(n) => n.clone(),
        RT::Impl(b) => format!("impl {}", render_bounds(b)),
        RT::Dyn(b) => format!("dyn {}", render_bounds(b)),
        RT::Other(s) => s.clone(),
    }
}

pub fn render_bounds(b: &[Bound]) -> String {
    b.iter()
        .map(|b| {
            let mut s = b.name.clone();
            let mut inner: Vec<String> = b.args.iter().map(render).collect();
            if let Some(o) = &b.output {
                inner.push(format!("Output = {}", render(o)));
            }
            if !inner.is_empty() {
                s.push_str(&format!("<{}>", inner.join(", ")));
            }
            s
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

// ---------------------------------------------------------------------------- lift

pub struct Lifter<'a> {
    /// Generic parameter name -> its bounds (fn and impl level).
    pub bounds: &'a [(String, Vec<Bound>)],
}

fn utc() -> RT {
    RT::Path {
        full: vec!["chrono".into(), "Utc".into()],
        args: vec![],
    }
}

impl Lifter<'_> {
    /// The single obvious instantiation of a set of bounds (SPEC 15.2 assisted row).
    fn instantiate(&self, bounds: &[Bound]) -> Result<RT, String> {
        let mut found: Option<RT> = None;
        for b in bounds.iter().filter(|b| !b.marker) {
            let one = match (b.name.as_str(), b.args.first()) {
                ("AsRef" | "Borrow" | "Into" | "From", Some(a)) => match a {
                    RT::Prim(p) if p == "str" => std_path("String", vec![]),
                    RT::Slice(i) if is_u8(i) => std_path("Vec", vec![RT::Prim("u8".into())]),
                    RT::Path { full, args } if last(full) == "String" => std_path("String", args.clone()),
                    RT::Path { full, .. } if matches!(last(full), "Path" | "PathBuf" | "OsStr") => {
                        std_path("String", vec![])
                    }
                    RT::Path { full, args } if last(full) == "Vec" && args.first().is_some_and(is_u8) => {
                        std_path("Vec", args.clone())
                    }
                    _ => return Err(format!("bound `{}` has no single obvious instantiation", render_bounds(&[b.clone()]))),
                },
                ("ToString" | "Display" | "IntoUrl", _) => std_path("String", vec![]),
                ("TimeZone", _) => utc(),
                _ => return Err(format!("bound `{}` has no single obvious instantiation", render_bounds(&[b.clone()]))),
            };
            match &found {
                Some(f) if *f != one => {
                    return Err("bounds disagree on one obvious instantiation".into());
                }
                _ => found = Some(one),
            }
        }
        found.ok_or_else(|| "unbounded generic has no single obvious instantiation".into())
    }

    fn generic(&self, name: &str) -> Result<RT, String> {
        let bounds = self
            .bounds
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, b)| b.as_slice())
            .unwrap_or(&[]);
        self.instantiate(bounds)
            .map_err(|e| format!("generic `{name}`: {e}"))
    }

    /// Replace generic parameters that have one obvious instantiation (best effort).
    pub fn resolve(&self, rt: &RT) -> RT {
        match rt {
            RT::Generic(g) => self.generic(g).unwrap_or_else(|_| rt.clone()),
            RT::Path { full, args } => RT::Path {
                full: full.clone(),
                args: args.iter().map(|a| self.resolve(a)).collect(),
            },
            _ => rt.clone(),
        }
    }

    pub fn value(&self, rt: &RT) -> Result<Ty, String> {
        let no = || Err(format!("`{}` has no AgentMark type", render(rt)));
        match rt {
            RT::Prim(p) if p == "bool" => Ok(Ty::Bool),
            RT::Prim(p) if p == "str" => Ok(Ty::Text),
            RT::Prim(p) if INTS.contains(&p.as_str()) => Ok(Ty::Int),
            RT::Prim(p) if p == "f32" || p == "f64" => {
                Err("floats have no AgentMark type (money must be integer minor units)".into())
            }
            RT::Ref { mutable: true, .. } => Err(format!("`{}` is a mutable borrow", render(rt))),
            RT::Ref { inner, .. } => self.value(inner),
            RT::Slice(i) if is_u8(i) => Ok(Ty::Bytes),
            RT::Slice(i) => Ok(Ty::List(Box::new(self.value(i)?))),
            RT::Path { full, args } => {
                let args: Vec<RT> = args.iter().map(|a| self.resolve(a)).collect();
                let args = args.as_slice();
                let n = last(full);
                if is_std(full) {
                    match (n, args) {
                        ("String", _) => return Ok(Ty::Text),
                        ("Vec", [a]) if is_u8(a) => return Ok(Ty::Bytes),
                        ("Vec", [a]) => return Ok(Ty::List(Box::new(self.value(a)?))),
                        ("Option", [a]) => return Ok(Ty::Maybe(Box::new(self.value(a)?))),
                        ("Box", [a]) => return self.value(a),
                        ("Cow", [a]) => return self.value(a),
                        _ => {}
                    }
                }
                if is_chrono(full) {
                    match (n, args) {
                        ("NaiveDate", _) => return Ok(Ty::Date),
                        ("DateTime", [RT::Path { full: z, .. }]) if last(z) == "Utc" => return Ok(Ty::DateTime),
                        ("DateTime", _) => return Err(format!("`{}` needs one obvious time zone", render(rt))),
                        ("Duration" | "TimeDelta", _) => return Ok(Ty::Duration),
                        _ => {}
                    }
                }
                match n {
                    "Url" | "Uri" | "PathBuf" | "Path" => Ok(Ty::Text),
                    "GenericArray" | "Output" => Ok(Ty::Bytes),
                    _ => no(),
                }
            }
            RT::Generic(g) => self.value(&self.generic(g)?),
            RT::Impl(b) => self.value(&self.instantiate(b)?),
            _ => no(),
        }
    }

    /// Is this return type "Result"-shaped (a std `Result` or an alias such as `ParseResult<T>`)?
    pub fn is_result(rt: &RT) -> bool {
        match rt {
            RT::Path { full, args } => {
                let n = last(full);
                !args.is_empty() && (n == "Result" || n.ends_with("Result"))
            }
            _ => false,
        }
    }

    /// Lift a return type; `None` means "returns nothing".
    pub fn ret(&self, rt: &RT) -> Result<Option<Ty>, String> {
        match rt {
            RT::Tuple(v) if v.is_empty() => Ok(None),
            RT::Path { args, .. } if Self::is_result(rt) => self.ret(&args[0]),
            RT::Impl(b) => match b.iter().find(|b| b.name == "Future") {
                Some(Bound { output: Some(o), .. }) => self.ret(o),
                _ => Err(format!("`{}` has no AgentMark type", render(rt))),
            },
            _ => self.value(rt).map(Some),
        }
    }
}

// ---------------------------------------------------------------------------- 1:1 conversions

const BOXERR: &str = "-> Result<_, Box<dyn std::error::Error + Send + Sync>>";

fn is_chrono_value(full: &[String], args: &[RT]) -> bool {
    is_chrono(full)
        && match last(full) {
            "NaiveDate" | "Duration" | "TimeDelta" => true,
            "DateTime" => matches!(args, [RT::Path { full: z, .. }] if last(z) == "Utc"),
            _ => false,
        }
}

/// Owned AgentMark value `e` -> the Rust argument for parameter type `rt`; `None` when the
/// mapping is not 1:1.
pub fn conv_in(e: &str, rt: &RT) -> Option<String> {
    match rt {
        RT::Prim(p) if p == "i64" || p == "bool" => Some(e.to_string()),
        RT::Prim(p) if SMALL_INTS.contains(&p.as_str()) => Some(format!("{p}::try_from({e})?")),
        RT::Ref { inner, mutable: false } => match &**inner {
            RT::Prim(p) if p == "str" => Some(format!("&{e}")),
            RT::Slice(i) if is_u8(i) => Some(format!("&{e}")),
            RT::Path { full, args } if is_std(full) && matches!((last(full), args.as_slice()), ("String", _)) => {
                Some(format!("&{e}"))
            }
            RT::Path { full, args } if is_std(full) && last(full) == "Vec" && args.first().is_some_and(is_u8) => {
                Some(format!("&{e}"))
            }
            RT::Path { full, args } if is_chrono_value(full, args) => Some(format!("&{e}")),
            _ => None,
        },
        RT::Path { full, args } if is_std(full) => match (last(full), args.as_slice()) {
            ("String", _) => Some(e.to_string()),
            ("Vec", [a]) if is_u8(a) => Some(e.to_string()),
            ("Vec", [a]) => {
                let inner = conv_in("v", a)?;
                Some(if inner == "v" {
                    e.to_string()
                } else {
                    format!("{e}.into_iter().map(|v| {BOXERR} {{ Ok({inner}) }}).collect::<Result<Vec<_>, _>>()?")
                })
            }
            ("Option", [RT::Ref { inner, mutable: false }])
                if matches!(&**inner, RT::Prim(p) if p == "str") || matches!(&**inner, RT::Slice(i) if is_u8(i)) =>
            {
                Some(format!("{e}.as_deref()"))
            }
            ("Option", [a]) => {
                let inner = conv_in("v", a)?;
                Some(if inner == "v" {
                    e.to_string()
                } else {
                    format!("{e}.map(|v| {BOXERR} {{ Ok({inner}) }}).transpose()?")
                })
            }
            _ => None,
        },
        RT::Path { full, args } if is_chrono_value(full, args) => Some(e.to_string()),
        _ => None,
    }
}

/// Rust value expression `e` of type `rt` -> AgentMark value; `None` when not 1:1.
pub fn conv_out(e: &str, rt: &RT) -> Option<String> {
    match rt {
        RT::Prim(p) if p == "i64" || p == "bool" => Some(e.to_string()),
        RT::Prim(p) if SMALL_INTS.contains(&p.as_str()) => Some(format!("i64::from({e})")),
        RT::Ref { inner, mutable: false } => match &**inner {
            RT::Prim(p) if p == "str" => Some(format!("{e}.to_string()")),
            RT::Slice(i) if is_u8(i) => Some(format!("{e}.to_vec()")),
            RT::Path { full, args } if is_chrono_value(full, args) => Some(format!("{e}.clone()")),
            RT::Path { full, args }
                if is_std(full)
                    && (last(full) == "String" || (last(full) == "Vec" && args.first().is_some_and(is_u8))) =>
            {
                Some(format!("{e}.clone()"))
            }
            _ => None,
        },
        RT::Path { full, args } if is_std(full) => match (last(full), args.as_slice()) {
            ("String", _) => Some(e.to_string()),
            ("Vec", [a]) if is_u8(a) => Some(e.to_string()),
            ("Vec", [a]) => {
                let inner = conv_out("v", a)?;
                Some(if inner == "v" {
                    e.to_string()
                } else {
                    format!("{e}.into_iter().map(|v| {inner}).collect::<Vec<_>>()")
                })
            }
            ("Option", [a]) => {
                let inner = conv_out("v", a)?;
                Some(if inner == "v" {
                    e.to_string()
                } else {
                    format!("{e}.map(|v| {inner})")
                })
            }
            _ => None,
        },
        RT::Path { full, args } if is_chrono_value(full, args) => Some(e.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(n: &str) -> RT {
        RT::Prim(n.into())
    }

    #[test]
    fn small_ints_convert_with_try_from_and_wide_ones_are_not_direct() {
        assert_eq!(conv_in("x", &p("i64")).unwrap(), "x");
        assert_eq!(conv_in("x", &p("u32")).unwrap(), "u32::try_from(x)?");
        assert!(conv_in("x", &p("u64")).is_none());
        assert_eq!(conv_out("y", &p("i32")).unwrap(), "i64::from(y)");
    }

    #[test]
    fn option_of_str_param_uses_as_deref() {
        let t = RT::Path {
            full: vec!["core".into(), "Option".into()],
            args: vec![RT::Ref { inner: Box::new(p("str")), mutable: false }],
        };
        assert_eq!(conv_in("s", &t).unwrap(), "s.as_deref()");
        assert_eq!(conv_out("r", &t).unwrap(), "r.map(|v| v.to_string())");
    }

    #[test]
    fn generic_with_asref_str_has_one_obvious_instantiation() {
        let bounds = vec![(
            "S".to_string(),
            vec![Bound { name: "AsRef".into(), args: vec![p("str")], output: None, marker: false }],
        )];
        let l = Lifter { bounds: &bounds };
        assert_eq!(l.value(&RT::Generic("S".into())), Ok(Ty::Text));
        assert!(l.value(&RT::Generic("T".into())).is_err());
    }
}
