//! `agentmark bind` (SPEC 15, 9): generate or refresh `<project>/bindings/<crate>.agent.md`.
//!
//! The model comes from `AGENTMARK_MODEL` (any driver string: `claude`, `claude:<model>`, `openai:<m>`,
//! `local:<m>`, `mock:<path to model.json>`), else the `[models]` entry `AGENTMARK_ENV` names, else Claude. `AGENTMARK_OFFLINE=1` passes `--offline` to every cargo call of the
//! binder. Rustdoc JSON can be supplied with `AGENTMARK_RUSTDOC_JSON` / `AGENTMARK_RUSTDOC_JSON_DIR`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentmark_rt::{Driver, ModelClient, ProviderConfig, Router};
use chrono::NaiveDate;

use super::fail;
use crate::binder::{bind, BindError, BindOptions, BindOutcome, Regenerated};
use crate::syntax::ast::{Decl, Effect};
use crate::syntax::parser::parse_file;

/// Kept for callers without a project (no `[models]` table).
#[allow(dead_code)]
pub(crate) fn model_from_env() -> Result<(Arc<dyn ModelClient>, Option<NaiveDate>), String> {
    model_for(ProviderConfig::default())
}

/// The binder's model: `AGENTMARK_MODEL`, else the `[models]` entry `AGENTMARK_ENV` names in the
/// project's `agentmark.toml`, else Claude (Q240). Same driver strings as a program's `- model:`.
pub(crate) fn model_for(config: ProviderConfig) -> Result<(Arc<dyn ModelClient>, Option<NaiveDate>), String> {
    let router = Router::from_env(config).map_err(|e| {
        format!("{e}. Set it, or point AGENTMARK_MODEL at a mock:<model.json> for offline use")
    })?;
    if router.forced().is_none() {
        // nothing chose a model: Claude, which needs its key before any binding work starts
        router.prepare(&Driver::Claude(None)).map_err(|e| {
            format!("{e}. Set it, or point AGENTMARK_MODEL at a mock:<model.json> for offline use")
        })?;
    }
    let today = router.mock_today();
    Ok((Arc::new(router), today))
}

/// `[models]` of the project's `agentmark.toml`, for the router (a missing or bad file is plain defaults:
/// `check` reports the file's own errors).
pub(crate) fn provider_config(project: &crate::project::Project) -> ProviderConfig {
    ProviderConfig {
        environments: project.config.models.iter().cloned().collect(),
        local_base_url: project.config.local_base_url.clone(),
    }
}

/// Today's date from the system clock (UTC), without a timezone database.
pub(crate) fn system_today() -> NaiveDate {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = i32::try_from(secs / 86_400).unwrap_or(0);
    NaiveDate::from_num_days_from_ce_opt(days + 719_163).unwrap_or_default()
}

pub(crate) fn run(name: Option<&str>, force: bool, all: bool, version: Option<String>, latest: bool, project: &str) -> u8 {
    let (model, mock_today) = match model_for(provider_config(&crate::project::Project::load(project))) {
        Ok(m) => m,
        Err(e) => return fail(e),
    };
    let dir = Path::new(project).join("bindings");
    let crates: Vec<(String, Option<String>)> = if all {
        match existing_crates(&dir) {
            Ok(c) => c,
            Err(e) => return fail(e),
        }
    } else {
        name.map(|n| vec![(n.to_string(), None)]).unwrap_or_default()
    };
    if crates.is_empty() {
        println!("No bindings in {} to refresh.", dir.display());
        return 0;
    }
    let mut failed = false;
    for (c, stem) in &crates {
        // `bind` writes `<crate>.agent.md`; a file with another name is never duplicated (Q117)
        if let Some(stem) = stem.as_ref().filter(|st| *st != c) {
            failed = true;
            eprintln!(
                "agentmark: bindings/{stem}.agent.md holds the binding of \"{c}\" but is not named \"{c}.agent.md\"; \
                 rename it so \"agentmark bind\" can refresh it"
            );
            continue;
        }
        let mut opts = BindOptions::new(&dir, model.clone(), mock_today.unwrap_or_else(system_today));
        opts.force = force;
        opts.version = version.clone();
        opts.latest = latest;
        opts.offline = matches!(std::env::var("AGENTMARK_OFFLINE").as_deref(), Ok("1") | Ok("true"));
        match bind(c, &opts) {
            Ok(out) => println!("{}", describe(c, &out)),
            Err(e) => {
                failed = true;
                eprintln!("agentmark: {}", explain(c, &e));
            }
        }
    }
    u8::from(failed)
}

fn explain(c: &str, e: &BindError) -> String {
    match e {
        BindError::Metadata(m) => format!("cannot read the interface of \"{c}\": {m}"),
        BindError::Invalid(m) => format!("the existing binding of \"{c}\" does not parse:\n{m}"),
        BindError::Io(m) => format!("cannot bind \"{c}\": {m}"),
    }
}

/// Crate names of `bindings/*.agent.md` (from the `- crate:` line, else the file name).
fn existing_crates(dir: &Path) -> Result<Vec<(String, Option<String>)>, String> {
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let mut names = Vec::new();
    let rd = std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for e in rd.filter_map(|e| e.ok()) {
        let file = e.file_name().to_string_lossy().to_string();
        let Some(stem) = file.strip_suffix(".agent.md") else { continue };
        let text = std::fs::read_to_string(e.path()).unwrap_or_default();
        let from_file = parse_file(&file, &text).ok().and_then(|f| {
            f.decls.iter().find_map(|d| match d {
                Decl::Binding(b) => Some(b.crate_name.name.clone()),
                _ => None,
            })
        });
        names.push((from_file.unwrap_or_else(|| stem.to_string()), Some(stem.to_string())));
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn proposed(path: &PathBuf) -> Option<&'static str> {
    let text = std::fs::read_to_string(path).ok()?;
    let f = parse_file(&path.display().to_string(), &text).ok()?;
    f.decls.iter().find_map(|d| match d {
        Decl::Binding(b) => b.proposed_effect.as_ref().map(|p| match p.value {
            Effect::Pure => "pure",
            Effect::Local => "local",
            Effect::Model => "model",
            _ => "network",
        }),
        _ => None,
    })
}

fn describe(c: &str, o: &BindOutcome) -> String {
    let file = o.path.display();
    let mut s = if o.generated {
        let why = match &o.reason {
            Some(Regenerated::Stale { from }) => format!(" (version changed from {from})"),
            Some(Regenerated::Forced) => " (forced)".to_string(),
            _ => String::new(),
        };
        format!(
            "{c} {}: generated {file}{why}: {} direct, {} assisted, {} not bound.",
            o.version, o.counts.direct, o.counts.assisted, o.counts.skipped
        )
    } else {
        format!(
            "{c} {}: cached, {file} is up to date ({} bound, {} not bound).",
            o.version, o.bound, o.counts.skipped
        )
    };
    if o.effect == "unverified" {
        let p = proposed(&o.path).unwrap_or("network");
        s.push_str(&format!(
            "\n  Effect is unverified: change the line \"- effect: unverified\" to \"- effect: {p}\" (or the right class) to approve it."
        ));
    } else {
        s.push_str(&format!("\n  Effect: {} (approved).", o.effect));
    }
    s
}
