//! Loading a project: `agentmark.toml`, `tools.toml`, the import graph (files, globs, folders,
//! crates), cycle detection.
//!
//! Two shapes are supported (DESIGN 3, SPEC 18.1):
//! - a **single file** (`examples/02-lead-qualifier.agent.md`): the root is the file's directory,
//!   `tools.toml` sits next to the file, and the project is the file plus whatever it imports;
//! - a **project folder** (`examples/01-invoice-chaser/`): every `*.agent.md` below the folder is
//!   part of the project.
//!
//! Loading never fails as a whole: problems (unreadable file, syntax error, bad import, cycle,
//! bad manifest) become [`Diagnostic`]s in [`Project::load_diagnostics`], and `sema::check`
//! returns them first.
//!
//! Display paths are what the author typed (`tests/errors/x.agent.md`); imported files get the
//! importing file's directory joined with the import path, lexically normalized.

use std::collections::BTreeMap;
use std::path::Path;

use crate::diag::{did_you_mean, Diagnostic, Span};
use crate::manifest::Manifest;
use crate::syntax::ast::{File, ImportKind};
use crate::syntax::parser::parse_file;

pub const E_READ: &str = "E110";
pub const E_IMPORT_MISSING: &str = "E111";
pub const E_IMPORT_CYCLE: &str = "E112";
pub const E_GLOB_EMPTY: &str = "E113";
pub const E_CONFIG: &str = "E114";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    SingleFile,
    Folder,
}

/// `agentmark.toml` (SPEC 18.1). Everything is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub name: Option<String>,
    pub version: Option<String>,
    pub targets: Vec<String>,
    /// `[lint] deny = [...]`: lint rules promoted to errors (read by `lint`).
    pub lint_deny: Vec<String>,
}

/// One import line, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedImport {
    /// File, glob or folder import: the files it brings in, each with the module prefix its names
    /// get (`Some("billing")` -> `billing.overdue`).
    Files(Vec<ImportedFile>),
    /// `crate x`: the binding file for it, if one exists (SPEC 15.1).
    Crate {
        name: String,
        binding: Option<usize>,
    },
    /// The import could not be resolved; a diagnostic was already recorded.
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedFile {
    /// Index into [`Project::files`].
    pub file: usize,
    pub prefix: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Display path, also the key into [`Project::sources`].
    pub path: String,
    pub text: String,
    pub ast: File,
    /// Parallel to `ast.imports`.
    pub imports: Vec<ResolvedImport>,
    /// Directory of the file relative to the project root (`records`, `rules`, ``).
    pub rel_dir: String,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub kind: ProjectKind,
    /// Project root (display form).
    pub root: String,
    pub config: Config,
    pub manifest: Manifest,
    /// All files that parsed, in load order. Indices are stable ids (`FileId`).
    pub files: Vec<SourceFile>,
    /// Files that are roots of the check: the single file, or every file of the folder.
    pub entry_files: Vec<usize>,
    /// Text of every file read (also files that failed to parse, `tools.toml`, `agentmark.toml`),
    /// keyed by display path, so diagnostics can be rendered.
    pub sources: BTreeMap<String, String>,
    /// Problems found while loading. Non-empty means `check` stops here.
    pub load_diagnostics: Vec<Diagnostic>,
}

// ---------------------------------------------------------------------------------- file system

/// The little bit of file system the loader needs, so tests can run on in-memory projects.
pub trait Fs {
    fn read(&self, path: &str) -> Option<String>;
    fn is_dir(&self, path: &str) -> bool;
    /// Names (not paths) of the entries of `dir`, sorted.
    fn list(&self, dir: &str) -> Vec<String>;
    fn exists(&self, path: &str) -> bool {
        self.read(path).is_some() || self.is_dir(path)
    }
}

pub struct RealFs;

/// `normalize(".")` is the empty string (the project root); the OS wants `.` for it.
fn real(path: &str) -> &str {
    if path.is_empty() {
        "."
    } else {
        path
    }
}

impl Fs for RealFs {
    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
    fn is_dir(&self, path: &str) -> bool {
        Path::new(real(path)).is_dir()
    }
    fn list(&self, dir: &str) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(real(dir))
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }
}

/// An in-memory file system: `(path, text)` pairs.
pub struct MemFs(pub Vec<(String, String)>);

