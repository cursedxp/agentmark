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

pub type ForeignFetch = Arc<dyn Fn(&str) -> Result<PathBuf, String> + Send + Sync>;

pub struct BindOptions {
    /// Folder holding `<crate>.agent.md` files (`.agentmark/bindings`).
    pub bindings_dir: PathBuf,
    /// Exact crate version; `None` = whatever the pre-generated rustdoc JSON says, or cargo resolves.
    pub version: Option<String>,
    /// Move an existing binding to the newest version. Without `version` or `latest`, an existing
    /// binding stays on the version in its header (Q155); only a missing one takes the newest.
    pub latest: bool,
    /// Regenerate even when the file is fresh, or hand-written. Overwrites an approved binding.
    pub force: bool,
    /// Pre-generated rustdoc JSON (tests, `AGENTMARK_RUSTDOC_JSON`); else it is built.
    pub rustdoc_json: Option<PathBuf>,
    pub model: Arc<dyn ModelClient>,
    /// `None` = a real `cargo check` with the same features as the rustdoc build.
    pub checker: Option<Box<dyn TypeChecker>>,
    /// Dependency features the library is bound (and its bodies checked) with.
    pub default_features: bool,
    pub features: Vec<String>,
    /// Per crate, from `[crates]` in `agentmark.toml`: `(default features, features)`; overrides the two above.
    pub crate_features: std::collections::BTreeMap<String, (bool, Vec<String>)>,
    /// Pre-generated rustdoc JSON of other crates (tests): used when the bound crate re-exports
    /// a trait from one of them. Otherwise they are built next to the main one.
    pub foreign_rustdoc: Vec<PathBuf>,
    /// Replaces the dependency rustdoc build (tests; e.g. one that fails).
    pub foreign_fetch: Option<ForeignFetch>,
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
            latest: false,
            force: false,
            rustdoc_json: None,
            model,
            checker: None,
            default_features: true,
            features: vec![],
            crate_features: Default::default(),
            foreign_rustdoc: vec![],
            foreign_fetch: None,
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

/// Rustdoc JSON text of dependency crate `c`: supplied files, the test hook,
/// `AGENTMARK_RUSTDOC_JSON_DIR/<c>.json`, then a build next to the main JSON (only when the
/// binder built that itself).
fn foreign_json(c: &str, main: &Path, built_here: bool, opts: &BindOptions) -> Result<String, String> {
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    if let Some(p) = opts.foreign_rustdoc.iter().find(|p| root_name(p).as_deref() == Some(c)) {
        return read(p);
    }
    if let Some(f) = &opts.foreign_fetch {
        return read(&f(c).map_err(|e| format!("rustdoc JSON for `{c}` unavailable: {e}"))?);
    }
    if let Some(dir) = rust::fetch::env_nonempty(rust::fetch::ENV_JSON_DIR) {
        let p = Path::new(&dir).join(format!("{}.json", c.replace('-', "_")));
        if p.exists() {
            return read(&p);
        }
    }
    if built_here && rust::fetch::env_nonempty(rust::fetch::ENV_JSON).is_none() {
        return read(&rust::fetch_dep_rustdoc_json(main, c, opts.offline)?);
    }
    Err(format!(
        "no rustdoc JSON for `{c}` supplied (put {c}.json in a directory named by {})",
        rust::fetch::ENV_JSON_DIR
    ))
}

fn root_name(path: &Path) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let root = v["root"].as_u64()?;
    v["index"][root.to_string()]["name"].as_str().map(String::from)
}

