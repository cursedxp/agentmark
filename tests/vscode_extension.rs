//! Builds, tests and packages `editors/vscode` (step 16) in a temporary copy, so the checkout gets
//! no `node_modules`, `out` or `.vsix`. Skipped with a message when npm is missing.

use std::path::Path;
use std::process::Command;

fn run(dir: &Path, program: &str, args: &[&str]) -> String {
    let out = Command::new(program).args(args).current_dir(dir).output().expect("spawn");
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "`{program} {}` failed\n{text}", args.join(" "));
    text
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let name = e.file_name();
        if matches!(name.to_str(), Some("node_modules" | "out" | "dist")) || name.to_string_lossy().ends_with(".vsix") {
            continue;
        }
        let dest = to.join(&name);
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}

#[test]
fn the_vscode_extension_builds_tests_and_packages() {
    if Command::new("npm").arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!("SKIPPED: npm is not installed, so editors/vscode was not built");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tmp = tempfile::tempdir().unwrap();
    // the tests read ../../examples and the schema, so keep the editors/vscode depth
    let dir = tmp.path().join("editors/vscode");
    copy_dir(&root.join("editors/vscode"), &dir);
    copy_dir(&root.join("examples"), &tmp.path().join("examples"));
    let out = tmp.path().join("vsix");
    std::fs::create_dir_all(&out).unwrap();
    run(&dir, "npm", &["ci", "--no-audit", "--no-fund"]);
    run(&dir, "npm", &["run", "compile"]);
    run(&dir, "npm", &["test"]);
    let log = run(&dir, "npx", &["@vscode/vsce", "package", "--out", out.to_str().unwrap()]);
    assert!(!log.contains("WARNING"), "vsce warned:\n{log}");
    assert!(std::fs::read_dir(&out).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".vsix")), "no .vsix was written");
}
