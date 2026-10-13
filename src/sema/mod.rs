//! Semantic check (DESIGN 3): names, types, effects, tool manifest, hard limits.
//!
//! Entry point: [`check`]. Interface for later passes: see `src/sema/README.md`.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::diag::{Diagnostic, Severity, Span};
use crate::manifest::Manifest;
use crate::project::Project;
use crate::syntax::ast::{Decl, Effect, Expr};

mod body;
mod decls;
mod expr;
mod limits;
mod parallel;
mod scope;
mod stmt;
mod tools;
mod types;
mod unused;

pub mod effects;

pub use effects::EffectSet;
pub use scope::Kind;
pub use types::Type;

use body::{BodyKey, Implicit, Memo};
use scope::{Key, Registry, Scope};

/// Identifies one AST node: the file (index into `Project::files`) and the node's span. Every
/// expression and statement has a distinct span within its file, so this is a stable key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct NodeKey {
    pub file: usize,
    pub span: Span,
}

impl NodeKey {
    pub fn new(file: usize, span: Span) -> Self {
        NodeKey { file, span }
    }
}

/// A record field with its resolved type.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldInfo {
    pub name: String,
    pub ty: Type,
    /// The type was not written and came from the name (SPEC 2.1).
    pub inferred: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecordInfo {
    pub name: String,
    pub file: usize,
    pub span: Span,
    pub fields: Vec<FieldInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParamInfo {
    pub name: String,
    pub ty: Type,
    pub has_default: bool,
}

/// Signature of a function, Rust block, binding item or command.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sig {
    pub params: Vec<ParamInfo>,
    pub out: Option<Type>,
    /// The effect the author declared.
    pub effect: Effect,
}

/// What kind of code a [`BlockInfo`] describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BlockKind {
    /// `# Function`
    Function,
    /// `# Rule` (the `when:` expression)
    Rule,
    /// An agent block with a trigger: `## Every …`, `## When …`, `## On start`.
    Entry,
    /// `## Do <name>`
    Action,
    /// `## GET /path` of a service
    Route,
    /// `## Run` of a command
    Command,
}

/// The effect of one block, function, rule or entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlockInfo {
    pub kind: BlockKind,
    pub file: usize,
    /// Span of the block's heading (or of the declaration heading for functions/rules/commands).
    pub span: Span,
    /// Name of the unit it belongs to: agent, service, command, function or rule.
    pub unit: String,
    /// Heading text (`Every day at 09:00`), `GET /invoices/{id}`, `Run`, or the function name.
    pub label: String,
    pub effect: EffectSet,
    /// Declared effect, for functions and rules.
    pub declared: Option<Effect>,
    /// The record value the block works on, as `(variable, record)`: the record of a rule, the
    /// event of `## When tool receives event`, or the loop record a `## Do` block is called with.
    pub param: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConstInfo {
    pub name: String,
    pub file: usize,
    pub ty: Type,
    pub value: Expr,
}

/// A `## Rust` block: trusted code whose effect the author declared (SPEC 17).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RustInfo {
    pub name: String,
    pub file: usize,
    pub span: Span,
    /// Declaration it was written under, if any.
    pub parent: Option<String>,
    pub sig: Sig,
}

/// One bound function of a `# Binding`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BindingItemInfo {
    pub name: String,
    pub sig: Sig,
}

/// A `# Binding` (SPEC 15.4).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BindingInfo {
    pub name: String,
    pub file: usize,
    pub span: Span,
    pub crate_name: String,
    /// `Unverified` until a human approves (SPEC 15.3).
    pub effect: Effect,
    pub proposed_effect: Option<Effect>,
    pub items: Vec<BindingItemInfo>,
}

impl BindingInfo {
    pub fn is_unverified(&self) -> bool {
        self.effect == Effect::Unverified
    }
}

