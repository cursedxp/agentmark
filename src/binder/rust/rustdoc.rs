//! rustdoc JSON -> [`CrateItems`].
//!
//! Parsed with `serde_json::Value`, not the `rustdoc-types` crate: that crate is not in the
//! offline registry cache and its version must match the nightly's `format_version` exactly.
//! The reader reads only the handful of fields it needs and refuses formats it was not written
//! for (`SUPPORTED_FORMAT`) instead of guessing.
//!
//! What counts as an item: every public free function (reachable from the crate root through
//! public modules and `pub use`), every public inherent method of a public struct/enum/union,
//! and every method of a trait impl on such a type unless the trait is std plumbing (`Clone`,
//! `Debug`, `Add`, ... see `NOISE_TRAITS`). Blanket and synthetic impls are skipped. Public
//! traits, type aliases of foreign types and re-exports of foreign items are not callable, so
//! they are listed under `## Not bound` instead of being tiered. Constants and statics are not
//! callable and are not listed. Trait methods that an impl does not override (provided methods)
//! are not in rustdoc's impl item list and therefore not seen.

use super::types::*;
use crate::binder::core::{CrateItems, EffectHints, HandleMode, Item, NotBoundEntry, Param, Shape, Sig, Ty};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

/// rustdoc JSON `format_version` this reader understands (nightly 2026-10, rustdoc-types 0.5x).
pub const SUPPORTED_FORMAT: u64 = 61;

const NOISE_TRAITS: &[&str] = &[
    "Clone", "Copy", "Debug", "Display", "Default", "PartialEq", "Eq", "PartialOrd", "Ord", "Hash",
    "From", "Into", "TryFrom", "TryInto", "Drop", "Deref", "DerefMut", "AsRef", "AsMut", "Borrow",
    "BorrowMut", "ToOwned", "ToString", "Send", "Sync", "Unpin", "UnwindSafe", "RefUnwindSafe",
    "Any", "Add", "Sub", "Mul", "Div", "Rem", "Neg", "Not", "AddAssign", "SubAssign", "MulAssign",
    "DivAssign", "RemAssign", "BitAnd", "BitOr", "BitXor", "Shl", "Shr", "FromStr", "Iterator",
    "IntoIterator", "DoubleEndedIterator", "ExactSizeIterator", "Extend", "FromIterator", "Error",
    "Future", "IntoFuture", "Sum", "Product", "Index", "IndexMut", "StructuralPartialEq",
    "CloneToUninit", "Pointer", "LowerHex", "UpperHex", "Octal", "Binary", "LowerExp", "UpperExp",
    "Write", "Read", "BufRead", "Seek", "Serialize", "Deserialize", "Arbitrary", "Step", "Fn",
    "FnMut", "FnOnce",
];

pub fn effect_hints() -> EffectHints {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect();
    EffectHints {
        network: s(&["reqwest", "tokio::net", "std::net", "core::net", "hyper", "ureq", "isahc", "surf", "TcpStream", "UdpSocket"]),
        local: s(&["std::fs", "tokio::fs", "std::process", "std::env", "std::io::stdin", "std::path::Path"]),
    }
}

const STYLE_GUIDE: &str = "\
- The body returns `Ok(value)` or fails with `?`; errors are boxed (`Box<dyn std::error::Error + Send + Sync>`).\n\
- AgentMark values arrive owned: text = String, int = i64, bool, bytes = Vec<u8>, date = chrono::NaiveDate, \
datetime = chrono::DateTime<chrono::Utc>, duration = chrono::Duration, list of T = Vec<T>, maybe T = Option<T>.\n\
- Convert integers with `try_from(..)?`, never `as`. No floats. No `unwrap`/`expect`/`panic!`.\n\
- Refer to the library by full path (`chrono::NaiveDate::from_ymd_opt`); the body has no `use` lines.\n\
- `handle of <crate>.<Type>` is the library's own Rust type `<crate>::<Type>`, never converted: a plain `handle of` \
parameter is `&T`, `mut handle of` is `&mut T`, `consume handle of` is `T`, a returned `handle of` is the owned value; \
the parameter named `self` is `self_` in Rust.\n\
- Keep it short: call the library, convert the result to the declared AgentMark type.";

pub fn snake(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let mut o = String::new();
    for (i, &c) in cs.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let prev = i.checked_sub(1).map(|j| cs[j]);
            let next = cs.get(i + 1).copied();
            let split = prev.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit())
                || (prev.is_some_and(|p| p.is_ascii_uppercase()) && next.is_some_and(|n| n.is_ascii_lowercase()));
            if split && !o.ends_with('_') {
                o.push('_');
            }
            o.push(c.to_ascii_lowercase());
        } else if c.is_ascii_alphanumeric() {
            o.push(c);
        } else if !o.ends_with('_') {
            o.push('_');
        }
    }
    o.trim_matches('_').to_string()
}

