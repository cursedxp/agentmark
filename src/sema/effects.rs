//! Effect sets (SPEC 8, DESIGN 3): what a statement, block or function can touch.
//!
//! The lattice is `pure < local < network < model`. Calls into bindings that nobody approved yet
//! are tracked separately as `unverified` (SPEC 15.3); for the "does the body exceed its
//! declared effect" test an unverified item counts like `network`.
//!
//! People read a set as the list of things it touches (SPEC 23.7): `touches nothing`,
//! `touches AI, crm, email, log`. The class words stay inside the compiler and `check --json`.

use std::fmt;

use serde::Serialize;

use crate::syntax::ast::Effect;

/// Which tools, bindings and built-ins a piece of code touches, grouped by effect class.
/// Names keep first-use order so the summary reads like the code: `network(email, crm)`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EffectSet {
    /// Asks a model.
    pub model: bool,
    /// Tools (and `network` functions, Rust blocks, bindings) that reach the outside.
    pub network: Vec<String>,
    /// Tools and built-ins (`print`, `file`, `state`) that touch only this machine.
    pub local: Vec<String>,
    /// Bindings whose effect nobody approved yet.
    pub unverified: Vec<String>,
}

fn add(list: &mut Vec<String>, name: &str) {
    if !list.iter().any(|n| n == name) {
        list.push(name.to_string());
    }
}

/// Numeric height in the lattice; `unverified` counts like `network`.
pub fn rank(e: Effect) -> u8 {
    match e {
        Effect::Pure => 0,
        Effect::Local => 1,
        Effect::Network | Effect::Unverified => 2,
        Effect::Model => 3,
    }
}

pub fn effect_name(e: Effect) -> &'static str {
    match e {
        Effect::Pure => "pure",
        Effect::Local => "local",
        Effect::Network => "network",
        Effect::Model => "model",
        Effect::Unverified => "unverified",
    }
}

impl EffectSet {
    pub fn pure() -> Self {
        EffectSet::default()
    }

    /// A set made of one source of the given class: `from_class(Network, "crm")`.
    pub fn of(class: Effect, source: &str) -> Self {
        let mut s = EffectSet::default();
        s.add(class, source);
        s
    }

    pub fn add(&mut self, class: Effect, source: &str) {
        match class {
            Effect::Pure => {}
            Effect::Local => add(&mut self.local, source),
            Effect::Network => add(&mut self.network, source),
            Effect::Model => self.model = true,
            Effect::Unverified => add(&mut self.unverified, source),
        }
    }

    pub fn merge(&mut self, other: &EffectSet) {
        self.model |= other.model;
        for n in &other.network {
            add(&mut self.network, n);
        }
        for n in &other.local {
            add(&mut self.local, n);
        }
        for n in &other.unverified {
            add(&mut self.unverified, n);
        }
    }

    pub fn is_pure(&self) -> bool {
        !self.model
            && self.network.is_empty()
            && self.local.is_empty()
            && self.unverified.is_empty()
    }

    /// The highest class present. `Unverified` is returned only when nothing above `network`
    /// is present and an unverified item is.
    pub fn class(&self) -> Effect {
        if self.model {
            Effect::Model
        } else if !self.network.is_empty() {
            Effect::Network
        } else if !self.unverified.is_empty() {
            Effect::Unverified
        } else if !self.local.is_empty() {
            Effect::Local
        } else {
            Effect::Pure
        }
    }

    /// Does this set go beyond what `declared` allows?
    pub fn exceeds(&self, declared: Effect) -> bool {
        rank(self.class()) > rank(declared)
    }

    /// The tools/sources behind the highest class, for messages like "network (email, crm)".
    pub fn detail(&self) -> String {
        let (name, list): (&str, &Vec<String>) = match self.class() {
            Effect::Model => return "model".to_string(),
            Effect::Network => ("network", &self.network),
            Effect::Unverified => ("unverified", &self.unverified),
            Effect::Local => ("local", &self.local),
            Effect::Pure => return "pure".to_string(),
        };
        format!("{name} ({})", list.join(", "))
    }
}

/// The word a person reads for an internal source name: the model is `AI`, the `file` tool
/// `files` (SPEC 23.7).
pub fn touch_word(source: &str) -> String {
    match source {
        "model" => "AI".to_string(),
        "file" => "files".to_string(),
        other => other.to_string(),
    }
}

