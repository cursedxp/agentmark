//! AST for Layers 3, 2 and 1 (DESIGN 2.3).
//!
//! Every node that a later pass may need to point at carries a [`Span`]. The whole tree is
//! `Serialize` so parse snapshots are stable `insta` YAML.
//!
//! Names are [`Ident`]s (text + span) so semantic errors can put the caret under the exact word.
//! Layer-2-only nodes that `agent_lower` produces ([`Entry`], [`StmtKind::Trace`],
//! [`StmtKind::ModelCall`]) live here too; the parser never builds them.

use crate::diag::Span;
use serde::Serialize;

/// Plain `>` comment lines directly above a `- key: value` property of a declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PropComment {
    pub key: String,
    pub comments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

// ---------------------------------------------------------------- file

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct File {
    pub path: String,
    /// Leading `>` comment block that is separated from the first heading by a blank line.
    pub doc: Option<String>,
    pub imports: Vec<Import>,
    pub decls: Vec<Decl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Import {
    pub span: Span,
    pub doc: Option<String>,
    pub kind: ImportKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ImportKind {
    /// `# Import: ./records/invoice.agent.md` or `... as billing`
    File { path: String, alias: Option<Ident> },
    /// `# Import: ./rules/*.agent.md`
    Glob { pattern: String },
    /// `# Import: ./billing as billing`
    Folder { path: String, alias: Ident },
    /// `# Import: crate serde_json` or `crate reqwest as http_client`
    Crate { name: Ident, alias: Option<Ident> },
}

// ---------------------------------------------------------------- types

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TypeExpr {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TypeKind {
    /// A built-in (`text`, `int`, `money`, `date`, `datetime`, `duration`, `bool`, `bytes`) or the
    /// name of a record; sema tells them apart.
    Named(String),
    /// `list of T`
    List(Box<TypeExpr>),
    /// `maybe T`
    Maybe(Box<TypeExpr>),
    /// `result of T`
    Result(Box<TypeExpr>),
    /// `map of K to V` (SPEC 22.2)
    Map(Box<TypeExpr>, Box<TypeExpr>),
    /// `handle of <crate>.<Type>`: a value of a library type AgentMark has no equivalent for
    /// (SPEC 22.4). `name` is the dotted path (`sha2.Sha256`); `mode` is how a binding
    /// parameter holds it.
    Handle { name: String, mode: HandleMode },
}

/// How a binding item uses a handle it is given (SPEC 22.4, OPEN_QUESTIONS Q360): the word in
/// front of `handle of` in a binding parameter. Everywhere else the mode is `Shared` and means
/// "owned" (the handle moves).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum HandleMode {
    /// `handle of …`: the item reads it (`&self`).
    Shared,
    /// `mut handle of …`: the item changes it (`&mut self`).
    Mut,
    /// `consume handle of …`: the item takes it (`self`); it cannot be used afterwards.
    Consume,
}

impl HandleMode {
    /// The word written in front of `handle of`, if any.
    pub fn word(self) -> Option<&'static str> {
        match self {
            HandleMode::Shared => None,
            HandleMode::Mut => Some("mut"),
            HandleMode::Consume => Some("consume"),
        }
    }
}

// ---------------------------------------------------------------- declarations

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Decl {
    Record(Record),
    /// `# Enum: Shape` (SPEC 22.1).
    Enum(Enum),
    /// `# Type: Totals` (SPEC 22.1).
    TypeAlias(TypeAlias),
    Rule(Rule),
    Agent(Agent),
    Function(Function),
    Service(Service),
    Command(Command),
    Binding(Binding),
    Constant(Constant),
    Run(Run),
    /// A `## Rust: name` block, hoisted to module level (OPEN_QUESTIONS Q2).
    RustBlock(RustBlock),
    /// Layer 2 only, produced by `agent_lower`: a block that runs on a trigger.
    Entry(Entry),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Effect {
    Pure,
    Local,
    Network,
    Model,
    /// Only valid on bindings, until a human approves them (SPEC 15.3).
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Record {
    pub span: Span,
    pub doc: Option<String>,
    pub name: Ident,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Field {
    pub span: Span,
    /// Plain `>` comment lines directly above the field.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<String>,
    pub name: Ident,
    /// `None` means "infer from the field name" (SPEC 2.1).
    pub ty: Option<TypeExpr>,
}

/// `# Enum: Shape` with one list item per variant: `- Circle: radius: int`, `- Empty`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Enum {
    pub span: Span,
    pub doc: Option<String>,
    pub name: Ident,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Variant {
    pub span: Span,
    /// Plain `>` comment lines directly above the variant.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<String>,
    pub name: Ident,
    /// Named fields; every one has a written type.
    pub fields: Vec<Field>,
}

/// `# Type: Totals` with `- is: <type>`: another name for a type.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TypeAlias {
    pub span: Span,
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prop_comments: Vec<PropComment>,
    pub name: Ident,
    pub target: TypeExpr,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rule {
    pub span: Span,
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prop_comments: Vec<PropComment>,
    pub name: Ident,
    pub for_record: Ident,
    pub when: Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Agent {
    pub span: Span,
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prop_comments: Vec<PropComment>,
    pub name: Ident,
    pub model: Spanned<String>,
    pub tools: Vec<Ident>,
    pub memory: Option<Spanned<Memory>>,
    pub timeout: Option<Spanned<DurationLit>>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Memory {
    None,
    File,
}

/// A trigger or named action inside an agent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Block {
    pub span: Span,
    pub doc: Option<String>,
    pub trigger: Trigger,
    /// The heading text exactly as written, e.g. `Every day at 09:00` (used as the trace label).
    pub label: String,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Trigger {
    Every(DurationLit),
    DailyAt { hour: u8, minute: u8 },
    Weekly { day: Weekday, hour: u8, minute: u8 },
    OnEvent { tool: Ident, event: Ident },
    OnStart,
    Do(Ident),
}

/// Layer 2 only (`agent_lower`): what is left of an agent's trigger block. The agent name and the
/// heading survive only as the plain strings `unit` and `label`, which traces report.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub span: Span,
    pub doc: Option<String>,
    /// Trace label of the agent that owned the block (`invoice-chaser`).
    pub unit: String,
    /// Heading text exactly as written (`Every day at 09:00`).
    pub label: String,
    pub trigger: EntryTrigger,
    /// The record the event delivers (`## When crm receives lead` -> `lead: Lead`).
    pub param: Option<Param>,
    /// The agent's `timeout:`, if it set one.
    pub timeout: Option<DurationLit>,
    /// The agent's `memory:` (`None` unless the agent said `file`).
    pub memory: Memory,
    pub body: Vec<Stmt>,
}

/// Layer 2 trigger of an [`Entry`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum EntryTrigger {
    Every(DurationLit),
    DailyAt { hour: u8, minute: u8 },
    Weekly { day: Weekday, hour: u8, minute: u8 },
    Event { tool: Ident, event: Ident },
    Start,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Param {
    pub name: Ident,
    pub ty: TypeExpr,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Function {
    pub span: Span,
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prop_comments: Vec<PropComment>,
    pub name: Ident,
    pub params: Vec<Param>,
    pub out: Option<TypeExpr>,
    pub effect: Spanned<Effect>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Service {
    pub span: Span,
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prop_comments: Vec<PropComment>,
    pub name: Ident,
    pub listen: Option<Spanned<String>>,
    pub state: Option<Ident>,
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Route {
    pub span: Span,
    pub doc: Option<String>,
    /// Upper-case HTTP method, e.g. `GET`.
    pub method: String,
    /// Path as written, e.g. `/invoices/{id}`.
    pub path: String,
    /// Names of the `{param}` segments in `path`.
    pub path_params: Vec<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Command {
    pub span: Span,
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prop_comments: Vec<PropComment>,
    pub name: Ident,
    pub args: Vec<Param>,
    /// Statements of the `## Run` block.
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Binding {
    pub span: Span,
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prop_comments: Vec<PropComment>,
    pub name: Ident,
    pub crate_name: Ident,
    pub effect: Spanned<Effect>,
    pub proposed_effect: Option<Spanned<Effect>>,
    pub items: Vec<BindingItem>,
    /// `## Handle: <crate>.<Type>` blocks: the Rust type a handle stands for when it is not simply
    /// `<crate>::<Type>` (a type with lifetimes or type parameters, instantiated).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub handles: Vec<BindingHandle>,
    pub not_bound: Vec<NotBound>,
}

/// `## Handle: sqlx.Query` with the Rust type in a fenced block (SPEC 22.4, Q370).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BindingHandle {
    pub span: Span,
    pub name: String,
    pub rust: RustCode,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BindingItem {
    pub span: Span,
    pub doc: Option<String>,
    pub name: Ident,
    pub params: Vec<Param>,
    pub out: Option<TypeExpr>,
    /// Verbatim Rust body.
    pub rust: RustCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotBound {
    pub span: Span,
    pub item: String,
    pub reason: Option<String>,
}

/// Verbatim Layer-1 code. `line` is the source line of the first code line (for source maps).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RustCode {
    pub code: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RustBlock {
    pub span: Span,
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prop_comments: Vec<PropComment>,
    pub name: Ident,
    /// Name of the declaration the block was written under, if any.
    pub parent: Option<Ident>,
    pub params: Vec<Param>,
    pub out: Option<TypeExpr>,
    pub effect: Spanned<Effect>,
    pub rust: RustCode,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Constant {
    pub span: Span,
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prop_comments: Vec<PropComment>,
    pub name: Ident,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Run {
    pub span: Span,
    pub doc: Option<String>,
    pub items: Vec<Ident>,
}

// ---------------------------------------------------------------- statements

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stmt {
    pub span: Span,
    /// Plain `>` comment lines directly above the statement (not documentation).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<String>,
    pub kind: StmtKind,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum StmtKind {
    /// `for each x in src:` or the filtered `for each overdue invoice in crm.invoices:`
    ForEach {
        filter_rule: Option<Ident>,
        var: Ident,
        source: Expr,
        body: Vec<Stmt>,
    },
    If {
        cond: Expr,
        then: Vec<Stmt>,
        else_: Vec<Stmt>,
    },
    /// `if score is some s:`
    IfSome {
        expr: Expr,
        bind: Ident,
        then: Vec<Stmt>,
        else_: Vec<Stmt>,
    },
    Stop,
    /// `do remind`, `do net_amount(invoice, 19) -> n`
    Do {
        name: Ident,
        args: Vec<Expr>,
        into: Option<Ident>,
        /// Written `try f(x) -> v` without the `do` (only after `try`, SPEC 22.3).
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        bare: bool,
    },
    /// `ask model [with v]: "prompt" [as Record] -> var`
    AskModel {
        with: Option<Expr>,
        prompt: Expr,
        as_record: Option<Ident>,
        into: Ident,
    },
    /// `email send customer: "..."`, `log: "..."`, `do crm get invoice id -> inv`
    ToolCall {
        tool: Ident,
        verb: Option<Ident>,
        target: Vec<TargetPart>,
        payload: Option<Payload>,
        into: Option<Ident>,
        /// Written with a leading `do` (SPEC 13.3).
        via_do: bool,
    },
    Guard {
        cond: Expr,
        else_: Option<Box<Stmt>>,
    },
    Set {
        var: Ident,
        expr: Expr,
    },
    Return(Expr),
    Respond {
        status: u16,
        body: Option<Expr>,
    },
    ReadBody {
        ty: TypeExpr,
        into: Ident,
    },
    ReadFile {
        path: Expr,
        ty: TypeExpr,
        into: Ident,
    },
    Append {
        value: Expr,
        list: Expr,
    },
    /// `read input -> text`: all of standard input (SPEC 22.3).
    ReadInput {
        into: Ident,
    },
    /// The built-in `file` tool (SPEC 22.3): `file read path -> content`, `file write path: text`,
    /// `file rename old to new`, `file list folder -> names`.
    File {
        verb: Ident,
        path: Expr,
        arg: Option<FileArg>,
        into: Option<Ident>,
    },
    /// `try csv.parse(text) as list of Sale -> sales`, `do text.trim(x) -> y`: a call of the
    /// built-in library whose value is stored (SPEC 22.3). `via_do` is the `do` spelling.
    CallInto {
        call: Expr,
        into: Ident,
        via_do: bool,
    },
    Print(Expr),
    Try {
        stmt: Box<Stmt>,
        /// `None`: no `otherwise:` (a failure stops the block and is traced).
        otherwise: Option<Vec<Stmt>>,
    },
    Parallel(Vec<Stmt>),
    /// `match shape:` with one branch per variant (SPEC 22.1).
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
        /// `- otherwise:`, always the last branch.
        otherwise: Option<Otherwise>,
    },
    /// Layer 2 only (`agent_lower`): ask the model. `ask model with v: "p" as R -> x`.
    /// `model` is the driver string of the agent that wrote it (`claude:haiku`).
    ModelCall {
        with: Option<Expr>,
        prompt: Expr,
        schema: Option<Ident>,
        into: Ident,
        model: Option<String>,
    },
    /// Layer 2 only (`agent_lower`): a line for the trace.
    Trace(TraceEvent),
    /// Collection statements (SPEC 22.2): `put`, `remove`, `for each … with index`, `for each k, v`.
    Coll(super::collections::CollStmt),
}

/// One branch of a `match`: `Circle c:`, `Shape.Empty:`, `some row:`, `none:`, `ok v:`, `error e:`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MatchArm {
    pub span: Span,
    /// Plain `>` comment lines directly above the branch.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<String>,
    /// `Shape` of `Shape.Empty`.
    pub qualifier: Option<Ident>,
    /// The variant, or `some` / `none` / `ok` / `error`.
    pub pattern: Ident,
    /// The name the branch gives the value it carries.
    pub bind: Option<Ident>,
    pub body: Vec<Stmt>,
}

/// `- otherwise:` of a `match`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Otherwise {
    pub span: Span,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<String>,
    pub body: Vec<Stmt>,
}

/// What a [`StmtKind::Trace`] records (DESIGN 5.1).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum TraceEvent {
    /// One `guard` line: `name` is the guard's source text with constants inlined, `ok` the
    /// (already evaluated) result.
    Guard { name: String, ok: Expr },
}

/// What follows the path of a `file` statement.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum FileArg {
    /// `file rename old to new`
    To(Expr),
    /// `file write path: content`
    Content(Expr),
}

/// One word or string after a tool verb: `customer`, `invoice`, `address "x@y.z"`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum TargetPart {
    Word(Ident),
    Value(Expr),
}

/// What follows the `:` of a tool call.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Payload {
    /// `status = "reminded"`, `score = q.score, status = "scored"`
    Assign(Vec<Assign>),
    /// `"text"`, `{ id: invoice.id }`, `inv`
    Value(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Assign {
    pub field: Ident,
    pub value: Expr,
}

// ---------------------------------------------------------------- expressions

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ExprKind {
    Str(String),
    Int(i64),
    /// Integer minor units (`12.50 EUR` is 1250 EUR); never a float.
    Money {
        minor: i64,
        currency: String,
    },
    Duration(DurationLit),
    /// `2026-10-13`, validated for shape only.
    Date(String),
    Bool(bool),
    Today,
    Now,
    /// `a`, `invoice.amount`, `state.invoices`
    Path(Vec<Ident>),
    /// A string containing `{...}` holes.
    Interp(Vec<InterpPart>),
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Not(Box<Expr>),
    /// `f(a, b)` or `billing.f(a)`
    Call {
        func: Vec<Ident>,
        args: Vec<Expr>,
    },
    /// `{ id: invoice.id, net: x }`
    Record(Vec<RecordField>),
    /// `some x`: a present `maybe` value (SPEC 22.1).
    Some(Box<Expr>),
    /// `none`: an absent `maybe` value.
    None,
    /// `Circle { radius: 3 }` or `Shape.Circle { radius: 3 }`: a variant with fields (SPEC 22.1).
    /// A field-less variant is written as a plain [`ExprKind::Path`] (`Empty`, `Shape.Empty`).
    Variant {
        qualifier: Option<Ident>,
        name: Ident,
        fields: Vec<RecordField>,
    },
    /// `json.parse(t) as Note`, `csv.parse(t) as list of Sale`: the call and the type to read (SPEC 22.3).
    Decode {
        call: Box<Expr>,
        ty: TypeExpr,
    },
    /// `count of rows`, `count of open invoices in crm.invoices`
    CountOf {
        rule: Option<Ident>,
        /// The plural noun word (`invoices`); documentation only.
        noun: Option<Ident>,
        source: Box<Expr>,
    },
    /// `first invoice in state.invoices where invoice.id = id`
    FirstIn {
        var: Ident,
        source: Box<Expr>,
        cond: Box<Expr>,
    },
    IsEmpty(Box<Expr>),
    IsNotEmpty(Box<Expr>),
    /// `q matches Qualification`
    Matches {
        expr: Box<Expr>,
        record: Ident,
    },
    /// `reminder length` (used as `reminder length < 800`)
    Length(Box<Expr>),
    /// `no amounts changed in reminder`
    NoAmountsChanged(Box<Expr>),
    /// Collection words (SPEC 22.2): `rows where …`, `sum of …`, `list of 1, 2`, …
    Coll(Box<super::collections::CollExpr>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum InterpPart {
    Text(String),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecordField {
    pub name: Ident,
    pub value: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DurationUnit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DurationLit {
    pub amount: i64,
    pub unit: DurationUnit,
}
