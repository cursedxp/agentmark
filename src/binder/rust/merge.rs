//! Merging the rustdoc JSON of the crates a facade re-exports from (Q411).
//!
//! A facade crate such as `sqlx` defines almost nothing itself: `pub use sqlx_core::query::query;`
//! is an item whose target is only a path in the facade's rustdoc JSON. The binder follows such a
//! re-export by merging the other crate's JSON into the facade's, so that every later step sees one
//! crate: item ids of the other crate are shifted into their own range, and an id that a crate only
//! knows as an external path (`sqlx_core::query::query`) is replaced by the id of the item that has
//! that path in whichever loaded crate defines it.

use serde_json::{Map, Value};
use std::collections::HashMap;

/// Ids of crate number `n` (0 is the facade) live in `n * RANGE ..`.
const RANGE: u64 = 10_000_000;

/// Keys whose value is an id, or a list of ids.
const ID_LISTS: &[&str] = &["items", "impls", "fields", "variants", "implementations", "tuple"];

/// Name of the crate a rustdoc JSON describes.
pub fn root_name(doc: &Value) -> Option<String> {
    let root = doc["root"].as_u64()?;
    doc["index"][root.to_string()]["name"].as_str().map(String::from)
}

fn path_key(kind: &str, path: &Value) -> Option<String> {
    let p: Vec<&str> = path.as_array()?.iter().filter_map(Value::as_str).collect();
    (!p.is_empty()).then(|| format!("{kind}:{}", p.join("::")))
}

struct Doc {
    value: Value,
    offset: u64,
}

impl Doc {
    fn knows(&self, id: u64) -> bool {
        self.value["index"].get(id.to_string()).is_some()
    }

    /// The id `id` of this crate's JSON has in the merged one.
    fn global(&self, id: u64, registry: &HashMap<String, u64>) -> u64 {
        if !self.knows(id) {
            let p = &self.value["paths"][id.to_string()];
            if p["crate_id"].as_u64().is_some_and(|c| c != 0) {
                if let Some(g) = path_key(p["kind"].as_str().unwrap_or(""), &p["path"]).and_then(|k| registry.get(&k)) {
                    return *g;
                }
            }
        }
        id + self.offset
    }
}

fn remap(v: &mut Value, doc: &Doc, registry: &HashMap<String, u64>) {
    let map_num = |n: &mut Value| {
        if let Some(i) = n.as_u64() {
            *n = Value::from(doc.global(i, registry));
        }
    };
    match v {
        Value::Object(o) => {
            for (k, child) in o.iter_mut() {
                if k == "id" {
                    map_num(child);
                } else if ID_LISTS.contains(&k.as_str()) && child.is_array() {
                    for n in child.as_array_mut().unwrap() {
                        map_num(n);
                    }
                } else if k == "links" && child.is_object() {
                    for (_, n) in child.as_object_mut().unwrap() {
                        map_num(n);
                    }
                } else {
                    remap(child, doc, registry);
                }
            }
        }
        Value::Array(a) => {
            for c in a {
                remap(c, doc, registry);
            }
        }
        _ => {}
    }
}

