//! `agentmark fmt` and `agentmark lint`.

use super::files::{in_bindings, source_files};
use super::{fail, plural, require_source};
use crate::diag::{render_all, Severity};
use crate::lint::{lint_with_sources, LintConfig};
use crate::project::Project;

pub(crate) fn fmt(path: &str, check: bool) -> u8 {
    if let Err(c) = require_source(path) {
        return c;
    }
    let files = match source_files(path) {
        Ok(f) => f,
        Err(e) => return fail(e),
    };
    let (mut changed, mut broken) = (0usize, false);
    for f in &files {
        let src = match std::fs::read_to_string(f) {
            Ok(s) => s,
            Err(e) => return fail(format!("cannot read {f}: {e}")),
        };
        match crate::fmt::format_source(&src, f) {
            Err(diags) => {
                eprintln!("{}", render_all(&diags, &src));
                broken = true;
            }
            Ok(out) if out == src => {}
            Ok(out) => {
                changed += 1;
                if check {
                    println!("not canonical: {f}");
                } else if let Err(e) = write_atomic(f, &out) {
                    return fail(format!("cannot write {f}: {e}"));
                } else {
                    println!("formatted {f}");
                }
            }
        }
    }
    if check {
        if changed > 0 {
            println!(
                "{changed} of {} are not canonical; run \"agentmark fmt\" to fix {}.",
                plural(files.len(), "file"),
                if changed == 1 { "it" } else { "them" }
            );
        } else if !broken {
            println!("{} canonical.", if files.len() == 1 { "1 file is".to_string() } else { format!("{} files are", files.len()) });
        }
        u8::from(changed > 0 || broken)
    } else {
        println!("{changed} of {} rewritten.", plural(files.len(), "file"));
        u8::from(broken)
    }
}

/// Write next to the target, then rename: a crash never leaves half a file.
fn write_atomic(path: &str, text: &str) -> std::io::Result<()> {
    let tmp = format!("{path}.agentmark-tmp");
    std::fs::write(&tmp, text)?;
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

pub(crate) fn lint(path: &str) -> u8 {
    if let Err(c) = require_source(path) {
        return c;
    }
    let project = Project::load(path);
    let errors: Vec<_> = project
        .load_diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .cloned()
        .collect();
    if !errors.is_empty() {
        eprintln!("{}", project.render(&errors));
        eprintln!("\nThe project does not load, so it cannot be linted. Fix the errors above first.");
        return 1;
    }
    let cfg = LintConfig::new(project.config.lint_deny.iter().cloned());
    let files: Vec<_> = project
        .files
        .iter()
        .filter(|f| !in_bindings(&f.path))
        .map(|f| (&f.ast, f.text.as_str()))
        .collect();
    let diags = lint_with_sources(&files, &cfg);
    if !diags.is_empty() {
        eprintln!("{}\n", project.render(&diags));
    }
    let errs = diags.iter().filter(|d| d.severity == Severity::Error).count();
    let warns = diags.len() - errs;
    println!(
        "lint: {}, {} in {}.",
        plural(errs, "error"),
        plural(warns, "warning"),
        plural(files.len(), "file")
    );
    u8::from(errs > 0)
}