impl Fs for MemFs {
    fn read(&self, path: &str) -> Option<String> {
        let p = normalize(path);
        self.0
            .iter()
            .find(|(k, _)| normalize(k) == p)
            .map(|(_, v)| v.clone())
    }
    fn is_dir(&self, path: &str) -> bool {
        let p = normalize(path);
        let prefix = if p.is_empty() {
            String::new()
        } else {
            format!("{p}/")
        };
        self.0
            .iter()
            .any(|(k, _)| normalize(k).starts_with(&prefix) && !prefix.is_empty())
    }
    fn list(&self, dir: &str) -> Vec<String> {
        let d = normalize(dir);
        let prefix = if d.is_empty() {
            String::new()
        } else {
            format!("{d}/")
        };
        let mut v: Vec<String> = self
            .0
            .iter()
            .filter_map(|(k, _)| {
                let n = normalize(k);
                let rest = n.strip_prefix(&prefix)?;
                Some(rest.split('/').next().unwrap_or(rest).to_string())
            })
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

// ---------------------------------------------------------------------------------- path helpers

/// Lexically normalize a `/`-separated path: drop `.`, resolve `..`, no trailing slash.
pub fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if matches!(out.last(), Some(l) if *l != "..") {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            p => out.push(p),
        }
    }
    let joined = out.join("/");
    if path.starts_with('/') {
        format!("/{joined}")
    } else {
        joined
    }
}

