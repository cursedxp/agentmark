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
| `stmt_effects: HashMap<NodeKey, EffectSet>` | effect of every statement, children included; key = the statement's span |
| `blocks: Vec<BlockInfo>` | one per function, rule, agent block (`Entry` = trigger blocks, `Action` = `## Do`), service route, command `## Run`; source order. `effect`, `declared` (functions, rules), `param` = `(variable, record)` the block works on (rule record, event record, or the loop record a `## Do` is called with) |
| `manifest: Manifest` | resolved `tools.toml` (`ToolDef { verbs, targets, sources, events, effect }`) |
| `constants: BTreeMap<String, ConstInfo>` | name, file, type, value expression (to inline) |
| `functions: BTreeMap<String, Sig>` | `# Function` signatures (`params`, `out`, declared `effect`) |
| `rust_blocks: Vec<RustInfo>` | every `## Rust` block: name, file, span, parent, `Sig` (declared, trusted effect) |
| `bindings: Vec<BindingInfo>` | every `# Binding`: crate, effect (`Unverified` until approved), proposed effect, items with `Sig`; `Checked::unverified_bindings()` |
| `warnings` | inferred field types (W001), unused import/declaration (W002/W003), crate without binding (W004), unverified binding (W005) |

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

- `## Do` blocks work on the record of the first caller's loop/event (Q30); `BlockInfo.param` records it.
- A source `crm.invoices` is `list of Invoice` (record found by the singular name, Q5); a `get`/`list` result
  is typed the same way.
- A bare field name (`amount`) resolves against the record in front of the block; `invoice.amount` against
  the variable. Constants are looked up after variables and are meant to be inlined.
- `state` in a route/command is a variable of the service's `state:` record (Q9).
- Narrowing: `is not empty` / `is some` / `guard` unwrap `maybe` values in the rest of the block.

## Layout

`mod.rs` driver and result types · `scope.rs` registry and per-file scopes · `decls.rs` record fields,
signatures, constants · `body.rs` per-body context, on-demand body checking · `expr.rs` expressions ·
`stmt.rs` statements · `tools.rs` manifest checks · `types.rs` · `effects.rs` · `limits.rs` (SPEC 18.3) ·
`parallel.rs` (SPEC 16) · `unused.rs` (SPEC 18.2).
