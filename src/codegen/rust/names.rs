//! Declared names -> Rust identifiers, deterministic and collision free.
//!
//! The IR keeps the names the author wrote (`invoice-chaser__remind`, `Every day at 09:00`).
//! Everything that becomes a Rust item gets one entry here, decided once, before any code is
//! written, so every call site agrees. A second item that maps to the same identifier in the same
//! namespace gets `_2`, `_3`, ... in program order.

use std::collections::{HashMap, HashSet};

use crate::ir::Program;

const KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
    "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
    "trait", "true", "type", "unsafe", "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box",
    "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];

/// Type names the generated files use unqualified; a record may not take them.
const RESERVED_TYPES: &[&str] = &[
    "String", "Vec", "Option", "Some", "None", "Result", "Ok", "Err", "Box", "Money", "Currency", "Ctx", "Duration",
    "NaiveDate", "DateTime", "Utc", "Value", "AgentError", "Trace", "Arc", "Mutex", "Reply", "App", "Router", "Cli",
    "Cmd", "Self", "Default", "Clone", "Debug", "Serialize", "Deserialize", "Weekday", "Schedule", "RtError",
];

/// Module names the generated files use.
const RESERVED_MODULES: &[&str] = &["records", "rules", "tools", "bindings", "main", "std", "core", "alloc"];

pub fn is_keyword(s: &str) -> bool {
    KEYWORDS.contains(&s)
}

/// `a-b c` -> `a_b_c`: letters, digits and `_` stay, everything else becomes `_`. Never empty,
/// never starts with a digit.
pub fn ident_chars(s: &str) -> String {
    let mut out: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if out.is_empty() {
        out.push('_');
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// A lower-case slug of a heading: `Every day at 09:00` -> `every_day_at_09_00`.
pub fn slug(label: &str) -> String {
    let mut out = String::new();
    let mut last_us = true;
    for c in label.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_us = false;
        } else if !last_us {
            out.push('_');
            last_us = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("block");
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert_str(0, "entry_");
    }
    out
}

/// `my-record` -> `MyRecord`; `AppState` stays.
pub fn camel(s: &str) -> String {
    let mut out = String::new();
    for part in s.split(|c: char| !c.is_ascii_alphanumeric()).filter(|p| !p.is_empty()) {
        let mut cs = part.chars();
        if let Some(f) = cs.next() {
            out.push(f.to_ascii_uppercase());
            out.push_str(cs.as_str());
        }
    }
    if out.is_empty() {
        out.push('X');
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, 'X');
    }
    out
}

/// A value-level identifier (variable, parameter, field, function): keywords get a trailing `_`.
pub fn value_ident(s: &str) -> String {
    let id = ident_chars(s);
    if is_keyword(&id) || id == "ctx" {
        format!("{id}_")
    } else {
        id
    }
}

/// Hands out unique identifiers in one namespace.
#[derive(Default)]
pub struct Namespace {
    used: HashSet<String>,
}

impl Namespace {
    pub fn with(reserved: &[&str]) -> Self {
        Namespace { used: reserved.iter().map(|s| s.to_string()).collect() }
    }
    pub fn take(&mut self, base: &str) -> String {
        let mut base = base.to_string();
        if is_keyword(&base) {
            base.push('_');
        }
        let mut cand = base.clone();
        let mut n = 2;
        while !self.used.insert(cand.clone()) {
            cand = format!("{base}_{n}");
            n += 1;
        }
        cand
    }
}

/// Where a function lives and what it is called there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnName {
    /// `rules` or `agent_<name>`.
    pub module: String,
    pub ident: String,
}

#[derive(Debug, Default)]
pub struct Names {
    /// Record name -> struct name.
    pub records: HashMap<String, String>,
    /// Enum name -> Rust enum name (same namespace as the records).
    pub enums: HashMap<String, String>,
    /// (enum, variant) -> name of the struct that holds the variant's fields (only variants
    /// with fields have one).
    pub variant_structs: HashMap<(String, String), String>,
    /// (enum, variant) -> the variant's identifier inside the Rust enum.
    pub variant_idents: HashMap<(String, String), String>,
    /// Declared function name -> home.
    pub fns: HashMap<String, FnName>,
    /// Per `Program::entries` index.
    pub entries: Vec<String>,
    /// Agent unit -> module (`agent_invoice_chaser`), in order of first appearance.
    pub units: Vec<(String, String)>,
    /// Per `Program::services` index.
    pub services: Vec<String>,
    /// Per `Program::commands` index.
    pub commands: Vec<String>,
    /// `## Rust` block name -> fn name in `bindings`.
    pub rust_fns: HashMap<String, String>,
    /// Binding name -> struct name in `bindings`.
    pub bindings: HashMap<String, String>,
    /// (binding, item) -> method name.
    pub binding_items: HashMap<(String, String), String>,
    /// Tool name -> trait name in `tools`.
    pub tools: HashMap<String, String>,
    /// Filter rule name -> `count_<..>` helper in `rules`.
    pub counters: HashMap<String, String>,
    /// Every value-level identifier that names an item; locals must not reuse them.
    pub item_idents: HashSet<String>,
}

