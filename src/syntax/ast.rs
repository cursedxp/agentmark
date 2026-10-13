//! AST for Layers 3, 2 and 1 (DESIGN 2.3).
//!
//! Every node that a later pass may need to point at carries a [`Span`]. The whole tree is
//! `Serialize` so parse snapshots are stable `insta` YAML.
//!
//! Names are [`Ident`]s (text + span) so semantic errors can put the caret under the exact word.
//! Layer-2-only nodes that `agent_lower` produces (`Entry`, `Trace`) are added by that pass.

use crate::diag::Span;
use serde::Serialize;

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
}

// ---------------------------------------------------------------- declarations

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Decl {
    Record(Record),
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
    pub name: Ident,
    /// `None` means "infer from the field name" (SPEC 2.1).
    pub ty: Option<TypeExpr>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rule {
    pub span: Span,
    pub doc: Option<String>,
    pub name: Ident,
    pub for_record: Ident,
    pub when: Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Agent {
    pub span: Span,
    pub doc: Option<String>,
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
    pub name: Ident,
    pub args: Vec<Param>,
    /// Statements of the `## Run` block.
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Binding {
    pub span: Span,
    pub doc: Option<String>,
    pub name: Ident,
    pub crate_name: Ident,
    pub effect: Spanned<Effect>,
    pub proposed_effect: Option<Spanned<Effect>>,
    pub items: Vec<BindingItem>,
    pub not_bound: Vec<NotBound>,
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
    Print(Expr),
    Try {
        stmt: Box<Stmt>,
        /// `None`: no `otherwise:` (a failure stops the block and is traced).
        otherwise: Option<Vec<Stmt>>,
    },
    Parallel(Vec<Stmt>),
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
