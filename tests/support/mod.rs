//! Helpers shared by the tests that drive the real `agentmark` binary and build what it generates
//! (`tests/codegen.rs`, `tests/examples_e2e.rs`).
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

pub const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// All generated projects share one target folder, so dependencies compile once.
pub fn gen_target() -> String {
    format!("{ROOT}/target/gen")
}

/// The built binary of a generated project (the package name is the project name).
pub fn gen_bin(name: &str) -> PathBuf {
    PathBuf::from(gen_target()).join("debug").join(name)
}

/// A scratch folder for what the compiler itself caches in `$TMPDIR` (build cache, rustdoc scratch):
/// kept under `target/`, so a test run leaves nothing behind in the system temp folder.
pub fn compiler_tmp() -> PathBuf {
    let d = PathBuf::from(format!("{ROOT}/target/test-tmp"));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `agentmark <args>` run from the repository root, offline, without any model or rustdoc override.
pub fn agentmark(args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_agentmark"));
    c.args(args).current_dir(ROOT);
    c.env("CARGO_TARGET_DIR", gen_target())
        .env("TMPDIR", compiler_tmp())
        .env("AGENTMARK_OFFLINE", "1")
        .env_remove("AGENTMARK_MODEL")
        .env_remove("AGENTMARK_ENV")
        .env_remove("OPENAI_API_KEY")
        .env_remove("OPENAI_BASE_URL")
        .env_remove("LOCAL_API_KEY")
        .env_remove("AGENTMARK_LOCAL_BASE_URL")
        .env_remove("ANTHROPIC_BASE_URL")
        .env_remove("AGENTMARK_FIXTURES")
        .env_remove("AGENTMARK_RUSTDOC_JSON")
        .env_remove("AGENTMARK_RUSTDOC_JSON_DIR")
        .env_remove("ANTHROPIC_API_KEY");
    c
}

/// Canned model and rustdoc for the binder, so `build` can generate a binding without network.
pub fn bind_env(c: &mut Command) {
    c.env("AGENTMARK_MODEL", format!("mock:{ROOT}/tests/binder/model.json"))
        .env("AGENTMARK_RUSTDOC_JSON", format!("{ROOT}/tests/binder/sha2.json"))
        .env("AGENTMARK_RUSTDOC_JSON_DIR", format!("{ROOT}/tests/binder"));
}

/// A unique temp folder, removed when dropped (so reruns never meet leftovers).
pub struct Tmp(tempfile::TempDir);

impl std::ops::Deref for Tmp {
    type Target = Path;
    fn deref(&self) -> &Path {
        self.0.path()
    }
}

impl AsRef<Path> for Tmp {
    fn as_ref(&self) -> &Path {
        self.0.path()
    }
}

pub fn temp(tag: &str) -> Tmp {
    Tmp(tempfile::Builder::new().prefix(&format!("agentmark-{tag}-")).tempdir().unwrap())
}

pub fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        if e.path().is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()));
        } else {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

pub fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

pub fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

pub fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

pub fn parse_lines(text: &str) -> Vec<Value> {
    text.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON: {l}: {e}"))).collect()
}

/// `cargo build --offline` of a generated project (into the shared target folder).
pub fn cargo_build_in(out: &Path) -> Output {
    Command::new("cargo")
        .args(["build", "--offline", "--manifest-path"])
        .arg(out.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", gen_target())
        .output()
        .unwrap()
}

static BUILD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Held while anything cargo-builds into (or executes from) the shared `target/gen`.
pub fn build_lock() -> std::sync::MutexGuard<'static, ()> {
    BUILD_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// `Command::output` under the build lock, for `agentmark run/trace/build` (they cargo-build).
pub trait OutputLocked {
    fn output_locked(&mut self) -> Output;
}

impl OutputLocked for Command {
    fn output_locked(&mut self) -> Output {
        let _g = build_lock();
        self.output().unwrap()
    }
}

/// Builds the generated project in `out` and copies its binary to `out/<name>.bin`, so the test
/// runs a private copy that no later build in the shared target folder can relink under it.
/// Asserts the build succeeds without warnings.
pub fn build_bin(out: &Path, name: &str) -> PathBuf {
    let _g = build_lock();
    let b = cargo_build_in(out);
    assert!(b.status.success(), "{name}: cargo build failed:\n{}", stderr(&b));
    assert!(!stderr(&b).contains("warning"), "{name}: generated code must be warning free:\n{}", stderr(&b));
    let copy = out.join(format!("{name}.bin"));
    std::fs::copy(gen_bin(name), &copy).unwrap();
    copy
}

/// Every fixture folder of `fixtures/<example>/` (the folder itself and each `variants/<case>/`)
/// that has an `expected.trace.jsonl`, as `(label, dir)`; the plain fixture comes first.
pub fn fixture_dirs_of(example: &str) -> Vec<(String, PathBuf)> {
    let dir = PathBuf::from(format!("{ROOT}/fixtures/{example}"));
    let mut out = Vec::new();
    if dir.join("expected.trace.jsonl").is_file() {
        out.push((example.to_string(), dir.clone()));
    }
    let mut vs: Vec<_> = std::fs::read_dir(dir.join("variants")).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    vs.sort();
    for v in vs {
        assert!(v.join("expected.trace.jsonl").is_file(), "{} has no expected.trace.jsonl", v.display());
        out.push((format!("{example}/variants/{}", v.file_name().unwrap().to_string_lossy()), v));
    }
    out
}

/// Every example name that has a fixture folder, sorted.
pub fn fixture_examples() -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(format!("{ROOT}/fixtures")).unwrap().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    names.sort();
    names.iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect()
}

/// Every fixture folder of every example, as `(example, dir)`.
pub fn fixture_dirs() -> Vec<(String, PathBuf)> {
    fixture_examples().into_iter().flat_map(|n| fixture_dirs_of(&n).into_iter().map(move |(_, d)| (n.clone(), d))).collect()
}