/// First sentence of a rustdoc comment as plain text: markdown links reduced to their text,
/// code fences and everything after the first paragraph dropped.
pub fn first_sentence(docs: &str) -> String {
    let mut para = Vec::new();
    for l in docs.lines() {
        let t = l.trim();
        if t.starts_with("```") {
            break;
        }
        if t.is_empty() {
            if para.is_empty() {
                continue;
            }
            break;
        }
        para.push(t);
    }
    let cs: Vec<char> = para.join(" ").chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '[' {
            if let Some(j) = cs[i..].iter().position(|&c| c == ']').map(|p| p + i) {
                out.extend(&cs[i + 1..j]);
                i = j + 1;
                let close = match cs.get(i) {
                    Some('(') => Some(')'),
                    Some('[') => Some(']'),
                    _ => None,
                };
                if let Some(cl) = close {
                    if let Some(k) = cs[i..].iter().position(|&c| c == cl) {
                        i += k + 1;
                    }
                }
                continue;
            }
        }
        out.push(cs[i]);
        i += 1;
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let end = out
        .match_indices(". ")
        .next()
        .map(|(p, _)| p + 1)
        .unwrap_or(out.len());
    let s = out[..end].trim().to_string();
    if s.is_empty() || s.ends_with('.') || s.ends_with('!') || s.ends_with('?') {
        s
    } else {
        format!("{s}.")
    }
}

fn trimmed_docs(docs: &str) -> String {
    let d: String = docs.chars().take(900).collect();
    // Stop before example code: it is long and the prompt only needs the description.
    d.split("```").next().unwrap_or("").trim().to_string()
}

/// `a::b::C<T, U>` -> `a::b::C<…>`: the generic arguments are noise in a one-line reason.
fn without_generics(ty: &str) -> String {
    match ty.find('<') {
        Some(i) => format!("{}<…>", &ty[..i]),
        None => ty.to_string(),
    }
}

fn kind(it: &Value) -> &str {
    it["inner"]
        .as_object()
        .and_then(|o| o.keys().next())
        .map(String::as_str)
        .unwrap_or("")
}

fn is_public(it: &Value) -> bool {
    it["visibility"] == "public"
        && !it["attrs"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x.as_str().is_some_and(|s| s.contains("doc(hidden)"))))
}

struct Walk<'a> {
    doc: &'a Value,
    crate_ident: String,
    pub_paths: HashMap<u64, Vec<String>>,
    order: Vec<u64>,
    seen_modules: HashSet<u64>,
    /// An impl can be listed by more than one type (`impl Trait for &Client`): emit it once.
    seen_impls: HashSet<u64>,
    extra: Vec<NotBoundEntry>,
    /// `pub use other_crate::Trait` seen in this crate: (source path, local public path).
    foreign_uses: Vec<(String, Vec<String>)>,
    names: HashSet<String>,
    items: Vec<Item>,
    /// Public types that become `handle of <crate>.<Type>` (SPEC 22.4): the paths a signature
    /// can name them by -> the dotted name below the crate.
    ctx: Rc<LiftCtx>,
    /// Type alias public path -> its `alias_…` line under `## Not bound`; dropped when the alias
    /// turned out to be a handle whose methods are bound.
    alias_entries: HashMap<Vec<String>, String>,
}

pub fn read_crate(json: &str) -> Result<CrateItems, String> {
    read_crate_with(json, &[])
}

/// Names of other crates whose rustdoc JSON would add items: crates this crate re-exports a
/// trait from and aliases a type of (sha2's `Sha256` is a `digest::CoreWrapper<..>`, and
/// `digest::Digest` is where hashing lives).
pub fn foreign_crates(json: &str) -> Result<Vec<String>, String> {
    let doc = parse_doc(json)?;
    let w = walked(&doc)?;
    let mut out: Vec<String> = Vec::new();
    for (source, _) in &w.foreign_uses {
        let c = source.split("::").next().unwrap_or("").to_string();
        if !c.is_empty() && !out.contains(&c) && !w.aliases_of(&c).is_empty() {
            out.push(c);
        }
    }
    Ok(out)
}

fn parse_doc(json: &str) -> Result<Value, String> {
    let doc: Value = serde_json::from_str(json).map_err(|e| format!("rustdoc JSON is not valid JSON: {e}"))?;
    let fv = doc["format_version"].as_u64().unwrap_or(0);
    if fv != SUPPORTED_FORMAT {
        return Err(format!(
            "rustdoc JSON format_version {fv} is not supported (this binder reads {SUPPORTED_FORMAT}); \
             regenerate it with the nightly this binder was built for"
        ));
    }
    Ok(doc)
}

fn walked(doc: &Value) -> Result<Walk<'_>, String> {
    let root_id = doc["root"].as_u64().ok_or("rustdoc JSON has no root")?;
    let root = &doc["index"][root_id.to_string()];
    let name = root["name"].as_str().ok_or("rustdoc root has no name")?.to_string();
    let mut w = Walk {
        doc,
        crate_ident: name.replace('-', "_"),
        pub_paths: HashMap::new(),
        order: Vec::new(),
        seen_modules: HashSet::new(),
        seen_impls: HashSet::new(),
        extra: Vec::new(),
        foreign_uses: Vec::new(),
        names: HashSet::new(),
        items: Vec::new(),
        ctx: Rc::new(LiftCtx::default()),
        alias_entries: HashMap::new(),
    };
    w.seen_modules.insert(root_id);
    let prefix = vec![w.crate_ident.clone()];
    w.walk_module(root_id, &prefix);
    let ctx = w.lift_ctx();
    w.ctx = Rc::new(ctx);
    Ok(w)
}

