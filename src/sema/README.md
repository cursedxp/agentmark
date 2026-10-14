# sema: interface for later passes

```rust
let project = agentmark::project::Project::load(path);          // file or folder
let checked: Result<Checked, Vec<Diagnostic>> = agentmark::sema::check(&project);
project.render(&diags)                                           // SPEC 10 text, any file of the project
```

`Project::from_str(path, text, tools_toml)` / `Project::from_memory(root, files)` build in-memory projects
(tests, LSP buffers).

## Project (`src/project.rs`)

| item | meaning |
|---|---|
| `Project.kind` | `SingleFile` (root = the file's folder) or `Folder` (every `*.agent.md` below it) |
| `Project.files: Vec<SourceFile>` | parsed files; the index is the **file id** used everywhere (`NodeKey.file`, `BlockInfo.file`) |
| `SourceFile { path, text, ast, imports, rel_dir }` | `imports[i]` is the resolution of `ast.imports[i]` (`ResolvedImport::Files(..)` with module prefix, `Crate { name, binding }`, `Unresolved`) |
| `Project.manifest`, `.config` | `tools.toml` (`Manifest`), `agentmark.toml` (`Config { name, version, targets, lint_deny }`) |
| `Project.entry_files` | roots of the check |
| `Project.load_diagnostics` | syntax / import / cycle / manifest errors; `check` returns them unchanged |
| `Project.sources` | text of every file read, so `render` works for `tools.toml` too |

## `sema::check` result

`Err(diags)`: every diagnostic (errors **and** warnings), ordered by file, line, column. Filter on
`Diagnostic.severity`. `Ok(Checked)`: no errors; warnings are in `Checked.warnings`.

`Checked` (all `Serialize`):

| field | meaning |
|---|---|
| `files: Vec<String>` | display path per file id |
| `records: BTreeMap<String, RecordInfo>` | every record; `fields: Vec<FieldInfo { name, ty, inferred, span }>`, declaration order, types resolved (SPEC 2.1 inference applied) |
| `expr_types: HashMap<NodeKey, Type>` | type of every expression, interpolation parts included. `NodeKey { file, span }`: the span is the expression's own span (unique per node within a file) |
| `expr_effects: HashMap<NodeKey, EffectSet>` | effect of every expression (sub-expressions included): `network(crm)` for `crm.invoices`, the callee's set for a call, `pure` otherwise |
| `stmt_effects: HashMap<NodeKey, EffectSet>` | effect of every statement, children included; key = the statement's span |
| `blocks: Vec<BlockInfo>` | one per function, rule, agent block (`Entry` = trigger blocks, `Action` = `## Do`), service route, command `## Run`; source order. `effect`, `declared` (functions, rules), `param` = `(variable, record)` the block works on (rule record, event record, or the loop record a `## Do` is called with) |
| `resolutions: HashMap<NodeKey, Resolution>` | what a name stands for, keyed by the span of the path / call expression, the `do` statement, or the rule-filter ident: `Var{name, def}` (first segment of a path; `def` = where it was defined, same file, `None` for `state` and a `## Do` block's received record), `Field{var, record, field}` (bare field), `Const{name}` (declared name, prefix removed; inline it), `Source{tool, source, record}` (`crm.invoices`), `Call{kind: Function\|RustBlock\|Binding\|DoBlock\|Rule, decl}` (declared name, never the aliased spelling; a binding call is `Call{Binding, binding name}` keyed by its statement) |
| `declarations: Vec<DeclInfo>` | every named declaration `{kind, name, file, span of the name}`; `Sig.file/span`, `ConstInfo.span`, `RecordInfo.span`, `BindingInfo.span`, `RustInfo.span` give the same for go-to-definition |
| `tool_calls: HashMap<NodeKey, ToolResolution>` | per tool-call statement (key = statement span): `tool`, `kind` (Manifest / Binding), `verb`, `target_word`, `record` (what the word names or stands in front of), `var` (the in-scope variable holding it), `field` (the field the runtime reads: `customer_email` else `customer` for `email send customer`, `id` for a crm target). `email send customer` without a record carrying such a field in front of the block is an error; a crm target gets `var` only from the record in front of the block or the single in-scope variable of that record (several = error "Which invoice?"); with an explicit value after the word (`crm get invoice id`) `var`/`field` are `None` |
| `unwraps: HashMap<NodeKey, Vec<usize>>` | path expressions that read a `maybe` value known to be non-empty (after `is not empty`, `guard`, `is some`): the 0-based segment indices whose value codegen must unwrap. Type of the expression is already the unwrapped type |
| `inferred_fields: Vec<InferredField>` | every field whose type came from SPEC 2.1 inference (`fallback` = no rule matched, the W001 case), for `check` to print |
| `manifest: Manifest` | resolved `tools.toml` (`ToolDef { verbs, targets, sources, events, effect }`) |
| `constants: BTreeMap<String, ConstInfo>` | name, file, type, value expression (to inline) |
| `functions: BTreeMap<String, Sig>` | `# Function` signatures (`params`, `out`, declared `effect`) |
| `rust_blocks: Vec<RustInfo>` | every `## Rust` block: name, file, span, parent, `Sig` (declared, trusted effect) |
| `bindings: Vec<BindingInfo>` | every `# Binding`: crate, effect (`Unverified` until approved), proposed effect, items with `Sig`; `Checked::unverified_bindings()` |
| `warnings` | inferred field types (W001, only when it falls back to text), unused import/declaration (W002/W003), crate without binding (W004), unverified binding (W005), several runnable declarations in a single file (W006), line longer than 120 only because of string text (W007) |

Helpers: `Checked::unit_effect(unit)`, `expr_type(file, span)`, `stmt_effect(file, span)`.

## Types (`sema::Type`)

`Text Int Money Date DateTime Duration Bool Bytes List(T) Maybe(T) Result(T) Record(name) Object(fields)`
(the type of a record literal) and `Unknown` (only appears next to an already-reported error, so it never
occurs in an `Ok` result except for tool results whose type the manifest cannot tell, e.g. `crm update … -> x`).
Record names are unique in a project (OPEN_QUESTIONS Q31), so `Record(String)` identifies a record.

## Effects (`sema::EffectSet`)

`{ model: bool, network: Vec<String>, local: Vec<String>, unverified: Vec<String> }`, names in first-use order.
`class()` is the highest class (`Pure < Local < Network < Model`; `Unverified` ranks like `Network` for the
"does the body exceed its declared effect" test), `is_pure()`, `merge`, `Display` = SPEC 8 summary:
`model, network(email, crm), local(log)` / `pure`. Sources: tools (their manifest effect), functions and Rust
blocks (name, declared effect; a function's own inferred set when it is larger), bindings (binding name),
built-ins `print`, `file` (`read file`), `state` (`append … to state.…`), and `model` for `ask model`.
Reading `crm.invoices` is `network(crm)`.

## What the check resolved that the AST does not say

- `## Do` blocks work on the record of the callers' loop/event (all callers must agree on the record, Q30); `BlockInfo.param` is `(first caller's variable, record)`.
- A source `crm.invoices` is `list of Invoice` (record found by the singular name, Q5); a `get`/`list` result
  is typed the same way.
- A bare field name (`amount`) resolves against the record in front of the block; `invoice.amount` against
  the variable. Constants are looked up after variables and are meant to be inlined.
- `state` in a route/command is a variable of the service's `state:` record (Q9); only one service may have one. Reading it is pure, `append … to state.…` is `local(state)` (Q39).
- Name order: variable, bare field, constant, tool source (needs the dot). `## Do` blocks nobody calls are checked leniently: a bare name passes if some record has such a field, otherwise it is reported (Q30).
- Narrowing keys bare fields as `var.field` (`score` and `lead.score` are one fact); re-checking a present value is fine. `is not empty` / `is some` / `guard` unwrap `maybe` values in the rest of the block.
- `result of T` (Q200): `return v` in such a function takes a `T`; `try do f(x) -> h` types `h` as `T`; calling it without `try`, or inside an expression, is an error. A crate import without binding is reported once on the import line (`E219` when a statement uses it, Q201), without `Unknown tool` or `Unused import` follow-ons.

## Layout

`mod.rs` driver and result types · `scope.rs` registry and per-file scopes · `decls.rs` record fields,
signatures, constants · `body.rs` per-body context, on-demand body checking · `expr.rs` expressions ·
`stmt.rs` statements · `tools.rs` manifest checks · `types.rs` · `effects.rs` · `limits.rs` (SPEC 18.3) ·
`parallel.rs` (SPEC 16) · `unused.rs` (SPEC 18.2).

## Bindings from `bindings/` (CLI `check`)

A `# Import: crate x` whose `bindings/x.agent.md` (or `.agentmark/bindings/`) exists is loaded by the project
loader as a file of the project; its `# Binding` is typed like any declaration, so `- sha2 sha256_digest data -> h`
checks against the bound signature, takes the binding's `- effect:` (or `unverified`, listed by
`Checked::unverified_bindings()` and by `agentmark check`). This needed no extra code in sema.