impl Names {
    pub fn build(p: &Program) -> Names {
        let mut n = Names::default();

        let mut types = Namespace::with(RESERVED_TYPES);
        for r in &p.records {
            n.records.insert(r.name.clone(), types.take(&camel(&r.name)));
        }
        for e in &p.enums {
            let en = types.take(&camel(&e.name));
            let mut idents = Namespace::default();
            for v in &e.variants {
                n.variant_idents.insert((e.name.clone(), v.name.clone()), idents.take(&camel(&v.name)));
                if !v.fields.is_empty() {
                    let st = types.take(&format!("{en}{}", camel(&v.name)));
                    n.variant_structs.insert((e.name.clone(), v.name.clone()), st);
                }
            }
            n.enums.insert(e.name.clone(), en);
        }

        // module names
        let mut modules = Namespace::with(RESERVED_MODULES);
        let mut unit_names: Vec<String> = Vec::new();
        for e in &p.entries {
            if !unit_names.contains(&e.unit) {
                unit_names.push(e.unit.clone());
            }
        }
        for f in &p.functions {
            if let Some((u, _)) = &f.display {
                if !unit_names.contains(u) {
                    unit_names.push(u.clone());
                }
            }
        }
        for u in &unit_names {
            n.units.push((u.clone(), modules.take(&format!("agent_{}", ident_chars(u).to_ascii_lowercase()))));
        }
        for s in &p.services {
            n.services.push(modules.take(&format!("service_{}", ident_chars(&s.name).to_ascii_lowercase())));
        }
        for c in &p.commands {
            n.commands.push(modules.take(&format!("command_{}", ident_chars(&c.name).to_ascii_lowercase())));
        }

        // entries first (they read like headings), then the functions of each module
        let mut per_module: HashMap<String, Namespace> = HashMap::new();
        per_module.insert("rules".into(), Namespace::default());
        for (_, m) in &n.units {
            per_module.insert(m.clone(), Namespace::default());
        }
        for e in &p.entries {
            let module = n.module_of_unit(&e.unit).to_string();
            let id = per_module.get_mut(&module).map(|ns| ns.take(&slug(&e.label))).unwrap_or_default();
            n.entries.push(id);
        }
        for f in &p.functions {
            let (module, base) = match &f.display {
                Some((u, label)) => {
                    let block = label.strip_prefix("Do ").unwrap_or(label);
                    (n.module_of_unit(u).to_string(), slug(block))
                }
                None => ("rules".to_string(), ident_chars(&f.name)),
            };
            let ident = per_module.get_mut(&module).map(|ns| ns.take(&base)).unwrap_or_default();
            n.fns.insert(f.name.clone(), FnName { module, ident });
        }
        // `count of <filter> x in src` helpers live in rules next to the filters
        for f in &p.functions {
            if f.display.is_none() && f.ret == Some(crate::ir::Type::Bool) && f.params.len() == 1 {
                let ns = per_module.get_mut("rules").expect("rules namespace");
                let id = ns.take(&format!("count_{}", ident_chars(&f.name)));
                n.counters.insert(f.name.clone(), id);
            }
        }

        // bindings.rs: one namespace for Rust blocks and binding structs
        let mut bind_ns = Namespace::with(RESERVED_TYPES);
        for r in &p.rust_fns {
            n.rust_fns.insert(r.name.clone(), bind_ns.take(&ident_chars(&r.name)));
        }
        for b in &p.bindings {
            if !n.bindings.contains_key(&b.binding) {
                n.bindings.insert(b.binding.clone(), bind_ns.take(&camel(&b.binding)));
            }
        }
        let mut item_ns: HashMap<String, Namespace> = HashMap::new();
        for b in &p.bindings {
            let ns = item_ns.entry(b.binding.clone()).or_default();
            n.binding_items.insert((b.binding.clone(), b.item.clone()), ns.take(&ident_chars(&b.item)));
        }

        let mut tools_ns = Namespace::with(RESERVED_TYPES);
        for t in &p.tools {
            n.tools.insert(t.name.clone(), tools_ns.take(&camel(&t.name)));
        }

        for f in n.fns.values() {
            n.item_idents.insert(f.ident.clone());
        }
        for e in &n.entries {
            n.item_idents.insert(e.clone());
        }
        for v in n.rust_fns.values().chain(n.counters.values()) {
            n.item_idents.insert(v.clone());
        }
        n
    }

    pub fn module_of_unit(&self, unit: &str) -> &str {
        self.units.iter().find(|(u, _)| u == unit).map(|(_, m)| m.as_str()).unwrap_or("rules")
    }

    /// A local variable's identifier: never a keyword, `ctx`, or the name of an item in scope.
    pub fn local(&self, name: &str) -> String {
        let id = value_ident(name);
        if self.item_idents.contains(&id) {
            format!("{id}_")
        } else {
            id
        }
    }

    pub fn enum_name(&self, name: &str) -> String {
        self.enums.get(name).cloned().unwrap_or_else(|| camel(name))
    }

    pub fn variant_struct(&self, e: &str, v: &str) -> String {
        self.variant_structs
            .get(&(e.to_string(), v.to_string()))
            .cloned()
            .unwrap_or_else(|| format!("{}{}", camel(e), camel(v)))
    }

    pub fn variant_ident(&self, e: &str, v: &str) -> String {
        self.variant_idents
            .get(&(e.to_string(), v.to_string()))
            .cloned()
            .unwrap_or_else(|| camel(v))
    }

    /// Every Rust type declared in `records.rs`: records, enums and the structs of variants.
    pub fn record_file_types(&self) -> impl Iterator<Item = &str> {
        self.records
            .values()
            .chain(self.enums.values())
            .chain(self.variant_structs.values())
            .map(String::as_str)
    }

    pub fn record(&self, name: &str) -> String {
        self.records.get(name).cloned().unwrap_or_else(|| camel(name))
    }
}
