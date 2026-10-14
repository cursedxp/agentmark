//! `agentmark build`, `run` and `trace` (SPEC 9, 15.1, 15.3, 17).
//!
//! `build` writes the Cargo project. `run` and `trace` write it into a cache folder, compile it
//! with cargo and run the binary with `--once --mock`. A compiler error inside user Rust (a
//! `## Rust` block or a binding body) is reported against the `.agent.md` line; any other rustc
//! failure is an internal error of agentmark, never a user error (SPEC 10).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use super::{bind, check, fail, plural, require_source};
use crate::binder::core::parse_header;
use crate::binder::{bind as run_binder, BindOptions};
use crate::codegen::rust::{self as cg, GeneratedFile};
use crate::diag::Severity;
use crate::ir;
use crate::project::{Project, ResolvedImport};
use crate::sema;

/// A project that passed every check, lowered and ready for codegen.
struct Prepared {
    project: Project,
    program: ir::Program,
    versions: BTreeMap<String, String>,
}

fn is_offline() -> bool {
    matches!(std::env::var("AGENTMARK_OFFLINE").as_deref(), Ok("1") | Ok("true"))
}

/// Check, auto-bind imported crates, refuse unapproved bindings, lower to IR.
fn prepare(path: &str, allow_unverified: bool) -> Result<Prepared, u8> {
    require_source(path)?;
    let mut announced: Vec<String> = Vec::new();
    for _round in 0..2 {
        let project = Project::load(path);
        // SPEC 15.1: every imported crate has a binding before the project is checked
        let generated_any = match bind_missing(&project, &mut announced, true) {
            Ok(g) => g,
            Err((name, e)) => {
                eprintln!("agentmark: cannot bind \"{name}\": {e}");
                return Err(1);
            }
        };
        if generated_any {
            continue;
        }
        let checked = match sema::check(&project) {
            Ok(c) => c,
            Err(diags) => {
                eprintln!("{}\n", project.render(&diags));
                let errors = diags.iter().filter(|d| d.severity == Severity::Error).count();
                eprintln!("{}: not built.", super::plural(errors, "error"));
                return Err(1);
            }
        };

        let shown: Vec<_> = checked.warnings.iter().filter(|d| d.code != "W005").cloned().collect();
        if !shown.is_empty() {
            eprintln!("{}\n", project.render(&shown));
        }
        let unverified: Vec<_> = checked.unverified_bindings().collect();
        if !unverified.is_empty() && !allow_unverified {
            eprintln!("Not built: these libraries are not approved yet (SPEC 15.3). Change the line to approve, or use --allow-unverified while developing.");
            for b in unverified {
                let line = check::effect_line(&project, b.file, &b.name);
                let to = b.proposed_effect.map(sema::effects::effect_name).unwrap_or("network");
                eprintln!(
                    "  {} (library {}): {}:{line}: change \"- effect: unverified\" to \"- effect: {to}\" (or the right class)",
                    b.name, b.crate_name, checked.files[b.file]
                );
            }
            return Err(1);
        }
        let program = ir::lower(&project, &checked);
        for n in cg::notes(&program, &project.root) {
            eprintln!("{n}");
        }
        let versions = crate_versions(&project);
        return Ok(Prepared { project, program, versions });
    }
    eprintln!("agentmark: the binding was written but the project still cannot find it");
    Err(1)
}

/// Bind every imported crate that has no binding yet (SPEC 15.1); shared by `build` and `check`.
/// `Ok(true)` when something was generated (the project has to be loaded again), `Err` carries
/// the crate and why it could not be bound. `announced` keeps each crate to one status line.
pub(crate) fn bind_missing(
    project: &Project,
    announced: &mut Vec<String>,
    say_cached: bool,
) -> Result<bool, (String, String)> {
    let mut generated_any = false;
    let mut crates: Vec<(String, Option<usize>)> = Vec::new();
    for f in &project.files {
        for imp in &f.imports {
            if let ResolvedImport::Crate { name, binding } = imp {
                if !crates.iter().any(|(n, _)| n == name) {
                    crates.push((name.clone(), *binding));
                }
            }
        }
    }
    for (name, binding) in &crates {
        if announced.contains(name) {
            continue;
        }
        announced.push(name.clone());
        if binding.is_some() {
            if say_cached {
                eprintln!("binding {name}: cached");
            }
            continue;
        }
        auto_bind(name, project).map_err(|e| (name.clone(), e))?;
        eprintln!("binding {name}: generated");
        generated_any = true;
    }
    Ok(generated_any)
}

