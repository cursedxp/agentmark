//! Builds, tests and packages `editors/vscode` (step 16). Skipped with a message when npm is missing.

use std::path::Path;
use std::process::Command;

fn run(dir: &Path, program: &str, args: &[&str]) {
    let out = Command::new(program).args(args).current_dir(dir).output().expect("spawn");
    assert!(
        out.status.success(),
        "`{program} {}` failed\n{}\n{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn the_vscode_extension_builds_tests_and_packages() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("editors/vscode");
    if Command::new("npm").arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!("SKIPPED: npm is not installed, so editors/vscode was not built");
        return;
    }
    let out = tempfile::tempdir().unwrap();
    run(&dir, "npm", &["ci", "--no-audit", "--no-fund"]);
    run(&dir, "npm", &["run", "compile"]);
    run(&dir, "npm", &["test"]);
    run(&dir, "npx", &["@vscode/vsce", "package", "--out", out.path().to_str().unwrap()]);
    let made = std::fs::read_dir(out.path()).unwrap().any(|e| {
        e.unwrap().file_name().to_string_lossy().ends_with(".vsix")
    });
    assert!(made, "no .vsix was written");
}
