//! Type-checking assisted bodies with a real `cargo check` in a scratch crate.

use crate::binder::core::{CheckRequest, Sig, Ty, TypeChecker};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn rust_ty(t: &Ty) -> String {
    match t {
        Ty::Text => "String".into(),
        Ty::Int => "i64".into(),
        Ty::Bool => "bool".into(),
        Ty::Bytes => "Vec<u8>".into(),
        Ty::Date => "chrono::NaiveDate".into(),
        Ty::DateTime => "chrono::DateTime<chrono::Utc>".into(),
        Ty::Duration => "chrono::Duration".into(),
        Ty::List(i) => format!("Vec<{}>", rust_ty(i)),
        Ty::Maybe(i) => format!("Option<{}>", rust_ty(i)),
    }
}

/// `pub async fn name(a: String) -> Result<i64, Box<dyn std::error::Error + Send + Sync>>`:
/// the function a binding body is the body of (OPEN_QUESTIONS Q70).
pub fn rust_fn_header(name: &str, sig: &Sig) -> String {
    let ps: Vec<String> = sig
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, rust_ty(&p.ty)))
        .collect();
    let ret = sig.ret.as_ref().map(rust_ty).unwrap_or_else(|| "()".into());
    format!(
        "pub async fn {name}({}) -> Result<{ret}, Box<dyn std::error::Error + Send + Sync>>",
        ps.join(", ")
    )
}

/// Runs `cargo check` on `<header> { <body> }` in a scratch crate that depends on the bound
/// crate (pinned to the bound version) and on `chrono` (the Rust types of date, datetime and
/// duration).
pub struct CargoChecker {
    pub offline: bool,
    dir: PathBuf,
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

impl CargoChecker {
    pub fn new(offline: bool) -> CargoChecker {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("agentmark-bind-check-{}-{n}", std::process::id()));
        CargoChecker { offline, dir }
    }

    fn prepare(&self, crate_name: &str, version: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(self.dir.join("src"))?;
        let chrono = if crate_name == "chrono" {
            String::new()
        } else {
            "chrono = { version = \"0.4\", default-features = false }\n".to_string()
        };
        let manifest = format!(
            "[package]\nname = \"agentmark_bind_check\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [workspace]\n\n[lib]\npath = \"src/lib.rs\"\n\n[dependencies]\n\
             {crate_name} = \"={version}\"\n{chrono}"
        );
        write_if_changed(&self.dir.join("Cargo.toml"), &manifest)
    }
}

fn write_if_changed(p: &Path, s: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(p).ok().as_deref() == Some(s) {
        return Ok(());
    }
    std::fs::write(p, s)
}

impl Drop for CargoChecker {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl CargoChecker {
    /// Check several bodies of one crate in a single `cargo check` (names must be unique). The
    /// error does not say which body failed; use [`TypeChecker::check`] to find out.
    pub fn check_all(&self, reqs: &[CheckRequest<'_>]) -> Result<(), String> {
        let first = reqs.first().ok_or("nothing to check")?;
        self.run(first.crate_name, first.version, reqs)
    }

    fn run(&self, crate_name: &str, version: &str, reqs: &[CheckRequest<'_>]) -> Result<(), String> {
        self.prepare(crate_name, version)
            .map_err(|e| format!("cannot prepare scratch crate: {e}"))?;
        let mut src = String::from("#![allow(unused, dead_code, unused_imports, unused_mut)]\n");
        for r in reqs {
            src.push_str(&format!("{} {{\n{}\n}}\n", rust_fn_header(r.name, r.sig), r.body));
        }
        std::fs::write(self.dir.join("src/lib.rs"), src).map_err(|e| e.to_string())?;
        self.cargo_check()
    }

    fn cargo_check(&self) -> Result<(), String> {
        let mut cmd = Command::new("cargo");
        cmd.arg("check").arg("--quiet").args(["--message-format", "short", "--color", "never"]);
        if self.offline {
            cmd.arg("--offline");
        }
        let out = cmd
            .current_dir(&self.dir)
            .output()
            .map_err(|e| format!("cannot run cargo: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&out.stderr);
        let errs: Vec<&str> = text
            .lines()
            .filter(|l| l.contains("error") && !l.starts_with("error: could not compile"))
            .take(3)
            .collect();
        Err(if errs.is_empty() { text.trim().chars().take(300).collect() } else { errs.join("; ") })
    }
}

impl TypeChecker for CargoChecker {
    fn check(&self, req: &CheckRequest<'_>) -> Result<(), String> {
        self.run(req.crate_name, req.version, std::slice::from_ref(req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::core::Param;

    #[test]
    fn header_uses_boxed_error_and_rust_types() {
        let sig = Sig {
            params: vec![Param { name: "s".into(), ty: Ty::List(Box::new(Ty::Text)) }],
            ret: Some(Ty::Maybe(Box::new(Ty::Date))),
        };
        assert_eq!(
            rust_fn_header("f", &sig),
            "pub async fn f(s: Vec<String>) -> Result<Option<chrono::NaiveDate>, Box<dyn std::error::Error + Send + Sync>>"
        );
    }
}