fn auto_bind(name: &str, project: &Project) -> Result<(), String> {
    let (model, mock_today) = bind::model_from_env()?;
    let dir = Path::new(&project.root).join("bindings");
    let mut opts = BindOptions::new(&dir, model, mock_today.unwrap_or_else(bind::system_today));
    opts.offline = is_offline();
    run_binder(name, &opts).map(|_| ()).map_err(|e| e.to_string())
}

/// Crate -> version from the header of its binding file.
fn crate_versions(project: &Project) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for f in &project.files {
        for imp in &f.imports {
            if let ResolvedImport::Crate { name, binding: Some(i) } = imp {
                if let Some(h) = parse_header(&project.files[*i].text) {
                    out.insert(name.clone(), h.version);
                }
            }
        }
    }
    out
}

fn options(p: &Prepared, path: &str) -> cg::Options {
    let stem = Path::new(path.trim_end_matches('/'))
        .file_name()
        .map(|s| s.to_string_lossy().trim_end_matches(".agent.md").to_string())
        .unwrap_or_else(|| "agentmark-app".into());
    cg::Options {
        root: p.project.root.clone(),
        rt_path: cg::default_rt_path(),
        crate_versions: p.versions.clone(),
        fallback_name: stem,
    }
}

/// The author's real tool implementations live in the PROJECT, at `rust/connect.rs` beside
/// `agentmark.toml` (Q135, amended in step 10): a build folder can be deleted without losing them.
/// Every build copies that file to `src/connect.rs` of the generated project. The first time it is
/// missing it is created from the stub template (an older `src/connect.rs` of the output is kept as
/// the start), and an existing file is never touched. A `--mock` run does not need the file, so it
/// neither creates one nor fails for the lack of one.
fn adopt_connect(prepared: &Prepared, dir: &Path, files: &mut [GeneratedFile], create: bool) -> Result<(), String> {
    let Some(slot) = files.iter_mut().find(|f| f.path == USER_FILE) else { return Ok(()) };
    let path = connect_path(&prepared.project.root);
    if !path.exists() {
        if !create {
            return Ok(());
        }
        let seed = std::fs::read_to_string(dir.join(USER_FILE)).unwrap_or_else(|_| slot.content.clone());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::write(&path, seed).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        eprintln!("created {}: your real tool implementations go here (build copies it into the output)", path.display());
    }
    slot.content = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    slot.user_owned = false;
    Ok(())
}

/// `<project>/rust/connect.rs`.
fn connect_path(root: &str) -> PathBuf {
    Path::new(if root.is_empty() { "." } else { root }).join("rust/connect.rs")
}

/// Write the project, removing unit files of an earlier build that no longer exist.
fn write_project(out: &Path, files: &[GeneratedFile]) -> Result<(), String> {
    let src = out.join("src");
    std::fs::create_dir_all(&src).map_err(|e| format!("cannot create {}: {e}", src.display()))?;
    if let Ok(rd) = std::fs::read_dir(&src) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            let ours = ["agent_", "service_", "command_"].iter().any(|p| n.starts_with(p)) && n.ends_with(".rs");
            if ours && !files.iter().any(|f| f.path == format!("src/{n}")) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    for f in files {
        let p = out.join(&f.path);
        if f.user_owned && p.exists() {
            continue;
        }
        std::fs::write(&p, &f.content).map_err(|e| format!("cannot write {}: {e}", p.display()))?;
    }
    Ok(())
}

pub(crate) fn build(path: &str, target: &str, out: &str, allow_unverified: bool, no_check: bool) -> u8 {
    if target != "rust" {
        return fail(format!("unknown target \"{target}\": only \"rust\" is implemented"));
    }
    let prepared = match prepare(path, allow_unverified) {
        Ok(p) => p,
        Err(c) => return c,
    };
    let mut files = cg::generate(&prepared.program, &options(&prepared, path));
    let dir = Path::new(out);
    if let Err(e) = adopt_connect(&prepared, dir, &mut files, true) {
        return fail(e);
    }
    if let Err(e) = write_project(dir, &files) {
        return fail(e);
    }
    if !no_check {
        // the generated project must compile: an error in the author's Rust is reported at its
        // .agent.md line, anything else is a bug of agentmark
        match cargo_build(dir, "check") {
            Ok(run) if run.ok => {}
            Ok(run) => return report_failure(&prepared, dir, &files, &run),
            Err(e) => return fail(e),
        }
    }
    println!("built {} ({}). Compile it with: cargo build --release --manifest-path {}/Cargo.toml", out, plural(files.len(), "file"), out.trim_end_matches('/'));
    0
}

