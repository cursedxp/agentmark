//! Getting rustdoc JSON for a crate: build it with `cargo +nightly rustdoc` in a scratch crate
//! when nightly is installed, else read the file named by `AGENTMARK_RUSTDOC_JSON`
//! (OPEN_QUESTIONS Q10).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const ENV_JSON: &str = "AGENTMARK_RUSTDOC_JSON";

#[derive(Debug, Clone)]
pub struct FetchSpec {
    pub crate_name: String,
    /// Exact version, or `None` for the newest the registry offers.
    pub version: Option<String>,
    pub offline: bool,
    pub default_features: bool,
    pub features: Vec<String>,
}

impl FetchSpec {
    pub fn new(crate_name: &str, version: Option<&str>) -> FetchSpec {
        FetchSpec {
            crate_name: crate_name.into(),
            version: version.map(String::from),
            offline: false,
            default_features: true,
            features: vec![],
        }
    }
}

pub fn nightly_available() -> bool {
    Command::new("cargo")
        .args(["+nightly", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn scratch(tag: &str) -> Result<PathBuf, String> {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("agentmark-{tag}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn write_manifest(dir: &Path, spec: &FetchSpec) -> Result<(), String> {
    let req = spec.version.as_deref().map(|v| format!("={v}")).unwrap_or_else(|| "*".into());
    let feats = spec.features.iter().map(|f| format!("\"{f}\"")).collect::<Vec<_>>().join(", ");
    let manifest = format!(
        "[package]\nname = \"agentmark_rustdoc_scratch\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
         [workspace]\n\n[lib]\npath = \"src/lib.rs\"\n\n[dependencies]\n\
         {} = {{ version = \"{req}\", default-features = {}, features = [{feats}] }}\n",
        spec.crate_name, spec.default_features
    );
    std::fs::write(dir.join("Cargo.toml"), manifest).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("src/lib.rs"), "").map_err(|e| e.to_string())
}

fn cargo(dir: &Path, args: &[&str], offline: bool) -> Result<std::process::Output, String> {
    let mut c = Command::new("cargo");
    // `--offline` is a cargo flag: it must come before the `--` that hands the rest to rustdoc.
    let cut = args.iter().position(|a| *a == "--").unwrap_or(args.len());
    c.args(&args[..cut]);
    if offline {
        c.arg("--offline");
    }
    c.args(&args[cut..]);
    let out = c.current_dir(dir).output().map_err(|e| format!("cannot run cargo: {e}"))?;
    if out.status.success() {
        Ok(out)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().chars().take(600).collect())
    }
}

/// The version cargo resolves for `spec` (no nightly needed).
pub fn resolve_version(spec: &FetchSpec) -> Result<String, String> {
    if let Some(v) = &spec.version {
        return Ok(v.clone());
    }
    let dir = scratch("resolve")?;
    let r = (|| {
        write_manifest(&dir, spec)?;
        let out = cargo(&dir, &["metadata", "--format-version", "1"], spec.offline)?;
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
        v["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["name"] == spec.crate_name.as_str())
            .and_then(|p| p["version"].as_str().map(String::from))
            .ok_or_else(|| format!("cargo did not resolve a crate named `{}`", spec.crate_name))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    r
}

/// Build rustdoc JSON for the crate and return the path of the file. The scratch directory is
/// left in place (the file lives in it); the OS temp cleaner removes it.
pub fn fetch_rustdoc_json_with(spec: &FetchSpec) -> Result<PathBuf, String> {
    if !nightly_available() {
        return match std::env::var(ENV_JSON) {
            Ok(p) if !p.is_empty() => Ok(PathBuf::from(p)),
            _ => Err(format!(
                "building rustdoc JSON needs `cargo +nightly`, which is not installed; \
                 set {ENV_JSON} to a pre-generated rustdoc JSON file"
            )),
        };
    }
    let dir = scratch("rustdoc")?;
    write_manifest(&dir, spec)?;
    cargo(
        &dir,
        &[
            "+nightly", "rustdoc", "-p", &spec.crate_name, "--target-dir", "target", "--", "-Z",
            "unstable-options", "--output-format", "json",
        ],
        spec.offline,
    )
    .map_err(|e| format!("rustdoc for `{}` failed: {e}", spec.crate_name))?;
    let file = dir.join("target/doc").join(format!("{}.json", spec.crate_name.replace('-', "_")));
    if file.exists() {
        Ok(file)
    } else {
        Err(format!("rustdoc produced no JSON at {}", file.display()))
    }
}

/// Rustdoc JSON of a dependency of the crate `main_json` was built for, built in the very same
/// scratch crate (so the dependency has the version the crate was built against).
pub fn fetch_dep_rustdoc_json(main_json: &Path, dep: &str, offline: bool) -> Result<PathBuf, String> {
    let dir = main_json
        .ancestors()
        .nth(3)
        .filter(|d| d.join("Cargo.toml").exists())
        .ok_or("rustdoc JSON was not built by the binder: no scratch crate to build a dependency in")?;
    cargo(
        dir,
        &["+nightly", "rustdoc", "-p", dep, "--target-dir", "target", "--", "-Z", "unstable-options", "--output-format", "json"],
        offline,
    )
    .map_err(|e| format!("rustdoc for dependency `{dep}` failed: {e}"))?;
    let file = dir.join("target/doc").join(format!("{}.json", dep.replace('-', "_")));
    if file.exists() { Ok(file) } else { Err(format!("rustdoc produced no JSON at {}", file.display())) }
}

/// `fetch_rustdoc_json(crate, version)`: see the module docs.
pub fn fetch_rustdoc_json(crate_name: &str, version: Option<&str>) -> Result<PathBuf, String> {
    fetch_rustdoc_json_with(&FetchSpec::new(crate_name, version))
}
