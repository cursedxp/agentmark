//! Open buffers, project roots and one analysis (load + check + lint) per project.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use agentmark::diag::{Diagnostic, Severity};
use agentmark::lint::{lint_with_sources, LintConfig};
use agentmark::project::{normalize, Fs, Project, ProjectKind, RealFs};
use agentmark::sema::{self, Checked};

/// One open buffer.
#[derive(Debug, Clone)]
pub struct Doc {
    pub text: String,
    pub version: i32,
}

/// Open buffers by normalized path. They override what is on disk (Q251).
pub type Docs = HashMap<String, Doc>;

/// The real file system with the open buffers laid over it.
pub struct OverlayFs<'a> {
    pub docs: &'a Docs,
    real: RealFs,
}

impl<'a> OverlayFs<'a> {
    pub fn new(docs: &'a Docs) -> Self {
        OverlayFs { docs, real: RealFs }
    }
}

impl Fs for OverlayFs<'_> {
    fn read(&self, path: &str) -> Option<String> {
        match self.docs.get(&normalize(path)) {
            Some(d) => Some(d.text.clone()),
            None => self.real.read(path),
        }
    }
    fn is_dir(&self, path: &str) -> bool {
        let p = normalize(path);
        let prefix = format!("{p}/");
        self.real.is_dir(path) || (!p.is_empty() && self.docs.keys().any(|k| k.starts_with(&prefix)))
    }
    fn list(&self, dir: &str) -> Vec<String> {
        let d = normalize(dir);
        let prefix = if d.is_empty() { String::new() } else { format!("{d}/") };
        let mut v = self.real.list(dir);
        for k in self.docs.keys() {
            if let Some(rest) = k.strip_prefix(&prefix) {
                v.push(rest.split('/').next().unwrap_or(rest).to_string());
            }
        }
        v.sort();
        v.dedup();
        v
    }
}

/// Where the project of a document starts: the nearest folder above it with an `agentmark.toml`,
/// otherwise the document alone (a single-file project).
pub fn project_target(fs: &dyn Fs, doc_path: &str) -> String {
    let mut dir = doc_path.rsplit_once('/').map(|(d, _)| d.to_string());
    while let Some(d) = dir {
        let toml = if d.is_empty() { "/agentmark.toml".to_string() } else { format!("{d}/agentmark.toml") };
        if fs.read(&toml).is_some() {
            return if d.is_empty() { "/".into() } else { d };
        }
        dir = d.rsplit_once('/').map(|(p, _)| p.to_string());
    }
    doc_path.to_string()
}

/// The last successful check of a project and the text of every file it was made from. When the
/// buffer is broken (so `check` has no result) features keep answering from it, for every file
/// whose text has not changed since (Q252).
#[derive(Debug)]
pub struct Good {
    pub checked: Checked,
    pub texts: BTreeMap<String, String>,
}

/// Which line of an older text is which line of a newer one (all 0-based): a line diff, so lines
/// that were not touched map one to one even with several edits in between (Q252).
#[derive(Debug, Clone)]
pub struct LineMap {
    to_old: Vec<Option<usize>>,
    to_new: Vec<Option<usize>>,
}

impl LineMap {
    pub fn between(old: &str, new: &str) -> LineMap {
        let o: Vec<&str> = old.split('\n').collect();
        let n: Vec<&str> = new.split('\n').collect();
        let mut to_old = vec![None; n.len()];
        let mut to_new = vec![None; o.len()];
        let mut link = |i: usize, j: usize| {
            to_old[j] = Some(i);
            to_new[i] = Some(j);
        };
        let prefix = o.iter().zip(&n).take_while(|(a, b)| a == b).count();
        let room = o.len().min(n.len()) - prefix;
        let suffix = o.iter().rev().zip(n.iter().rev()).take(room).take_while(|(a, b)| a == b).count();
        for k in 0..prefix {
            link(k, k);
        }
        for k in 0..suffix {
            link(o.len() - suffix + k, n.len() - suffix + k);
        }
        // longest common subsequence of what is left in the middle (files are small; a huge
        // middle keeps only prefix and suffix)
        let (om, nm) = (&o[prefix..o.len() - suffix], &n[prefix..n.len() - suffix]);
        if om.len().saturating_mul(nm.len()) <= 4_000_000 {
            let w = nm.len() + 1;
            let mut dp = vec![0u32; (om.len() + 1) * w];
            for i in (0..om.len()).rev() {
                for j in (0..nm.len()).rev() {
                    dp[i * w + j] = if om[i] == nm[j] { dp[(i + 1) * w + j + 1] + 1 } else { dp[(i + 1) * w + j].max(dp[i * w + j + 1]) };
                }
            }
            let (mut i, mut j) = (0, 0);
            while i < om.len() && j < nm.len() {
                if om[i] == nm[j] {
                    link(prefix + i, prefix + j);
                    i += 1;
                    j += 1;
                } else if dp[(i + 1) * w + j] >= dp[i * w + j + 1] {
                    i += 1;
                } else {
                    j += 1;
                }
            }
        }
        LineMap { to_old, to_new }
    }

    pub fn new_to_old(&self, line: usize) -> Option<usize> {
        self.to_old.get(line).copied().flatten()
    }

    pub fn old_to_new(&self, line: usize) -> Option<usize> {
        self.to_new.get(line).copied().flatten()
    }