// ---------------------------------------------------------------------------------- cargo

/// One error line of rustc: where, and what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RustcError {
    pub file: String,
    pub line: u32,
    pub message: String,
    /// Other places the message points at (secondary spans and sub-messages).
    pub others: Vec<(String, u32)>,
}

/// An error in code the author wrote, mapped back to the `.agent.md` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UserError {
    pub file: String,
    pub line: u32,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    /// Errors inside `## Rust` blocks or binding bodies.
    User(Vec<UserError>),
    /// Anything else: agentmark generated code that does not compile.
    Ice { location: String, message: String },
}

/// Which `.agent.md` line a generated line comes from, if it lies in a user block of
/// `bindings.rs`: the verbatim code (line by line), or the wrapper signature above it (the line of
/// the block's heading).
fn map_user_line(content: &str, line: u32) -> Option<(String, u32)> {
    let lines: Vec<&str> = content.lines().collect();
    let idx = (line as usize).checked_sub(1)?;
    if idx >= lines.len() {
        return None;
    }
    for i in (0..=idx).rev() {
        let t = lines[i].trim();
        if t == "// agentmark:end" {
            return None;
        }
        if let Some(rest) = t.strip_prefix("// agentmark:begin ") {
            let (file, l) = rest.rsplit_once(':')?;
            let first: u32 = l.parse().ok()?;
            // the first code line follows the marker
            let offset = (idx as i64) - (i as i64 + 1);
            if offset < 0 {
                return None;
            }
            return Some((file.to_string(), first + offset as u32));
        }
        if let Some(rest) = t.strip_prefix("// agentmark:block ") {
            let (file, l) = rest.rsplit_once(':')?;
            return Some((file.to_string(), l.parse().ok()?));
        }
    }
    None
}

/// Where the author's own Rust lives besides `.agent.md` blocks.
const USER_FILE: &str = "src/connect.rs";

/// Sort rustc's errors into the author's and agentmark's own. `files` are the generated files
/// (`src/bindings.rs` ...), to find the verbatim regions. An error whose primary span is in
/// generated wrapper code but that also points into a user block (for example a block that ends
/// in `;` and so returns `()`) is the author's.
pub(crate) fn classify(errors: &[RustcError], files: &[GeneratedFile]) -> Failure {
    let mut user = Vec::new();
    let mut first_other: Option<&RustcError> = None;
    let locate = |file: &str, line: u32| -> Option<(String, u32)> {
        if file == USER_FILE {
            return Some((file.to_string(), line));
        }
        let content = files.iter().find(|f| f.path == file).map(|f| f.content.as_str())?;
        map_user_line(content, line)
    };
    for e in errors {
        let hit = locate(&e.file, e.line).or_else(|| e.others.iter().find_map(|(f, l)| locate(f, *l)));
        match hit {
            Some((file, line)) => user.push(UserError { file, line, message: e.message.clone() }),
            None => {
                first_other.get_or_insert(e);
            }
        }
    }
    if !user.is_empty() {
        user.dedup();
        return Failure::User(user);
    }
    match first_other {
        Some(e) => Failure::Ice { location: format!("{}:{}", e.file, e.line), message: e.message.clone() },
        None => Failure::Ice { location: String::new(), message: "cargo failed without a compiler error".into() },
    }
}

struct CargoRun {
    ok: bool,
    errors: Vec<RustcError>,
    binary: Option<PathBuf>,
    stderr: String,
}

