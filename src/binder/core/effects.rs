//! Proposed effect class for a fresh binding (SPEC 15.3). Only a proposal: every binding is
//! born `unverified` and a human writes the real `effect:` line.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProposedEffect {
    Pure,
    Local,
    Network,
}

impl fmt::Display for ProposedEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ProposedEffect::Pure => "pure",
            ProposedEffect::Local => "local",
            ProposedEffect::Network => "network",
        })
    }
}

/// Words that betray file-system or network use, supplied by the target (for Rust:
/// `std::fs` -> local, `reqwest` / `tokio::net` -> network).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectHints {
    pub network: Vec<String>,
    pub local: Vec<String>,
}

/// Network beats local beats pure. `texts` are the crate name, the item signatures and the
/// generated bodies: anything where a hint word would show up.
pub fn propose_effect<'a>(
    hints: &EffectHints,
    texts: impl IntoIterator<Item = &'a str>,
) -> ProposedEffect {
    let mut best = ProposedEffect::Pure;
    for t in texts {
        if hints.network.iter().any(|h| t.contains(h.as_str())) {
            return ProposedEffect::Network;
        }
        if hints.local.iter().any(|h| t.contains(h.as_str())) {
            best = ProposedEffect::Local;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints() -> EffectHints {
        EffectHints {
            network: vec!["reqwest".into(), "tokio::net".into()],
            local: vec!["std::fs".into()],
        }
    }

    #[test]
    fn network_beats_local_beats_pure() {
        assert_eq!(propose_effect(&hints(), ["1 + 1"]), ProposedEffect::Pure);
        assert_eq!(
            propose_effect(&hints(), ["x", "std::fs::read(p)"]),
            ProposedEffect::Local
        );
        assert_eq!(
            propose_effect(&hints(), ["std::fs::read(p)", "reqwest::get(u)"]),
            ProposedEffect::Network
        );
    }
}
