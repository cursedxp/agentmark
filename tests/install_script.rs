//! `install.sh` (the one-command installer). The always-on tests check the syntax, `--help`, the
//! missing-cargo message and the whole flow (clone, update, install, uninstall) against a local
//! git repository with a stand-in `cargo` that records its arguments. The real install, which
//! compiles the compiler and the language server, runs only with `AGENTMARK_TEST_INSTALL=1`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn script() -> PathBuf {
    Path::new(ROOT).join("install.sh")
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A git repository with one commit on `main`, standing in for the GitHub repository.
fn fake_origin(dir: &Path) -> String {
    let repo = dir.join("origin");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("file.txt"), "one\n").unwrap();
    std::fs::create_dir_all(repo.join("crates/agentmark-rt")).unwrap();
    std::fs::write(repo.join("crates/agentmark-rt/.keep"), "").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    repo.to_string_lossy().into_owned()
}

/// A folder with a `cargo` that appends its arguments to `cargo.log` and prints nothing.
fn fake_cargo(dir: &Path) -> PathBuf {
    let bin = dir.join("fakebin");
    std::fs::create_dir_all(&bin).unwrap();
    let cargo = bin.join("cargo");
    std::fs::write(&cargo, format!("#!/bin/sh\necho \"$*\" >> \"{}/cargo.log\"\n", dir.display())).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn run(args: &[&str], envs: &[(&str, &str)]) -> Output {
    Command::new("sh").arg(script()).args(args).envs(envs.iter().copied()).output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn path_with(bin: &Path) -> String {
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}

#[test]
fn script_has_valid_posix_syntax() {
    let out = Command::new("sh").arg("-n").arg(script()).output().unwrap();
    assert!(out.status.success(), "sh -n failed: {}", text(&out));
}

#[test]
fn help_describes_the_options() {
    let out = run(&["--help"], &[]);
    assert!(out.status.success(), "{}", text(&out));
    let t = text(&out);
    for word in ["--uninstall", "AGENTMARK_HOME", "AGENTMARK_REF", "AGENTMARK_REPO", "AGENTMARK_NO_LSP"] {
        assert!(t.contains(word), "--help should mention {word}:\n{t}");
    }
    assert_eq!(run(&["--bogus"], &[]).status.code(), Some(2), "an unknown option is a usage error");
}

#[test]
fn missing_cargo_prints_the_rustup_line_and_fails() {
    let dir = tempfile::tempdir().unwrap();
    // a PATH with git and sh tools but no cargo
    let bin = dir.path().join("nocargo");
    std::fs::create_dir_all(&bin).unwrap();
    for tool in ["git", "mkdir", "rm", "cat", "printf", "dirname"] {
        if let Ok(o) = Command::new("sh").args(["-c", &format!("command -v {tool}")]).output() {
            let p = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if p.starts_with('/') {
                std::os::unix::fs::symlink(p, bin.join(tool)).unwrap();
            }
        }
    }
    let home = dir.path().join("home");
    let out = Command::new("/bin/sh")
        .arg(script())
        .env("PATH", &bin)
        .env("AGENTMARK_HOME", &home)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("https://rustup.rs"), "{}", text(&out));
    assert!(!home.exists(), "nothing is cloned before the toolchain check passes");
}

#[test]
fn install_update_and_uninstall_flow() {
    let dir = tempfile::tempdir().unwrap();
    let origin = fake_origin(dir.path());
    let bin = fake_cargo(dir.path());
    let home = dir.path().join("home");
    let path = path_with(&bin);
    let envs = [("PATH", path.as_str()), ("AGENTMARK_REPO", origin.as_str()), ("AGENTMARK_HOME", home.to_str().unwrap()), ("CARGO_INSTALL_ROOT", dir.path().to_str().unwrap())];

    // first run: clone, then both installs, from the persistent checkout
    let out = run(&[], &envs);
    assert!(out.status.success(), "{}", text(&out));
    let src = home.join("src");
    assert_eq!(std::fs::read_to_string(src.join("file.txt")).unwrap(), "one\n");
    let log = std::fs::read_to_string(dir.path().join("cargo.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert_eq!(lines[0], format!("install --locked --path {}", src.display()));
    assert_eq!(lines[1], format!("install --locked --path {}/crates/agentmark-lsp", src.display()));
    assert!(text(&out).contains("PATH"), "a hint that the bin folder is not on PATH:\n{}", text(&out));
    assert!(text(&out).contains("Keep"), "the warning about the source folder:\n{}", text(&out));

    // second run: the new commit arrives (update, not a second clone), the language server is skipped on request
    let origin_path = Path::new(&origin);
    std::fs::write(origin_path.join("file.txt"), "two\n").unwrap();
    git(origin_path, &["commit", "-q", "-am", "two"]);
    let mut envs2 = envs.to_vec();
    envs2.push(("AGENTMARK_NO_LSP", "1"));
    let out = run(&[], &envs2);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(std::fs::read_to_string(src.join("file.txt")).unwrap(), "two\n");
    let log = std::fs::read_to_string(dir.path().join("cargo.log")).unwrap();
    assert_eq!(log.lines().count(), 3, "one more install, no language server:\n{log}");

    // uninstall: both binaries, then the folder
    let out = run(&["--uninstall"], &envs);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains(src.to_str().unwrap()), "the folder is printed before it is removed");
    assert!(!src.exists() && !home.exists(), "the checkout goes, and the then-empty home folder with it");
    let log = std::fs::read_to_string(dir.path().join("cargo.log")).unwrap();
    assert!(log.contains("uninstall agentmark\n") && log.contains("uninstall agentmark-lsp\n"), "{log}");
}

/// A download that stops half way must run nothing: no command, no folder.
#[test]
fn a_truncated_script_does_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let origin = fake_origin(dir.path());
    let bin = fake_cargo(dir.path());
    let home = dir.path().join("home");
    let full = std::fs::read(script()).unwrap();
    let cut = dir.path().join("cut.sh");
    std::fs::write(&cut, &full[..full.len() * 6 / 10]).unwrap();
    let path = path_with(&bin);
    let out = Command::new("sh")
        .arg(&cut)
        .env("PATH", &path)
        .env("AGENTMARK_REPO", &origin)
        .env("AGENTMARK_HOME", &home)
        .output()
        .unwrap();
    assert!(!out.status.success(), "a cut script is a syntax error");
    assert!(!dir.path().join("cargo.log").exists(), "no cargo command ran");
    assert!(!home.exists(), "nothing was created");
}

/// Uninstall removes only an AgentMark checkout, never an arbitrary folder.
#[test]
fn uninstall_refuses_a_folder_that_is_not_a_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_cargo(dir.path());
    let home = dir.path().join("precious");
    std::fs::create_dir_all(home.join("src")).unwrap();
    std::fs::write(home.join("src/notes.txt"), "keep me").unwrap();
    std::fs::write(home.join("other.txt"), "keep me too").unwrap();
    let path = path_with(&bin);
    let out = run(&["--uninstall"], &[("PATH", path.as_str()), ("AGENTMARK_HOME", home.to_str().unwrap())]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("does not look like an AgentMark checkout"), "{}", text(&out));
    assert!(home.join("src/notes.txt").exists() && home.join("other.txt").exists());
    assert!(!dir.path().join("cargo.log").exists(), "cargo uninstall did not run either");
    for bad in ["/", ""] {
        let out = run(&["--uninstall"], &[("PATH", path.as_str()), ("AGENTMARK_HOME", bad)]);
        assert_eq!(out.status.code(), Some(1), "AGENTMARK_HOME={bad:?}: {}", text(&out));
    }
}

