//! Effect sets (SPEC 8, DESIGN 3): what a statement, block or function can touch.
//!
//! The lattice is `pure < local < network < model`. Calls into bindings that nobody approved yet
//! are tracked separately as `unverified` (SPEC 15.3); for the "does the body exceed its
//! declared effect" test an unverified item counts like `network`.

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
    fn merge_unions_in_first_use_order() {
        let mut a = EffectSet::of(Effect::Network, "email");
        let mut b = EffectSet::of(Effect::Network, "crm");
        b.add(Effect::Model, "model");
        a.merge(&b);
        assert_eq!(a.to_string(), "model, network(email, crm)");
    }
}
