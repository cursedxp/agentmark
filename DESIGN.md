# AgentMark — Design (v0.1 → v0.2)

This document fixes the architecture every implementation step follows. SPEC.md is the
language; this file is how the compiler is built. When the two disagree, SPEC wins and the
disagreement goes into `OPEN_QUESTIONS.md`.

## 1. Repository layout

The repository root is a Cargo **package and workspace** at the same time, so the paths named
in PROMPT.md (`tests/errors/`, `tests/docs.rs`, `src/binder/core`, `codegen/rust.rs`) are literal.

```
Cargo.toml                    [package] agentmark (lib + bin) + [workspace] members
src/
  lib.rs                      pub mod for every pass; the LSP depends on this crate as a library
  main.rs                     thin: calls cli::run()
  cli.rs                      clap derive: check build run trace bind fmt lint doc
  diag.rs                     Diagnostic, Span, render (file:line, caret, help), did_you_mean
  syntax/lexer.rs             line lexer: source text -> Vec<Line>
  syntax/ast.rs               AST for Layers 3, 2, 1 (one enum per syntactic category)
  syntax/parser.rs            chumsky parsers: lines -> AST, expression/statement grammar
  project.rs                  agentmark.toml, import graph, glob imports, cycle detection
  manifest.rs                 tools.toml model + validation
  sema/                       semantic check
    mod.rs                    check(&Project) -> Result<Checked, Vec<Diagnostic>>
    scope.rs                  name resolution per file (a file sees only what it imports)
    types.rs                  Type, inference (2.1), unit/currency checks (5, 6)
    effects.rs                effect inference + verification (8, 13.1, 17)
    limits.rs                 hard limits (18.3)
    parallel.rs               write-conflict check (16)
  lower/agent_lower.rs        L3 -> L2 (AST -> AST): Agent/Rule/trigger/ask model/guard disappear
  ir/mod.rs                   IR types (no agent concepts, effect on every node)
  ir/build.rs                 L2 AST (checked) -> IR
  codegen/rust.rs             IR -> Cargo project (SPEC 9.1); submodules under codegen/rust/
  fmt.rs  lint.rs  doc.rs     readability tooling (18.4, 18.6, 18.7)
  binder/core/                target-agnostic: tiers, binding-file writer, staleness, unverified
  binder/rust/                rustdoc JSON reader -> binder/core item model
crates/agentmark-rt/          runtime crate generated projects depend on (SPEC 9.2)
crates/agentmark-lsp/         v0.2 language server (reuses `agentmark` lib)
editors/vscode/  editors/tree-sitter-agentmark/  templates/  docs/
tests/                        integration tests: parse snapshots, errors, examples e2e, docs
tests/errors/*.agent.md       deliberate mistakes; each file's first lines document the exact expected message
```

Crates: `chumsky = "0.10"` (no hand-rolled parser), `clap` 4 derive, `insta` for snapshots,
`strsim` for did-you-mean, `toml`, `serde`/`serde_json`, `thiserror`, `glob`, `tempfile` (tests).
Runtime: `tokio`, `chrono`, `serde`, `serde_json`, `reqwest` (rustls), `axum` 0.7, `async-trait`, `thiserror`.

## 2. Front end

### 2.1 Lexer (`syntax/lexer.rs`)

