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

/// The examples whose fixtures are traces (v0.1/v0.2 contract); 06-10 use the program contract (SPEC 22.7).
pub const AGENT_FIXTURES: [&str; 5] = ["01-invoice-chaser", "02-lead-qualifier", "03-support-triage", "04-daily-report", "05-invoice-service"];

/// Every fixture folder of every agent example, as `(example, dir)`.
pub fn fixture_dirs() -> Vec<(String, PathBuf)> {
    AGENT_FIXTURES.iter().map(|n| n.to_string()).flat_map(|n| fixture_dirs_of(&n).into_iter().map(move |(_, d)| (n.clone(), d))).collect()
}

/// Every folder of a program fixture (SPEC 22.7) of `example`: the folder itself, then each `variants/<case>/`,
/// as `(label, dir)`. Unlike `fixture_dirs_of` it does not need an `expected.trace.jsonl`.
pub fn program_fixture_dirs_of(example: &str) -> Vec<(String, PathBuf)> {
    let dir = PathBuf::from(format!("{ROOT}/fixtures/{example}"));
    let mut out = vec![(example.to_string(), dir.clone())];
    let mut vs: Vec<_> = std::fs::read_dir(dir.join("variants")).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    vs.sort();
    for v in vs {
        out.push((format!("{example}/variants/{}", v.file_name().unwrap().to_string_lossy()), v));
    }
    out
}

/// `name` inside fixture folder `dir`, or, when a variant lacks it, inside the parent fixture (SPEC 22.7).
pub fn fixture_part(dir: &Path, name: &str) -> Option<PathBuf> {
    let own = dir.join(name);
    if own.exists() {
        return Some(own);
    }
    let parent = dir.parent()?;
    if parent.file_name()? == "variants" {
        let p = parent.parent()?.join(name);
        return p.exists().then_some(p);
    }
    None
}

/// Every file under `dir` as `(relative path with '/', bytes)`, sorted.
pub fn tree_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, d: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.push((rel, std::fs::read(&p).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// Removes a path when dropped, so a failing assertion leaves nothing behind.
struct Cleanup(Vec<PathBuf>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_dir_all(p);
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Runs a built program on one program fixture (SPEC 22.7): `input/` copied into a fresh working directory,
/// `run.json` arguments (and optional stdin), then `expected.stdout`, `expected/` (exactly that file set) and an
/// optional `expected.trace.jsonl` are compared. A fixture with `requests.json` is a service: the requests are
/// replayed (`--once --mock`) and their `expect_status` / `expect_body` checked by the program itself.
pub fn run_program_fixture(bin: &Path, label: &str, dir: &Path) {
    let work = temp("fx");
    let mut cleanup = Cleanup(vec![]);
    if let Some(input) = fixture_part(dir, "input") {
        copy_dir(&input, &work);
    }
    let requests = fixture_part(dir, "requests.json");
    let mut cmd = Command::new(bin);
    cmd.current_dir(&*work);
    let mut stdin_text = None;
    if let Some(r) = &requests {
        std::fs::copy(r, work.join("requests.json")).unwrap();
        // mock mode wants a fixtures folder with a model file, which a non-agent service never asks
        let fx = work.join("..").join(format!("{}-fixtures", work.file_name().unwrap().to_string_lossy()));
        write(&fx, "model.json", r#"{"today": "2026-10-14", "responses": []}"#);
        cmd.args(["--once", "--mock", "--fixtures"]).arg(&fx);
        cleanup.0.push(fx);
    } else {
        let run: Value = serde_json::from_str(&std::fs::read_to_string(fixture_part(dir, "run.json").unwrap_or_else(|| panic!("{label}: no run.json"))).unwrap()).unwrap();
        for a in run["args"].as_array().into_iter().flatten() {
            cmd.arg(a.as_str().unwrap());
        }
        stdin_text = run.get("stdin").and_then(|v| v.as_str()).map(String::from);
    }
    let trace_file = fixture_part(dir, "expected.trace.jsonl").map(|_| work.join("..").join(format!("{}-trace.jsonl", work.file_name().unwrap().to_string_lossy())));
    if let Some(t) = &trace_file {
        cleanup.0.push(t.clone());
        cmd.arg("--trace").arg(t);
    }
    cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    {
        use std::io::Write;
        let mut si = child.stdin.take().unwrap();
        if let Some(t) = &stdin_text {
            si.write_all(t.as_bytes()).unwrap();
        }
    }
    let o = child.wait_with_output().unwrap();
    assert!(o.status.success(), "{label}: the program failed ({}):\n{}", o.status, stderr(&o));
    if let Some(want) = fixture_part(dir, "expected.stdout") {
        assert_eq!(stdout(&o), std::fs::read_to_string(want).unwrap(), "{label}: stdout differs");
    }
    if let Some(want) = fixture_part(dir, "expected") {
        // the replayed `requests.json` of a service is input, not a result
        let got: Vec<_> = tree_files(&work).into_iter().filter(|(n, _)| !(requests.is_some() && n == "requests.json")).collect();
        let want = tree_files(&want);
        let names = |v: &[(String, Vec<u8>)]| v.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
        assert_eq!(names(&got), names(&want), "{label}: the working directory must hold exactly the expected files");
        for ((n, g), (_, w)) in got.iter().zip(&want) {
            assert!(g == w, "{label}: file {n} differs:\n got: {}\nwant: {}", String::from_utf8_lossy(g), String::from_utf8_lossy(w));
        }
    }
    if let (Some(t), Some(want)) = (&trace_file, fixture_part(dir, "expected.trace.jsonl")) {
        let got = parse_lines(&std::fs::read_to_string(t).unwrap());
        let want = parse_lines(&std::fs::read_to_string(want).unwrap());
        assert_eq!(got, want, "{label}: trace differs");
    }
}
