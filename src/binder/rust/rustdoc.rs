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
use crate::binder::core::{CrateItems, EffectHints, Item, NotBoundEntry, Param, Shape, Sig};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

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
        network: s(&["reqwest", "tokio::net", "std::net", "hyper", "ureq", "isahc", "surf", "TcpStream", "UdpSocket"]),
        local: s(&["std::fs", "tokio::fs", "std::process", "std::env", "std::io::stdin", "std::path::Path"]),
    }
}

const STYLE_GUIDE: &str = "\
- The body returns `Ok(value)` or fails with `?`; errors are boxed (`Box<dyn std::error::Error + Send + Sync>`).\n\
- AgentMark values arrive owned: text = String, int = i64, bool, bytes = Vec<u8>, date = chrono::NaiveDate, \
datetime = chrono::DateTime<chrono::Utc>, duration = chrono::Duration, list of T = Vec<T>, maybe T = Option<T>.\n\
- Convert integers with `try_from(..)?`, never `as`. No floats. No `unwrap`/`expect`/`panic!`.\n\
- Refer to the library by full path (`chrono::NaiveDate::from_ymd_opt`); the body has no `use` lines.\n\
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
    extra: Vec<NotBoundEntry>,
    names: HashSet<String>,
    items: Vec<Item>,
}

