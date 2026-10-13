//! The binder (SPEC 15): turns a library's public API into `bindings/<crate>.agent.md`.
//! `core` is target-agnostic, `rust` reads rustdoc JSON. See `README.md` in this folder.

pub mod core;
pub mod rust;

use crate::syntax::ast::{Decl, Effect};
use agentmark_rt::ModelClient;
use chrono::NaiveDate;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use self::core::{freshness, propose_effect, render_binding, run_tiers, Counts, Freshness, TypeChecker};

#[derive(Debug)]
pub enum BindError {
    Io(String),
    /// Could not get or read the library metadata (rustdoc JSON).
    Metadata(String),
    /// A binding file that exists does not parse.
    Invalid(String),
}

impl fmt::Display for BindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BindError::Io(m) | BindError::Metadata(m) | BindError::Invalid(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for BindError {}

pub struct BindOptions {
    /// Folder holding `<crate>.agent.md` files (`.agentmark/bindings`).
    pub bindings_dir: PathBuf,
    /// Exact crate version; `None` = whatever the pre-generated rustdoc JSON says, or cargo resolves.
    pub version: Option<String>,
    /// Regenerate even when the file is fresh, or hand-written. Overwrites an approved binding.
    pub force: bool,
    /// Pre-generated rustdoc JSON (tests, `AGENTMARK_RUSTDOC_JSON`); else it is built.
    pub rustdoc_json: Option<PathBuf>,
    pub model: Arc<dyn ModelClient>,
    pub checker: Box<dyn TypeChecker>,
    /// Date written in the header.
    pub today: NaiveDate,
    /// Pass `--offline` to every cargo call.
    pub offline: bool,
}

impl BindOptions {
    pub fn new(bindings_dir: impl Into<PathBuf>, model: Arc<dyn ModelClient>, today: NaiveDate) -> BindOptions {
        BindOptions {
            bindings_dir: bindings_dir.into(),
            version: None,
            force: false,
            rustdoc_json: None,
            model,
            checker: Box::new(rust::CargoChecker::new(false)),
            today,
            offline: false,
        }
    }
}

/// What a `bind` call did. On a cache hit `direct`/`assisted` are not known (the file does not
/// record tiers) and `counts.skipped` is the number of `## Not bound` lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindOutcome {
    pub path: PathBuf,
    /// `false` = cache hit, the file was left alone.
    pub generated: bool,
    pub counts: Counts,
    /// Bound items in the file (always known).
    pub bound: usize,
    pub version: String,
    /// The `effect:` line of the file now on disk (`unverified` unless a human changed it).
    pub effect: String,
    /// Why a regeneration happened: `None` on cache hit.
    pub reason: Option<Regenerated>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Regenerated {
    Missing,
    Stale { from: String },
    Forced,
}

pub fn binding_path(dir: &Path, crate_name: &str) -> PathBuf {
    dir.join(format!("{crate_name}.agent.md"))
}

fn model_version(path: &Path) -> Result<String, BindError> {
    let text = std::fs::read_to_string(path).map_err(|e| BindError::Metadata(format!("{}: {e}", path.display())))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| BindError::Metadata(format!("{}: {e}", path.display())))?;
    v["crate_version"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| BindError::Metadata(format!("{} has no crate_version", path.display())))
}

fn spec(crate_name: &str, version: Option<&str>, opts: &BindOptions) -> rust::FetchSpec {
    let mut s = rust::FetchSpec::new(crate_name, version);
    s.offline = opts.offline;
    s
}

/// Parse a binding file and count what is in it.
fn read_counts(path: &Path) -> Result<(Counts, usize, String), BindError> {
    let text = std::fs::read_to_string(path).map_err(|e| BindError::Io(format!("{}: {e}", path.display())))?;
    let name = path.display().to_string();
    let file = crate::syntax::parser::parse_file(&name, &text)
        .map_err(|d| BindError::Invalid(crate::diag::render_all(&d, &text)))?;
    for d in &file.decls {
        if let Decl::Binding(b) = d {
            let eff = match b.effect.value {
                Effect::Pure => "pure",
                Effect::Local => "local",
                Effect::Network => "network",
                Effect::Model => "model",
                Effect::Unverified => "unverified",
            };
            let counts = Counts {
                direct: 0,
                assisted: 0,
                skipped: b.not_bound.len(),
            };
            return Ok((counts, b.items.len(), eff.to_string()));
        }
    }
    Err(BindError::Invalid(format!("{name} has no `# Binding` declaration")))
}

