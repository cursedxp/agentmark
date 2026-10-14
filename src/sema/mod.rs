//! Semantic check (DESIGN 3): names, types, effects, tool manifest, hard limits.
//!
//! Entry point: [`check`]. Interface for later passes: see `src/sema/README.md`.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::diag::{Diagnostic, Severity, Span};
use crate::manifest::Manifest;
use crate::project::Project;
use crate::syntax::ast::{Decl, Effect, Expr, HandleMode};

mod body;
mod collections;
mod decls;
mod enums;
mod expr;
mod limits;
mod parallel;
mod scope;
mod stdlib;
mod stmt;
mod tools;
mod types;
mod unused;
pub mod user_tools;

pub mod effects;
mod handles;

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

/// What a name in the source stands for, so later passes never re-derive scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Resolution {
    /// A variable: loop variable, `->`/`set` result, parameter, route parameter, `state`,
    /// event record. For a path (`invoice.amount`) this is the first segment.
    Var {
        name: String,
        /// Where it was defined, in the same file (parameter, loop variable, `->` result, …);
        /// `None` for `state` and the record a `## Do` block receives.
        def: Option<Span>,
    },
    /// A bare field of the record in front of the block (`amount` for `invoice.amount`);
    /// `var` is that record's variable.
    Field {
        var: Option<String>,
        record: String,
        field: String,
    },
    /// A constant, by its declared name (module prefix removed). Meant to be inlined.
    Const { name: String },
    /// A tool's readable collection, `crm.invoices`; `record` is the element record.
    Source {
        tool: String,
        source: String,
        record: Option<String>,
    },
    /// A call (`f(x)`, `do f`, `do remind`, a rule used as a filter). `decl` is the declared
    /// name, never the module-qualified spelling.
    Call { kind: CallKind, decl: String },
    /// An enum variant: `Empty`, `Shape.Empty` (a path) or `Circle { radius: 3 }`. Both names
    /// are the declared ones.
    Variant { enum_name: String, variant: String },
    /// A record built with `Sale { region: "EU", … }`.
    Record { record: String },
}

/// What one branch of a `match` stands for (`Checked::patterns`, keyed by the branch line).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Pattern {
    /// A variant of the enum being matched.
    Variant { enum_name: String, variant: String },
    /// `some x:` of a `maybe`.
    Some,
    /// `none:` of a `maybe`.
    None,
    /// `ok x:` of a `result`.
    Ok,
    /// `error e:` of a `result` (`e` is the failure's message, text).
    Error,
}

/// A variant of an enum with its fields' resolved types.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VariantInfo {
    pub name: String,
    pub span: Span,
    pub fields: Vec<FieldInfo>,
}

/// An `# Enum` (SPEC 22.1).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnumInfo {
    pub name: String,
    pub file: usize,
    pub span: Span,
    pub variants: Vec<VariantInfo>,
}

/// A `# Type` alias with the type it stands for (aliases are resolved wherever they are written,
/// so later passes never see one).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AliasInfo {
    pub name: String,
    pub file: usize,
    pub span: Span,
    pub ty: Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CallKind {
    Function,
    RustBlock,
    Binding,
    /// A `## Do` block of the same agent.
    DoBlock,
    Rule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ToolKind {
    Manifest,
    Binding,
    /// A `# Tool` written in AgentMark (SPEC 22.5): the IR calls the function `<tool>__<verb>`.
    User,
}

/// Resolution of a tool-call statement (`email send customer: …`, `pdf render doc -> b`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolResolution {
    pub tool: String,
    pub kind: ToolKind,
    /// `Some("sha2.Sha256")`: the statement is a method call on a handle variable (SPEC 22.4);
    /// the variable is the statement's tool word.
    pub on: Option<String>,
    pub verb: Option<String>,
    /// First target word (`customer`, `invoice`, `address`).
    pub target_word: Option<String>,
    /// Record the target word names or stands in front of (`Invoice`).
    pub record: Option<String>,
    /// The in-scope variable holding that record.
    pub var: Option<String>,
    /// Field of that record the runtime reads: the `to` address of `email send customer`
    /// (`customer_email`, else `customer`, Q4) or the `id` of a crm target (Q5).
    pub field: Option<String>,
    /// The plural source of a crm-like target word (`invoice` -> `invoices`, Q5).
    pub source: Option<String>,
    /// Type of the call's result (`-> x`): `get` gives the record, `list` a list of it, a
    /// text-only tool gives text, a binding its declared `out`; `None` when sema cannot tell.
    pub result: Option<Type>,
}

/// A named declaration and where its name is written (go-to-definition, hover).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeclInfo {
    pub kind: Kind,
    pub name: String,
    pub file: usize,
    pub span: Span,
}

