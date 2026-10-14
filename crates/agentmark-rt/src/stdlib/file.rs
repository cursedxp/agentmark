//! The built-in `file` tool (SPEC 22.3, effect `local`, no `tools.toml` entry). Paths are used as written,
//! so relative paths start at the working directory. Every call is traced before it runs:
//! `{"kind":"file","verb":"rename","path":"a","to":"b"}`; a call that fails is traced too.

use crate::{Ctx, RtError};
use std::io::Write;

fn fail(what: &str, path: &str, e: std::io::Error) -> RtError {
    let why = match e.kind() {
        std::io::ErrorKind::NotFound => "it does not exist".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        std::io::ErrorKind::InvalidData => "it is not text (UTF-8)".to_string(),
        _ => e.to_string(),
    };
    RtError::Lib(format!("cannot {what} \"{path}\": {why}"))
}

/// `file read path -> content`: the whole file as text.
pub fn read(ctx: &Ctx, path: &str) -> Result<String, RtError> {
    ctx.trace_file("read", path, None)?;
    std::fs::read_to_string(path).map_err(|e| fail("read", path, e))
}

/// `file write path: content`: exactly `content`; an existing file is replaced.
pub fn write(ctx: &Ctx, path: &str, content: &str) -> Result<(), RtError> {
    ctx.trace_file("write", path, None)?;
    std::fs::write(path, content).map_err(|e| fail("write", path, e))
}

/// `file append path: line`: `line` and a newline at the end (the file is created when missing).
pub fn append(ctx: &Ctx, path: &str, line: &str) -> Result<(), RtError> {
    ctx.trace_file("append", path, None)?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| fail("append to", path, e))?;
    writeln!(f, "{line}").map_err(|e| fail("append to", path, e))
}

/// `file list folder -> names`: the names inside the folder (files and folders), sorted.
pub fn list(ctx: &Ctx, path: &str) -> Result<Vec<String>, RtError> {
    ctx.trace_file("list", path, None)?;
    let mut names = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|e| fail("list", path, e))? {
        let entry = entry.map_err(|e| fail("list", path, e))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

/// `file rename old to new`.
pub fn rename(ctx: &Ctx, old: &str, new: &str) -> Result<(), RtError> {
    ctx.trace_file("rename", old, Some(new))?;
    if std::path::Path::new(new).exists() {
        return Err(RtError::Lib(format!("cannot rename \"{old}\" to \"{new}\": \"{new}\" already exists")));
    }
    std::fs::rename(old, new).map_err(|e| fail("rename", old, e))
}

/// `file exists path -> present`: is there a file or folder at the path?
pub fn exists(ctx: &Ctx, path: &str) -> Result<bool, RtError> {
    ctx.trace_file("exists", path, None)?;
    Ok(std::path::Path::new(path).exists())
}

/// `file delete path`: a file, or an empty folder.
pub fn delete(ctx: &Ctx, path: &str) -> Result<(), RtError> {
    ctx.trace_file("delete", path, None)?;
    let p = std::path::Path::new(path);
    if p.is_dir() {
        std::fs::remove_dir(p).map_err(|e| fail("delete", path, e))
    } else {
        std::fs::remove_file(p).map_err(|e| fail("delete", path, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Clock, MockModel, MockTools, Trace};
    use std::sync::Arc;

    fn ctx() -> (Ctx, Trace) {
        let trace = Trace::memory();
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
        let model = MockModel::new(Some(today), vec![]);
        (Ctx::new(trace.clone(), Arc::new(model), Arc::new(MockTools::empty()), Clock::fixed(today)), trace)
    }

    #[test]
    fn every_verb_works_and_is_traced() {
        let dir = tempfile::tempdir().unwrap();
        let p = |n: &str| dir.path().join(n).to_string_lossy().into_owned();
        let (ctx, trace) = ctx();
        write(&ctx, &p("a.txt"), "one\n").unwrap();
        append(&ctx, &p("a.txt"), "two").unwrap();
        assert_eq!(read(&ctx, &p("a.txt")).unwrap(), "one\ntwo\n");
        write(&ctx, &p("b.txt"), "x").unwrap();
        assert_eq!(list(&ctx, &dir.path().to_string_lossy()).unwrap(), ["a.txt", "b.txt"]);
        rename(&ctx, &p("b.txt"), &p("c.txt")).unwrap();
        assert!(exists(&ctx, &p("c.txt")).unwrap() && !exists(&ctx, &p("b.txt")).unwrap());
        write(&ctx, &p("d.txt"), "y").unwrap();
        let e = rename(&ctx, &p("d.txt"), &p("a.txt")).unwrap_err().to_string();
        assert!(e.ends_with("already exists"), "{e}");
        std::fs::remove_file(p("d.txt")).unwrap();
        delete(&ctx, &p("c.txt")).unwrap();
        assert!(!exists(&ctx, &p("c.txt")).unwrap());
        append(&ctx, &p("new.log"), "first").unwrap();
        assert_eq!(read(&ctx, &p("new.log")).unwrap(), "first\n");
        let lines = trace.lines();
        assert_eq!(lines.len(), 14);
        assert!(lines[0].starts_with(r#"{"kind":"file","verb":"write","path":""#), "{}", lines[0]);
        assert!(lines[5].contains(r#""verb":"rename""#) && lines[5].contains(r#""to":""#), "{}", lines[5]);
        assert!(lines[1].contains(r#""verb":"append""#) && !lines[1].contains("\"to\""));
    }

    #[test]
    fn failures_are_plain_and_traced() {
        let (ctx, trace) = ctx();
        let e = read(&ctx, "does/not/exist.txt").unwrap_err().to_string();
        assert_eq!(e, "cannot read \"does/not/exist.txt\": it does not exist");
        assert_eq!(trace.lines().len(), 1);
        assert!(list(&ctx, "nope").is_err() && rename(&ctx, "nope", "x").is_err() && delete(&ctx, "nope").is_err());
    }

    #[test]
    fn delete_removes_empty_folders_only() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("f"), "x").unwrap();
        let (ctx, _) = ctx();
        assert!(delete(&ctx, &sub.to_string_lossy()).is_err());
        std::fs::remove_file(sub.join("f")).unwrap();
        delete(&ctx, &sub.to_string_lossy()).unwrap();
        assert!(!sub.exists());
    }
}