fn spec(crate_name: &str, version: Option<&str>, opts: &BindOptions) -> rust::FetchSpec {
    let mut s = rust::FetchSpec::new(crate_name, version);
    s.offline = opts.offline;
    s.default_features = opts.default_features;
    s.features = opts.features.clone();
    if let Some((d, f)) = opts.crate_features.get(crate_name) {
        s.default_features = *d;
        s.features = f.clone();
    }
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

/// The version in the header of an existing binding of this crate.
fn header_version(path: &Path, crate_name: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    self::core::parse_header(&text).filter(|h| h.crate_name == crate_name).map(|h| h.version)
}

/// The library's items for `version`: from the given rustdoc JSON, else built with cargo. The
/// guard removes the scratch crate the JSON was built in when it is dropped.
fn load_crate(
    crate_name: &str,
    version: &str,
    json_given: Option<PathBuf>,
    opts: &BindOptions,
) -> Result<(core::CrateItems, rust::ScratchGuard), BindError> {
    let built_here = json_given.is_none();
    let json_path = match json_given {
        Some(p) => p,
        None => rust::fetch_rustdoc_json_with(&spec(crate_name, Some(version), opts)).map_err(BindError::Metadata)?,
    };
    // the scratch crate the JSON was built in goes when binding is over, whatever the outcome
    let _scratch = if built_here { rust::ScratchGuard::of_json(&json_path) } else { rust::ScratchGuard::new_none() };
    let json = std::fs::read_to_string(&json_path)
        .map_err(|e| BindError::Metadata(format!("{}: {e}", json_path.display())))?;
    let mut foreign = Vec::new();
    let mut foreign_errors: Vec<(String, String)> = Vec::new();
    for c in rust::foreign_crates(&json).map_err(BindError::Metadata)? {
        match foreign_json(&c, &json_path, built_here, opts) {
            Ok(text) => foreign.push(text),
            Err(e) => foreign_errors.push((c, e)),
        }
    }
    let mut ci = rust::read_crate_with(&json, &foreign).map_err(BindError::Metadata)?;
    // A dependency whose rustdoc could not be had is said so in the re-export's line.
    for (c, e) in &foreign_errors {
        let prefix = format!("re-export of `{c}");
        for n in ci.extra_not_bound.iter_mut().filter(|n| n.reason.starts_with(&prefix)) {
            n.reason.push_str(&format!("; its methods on this crate's types are not bound: {e}"));
        }
    }
    if ci.version != version {
        return Err(BindError::Metadata(format!(
            "rustdoc JSON is for {} {} but {version} was asked for",
            ci.name, ci.version
        )));
    }

    Ok((ci, _scratch))
}

/// `agentmark bind <crate>` (SPEC 15.1): reuse the cached binding when it is for the wanted
/// version, otherwise run the three tiers and write a fresh `unverified` file.
pub fn bind(crate_name: &str, opts: &BindOptions) -> Result<BindOutcome, BindError> {
    let path = binding_path(&opts.bindings_dir, crate_name);
    let env_json = rust::fetch::env_nonempty(rust::fetch::ENV_JSON).map(PathBuf::from);
    let json_given = opts.rustdoc_json.clone().or(env_json);
    // an existing binding is the lock (Q155): no silent jump to a newer release
    let pinned = if opts.version.is_none() && !opts.latest { header_version(&path, crate_name) } else { None };
    let version = match (&opts.version, &pinned, &json_given) {
        (Some(v), _, _) => v.clone(),
        (None, Some(v), _) => v.clone(),
        (None, None, Some(j)) => model_version(j)?,
        (None, None, None) => rust::resolve_version(&spec(crate_name, None, opts)).map_err(BindError::Metadata)?,
    };

    let state = freshness(&path, crate_name, &version).map_err(|e| BindError::Io(format!("{}: {e}", path.display())))?;
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

    let (ci, _scratch) = load_crate(crate_name, &version, json_given, opts)?;

    let default_checker;
    let checker: &dyn TypeChecker = match &opts.checker {
        Some(c) => &**c,
        None => {
            default_checker = rust::CargoChecker::for_spec(&spec(crate_name, Some(&version), opts)).with_handle_types(ci.handle_types.clone());
            &default_checker
        }
    };
    let bound = run_blocking(run_tiers(&ci, &*opts.model, checker));
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
    write_atomic(&path, &text).map_err(|e| BindError::Io(format!("{}: {e}", path.display())))?;
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

/// A statement of the author against an item listed under `## Not bound`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnDemandUse {
    pub item: String,
    /// The statement as written, without the leading `- `.
    pub statement: String,
    /// How many values the statement gives after the item name.
    pub arity: usize,
}

#[derive(Debug)]
pub enum OnDemandError {
    /// The binding or the library's metadata could not be read (no rustdoc, bad file).
    Setup(BindError),
    /// The model's code failed the type check [`core::ondemand::ATTEMPTS`] times; nothing was written.
    Failed(core::ondemand::Failure),
}

/// The result of [`produce_on_demand`]: the new text of one binding file, not yet written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnDemandProduced {
    pub path: PathBuf,
    pub old_text: String,
    pub new_text: String,
    /// Names of the items that are bound in `new_text`.
    pub items: Vec<String>,
}