/// Result of a successful check.
#[derive(Debug, Clone, Serialize)]
pub struct Checked {
    /// Display paths of the project's files; `NodeKey::file` and `BlockInfo::file` index into it.
    pub files: Vec<String>,
    /// Warnings (inferred field types, unused imports and declarations, unbound crates, …).
    pub warnings: Vec<Diagnostic>,
    /// Every record by name, fields in declaration order.
    pub records: BTreeMap<String, RecordInfo>,
    /// Type of every expression (including those inside interpolated strings).
    pub expr_types: HashMap<NodeKey, Type>,
    /// Effect of every statement, nested statements included in their parent's set.
    pub stmt_effects: HashMap<NodeKey, EffectSet>,
    /// Effect of every function, rule, entry, action, route and command, in source order.
    pub blocks: Vec<BlockInfo>,
    /// The resolved tool manifest.
    pub manifest: Manifest,
    /// Constants by name, with their types (they are inlined later).
    pub constants: BTreeMap<String, ConstInfo>,
    /// Functions by name.
    pub functions: BTreeMap<String, Sig>,
    /// Every `## Rust` block with its declared (trusted) effect.
    pub rust_blocks: Vec<RustInfo>,
    /// Every binding, approved or not.
    pub bindings: Vec<BindingInfo>,
}

impl Checked {
    /// Effect of the union of all blocks of the unit (agent, service, command) called `unit`.
    pub fn unit_effect(&self, unit: &str) -> EffectSet {
        let mut e = EffectSet::pure();
        for b in self.blocks.iter().filter(|b| b.unit == unit) {
            e.merge(&b.effect);
        }
        e
    }

    pub fn expr_type(&self, file: usize, span: Span) -> Option<&Type> {
        self.expr_types.get(&NodeKey::new(file, span))
    }

    pub fn stmt_effect(&self, file: usize, span: Span) -> Option<&EffectSet> {
        self.stmt_effects.get(&NodeKey::new(file, span))
    }

    /// Bindings that still need a human to approve their effect.
    pub fn unverified_bindings(&self) -> impl Iterator<Item = &BindingInfo> {
        self.bindings.iter().filter(|b| b.is_unverified())
    }
}

/// Check a loaded project. `Ok` carries everything later passes need; `Err` carries every
/// diagnostic found (errors and warnings, ordered by file and line). A project that failed to
/// load returns its load diagnostics unchanged.
pub fn check(project: &Project) -> Result<Checked, Vec<Diagnostic>> {
    if !project.load_diagnostics.is_empty() {
        return Err(project.load_diagnostics.clone());
    }
    let mut ck = Checker::new(project);
    ck.run();
    ck.finish()
}

// ---------------------------------------------------------------------------------- checker

/// Constants are typed on demand (one may use another).
enum ConstState {
    InProgress,
    Done(Type),
}

pub(crate) struct Checker<'a> {
    pub p: &'a Project,
    pub diags: Vec<Diagnostic>,
    pub reg: Registry,
    pub scopes: Vec<Scope>,
    /// Declarations that something refers to (for unused-declaration warnings).
    pub used: HashSet<Key>,
    /// `(file, import index)` of imports from which something was used.
    pub used_imports: HashSet<(usize, usize)>,
    pub records: BTreeMap<String, RecordInfo>,
    pub sigs: HashMap<Key, Sig>,
    pub bindings: BTreeMap<String, BindingInfo>,
    pub rust_blocks: Vec<RustInfo>,
    /// Record each rule is `for`.
    pub rule_records: HashMap<String, String>,
    consts: HashMap<String, ConstState>,
    pub const_infos: BTreeMap<String, ConstInfo>,
    pub expr_types: HashMap<NodeKey, Type>,
    pub stmt_effects: HashMap<NodeKey, EffectSet>,
    pub blocks: Vec<BlockInfo>,
    pub memo: HashMap<BodyKey, Memo>,
    /// The record variable a `## Do` block receives from its first caller (Q30).
    pub do_ctx: HashMap<BodyKey, Option<Implicit>>,
    /// `do` blocks that something calls: `(file, agent decl, block)`.
    pub called_blocks: HashSet<(usize, usize, usize)>,
}