/// The internal source name of a word written after `touches:`; `None` for a word that names no
/// built-in (a tool, a library).
pub fn builtin_touch(word: &str) -> Option<(Effect, &'static str)> {
    match word.to_ascii_lowercase().as_str() {
        "ai" => Some((Effect::Model, "model")),
        "files" | "file" => Some((Effect::Local, "file")),
        "print" => Some((Effect::Local, "print")),
        "input" => Some((Effect::Local, "input")),
        "state" => Some((Effect::Local, "state")),
        _ => None,
    }
}

impl EffectSet {
    /// Every source in the set (`model` included), internal names.
    pub fn sources(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.model {
            out.push("model".to_string());
        }
        for n in self.network.iter().chain(&self.local).chain(&self.unverified) {
            if !out.contains(n) {
                out.push(n.clone());
            }
        }
        out
    }

    /// The things the set touches, as people read them: `AI` first, then alphabetical; a library
    /// nobody approved yet says so.
    pub fn touch_words(&self) -> Vec<String> {
        let mut words: Vec<String> = self
            .network
            .iter()
            .chain(&self.local)
            .map(|n| touch_word(n))
            .chain(self.unverified.iter().map(|n| format!("{n} (not approved yet)")))
            .collect();
        words.sort_by_key(|w| w.to_lowercase());
        words.dedup();
        if self.model {
            words.insert(0, "AI".to_string());
        }
        words
    }

    /// `nothing`, or `AI, crm, email, log` (SPEC 23.7).
    pub fn touches(&self) -> String {
        if self.is_pure() {
            "nothing".to_string()
        } else {
            self.touch_words().join(", ")
        }
    }

    /// Does the set contain the source `name` (internal name)?
    pub fn has(&self, name: &str) -> bool {
        if name == "model" {
            return self.model;
        }
        self.network.iter().chain(&self.local).chain(&self.unverified).any(|n| n == name)
    }
}

impl fmt::Display for EffectSet {
    /// `pure`, or e.g. `model, network(email, crm), local(log)` (SPEC 8).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_pure() {
            return f.write_str("pure");
        }
        let mut parts: Vec<String> = Vec::new();
        if self.model {
            parts.push("model".into());
        }
        if !self.network.is_empty() {
            parts.push(format!("network({})", self.network.join(", ")));
        }
        if !self.local.is_empty() {
            parts.push(format!("local({})", self.local.join(", ")));
        }
        if !self.unverified.is_empty() {
            parts.push(format!("unverified({})", self.unverified.join(", ")));
        }
        f.write_str(&parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_follows_the_spec_summary() {
        let mut e = EffectSet::pure();
        assert_eq!(e.to_string(), "pure");
        e.add(Effect::Model, "model");
        e.add(Effect::Network, "email");
        e.add(Effect::Network, "crm");
        e.add(Effect::Network, "email");
        e.add(Effect::Local, "log");
        assert_eq!(e.to_string(), "model, network(email, crm), local(log)");
    }

    #[test]
    fn class_and_exceeds() {
        let mut e = EffectSet::of(Effect::Local, "log");
        assert_eq!(e.class(), Effect::Local);
        assert!(!e.exceeds(Effect::Local));
        assert!(e.exceeds(Effect::Pure));
        e.add(Effect::Network, "crm");
        assert_eq!(e.class(), Effect::Network);
        assert!(e.exceeds(Effect::Local));
        assert!(!e.exceeds(Effect::Model));
        assert_eq!(e.detail(), "network (crm)");
    }

    #[test]
    fn unverified_counts_like_network_for_limits() {
        let e = EffectSet::of(Effect::Unverified, "pdf");
        assert!(!e.is_pure());
        assert!(e.exceeds(Effect::Local));
        assert!(!e.exceeds(Effect::Network));
        assert_eq!(e.to_string(), "unverified(pdf)");
    }

    #[test]
    fn touches_reads_as_words() {
        let mut e = EffectSet::pure();
        assert_eq!(e.touches(), "nothing");
        e.add(Effect::Network, "email");
        e.add(Effect::Network, "crm");
        e.add(Effect::Local, "log");
        e.add(Effect::Model, "model");
        assert_eq!(e.touches(), "AI, crm, email, log");
        let mut f = EffectSet::of(Effect::Local, "print");
        f.add(Effect::Local, "file");
        assert_eq!(f.touches(), "files, print");
    }

    #[test]
    fn merge_unions_in_first_use_order() {
        let mut a = EffectSet::of(Effect::Network, "email");
        let mut b = EffectSet::of(Effect::Network, "crm");
        b.add(Effect::Model, "model");
        a.merge(&b);
        assert_eq!(a.to_string(), "model, network(email, crm)");
    }
}
