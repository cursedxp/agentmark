//! The IR (DESIGN 4.2): what the code generator reads. Target independent, **no agent concepts**
//! (no agent, rule, trigger, `ask model` or guard node), constants inlined, and an [`EffectSet`]
//! on every statement and expression. `src/ir/README.md` describes every type in one line.
//!
//! Build it with [`lower`] (`agent_lower` + [`build::build`]); read effects with
//! [`effect_summary`].

use serde::Serialize;

pub use crate::manifest::ToolDef;
pub use crate::sema::{EffectSet, Pattern, Type};
pub use crate::syntax::ast::{BinOp, DurationUnit, Effect, HandleMode, Weekday};

pub mod build;
pub mod collections;

/// `agent_lower` then [`build::build`]: the whole front-to-IR step for a checked project.
pub fn lower(project: &crate::project::Project, checked: &crate::sema::Checked) -> Program {
    let l2 = crate::lower::agent_lower(project, checked);
    build::build(project, checked, &l2)
}

// ---------------------------------------------------------------------------------- program

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Program {
    /// `name` / `version` from `agentmark.toml`, if given.
    pub package: Package,
    pub records: Vec<Record>,
    /// `# Enum`s (`# Type` aliases are gone: every type is already resolved).
    pub enums: Vec<Enum>,
    /// Functions: user `# Function`s, former rules (`Fn(record) -> bool`) and former `## Do`
    /// blocks (named `<agent>__<block>`).
    pub functions: Vec<Fn>,
    /// Blocks that run on a trigger.
    pub entries: Vec<Entry>,
    pub services: Vec<Service>,
    pub commands: Vec<Command>,
    /// `## Rust` blocks, verbatim.
    pub rust_fns: Vec<RustFn>,
    /// Items of `# Binding`s, verbatim.
    pub bindings: Vec<BindingFn>,
    /// `# Import: crate x`.
    pub crates: Vec<CrateImport>,
    /// Rust type of a handle when it is not `<crate>::<Type>` (`## Handle:` blocks of bindings).
    pub handle_rust: std::collections::BTreeMap<String, String>,
    /// The tool manifest (`tools.toml`), so codegen can write one trait per tool.
    pub tools: Vec<ToolDef>,
    /// What the program runs (SPEC 23.4): its agents, services and commands, in order.
    pub run: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Package {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CrateImport {
    pub name: String,
    pub alias: Option<String>,
    /// Name of the `# Binding` for it, if a binding file exists.
    pub binding: Option<String>,
}

/// Where a node came from, for the `// <file>.agent.md:<line>` source-map comments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceRef {
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Record {
    pub name: String,
    pub doc: Option<String>,
    pub fields: Vec<RecordField>,
    pub source: SourceRef,
}

/// An enum (SPEC 22.1). Its JSON form is internally tagged: `{"kind":"Circle","radius":3}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Enum {
    pub name: String,
    pub doc: Option<String>,
    pub variants: Vec<Variant>,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<RecordField>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecordField {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    /// Default value (command arguments).
    pub default: Option<Expr>,
    /// A handle parameter the function only borrows: it is passed as `&T` and the caller keeps it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub lent: bool,
}

/// A variable that a statement introduces.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalVar {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fn {
    /// Declared name; lowering-made names are `<agent>__<block>` (unique, not Rust-safe).
    pub name: String,
    /// `(unit, label)` to show in the effect summary instead of `name`: `("invoice-chaser",
    /// "Do remind")` for a former `## Do` block. `None` for ordinary functions and rules.
    pub display: Option<(String, String)>,
    pub doc: Option<String>,
    pub params: Vec<Param>,
    /// `None`: returns nothing. `Type::Result(T)`: `Result<T, AgentError>` in Rust.
    pub ret: Option<Type>,
    pub effect: EffectSet,
    pub body: Vec<Stmt>,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    /// Trace label of the owning agent (`invoice-chaser`). The only trace of "agent" in the IR.
    pub unit: String,
    /// Trace label of the entry: its heading (`Every day at 09:00`).
    pub label: String,
    pub trigger: Trigger,
    /// The event record: `## When crm receives lead` -> `lead: Lead`.
    pub param: Option<LocalVar>,
    /// Per-execution time limit (default 5 minutes, SPEC 2.3).
    pub timeout: Duration,
    /// The agent said `memory: file`.
    pub memory_file: bool,
    pub body: Vec<Stmt>,
    pub effect: EffectSet,
    pub source: SourceRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Duration {
    pub amount: i64,
    pub unit: DurationUnit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Trigger {
    Every(Duration),
    DailyAt { hour: u8, minute: u8 },
    Weekly { day: Weekday, hour: u8, minute: u8 },
    /// Inbound event of a tool.
    Event { tool: String, event: String },
    /// Once when the binary starts.
    Start,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Service {
    pub name: String,
    pub listen: Option<String>,
    /// Record that holds the service's state.
    pub state: Option<String>,
    pub routes: Vec<Route>,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Route {
    /// Upper-case method (`GET`).
    pub method: String,
    /// Path as written (`/invoices/{id}`).
    pub path: String,
    /// `{param}` names; each is a `text` variable in the body.
    pub path_params: Vec<String>,
    /// `GET /invoices/{id}`.
    pub label: String,
    pub body: Vec<Stmt>,
    pub effect: EffectSet,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Command {
    pub name: String,
    pub args: Vec<Param>,
    /// Record of the project's service state, available as `state` (Q9).
    pub state: Option<String>,
    pub body: Vec<Stmt>,
    pub effect: EffectSet,
    pub source: SourceRef,
    /// The `- steps:` line (the steps belong to it).
    pub run: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RustFn {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    /// Declared (trusted) effect, with the block's name as source.
    pub effect: EffectSet,
    /// Verbatim Rust.
    pub code: String,
    /// Source line of the first code line.
    pub code_line: u32,
    pub source: SourceRef,
}

/// The key that tells two binding items apart: `item`, or `Type::item` for a method.
pub fn binding_key(item: &str, on: Option<&str>) -> String {
    match on {
        Some(h) => format!("{h}::{item}"),
        None => item.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BindingFn {
    /// Binding name (`sha2`).
    pub binding: String,
    pub crate_name: String,
    /// Item name (`sha256_digest`); a call `sha2 sha256_digest x` is [`EffectCall::Binding`].
    pub item: String,
    /// `Some("sha2.Sha256")`: the item is a method of that handle type; its first parameter is
    /// the handle (`self`). Methods of different handle types may share an `item` name.
    pub on: Option<String>,
    /// How each parameter holds a handle (`Shared` = borrowed, `Mut`, `Consume` = moved in);
    /// `Shared` for every parameter that is not a handle.
    pub modes: Vec<HandleMode>,
    pub doc: Option<String>,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    /// Declared effect of the binding (`Unverified` until approved).
    pub effect: Effect,
    /// Verbatim Rust body.
    pub code: String,
    pub code_line: u32,
    pub source: SourceRef,
}

// ---------------------------------------------------------------------------------- statements

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stmt {
    pub kind: StmtKind,
    pub effect: EffectSet,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[allow(clippy::large_enum_variant)]
pub enum StmtKind {
    /// Declare a new variable.
    Let { name: String, ty: Type, value: Expr },
    /// Change a variable that exists; `ty` is the variable's declared type.
    Assign { name: String, ty: Type, value: Expr },
    If {
        cond: Expr,
        then: Vec<Stmt>,
        else_: Vec<Stmt>,
    },
    /// `if x is some v:`; `expr` is `maybe T`, `bind` is the `T` inside `then`.
    IfSome {
        expr: Expr,
        bind: LocalVar,
        then: Vec<Stmt>,
        else_: Vec<Stmt>,
    },
    ForEach {
        var: LocalVar,
        source: Expr,
        body: Vec<Stmt>,
    },
    Return(Expr),
    /// End the enclosing function, entry, route or command (SPEC 4.1: "end current block").
    Stop,
    /// Something with an effect; `into` declares a variable for its result.
    Effect {
        call: EffectCall,
        into: Option<LocalVar>,
    },
    Respond { status: u16, body: Option<Expr> },
    /// `append v to state.invoices`; `list` is a path to the list.
    Append { value: Expr, list: Expr },
    Parallel(Vec<Stmt>),
    /// `match v:` over an enum, a `maybe` or a `result`. Without `otherwise` the arms cover every
    /// case (sema checked). `scrutinee` is a `result` only as the call that makes it.
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
        otherwise: Option<Vec<Stmt>>,
    },
    /// `put`, `remove` and the loops with an index or over a map (SPEC 22.2).
    Coll(collections::CollStmt),
    /// `try s [otherwise: …]`. `defines` is the variable `stmt` introduces (`-> rows`): it is
    /// visible after the `try`, and `otherwise` may assign it. Without `otherwise` a failure
    /// stops the block and is traced as an `error`.
    Try {
        stmt: Box<Stmt>,
        otherwise: Option<Vec<Stmt>>,
        defines: Option<LocalVar>,
    },
}

/// One branch of a [`StmtKind::Match`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    /// The name the branch gives its value: the fields of the variant (`Type::Variant`), the
    /// `T` of `some` / `ok`, or the failure message (`text`) of `error`.
    pub bind: Option<LocalVar>,
    pub body: Vec<Stmt>,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum EffectCall {
    Tool(ToolCall),
    Model(ModelCall),
    /// A call into a `# Binding` item.
    Binding {
        binding: String,
        item: String,
        /// The handle type when this is a method call; `args[0]` is then the handle variable.
        on: Option<String>,
        args: Vec<Expr>,
    },
    /// A call of a `## Rust` block.
    Rust { name: String, args: Vec<Expr> },
    /// A call of a function (`do f(x) -> y`, and `do <action>` of a former `## Do` block).
    Fn { name: String, args: Vec<Expr> },
    Io(IoCall),
    Trace(TraceEvent),
    /// The built-in `file` tool (SPEC 22.3).
    File(FileCall),
    /// A call of the built-in library that can fail (`number.parse`, `csv.parse … as …`): its
    /// value is the `T` of the `result of T` expression (a failure is the error of the `try`).
    Std(Expr),
}

/// `file <verb> <path> [to <new> | : <text>]`, traced `{"kind":"file","verb":…,"path":…}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileCall {
    /// `read`, `write`, `append`, `list`, `rename`, `exists` or `delete`.
    pub verb: String,
    pub path: Expr,
    /// The new path of `rename`, the content of `write` / `append`.
    pub arg: Option<Expr>,
    /// `read` into a list of this record: the file is CSV (SPEC 23.10).
    pub records: Option<Type>,
}

/// A call of a manifest tool, with everything resolved (codegen never looks names up).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolCall {
    pub tool: String,
    pub verb: Option<String>,
    pub target: Option<Target>,
    pub payload: Option<Payload>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Target {
    /// `email send customer`: the address is read from `var.field` (Q4); `word` is what the
    /// trace calls the target (`customer`).
    Field {
        word: String,
        var: String,
        field: String,
        value: Expr,
    },
    /// `crm update invoice`, `crm get invoice id`: a record of a tool source. `source` is the
    /// plural the runtime uses (`invoices`, Q5); `id` is where the record id comes from.
    Source {
        word: String,
        source: String,
        record: Option<String>,
        /// The id as an expression (`invoice.id`, or the value written after the word).
        id: Option<Expr>,
        /// `(var, field)` when the id is a field of an in-scope record.
        id_from: Option<(String, String)>,
    },
    /// A literal or computed text: `email send address "x@y.z"` (`word` = `address`) or the URL
    /// of `http post "…"` (`word` = `None`).
    Literal { word: Option<String>, value: Expr },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[allow(clippy::large_enum_variant)] // an `Expr` is large since the library call (`Std`) and enum values joined it
pub enum Payload {
    /// `status = "reminded"`: fields to set, in written order.
    Assign(Vec<(String, Expr)>),
    /// `"text"`, `{ id: invoice.id }`, `inv`.
    Value(Expr),
}

/// `ask model` (DESIGN 4.1).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelCall {
    /// The prompt, interpolated.
    pub prompt: Expr,
    /// `with v`: reported in the trace as `{"id": v.id}`.
    pub context: Option<Expr>,
    /// `as Record`: the answer is parsed and validated against it.
    pub schema: Option<String>,
    /// Driver string of the agent that asked (`claude:haiku`); `None` outside agents.
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum IoCall {
    ReadFile { path: Expr, ty: Type },
    ReadBody { ty: Type },
    /// `read input -> text`: all of standard input.
    ReadInput,
    Print { message: Expr },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum TraceEvent {
    /// One `guard` line. `name` is the guard's source text with constants inlined and `ok` the
    /// already evaluated result (`bool`).
    Guard { name: String, ok: Expr },
    /// A user tool was called (SPEC 22.5): traced like a tool, `{"kind":"tool","tool":…,"verb":…,
    /// "args":{param: value}}`. `args` are the verb's parameters (handles are left out).
    ToolCall {
        tool: String,
        verb: String,
        args: Vec<(String, Expr)>,
    },
}

// ---------------------------------------------------------------------------------- expressions

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Type,
    pub effect: EffectSet,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ExprKind {
    Str(String),
    Int(i64),
    /// Integer minor units.
    Money { minor: i64, currency: String },
    Duration(Duration),
    /// `2026-10-13`.
    Date(String),
    Bool(bool),
    Today,
    Now,
    /// A variable, parameter, loop variable or `state`.
    Var(String),
    /// `base.field`.
    Field { base: Box<Expr>, field: String },
    /// `inner` is `maybe T` but known to be present here (after `is not empty`, `guard`,
    /// `is some`); the value is the `T`. `what` is the source text, for the `expect` message.
    Unwrap { inner: Box<Expr>, what: String },
    /// A `text` passed where a binding takes `bytes`: its UTF-8 bytes (Q151). `ty` is `bytes`.
    TextToBytes(Box<Expr>),
    Interp(Vec<InterpPart>),
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Not(Box<Expr>),
    /// A call of a function (`Fn`) or `## Rust` block.
    Call {
        callee: Callee,
        name: String,
        args: Vec<Expr>,
    },
    /// A record literal `{ id: x }` (its `ty` is `Type::Object`).
    Record(Vec<(String, Expr)>),
    /// `some x`: a present `maybe`.
    Some(Box<Expr>),
    /// `none`: an absent `maybe` (its `ty` is the `maybe T` the context asks for, or `maybe ?`).
    None,
    /// A record built field by field: `Sale { region: "EU", amount: 120.00 EUR }`. Fields a
    /// `maybe` record field leaves out are not listed. `ty` is `Type::Record`.
    Struct {
        record: String,
        fields: Vec<(String, Expr)>,
    },
    /// An enum value: `Circle { radius: 3 }`, `Empty`. Fields a `maybe` variant field leaves out
    /// are not listed.
    Variant {
        enum_name: String,
        variant: String,
        fields: Vec<(String, Expr)>,
    },
    /// A call of the built-in library (`text.split(t, ",")`, SPEC 22.3). `decode` is the type of
    /// `json.parse(t) as T` / the element type of `csv.parse(t) as list of T`; `ty` is the type
    /// of the whole call (`result of …` for the ones that can fail).
    Std {
        module: String,
        name: String,
        args: Vec<Expr>,
        decode: Option<Type>,
    },
    /// `crm.invoices`: read a tool's collection.
    Source {
        tool: String,
        source: String,
        record: Option<String>,
    },
    /// `count of …`; with `filter`, only elements for which the pure `Fn(record) -> bool` of
    /// that name holds.
    Count {
        source: Box<Expr>,
        filter: Option<String>,
    },
    /// `first x in source where cond` -> `maybe T`.
    First {
        var: String,
        source: Box<Expr>,
        cond: Box<Expr>,
    },
    IsEmpty(Box<Expr>),
    IsNotEmpty(Box<Expr>),
    /// `x matches Record`: the model's answer fits the record.
    Matches { expr: Box<Expr>, record: String },
    /// `x length`.
    Length(Box<Expr>),
    /// `no amounts changed in x`: the money in `output` all appear in `input` (the context or
    /// prompt of the model call that wrote `output`; `output` itself when it did not come from
    /// a model call, which makes the check trivially true).
    NoAmountsChanged { output: Box<Expr>, input: Box<Expr> },
    /// A collection word (SPEC 22.2).
    Coll(Box<collections::CollOp>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Callee {
    Fn,
    Rust,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[allow(clippy::large_enum_variant)]
pub enum InterpPart {
    Text(String),
    Expr(Expr),
}

// ---------------------------------------------------------------------------------- effects

/// SPEC 8: the effect summary per block, computed from the IR: `(unit, label, effects)` for
/// every entry, service route, command and function, in program order.
/// `EffectSet`'s `Display` is the SPEC string (`model, network(crm, email), local(log)`).
pub fn effect_summary(p: &Program) -> Vec<(String, String, EffectSet)> {
    let mut out = Vec::new();
    for e in &p.entries {
        out.push((e.unit.clone(), e.label.clone(), e.effect.clone()));
    }
    for s in &p.services {
        for r in &s.routes {
            out.push((s.name.clone(), r.label.clone(), r.effect.clone()));
        }
    }
    for c in &p.commands {
        out.push((c.name.clone(), "Run".to_string(), c.effect.clone()));
    }
    for f in &p.functions {
        let (unit, label) = f.display.clone().unwrap_or((f.name.clone(), f.name.clone()));
        out.push((unit, label, f.effect.clone()));
    }
    out
}

/// The union of the effects of some statements.
pub fn union_effects(stmts: &[Stmt]) -> EffectSet {
    let mut e = EffectSet::pure();
    for s in stmts {
        e.merge(&s.effect);
    }
    e
}
