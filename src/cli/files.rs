//! Finding the `.agent.md` files a command works on.

use std::path::Path;

/// Folders the walk never enters. Generated bindings are skipped too: they are rewritten by
/// `agentmark bind`, so formatting or linting them would only produce noise (Q111).
fn skipped_dir(name: &str) -> bool {
    matches!(name, "docs" | "fixtures" | "target" | "node_modules" | "bindings")
        || name.starts_with('.')
}

/// Is this display path inside a `bindings/` folder?
pub(crate) fn in_bindings(path: &str) -> bool {
    path.split('/').any(|c| c == "bindings")
}

/// `path` itself when it is a file, else every `*.agent.md` below it (sorted).
pub(crate) fn source_files(path: &str) -> Result<Vec<String>, String> {
    let p = Path::new(path);
    if p.is_file() {
        return Ok(vec![path.to_string()]);
    }
    if !p.is_dir() {
        return Err(format!("cannot find \"{path}\""));
    }
    let mut out = Vec::new();
    walk(p, &mut out);
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.filter_map(|e| e.ok()) {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if !skipped_dir(&name) {
                walk(&path, out);
            }
        } else if name.ends_with(".agent.md") {
            out.push(path.to_string_lossy().to_string());
        }
    }
}