fn parent(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

fn join(dir: &str, rel: &str) -> String {
    if dir.is_empty() {
        normalize(rel)
    } else {
        normalize(&format!("{dir}/{rel}"))
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `*` matches any run of characters inside one path component.
fn wildcard_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    fn go(p: &[char], n: &[char]) -> bool {
        match p.split_first() {
            None => n.is_empty(),
            Some(('*', rest)) => (0..=n.len()).any(|i| go(rest, &n[i..])),
            Some((c, rest)) => n.first() == Some(c) && go(rest, &n[1..]),
        }
    }
    go(&p, &n)
}

fn is_source_name(name: &str) -> bool {
    name.ends_with(".agent.md")
}

/// Directories the folder walk never enters.
fn skipped_dir(name: &str) -> bool {
    matches!(name, "docs" | "fixtures" | "target" | "node_modules")
        || (name.starts_with('.') && name != ".agentmark")
}

fn walk(fs: &dyn Fs, dir: &str, out: &mut Vec<String>) {
    for name in fs.list(dir) {
        let path = join(dir, &name);
        if fs.is_dir(&path) {
            if !skipped_dir(&name) {
                walk(fs, &path, out);
            }
        } else if is_source_name(&name) {
            out.push(path);
        }
    }
}

// ---------------------------------------------------------------------------------- loading

impl Project {
    /// Load from disk: `path` is a `.agent.md` file or a project folder.
    pub fn load(path: &str) -> Project {
        Project::load_with(&RealFs, path)
    }

    /// A single in-memory file with its manifest text (empty = no `tools.toml`).
    pub fn from_str(path: &str, text: &str, tools_toml: &str) -> Project {
        let mut files = vec![(path.to_string(), text.to_string())];
        if !tools_toml.is_empty() {
            files.push((join(&parent(path), "tools.toml"), tools_toml.to_string()));
        }
        Project::load_with(&MemFs(files), path)
    }

    /// An in-memory project: `files` are `(path, text)`; `root` is a folder (folder project) or
    /// one of the files (single-file project).
    pub fn from_memory(root: &str, files: Vec<(&str, &str)>) -> Project {
        let fs = MemFs(
            files
                .into_iter()
                .map(|(p, t)| (p.to_string(), t.to_string()))
                .collect(),
        );
        Project::load_with(&fs, root)
    }

    pub fn load_with(fs: &dyn Fs, path: &str) -> Project {
        let given = normalize(path);
        let is_dir = fs.is_dir(&given) || (!given.ends_with(".md") && !fs.exists(&given));
        let (kind, root) = if is_dir {
            (ProjectKind::Folder, given.clone())
        } else {
            (ProjectKind::SingleFile, parent(&given))
        };
        let mut b = Loader {
            fs,
            root: root.clone(),
            files: Vec::new(),
            sources: BTreeMap::new(),
            diags: Vec::new(),
            stack: Vec::new(),
            failed: Vec::new(),
        };
        let config = b.config();
        let manifest = b.manifest();
        let mut entry_files = Vec::new();
        if kind == ProjectKind::Folder {
            let mut found = Vec::new();
            walk(fs, &root, &mut found);
            if found.is_empty() && !fs.is_dir(&root) {
                b.diags.push(Diagnostic::error(
                    E_READ,
                    &given,
                    Span::new(1, 1, 1),
                    format!("Cannot find the project \"{given}\"."),
                ));
            }
            for f in found {
                if let Some(i) = b.load_file(&f) {
                    entry_files.push(i);
                }
            }
        } else if let Some(i) = b.load_file(&given) {
            entry_files.push(i);
        }
        entry_files.sort_unstable();
        entry_files.dedup();
        Project {
            kind,
            root,
            config,
            manifest,
            files: b.files,
            entry_files,
            sources: b.sources,
            load_diagnostics: b.diags,
        }
    }

    /// Render diagnostics of any file of this project in the SPEC 10 format, separated by a blank
    /// line.
    pub fn render(&self, diags: &[Diagnostic]) -> String {
        diags
            .iter()
            .map(|d| {
                let src = self.sources.get(&d.file).map(String::as_str).unwrap_or("");
                d.render(src)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Index of the file with this display path.
    pub fn file_index(&self, path: &str) -> Option<usize> {
        let p = normalize(path);
        self.files.iter().position(|f| f.path == p)
    }
}

struct Loader<'a> {
    fs: &'a dyn Fs,
    root: String,
    files: Vec<SourceFile>,
    sources: BTreeMap<String, String>,
    diags: Vec<Diagnostic>,
    /// Files being loaded right now (for cycle detection).
    stack: Vec<usize>,
    /// Display paths of files that could not be parsed (so they are not read twice).
    failed: Vec<String>,
}

const CONFIG_KEYS: [&str; 4] = ["name", "version", "targets", "lint"];

impl Loader<'_> {
    fn config(&mut self) -> Config {
        let path = join(&self.root, "agentmark.toml");
        let Some(text) = self.fs.read(&path) else {
            return Config::default();
        };
        self.sources.insert(path.clone(), text.clone());
        let table: toml::Table = match text.parse() {
            Ok(t) => t,
            Err(e) => {
                let line = e
                    .span()
                    .map(|r| text[..r.start.min(text.len())].matches('\n').count() as u32 + 1)
                    .unwrap_or(1);
                self.diags.push(Diagnostic::error(
                    E_CONFIG,
                    &path,
                    Span::new(line, 1, 1),
                    format!("This is not valid TOML: {}.", e.message()),
                ));
                return Config::default();
            }
        };
        let mut cfg = Config::default();
        let line_of = |key: &str| -> u32 {
            text.lines()
                .position(|l| l.trim_start().starts_with(key))
                .map(|i| i as u32 + 1)
                .unwrap_or(1)
        };
        for (key, value) in &table {
            let line = line_of(key);
            let span = Span::new(line, 1, key.chars().count() as u32);
            match key.as_str() {
                "name" | "version" => match value.as_str() {
                    Some(s) if key == "name" => cfg.name = Some(s.to_string()),
                    Some(s) => cfg.version = Some(s.to_string()),
                    None => self.diags.push(Diagnostic::error(
                        E_CONFIG,
                        &path,
                        span,
                        format!("\"{key}\" in agentmark.toml must be text."),
                    )),
                },
                "targets" => match string_list(value) {
                    Some(v) => cfg.targets = v,
                    None => self.diags.push(Diagnostic::error(
                        E_CONFIG,
                        &path,
                        span,
                        "\"targets\" in agentmark.toml must be a list of words, like [\"rust\"].",
                    )),
                },
                "lint" => {
                    let deny = value.get("deny").and_then(string_list);
                    match deny {
                        Some(v) => cfg.lint_deny = v,
                        None if value.get("deny").is_none() => {}
                        None => self.diags.push(Diagnostic::error(
                            E_CONFIG,
                            &path,
                            Span::new(line_of("deny"), 1, 4),
                            "\"deny\" under [lint] must be a list of rule names.",
                        )),
                    }
                }
                other => self.diags.push(
                    Diagnostic::error(
                        E_CONFIG,
                        &path,
                        span,
                        format!("Unknown key \"{other}\" in agentmark.toml."),
                    )
                    .suggesting(other, CONFIG_KEYS)
                    .with_help(format!(
                        "agentmark.toml accepts: {}.",
                        CONFIG_KEYS.join(", ")
                    )),
                ),
            }
        }
        cfg
    }

    fn manifest(&mut self) -> Manifest {
        let path = join(&self.root, "tools.toml");
        let Some(text) = self.fs.read(&path) else {
            return Manifest::empty();
        };
        self.sources.insert(path.clone(), text.clone());
        match Manifest::parse(&path, &text) {
            Ok(m) => m,
            Err(d) => {
                self.diags.extend(d);
                Manifest::empty()
            }
        }
    }

    fn rel_dir(&self, path: &str) -> String {
        let dir = parent(path);
        if self.root.is_empty() {
            dir
        } else {
            dir.strip_prefix(&self.root)
                .map(|r| r.trim_start_matches('/').to_string())
                .unwrap_or(dir)
        }
    }

    /// Load (parse + resolve imports of) the file at display path `path`. Returns its index, or
    /// `None` if it could not be read or parsed (a diagnostic is recorded).
    fn load_file(&mut self, path: &str) -> Option<usize> {
        if let Some(i) = self.files.iter().position(|f| f.path == path) {
            return Some(i);
        }
        if self.failed.iter().any(|p| p == path) {
            return None;
        }
        let Some(text) = self.fs.read(path) else {
            self.failed.push(path.to_string());
            self.diags.push(Diagnostic::error(
                E_READ,
                path,
                Span::new(1, 1, 1),
                format!("Cannot read the file \"{path}\"."),
            ));
            return None;
        };
        self.sources.insert(path.to_string(), text.clone());
        let ast = match parse_file(path, &text) {
            Ok(a) => a,
            Err(d) => {
                self.failed.push(path.to_string());
                self.diags.extend(d);
                return None;
            }
        };
        let idx = self.files.len();
        let n_imports = ast.imports.len();
        self.files.push(SourceFile {
            path: path.to_string(),
            text,
            ast,
            imports: vec![ResolvedImport::Unresolved; n_imports],
            rel_dir: self.rel_dir(path),
        });
        self.stack.push(idx);
        for i in 0..n_imports {
            let resolved = self.resolve_import(idx, i);
            self.files[idx].imports[i] = resolved;
        }
        self.stack.pop();
        Some(idx)
    }

    fn import_error(
        &mut self,
        file: usize,
        import: usize,
        code: &'static str,
        msg: String,
    ) -> Diagnostic {
        let imp = &self.files[file].ast.imports[import];
        Diagnostic::error(code, &self.files[file].path, imp.span, msg)
    }

    /// Load `target` as an import of `file`, reporting a cycle if it is already being loaded.
    fn follow(&mut self, file: usize, import: usize, target: &str) -> Option<usize> {
        if let Some(pos) = self.files.iter().position(|f| f.path == target) {
            if self.stack.contains(&pos) {
                let start = self.stack.iter().position(|s| *s == pos).unwrap_or(0);
                let mut chain: Vec<String> = self.stack[start..]
                    .iter()
                    .map(|i| self.files[*i].path.clone())
                    .collect();
                chain.push(self.files[pos].path.clone());
                let d = self
                    .import_error(
                        file,
                        import,
                        E_IMPORT_CYCLE,
                        format!("Import cycle: {}.", chain.join(" -> ")),
                    )
                    .with_help("Imports must form a tree: move what both files need into a third file that imports neither.");
                self.diags.push(d);
                return None;
            }
            return Some(pos);
        }
        self.load_file(target)
    }

    fn resolve_import(&mut self, file: usize, import: usize) -> ResolvedImport {
        let dir = parent(&self.files[file].path);
        let kind = self.files[file].ast.imports[import].kind.clone();
        match kind {
            ImportKind::File { path, alias } => {
                let target = join(&dir, &path);
                if self.fs.read(&target).is_none() {
                    let names: Vec<String> = self
                        .fs
                        .list(&parent(&target))
                        .into_iter()
                        .filter(|n| is_source_name(n))
                        .collect();
                    let hit = did_you_mean(file_name(&target), names.iter().map(String::as_str));
                    let mut d = self.import_error(
                        file,
                        import,
                        E_IMPORT_MISSING,
                        format!("Cannot find the file \"{path}\"."),
                    );
                    if let Some(h) = hit {
                        let last = file_name(&path);
                        let stem = &path[..path.len() - last.len()];
                        d = d.with_did_you_mean(Some(format!("{stem}{h}")));
                    }
                    self.diags.push(d);
                    return ResolvedImport::Unresolved;
                }
                match self.follow(file, import, &target) {
                    Some(i) => ResolvedImport::Files(vec![ImportedFile {
                        file: i,
                        prefix: alias.map(|a| a.name),
                    }]),
                    None => ResolvedImport::Unresolved,
                }
            }
            ImportKind::Glob { pattern } => {
                let full = join(&dir, &pattern);
                let (gdir, gpat) = (parent(&full), file_name(&full).to_string());
                let own = self.files[file].path.clone();
                let matches: Vec<String> = self
                    .fs
                    .list(&gdir)
                    .into_iter()
                    .filter(|n| wildcard_match(&gpat, n) && is_source_name(n))
                    .map(|n| join(&gdir, &n))
                    .filter(|p| *p != own && !self.fs.is_dir(p))
                    .collect();
                if matches.is_empty() {
                    let d = self.import_error(
                        file,
                        import,
                        E_GLOB_EMPTY,
                        format!("The pattern \"{pattern}\" matches no files."),
                    );
                    self.diags.push(d);
                    return ResolvedImport::Unresolved;
                }
                let mut out = Vec::new();
                for m in matches {
                    if let Some(i) = self.follow(file, import, &m) {
                        out.push(ImportedFile {
                            file: i,
                            prefix: None,
                        });
                    }
                }
                ResolvedImport::Files(out)
            }
            ImportKind::Folder { path, alias } => {
                let target = join(&dir, &path);
                if !self.fs.is_dir(&target) {
                    let d = self.import_error(
                        file,
                        import,
                        E_IMPORT_MISSING,
                        format!("Cannot find the folder \"{path}\"."),
                    );
                    self.diags.push(d);
                    return ResolvedImport::Unresolved;
                }
                let mut found = Vec::new();
                walk(self.fs, &target, &mut found);
                let mut out = Vec::new();
                for f in found {
                    // `billing/refunds/rules.agent.md` -> prefix `billing.refunds`
                    let rel = parent(&f);
                    let rel = rel
                        .strip_prefix(&target)
                        .unwrap_or(&rel)
                        .trim_start_matches('/');
                    let mut prefix = alias.name.clone();
                    for part in rel.split('/').filter(|p| !p.is_empty()) {
                        prefix.push('.');
                        prefix.push_str(part);
                    }
                    if let Some(i) = self.follow(file, import, &f) {
                        out.push(ImportedFile {
                            file: i,
                            prefix: Some(prefix),
                        });
                    }
                }
                ResolvedImport::Files(out)
            }
            ImportKind::Crate { name, .. } => {
                let crate_name = name.name.clone();
                let underscored = crate_name.replace('-', "_");
                let mut binding = None;
                for dir in ["bindings", ".agentmark/bindings"] {
                    for n in [&crate_name, &underscored] {
                        let p = join(&self.root, &format!("{dir}/{n}.agent.md"));
                        if self.fs.read(&p).is_some() {
                            binding = self.follow(file, import, &p);
                            break;
                        }
                    }
                    if binding.is_some() {
                        break;
                    }
                }
                ResolvedImport::Crate {
                    name: crate_name,
                    binding,
                }
            }
        }
    }
}

fn string_list(v: &toml::Value) -> Option<Vec<String>> {
    v.as_array()?
        .iter()
        .map(|x| x.as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(name: &str) -> String {
        format!("# Record: {name}\n- id: text\n")
    }

    #[test]
    fn normalize_resolves_dots() {
        assert_eq!(normalize("a/b/../c/./d"), "a/c/d");
        assert_eq!(normalize("./x.md"), "x.md");
        assert_eq!(normalize("../x.md"), "../x.md");
        assert_eq!(normalize("/a/../b"), "/b");
    }

    #[test]
    fn wildcards_match_inside_one_component() {
        assert!(wildcard_match("*.agent.md", "overdue.agent.md"));
        assert!(!wildcard_match("*.agent.md", "overdue.md"));
        assert!(wildcard_match("a*c", "abbbc"));
    }

    #[test]
    fn single_file_reads_tools_next_to_it() {
        let p = Project::load("examples/02-lead-qualifier.agent.md");
        assert_eq!(p.kind, ProjectKind::SingleFile);
        assert!(p.load_diagnostics.is_empty(), "{:?}", p.load_diagnostics);
        assert_eq!(p.files.len(), 1);
        assert!(p.manifest.tool("crm").is_some());
        assert_eq!(p.root, "examples");
    }

    #[test]
    fn folder_project_loads_every_file_and_the_config() {
        let p = Project::load("examples/01-invoice-chaser");
        assert_eq!(p.kind, ProjectKind::Folder);
        assert!(p.load_diagnostics.is_empty(), "{:?}", p.load_diagnostics);
        assert_eq!(p.files.len(), 5);
        assert_eq!(p.config.name.as_deref(), Some("invoice-chaser"));
        assert_eq!(p.config.targets, ["rust"]);
        assert_eq!(p.config.lint_deny, ["prompt-in-loop", "missing-doc"]);
        assert!(p.manifest.tool("email").is_some());
        let agent = p
            .file_index("examples/01-invoice-chaser/agents/invoice-chaser.agent.md")
            .unwrap();
        // `../records/invoice.agent.md` is normalized to the same file the folder walk found.
        assert!(matches!(&p.files[agent].imports[0], ResolvedImport::Files(v) if v.len() == 1));
        assert_eq!(p.files[agent].rel_dir, "agents");
    }

    #[test]
    fn single_file_follows_its_imports_only() {
        let p = Project::from_memory(
            "app/main.agent.md",
            vec![
                (
                    "app/main.agent.md",
                    "# Import: ./a.agent.md\n\n# Record: M\n- id: text\n",
                ),
                ("app/a.agent.md", &rec("A")),
                ("app/unused.agent.md", &rec("U")),
            ],
        );
        assert!(p.load_diagnostics.is_empty());
        assert_eq!(p.files.len(), 2);
    }

    #[test]
    fn glob_imports_every_match_but_not_itself() {
        let p = Project::from_memory(
            "app",
            vec![
                (
                    "app/main.agent.md",
                    "# Import: ./rules/*.agent.md\n\n# Record: M\n- id: text\n",
                ),
                ("app/rules/a.agent.md", &rec("A")),
                ("app/rules/b.agent.md", &rec("B")),
            ],
        );
        assert!(p.load_diagnostics.is_empty(), "{:?}", p.load_diagnostics);
        let main = p.file_index("app/main.agent.md").unwrap();
        match &p.files[main].imports[0] {
            ResolvedImport::Files(v) => assert_eq!(v.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn empty_glob_is_an_error() {
        let p = Project::from_memory(
            "app/main.agent.md",
            vec![(
                "app/main.agent.md",
                "# Import: ./rules/*.agent.md\n\n# Record: M\n- id: text\n",
            )],
        );
        let out = p.render(&p.load_diagnostics);
        assert!(
            out.contains("The pattern \"./rules/*.agent.md\" matches no files."),
            "{out}"
        );
    }

    #[test]
    fn folder_import_prefixes_names_with_the_alias() {
        let p = Project::from_memory(
            "app/main.agent.md",
            vec![
                (
                    "app/main.agent.md",
                    "# Import: ./billing as billing\n\n# Record: M\n- id: text\n",
                ),
                ("app/billing/rules.agent.md", &rec("A")),
                ("app/billing/sub/more.agent.md", &rec("B")),
            ],
        );
        assert!(p.load_diagnostics.is_empty(), "{:?}", p.load_diagnostics);
        match &p.files[0].imports[0] {
            ResolvedImport::Files(v) => {
                let prefixes: Vec<_> = v.iter().map(|f| f.prefix.clone().unwrap()).collect();
                assert_eq!(prefixes, ["billing", "billing.sub"]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn missing_import_suggests_the_close_file() {
        let p = Project::from_memory(
            "app/main.agent.md",
            vec![
                (
                    "app/main.agent.md",
                    "# Import: ./recrods.agent.md\n\n# Record: M\n- id: text\n",
                ),
                ("app/records.agent.md", &rec("A")),
            ],
        );
        let out = p.render(&p.load_diagnostics);
        assert_eq!(
            out,
            "app/main.agent.md:1\n  # Import: ./recrods.agent.md\n    ^^^^^^^^^^^^^^^^^^^^^^^^^^\n  Cannot find the file \"./recrods.agent.md\". Did you mean \"./records.agent.md\"?"
        );
    }

    #[test]
    fn import_cycle_prints_the_cycle() {
        let p = Project::from_memory(
            "app/a.agent.md",
            vec![
                (
                    "app/a.agent.md",
                    "# Import: ./b.agent.md\n\n# Record: A\n- id: text\n",
                ),
                (
                    "app/b.agent.md",
                    "# Import: ./c.agent.md\n\n# Record: B\n- id: text\n",
                ),
                (
                    "app/c.agent.md",
                    "# Import: ./a.agent.md\n\n# Record: C\n- id: text\n",
                ),
            ],
        );
        let out = p.render(&p.load_diagnostics);
        assert!(
            out.starts_with("app/c.agent.md:1\n  # Import: ./a.agent.md\n"),
            "{out}"
        );
        assert!(
            out.contains("Import cycle: app/a.agent.md -> app/b.agent.md -> app/c.agent.md -> app/a.agent.md."),
            "{out}"
        );
        assert_eq!(p.load_diagnostics.len(), 1);
    }

    #[test]
    fn a_file_importing_itself_is_a_cycle() {
        let p = Project::from_memory(
            "app/a.agent.md",
            vec![(
                "app/a.agent.md",
                "# Import: ./a.agent.md\n\n# Record: A\n- id: text\n",
            )],
        );
        let out = p.render(&p.load_diagnostics);
        assert!(
            out.contains("Import cycle: app/a.agent.md -> app/a.agent.md."),
            "{out}"
        );
    }

    #[test]
    fn syntax_errors_in_an_imported_file_are_reported_with_that_file() {
        let p = Project::from_memory(
            "app/a.agent.md",
            vec![
                (
                    "app/a.agent.md",
                    "# Import: ./b.agent.md\n\n# Record: A\n- id: text\n",
                ),
                ("app/b.agent.md", "# Record: B\n\t- id: text\n"),
            ],
        );
        let out = p.render(&p.load_diagnostics);
        assert!(out.starts_with("app/b.agent.md:2"), "{out}");
        assert!(out.contains("Tabs are not allowed"), "{out}");
    }

    #[test]
    fn crate_import_finds_a_binding_file() {
        let p = Project::from_memory(
            "app/main.agent.md",
            vec![
                (
                    "app/main.agent.md",
                    "# Import: crate printpdf\n\n# Record: M\n- id: text\n",
                ),
                (
                    "app/bindings/printpdf.agent.md",
                    "# Binding: pdf\n- crate: printpdf\n- effect: local\n",
                ),
            ],
        );
        match &p.files[0].imports[0] {
            ResolvedImport::Crate { name, binding } => {
                assert_eq!(name, "printpdf");
                assert!(binding.is_some());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn crate_import_without_binding_is_known_but_unbound() {
        let p = Project::from_str(
            "a.agent.md",
            "# Import: crate sha2\n\n# Record: M\n- id: text\n",
            "",
        );
        assert!(p.load_diagnostics.is_empty());
        assert!(matches!(
            &p.files[0].imports[0],
            ResolvedImport::Crate { binding: None, .. }
        ));
    }

    #[test]
    fn bad_agentmark_toml_is_reported() {
        let p = Project::from_memory(
            "app",
            vec![
                ("app/agentmark.toml", "nmae = \"x\"\n"),
                ("app/main.agent.md", &rec("A")),
            ],
        );
        let out = p.render(&p.load_diagnostics);
        assert!(
            out.contains("Unknown key \"nmae\" in agentmark.toml. Did you mean \"name\"?"),
            "{out}"
        );
    }

    #[test]
    fn bad_tools_toml_is_reported() {
        let p = Project::from_str("a.agent.md", &rec("A"), "[email]\nverbs = []\n");
        let out = p.render(&p.load_diagnostics);
        assert!(out.starts_with("tools.toml:1"), "{out}");
    }

    #[test]
    fn missing_single_file_is_reported() {
        let p = Project::load("does/not/exist.agent.md");
        let out = p.render(&p.load_diagnostics);
        assert!(
            out.contains("Cannot read the file \"does/not/exist.agent.md\"."),
            "{out}"
        );
    }
}