/// `main` with the items of `others` added; ids that point at another loaded crate now point at the
/// item itself. The root stays the facade's.
pub fn merge(main: Value, others: Vec<Value>) -> Value {
    let mut docs: Vec<Doc> = std::iter::once(main).chain(others).enumerate().map(|(n, value)| Doc { value, offset: n as u64 * RANGE }).collect();
    // path -> merged id of every item some loaded crate defines (the facade first: it wins a clash)
    let mut registry: HashMap<String, u64> = HashMap::new();
    for d in &docs {
        let Some(paths) = d.value["paths"].as_object() else { continue };
        for (id, p) in paths {
            let Ok(i) = id.parse::<u64>() else { continue };
            if p["crate_id"] == 0 && d.knows(i) {
                if let Some(k) = path_key(p["kind"].as_str().unwrap_or(""), &p["path"]) {
                    registry.entry(k).or_insert(i + d.offset);
                }
            }
        }
    }
    let mut index = Map::new();
    let mut paths = Map::new();
    let mut externals = Map::new();
    let mut rest = Value::Null;
    for (n, d) in docs.iter_mut().enumerate() {
        let mut value = std::mem::take(&mut d.value);
        // `remap` reads this crate's old tables, so it works on a copy of the parts it changes
        let old = Doc { value: value.clone(), offset: d.offset };
        let mut idx = value["index"].take();
        remap(&mut idx, &old, &registry);
        if let Some(o) = idx.as_object() {
            for (_, item) in o {
                if let Some(g) = item["id"].as_u64() {
                    index.insert(g.to_string(), item.clone());
                }
            }
        }
        if let Some(o) = old.value["paths"].as_object() {
            for (id, p) in o {
                let Ok(i) = id.parse::<u64>() else { continue };
                let g = old.global(i, &registry);
                // an external path that some loaded crate defines is that crate's own entry
                if p["crate_id"].as_u64().is_some_and(|c| c != 0) && g != i + d.offset {
                    continue;
                }
                let mut p = p.clone();
                if n > 0 && p["crate_id"].as_u64().is_some_and(|c| c != 0) {
                    p["crate_id"] = Value::from(p["crate_id"].as_u64().unwrap() + d.offset);
                }
                // the facade's own entry wins; an item found elsewhere keeps the defining crate's entry
                paths.entry(g.to_string()).or_insert(p);
            }
        }
        if let Some(o) = old.value["external_crates"].as_object() {
            for (cid, c) in o {
                let Ok(c0) = cid.parse::<u64>() else { continue };
                externals.insert((if n > 0 { c0 + d.offset } else { c0 }).to_string(), c.clone());
            }
        }
        if n == 0 {
            rest = value;
        }
        d.value = Value::Null;
    }
    rest["index"] = Value::Object(index);
    rest["paths"] = Value::Object(paths);
    rest["external_crates"] = Value::Object(externals);
    rest
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn facade() -> Value {
        json!({
            "root": 0, "format_version": 61,
            "index": {
                "0": {"id": 0, "name": "facade", "inner": {"module": {"items": [1]}}},
                "1": {"id": 1, "name": "go", "visibility": "public", "inner": {"use": {"source": "base::go", "name": "go", "id": 7, "is_glob": false}}}
            },
            "paths": {"7": {"crate_id": 1, "path": ["base", "go"], "kind": "function"}},
            "external_crates": {"1": {"name": "base"}}
        })
    }

    fn base() -> Value {
        json!({
            "root": 0, "format_version": 61,
            "index": {
                "0": {"id": 0, "name": "base", "inner": {"module": {"items": [3]}}},
                "3": {"id": 3, "name": "go", "visibility": "public", "inner": {"function": {"sig": {"inputs": [], "output": {"resolved_path": {"id": 4, "path": "Thing"}}}}}},
                "4": {"id": 4, "name": "Thing", "visibility": "public", "inner": {"struct": {"impls": []}}}
            },
            "paths": {"3": {"crate_id": 0, "path": ["base", "go"], "kind": "function"}, "4": {"crate_id": 0, "path": ["base", "Thing"], "kind": "struct"}},
            "external_crates": {}
        })
    }

    #[test]
    fn a_reexport_points_at_the_item_of_the_other_crate() {
        let m = merge(facade(), vec![base()]);
        // the use now names the function, which lives in the second range
        assert_eq!(m["index"]["1"]["inner"]["use"]["id"], RANGE + 3);
        assert_eq!(m["index"][(RANGE + 3).to_string()]["name"], "go");
        // ids inside the other crate moved with it
        assert_eq!(m["index"][(RANGE + 3).to_string()]["inner"]["function"]["sig"]["output"]["resolved_path"]["id"], RANGE + 4);
        assert_eq!(m["paths"][(RANGE + 4).to_string()]["path"], json!(["base", "Thing"]));
        assert_eq!(m["root"], 0);
    }
}