impl<'a> Checker<'a> {
    fn new(p: &'a Project) -> Self {
        Checker {
            p,
            diags: Vec::new(),
            reg: Registry::default(),
            scopes: Vec::new(),
            used: HashSet::new(),
            used_imports: HashSet::new(),
            records: BTreeMap::new(),
            sigs: HashMap::new(),
            bindings: BTreeMap::new(),
            rust_blocks: Vec::new(),
            rule_records: HashMap::new(),
            consts: HashMap::new(),
            const_infos: BTreeMap::new(),
            expr_types: HashMap::new(),
            stmt_effects: HashMap::new(),
            blocks: Vec::new(),
            memo: HashMap::new(),
            do_ctx: HashMap::new(),
            called_blocks: HashSet::new(),
        }
    }

    pub fn path(&self, file: usize) -> &'a str {
        &self.p.files[file].path
    }

    pub fn error(
        &mut self,
        code: &'static str,
        file: usize,
        span: Span,
        msg: impl Into<String>,
    ) -> usize {
        self.diags
            .push(Diagnostic::error(code, self.path(file), span, msg));
        self.diags.len() - 1
    }

    pub fn warn(
        &mut self,
        code: &'static str,
        file: usize,
        span: Span,
        msg: impl Into<String>,
    ) -> usize {
        self.diags
            .push(Diagnostic::warning(code, self.path(file), span, msg));
        self.diags.len() - 1
    }

    fn run(&mut self) {
        let (reg, scopes) = scope::build(self.p, &mut self.diags);
        self.reg = reg;
        self.scopes = scopes;
        self.declare_records();
        self.declare_signatures();
        self.check_agent_headers();
        self.check_constants();
        self.check_all_bodies();
        self.check_runs();
        limits::check(self);
        unused::check(self);
    }

    fn finish(mut self) -> Result<Checked, Vec<Diagnostic>> {
        let order: HashMap<&str, usize> = self
            .p
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.path.as_str(), i))
            .collect();
        self.diags.sort_by_key(|d| {
            (
                order.get(d.file.as_str()).copied().unwrap_or(usize::MAX),
                d.span.line,
                d.span.col,
                d.severity == Severity::Warning,
            )
        });
        self.diags.dedup();
        if self.diags.iter().any(|d| d.severity == Severity::Error) {
            return Err(self.diags);
        }
        let mut functions = BTreeMap::new();
        for ((kind, name), sig) in &self.sigs {
            if *kind == Kind::Function {
                functions.insert(name.clone(), sig.clone());
            }
        }
        Ok(Checked {
            files: self.p.files.iter().map(|f| f.path.clone()).collect(),
            warnings: self.diags,
            records: self.records,
            expr_types: self.expr_types,
            stmt_effects: self.stmt_effects,
            blocks: self.blocks,
            manifest: self.p.manifest.clone(),
            constants: self.const_infos,
            functions,
            rust_blocks: self.rust_blocks,
            bindings: self.bindings.into_values().collect(),
        })
    }

    /// Typing of constants happens once, on demand; this runs all of them.
    fn check_constants(&mut self) {
        let names: Vec<String> = self
            .reg
            .of_kind(Kind::Constant)
            .map(|e| e.name.clone())
            .collect();
        for n in names {
            self.const_type(&n);
        }
    }

    /// `# Run` items must name an agent, service or command in scope.
    fn check_runs(&mut self) {
        for (fi, f) in self.p.files.iter().enumerate() {
            for d in &f.ast.decls {
                let Decl::Run(run) = d else { continue };
                for item in &run.items {
                    let kinds = [Kind::Agent, Kind::Service, Kind::Command];
                    if self.lookup(fi, &item.name, &kinds).is_none() {
                        let cands = self.scopes[fi].names_of(&kinds);
                        let d = Diagnostic::error(
                            "E214",
                            self.path(fi),
                            item.span,
                            format!("Unknown agent, service or command \"{}\".", item.name),
                        )
                        .with_did_you_mean(scope::suggest(&item.name, &cands))
                        .with_help("Each item of \"# Run\" must be imported into this file first.");
                        self.diags.push(d);
                    }
                }
            }
        }
    }
}