/// A field whose type came from SPEC 2.1 inference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InferredField {
    pub record: String,
    pub field: String,
    pub ty: Type,
    /// No name rule matched and the type fell back to `text` (this is the W001 warning).
    pub fallback: bool,
    pub file: usize,
    pub span: Span,
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
    /// File and span of the declaring name (go-to-definition).
    pub file: usize,
    pub span: Span,
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
    /// One verb of a `# Tool` (SPEC 22.5): `unit` is the tool, `label` the verb.
    Tool,
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
    /// Span of the constant's name.
    pub span: Span,
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
    /// How each parameter holds a handle (`Shared` for everything that is not one).
    pub modes: Vec<HandleMode>,
}

impl BindingItemInfo {
    /// The handle type this item is a method of (its first parameter is `self: … handle of T`).
    pub fn receiver(&self) -> Option<&str> {
        match self.sig.params.first() {
            Some(ParamInfo { name, ty: Type::Handle(h), .. }) if name == "self" => Some(h),
            _ => None,
        }
    }
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
    /// Rust type of the handles whose type is not simply `<crate>::<Type>` (`## Handle:` blocks).
    pub handles: BTreeMap<String, String>,
}

impl BindingInfo {
    /// The effect to check calls against: the real one, or under `--allow-unverified` the proposal
    /// of an unverified binding (Q203).
    pub fn effective(&self, assume_proposed: bool) -> Effect {
        match (self.effect, self.proposed_effect) {
            (Effect::Unverified, Some(p)) if assume_proposed => p,
            (e, _) => e,
        }
    }

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
    /// Every enum by name, variants in declaration order.
    pub enums: BTreeMap<String, EnumInfo>,
    /// Every `# Type` alias by name.
    pub aliases: BTreeMap<String, AliasInfo>,
    /// The branches of every `match` (key = the branch line's span).
    pub patterns: HashMap<NodeKey, Pattern>,
    /// What every written type (key = the type expression's span) stands for, aliases resolved.
    pub type_exprs: HashMap<NodeKey, Type>,
    /// Type of every expression (including those inside interpolated strings).
    pub expr_types: HashMap<NodeKey, Type>,
    /// Effect of every expression (sub-expressions included). Only effectful ones come from
    /// tool sources and calls; everything else is `pure`.
    pub expr_effects: HashMap<NodeKey, EffectSet>,
    /// Effect of every statement, nested statements included in their parent's set.
    pub stmt_effects: HashMap<NodeKey, EffectSet>,
    /// What each path / call expression, `do` statement, rule filter ident (key = the ident's
    /// span) stands for.
    pub resolutions: HashMap<NodeKey, Resolution>,
    /// Tool-call statements, keyed by the statement span.
    pub tool_calls: HashMap<NodeKey, ToolResolution>,
    /// Path expressions that read a `maybe` value known to be non-empty here: the 0-based
    /// segment indices whose value must be unwrapped (`found.id` after `guard: found is not
    /// empty` -> `[0]`). Codegen unwraps exactly there.
    pub unwraps: HashMap<NodeKey, Vec<usize>>,
    /// For every function, which of its parameters are handles it only borrows: the caller keeps them (Q414).
    pub fn_lends: HashMap<String, Vec<bool>>,
    /// Arguments of binding calls (key = the argument's span) that are `text` where the bound
    /// function takes `bytes`: the IR wraps them in `TextToBytes` (UTF-8, Q151).
    pub text_to_bytes: std::collections::HashSet<NodeKey>,
    /// Every named declaration (record, rule, function, constant, Rust block, binding, agent,
    /// service, command) with the span of its name.
    pub declarations: Vec<DeclInfo>,
    /// Every field whose type was inferred (SPEC 2.1), for `check` to print.
    pub inferred_fields: Vec<InferredField>,
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
    /// `(file, import index)` of crate imports without a binding whose name the file used as a tool.
    pub unbound_used: HashSet<(usize, usize)>,
    pub records: BTreeMap<String, RecordInfo>,
    pub enums: BTreeMap<String, EnumInfo>,
    /// Aliases: `None` while one is being resolved (cycle guard).
    pub alias_state: HashMap<String, Option<Type>>,
    pub alias_infos: BTreeMap<String, AliasInfo>,
    pub patterns: HashMap<NodeKey, Pattern>,
    pub type_exprs: HashMap<NodeKey, Type>,
    pub sigs: HashMap<Key, Sig>,
    pub bindings: BTreeMap<String, BindingInfo>,
    /// Every `handle of <crate>.<Type>` some binding mentions (SPEC 22.4), with the names of the
    /// bindings that do.
    pub handle_types: BTreeMap<String, Vec<String>>,
    pub rust_blocks: Vec<RustInfo>,
    /// Verbs of user tools by function name (`slugger__slug`).
    pub tool_verbs: HashMap<String, user_tools::VerbRef>,
    /// Record each rule is `for`.
    pub rule_records: HashMap<String, String>,
    consts: HashMap<String, ConstState>,
    pub const_infos: BTreeMap<String, ConstInfo>,
    pub expr_types: HashMap<NodeKey, Type>,
    pub expr_effects: HashMap<NodeKey, EffectSet>,
    pub stmt_effects: HashMap<NodeKey, EffectSet>,
    pub resolutions: HashMap<NodeKey, Resolution>,
    pub tool_calls: HashMap<NodeKey, ToolResolution>,
    pub unwraps: HashMap<NodeKey, Vec<usize>>,
    pub fn_lends: HashMap<String, Vec<bool>>,
    pub text_to_bytes: std::collections::HashSet<NodeKey>,
    pub inferred_fields: Vec<InferredField>,
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
            unbound_used: HashSet::new(),
            records: BTreeMap::new(),
            enums: BTreeMap::new(),
            alias_state: HashMap::new(),
            alias_infos: BTreeMap::new(),
            patterns: HashMap::new(),
            type_exprs: HashMap::new(),
            sigs: HashMap::new(),
            bindings: BTreeMap::new(),
            handle_types: Default::default(),
            rust_blocks: Vec::new(),
            tool_verbs: HashMap::new(),
            rule_records: HashMap::new(),
            consts: HashMap::new(),
            const_infos: BTreeMap::new(),
            expr_types: HashMap::new(),
            expr_effects: HashMap::new(),
            stmt_effects: HashMap::new(),
            resolutions: HashMap::new(),
            tool_calls: HashMap::new(),
            unwraps: HashMap::new(),
            fn_lends: HashMap::new(),
            text_to_bytes: std::collections::HashSet::new(),
            inferred_fields: Vec::new(),
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

