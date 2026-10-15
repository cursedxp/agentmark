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
| `Project.manifest`, `.config` | `tools.toml` (`Manifest`), `agentmark.toml` (`Config { name, version, targets, lint_deny, rust_dependencies }`) |
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
| `enums: BTreeMap<String, EnumInfo>` | every `# Enum` (SPEC 22.1): `variants: Vec<VariantInfo { name, span, fields: Vec<FieldInfo> }>` in declaration order, field types resolved |
| `aliases: BTreeMap<String, AliasInfo>` | every `# Type`, with the type it names. Aliases are replaced wherever a type is written, so later passes never see one; this is for tools |
| `type_exprs: HashMap<NodeKey, Type>` | what every written type (key = the `TypeExpr`'s span) stands for, aliases resolved. The IR reads parameter, `out`, `read … as` types from here |
| `patterns: HashMap<NodeKey, Pattern>` | what each branch of a `match` is (key = the branch line's span): `Variant{enum_name, variant}`, `Some`, `None`, `Ok`, `Error` |
| `records: BTreeMap<String, RecordInfo>` | every record; `fields: Vec<FieldInfo { name, ty, inferred, span }>`, declaration order, types resolved (SPEC 2.1 inference applied) |
| `expr_types: HashMap<NodeKey, Type>` | type of every expression, interpolation parts included. `NodeKey { file, span }`: the span is the expression's own span (unique per node within a file) |
| `expr_effects: HashMap<NodeKey, EffectSet>` | effect of every expression (sub-expressions included): `network(crm)` for `crm.invoices`, the callee's set for a call, `pure` otherwise |
| `stmt_effects: HashMap<NodeKey, EffectSet>` | effect of every statement, children included; key = the statement's span |
| `blocks: Vec<BlockInfo>` | one per function, rule, agent block (`Entry` = trigger blocks, `Action` = `## Do`), service route, command `- steps:`; source order. `effect`, `declared` (functions, rules), `param` = `(variable, record)` the block works on (rule record, event record, or the loop record a `## Do` is called with) |
| `resolutions: HashMap<NodeKey, Resolution>` | what a name stands for, keyed by the span of the path / call expression, the `do` statement, or the rule-filter ident: `Var{name, def}` (first segment of a path; `def` = where it was defined, same file, `None` for `state` and a `## Do` block's received record), `Field{var, record, field}` (bare field), `Const{name}` (declared name, prefix removed; inline it), `Source{tool, source, record}` (`crm.invoices`), `Call{kind: Function\|RustBlock\|Binding\|DoBlock\|Rule, decl}` (declared name, never the aliased spelling; a binding call is `Call{Binding, binding name}` keyed by its statement), `Variant{enum_name, variant}` (`Empty`, `Shape.Empty` as a path, or `Circle { radius: 3 }`; declared names) |
| `declarations: Vec<DeclInfo>` | every named declaration `{kind, name, file, span of the name}`; `Sig.file/span`, `ConstInfo.span`, `RecordInfo.span`, `BindingInfo.span`, `RustInfo.span` give the same for go-to-definition |
| `tool_calls: HashMap<NodeKey, ToolResolution>` | per tool-call statement (key = statement span): `tool`, `kind` (Manifest / Binding), `verb`, `target_word`, `record` (what the word names or stands in front of), `var` (the in-scope variable holding it), `field` (the field the runtime reads: `customer_email` else `customer` for `email send customer`, `id` for a crm target). `email send customer` without a record carrying such a field in front of the block is an error; a crm target gets `var` only from the record in front of the block or the single in-scope variable of that record (several = error "Which invoice?"); with an explicit value after the word (`crm get invoice id`) `var`/`field` are `None` |
| `unwraps: HashMap<NodeKey, Vec<usize>>` | path expressions that read a `maybe` value known to be non-empty (after `is not empty`, `guard`, `is some`): the 0-based segment indices whose value codegen must unwrap. Type of the expression is already the unwrapped type |
| `inferred_fields: Vec<InferredField>` | every field whose type came from SPEC 2.1 inference (`fallback` = no rule matched, the W001 case), for `check` to print |
| `manifest: Manifest` | resolved `tools.toml` (`ToolDef { verbs, targets, sources, events, effect }`) |
| `constants: BTreeMap<String, ConstInfo>` | name, file, type, value expression (to inline) |
| `functions: BTreeMap<String, Sig>` | `# Function` signatures (`params`, `out`, declared `effect`) |
| `rust_blocks: Vec<RustInfo>` | every `## Rust` block: name, file, span, parent, `Sig` (declared, trusted effect) |
| `bindings: Vec<BindingInfo>` | every `# Binding`: crate, effect (`Unverified` until approved), proposed effect, items with `Sig`; `Checked::unverified_bindings()` |
| `warnings` | inferred field types (W001, only when it falls back to text), unused import/declaration (W002/W003), crate without binding (W004), unverified binding (W005), several runnable declarations in a single file (W006), line longer than 120 only because of string text (W007), unverified binding assumed to be its proposed effect under `--allow-unverified` (W008, Q203) |

Helpers: `Checked::unit_effect(unit)`, `expr_type(file, span)`, `stmt_effect(file, span)`.

## Types (`sema::Type`)

`Text Int Money Date DateTime Duration Bool Bytes List(T) Maybe(T) Result(T) Record(name) Enum(name)
Variant(enum, variant) Handle("crate.Type") Object(fields)` (the type of a record literal) and `Unknown` (only appears next to an already-reported error, so it never
`Text Int Money Date DateTime Duration Bool Bytes List(T) Map(K, V) Maybe(T) Result(T) Record(name) Handle("crate.Type") Object(fields)`
(the type of a record literal) and `Unknown` (only appears next to an already-reported error, so it never
occurs in an `Ok` result except for tool results whose type the manifest cannot tell, e.g. `crm update … into x`).
Record names are unique in a project (OPEN_QUESTIONS Q31), so `Record(String)` identifies a record. Records, enums and
type aliases share one namespace (Q302), so `Enum(String)` identifies an enum too. `Variant(enum, variant)` is only the
type of the name a `match` branch gives (`Circle c:`): the fields of one variant, readable as `c.radius`, not a value of the
enum (Q304). A `# Type` alias never becomes a `Type`: it is replaced by what it names.

## Collection words (`sema/collections.rs`, SPEC 22.2)

`ExprKind::Coll` and `StmtKind::Coll` are typed there. `CollExpr::parts()` (in `syntax/collections.rs`) says which child
expressions run per element: those are checked in a frame where `item` is the element and, for a record, its fields are bare
names (`Implicit { var: "item", .. }`), with `PureKind::Element` so an effect is an error (Q323). `Resolution::Var { name: "item" }`
and `Resolution::Field { var: Some("item"), .. }` are therefore what the IR sees. Results: Q325; map keys: `check_map_key` (E321);
codes E320 to E328 (list items, key, order, sum, not a collection, join, map entry, variable hides a field, target is not a variable); E329 (nesting) comes from the parser.

## Effects (`sema::EffectSet`)

`{ model: bool, network: Vec<String>, local: Vec<String>, unverified: Vec<String> }`, names in first-use order.
`class()` is the highest class (`Pure < Local < Network < Model`; `Unverified` ranks like `Network` for the
"does the body exceed its declared effect" test), `is_pure()`, `merge`, `Display` = SPEC 8 summary:
`model, network(email, crm), local(log)` / `pure`. Sources: tools (their manifest effect), functions and Rust
blocks (name, declared effect; a function's own inferred set when it is larger), bindings (binding name),
built-ins `print`, `file` (`read file`), `state` (`add … to state.…`), and `model` for `ask model`.
Reading `crm.invoices` is `network(crm)`.

## What the check resolved that the AST does not say

- `## Do` blocks work on the record of the callers' loop/event (all callers must agree on the record, Q30); `BlockInfo.param` is `(first caller's variable, record)`.
- A source `crm.invoices` is `list of Invoice` (record found by the singular name, Q5); a `get`/`list` result
  is typed the same way.
- A bare field name (`amount`) resolves against the record in front of the block; `invoice.amount` against
  the variable. Constants are looked up after variables and are meant to be inlined.
- `state` in a route/command is a variable of the service's `state:` record (Q9); only one service may have one. Reading it is pure, `add … to state.…` touches `state` (Q39).
- Name order: variable, bare field, constant, tool source (needs the dot). `## Do` blocks nobody calls are checked leniently: a bare name passes if some record has such a field, otherwise it is reported (Q30).
- Narrowing keys bare fields as `var.field` (`score` and `lead.score` are one fact); re-checking a present value is fine. `is not empty` / `is some` / `guard` unwrap `maybe` values in the rest of the block.
- User-defined types (SPEC 22.1, `enums.rs`): a path resolves to a variable, a bare field, a constant, a variant, then a tool source (Q303). `match v:` takes an enum, a `maybe` or a `result`; every branch gets a `Pattern` in `Checked::patterns`; a `result` is stored by `f() into x` or matched at the call, and only a `match` may read it (Q300). Without `otherwise:` every case must be handled (E310 names the missing ones).
- `result of T` (Q200): `return v` in such a function takes a `T`; `try f(x) into h` (or `try do f(x) into h`, Q307) types `h` as `T`; calling it without `try`, or inside an expression, is an error. A crate import without binding is reported once on the import line (`E219` when a statement uses it, Q201), without `Unknown tool` or `Unused import` follow-ons.

## Handles (`handles.rs`, SPEC 22.4, Q360 to Q369)

`handle of <crate>.<Type>` (`Type::Handle("sha2.Sha256")`) is a library value AgentMark cannot look into. Where the bindings declare
the types: `Checker.handle_types` (type -> bindings that mention it), filled before anything is typed; an unknown one is E360 with a
did-you-mean. A binding item's `self` parameter (first, a handle) makes it a method (`BindingItemInfo::receiver()`), and each
parameter carries a `HandleMode` (`modes`): `handle of` reads, `mut handle of` changes, `consume handle of` uses it up.

- A statement whose first word is a variable of handle type is a method call: `binding_call(.., recv = Some(type))` looks the verb up
  among the items with that receiver (`ToolResolution.on = Some(type)`, tool word = the variable). Constructors are ordinary items.
- Ownership is `Cx.consumed` (variable -> the span that used it up): `consume` receivers and parameters, passing a handle to a function
  (`move_out`), `set` and `return`-less moves use it up; reading it again is E361 naming that line. `if`/`else` join with "may have been"
  (a branch that ends in `stop`/`return` contributes nothing), a handle from outside a loop cannot be used up inside it, `define` revives.
- Default-deny reads: `path_expr` on a handle variable is E362 unless the caller set `Cx.handle_ok` (`set`, function and binding
  arguments, `return`), so printing, interpolating, comparing, saving, sending and tracing are all refused at one place.
- E363 (no handle in `list of`/`maybe`/a record field), E365 (`mut`/`consume` outside binding parameters, `self` misplaced, a handle
  given twice to one call, a borrowed handle that is not a variable), E366 (`try … into h otherwise:` whose `otherwise` does not leave).

## Layout

`mod.rs` driver and result types · `scope.rs` registry and per-file scopes · `decls.rs` record fields,
signatures, constants · `body.rs` per-body context, on-demand body checking · `expr.rs` expressions ·
`stmt.rs` statements · `enums.rs` enums, aliases, variant values, `match` (E300-E314, W300) · `tools.rs` manifest checks · `types.rs` · `effects.rs` · `limits.rs` (SPEC 18.3) ·
`parallel.rs` (SPEC 16) · `unused.rs` (SPEC 18.2).

## Bindings from `bindings/` (CLI `check`)

A `# Import: crate x` whose `bindings/x.agent.md` (or `.agentmark/bindings/`) exists is loaded by the project
loader as a file of the project; its `# Binding` is typed like any declaration, so `sha2 sha256_digest data into h`
checks against the bound signature, takes the binding's `- effect:` (or `unverified`, listed by
`Checked::unverified_bindings()` and by `agentmark check`). This needed no extra code in sema.

## Built-in library (step 22)
`src/stdlib.rs` is the one table (`FUNCS`, `FILE_VERBS`, `MODULES`); `src/sema/stdlib.rs` types from it. `module.fn(args)` where `module` is `text regex number money date path json csv` and no imported function has that name is a library call: no `Resolution` is recorded (the IR recognises it by the table), the type is the table row's (`Num` = the type of the first argument; `Decoded` = `result of T` from the `as T` that the parser wraps in `ExprKind::Decode`). A call that can fail is `Type::Result(T)` and is accepted only directly after `try` (`Cx.std_ok`); `StmtKind::CallInto` defines its variable with the plain `T`. Steps: `file …` (touches `files`), `read input into t` (`input`). A typed read (`file read p into sales`, SPEC 23.10) records the list-of-record type in `Checked::file_reads`. Errors E340 (unknown function), E341 (result without `try`), E342 (`as T`), E343 (literal pattern, also looked up through a constant), E344/E345 (`file` verb / shape), E347 (`[file]` in tools.toml, raised by the manifest).

## User-defined tools (`user_tools.rs`, SPEC 22.5, Q380 to Q389)
`# Tool: name` is `Decl::Tool` / `Kind::Tool`. `declare_tools` (after the signatures) reports E380 (name taken by `tools.toml`, `file` or a binding) and E381 (verb twice) and gives every verb a `Sig` under `(Kind::Tool, "<tool>__<verb>")` (`user_tools::fn_name`; not in `Checked.functions`) plus `Checker.tool_verbs` (function name -> `VerbRef { file, decl, verb }`). A verb body is `BodyKey::ToolVerb`, checked like a `# Function` body with `FnCx.noun = "tool"` (E206 against the tool's `- touches:`; E386 when `-> type` is never returned; a `rust` fence is trusted and has exactly the declared effect). `callee_effect(Kind::Tool, …)` is the body's effect raised to the declared one, named after the tool. Each verb is a `BlockInfo` of `BlockKind::Tool` (`unit` = tool, `label` = verb).
- Step `tool verb a b [: c] [into x]`: `tools.rs::tool_call` falls through manifest, handle variable and binding to `Kind::Tool` -> `user_tools::user_tool_call`: verb lookup (E383), arguments checked with `check_args`, `: value` is the last argument, `field = value` is E384, `into x` on a verb without a result E385, `result of T` needs `try`. It records `ToolResolution { kind: ToolKind::User, tool, verb }` and `Resolution::Call { Function, "<tool>__<verb>" }` on the statement.
- Expression `tool.verb(a)` (also `module.tool.verb(a)`): `expr.rs::call` asks `user_tool_expr` before the library; a tool that lacks the verb leaves `text.lower(…)` to the library; a non-pure tool is E382. The call expression's `Resolution::Call` is the same function name.
- Agents: `- tools:` accepts tool names (`check_agent_headers`); statements need the listing (E208).

## v0.4 additions (SPEC 23)

- **Types from names** (`decls.rs`, `infer_type` and `match_user_type`): an `- in:` / `- out:` name without a type is matched against the records, enums and aliases the file sees, longest ending first, then by its last word (`types.rs::infer_field_type`); no answer is E230.
- **`- touches:`** (`decls.rs::touch_set`): the words become an `EffectSet` (`nothing`, `AI`, `files`, `print`, `input`, `state`, a manifest tool, a binding or crate, a `# Tool`); an unknown word is E232. A function or tool with the line is checked in `stmt.rs` (E206 "says `- touches: X`, but this step touches Y"); without it the set is computed from the body. A `## Rust` block or a Rust tool verb must have the line (E012). `effects.rs::touch_word` turns a source back into the word `check` prints.
- **Typed file read** (`stdlib.rs`): `file read p into sales` where the name is the plural of a record (or `into x: list of R`) is recorded in `Checked::file_reads`; the IR carries it in `FileCall.records`.
- Names, steps and `into`: a step's `into <name>` is the old `-> name`, so the scope code is unchanged.