/// `foreign` are rustdoc JSON texts of other crates (see [`foreign_crates`]): a trait this crate
/// re-exports gets its receiver-less methods bound on this crate's type aliases of that crate's
/// types (OPEN_QUESTIONS Q73).
pub fn read_crate_with(json: &str, foreign: &[String]) -> Result<CrateItems, String> {
    let doc = parse_doc(json)?;
    let mut w = walked(&doc)?;
    let root_id = doc["root"].as_u64().unwrap();
    let name = doc["index"][root_id.to_string()]["name"].as_str().unwrap().to_string();
    let version = doc["crate_version"]
        .as_str()
        .ok_or("rustdoc JSON has no crate_version (was it built from a published crate?)")?
        .to_string();
    w.emit_all();
    for f in foreign {
        let fdoc = parse_doc(f)?;
        w.emit_foreign(&fdoc);
    }
    let handle_types = w.ctx.rust_types.borrow().clone();
    Ok(CrateItems {
        name,
        version,
        language: "Rust".into(),
        style_guide: STYLE_GUIDE.into(),
        code_lang: "rust".into(),
        hints: effect_hints(),
        items: w.items,
        extra_not_bound: w.extra,
        handle_types,
    })
}

impl<'a> Walk<'a> {
    fn get(&self, id: u64) -> Option<&'a Value> {
        self.doc["index"].get(id.to_string())
    }

    fn register(&mut self, id: u64, path: Vec<String>) {
        match self.pub_paths.get(&id) {
            Some(p) if p.len() <= path.len() => {}
            Some(_) => {
                self.pub_paths.insert(id, path);
            }
            None => {
                self.pub_paths.insert(id, path);
                self.order.push(id);
            }
        }
    }

    fn walk_module(&mut self, id: u64, prefix: &[String]) {
        let Some(m) = self.get(id) else { return };
        let ids: Vec<u64> = m["inner"]["module"]["items"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default();
        for cid in ids {
            self.visit(cid, prefix, None);
        }
    }

    /// `alias` is the name a `pub use ... as alias` gives the item.
    fn visit(&mut self, id: u64, prefix: &[String], alias: Option<&str>) {
        let Some(it) = self.get(id) else { return };
        let k = kind(it);
        if k == "use" {
            self.visit_use(&it["inner"]["use"], prefix);
            return;
        }
        if !is_public(it) && alias.is_none() {
            return;
        }
        let name = alias.or(it["name"].as_str()).unwrap_or("?").to_string();
        let mut path = prefix.to_vec();
        path.push(name);
        match k {
            "module" => {
                self.register(id, path.clone());
                if self.seen_modules.insert(id) {
                    self.walk_module(id, &path);
                }
            }
            "function" | "struct" | "enum" | "union" | "trait" | "type_alias" => self.register(id, path),
            _ => {}
        }
    }

    fn visit_use(&mut self, u: &Value, prefix: &[String]) {
        let source = u["source"].as_str().unwrap_or("?");
        let uname = u["name"].as_str().unwrap_or("?");
        let target = u["id"].as_u64().filter(|i| self.get(*i).is_some());
        let glob = u["is_glob"].as_bool().unwrap_or(false);
        match target {
            Some(t) if glob => {
                if self.seen_modules.insert(t) {
                    self.walk_module(t, prefix);
                }
            }
            Some(t) => {
                // The `use` item itself is public (checked by rustdoc: private uses are absent
                // from the module's public view only via visibility), so bind its target.
                let tk = kind(self.get(t).unwrap());
                if tk == "module" {
                    let mut p = prefix.to_vec();
                    p.push(uname.to_string());
                    self.register(t, p.clone());
                    if self.seen_modules.insert(t) {
                        self.walk_module(t, &p);
                    }
                } else {
                    self.visit(t, prefix, Some(uname));
                }
            }
            None => {
                let mut base = format!("reexport_{}{}", if glob { "glob_" } else { "" }, snake(source));
                if !glob && source.rsplit("::").next() != Some(uname) {
                    base.push_str(&format!("_as_{}", snake(uname)));
                }
                let name = self.unique(base);
                let krate = source.split("::").next().unwrap_or(source);
                self.extra.push(NotBoundEntry {
                    name,
                    reason: format!(
                        "re-export of `{source}` from another crate, not a callable of this one \
                         (run `agentmark bind {krate}` for that crate's own API)"
                    ),
                });
                if !glob {
                    let mut p = prefix.to_vec();
                    p.push(uname.to_string());
                    self.foreign_uses.push((source.to_string(), p));
                }
            }
        }
    }

    /// What the lift needs to know of the crate: the public types that become handles, who
    /// implements which trait, and the public path of every type.
    fn lift_ctx(&self) -> LiftCtx {
        let mut ctx = LiftCtx::default();
        for id in &self.order {
            let Some(it) = self.get(*id) else { continue };
            let k = kind(it);
            let path = &self.pub_paths[id];
            let canon: Option<Vec<String>> = self.doc["paths"][id.to_string()]["path"]
                .as_array()
                .map(|c| c.iter().filter_map(|s| s.as_str().map(String::from)).collect());
            if let Some(c) = &canon {
                if c != path {
                    ctx.pubmap.insert(c.clone(), path.clone());
                }
            }
            if path.len() < 2 {
                continue;
            }
            let params = |g: &Value| -> (usize, usize, usize) {
                let ps = g["params"].as_array().cloned().unwrap_or_default();
                let n = |key: &str| ps.iter().filter(|p| p["kind"].get(key).is_some()).count();
                (n("lifetime"), n("type"), n("const"))
            };
            let def = match k {
                "struct" | "enum" | "union" => {
                    let g = &it["inner"][k]["generics"];
                    // a rustdoc file without `generics` cannot say a type is plain: no handle
                    if g.is_null() {
                        continue;
                    }
                    let (lt, ty, cn) = params(g);
                    (cn == 0).then_some((lt, ty > 0))
                }
                "type_alias" => {
                    let a = &it["inner"]["type_alias"];
                    let plain = a["generics"]["params"].as_array().is_some_and(|p| p.is_empty()) && {
                        let t = TypeParser::new(self.doc, None).parse(&a["type"]);
                        matches!(&t, RT::Path { .. }) && !t.is_std_path()
                    };
                    plain.then_some((0, false))
                }
                _ => None,
            };
            let Some((lifetimes, generic)) = def else { continue };
            let d = HandleDef { name: path.join("."), rust_path: path.join("::"), lifetimes, generic };
            ctx.handles.insert(path.clone(), d.clone());
            if let Some(c) = canon.filter(|c| c.len() >= 2) {
                ctx.handles.entry(c).or_insert(d);
            }
        }
        // implementors of traits: the crate's own concrete types
        if let Some(index) = self.doc["index"].as_object() {
            for it in index.values().filter(|i| kind(i) == "impl") {
                let imp = &it["inner"]["impl"];
                if imp["is_synthetic"] == true || !imp["blanket_impl"].is_null() || imp["is_negative"] == true || imp["trait"].is_null() {
                    continue;
                }
                let tpath = imp["trait"]["path"].as_str().unwrap_or("?");
                let tname = TypeParser::new(self.doc, None)
                    .full_path_of(imp["trait"]["id"].as_u64(), tpath)
                    .join("::");
                let ty = TypeParser::new(self.doc, None).parse(&imp["for"]);
                let concrete = matches!(&ty, RT::Path { args, .. } if args.is_empty())
                    && imp["generics"]["params"].as_array().is_none_or(|p| p.is_empty());
                if concrete {
                    let v = ctx.implementors.entry(tname).or_default();
                    if !v.contains(&ty) {
                        v.push(ty);
                    }
                }
            }
        }
        ctx
    }

    fn unique(&mut self, base: String) -> String {
        let base = if base.is_empty() { "item".to_string() } else { base };
        let mut n = base.clone();
        let mut i = 2;
        while !self.names.insert(n.clone()) {
            n = format!("{base}_{i}");
            i += 1;
        }
        n
    }

    fn emit_all(&mut self) {
        let order = self.order.clone();
        for id in order {
            let Some(it) = self.get(id) else { continue };
            let path = self.pub_paths[&id].clone();
            match kind(it) {
                "function" => {
                    let doc = self.doc;
                    let item = self.build_fn(doc, it, &path, None);
                    self.items.push(item);
                }
                "struct" | "enum" | "union" => self.emit_type(id, &path),
                "trait" => {
                    let name = self.unique(format!("trait_{}", snake(&path[1..].join("_"))));
                    self.extra.push(NotBoundEntry {
                        name,
                        reason: format!("{} is a trait; traits are not bound (use a `## Rust` block)", path.join("::")),
                    });
                }
                "type_alias" => {
                    let target_id = it["inner"]["type_alias"]["type"]["resolved_path"]["id"].as_u64();
                    let std_target = it["inner"]["type_alias"]["type"]["resolved_path"]["path"]
                        .as_str()
                        .is_some_and(|p| matches!(p, "Result" | "Option" | "Vec" | "String" | "Box"))
                        || TypeParser::new(self.doc, None)
                            .parse(&it["inner"]["type_alias"]["type"])
                            .is_std_path();
                    if !std_target && target_id.is_none_or(|t| self.get(t).is_none()) {
                        let name = self.unique(format!("alias_{}", snake(&path[1..].join("_"))));
                        self.alias_entries.insert(path.clone(), name.clone());
                        let ty = TypeParser::new(self.doc, None).parse(&it["inner"]["type_alias"]["type"]);
                        self.extra.push(NotBoundEntry {
                            name,
                            reason: format!(
                                "{} is a type alias for `{}` from crate {}; its methods live there",
                                path.join("::"),
                                without_generics(&render(&ty)),
                                render(&ty).trim_start_matches('<').split([':', '<']).next().unwrap_or("another crate")
                            ),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    fn emit_type(&mut self, id: u64, tpath: &[String]) {
        let it = self.get(id).unwrap();
        let k = kind(it);
        let impls: Vec<u64> = it["inner"][k]["impls"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default();
        for iid in impls {
            let Some(imp) = self.get(iid) else { continue };
            if !self.seen_impls.insert(iid) {
                continue;
            }
            let inner = &imp["inner"]["impl"];
            if inner["is_synthetic"] == true || !inner["blanket_impl"].is_null() || inner["is_negative"] == true {
                continue;
            }
            let trait_ = if inner["trait"].is_null() {
                None
            } else {
                let t = &inner["trait"];
                let tname = t["path"].as_str().unwrap_or("?").rsplit("::").next().unwrap_or("?").to_string();
                if NOISE_TRAITS.contains(&tname.as_str()) {
                    continue;
                }
                let tid = t["id"].as_u64();
                let tpath = tid
                    .and_then(|i| self.pub_paths.get(&i).cloned())
                    .or_else(|| {
                        tid.and_then(|i| self.doc["paths"][i.to_string()]["path"].as_array().map(|a| {
                            a.iter().filter_map(|s| s.as_str().map(String::from)).collect()
                        }))
                    })
                    .unwrap_or_else(|| vec![tname.clone()]);
                Some((tname, tpath))
            };
            let fns: Vec<u64> = inner["items"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default();
            for fid in fns {
                let Some(f) = self.get(fid) else { continue };
                if kind(f) != "function" || (trait_.is_none() && !is_public(f)) {
                    continue;
                }
                let owner = Owner {
                    self_ty: TypeParser::new(self.doc, None).parse(&inner["for"]),
                    impl_generics: inner["generics"].clone(),
                    type_path: tpath.to_vec(),
                    trait_: trait_.clone(),
                    foreign_trait: false,
                };
                let path = tpath.to_vec();
                let doc = self.doc;
                let item = self.build_fn(doc, f, &path, Some(owner));
                self.items.push(item);
            }
        }
    }

    /// Public type aliases whose target type belongs to crate `krate`: (alias public path, alias type).
    fn aliases_of(&self, krate: &str) -> Vec<(Vec<String>, RT)> {
        let mut out = Vec::new();
        for id in &self.order {
            let it = self.get(*id).unwrap();
            if kind(it) != "type_alias" {
                continue;
            }
            let target = TypeParser::new(self.doc, None).parse(&it["inner"]["type_alias"]["type"]);
            if matches!(&target, RT::Path { full, .. } if full.first().map(String::as_str) == Some(krate)) {
                let path = self.pub_paths[id].clone();
                out.push((path.clone(), RT::Path { full: path, args: vec![] }));
            }
        }
        out
    }

    /// For each `pub use krate::Trait` of this crate whose definition is in `fdoc`: bind the
    /// trait's receiver-less methods on this crate's aliases of `krate`'s types, with `Self` the
    /// alias. Methods with a `self` receiver are one aggregated not-bound line.
    fn emit_foreign(&mut self, fdoc: &Value) {
        let Some(froot) = fdoc["root"].as_u64() else { return };
        let Some(krate) = fdoc["index"][froot.to_string()]["name"].as_str().map(String::from) else { return };
        let aliases = self.aliases_of(&krate);
        let uses: Vec<(String, Vec<String>)> = self
            .foreign_uses
            .iter()
            .filter(|(s, _)| s.split("::").next() == Some(krate.as_str()))
            .cloned()
            .collect();
        for (source, local_path) in uses {
            let tname = source.rsplit("::").next().unwrap_or("");
            let Some(tr) = fdoc["index"].as_object().into_iter().flat_map(|m| m.values()).find(|i| {
                kind(i) == "trait" && i["name"] == tname && i["visibility"] == "public"
            }) else {
                continue;
            };
            let methods: Vec<&Value> = tr["inner"]["trait"]["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| m.as_u64().and_then(|m| fdoc["index"].get(m.to_string())))
                .filter(|m| kind(m) == "function")
                .collect();
            let receiverless = |m: &Value| m["inner"]["function"]["sig"]["inputs"][0][0] != "self";
            for (apath, aty) in &aliases {
                // an alias that is a handle (SPEC 22.4) gets every method of the trait, the ones
                // with a `self` included
                let is_handle = self.ctx.handles.contains_key(apath);
                for m in methods.iter().filter(|m| is_handle || receiverless(m)) {
                    let owner = Owner {
                        self_ty: aty.clone(),
                        impl_generics: Value::Null,
                        type_path: apath.clone(),
                        trait_: Some((tname.to_string(), local_path.clone())),
                        foreign_trait: true,
                    };
                    let item = self.build_fn(fdoc, m, apath, Some(owner));
                    self.items.push(item);
                }
                if is_handle && !methods.is_empty() {
                    if let Some(entry) = self.alias_entries.get(apath).cloned() {
                        self.extra.retain(|e| e.name != entry);
                    }
                }
            }
            let with_self: Vec<&str> = methods.iter().filter(|m| !receiverless(m)).filter_map(|m| m["name"].as_str()).collect();
            // one entry per method and alias, so no name is hidden inside a list (SPEC 15.2)
            let ctx = self.ctx.clone();
            for (apath, _) in aliases.iter().filter(|(p, _)| !ctx.handles.contains_key(p)) {
                for m in &with_self {
                    let name = self.unique(format!("{}_{}", snake(&apath[1..].join("_")), snake(m)));
                    self.extra.push(NotBoundEntry {
                        name,
                        reason: format!(
                            "takes `self`, AgentMark cannot hold the value ({}::{} on {})",
                            local_path.join("::"),
                            m,
                            apath.join("::")
                        ),
                    });
                }
            }
        }
    }

    fn build_fn(&mut self, doc: &Value, f: &Value, path: &[String], owner: Option<Owner>) -> Item {
        let fname = f["name"].as_str().unwrap_or("f").to_string();
        let inner = &f["inner"]["function"];
        let display_path = match &owner {
            Some(Owner { foreign_trait: true, trait_: Some((_, tp)), .. }) => {
                format!("<{} as {}>::{}", path.join("::"), tp.join("::"), fname)
            }
            Some(_) => format!("{}::{}", path.join("::"), fname),
            None => path.join("::"),
        };
        // Name: path below the crate, snake-cased.
        let name_base = match &owner {
            Some(_) => format!("{}_{}", snake(&path[1..].join("_")), snake(&fname)),
            None => snake(&path[1..].join("_")),
        };
        let name = self.unique(name_base.trim_matches('_').to_string());
        let docs_raw = f["docs"].as_str().unwrap_or("");
        let summary = first_sentence(docs_raw);
        let docs = trimmed_docs(docs_raw);

        let self_ty = owner.as_ref().map(|o| o.self_ty.clone());
        let mut tp = TypeParser::new(doc, self_ty.clone());

        // Generics (fn and impl) -> bounds map and hazards.
        let mut bounds: Vec<(String, Vec<Bound>)> = Vec::new();
        let mut has_generics = false;
        let mut self_bound = false;
        let mut impl_generics = false;
        let ctx = self.ctx.clone();
        let mut gens = vec![&inner["generics"]];
        if let Some(o) = &owner {
            gens.push(&o.impl_generics);
        }
        for (gi, g) in gens.into_iter().enumerate() {
            for p in g["params"].as_array().into_iter().flatten() {
                let n = p["name"].as_str().unwrap_or("?").to_string();
                if p["kind"].get("lifetime").is_some() {
                    tp.hz.lifetimes = true;
                } else if let Some(t) = p["kind"].get("type") {
                    has_generics = true;
                    // the impl's own type parameters (`impl<Tz: TimeZone> … for DateTime<Tz>`)
                    impl_generics |= gi == 1;
                    let b = tp.parse_bounds(&t["bounds"]);
                    bounds.push((n, b));
                } else {
                    has_generics = true;
                    impl_generics |= gi == 1;
                }
            }
            for w in g["where_predicates"].as_array().into_iter().flatten() {
                if let Some(bp) = w.get("bound_predicate") {
                    let b = tp.parse_bounds(&bp["bounds"]);
                    match tp.parse(&bp["type"]) {
                        RT::Generic(n) => match bounds.iter_mut().find(|(x, _)| *x == n) {
                            Some((_, v)) => v.extend(b),
                            None => bounds.push((n, b)),
                        },
                        // `where Self: FixedOutputReset`: a condition on the type the method is
                        // bound on; only the type checker can tell whether it holds
                        other if owner.is_some() && Some(&other) == self_ty.as_ref() => self_bound = true,
                        _ => tp.hz.assoc = true,
                    }
                } else if w.get("lifetime_predicate").is_some() {
                    tp.hz.lifetimes = true;
                } else {
                    tp.hz.assoc = true;
                }
            }
        }

        // Receiver and parameters.
        let mut recv = Recv::None;
        let mut params: Vec<(String, RT)> = Vec::new();
        for (i, inp) in inner["sig"]["inputs"].as_array().into_iter().flatten().enumerate() {
            let pname = inp[0].as_str().unwrap_or("arg");
            let t = &inp[1];
            if i == 0 && pname == "self" && owner.is_some() {
                recv = if t["generic"] == "Self" {
                    Recv::Value
                } else if t["borrowed_ref"]["type"]["generic"] == "Self" {
                    if t["borrowed_ref"]["is_mutable"] == true { Recv::MutRef } else { Recv::Ref }
                } else {
                    Recv::Other
                };
                tp.parse(t);
                continue;
            }
            params.push((pname.to_string(), tp.parse(t)));
        }
        let ret_rt = if inner["sig"]["output"].is_null() {
            RT::Tuple(vec![])
        } else {
            tp.parse(&inner["sig"]["output"])
        };
        let is_async = inner["header"]["is_async"] == true;
        let is_unsafe = inner["header"]["is_unsafe"] == true;
        let hz = tp.hz.clone();

        // Sanitised unique parameter names (receiver first).
        let mut used: HashSet<String> = HashSet::new();
        let recv_name = self_ty.as_ref().map(|t| {
            let base = match t {
                RT::Path { full, .. } => snake(last(full)),
                _ => "value".into(),
            };
            fresh(&mut used, base)
        });
        let pnames: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(i, (n, _))| {
                let s = snake(n.trim_start_matches('_'));
                fresh(&mut used, if s.is_empty() { format!("arg{}", i + 1) } else { s })
            })
            .collect();

        let source_sig = render_sig(&fname, recv, &params, &ret_rt, &bounds, is_async);
        let contract_of = |sig: &Sig, h: &BTreeMap<String, String>| super::checker::rust_fn_header_with(&name, sig, h);
        let skipped = |reason: String| Shape::Skipped { reason };
        // Lifetimes and associated types keep an item out of reach unless it has a handle: a handle
        // owns whatever the Rust value borrows, and the assisted tier writes that glue.
        let borrowy = hz.lifetimes || hz.assoc;
        let borrow_reason = if hz.lifetimes { "lifetimes in the public API" } else { "associated types in the signature" };
        let fail = |msg: String| skipped(if borrowy { borrow_reason.to_string() } else { msg });
        let build = |catchall: bool| -> (Shape, Option<String>, BTreeMap<String, String>) {
        let lifter = Lifter { bounds: &bounds, ctx: &ctx, catchall, pending: RefCell::new(BTreeMap::new()) };
        // a method of a handle type is written with its method name; the handle tells methods apart
        let recv_handle = match recv {
            Recv::None | Recv::Other => None,
            _ => self_ty.as_ref().and_then(|t| lifter.handle_of(t)),
        };
        let heading = recv_handle.as_ref().map(|h| {
            let prefix = format!("{}_", snake(&h.split('.').skip(1).collect::<Vec<_>>().join("_")));
            name.strip_prefix(&prefix).unwrap_or(&name).to_string()
        });
        let shape = (|| {
            if is_unsafe {
                return skipped("unsafe function".into());
            }
            if hz.closure {
                return skipped("takes a closure and AgentMark has no function type".into());
            }
            match recv {
                Recv::Other => return skipped("unsupported receiver (self: Box<Self> or similar)".into()),
                Recv::MutRef if recv_handle.is_none() => return skipped("mutates its receiver (&mut self)".into()),
                _ => {}
            }
            let mut sparams: Vec<Param> = Vec::new();
            if let Some(h) = &recv_handle {
                let mode = match recv {
                    Recv::Value => HandleMode::Consume,
                    Recv::MutRef => HandleMode::Mut,
                    _ => HandleMode::Shared,
                };
                sparams.push(Param { name: "self".into(), ty: Ty::handle(h.clone(), mode) });
            } else if matches!(recv, Recv::Value | Recv::Ref) {
                let st = self_ty.as_ref().unwrap();
                match lifter.value(st) {
                    Ok(ty) => sparams.push(Param { name: recv_name.clone().unwrap(), ty }),
                    Err(e) => return fail(format!("method needs a value that AgentMark cannot hold: {e}")),
                }
            }
            for ((_, rt), pn) in params.iter().zip(&pnames) {
                match lifter.param(rt) {
                    Ok(ty) => sparams.push(Param { name: pn.clone(), ty }),
                    Err(e) => return fail(format!("parameter `{pn}`: {e}")),
                }
            }
            let ret = match lifter.ret(&ret_rt) {
                Ok(r) => r,
                Err(e) => return fail(format!("return type: {e}")),
            };
            let sig = Sig { params: sparams, ret };
            let handle_item = sig.params.iter().any(|p| matches!(p.ty, Ty::Handle { .. }))
                || matches!(sig.ret, Some(Ty::Handle { .. }));
            if borrowy && !handle_item {
                return skipped(borrow_reason.into());
            }

            // Direct tier: 1:1 types, no generics, no async, no Result wrapping. An item that
            // involves a handle has no model-free alternative, so its rules also take generics
            // with one obvious instantiation, another crate's trait and `where Self: …`; the
            // type checker confirms the body (`verify`).
            let mut why = Vec::new();
            // a handle item may be generic in its own parameters (`impl AsRef<[u8]>`), but not in
            // the type it is bound on: its path would need type arguments
            if has_generics && (!handle_item || impl_generics) {
                why.push("generic over a type with one obvious instantiation");
            }
            if borrowy {
                why.push("the handle owns what the Rust value borrows");
            }
            if is_async {
                why.push("async");
            }
            if Lifter::is_result(&ret_rt) {
                why.push("returns Result");
            }
            // Never trust rules for another crate's trait: the body is always type-checked.
            if owner.as_ref().is_some_and(|o| o.foreign_trait) && !handle_item {
                why.push("method of a trait from another crate");
            }
            let direct = if why.is_empty() {
                direct_body(
                    path,
                    &fname,
                    &owner,
                    recv,
                    recv_handle.is_some().then_some("self_").or(recv_name.as_deref()),
                    &params,
                    &pnames,
                    &ret_rt,
                    &lifter,
                    &sig,
                )
            } else {
                None
            };
            match direct {
                Some(body) => Shape::Direct {
                    sig,
                    body,
                    verify: handle_item || self_bound || owner.as_ref().is_some_and(|o| o.foreign_trait),
                },
                None => {
                    if why.is_empty() {
                        why.push("types do not map 1:1 onto AgentMark types");
                    }
                    Shape::Assisted { sig, why: why.join(", ") }
                }
            }
        })();
        (shape, heading, lifter.pending.into_inner())
        };
        // First as v0.2 lifted: a bound with several implementors has no obvious instantiation.
        // An item that fails that way may still be a handle item whose type parameter has exactly
        // one implementor in this crate (`DB: Database` with one database compiled in): try again
        // with that rule, and keep the result only when it really involves a handle.
        let (mut shape, mut heading, mut pending) = build(false);
        if matches!(shape, Shape::Skipped { .. }) {
            let (s2, h2, p2) = build(true);
            let handle_sig = |sh: &Shape| match sh {
                Shape::Direct { sig, .. } | Shape::Assisted { sig, .. } => {
                    sig.params.iter().any(|p| matches!(p.ty, Ty::Handle { .. })) || matches!(sig.ret, Some(Ty::Handle { .. }))
                }
                Shape::Skipped { .. } => false,
            };
            if handle_sig(&s2) {
                (shape, heading, pending) = (s2, h2, p2);
            }
        }
        // the Rust type of a handle is recorded only for an item that is bound or goes to the model
        if !matches!(shape, Shape::Skipped { .. }) {
            ctx.rust_types.borrow_mut().extend(pending.clone());
        }
        let contract = match &shape {
            Shape::Direct { sig, .. } | Shape::Assisted { sig, .. } => contract_of(sig, &ctx.rust_types.borrow()),
            Shape::Skipped { .. } => String::new(),
        };
        Item {
            name,
            heading,
            path: display_path,
            summary,
            docs,
            source_sig,
            contract,
            shape,
        }
    }
}

struct Owner {
    self_ty: RT,
    impl_generics: Value,
    type_path: Vec<String>,
    trait_: Option<(String, Vec<String>)>,
    /// The trait comes from another crate's rustdoc JSON (so `path` shows `<T as Trait>::m`).
    foreign_trait: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Recv {
    None,
    Value,
    Ref,
    MutRef,
    Other,
}

fn fresh(used: &mut HashSet<String>, base: String) -> String {
    let mut n = base.clone();
    let mut i = 2;
    while !used.insert(n.clone()) {
        n = format!("{base}_{i}");
        i += 1;
    }
    n
}

#[allow(clippy::too_many_arguments)]
fn direct_body(
    path: &[String],
    fname: &str,
    owner: &Option<Owner>,
    recv: Recv,
    recv_name: Option<&str>,
    params: &[(String, RT)],
    pnames: &[String],
    ret: &RT,
    lifter: &Lifter,
    sig: &Sig,
) -> Option<String> {
    // `sig.params` has the receiver first when the item has one that AgentMark holds
    let skip = sig.params.len() - params.len();
    let recv_handle = skip == 1 && matches!(&sig.params[0], Param { name, ty: Ty::Handle { .. } } if name == "self");
    let mut args = Vec::new();
    for (i, ((_, rt), pn)) in params.iter().zip(pnames).enumerate() {
        match &sig.params[skip + i].ty {
            // already the Rust parameter type (`&T`, `&mut T`, `T`): passed as it is
            Ty::Handle { .. } => args.push(pn.clone()),
            _ => args.push(conv_in(pn, &lifter.instantiated(rt))?),
        }
    }
    let body = |wide: bool| -> Option<String> {
        // arguments on a line of their own when the call would not fit a binding line (SPEC 18.3)
        let list = |a: &[String]| {
            if wide && !a.is_empty() {
                format!("\n{}\n", a.iter().map(|x| format!("    {x},")).collect::<Vec<_>>().join("\n"))
            } else {
                a.join(", ")
            }
        };
        let call = match owner {
            None => format!("{}({})", path.join("::"), list(&args)),
            Some(o) => match &o.trait_ {
                None => match recv {
                    Recv::None => format!("{}::{}({})", o.type_path.join("::"), fname, list(&args)),
                    _ => format!("{}.{}({})", recv_name?, fname, list(&args)),
                },
                Some((_, tp)) => {
                    let mut all = Vec::new();
                    match recv {
                        Recv::Value => all.push(recv_name?.to_string()),
                        Recv::Ref if recv_handle => all.push(recv_name?.to_string()),
                        Recv::Ref => all.push(format!("&{}", recv_name?)),
                        Recv::MutRef => all.push(recv_name?.to_string()),
                        _ => {}
                    }
                    all.extend(args.iter().cloned());
                    format!("<{} as {}>::{}({})", o.type_path.join("::"), tp.join("::"), fname, list(&all))
                }
            },
        };
        Some(match (ret, &sig.ret) {
            (RT::Tuple(v), _) if v.is_empty() => format!("{call};\nOk(())"),
            // a handle comes back as the library gave it
            (_, Some(Ty::Handle { .. })) => format!("Ok({call})"),
            (RT::Path { full, .. }, Some(Ty::Bytes)) if matches!(last(full), "Output" | "GenericArray") => {
                format!("Ok({call}.to_vec())")
            }
            _ => format!("Ok({})", conv_out(&call, ret)?),
        })
    };
    let one = body(false)?;
    // the binding file indents the body by two
    if one.lines().any(|l| l.chars().count() + 2 > 118) {
        // still too long (a nested conversion): the model writes it in more lines
        body(true).filter(|b| b.lines().all(|l| l.chars().count() + 2 <= 118))
    } else {
        Some(one)
    }
}

fn render_sig(
    name: &str,
    recv: Recv,
    params: &[(String, RT)],
    ret: &RT,
    bounds: &[(String, Vec<Bound>)],
    is_async: bool,
) -> String {
    let mut ps = Vec::new();
    match recv {
        Recv::Value => ps.push("self".to_string()),
        Recv::Ref => ps.push("&self".to_string()),
        Recv::MutRef => ps.push("&mut self".to_string()),
        Recv::Other => ps.push("self: _".to_string()),
        Recv::None => {}
    }
    for (n, t) in params {
        ps.push(format!("{n}: {}", render(t)));
    }
    let generics: Vec<String> = bounds
        .iter()
        .map(|(n, b)| {
            let b: Vec<Bound> = b.iter().filter(|x| !x.marker).cloned().collect();
            if b.is_empty() { n.clone() } else { format!("{n}: {}", render_bounds(&b)) }
        })
        .collect();
    let g = if generics.is_empty() { String::new() } else { format!("<{}>", generics.join(", ")) };
    let r = match ret {
        RT::Tuple(v) if v.is_empty() => String::new(),
        _ => format!(" -> {}", render(ret)),
    };
    format!("pub {}fn {name}{g}({}){r}", if is_async { "async " } else { "" }, ps.join(", "))
}

/// The function the body is placed in (also what the scratch type-check compiles).
pub fn contract(name: &str, sig: &Sig) -> String {
    super::checker::rust_fn_header(name, sig)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_handles_acronyms_and_digits() {
        assert_eq!(snake("NaiveDate"), "naive_date");
        assert_eq!(snake("IsoWeek"), "iso_week");
        assert_eq!(snake("HTTPClient"), "http_client");
        assert_eq!(snake("Sha256VarCore"), "sha256_var_core");
        assert_eq!(snake("from_ymd_opt"), "from_ymd_opt");
    }

    #[test]
    fn first_sentence_strips_links_and_stops_at_the_period() {
        assert_eq!(
            first_sentence("Parses a [`NaiveDate`](crate::NaiveDate) from text. More detail.\n\nOther."),
            "Parses a `NaiveDate` from text."
        );
        assert_eq!(first_sentence("see [x][1]\n```\ncode\n```"), "see x.");
        assert_eq!(first_sentence(""), "");
    }
}