    /// Is `name` the tool name of a crate this file imports but that has no binding yet? Then the
    /// missing binding is the one problem to report (on the import line), not an unknown tool.
    pub fn unbound_crate_tool(&mut self, file: usize, name: &str) -> bool {
        let p = self.p;
        for (ii, imp) in p.files[file].imports.iter().enumerate() {
            if let crate::project::ResolvedImport::Crate { name: c, binding: None } = imp {
                if c == name || c.replace('-', "_") == name {
                    self.unbound_used.insert((file, ii));
                    return true;
                }
            }
        }
        false
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
        self.declare_aliases();
        self.collect_handle_types();
        self.declare_records();
        self.declare_enums();
        self.check_recursive_types();
        self.declare_signatures();
        self.declare_tools();
        self.check_agent_headers();
        self.check_service_states();
        self.check_constants();
        self.check_all_bodies();
        self.check_runs();
        self.check_default_command();
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
            enums: self.enums,
            aliases: self.alias_infos,
            patterns: self.patterns,
            type_exprs: self.type_exprs,
            expr_types: self.expr_types,
            expr_effects: self.expr_effects,
            stmt_effects: self.stmt_effects,
            resolutions: self.resolutions,
            tool_calls: self.tool_calls,
            unwraps: self.unwraps,
            fn_lends: self.fn_lends,
            text_to_bytes: self.text_to_bytes,
            declarations: self
                .reg
                .ents
                .values()
                .map(|e| DeclInfo {
                    kind: e.kind,
                    name: e.name.clone(),
                    file: e.file,
                    span: e.span,
                })
                .collect(),
            inferred_fields: self.inferred_fields,
            blocks: self.blocks,
            manifest: self.p.manifest.clone(),
            constants: self.const_infos,
            functions,
            rust_blocks: self.rust_blocks,
            bindings: self.bindings.into_values().collect(),
        })
    }

    /// Commands use the `state` of the project's service (Q9), so only one service may have one.
    fn check_service_states(&mut self) {
        let mut first: Option<(usize, u32, String)> = None;
        for (fi, f) in self.p.files.iter().enumerate() {
            for d in &f.ast.decls {
                let Decl::Service(s) = d else { continue };
                let Some(st) = &s.state else { continue };
                match &first {
                    None => first = Some((fi, st.span.line, s.name.name.clone())),
                    Some((ff, line, name)) => {
                        let msg = format!(
                            "Only one service may have a \"state:\" (\"{name}\" in {}:{line} has one), because commands use it.",
                            self.p.files[*ff].path
                        );
                        let d = Diagnostic::error("E218", self.path(fi), st.span, msg)
                            .with_help("Keep the state in one service, or pass data through a tool.");
                        self.diags.push(d);
                    }
                }
            }
        }
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