    /// The old line for a new one, or the nearest mapped line above it (for "which block am I in").
    pub fn new_to_old_near(&self, line: usize) -> usize {
        (0..=line).rev().find_map(|l| self.new_to_old(l)).unwrap_or(0)
    }
}

/// Everything the features need about one project at one moment.
pub struct Analysis {
    /// The key of the project (its folder, or the document for a single-file project).
    pub root: String,
    pub project: Project,
    /// Errors and warnings of `sema::check` (or of loading).
    pub compiler: Vec<Diagnostic>,
    pub lint: Vec<Diagnostic>,
    /// The result of the check just made, or the last good one.
    pub good: Option<Rc<Good>>,
    /// `good` is from this very check.
    pub fresh: bool,
    /// Line maps by path, made on first use.
    maps: RefCell<HashMap<String, Rc<LineMap>>>,
}

impl Analysis {
    /// Check the project `target` (a folder or a file, see [`project_target`]) that `doc_path`
    /// belongs to, with the open buffers laid over the disk.
    pub fn run(docs: &Docs, doc_path: &str, target: &str, previous: Option<Rc<Good>>) -> Analysis {
        let fs = OverlayFs::new(docs);
        let mut project = Project::load_with(&fs, target);
        let mut root = target.to_string();
        // A file inside a skipped folder (`fixtures/`, ...) is not part of the folder project:
        // it is checked on its own.
        if project.kind == ProjectKind::Folder
            && doc_path.ends_with(".agent.md")
            && !project.sources.contains_key(doc_path)
        {
            project = Project::load_with(&fs, doc_path);
            root = doc_path.to_string();
        }
        // a file a diagnostic points at but the loader did not read (config in an overlay, ...)
        let missing: Vec<String> = project.load_diagnostics.iter().map(|d| d.file.clone()).filter(|f| !project.sources.contains_key(f)).collect();
        for f in missing {
            if let Some(t) = fs.read(&f) {
                project.sources.insert(f, t);
            }
        }
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sema::check(&project)));
        let (compiler, good, fresh) = match checked {
            Ok(Ok(c)) => {
                let warnings = c.warnings.clone();
                let texts = project_texts(&project);
                (warnings, Some(Rc::new(Good { checked: c, texts })), true)
            }
            Ok(Err(d)) => (d, previous, false),
            Err(_) => {
                eprintln!("agentmark-lsp: the checker panicked on {target}; keeping the last good result");
                (Vec::new(), previous, false)
            }
        };
        let cfg = LintConfig::new(project.config.lint_deny.iter().cloned());
        let files: Vec<_> = project
            .files
            .iter()
            .filter(|f| !in_bindings(&f.path))
            .map(|f| (&f.ast, f.text.as_str()))
            .collect();
        let lint = lint_with_sources(&files, &cfg);
        Analysis { root, project, compiler, lint, good, fresh, maps: RefCell::default() }
    }

    /// The facts of the last good check for `path`, with the map from the current lines of that
    /// file to the lines the facts talk about. The file may have changed since: lines that did not
    /// are mapped, the others are not (Q252).
    pub fn facts(&self, path: &str) -> Option<(&Checked, Rc<LineMap>)> {
        let good = self.good.as_ref()?;
        let now = self.project.sources.get(path)?;
        if let Some(m) = self.maps.borrow().get(path) {
            return Some((&good.checked, m.clone()));
        }
        let old = if self.fresh { now } else { good.texts.get(path)? };
        let map = Rc::new(LineMap::between(old, now));
        self.maps.borrow_mut().insert(path.to_string(), map.clone());
        Some((&good.checked, map))
    }

    /// A span of the facts, in the lines of the current text of `path`; `None` when that line was
    /// changed.
    pub fn current_span(&self, path: &str, span: agentmark::diag::Span) -> Option<agentmark::diag::Span> {
        let (_, map) = self.facts(path)?;
        let line = map.old_to_new((span.line as usize).checked_sub(1)?)?;
        Some(agentmark::diag::Span { line: line as u32 + 1, ..span })
    }

    /// 1-based line of the facts that is nearest to the 0-based `line` of the current text.
    pub fn facts_line(&self, path: &str, line: usize) -> u32 {
        match self.facts(path) {
            Some((_, map)) => map.new_to_old_near(line) as u32 + 1,
            None => line as u32 + 1,
        }
    }

    /// Like [`Analysis::facts`], but any file will do (completion lists, where an older answer is
    /// better than none).
    pub fn any_facts(&self) -> Option<&Checked> {
        self.good.as_ref().map(|g| &g.checked)
    }

    /// The text of `path` as this analysis saw it.
    pub fn text(&self, path: &str) -> Option<&str> {
        self.project.sources.get(path).map(String::as_str)
    }

    pub fn file_id(&self, path: &str) -> Option<usize> {
        self.project.file_index(path)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.compiler.iter().chain(self.lint.iter()).filter(|d| d.severity == Severity::Error)
    }

    pub fn model_actions_enabled(&self) -> bool {
        self.project.config.lsp_model_actions
    }
}

fn project_texts(p: &Project) -> BTreeMap<String, String> {
    p.sources.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Generated bindings are not linted or formatted (they are rewritten by `agentmark bind`, Q111).
pub fn in_bindings(path: &str) -> bool {
    path.split('/').any(|c| c == "bindings")
}