AgentMark is line-structured, so the lexer is a line classifier. Each `Line` has `{ line_no,
indent (in 2-space units), kind, text, span }`. Kinds: `Heading1`, `Heading2`, `Comment` (`>`),
`Item` (`- …`), `Control` (bare line ending in `:` or bare `stop`, e.g. `for each … :` without a dash),
`FenceOpen(lang)`, `FenceBody`, `FenceClose`, `Blank`. Tabs → error `E001`. Odd indentation →
error `E002`. Fenced code is captured verbatim (indent stripped by the fence's own indent) so Rust
passes through untouched. Every `Line` keeps byte offsets for diagnostics.

### 2.2 Parser (`syntax/parser.rs`)

Two levels, both chumsky:

1. **Structure**: `Vec<Line>` → declarations. A `Heading1` opens a declaration; `- key: value` items
   before the first `Heading2` are properties; a `Heading2` opens a block; indented items/control
   lines form a statement tree by indentation. A `>` comment directly above a heading becomes that
   node's `doc`. In a `# Function`, the indented statements after the properties are the body
   (SPEC 13.1 example).
2. **Inline**: chumsky parsers over the text of one line for headings (triggers, routes, Rust
   block signatures, binding signatures), statements and expressions.

### 2.3 AST (`syntax/ast.rs`)

```
File { path, decls: Vec<Decl>, imports: Vec<Import> }
Decl  = Record{name, fields:[Field{name, ty: Option<TypeExpr>}]}
      | Rule{name, for_record, when: Expr}
      | Agent{name, model, tools, memory, timeout, blocks:[Block]}
      | Function{name, params, out, effect, body:[Stmt]}
      | Service{name, listen, state, routes:[Route{method, path, body}]}
      | Command{name, args:[Arg{name, ty, default}], body}
      | Binding{name, crate_name, effect, proposed_effect, items:[BindingItem{sig, rust, doc}], not_bound}
      | Constant{name, value: Expr}
      | Run{items:[Ident]}
      | RustBlock{name, params, out, effect, code, code_line}   // `## Rust:` hoisted to module level
      | Entry{trigger, label, body}                              // L2-only, produced by agent_lower
Trigger = Every(Duration) | DailyAt(h,m) | Weekly(Weekday,h,m) | OnEvent{tool,event} | OnStart | Do(name)
Stmt  = ForEach{var, filter_rule: Option, source, body} | If{cond, then, else_} | IfSome{expr, bind, then, else_}
      | Stop | Do{name, args, into} | AskModel{with, prompt, as_record, into}
      | ToolCall{tool, verb, target: Option<Target>, payload: Option<Payload>, into}
      | Guard{cond: GuardCond, else_: Option<Box<Stmt>>} | Set{var, expr} | Return(expr)
      | Respond{status, body} | ReadBody{ty, into} | ReadFile{path, ty, into} | Append{value, list}
      | Print(StrLit) | Try{stmt, otherwise} | Parallel(Vec<Stmt>)
      | Call{fn, args, into} | Trace{..} (L2-only, produced by agent_lower)
Expr  = Lit | Path(a.b.c) | Interp(String parts) | Binary | Unary(not) | Call | RecordLit
      | CountOf{rule, source} | FirstIn{var, source, where} | IsEmpty/IsNotEmpty | Today | Now
```

Every node carries a `Span` (file, line, column range). The AST is `serde::Serialize` so parse
snapshots are stable `insta` YAML.

## 3. Semantic check (`sema/`)

- **Scope**: a file sees only its imports (18.2). Names: records, rules, functions, constants,
  agents/services/commands, Rust blocks, bindings, tools (from `tools.toml` next to the project root).
- **Types** (SPEC 6): `Text Int Money Date DateTime Duration Bool Bytes List(T) Maybe(T) Record(name)`.
  Field inference per 2.1 applies the name rules to the last word of the field name (`due_date`, `createdAt`); only when no rule matches and the type falls back to `text` is it a warning `W001` printing the inferred type (SPEC: "inference failure is a warning"). Every inferred field is listed in `Checked.inferred_fields` so `check` can print it. Units are real types:
  `money + duration` is an error; money arithmetic requires matching currency when both sides are
  literals and is otherwise checked at runtime; `money * int`, `money / int` → money;
  `date ± duration` → date; `Maybe(T)` must be unwrapped with `is some` / `is not empty`.
- **Effects** (SPEC 8): lattice `pure < local < network < model`, plus `unverified` for bindings.
  An `EffectSet { class, tools: BTreeSet<String> }` is computed for every statement and expression.
  Rule bodies and guard conditions must be `pure`; a `# Function` body must not exceed its declared
  `effect`; Rust blocks and bindings contribute their declared effect.
- **Manifest** (7): unknown tool, unknown verb, unknown target/source, tool not listed in the
  agent's `tools`, all errors with did-you-mean.
- **parallel** (16): two statements writing the same variable → error.
- **Hard limits** (18.3) are part of `check`; each message contains the suggested fix.

Errors are `Diagnostic { code, severity, span, message, help }` rendered as in SPEC 10:

```
examples/invoice-chaser.agent.md:12
  - email send custmer: "..."
               ^^^^^^^
  Unknown target "custmer" for tool email. Did you mean "customer"?
```

did-you-mean: Levenshtein distance ≤ 2 against every candidate name of that category.

## 4. Lowering

### 4.1 `lower/agent_lower.rs` (L3 → L2, AST → AST)

| L3 construct | L2 result |
|---|---|
| `# Agent: a` | nothing at runtime except its blocks; agent name kept only as a trace label string |
| `## Every …` / `## Every day at` / `## Every <weekday> at` / `## On start` | `Decl::Entry{ trigger: Schedule(..)/Start, label: "<heading text>" }` |
| `## When <tool> receives <event>` | `Decl::Entry{ trigger: Event{tool, event}, param: <event> }` |
| `## Do <name>` | `Decl::Function{ name: "<agent>__<name>", effect: inferred }`; `do <name>` → `Call` |
| `# Rule: r` (`for: R`, `when: e`) | `Decl::Function{ name: r, params: [record of R, fields in scope], out: bool, effect: pure, body: [Return(e)] }` |
| `for each r rec in src` | `ForEach{ var: rec, source: src, body: [If{ cond: Call(r, rec), then: body }] }` |
| `ask model …` | `Call{ fn: rt::model_complete, args: [prompt, context, as_record] }` (effect `model`) |
| `guard: c [else s]` | `Set tmp = c; Trace(guard, name, tmp); If{ cond: not tmp, then: [s or Stop] }` |

After this pass no `Agent`, `Rule`, trigger, `AskModel` or `Guard` node remains; `agent_lower`
unit tests assert that by walking the L2 AST.

### 4.2 IR (`ir/`)

Target-independent; no agent concepts; every node carries an `EffectSet`.

```
Program { records, functions: [Fn], entries: [Entry], services: [Service], commands: [Command],
          rust_blocks: [RustFn], bindings: [BindingFn], constants inlined }
Fn      { name, params, ret: Type, effect: EffectSet, body: [Stmt], source: SourceRef }
Entry   { unit: String /* trace label */, label: String, trigger: Schedule|Start|Event{tool,event}, param, body, effect }
Stmt    { kind: Let|Assign|If|IfSome|ForEach|Return|Stop|Effect(EffectCall)|Respond|Append|Parallel|Try, effect, source }
EffectCall = Tool{tool, verb, target, payload} | Model{prompt, context, schema} | Binding{binding, item, args}
           | Rust{fn, args} | Io{ReadFile|ReadBody|Print} | Trace{TraceEvent}
Expr    { kind, ty, effect }
```

`check` prints the effect summary from the IR (`// Effects: model, network(email, crm)`), which is
what makes the summary and a future `parallel` optimizer share one source of truth.

### 4.3 Codegen (`codegen/rust.rs`)

Emits the SPEC 9.1 layout. One `async fn` per block; every emitted statement is preceded by a
`// <file>.agent.md:<line>` source-map comment so `src/agent_invoice_chaser.rs` reads like the
original. `## Rust` bodies and binding bodies are copied verbatim inside `#[allow(unused)] fn`s with
a source-map comment. `# Service` → axum `Router` with shared `Arc<Mutex<State>>`; `# Command` →
clap subcommand; `# Binding` → `impl` block; `tools.toml` → one `#[async_trait] trait` per tool
plus `impl Trait for agentmark_rt::MockTools` delegating to `MockTools::invoke`. Generated
`Cargo.toml` depends on `agentmark-rt` by path (absolute path of the compiler checkout), so tests
work offline. A rustc failure on generated code is reported as `internal compiler error` with the
generated file path — never as a user error.

## 5. Runtime (`crates/agentmark-rt`)

- `Money { minor: i64, currency: Currency }`, `Currency` (ISO code, minor-unit exponent 2 for EUR,
  USD, GBP, CHF; 0 for JPY). Parse/Display `"1200.00 EUR"`. Checked add/sub (currency mismatch →
  `MoneyError::CurrencyMismatch`), `mul_int`, `div_int` with **round half away from zero**,
  serde as the display string.
- `Ctx` holds `trace: Trace`, `model: Arc<dyn ModelClient>`, `tools: Arc<MockTools or real>`,
  `clock` (today/now; mock today comes from `model.json` `"today"`), and the current trace labels
  `unit` and `entry`.
- Scheduler: `Every(Duration)`, `EveryDayAt(h,m)`, `Weekly(weekday,h,m)` on tokio; `--once` runs
  every scheduled entry exactly once, in `# Run` order, then the event entries once per fixture item.
- `ModelClient` trait (`async fn complete(&self, req: ModelRequest) -> Result<String>`), with
  `AnthropicModel` (reqwest, Messages API) and `MockModel` (responses keyed by prompt prefix,
  longest-prefix match). v0.2 adds `OpenAiModel`, `LocalModel` (OpenAI-compatible base URL).
- `MockTools`: loads `fixtures/<tool>.<source>.json`; `list/get` return fixture data,
  `update/send/post` return `null`; nothing is traced here — tracing is done by `Ctx`.
- `Trace`: append-only JSONL writer (file or stdout).

### 5.1 Trace schema (exact; tests compare parsed JSON objects line by line)

| kind | fields |
|---|---|
| `model` | `agent`, `block`, `prompt_prefix` (the fixture key that matched), `context` (only with `ask model with v`: `{"id": v.id}`) |
| `guard` | `agent`, `name` (guard source text with constants inlined), `result` `"pass"`/`"fail"` |
| `tool` email | `tool`, `verb`, `target`, `to` |
| `tool` crm | `tool`, `verb`, `source`, `id`, and for `update`: `set` (object of assigned fields, or the whole record) |
| `tool` http | `tool`, `verb`, `url`, `body` (if any) |
| `tool` log | `tool`, `message` |
| `tool` (user `# Tool`, SPEC 22.5) | `tool`, `verb`, `args` (object of parameter values; handle parameters omitted) |
| `file` (built-in, SPEC 22.3) | `verb`, `path`, and `to` for `rename` |
| `http` | `method`, `path`, `status` (one line per request served by a `# Service`) |
| `print` | `message` |
| `error` | `agent`, `block`, `message` (a `try` without `otherwise` that failed) |

`agent` is the trace label of the enclosing unit (agent, service, or command name); `block` is the
heading of the **entry** that started the run (a `## Do` block called from `## Every day at 09:00`
reports `Every day at 09:00`). Reading a source in `for each … in crm.invoices` is not traced.

Target resolution for tool calls: an email target word `customer` resolves to the `customer_email`
field of the in-scope record if present, else the `customer` field; `address "<literal>"` uses the
literal. A crm target word is the singular of a source (`invoice` → `invoices`); `id` is the
in-scope record's `id`.

## 6. Binder (`binder/`)

`binder/rust` runs `cargo +nightly rustdoc -Z unstable-options --output-format json` in a scratch
crate (or reads `AGENTMARK_RUSTDOC_JSON` pointing at a pre-generated file — tests use checked-in,
trimmed rustdoc JSON fixtures under `tests/binder/` so `cargo test` needs neither nightly nor
network) and maps it into `binder/core::CrateItems { name, version, items: [Item{path, doc, sig}] }`.
`binder/core` assigns every public item to exactly one tier (direct → assisted via `ModelClient`,
body type-checked by `cargo check` in a scratch crate → skipped with reason), writes
`bindings/<crate>.agent.md` with the header from 15.4, a one-line rustdoc description above every
bound item, `- effect: unverified` and `- proposed-effect: …`. Staleness = header version ≠
resolved crate version → re-bind and re-mark `unverified`.

## 7. How each example lowers to IR

**01 invoice-chaser.** `overdue` becomes a pure `Fn(invoice: Invoice) -> bool` with
`grace_period` inlined as `14 days`. `## Every day at 09:00` becomes an `Entry{trigger: DailyAt(9,0),
unit: "invoice-chaser"}` whose body is a `ForEach` over the effectful source read `crm.invoices`
with an `If(overdue(invoice))` around `Call(invoice_chaser__remind, invoice)`. `## Do remind` becomes
an `Fn` (effect `model, network(email, crm), local(log)`) containing a `Model` effect call, two
`Trace(guard)`+`If(not …) Stop` pairs, and three `Tool` effect calls.

**02 lead-qualifier.** `## When crm receives lead` becomes an `Entry{trigger: Event{crm, lead},
param: lead: Lead}`; `ask model with lead … as Qualification` is a `Model` call with a
`Qualification` schema whose result is parsed and validated; the guard is a `matches` check; the
`if q.score >= 7` is an ordinary `If` choosing between an email `Tool` call and a log `Tool` call.

**03 support-triage.** `## Every 15 minutes` → `Entry{Every(15m)}` with a `ForEach` over
`crm.tickets`; `ticket.priority is not empty` is a pure `Maybe` test guarding `Stop` (which ends
the whole entry, per SPEC 4.1); the guard on `t.priority` lowers to trace + `If`; the `http post`
carries a record literal body.

**04 daily-report.** `## Every Monday at 08:00` → `Entry{Weekly(Mon, 8, 0)}`; `count of open
invoices in crm.invoices` is a `CountOf` expression whose filter calls the pure rule `Fn`; the
`ask model` without `with` produces a `Model` call with no context; `email send address "…"` uses
the literal address.

**05 invoice-service.** No L3 constructs, so `agent_lower` is the identity. `# Service: api` →
`Service{routes}` lowered to an axum router over `Arc<Mutex<AppState>>`; `guard … else respond 404`
is the only L3-looking construct and lowers to `If(not …) { Respond(404) }`; `first … where` is a
pure search expression; `# Command: import-csv` → clap subcommand with a `Try` around the
`ReadFile` effect call; `net_amount` is a pure `Fn` doing `Money` arithmetic; `sha2` comes from a
binder-generated `# Binding`, and the `## Rust: fingerprint` block only formats its output.