/// An update never discards the user's edits.
#[test]
fn update_with_local_changes_stops_and_keeps_them() {
    let dir = tempfile::tempdir().unwrap();
    let origin = fake_origin(dir.path());
    let bin = fake_cargo(dir.path());
    let home = dir.path().join("home");
    let path = path_with(&bin);
    let envs = [("PATH", path.as_str()), ("AGENTMARK_REPO", origin.as_str()), ("AGENTMARK_HOME", home.to_str().unwrap()), ("CARGO_INSTALL_ROOT", dir.path().to_str().unwrap())];
    assert!(run(&[], &envs).status.success());
    let src = home.join("src");
    std::fs::write(src.join("file.txt"), "my edit\n").unwrap();
    let origin_path = Path::new(&origin);
    std::fs::write(origin_path.join("file.txt"), "two\n").unwrap();
    git(origin_path, &["commit", "-q", "-am", "two"]);
    let installs_before = std::fs::read_to_string(dir.path().join("cargo.log")).unwrap().lines().count();
    let out = run(&[], &envs);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains(&format!("local changes in {}; commit or discard them, then run again", src.display())), "{}", text(&out));
    assert_eq!(std::fs::read_to_string(src.join("file.txt")).unwrap(), "my edit\n");
    assert_eq!(std::fs::read_to_string(dir.path().join("cargo.log")).unwrap().lines().count(), installs_before, "nothing was installed");
}

/// The real thing: clones this workspace, compiles both binaries into a scratch install root and
/// runs the installed compiler. Slow, so it is opt-in.
#[test]
fn real_install_when_asked() {
    if std::env::var("AGENTMARK_TEST_INSTALL").as_deref() != Ok("1") {
        eprintln!("skipped: set AGENTMARK_TEST_INSTALL=1 to run the real install");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin");
    let st = Command::new("git").args(["clone", "-q", ROOT]).arg(&origin).status().unwrap();
    assert!(st.success());
    let home = dir.path().join("home");
    let root = dir.path().join("root");
    let out = run(
        &[],
        &[
            ("AGENTMARK_REPO", origin.to_str().unwrap()),
            ("AGENTMARK_REF", "HEAD"),
            ("AGENTMARK_HOME", home.to_str().unwrap()),
            ("CARGO_INSTALL_ROOT", root.to_str().unwrap()),
        ],
    );
    assert!(out.status.success(), "{}", text(&out));
    let version = Command::new(root.join("bin/agentmark")).arg("--version").output().unwrap();
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("agentmark "));
    assert!(root.join("bin/agentmark-lsp").exists());
    assert!(home.join("src/crates/agentmark-rt").exists());
}

/// The docs name the real repository and the installer, never "this repository".
#[test]
fn docs_name_the_installer_and_the_real_repository() {
    let read = |p: &str| std::fs::read_to_string(Path::new(ROOT).join(p)).unwrap();
    let readme = read("README.md");
    assert!(readme.contains("curl -fsSL https://raw.githubusercontent.com/cursedxp/agentmark/main/install.sh | sh"));
    assert!(readme.contains("git clone https://github.com/cursedxp/agentmark"));
    for p in ["README.md", "docs/guide.md", "docs/reference.md", "docs/internals.md", "docs/libraries.md", "docs/editor.md", "templates/CLAUDE.md", "templates/.cursorrules"] {
        let t = read(p);
        assert!(!t.contains("this repository") && !t.contains("/path/to/agentmark"), "{p} has vague repository wording");
    }
    assert!(read("LICENSE").contains("Copyright (c) 2026 Anil Ozsoy"));
}