/// `agentmark bind <crate>` (SPEC 15.1): reuse the cached binding when it is for the wanted
/// version, otherwise run the three tiers and write a fresh `unverified` file.
pub fn bind(crate_name: &str, opts: &BindOptions) -> Result<BindOutcome, BindError> {
    let path = binding_path(&opts.bindings_dir, crate_name);
    let env_json = std::env::var(rust::fetch::ENV_JSON).ok().filter(|s| !s.is_empty()).map(PathBuf::from);
    let json_given = opts.rustdoc_json.clone().or(env_json);
    let version = match (&opts.version, &json_given) {
        (Some(v), _) => v.clone(),
        (None, Some(j)) => model_version(j)?,
        (None, None) => rust::resolve_version(&spec(crate_name, None, opts)).map_err(BindError::Metadata)?,
    };

    let state = freshness(&path, &version).map_err(|e| BindError::Io(format!("{}: {e}", path.display())))?;
    let reason = match (&state, opts.force) {
        (Freshness::Missing, _) => Some(Regenerated::Missing),
        (Freshness::Stale { bound_version }, _) => Some(Regenerated::Stale { from: bound_version.clone() }),
        (_, true) => Some(Regenerated::Forced),
        (Freshness::Fresh | Freshness::Unmanaged, false) => None,
    };
    let Some(reason) = reason else {
        let (counts, bound, effect) = read_counts(&path)?;
        return Ok(BindOutcome { path, generated: false, counts, bound, version, effect, reason: None });
    };

    let json_path = match json_given {
        Some(p) => p,
        None => rust::fetch_rustdoc_json_with(&spec(crate_name, Some(&version), opts)).map_err(BindError::Metadata)?,
    };
    let json = std::fs::read_to_string(&json_path)
        .map_err(|e| BindError::Metadata(format!("{}: {e}", json_path.display())))?;
    let ci = rust::read_crate(&json).map_err(BindError::Metadata)?;
    if ci.version != version {
        return Err(BindError::Metadata(format!(
            "rustdoc JSON is for {} {} but {version} was asked for",
            ci.name, ci.version
        )));
    }

    let bound = run_blocking(run_tiers(&ci, &*opts.model, &*opts.checker));
    let texts: Vec<&str> = std::iter::once(ci.name.as_str())
        .chain(ci.items.iter().map(|i| i.source_sig.as_str()))
        .chain(bound.items.iter().map(|i| i.body.as_str()))
        .collect();
    let proposed = propose_effect(&ci.hints, texts);
    let text = render_binding(&bound, &ci.code_lang, proposed, &opts.today.format("%Y-%m-%d").to_string());

    std::fs::create_dir_all(&opts.bindings_dir).map_err(|e| BindError::Io(e.to_string()))?;
    if path.exists() {
        // Hand edits and approvals are not lost: the previous file stays next to the new one.
        let mut prev = path.clone().into_os_string();
        prev.push(".prev");
        std::fs::copy(&path, PathBuf::from(prev)).map_err(|e| BindError::Io(e.to_string()))?;
    }
    std::fs::write(&path, &text).map_err(|e| BindError::Io(format!("{}: {e}", path.display())))?;
    Ok(BindOutcome {
        path,
        generated: true,
        counts: bound.counts,
        bound: bound.counts.bound(),
        version,
        effect: "unverified".into(),
        reason: Some(reason),
    })
}

/// Drive an async future from sync code, on its own thread so it also works when the caller is
/// already inside a tokio runtime.
fn run_blocking<F: std::future::Future + Send>(f: F) -> F::Output
where
    F::Output: Send,
{
    std::thread::scope(|s| {
        s.spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime")
                .block_on(f)
        })
        .join()
        .expect("binder thread panicked")
    })
}

/// Plain-text cheat sheet for `agentmark doc <crate>`: what the binding offers, how trusted it
/// is, and what is not bound.
pub fn doc_cheatsheet(binding_file: &Path) -> Result<String, BindError> {
    let text = std::fs::read_to_string(binding_file)
        .map_err(|e| BindError::Io(format!("{}: {e}", binding_file.display())))?;
    let name = binding_file.display().to_string();
    let file = crate::syntax::parser::parse_file(&name, &text)
        .map_err(|d| BindError::Invalid(crate::diag::render_all(&d, &text)))?;
    let Some(b) = file.decls.iter().find_map(|d| match d {
        Decl::Binding(b) => Some(b),
        _ => None,
    }) else {
        return Err(BindError::Invalid(format!("{name} has no `# Binding` declaration")));
    };
    let eff = |e: Effect| match e {
        Effect::Pure => "pure",
        Effect::Local => "local",
        Effect::Network => "network",
        Effect::Model => "model",
        Effect::Unverified => "unverified",
    };
    let mut o = String::new();
    let header = self::core::parse_header(&text);
    match &header {
        Some(h) => o.push_str(&format!("{} {} (bound {})\n", h.crate_name, h.version, h.date)),
        None => o.push_str(&format!("{} (hand-written binding)\n", b.crate_name.name)),
    }
    o.push_str(&format!("effect: {}", eff(b.effect.value)));
    if b.effect.value == Effect::Unverified {
        match &b.proposed_effect {
            Some(p) => o.push_str(&format!(
                " (proposed: {}; change the `- effect:` line to approve)",
                eff(p.value)
            )),
            None => o.push_str(" (change the `- effect:` line to approve)"),
        }
    }
    o.push_str("\n\nUse in a statement as `<binding> <item> args`; items:\n");
    for it in &b.items {
        let params: Vec<String> = it
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name.name, type_text(&p.ty)))
            .collect();
        let ret = it.out.as_ref().map(|t| format!(" -> {}", type_text(t))).unwrap_or_default();
        o.push_str(&format!("  {}.{}({}){}\n", b.name.name, it.name.name, params.join(", "), ret));
        if let Some(d) = &it.doc {
            o.push_str(&format!("      {d}\n"));
        }
    }
    if b.items.is_empty() {
        o.push_str("  (nothing bound)\n");
    }
    if !b.not_bound.is_empty() {
        o.push_str("\nNot bound (use a `## Rust` block for these):\n");
        for n in &b.not_bound {
            match &n.reason {
                Some(r) => o.push_str(&format!("  {}: {r}\n", n.item)),
                None => o.push_str(&format!("  {}\n", n.item)),
            }
        }
    }
    Ok(o)
}

fn type_text(t: &crate::syntax::ast::TypeExpr) -> String {
    use crate::syntax::ast::TypeKind;
    match &t.kind {
        TypeKind::Named(n) => n.clone(),
        TypeKind::List(i) => format!("list of {}", type_text(i)),
        TypeKind::Maybe(i) => format!("maybe {}", type_text(i)),
        TypeKind::Result(i) => format!("result of {}", type_text(i)),
    }
}
