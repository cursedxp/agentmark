//! The three tiers (SPEC 15.2) applied to every item: direct (rules, no model) -> assisted
//! (model writes the body, the type checker accepts or rejects it) -> skipped (listed under
//! `## Not bound` with the reason). Every item lands in exactly one tier; nothing is dropped.

use super::model::{CrateItems, Item, Shape, Sig};
use super::typecheck::{CheckRequest, TypeChecker};
use agentmark_rt::{ModelClient, ModelRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Direct,
    Assisted,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundItem {
    pub name: String,
    pub path: String,
    pub summary: String,
    pub sig: Sig,
    pub body: String,
    pub tier: Tier,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub direct: usize,
    pub assisted: usize,
    pub skipped: usize,
}

impl Counts {
    pub fn bound(&self) -> usize {
        self.direct + self.assisted
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundCrate {
    pub crate_name: String,
    pub version: String,
    pub items: Vec<BoundItem>,
    /// `(item name, reason)` in the order they will be listed.
    pub not_bound: Vec<(String, String)>,
    /// One entry per input item, in input order: `(item name, tier)`. The audit trail for
    /// "exactly one tier each".
    pub placements: Vec<(String, Tier)>,
    pub counts: Counts,
}

/// Run every item through the tiers. `extra_not_bound` entries (traits, foreign re-exports)
/// are listed after the skipped items and counted as skipped.
pub async fn run_tiers(
    ci: &CrateItems,
    model: &dyn ModelClient,
    checker: &dyn TypeChecker,
) -> BoundCrate {
    let mut out = BoundCrate {
        crate_name: ci.name.clone(),
        version: ci.version.clone(),
        items: Vec::new(),
        not_bound: Vec::new(),
        placements: Vec::new(),
        counts: Counts::default(),
    };
    for item in &ci.items {
        let tier = match &item.shape {
            Shape::Direct { sig, body } => {
                push_bound(&mut out, item, sig, body.clone(), Tier::Direct);
                Tier::Direct
            }
            Shape::Assisted { sig, why } => {
                match assist(ci, item, sig, why, model, checker).await {
                    Ok(body) => {
                        push_bound(&mut out, item, sig, body, Tier::Assisted);
                        Tier::Assisted
                    }
                    Err(reason) => {
                        out.not_bound.push((item.name.clone(), with_path(item, &reason)));
                        Tier::Skipped
                    }
                }
            }
            Shape::Skipped { reason } => {
                out.not_bound.push((item.name.clone(), with_path(item, reason)));
                Tier::Skipped
            }
        };
        match tier {
            Tier::Direct => out.counts.direct += 1,
            Tier::Assisted => out.counts.assisted += 1,
            Tier::Skipped => out.counts.skipped += 1,
        }
        out.placements.push((item.name.clone(), tier));
    }
    for e in &ci.extra_not_bound {
        out.not_bound.push((e.name.clone(), e.reason.clone()));
        out.counts.skipped += 1;
        out.placements.push((e.name.clone(), Tier::Skipped));
    }
    out
}

fn push_bound(out: &mut BoundCrate, item: &Item, sig: &Sig, body: String, tier: Tier) {
    out.items.push(BoundItem {
        name: item.name.clone(),
        path: item.path.clone(),
        summary: item.summary.clone(),
        sig: sig.clone(),
        body,
        tier,
    });
}

/// Skip reasons name the library path so the user knows which item to write a `## Rust` block for.
fn with_path(item: &Item, reason: &str) -> String {
    format!("{reason} ({})", item.path)
}

/// One line of text, whitespace flattened. Never cut: the writer wraps long causes (SPEC 15.2).
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Models like to wrap code in fences; the binding file wants the bare body.
fn strip_fences(text: &str) -> String {
    let t = text.trim();
    let Some(rest) = t.strip_prefix("```") else {
        return t.to_string();
    };
    let rest = rest.split_once('\n').map(|(_, r)| r).unwrap_or("");
    rest.rsplit_once("```")
        .map(|(body, _)| body)
        .unwrap_or(rest)
        .trim()
        .to_string()
}

/// The prompt's first line is `bind <path>` so mock fixtures can key on it (longest prefix).
pub fn assist_prompt(ci: &CrateItems, item: &Item, sig: &Sig, why: &str) -> String {
    let docs = if item.docs.trim().is_empty() {
        "(no documentation)"
    } else {
        item.docs.trim()
    };
    format!(
        "bind {path}\n\
         You are the AgentMark binder. Write the {lang} body of one binding item for the crate \
         `{krate}` {version}.\n\n\
         {lang} signature of the library item:\n{src}\n\n\
         AgentMark signature to implement (types: text, int, bool, bytes, date, datetime, \
         duration, list of T, maybe T):\n{heading}\n\n\
         Why this needs hand-written glue: {why}\n\n\
         The body is placed inside:\n{contract}\n\n\
         Documentation of the item:\n{docs}\n\n\
         Style guide:\n{style}\n\n\
         Reply with only the {lang} body (no fences, no explanation).",
        path = item.path,
        lang = ci.language,
        krate = ci.name,
        version = ci.version,
        src = item.source_sig,
        heading = sig.heading(&item.name),
        contract = item.contract,
        style = ci.style_guide,
    )
}

async fn assist(
    ci: &CrateItems,
    item: &Item,
    sig: &Sig,
    why: &str,
    model: &dyn ModelClient,
    checker: &dyn TypeChecker,
) -> Result<String, String> {
    let prompt = assist_prompt(ci, item, sig, why);
    let resp = model
        .complete(&ModelRequest::new(prompt))
        .await
        .map_err(|e| format!("assisted tier: model gave no body ({})", one_line(&e.to_string())))?;
    let body = strip_fences(&resp.text);
    if body.is_empty() {
        return Err("assisted tier: model returned an empty body".into());
    }
    if body.contains("```") {
        return Err("assisted tier: model body contains a code fence".into());
    }
    checker
        .check(&CheckRequest {
            crate_name: &ci.name,
            version: &ci.version,
            name: &item.name,
            sig,
            body: &body,
        })
        .map_err(|e| format!("assisted body failed type-check: {}", one_line(&e)))?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_are_stripped() {
        assert_eq!(strip_fences("```rust\nOk(1)\n```"), "Ok(1)");
        assert_eq!(strip_fences("  Ok(1) "), "Ok(1)");
    }
}
