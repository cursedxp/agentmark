//! `agentmark bind` (SPEC 15, 9): generate or refresh `<project>/bindings/<crate>.agent.md`.
//!
//! The model comes from `AGENTMARK_MODEL`: `claude` (default) or `claude:<model>` call the
//! Anthropic API (`ANTHROPIC_API_KEY`); `mock:<path to model.json>` uses the canned `MockModel`
//! (tests, offline work). `AGENTMARK_OFFLINE=1` passes `--offline` to every cargo call of the
//! binder. Rustdoc JSON can be supplied with `AGENTMARK_RUSTDOC_JSON` / `AGENTMARK_RUSTDOC_JSON_DIR`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentmark_rt::{AnthropicModel, MockModel, ModelClient};
use chrono::NaiveDate;

use super::fail;
use crate::binder::{bind, BindError, BindOptions, BindOutcome, Regenerated};
use crate::syntax::ast::{Decl, Effect};
use crate::syntax::parser::parse_file;

const DEFAULT_CLAUDE: &str = "claude-sonnet-4-5";

fn model_from_env() -> Result<(Arc<dyn ModelClient>, Option<NaiveDate>), String> {
    let spec = std::env::var("AGENTMARK_MODEL").unwrap_or_default();
    let spec = if spec.is_empty() { "claude".to_string() } else { spec };
    if let Some(path) = spec.strip_prefix("mock:") {
        let m = MockModel::from_file(path).map_err(|e| format!("cannot load the mock model: {e}"))?;
        let today = m.today();
        return Ok((Arc::new(m), today));
    }
    let id = match spec.as_str() {
        "claude" => DEFAULT_CLAUDE,
        s => s.strip_prefix("claude:").ok_or_else(|| {
            format!("AGENTMARK_MODEL=\"{s}\" is not supported here; use \"claude\", \"claude:<model>\" or \"mock:<path to model.json>\"")
        })?,
    };
    let m = AnthropicModel::from_env(id)
        .map_err(|e| format!("{e}. Set it, or point AGENTMARK_MODEL at a mock:<model.json> for offline use"))?;
    Ok((Arc::new(m), None))
}

/// Today's date from the system clock (UTC), without a timezone database.
fn system_today() -> NaiveDate {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = i32::try_from(secs / 86_400).unwrap_or(0);
    NaiveDate::from_num_days_from_ce_opt(days + 719_163).unwrap_or_default()
}

pub(crate) fn run(name: Option<&str>, force: bool, all: bool, project: &str) -> u8 {
    let (model, mock_today) = match model_from_env() {
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