/// Write a file so that a reader sees the old or the new content, never half of it.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// The version of `crate_name` the project is locked to: a `Cargo.lock` next to `bindings/` (or
/// above `.agentmark/bindings/`).
fn locked_version(bindings_dir: &Path, crate_name: &str) -> Option<String> {
    let mut dirs = vec![];
    if let Some(p) = bindings_dir.parent() {
        dirs.push(p.to_path_buf());
        if let Some(pp) = p.parent() {
            dirs.push(pp.to_path_buf());
        }
    }
    for d in dirs {
        let Ok(text) = std::fs::read_to_string(d.join("Cargo.lock")) else { continue };
        let mut lines = text.lines();
        while let Some(l) = lines.next() {
            if l.trim() == format!("name = \"{crate_name}\"") {
                if let Some(v) = lines.next().and_then(|l| l.trim().strip_prefix("version = \"")) {
                    return v.strip_suffix('"').map(String::from);
                }
            }
        }
    }
    None
}

/// First half of the on-demand tier (SPEC 15.2, last row) for one crate: write the body of every
/// item in `uses` that the binding lists under `## Not bound`, type-check it, and compute the new
/// binding text. Nothing is written; [`commit_on_demand`] does that, so a caller with several
/// crates can commit only when every crate succeeded. `log` gets one plain line per model
/// attempt. `Ok(None)` = nothing in `uses` is listed as not bound.
pub fn produce_on_demand(
    crate_name: &str,
    uses: &[OnDemandUse],
    opts: &BindOptions,
    log: &mut (dyn FnMut(&str) + Send),
) -> Result<Option<OnDemandProduced>, OnDemandError> {
    use self::core::ondemand::{write_item, Request};
    let setup = OnDemandError::Setup;
    let path = binding_path(&opts.bindings_dir, crate_name);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| setup(BindError::Io(format!("{}: {e}", path.display()))))?;
    let name = path.display().to_string();
    let file = crate::syntax::parser::parse_file(&name, &text)
        .map_err(|d| setup(BindError::Invalid(crate::diag::render_all(&d, &text))))?;
    let Some(binding) = file.decls.iter().find_map(|d| match d {
        Decl::Binding(b) => Some(b),
        _ => None,
    }) else {
        return Err(setup(BindError::Invalid(format!("{name} has no `# Binding` declaration"))));
    };
    let mut todo: Vec<(&OnDemandUse, String)> = Vec::new();
    for u in uses {
        if todo.iter().any(|(t, _)| t.item == u.item) {
            continue;
        }
        if let Some(nb) = binding.not_bound.iter().find(|n| n.item == u.item) {
            todo.push((u, nb.reason.clone().unwrap_or_default()));
        }
    }
    if todo.is_empty() {
        return Ok(None);
    }

    let env_json = rust::fetch::env_nonempty(rust::fetch::ENV_JSON).map(PathBuf::from);
    let json_given = opts.rustdoc_json.clone().or(env_json);
    // the version the binding is for: its header, else the project's lock file, else the rustdoc
    // the caller supplied; never "the newest" (the API the author wrote against is the old one)
    let version = match (&opts.version, header_version(&path, crate_name), &json_given) {
        (Some(v), _, _) => v.clone(),
        (None, Some(v), _) => v,
        (None, None, json) => match (locked_version(&opts.bindings_dir, crate_name), json) {
            (Some(v), _) => v,
            (None, Some(j)) => model_version(j).map_err(setup)?,
            (None, None) => {
                return Err(setup(BindError::Metadata(format!(
                    "this binding was written by hand and does not say which version of \"{crate_name}\" it is for; \
                     add a Cargo.lock next to bindings/ that names it, or put the generated header line with the version on top of the file \
                     (`> generated by agentmark bind \u{2014} {crate_name} <version> \u{2014} <date>`)"
                ))))
            }
        },
    };
    let (ci, _scratch) = load_crate(crate_name, &version, json_given, opts).map_err(setup)?;
    let default_checker;
    let checker: &dyn TypeChecker = match &opts.checker {
        Some(c) => &**c,
        None => {
            default_checker = rust::CargoChecker::for_spec(&spec(crate_name, Some(&version), opts)).with_handle_types(ci.handle_types.clone());
            &default_checker
        }
    };

    let mut added: Vec<core::BoundItem> = Vec::new();
    for (u, reason) in &todo {
        let req = Request { crate_name, item: &u.item, statement: &u.statement, reason, arity: u.arity };
        let one = run_blocking(write_item(&ci, &req, &*opts.model, checker, &mut *log));
        added.push(one.map_err(OnDemandError::Failed)?);
    }

    let texts: Vec<&str> = std::iter::once(ci.name.as_str())
        .chain(added.iter().flat_map(|i| {
            ci.items.iter().filter(move |c| c.name == i.name).map(|c| c.source_sig.as_str())
        }))
        .chain(added.iter().map(|i| i.body.as_str()))
        .collect();
    let proposed = propose_effect(&ci.hints, texts);
    let new_text = core::add_on_demand(&text, &added, &ci.code_lang, proposed);
    Ok(Some(OnDemandProduced {
        path,
        old_text: text,
        new_text,
        items: added.into_iter().map(|i| i.name).collect(),
    }))
}

