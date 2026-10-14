//! Rustdoc JSON types -> a small Rust type model (`RT`), its rendering, the lift onto AgentMark
//! types, and the 1:1 conversions used by the direct tier.

use crate::binder::core::{HandleMode, Ty};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

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
    /// `<S as Trait>::Name<'a>`: kept so a handle's type can be written out (`lifetimes` is the
    /// number of lifetime arguments of the associated type).
    Qual { self_ty: Box<RT>, trait_path: Option<Vec<String>>, name: String, lifetimes: usize },
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    pub name: String,
    /// The trait's path (`digest::Digest`) as far as rustdoc says; `name` is its last segment.
    pub full: Vec<String>,
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
/// Integers that can be wider than `int`: converted with a checked conversion that fails the call
/// on overflow (SPEC 22.4).
const WIDE_INTS: &[&str] = &["u64", "usize", "isize"];

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

    pub fn full_path_of(&self, id: Option<u64>, fallback: &str) -> Vec<String> {
        self.full_path(id, fallback)
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
                let tr = &body["trait"];
                let trait_path = if tr.is_null() {
                    None
                } else {
                    Some(self.full_path(tr["id"].as_u64(), tr["path"].as_str().unwrap_or("?")))
                };
                let lifetimes = body["args"]["angle_bracketed"]["args"]
                    .as_array()
                    .map_or(0, |a| a.iter().filter(|x| x.get("lifetime").is_some()).count());
                RT::Qual {
                    self_ty: Box::new(self.parse(&body["self_type"])),
                    trait_path,
                    name: body["name"].as_str().unwrap_or("?").to_string(),
                    lifetimes,
                }
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
            let full = self.full_path(t["id"].as_u64(), t["path"].as_str().unwrap_or("?"));
            out.push(Bound {
                name,
                full,
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

/// Rust-looking text for a type. Only the prelude types (`String`, `Vec`, `Option`, `Result`,
/// `Box`) are shortened; every other path keeps its full module path, so the effect heuristics
/// can see `std::fs::File`, `std::path::Path` or `core::net::SocketAddr`.
pub fn render(t: &RT) -> String {
    match t {
        RT::Prim(p) => p.clone(),
        RT::Slice(i) => format!("[{}]", render(i)),
        RT::Array(i) => format!("[{}; _]", render(i)),
        RT::Tuple(v) => format!("({})", v.iter().map(render).collect::<Vec<_>>().join(", ")),
        RT::Ref { inner, mutable } => format!("&{}{}", if *mutable { "mut " } else { "" }, render(inner)),
        RT::Ptr => "*const _".into(),
        RT::Path { full, args } => {
            let prelude = matches!(last(full), "String" | "Vec" | "Option" | "Result" | "Box" | "Cow");
            let name = if is_std(full) && prelude { last(full).to_string() } else { full.join("::") };
            if args.is_empty() {
                name
            } else {
                format!("{name}<{}>", args.iter().map(render).collect::<Vec<_>>().join(", "))
            }
        }
        RT::Generic(n) => n.clone(),
        RT::Impl(b) => format!("impl {}", render_bounds(b)),
        RT::Dyn(b) => format!("dyn {}", render_bounds(b)),
        RT::Qual { self_ty, trait_path, name, lifetimes } => {
            let lt = if *lifetimes == 0 { String::new() } else { format!("<{}>", vec!["'static"; *lifetimes].join(", ")) };
            match trait_path {
                Some(t) => format!("<{} as {}>::{name}{lt}", render(self_ty), t.join("::")),
                None => format!("<{}>::{name}{lt}", render(self_ty)),
            }
        }
        RT::Other(s) => s.clone(),
    }
}

/// `t` with every path replaced by the public one (`pubmap`: canonical path -> public path).
fn publicize(t: &RT, pubmap: &HashMap<Vec<String>, Vec<String>>) -> RT {
    let p = |x: &RT| publicize(x, pubmap);
    match t {
        RT::Path { full, args } => RT::Path {
            full: pubmap.get(full).cloned().unwrap_or_else(|| full.clone()),
            args: args.iter().map(p).collect(),
        },
        RT::Slice(i) => RT::Slice(Box::new(p(i))),
        RT::Array(i) => RT::Array(Box::new(p(i))),
        RT::Tuple(v) => RT::Tuple(v.iter().map(p).collect()),
        RT::Ref { inner, mutable } => RT::Ref { inner: Box::new(p(inner)), mutable: *mutable },
        RT::Qual { self_ty, trait_path, name, lifetimes } => RT::Qual {
            self_ty: Box::new(p(self_ty)),
            trait_path: trait_path.as_ref().map(|t| pubmap.get(t).cloned().unwrap_or_else(|| t.clone())),
            name: name.clone(),
            lifetimes: *lifetimes,
        },
        other => other.clone(),
    }
}

/// `sqlx::Any`, `AnyConnection`: a type named `Any…` that erases which concrete type it is.
pub fn is_erased(t: &RT) -> bool {
    matches!(t, RT::Path { full, .. } if full.last().is_some_and(|n| n == "Any" || (n.starts_with("Any") && n[3..].starts_with(|c: char| c.is_ascii_uppercase()))))
}

/// A type that cannot be written out as a handle's one instantiation: a parameter or an associated
/// type that is still open, or the type-erased driver.
fn is_open(t: &RT) -> bool {
    match t {
        RT::Generic(_) | RT::Impl(_) | RT::Qual { .. } => true,
        RT::Path { args, .. } => is_erased(t) || args.iter().any(is_open),
        RT::Slice(i) | RT::Array(i) | RT::Ref { inner: i, .. } => is_open(i),
        RT::Tuple(v) => v.iter().any(is_open),
        _ => false,
    }
}

fn has_generic(t: &RT) -> bool {
    match t {
        RT::Generic(_) | RT::Impl(_) => true,
        RT::Path { args, .. } => args.iter().any(has_generic),
        RT::Slice(i) | RT::Array(i) | RT::Ref { inner: i, .. } => has_generic(i),
        RT::Tuple(v) => v.iter().any(has_generic),
        RT::Qual { self_ty, .. } => has_generic(self_ty),
        _ => false,
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

/// A public type of the bound crate that becomes a handle (SPEC 22.4).
#[derive(Debug, Clone)]
pub struct HandleDef {
    /// `sha2.Sha256`: the dotted name below the crate.
    pub name: String,
    /// Its public Rust path, `sha2::Sha256`.
    pub rust_path: String,
    /// Lifetime parameters: written `'static` (the handle owns what it borrows).
    pub lifetimes: usize,
    /// It has type parameters: written with the single instantiation the signatures give.
    pub generic: bool,
}

/// What the lift needs to know about the whole crate.
#[derive(Default)]
pub struct LiftCtx {
    /// Public types that become handles, by every path a signature can name them by.
    pub handles: HashMap<Vec<String>, HandleDef>,
    /// Trait (last name) -> the concrete types of the crate that implement it: a bound with exactly one
    /// of them has one obvious instantiation (`DB: Database` with one database compiled in).
    pub implementors: HashMap<String, Vec<RT>>,
    /// Canonical path -> public path of the crate's own types.
    pub pubmap: HashMap<Vec<String>, Vec<String>>,
    /// Handle name -> its Rust type, for the generic / lifetime ones as the signatures instantiate them.
    pub rust_types: RefCell<BTreeMap<String, String>>,
    /// `(Self type, associated type name)` -> the type, from the crate's trait impls
    /// (`<Sqlite as Database>::QueryResult` is `SqliteQueryResult`).
    pub assoc: HashMap<(Vec<String>, String), RT>,
    /// What an `Executor` bound stands for: a borrow of the crate's one pool alias (Q412).
    pub executor: Option<RT>,
}

pub struct Lifter<'a> {
    /// Generic parameter name -> its bounds (fn and impl level).
    pub bounds: &'a [(String, Vec<Bound>)],
    pub ctx: &'a LiftCtx,
    /// Also instantiate a bound by the crate's only implementor of the trait (used for items that
    /// turn out to involve a handle; other generics keep their v0.2 treatment).
    pub catchall: bool,
    /// Rust types of handles met while lifting this item; the caller keeps them only if the item is bound.
    pub pending: RefCell<BTreeMap<String, String>>,
    /// Type parameters already known (an impl for `Pool<DB>` reached through the alias `SqlitePool`).
    pub fixed: HashMap<String, RT>,
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
                    _ => return Err(format!("bound `{}` has no single obvious instantiation", render_bounds(std::slice::from_ref(b)))),
                },
                ("ToString" | "Display" | "IntoUrl", _) => std_path("String", vec![]),
                // a value handed to a database: text is the one value every column takes (Q412)
                ("Encode" | "Type", _) if self.catchall => std_path("String", vec![]),
                ("Executor", _) if self.catchall => match &self.ctx.executor {
                    Some(e) => e.clone(),
                    None => return Err(format!("bound `{}` has no single obvious instantiation", render_bounds(std::slice::from_ref(b)))),
                },
                ("TimeZone", _) => utc(),
                _ if self.catchall => match self.ctx.implementors.get(&b.full.join("::")).map(Vec::as_slice) {
                    Some([only]) => only.clone(),
                    _ => return Err(format!("bound `{}` has no single obvious instantiation", render_bounds(std::slice::from_ref(b)))),
                },
                _ => return Err(format!("bound `{}` has no single obvious instantiation", render_bounds(std::slice::from_ref(b)))),
            };
            match &found {
                Some(f) if *f != one => {
                    return Err("bounds disagree on one obvious instantiation".into());
                }
                _ => found = Some(one),
            }
        }
        // a parameter nothing but markers (`Send`) constrains is a value of any kind; text is the AgentMark
        // value that stands for one (`query_scalar::<DB, O>`: the column read as text, Q412)
        if found.is_none() && self.catchall && bounds.iter().all(|b| b.marker) {
            return Ok(std_path("String", vec![]));
        }
        found.ok_or_else(|| "unbounded generic has no single obvious instantiation".into())
    }

    fn generic(&self, name: &str) -> Result<RT, String> {
        if let Some(t) = self.fixed.get(name) {
            return Ok(t.clone());
        }
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
            RT::Qual { self_ty, trait_path, name, lifetimes } => {
                let st = self.resolve(self_ty);
                if let RT::Path { full, .. } = &st {
                    if let Some(t) = self.ctx.assoc.get(&(full.clone(), name.clone())) {
                        return t.clone();
                    }
                }
                RT::Qual { self_ty: Box::new(st), trait_path: trait_path.clone(), name: name.clone(), lifetimes: *lifetimes }
            }
            RT::Ref { inner, mutable } => RT::Ref { inner: Box::new(self.resolve(inner)), mutable: *mutable },
            RT::Slice(i) => RT::Slice(Box::new(self.resolve(i))),
            RT::Tuple(v) => RT::Tuple(v.iter().map(|t| self.resolve(t)).collect()),
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
            RT::Ref { inner, .. } if self.handle_of(inner).is_some() => {
                Err(format!("`{}` borrows a handle, and a borrow cannot be returned or stored", render(rt)))
            }
            RT::Ref { inner, .. } => self.value(inner),
            RT::Slice(i) if is_u8(i) => Ok(Ty::Bytes),
            RT::Slice(i) => Ok(Ty::List(Box::new(self.not_handle(self.value(i)?, "a list")?))),
            RT::Path { full, args } => {
                let args: Vec<RT> = args.iter().map(|a| self.resolve(a)).collect();
                let args = args.as_slice();
                let n = last(full);
                if is_std(full) {
                    match (n, args) {
                        ("String", _) => return Ok(Ty::Text),
                        ("Vec", [a]) if is_u8(a) => return Ok(Ty::Bytes),
                        ("Vec", [a]) => return Ok(Ty::List(Box::new(self.not_handle(self.value(a)?, "a list")?))),
                        ("Option", [a]) => return Ok(Ty::Maybe(Box::new(self.not_handle(self.value(a)?, "a maybe")?))),
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
                    _ => match self.raw_handle(rt) {
                        Some(h) => Ok(Ty::handle(h, HandleMode::Shared)),
                        None => no(),
                    },
                }
            }
            RT::Generic(g) => self.value(&self.generic(g)?),
            RT::Impl(b) => self.value(&self.instantiate(b)?),
            RT::Qual { .. } => {
                let r = self.resolve(rt);
                if r == *rt { no() } else { self.value(&r) }
            }
            _ => no(),
        }
    }

    /// The handle name of a public type of the bound crate, by its path alone. A type with
    /// lifetimes or type parameters is written out with `'static` and the one instantiation the
    /// signatures give; a second, different one is not a handle.
    fn raw_handle(&self, rt: &RT) -> Option<String> {
        let RT::Path { full, args } = rt else { return None };
        let def = self.ctx.handles.get(full)?;
        if !def.generic && def.lifetimes == 0 {
            return args.is_empty().then(|| def.name.clone());
        }
        let mut parts: Vec<String> = vec!["'static".to_string(); def.lifetimes];
        for a in args {
            let r = self.resolve(a);
            if has_generic(&r) {
                // a type parameter of the impl that nothing here pins down is the instantiation the
                // handle already has (`Query<'q, DB, A>` in a method: the `Query` that `query` returned)
                if self.catchall && self.ctx.rust_types.borrow().contains_key(&def.name) {
                    return Some(def.name.clone());
                }
                return None;
            }
            if is_open(&r) {
                return None;
            }
            parts.push(self.rust_arg(&r));
        }
        let ty = if parts.is_empty() { def.rust_path.clone() } else { format!("{}<{}>", def.rust_path, parts.join(", ")) };
        let mut pending = self.pending.borrow_mut();
        match self.ctx.rust_types.borrow().get(&def.name).or_else(|| pending.get(&def.name)) {
            Some(prev) if *prev != ty => None,
            _ => {
                pending.insert(def.name.clone(), ty);
                Some(def.name.clone())
            }
        }
    }

    /// A type argument of a handle's Rust type, by public paths and with `'static` for every lifetime
    /// parameter of a type of the crate (rustdoc's paths do not keep lifetime arguments).
    fn rust_arg(&self, t: &RT) -> String {
        match t {
            RT::Path { full, args } => {
                let name = render(&RT::Path { full: self.ctx.pubmap.get(full).unwrap_or(full).clone(), args: vec![] });
                let lifetimes = self.ctx.handles.get(full).map_or(0, |d| d.lifetimes);
                let mut parts: Vec<String> = vec!["'static".to_string(); lifetimes];
                parts.extend(args.iter().map(|a| self.rust_arg(a)));
                if parts.is_empty() { name } else { format!("{name}<{}>", parts.join(", ")) }
            }
            RT::Ref { inner, mutable } => format!("&{}{}", if *mutable { "mut " } else { "" }, self.rust_arg(inner)),
            other => render(&publicize(other, &self.ctx.pubmap)),
        }
    }

    /// The handle name when AgentMark holds `rt` as a handle: a type with a built-in AgentMark
    /// type (`NaiveDate`, `Url`, ...) is a value, not a handle.
    pub fn handle_of(&self, rt: &RT) -> Option<String> {
        self.raw_handle(rt)?;
        match self.value(rt) {
            Ok(Ty::Handle { name, .. }) => Some(name),
            _ => None,
        }
    }

    fn not_handle(&self, t: Ty, what: &str) -> Result<Ty, String> {
        match t {
            Ty::Handle { name, .. } => Err(format!("`handle of {name}` cannot be in {what}")),
            t => Ok(t),
        }
    }

    /// A parameter: a handle is lent (`&T`), lent mutably (`&mut T`) or handed over (`T`).
    pub fn param(&self, rt: &RT) -> Result<Ty, String> {
        // `E: Executor` stands for `&SqlitePool`: look at what the parameter is instantiated as
        let inst;
        let rt = match rt {
            RT::Generic(_) | RT::Impl(_) => {
                inst = self.instantiated(rt);
                &inst
            }
            _ => rt,
        };
        if let RT::Ref { inner, mutable } = rt {
            if let Some(h) = self.handle_of(inner) {
                return Ok(Ty::handle(h, if *mutable { HandleMode::Mut } else { HandleMode::Shared }));
            }
        }
        match self.value(rt)? {
            Ty::Handle { name, .. } => Ok(Ty::handle(name, HandleMode::Consume)),
            t => Ok(t),
        }
    }

    /// The type a generic parameter or `impl Trait` stands for (`impl AsRef<[u8]>` -> `Vec<u8>`);
    /// anything else as it is.
    pub fn instantiated(&self, rt: &RT) -> RT {
        match rt {
            RT::Generic(g) => self.generic(g).unwrap_or_else(|_| rt.clone()),
            RT::Impl(b) => self.instantiate(b).unwrap_or_else(|_| rt.clone()),
            _ => rt.clone(),
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
        RT::Prim(p) if SMALL_INTS.contains(&p.as_str()) || WIDE_INTS.contains(&p.as_str()) => {
            Some(format!("{p}::try_from({e})?"))
        }
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
        RT::Prim(p) if WIDE_INTS.contains(&p.as_str()) => Some(format!("i64::try_from({e})?")),
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
                } else if inner.contains('?') {
                    format!("{e}.into_iter().map(|v| {BOXERR} {{ Ok({inner}) }}).collect::<Result<Vec<_>, _>>()?")
                } else {
                    format!("{e}.into_iter().map(|v| {inner}).collect::<Vec<_>>()")
                })
            }
            ("Option", [a]) => {
                let inner = conv_out("v", a)?;
                Some(if inner == "v" {
                    e.to_string()
                } else if inner.contains('?') {
                    format!("{e}.map(|v| {BOXERR} {{ Ok({inner}) }}).transpose()?")
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
        // wider than `int`: a checked conversion that fails the call on overflow
        assert_eq!(conv_in("x", &p("u64")).unwrap(), "u64::try_from(x)?");
        assert_eq!(conv_out("y", &p("usize")).unwrap(), "i64::try_from(y)?");
        assert!(conv_in("x", &p("f64")).is_none());
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
            vec![Bound { name: "AsRef".into(), full: vec![], args: vec![p("str")], output: None, marker: false }],
        )];
        let ctx = LiftCtx::default();
        let l = Lifter { bounds: &bounds, ctx: &ctx, catchall: false, pending: Default::default(), fixed: Default::default() };
        assert_eq!(l.value(&RT::Generic("S".into())), Ok(Ty::Text));
        assert!(l.value(&RT::Generic("T".into())).is_err());
    }
}