pub fn read_crate(json: &str) -> Result<CrateItems, String> {
    let doc: Value = serde_json::from_str(json).map_err(|e| format!("rustdoc JSON is not valid JSON: {e}"))?;
    let fv = doc["format_version"].as_u64().unwrap_or(0);
    if fv != SUPPORTED_FORMAT {
        return Err(format!(
            "rustdoc JSON format_version {fv} is not supported (this binder reads {SUPPORTED_FORMAT}); \
             regenerate it with the nightly this binder was built for"
        ));
    }
    let root_id = doc["root"].as_u64().ok_or("rustdoc JSON has no root")?;
    let root = &doc["index"][root_id.to_string()];
    let name = root["name"].as_str().ok_or("rustdoc root has no name")?.to_string();
    let version = doc["crate_version"]
        .as_str()
        .ok_or("rustdoc JSON has no crate_version (was it built from a published crate?)")?
        .to_string();
    let mut w = Walk {
        doc: &doc,
        crate_ident: name.replace('-', "_"),
        pub_paths: HashMap::new(),
        order: Vec::new(),
        seen_modules: HashSet::new(),
        extra: Vec::new(),
        names: HashSet::new(),
        items: Vec::new(),
    };
    w.seen_modules.insert(root_id);
    let prefix = vec![w.crate_ident.clone()];
    w.walk_module(root_id, &prefix);
    w.emit_all();
    Ok(CrateItems {
        name,
        version,
        language: "Rust".into(),
        style_guide: STYLE_GUIDE.into(),
        code_lang: "rust".into(),
        hints: effect_hints(),
        items: w.items,
        extra_not_bound: w.extra,
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
                let base = if glob { format!("reexport_glob_{source}") } else { format!("reexport_{uname}") };
                let name = self.unique(snake(&base));
                self.extra.push(NotBoundEntry {
                    name,
                    reason: format!("re-export of `{source}` from another crate; bind that crate to use it"),
                });
            }
        }
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
                    let item = self.build_fn(id, &path, None);
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
                        let ty = TypeParser::new(self.doc, None).parse(&it["inner"]["type_alias"]["type"]);
                        self.extra.push(NotBoundEntry {
                            name,
                            reason: format!(
                                "{} is a type alias for `{}` from another crate; its methods live there",
                                path.join("::"),
                                render(&ty)
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
                    impl_item: imp,
                    type_path: tpath.to_vec(),
                    trait_: trait_.clone(),
                };
                let path = tpath.to_vec();
                let item = self.build_fn(fid, &path, Some(owner));
                self.items.push(item);
            }
        }
    }

    fn build_fn(&mut self, fid: u64, path: &[String], owner: Option<Owner>) -> Item {
        let doc = self.doc;
        let f = self.get(fid).unwrap();
        let fname = f["name"].as_str().unwrap_or("f").to_string();
        let inner = &f["inner"]["function"];
        let display_path = match &owner {
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

        let self_ty = owner.as_ref().map(|o| {
            let mut p = TypeParser::new(doc, None);
            p.parse(&o.impl_item["inner"]["impl"]["for"])
        });
        let mut tp = TypeParser::new(doc, self_ty.clone());

        // Generics (fn and impl) -> bounds map and hazards.
        let mut bounds: Vec<(String, Vec<Bound>)> = Vec::new();
        let mut has_generics = false;
        let mut gens = vec![&inner["generics"]];
        if let Some(o) = &owner {
            gens.push(&o.impl_item["inner"]["impl"]["generics"]);
        }
        for g in gens {
            for p in g["params"].as_array().into_iter().flatten() {
                let n = p["name"].as_str().unwrap_or("?").to_string();
                if p["kind"].get("lifetime").is_some() {
                    tp.hz.lifetimes = true;
                } else if let Some(t) = p["kind"].get("type") {
                    has_generics = true;
                    let b = tp.parse_bounds(&t["bounds"]);
                    bounds.push((n, b));
                } else {
                    has_generics = true;
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
        let lifter = Lifter { bounds: &bounds };
        let contract_of = |sig: &Sig| contract(&name, sig);

        let skipped = |reason: String| Shape::Skipped { reason };
        let shape = (|| {
            if is_unsafe {
                return skipped("unsafe function".into());
            }
            if hz.lifetimes {
                return skipped("lifetimes in the public API".into());
            }
            if hz.assoc {
                return skipped("associated types in the signature".into());
            }
            if hz.closure {
                return skipped("takes a closure and AgentMark has no function type".into());
            }
            match recv {
                Recv::Other => return skipped("unsupported receiver (self: Box<Self> or similar)".into()),
                Recv::MutRef => return skipped("mutates its receiver (&mut self)".into()),
                _ => {}
            }
            let mut sparams: Vec<Param> = Vec::new();
            if matches!(recv, Recv::Value | Recv::Ref) {
                let st = self_ty.as_ref().unwrap();
                match lifter.value(st) {
                    Ok(ty) => sparams.push(Param { name: recv_name.clone().unwrap(), ty }),
                    Err(e) => return skipped(format!("method needs a value that AgentMark cannot hold: {e}")),
                }
            }
            for ((_, rt), pn) in params.iter().zip(&pnames) {
                match lifter.value(rt) {
                    Ok(ty) => sparams.push(Param { name: pn.clone(), ty }),
                    Err(e) => return skipped(format!("parameter `{pn}`: {e}")),
                }
            }
            let ret = match lifter.ret(&ret_rt) {
                Ok(r) => r,
                Err(e) => return skipped(format!("return type: {e}")),
            };
            let sig = Sig { params: sparams, ret };

            // Direct tier: 1:1 types, no generics, no async, no Result wrapping.
            let mut why = Vec::new();
            if has_generics {
                why.push("generic over a type with one obvious instantiation");
            }
            if is_async {
                why.push("async");
            }
            if Lifter::is_result(&ret_rt) {
                why.push("returns Result");
            }
            let direct = if why.is_empty() {
                direct_body(path, &fname, &owner, recv, recv_name.as_deref(), &params, &pnames, &ret_rt)
            } else {
                None
            };
            match direct {
                Some(body) => Shape::Direct { sig, body },
                None => {
                    if why.is_empty() {
                        why.push("types do not map 1:1 onto AgentMark types");
                    }
                    Shape::Assisted { sig, why: why.join(", ") }
                }
            }
        })();

        let contract = match &shape {
            Shape::Direct { sig, .. } | Shape::Assisted { sig, .. } => contract_of(sig),
            Shape::Skipped { .. } => String::new(),
        };
        Item {
            name,
            path: display_path,
            summary,
            docs,
            source_sig,
            contract,
            shape,
        }
    }
}

struct Owner<'a> {
    impl_item: &'a Value,
    type_path: Vec<String>,
    trait_: Option<(String, Vec<String>)>,
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
) -> Option<String> {
    let mut args = Vec::new();
    for ((_, rt), pn) in params.iter().zip(pnames) {
        args.push(conv_in(pn, rt)?);
    }
    let call = match owner {
        None => format!("{}({})", path.join("::"), args.join(", ")),
        Some(o) => match &o.trait_ {
            None => match recv {
                Recv::None => format!("{}::{}({})", o.type_path.join("::"), fname, args.join(", ")),
                _ => format!("{}.{}({})", recv_name?, fname, args.join(", ")),
            },
            Some((_, tp)) => {
                let mut all = Vec::new();
                match recv {
                    Recv::Value => all.push(recv_name?.to_string()),
                    Recv::Ref => all.push(format!("&{}", recv_name?)),
                    _ => {}
                }
                all.extend(args);
                format!("<{} as {}>::{}({})", o.type_path.join("::"), tp.join("::"), fname, all.join(", "))
            }
        },
    };
    Some(match ret {
        RT::Tuple(v) if v.is_empty() => format!("{call};\nOk(())"),
        _ => format!("Ok({})", conv_out(&call, ret)?),
    })
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