fn cargo_build(dir: &Path, sub: &str) -> Result<CargoRun, String> {
    let mut cmd = Command::new("cargo");
    cmd.arg(sub).arg("--quiet").arg("--message-format=json").arg("--manifest-path").arg(dir.join("Cargo.toml"));
    if is_offline() {
        cmd.arg("--offline");
    }
    if std::env::var_os("CARGO_TARGET_DIR").is_none() {
        cmd.env("CARGO_TARGET_DIR", std::env::temp_dir().join("agentmark-target"));
    }
    let out = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output().map_err(|e| format!("cannot run cargo: {e}"))?;
    let mut errors = Vec::new();
    let mut binary = None;
    for l in String::from_utf8_lossy(&out.stdout).lines() {
        let Ok(v) = serde_json::from_str::<Value>(l) else { continue };
        match v["reason"].as_str() {
            Some("compiler-message") => {
                let m = &v["message"];
                if m["level"].as_str() != Some("error") {
                    continue;
                }
                let msg = m["message"].as_str().unwrap_or_default().to_string();
                if msg.starts_with("aborting due to") {
                    continue;
                }
                let mut spans: Vec<(String, u32, bool)> = Vec::new();
                fn collect(m: &Value, spans: &mut Vec<(String, u32, bool)>) {
                    for s in m["spans"].as_array().into_iter().flatten() {
                        spans.push((
                            s["file_name"].as_str().unwrap_or_default().to_string(),
                            s["line_start"].as_u64().unwrap_or(0) as u32,
                            s["is_primary"].as_bool() == Some(true),
                        ));
                    }
                    for c in m["children"].as_array().into_iter().flatten() {
                        collect(c, spans);
                    }
                }
                collect(m, &mut spans);
                let primary = spans.iter().find(|s| s.2).or(spans.first()).cloned().unwrap_or_default();
                let others = spans.iter().filter(|s| (&s.0, s.1) != (&primary.0, primary.1)).map(|s| (s.0.clone(), s.1)).collect();
                errors.push(RustcError { file: primary.0, line: primary.1, message: msg, others });
            }
            Some("compiler-artifact") => {
                if let Some(e) = v["executable"].as_str() {
                    binary = Some(PathBuf::from(e));
                }
            }
            _ => {}
        }
    }
    Ok(CargoRun { ok: out.status.success(), errors, binary, stderr: String::from_utf8_lossy(&out.stderr).to_string() })
}

fn report_failure(prepared: &Prepared, dir: &Path, files: &[GeneratedFile], run: &CargoRun) -> u8 {
    if run.errors.is_empty() {
        // cargo itself failed (dependencies, network): not a language error either
        eprintln!("agentmark: cargo could not build the generated project in {}:\n{}", dir.display(), run.stderr.trim());
        return 2;
    }
    match classify(&run.errors, files) {
        Failure::User(errs) => {
            for e in &errs {
                if e.file == USER_FILE {
                    // the project's copy when there is one: that is the file to fix
                    let own = connect_path(&prepared.project.root);
                    let path = if own.exists() { own } else { dir.join(&e.file) };
                    let text = std::fs::read_to_string(&path).ok().and_then(|t| t.lines().nth(e.line as usize - 1).map(String::from)).unwrap_or_default();
                    eprintln!("{}:{}\n  {}\n  This file is yours (agentmark never rewrites it) and it does not compile: {}\n", path.display(), e.line, text.trim_end(), e.message);
                    continue;
                }
                let shown = crate::project::normalize(&format!("{}/{}", prepared.project.root, e.file));
                let text = prepared.project.sources.get(&shown).and_then(|s| s.lines().nth(e.line as usize - 1)).unwrap_or("");
                eprintln!("{shown}:{}\n  {}\n  The Rust code in this block does not compile: {}\n", e.line, text.trim_end(), e.message);
            }
            1
        }
        Failure::Ice { location, message } => {
            eprintln!(
                "internal compiler error: generated Rust did not compile — this is a bug in agentmark, not in your project.\n  {}/{} : {}\n  Please report it with the project that caused it.",
                dir.display(),
                location,
                message
            );
            2
        }
    }
}

/// A stable folder for the generated project of `path`.
fn cache_dir(path: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    abs.hash(&mut h);
    let name = abs.file_name().map(|n| n.to_string_lossy().replace(".agent.md", "")).unwrap_or_else(|| "project".into());
    std::env::temp_dir().join("agentmark-build").join(format!("{name}-{:x}", h.finish()))
}