/// Second half: save what [`produce_on_demand`] made. Each file is replaced atomically; the previous
/// files are kept as `.prev` after all replacements succeeded. If a write fails, files already replaced are put back.
pub fn commit_on_demand(all: &[OnDemandProduced]) -> Result<(), BindError> {
    let prev_of = |p: &PathBuf| {
        let mut prev = p.clone().into_os_string();
        prev.push(".prev");
        PathBuf::from(prev)
    };
    let mut done: Vec<&OnDemandProduced> = Vec::new();
    for p in all {
        let step = write_atomic(&p.path, &p.new_text);
        if let Err(e) = step {
            for d in done {
                let _ = write_atomic(&d.path, &d.old_text);
            }
            return Err(BindError::Io(format!("{}: {e}", p.path.display())));
        }
        done.push(p);
    }
    // the previous files are kept only once every replacement went through
    for p in all {
        std::fs::write(prev_of(&p.path), &p.old_text)
            .map_err(|e| BindError::Io(format!("{}: {e}", prev_of(&p.path).display())))?;
    }
    Ok(())
}

/// One crate, produce and commit (the single-crate case). Returns the items now bound.
pub fn bind_on_demand(
    crate_name: &str,
    uses: &[OnDemandUse],
    opts: &BindOptions,
    log: &mut (dyn FnMut(&str) + Send),
) -> Result<Vec<String>, OnDemandError> {
    match produce_on_demand(crate_name, uses, opts, log)? {
        None => Ok(vec![]),
        Some(p) => {
            commit_on_demand(std::slice::from_ref(&p)).map_err(OnDemandError::Setup)?;
            Ok(p.items)
        }
    }
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
    let is_method = |it: &crate::syntax::ast::BindingItem| it.params.first().is_some_and(|p| p.name.name == "self");
    let mut methods = false;
    for it in b.items.iter().filter(|it| !is_method(it)) {
        o.push_str(&item_line(&format!("{}.", b.name.name), it, 0));
    }
    for it in b.items.iter().filter(|it| is_method(it)) {
        if !methods {
            o.push_str("\nMethods, called on a variable that holds the handle: `<variable> <method> args`:\n");
            methods = true;
        }
        let recv = match &it.params[0].ty.kind {
            crate::syntax::ast::TypeKind::Handle { name, mode } => {
                let how = match mode {
                    crate::syntax::ast::HandleMode::Shared => "reads it",
                    crate::syntax::ast::HandleMode::Mut => "changes it",
                    crate::syntax::ast::HandleMode::Consume => "uses it up",
                };
                format!("<handle of {name}> ({how}) ")
            }
            _ => String::new(),
        };
        o.push_str(&item_line(&recv, it, 1));
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

/// `  sha2.sha256_new() -> handle of sha2.Sha256` plus its doc line; `skip` leaves out `self`.
fn item_line(lead: &str, it: &crate::syntax::ast::BindingItem, skip: usize) -> String {
    let params: Vec<String> = it
        .params
        .iter()
        .skip(skip)
        .map(|p| format!("{}: {}", p.name.name, type_text(&p.ty)))
        .collect();
    let ret = it.out.as_ref().map(|t| format!(" -> {}", type_text(t))).unwrap_or_default();
    let mut o = format!("  {lead}{}({}){ret}\n", it.name.name, params.join(", "));
    if let Some(d) = &it.doc {
        o.push_str(&format!("      {d}\n"));
    }
    o
}

fn type_text(t: &crate::syntax::ast::TypeExpr) -> String {
    use crate::syntax::ast::TypeKind;
    match &t.kind {
        TypeKind::Named(n) => n.clone(),
        TypeKind::List(i) => format!("list of {}", type_text(i)),
        TypeKind::Maybe(i) => format!("maybe {}", type_text(i)),
        TypeKind::Result(i) => format!("result of {}", type_text(i)),
        TypeKind::Map(k, v) => format!("map of {} to {}", type_text(k), type_text(v)),
        TypeKind::Handle { name, mode } => match mode.word() {
            Some(w) => format!("{w} handle of {name}"),
            None => format!("handle of {name}"),
        },
    }
}