fn build_and_run(path: &str, mock: bool, fixtures: Option<&str>, allow_unverified: bool, trace: bool) -> u8 {
    if mock && fixtures.is_none() {
        return fail("--mock needs --fixtures <folder> with model.json and the tool fixtures");
    }
    if let Some(f) = fixtures {
        if !Path::new(f).is_dir() {
            return fail(format!("cannot find the fixtures folder \"{f}\""));
        }
    }
    let prepared = match prepare(path, allow_unverified) {
        Ok(p) => p,
        Err(c) => return c,
    };
    let mut files = cg::generate(&prepared.program, &options(&prepared, path));
    let dir = cache_dir(path);
    if let Err(e) = adopt_connect(&prepared, &dir, &mut files, !mock) {
        return fail(e);
    }
    if let Err(e) = write_project(&dir, &files) {
        return fail(e);
    }
    let run = match cargo_build(&dir, "build") {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    if !run.ok {
        return report_failure(&prepared, &dir, &files, &run);
    }
    let Some(bin) = run.binary else {
        return fail("cargo built nothing to run");
    };
    let mut cmd = Command::new(bin);
    cmd.arg("--once");
    if mock {
        cmd.arg("--mock");
    }
    if let Some(f) = fixtures {
        cmd.arg("--fixtures").arg(f);
    }
    if trace {
        cmd.arg("--trace").arg("-");
    }
    match cmd.status() {
        Ok(s) => s.code().unwrap_or(2).clamp(0, 255) as u8,
        Err(e) => fail(format!("cannot run the built program: {e}")),
    }
}

pub(crate) fn run(path: &str, mock: bool, fixtures: Option<&str>, allow_unverified: bool) -> u8 {
    build_and_run(path, mock, fixtures, allow_unverified, false)
}

pub(crate) fn trace(path: &str, mock: bool, fixtures: Option<&str>, allow_unverified: bool) -> u8 {
    build_and_run(path, mock, fixtures, allow_unverified, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gf(path: &str, content: &str) -> GeneratedFile {
        GeneratedFile { path: path.into(), content: content.into(), user_owned: false }
    }

    fn err(file: &str, line: u32, message: &str) -> RustcError {
        RustcError { file: file.into(), line, message: message.into(), others: vec![] }
    }

    const BINDINGS: &str = "//! header\n\n// agentmark:block app/x.agent.md:18\npub fn shout() -> String {\n    // agentmark:begin app/x.agent.md:20\n    let a = 1;\n    a.nope()\n    // agentmark:end\n}\n\nfn other() {}\n";

    #[test]
    fn an_error_inside_a_user_block_maps_to_its_agent_md_line() {
        let f = classify(&[err("src/bindings.rs", 7, "no method")], &[gf("src/bindings.rs", BINDINGS)]);
        assert_eq!(f, Failure::User(vec![UserError { file: "app/x.agent.md".into(), line: 21, message: "no method".into() }]));
    }

    #[test]
    fn the_first_line_of_the_block_is_the_line_after_the_marker() {
        let f = classify(&[err("src/bindings.rs", 6, "m")], &[gf("src/bindings.rs", BINDINGS)]);
        assert_eq!(f, Failure::User(vec![UserError { file: "app/x.agent.md".into(), line: 20, message: "m".into() }]));
    }

    #[test]
    fn an_error_outside_user_code_is_an_internal_compiler_error() {
        for (file, line) in [("src/bindings.rs", 2), ("src/bindings.rs", 11), ("src/main.rs", 4), ("src/agent_a.rs", 1)] {
            let f = classify(&[err(file, line, "mismatched types")], &[gf("src/bindings.rs", BINDINGS), gf("src/main.rs", "fn main() {}\n")]);
            assert_eq!(f, Failure::Ice { location: format!("{file}:{line}"), message: "mismatched types".into() }, "{file}:{line}");
        }
    }

    #[test]
    fn user_errors_win_over_the_errors_they_cause_elsewhere() {
        let f = classify(
            &[err("src/main.rs", 9, "cascade"), err("src/bindings.rs", 7, "root cause")],
            &[gf("src/bindings.rs", BINDINGS), gf("src/main.rs", "")],
        );
        assert!(matches!(f, Failure::User(v) if v.len() == 1 && v[0].message == "root cause"));
    }

    #[test]
    fn an_error_on_the_wrapper_signature_belongs_to_the_block_heading() {
        // a block that ends in `;` returns `()`: rustc points at the generated signature
        let f = classify(&[err("src/bindings.rs", 4, "mismatched types")], &[gf("src/bindings.rs", BINDINGS)]);
        assert_eq!(f, Failure::User(vec![UserError { file: "app/x.agent.md".into(), line: 18, message: "mismatched types".into() }]));
    }

    #[test]
    fn a_secondary_span_in_user_code_makes_the_error_the_authors() {
        let mut e = err("src/agent_a.rs", 40, "mismatched types");
        e.others = vec![("src/bindings.rs".into(), 7)];
        let f = classify(&[e], &[gf("src/bindings.rs", BINDINGS), gf("src/agent_a.rs", "")]);
        assert!(matches!(f, Failure::User(v) if v[0].line == 21));
    }

    #[test]
    fn errors_in_connect_rs_are_the_users_too() {
        let f = classify(&[err("src/connect.rs", 12, "cannot find trait")], &[]);
        assert_eq!(f, Failure::User(vec![UserError { file: "src/connect.rs".into(), line: 12, message: "cannot find trait".into() }]));
    }

    #[test]
    fn no_errors_at_all_is_still_not_a_user_error() {
        assert!(matches!(classify(&[], &[]), Failure::Ice { .. }));
    }
}
