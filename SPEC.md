# AgentMark — Language Specification v0.1

AgentMark is a markup-based language for describing AI agent workflows. Source files are a strict subset of Markdown (`.agent.md`) and compile to a standalone Rust binary.

Design goals, in priority order:
1. Readable by a non-programmer (an operations manager can review an agent).
2. Every side effect is visible and guardable.
3. Compiles to one self-contained binary with no runtime dependencies on the target machine.
4. Small core, but **general**: anything buildable in Rust must be buildable from AgentMark, by escalating through three layers rather than by growing the surface syntax.

## 0. Architecture — three layers

```
Layer 3  Agent layer      # Agent, # Rule, triggers, ask model, guards       (sections 2–4)
Layer 2  Core language    # Record, # Function, # Module, # Service,
                          # Command, control flow, errors, crate bindings   (sections 13–16)
Layer 1  Escape hatch     ## Rust blocks — verbatim Rust inside any block    (section 17)
```

- Layer 3 is sugar over Layer 2. Every agent construct lowers to core constructs in the IR; the codegen never sees "agent" concepts.
- Layer 2 is a complete, if minimal, general-purpose language: typed records, functions, modules, pattern matching on `maybe`/results, explicit errors, and a binding mechanism to call any Rust crate.
- Layer 1 guarantees nothing is impossible: when the core cannot express something, the author drops to Rust for that block only. The compiler still tracks the block's declared effects.

The rule for the language designer: **a feature goes into Layer 2 only when it has appeared in Layer 1 three times.** This keeps the core small while the escape hatch absorbs the long tail.

Compiler architecture must reflect this: `agent_lower.rs` (L3→L2) is a separate pass from `codegen/rust.rs` (L2→Rust), and the test suite has separate fixtures for each pass.

---

## 1. File structure

A file is a sequence of **declarations** (parts). Each declaration starts with a level-1 heading.

```
# Agent: <name>
# Record: <name>
# Rule: <name> <Record>
```

Inside a declaration (section 23 has the full mapping):
- The paragraph directly under the heading is its **description**.
- `- key: value` bullets are **properties** (and fields).
- `## <heading>` lines start **blocks** (triggers or named actions).
- Numbered lines (`1.`, `2.`, …) are **steps**; steps under a step are indented to its text.

Rules:
- A step under a numbered item is indented to its text (3 spaces under `1.`), a line under a bullet by 2 spaces. Tabs are an error.
- Blank lines are ignored.
- Lines starting with `>` are notes for people.
- Headings are case-insensitive; identifiers are case-sensitive.
- Identifiers: letters, digits, `-`, `_`. Must start with a letter.

---

## 2. Declarations

### 2.1 Record

Describes a data shape. Fields are list items with an optional type.

```
# Record: Invoice
- customer: text
- amount: money
- due: date
- status: text
```

If type is omitted, it is inferred from the field name:
- names ending in `date`, `at`, `on` → `date`
- names ending in `amount`, `price`, `total`, `fee` → `money`
- names ending in `count`, `qty`, `number` → `int`
- otherwise → `text`

Inference failure is a **compile warning**, not an error. The compiler prints the inferred type.

### 2.2 Rule

A named, pure boolean expression over a record. Rules have no side effects and the compiler must reject any effectful statement inside one. The record is the last word of the heading (23.8).

```
# Rule: overdue Invoice
Unpaid and more than 14 days past due.

- when: today > due + 14 days
```

### 2.3 Agent

The executable unit. Properties:

| key | required | meaning |
|---|---|---|
| `model` | yes | LLM identifier, e.g. `claude`, `claude:haiku`, `local:kat-coder` |
| `tools` | no | comma-separated list of tool names the agent may use |
| `memory` | no | `none` (default) or `file` — persist state between runs |
| `timeout` | no | duration, default `5 minutes` per block execution |

An agent may only call tools listed in `tools`. Calling an unlisted tool is a compile error.

---

## 3. Blocks (triggers)

| heading | fires when |
|---|---|
| `## Every <duration>` | periodically, e.g. `Every 30 minutes` |
| `## Every day at <HH:MM>` | daily at local time |
| `## Every <weekday> at <HH:MM>` | weekly |
| `## When <tool> receives <event>` | inbound event from a tool, e.g. `When email receives message` |
| `## On start` | once when the binary starts |
| `## Do <name>` | named action, callable with `do <name>` from other blocks |

Blocks execute sequentially within an agent. Two triggers firing at once are queued, never run in parallel.

---

## 4. Statements

Statements are steps: numbered list items (`1. `). A step with steps under it ends in `:` (section 23.3).

### 4.1 Control

```
for each <var> in <source>:
for each <rule> <record> in <source>:     > filtered loop, e.g. "for each overdue invoice in crm.invoices"
if <condition>:
else:
stop                                       > end current block
do <action-name>                           > call a "## Do" block
```

### 4.2 Model

```
ask model: "<prompt>" into <var>
ask model: "<prompt>" as <Record> into <var>     > structured output, validated against Record
ask model with <var>: "<prompt>" into <var>      > pass a value as context
```

`ask model` is effectful (network). The result is `text` unless `as <Record>` is given.

### 4.3 Tool calls

```
<tool> <verb> [<target>]: "<text or data>"
```

Examples:
```
1. email send customer: "Your invoice {amount} is overdue."
2. crm update invoice: status = "reminded"
3. http post "https://api.example.com/hook": { id: invoice.id }
4. log: "Processed {customer}"
```

Available verbs are defined per tool in the tool manifest (section 7). Unknown verb is a compile error.

### 4.4 Guards

```
guard: <condition>
guard: <condition> else <statement>
```

A guard is a pure check. If it fails and no `else` is given, the current block stops and the failure is logged with the guard's source line. Guards may reference model output; this is how you constrain what the LLM produced before acting on it.

Built-in guard helpers:
- `no amounts changed in <var>` — model output contains no money values differing from input
- `<var> is not empty`
- `<var> matches <Record>`
- `<var> length < <int>`

### 4.5 Naming a value

```
<expression> into <var>
```

The only way to name a result (section 23.9); a name that already exists gets the new value.

---

## 5. Expressions

- Literals: `"text"`, `42`, `12.50 EUR`, `2026-10-13`, `14 days`, `true`
- Fields: `invoice.amount`, or bare `amount` when inside a `for each` over a record
- Interpolation inside strings: `{customer}`, `{invoice.amount}`
- Comparison: `=`, `!=`, `<`, `>`, `<=`, `>=`
- Logic: `and`, `or`, `not`
- Date/duration arithmetic: `due + 14 days`, `today - 1 month`
- Built-ins: `today`, `now`, `count of <source>`

Units are real types. `12 EUR + 3 days` is a compile error. `money + money` requires matching currency.

---

## 6. Types

| type | Rust representation |
|---|---|
| `text` | `String` |
| `int` | `i64` |
| `money` | `Money { minor: i64, currency: Currency }` (integer minor units, no floats) |
| `date` | `chrono::NaiveDate` |
| `datetime` | `chrono::DateTime<Utc>` |
| `duration` | `chrono::Duration` |
| `bool` | `bool` |
| `list of <T>` | `Vec<T>` |
| `maybe <T>` | `Option<T>` |

Any value that can be absent must be declared `maybe`. There is no null.

---

## 7. Tool manifest

Tools are declared in `tools.toml` next to the source file. The compiler reads it to validate verbs and generate typed bindings.

```toml
[email]
verbs = ["send"]
targets = ["customer", "address"]
effect = "network"

[crm]
verbs = ["list", "get", "update"]
sources = ["invoices", "leads", "tickets"]
effect = "network"

[http]
verbs = ["get", "post"]
effect = "network"

[log]
verbs = []
effect = "local"
```

The generated Rust code defines one trait per tool. The runtime ships a `MockTools` implementation (records calls, returns fixture data) and the user provides real implementations.

---

## 8. Effects

Every statement has an effect class: `pure`, `local`, `network`, `model`. People read it as what the statement touches (section 23.7): `nothing`, `files`, `AI`, the tools by name.

- `# Rule` bodies and `guard` conditions must touch nothing.
- The compiler emits an **effect summary** per block at the top of the generated file, e.g. `// Effects: model, network(email, crm)`.
- `agentmark check` prints what each part touches (`invoice-chaser / Every day at 09:00: touches AI, crm, email, log`) so a reviewer can see what an agent can touch without reading the code.

---

## 9. Compilation

```
agentmark check <file>                       validate, print effect summary, inferred types, unverified bindings
agentmark build <file> --target rust -o <dir> emit a Cargo project (auto-binds imported crates first)
agentmark run <file> --mock                  build + run against MockTools with fixtures
agentmark trace <file> --mock                run and print the tool-call trace (used by tests)
agentmark bind <crate> [--force] [--all]     generate or refresh a library binding (section 15)
```

Pipeline: `source → lexer → parser → AST → semantic check (types, effects, tool manifest) → IR → Rust codegen`.

The IR is target-independent. Adding a target means adding one `codegen/<target>.rs` file. Only Rust is implemented in v0.1.

### 9.1 Generated Rust project layout

```
out/
  Cargo.toml            depends on agentmark-rt (runtime crate) + tokio + chrono + serde
  src/main.rs           CLI: --once, --schedule, --mock, --fixtures <dir>
  src/agent_<name>.rs   one file per agent; one async fn per block
  src/records.rs        structs for every # Record, with serde derives
  src/rules.rs          one fn per # Rule
  src/tools.rs          traits generated from tools.toml
```

### 9.2 Runtime crate `agentmark-rt`

Lives in the same repository. Provides:
- `Money`, `Currency`
- scheduler (`Every`, `EveryDayAt`) on tokio
- `ModelClient` trait + Anthropic implementation + `MockModel`
- `MockTools` + fixture loader (JSON files in `fixtures/`)
- `Trace` recorder: every tool/model call appended as one JSON line

---

## 10. Error messages

Errors must point at a line and say what was expected in plain language:

```
examples/invoice-chaser.agent.md:12
  - email send custmer: "..."
                ^^^^^^^
  Unknown target "custmer" for tool email. Did you mean "customer"?
```

Never print a Rust compiler error to the user for a mistake in AgentMark source. If generated Rust fails to compile, that is a compiler bug and must be reported as such.

---

## 11. Testing contract

For every file in `examples/`, there is a matching `fixtures/<name>/` folder containing:
- input data per tool source (e.g. `crm.invoices.json`)
- `model.json` — canned responses for `ask model`, keyed by prompt prefix
- `expected.trace.jsonl` — the exact sequence of tool and model calls the agent must make

`cargo test` in the compiler repo must: build each example, run it with `--mock --fixtures`, and compare the produced trace with `expected.trace.jsonl`. Tests compare behavior, not generated source text.

---

## 13. Core language — functions and modules (Layer 2)

### 13.1 Function

```
# Function: net_amount
The invoice amount after taking off VAT.

- in: invoice, vat: int
- out: money
- touches: nothing
- steps:
  1. invoice.amount into gross
  2. return gross - gross * vat / 100
```

- `touches` is optional (23.7): without it the checker works out what the function touches; written, it is a promise the checker verifies.
- Functions are called as `net_amount(invoice, 19)` in expressions, or `do net_amount(invoice, 19) into n` as a step.
- Recursion is allowed. Closures are not (v0.1).

### 13.2 Module and import

One file = one module. Files in subfolders are nested modules.

```
# Uses
- [billing rules](./billing/rules.agent.md) as billing

# Import: crate serde_json
# Import: crate reqwest as http_client
```

Imported names are referenced as `billing.overdue`. Crate imports make the crate available to `## Rust` blocks and to bindings (section 15).

### 13.3 Service and Command

Two more top-level declarations turn AgentMark into a general application language:

```
# Service: api
Serves invoices over HTTP.

- listen: 0.0.0.0:8080
- state: AppState

## GET /invoices/{id}
1. do crm get invoice id into inv
2. guard: inv is not empty else respond 404
3. respond 200 inv

## POST /invoices
1. read body as Invoice into inv
2. do crm update invoice: inv
3. respond 201 inv
```

```
# Command: import-csv
Loads invoices from a CSV file into the CRM.

- in: path: text, dry_run: bool = false
- steps:
  1. try read file path as list of Invoice into invoices
  2. for each invoice in invoices:
     1. if not dry_run:
        1. crm update invoice: invoice
  3. print: "Imported {count of invoices} rows"
```

`Service` lowers to an axum router; `Command` lowers to a clap subcommand. Both are Layer 2 — no model involved unless the author adds `ask model`.

## 14. Errors

There is no exception mechanism. Any statement that can fail is written with `try`:

```
1. try http get "https://api.example.com/rates" into rates
   otherwise:
   1. log: "Rates unavailable, using cached"
   2. cached_rates into rates
```

Without `otherwise`, a failed `try` stops the current block and records the error in the trace. A function whose `out` is `result of <T>` returns `Result<T, AgentError>` in Rust; callers must use `try`.

`maybe` values are unwrapped the same way:

```
1. if invoice.score is some s:
   1. log: "{s}"
   else:
   1. log: "unscored"
```

## 15. Library access — automatic bindings

**Principle: using a library must never require writing target-language code.** A user who writes `# Import: crate printpdf` gets `pdf render doc into bytes` with nothing else to do. The compiler generates and caches the binding itself.

### 15.1 Flow

```
# Import: crate printpdf        → compiler checks .agentmark/bindings/printpdf.agent.md
                                   missing or stale (crate version changed)?
                                   → run the binder (15.2), write the file, continue
```

Generated bindings are ordinary Layer-2 files (section 15.4) committed to the repo, so they are reviewable, diffable, and editable by hand. `agentmark bind <crate>` runs the binder explicitly; `agentmark bind --all` refreshes every cached binding.

### 15.2 The binder

The binder is **target-agnostic by design**: `binder/<target>/` reads the target's library metadata and `binder/core` turns it into AgentMark declarations. For Rust the metadata source is rustdoc JSON (`cargo rustdoc -- -Z unstable-options --output-format json`, or `cargo doc` on stable where available); a future TypeScript target would read `.d.ts` files. Only Rust is implemented in v0.1, but `binder/core` must not contain anything Rust-specific.

Three tiers, applied in order to every public function/method of the crate:

| tier | condition | result |
|---|---|---|
| **direct** | every parameter and the return type map 1:1 onto AgentMark types (section 6), no generics, no closures | binding written by rules, no model call |
| **assisted** | builder patterns, generics with a single obvious instantiation, closures with one argument, `Result`/`Option` wrapping | binder calls `ModelClient` with the signature, the crate's docs for that item, and the AgentMark style guide; the model returns the Rust body of the binding; binder type-checks it against the declared signature before accepting |
| **skipped** | traits with associated types, lifetimes in the public API, unsafe functions, anything the assisted tier fails to type-check | listed in the binding file under `## Not bound` with the reason, so the user knows to use a `## Rust` block for those |
| **on-demand** (v0.2) | the user writes a plain statement against a skipped item, e.g. `pdf render doc to "invoice.pdf"` | at compile time the binder asks `ModelClient` to write the Rust body from the user's statement plus the item's rustdoc, type-checks it, saves it to `bindings/` as `unverified`, and continues the build. The user never sees Rust; `check` shows the line to approve. If the model's code fails type-check twice, the build stops with a clear error naming the item — never silently wrong code. |

The binder never silently drops an item: everything is either bound or listed as not bound. With the on-demand tier, "not bound" becomes "not bound yet".

### 15.3 Effects and trust

Every generated binding is born with `- effect: unverified`. The binder proposes a class (`pure` / `local` / `network`) from heuristics (uses `std::fs` → local, uses `reqwest`/`tokio::net` → network, otherwise pure) and writes it as `- proposed-effect: network`. 

- `agentmark check` lists every `unverified` binding and refuses to **build** a release binary while any remain; `--allow-unverified` overrides for development.
- The user approves by changing the line to `- effect: network` (or whatever is correct). One line, once per crate version.
- A binding approved for version X is re-marked `unverified` when the crate version changes, because the API may have changed.

This keeps the guarantee from section 8: a reviewer can still see exactly what an agent can touch, and no library enters the trusted set without one explicit human decision.

### 15.4 Binding file format

A binding is a Layer-2 declaration. Hand-written and generated ones look identical:

````
# Binding: pdf
- crate: printpdf
- effect: local

## render(doc: Document) -> bytes
Renders the document as a PDF.

  ```rust
  let mut pdf = printpdf::PdfDocument::new(&doc.title, Mm(210.0), Mm(297.0), "L1");
  // ...
  Ok(pdf.save_to_bytes()?)
  ```
````

A binding is a Layer-2 declaration whose bodies are Layer-1 Rust. After declaring it, `pdf render doc into bytes` is an ordinary tool-style statement with a checked effect. This is how the language reaches "anything Rust can do" without the core growing: the binder wraps it once, everyone uses it everywhere.

Generated files carry a header the binder uses for staleness, as the description under the heading (23.2):

```
# Binding: printpdf
generated by agentmark bind — printpdf 0.7.0 — 2026-10-13
edit freely; re-run `agentmark bind printpdf --force` to regenerate
```

### 15.5 Sharing

Bindings are plain files, so a community registry is just a git repo of `bindings/*.agent.md`. `agentmark bind <crate>` checks a configured registry first and only runs the binder when nothing is found. Out of scope for v0.1, but the file format must not need to change to support it.

## 16. Concurrency

v0.1 keeps blocks sequential, but the IR must already carry an effect graph so that v0.2 can run independent pure/local statements concurrently. Codegen uses `tokio` from the start; all generated fns are `async`.

Explicit opt-in is available now:

```
1. parallel:
   1. do fetch_rates into rates
   2. do fetch_invoices into invoices
```

Statements inside `parallel` must not write the same variable. The compiler checks this.

## 17. Escape hatch — `## Rust` blocks (Layer 1)

Any agent, service, command, or function may contain a block whose heading is `## Rust: <name>` and whose body is a fenced `rust` code block. Variables in scope are available by name; the block must end with a value of the declared type.

````
## Rust: hash_payload -> text
- in: payload: text
- touches: nothing

```rust
use sha2::{Digest, Sha256};
format!("{:x}", Sha256::digest(payload.as_bytes()))
```
````

Rules:
- The `- touches:` line is mandatory and trusted (the compiler cannot verify arbitrary Rust). `agentmark check` lists every Rust block in its summary so reviewers see where trust is being extended.
- Generated code wraps the block in its own `fn` with `#[allow(unused)]`, so a mistake inside it produces a Rust error pointing at the `.agent.md` line via `#[track_caller]`-style source mapping comments.
- Rust blocks may `use` any crate declared with `# Import: crate`.

## 18. Code organization and readability

AgentMark is Markdown so that a human can read a program the way they read a document. The language enforces that, rather than hoping for it.

### 18.1 Project layout

A project is a folder. The compiler reads `agentmark.toml` at its root; everything else is convention the `lint` command checks:

```
my-project/
  agentmark.toml          name, version, targets, lint settings
  tools.toml              tool manifest (section 7)
  main.agent.md           entry point: `# Program` (title, description, `- start:`) and `# Uses`
  records/                one # Record per file:  records/invoice.agent.md
  rules/                  one # Rule per file:     rules/overdue.agent.md
  agents/                 one # Agent per file:    agents/invoice-chaser.agent.md
  functions/              one # Function per file
  bindings/               generated by the binder (section 15); reviewed, committed
  fixtures/               test data and expected traces
  docs/                   optional prose; not compiled
```

`# Program` and `# Uses` are the only parts of `main.agent.md` (section 23.4):

```
# Program: billing
Chases unpaid invoices and serves them over HTTP.

# Uses
- [invoice-chaser agent](./agents/invoice-chaser.agent.md)
- [api service](./services/api.agent.md)
```

Each file it uses brings its `# Agent`, `# Service`, or `# Command`. The binary starts all agents and services; commands become subcommands. A project with no `# Program` compiles to a library, not a binary.

### 18.2 Imports

```
# Uses
- [Invoice](./records/invoice.agent.md)
- [overdue rule](./rules/overdue.agent.md)
- [every rule](./rules/*.agent.md)
- [billing](./billing/) as billing

# Import: crate printpdf
```

A glob uses every file it matches; a folder is a module, its names become `billing.<name>`; `# Import: crate` is a library (section 15).

- A file sees only what it imports. There is no global scope.
- Import cycles are a compile error, with the cycle printed.
- `agentmark check` reports unused imports and unused declarations.

### 18.3 Hard limits (compile errors)

These cannot be disabled, because they are what keep a file readable:

| rule | limit |
|---|---|
| nesting depth of statements | 3 (`for each` → `if` → `if` is the maximum) |
| statements per block | 25 |
| declarations per file | 1 of `# Agent` / `# Service` / `# Command`; records and rules may share a file only in `records/` and `rules/` |
| line length | 120 characters |
| block body with no statements | error (empty blocks hide intent) |

When a block exceeds the limit, the error message suggests the fix: "Move lines 14–31 into a `## Do <name>` block and call it with `do <name>`."

### 18.4 Soft rules (`agentmark lint`)

Warnings by default; `agentmark.toml` can promote any to errors (`[lint] deny = ["…"]`):

- `long-file` — file over 150 lines
- `magic-value` — a literal money, duration, or URL used inline more than once; suggest a `# Constant`
- `unnamed-guard` — a guard whose condition is longer than 60 chars that nothing explains: no description on its block or part, no `> note` line above it
- `duplicate-rule` — two `# Rule`s with identical `when:` expressions
- `prompt-in-loop` — `ask model` inside `for each` without a `guard` on its result
- `missing-doc` — an `# Agent`, `# Service`, or `# Command` with no description paragraph under its heading
- `vague-name` — identifiers `data`, `tmp`, `x`, `result`, `thing`

### 18.5 Constants

To support `magic-value`:

```
# Constant: grace_period
- value: 14 days

# Constant: sales_inbox
- value: "sales@optimotion.dev"
```

Constants are pure, imported like anything else, and inlined at compile time.

### 18.6 Formatter

`agentmark fmt` rewrites a file into canonical form: `# Program`, then `# Uses` (sorted), then crate imports, then the other parts; one blank line between declarations; descriptions under their headings; properties before blocks; steps numbered 1, 2, 3 at every level and indented to their parent's text; trailing whitespace removed (section 23). `fmt --check` fails CI if a file is not canonical. There are no formatting options — one style, so every AgentMark file in the world looks the same.

### 18.7 Descriptions

The paragraph directly under a heading is its documentation (section 23.2). `agentmark doc` collects these into a single readable page per project, so the explanation an operations manager reads is the same text the author wrote next to the code — they cannot drift apart.

```
# Agent: invoice-chaser
Chases unpaid invoices once a day. Never changes amounts; only sends reminders.
```

## 19. Model providers

`- model:` is a driver string, never a vendor lock. The runtime's `ModelClient` trait has one implementation per provider; the language is unaware of which is in use.

| string | implementation |
|---|---|
| `claude`, `claude:<model>` | Anthropic Messages API |
| `openai:<model>` | OpenAI Chat Completions |
| `local:<model>` | any OpenAI-compatible local server (Ollama, LM Studio, vLLM); base URL from `agentmark.toml` |
| `azure:<deployment>`, `bedrock:<model>` | enterprise routes, v0.2 |
| `mock` | fixture-driven, for tests |

`agentmark.toml` may map environments: `[models] production = "claude" / test = "mock" / dev = "local:kat-coder"`. The binder and the on-demand tier use the same trait, so a company that forbids external APIs can run the whole toolchain on a local model.

## 20. v0.2 scope — editor and tooling

Built only after v0.1 passes all tests. Nothing here changes the language.

1. **On-demand binding tier** (15.2, last row).
2. **Model providers** `openai`, `local`, `azure`, `bedrock` (19).
3. **Language server** (`agentmark-lsp`, in the same workspace, reusing the compiler as a library): diagnostics with the compiler's messages, completion for records/rules/tools/bindings, go-to-definition across imports, hover showing a block's effect summary, format-on-save via `fmt`, code actions for every lint rule that has a mechanical fix. Optional model-backed actions (`explain this block`, `suggest a guard`) go through `ModelClient` and are off unless `agentmark.toml` enables them.
4. **VS Code extension** (`editors/vscode/`): TextMate grammar for `.agent.md`, LSP client, `agentmark.toml` schema, and a one-click "show what this agent can touch" panel reading `check --json`. Publishable to the Marketplace.
5. **tree-sitter grammar** (`editors/tree-sitter-agentmark/`) so Zed, Neovim, and Helix get highlighting from the same source.
6. **AI-assistant compatibility**: `docs/reference.md` must stay the single file an LLM needs; ship a `CLAUDE.md` and `.cursorrules` template in `templates/` that point to it.

## 21. Out of scope for v0.2

- Automatic parallelism (explicit `parallel:` only)
- Multiple agents talking to each other
- Targets other than Rust
- Closures and generics in the core language

## 22. v0.3 — a general-purpose Markdown language

From v0.3 AgentMark is first a general-purpose programming language written as Markdown; agents (Layer 3) are one feature built on it. Everything in this section is Layer 2. The layer rule (section 0) still holds: a construct below enters the language only because one of the v0.3 examples (22.7) needs it; the examples contain no `## Rust` block.

### 22.1 User-defined types

```
# Enum: Shape
- Circle: radius: int
- Rect: width: int, height: int
- Empty

# Type: Totals
- is: map of text to money
```

- An `# Enum` lists variants; a variant may carry named fields. Values are written `Circle { radius: 3 }` or, for a field-less variant, `Empty`. Variant names are unique within the file's scope; a clash is resolved by qualifying, `Shape.Empty`.
- A `# Type` is an alias; it is interchangeable with what it names.
- `match` branches over an enum, a `maybe`, or a `result`:

```
1. match shape:
   - Circle c:
     1. print: "circle {c.radius}"
   - Rect r:
     1. print: "rect {r.width}x{r.height}"
   - Empty:
     1. print: "nothing"

2. match found:          > maybe T
   - some row:
     1. print: "{row.id}"
   - none:
     1. stop

3. match parsed:         > result of T
   - ok value:
     1. value into total
   - error problem:
     1. print: "{problem}"
```

Records are built the same way as variants: `Sale { region: "EU", amount: 120.00 EUR }`. Every field that is not `maybe` must be given.

A `maybe` value is written `some x` or `none`. In a branch the name after the variant is optional (`- error:` when the problem is not used).

Every variant must be handled or a `- otherwise:` branch given; a missing variant is a compile error naming it. `match` is a statement; it is pure unless its branches are not.

### 22.2 Collection words

Collections are `list of T` and `map of K to V`. Everything below is an expression unless shown as a statement. Inside the expression after `where`, `by`, `to` and `of`, the fields of the current element are in scope by their bare names (as in a `for each` over a record), and the element itself is `item`.

| form | result |
|---|---|
| `list of 1, 2, 3` · `empty list of text` | list literal |
| `map of "EU" to 1, "US" to 2` · `empty map of text to int` | map literal |
| `rows where region = "EU"` | filtered list |
| `map rows to amount` | list of the expression's values |
| `sort rows by amount` · `sort rows by amount descending` | sorted list (stable) |
| `group rows by region` | `map of K to list of T` |
| `sum of amount in rows` · `sum of numbers` | number or money (money of one currency; empty → 0) |
| `count of rows` · `count of rows where …` | int |
| `min of amount in rows` · `max of …` | `maybe` of the element type |
| `first of rows` · `last of rows` | `maybe T` |
| `take 10 from rows` · `skip 2 from rows` | list |
| `unique rows` · `unique region in rows` | list, first occurrence kept |
| `join names with ", "` | text |
| `totals at "EU"` | `maybe V` |
| `keys of totals` · `values of totals` | list (keys sorted) |

Statements: `add x to list`, `put v at k in m`, `remove k from m`, and loops `for each row in rows with index i:` and `for each region, total in totals:` (map entries in key order). Collection words are pure; they never take a function value (closures stay out of scope, section 21).

### 22.3 Built-in library

The library is part of the language, not a binding; every function has a fixed effect class shown in `check`. Pure functions are called as `module.name(arguments)`:

| module | functions (all pure) |
|---|---|
| `text` | `split(t, sep)`, `lines(t)`, `trim(t)`, `lower(t)`, `upper(t)`, `replace(t, from, to)`, `contains(t, part)`, `starts with(t, p)` (spelled `starts_with`), `ends_with`, `length(t)`, `pad(t, width)`, `repeat(t, n)` |
| `regex` | `matches(t, pattern)`, `find(t, pattern)` → `maybe text`, `captures(t, pattern)` → `maybe list of text`, `replace(t, pattern, with)` (every match) |
| `number` | `round(x, places)`, `parse(t)` → `result of int`, `min(a, b)`, `max(a, b)`, `abs(x)`, `format(x, places)` |
| `money` | `parse(t)` → `result of money`, `format(m)` |
| `date` | `parse(t, pattern)` → `result of date`, `format(d, pattern)`, `days between(a, b)` (spelled `days_between`), `today` stays a built-in |
| `path` | `join(a, b)`, `name(p)`, `stem(p)`, `extension(p)`, `parent(p)` |
| `json` | `parse(t) as T` → `result of T`, `encode(v)` → text |
| `csv` | `parse(t) as list of T` → `result of list of T` (header row maps to field names), `encode(rows)` → text |

A function that returns `result of T` (such as `csv.parse` or `date.parse`) is used in one of two ways: `try <call> into var` (the `do` of section 14, `try do <call> into var`, is accepted and means the same) with an optional `otherwise:` line and steps (section 14), after which `var` has type `T`; or `match <call>:` with `ok`/`error` branches. `if <expr> is some x:` works on any `maybe` expression, including a call.

Files are a built-in tool `file` with effect `local`; it needs no `tools.toml` entry:

```
file read path into content              > result of text; use try (23.10: a list of records reads CSV)
file write path: content
file append path: line
file list folder into names              > list of text, sorted
file rename old to new
file exists path into present            > bool
file delete path
```

Every `file` call is traced (`{"kind":"file","verb":…,"path":…}`) so tests see what a program touched. Reading standard input is `read input into text`; `print:` writes a line to standard output.

### 22.4 Opaque library types

When the binder (section 15) meets a Rust type that has no AgentMark equivalent, it no longer skips the API around it: it declares the type as `handle of <crate>.<Type>` and binds its constructors and methods.

```
1. sha2 sha256_new into hasher           > hasher: handle of sha2.Sha256
2. hasher update data                    > &mut self method, data: bytes (text accepted, Q151)
3. hasher finalize into digest           > self method: consumes hasher
```

- A handle can be stored in variables and passed to functions; it cannot be printed, compared, put in a record that is serialized, or traced.
- A method that takes `self` by value consumes the handle; using it afterwards is a compile error that names the line where it was consumed.
- A method call is an ordinary tool-style statement `<handle> <method> <arguments> [into var]` with the binding's checked effect.
- A handle owns whatever the Rust value borrows: types with lifetimes, and generic types whose parameters have a single instantiation in the crate as configured (for example `sqlx::query` with the `sqlite` feature, `DB = Sqlite`), are bound as handles through the assisted tier instead of being skipped. Rust integers wider than `int` (`u64`) convert with a checked conversion that fails the call on overflow.
- Crate features come from `agentmark.toml`: `[crates] sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio"] }`. The binder and the generated `Cargo.toml` both use them; without an entry the crate's defaults apply.
- Target: for `sha2`, at least half of the items that v0.2 listed under `## Not bound` become bound.

### 22.5 User-defined tools

A tool can be written in AgentMark itself:

```
# Tool: slugger
- touches: nothing

## slug(title: text) -> text
1. text.lower(title) into lowered
2. return regex.replace(lowered, "[^a-z0-9]+", "-")
```

Each `## verb(params) -> type` block is a verb; its body is numbered AgentMark steps or a fenced `rust` block (Layer 1). The tool is called like any tool, `slugger slug title into s`, or as an expression `slugger.slug(title)` when it touches nothing. A written `- touches:` is checked against the body exactly like a `# Function` (and is required when a verb is Rust). `tools.toml` and `rust/connect.rs` remain for tools implemented outside the program (a real CRM, an email provider); a project that only uses `# Tool` declarations and the built-in `file` tool needs neither.

### 22.6 Programs

A program (section 23.4) whose only runnable part is a single `# Command` makes that command the binary's default: `my-tool <args>` runs it without a subcommand name. Command arguments are positional in the order declared, or `--name value`.

### 22.7 v0.3 examples and testing contract

Five examples, none an agent, none containing `## Rust`:

| example | exercises |
|---|---|
| `06-sales-report` | CSV parse, `group by`, `sum of`, `sort … descending`, money, writes a report file |
| `07-file-renamer` | `file list`, regex captures, `date.format`, `file rename`, a `--dry-run` flag |
| `08-notes-api` | `# Service` REST API over SQLite through the binder (`sqlx` handles), JSON bodies, `match` on results |
| `09-markdown-to-html` | text and regex functions, an `# Enum` for block kinds, `match`, string building |
| `10-log-analyzer` | `lines`, regex captures, `group by`, `count of`, `take … from`, top-N report |

A non-agent fixture folder holds `run.json` (`{"args": […]}`, optional `"stdin"`), an `input/` folder copied into a fresh working directory before the run, and the expected results: `expected.stdout`, an optional `expected/` folder compared file by file with the working directory after the run (the working directory must then hold exactly those files), and an optional `expected.trace.jsonl`. Services use `requests.json` as in v0.2. A variant folder under `fixtures/<name>/variants/<case>/` holds only what differs from its parent; anything it lacks (`input/`, `run.json`, …) is taken from the parent folder. For services, each entry of `requests.json` may carry `expect_status` and `expect_body`; a mismatch fails the test.

### 22.8 Out of scope for v0.3

- Closures, first-class functions, user generics.
- Async user code (handles to async library APIs are awaited by the generated code).
- Operator overloading; every operator keeps the meaning of section 5.

## 23. v0.4 — plain sentences, real Markdown

From v0.4 every line of a program reads out loud as a sentence, a rendered `.agent.md` file reads like a well-written document, and every Markdown element means in AgentMark what it means in ordinary Markdown. Nothing changes in what a program does: only its syntax and the human-facing output of `check` and `doc`. Where this section and an earlier one disagree, this section wins; the earlier sections show the v0.4 syntax in their samples.

### 23.1 Markdown element → meaning

| Markdown | meaning in AgentMark |
|---|---|
| `#` heading | a part: `Program`, `Uses`, `Record`, `Enum`, `Type`, `Rule`, `Function`, `Command`, `Agent`, `Service`, `Tool`, `Binding`, `Constant` |
| `##` heading | a part of a part: an agent's trigger or `Do` block, a service route, a tool verb, a `Rust` block |
| paragraph directly under a heading | the description of that part |
| `-` bullet list | properties and fields (order does not matter) |
| `1.` numbered list | steps (order matters) |
| `[name](target)` link | a file the program uses, or the command it starts with |
| fenced ```` ```rust ```` block | Rust code (Layer 1) |

Bold, italic and the `*` / `+` bullet markers carry no meaning; a `*` or `+` bullet is read as `-` and `fmt` writes `-`. A `>` line is a note for people (23.12).

### 23.2 Descriptions

The description of a part is the paragraph directly under its heading, before its first list:

```
# Rule: big Sale
A sale of 100 EUR or more.

- when: amount >= 100 EUR
```

A description may run over several lines and several paragraphs. `agentmark doc`, the `missing-doc` lint and the language server read it from there. A `>` line directly above a heading is an error (23.13).

### 23.3 Steps

Every list of steps is numbered: the steps of a command and a function (under `- steps:`), of an agent trigger or `## Do` block, of a service route and of a tool verb. The steps under `for each`, `if`, `else`, `otherwise`, `match` branches and `parallel` are numbered too.

```
- steps:
  1. try file read path into sales
     otherwise:
     1. print: "Could not read {path}"
     2. stop
  2. for each entry in region_totals(sales):
     1. print: "{entry.region}: {entry.revenue}"
```

- A line with steps under it ends in `:`. The steps sit one level deeper: under a numbered item they start at its text (3 spaces under `1.` to `9.`, 4 under `10.` to `99.`), under a bullet 2 spaces deeper.
- `else:` and `otherwise:` are plain lines inside the `if` or `try` step, at the indentation of its steps; the steps after them belong to them:

```
1. if photo_stamp(name) is some stamp:
   1. print: "{name} -> {stamp}"
   else:
   1. print: "skipped {name}"
```

- The branches of a `match` are bullets (their order does not matter); the steps of a branch are numbered:

```
1. match block:
   - Heading h:
     1. return "<h{h.level}>{h.content}</h{h.level}>"
   - Blank:
     1. return ""
```

- The numbers are for people: `1.` on every line works too, and `fmt` writes 1, 2, 3, … at every level and fixes the indentation.
- A bullet where a step belongs is an error (E070), and so is a numbered item where a property belongs (E071).

### 23.4 Program and Uses

The program's title, description and starting point are the first part of its entry file (`main.agent.md` in a folder project, the file itself in a single-file program):

```
# Program: sales
Reads a sales file and prints how much it brought in, region by region.

- start: [sales-report](#command-sales-report)
```

- `start:` names the command the program starts with: a link to the command's heading (GitHub's anchor, `#command-<name>`), a link to the file that declares it (`./commands/sales-report.agent.md`), or the plain name. The checker verifies that it is a command of the program. `fmt` writes the link form.
- With exactly one command, `start:` can be left out. A program's agents and services need no `start:`: they all run.
- What runs: every agent, service and command of the entry file and of the files its `# Uses` lists. The program starts without a subcommand name when it has exactly one command and no agent or service (22.6); otherwise its commands are subcommands.
- `# Program` must be the first heading of the entry file, and only the entry file has one (E072). A project without `# Program` is a library.

The files a file uses are listed in one `# Uses` section at its top (after `# Program`):

```
# Uses
- [constants](../constants.agent.md)
- [Sale](../records/sale.agent.md)
```

- The link text is a label for people; the path is what is used. Paths are relative to the file, so every link is clickable on GitHub.
- A folder or a glob is a path too: `- [every rule](./rules/*.agent.md)`. A used file or folder gets a module name by writing `as <name>` after the link: `- [billing rules](./billing/) as billing`.
- Libraries keep their heading: `# Import: crate sha2` (23.14).

### 23.5 Properties of functions, commands and tools

```
# Function: region_totals
Adds up the sales of every region, biggest revenue first.

- in: sales
- out: region totals
- touches: nothing
- steps:
  1. group sales by region into by_region
  2. …
```

- `- in:` lists the inputs, `- out:` the result, `- steps:` the steps. Commands use `- in:` exactly like functions (with defaults: `- in: path: text, dry_run: bool = false`).
- `- touches:` (23.7) is optional on a function and a tool.

### 23.6 Types from names

In `- in:` and `- out:` a name alone is enough when it says what it is:

| written | means |
|---|---|
| `in: sales` | `sales: list of Sale` (a record `Sale` exists; `sales` is its plural) |
| `in: invoice` | `invoice: Invoice` |
| `in: old sales, new sales` | `old_sales: list of Sale`, `new_sales: list of Sale` |
| `out: region totals` | `list of RegionTotal` |
| `in: due_date` | `due_date: date` (the last-word table of 2.1) |
| `in: path: text` | as written |

- The words of a name are matched against the records, enums and type aliases the file can see, longest ending first (`region totals` → `RegionTotal`), ignoring case and `_`. The words before the matched ones are part of the name.
- Plurals are regular English only: `+s`, `+es`, `y` → `ies`. A plural means `list of`. Irregular plurals need the type written.
- No match: the last word decides as for record fields (2.1): `date` / `at` / `on` → `date`, `amount` / `price` / `total` / `fee` → `money`, `count` / `qty` / `number` → `int`.
- Still unclear: error E230, "Could not tell what `vat` is. Write its type: `vat: <type>`."
- An input written `name: type` is never guessed.

### 23.7 What a part touches

`- touches:` replaces `- effect:`. The words "effect", "pure", "local", "network" and "model" are no longer part of the syntax or of the reports.

| written | means |
|---|---|
| `- touches: nothing` | reads and computes only (was `pure`) |
| `- touches: files` | the `file` tool |
| `- touches: print`, `input` | standard output, standard input |
| `- touches: AI` | asks a model |
| `- touches: crm, email` | the tools, libraries or `state` named |

- On a `# Function` and a `# Tool` the line is optional: without it the checker works out what the part touches. Written, it is a checked promise: a step that touches something not on the list is an error (E206).
- A `## Rust` block, which the checker cannot read, must say what it touches; the line is trusted, as `- effect:` was.
- Inside the compiler every part still has its effect class (`pure` < `local` < `network` < `model`, 8); the IR and `check --json` keep it.
- `check` and `doc` say it the same way, one line per part:

```
region_totals: touches nothing
sales-report: touches files, print
invoice-chaser / Every day at 09:00: touches AI, crm, email, log
```

- `AI` comes first, then the rest in alphabetical order; the `file` tool is written `files`.
- Libraries (`# Binding`) keep `- effect:` and `- proposed-effect:` and `tools.toml` keeps `effect =` (OPEN_QUESTIONS Q430).

### 23.8 Rules name their record in the heading

```
# Rule: big Sale
A sale of 100 EUR or more.

- when: amount >= 100 EUR
```

The last word of the heading is the record, the words before it the rule's name (joined with `_`). The rule then reads like its use: `for each big sale in sales:`. `- for:` is gone.

### 23.9 Naming a result: `into`

`into <name>` at the end of a step names its result. It is the only way; `set` and `->` are gone.

| v0.3 | v0.4 |
|---|---|
| `set by_region = group sales by region` | `group sales by region into by_region` |
| `set totals = empty list of RegionTotal` | `empty list of RegionTotal into totals` |
| `file read path -> content` | `file read path into content` |
| `ask model with invoice: "…" -> reminder` | `ask model with invoice: "…" into reminder` |
| `slugger slug title -> s` | `slugger slug title into s` |
| `do net_amount(invoice, 19) -> n` | `do net_amount(invoice, 19) into n` |
| `read body as Invoice -> inv` | `read body as Invoice into inv` |

- `<value> into <name>` with a name that already exists gives it a new value, as `set` did (OPEN_QUESTIONS Q431).
- `into` is a reserved word.
- `->` stays in signatures, where it means "returns": `## slug(title: text) -> text`, `## Rust: fingerprint -> text`, binding items.

### 23.10 Reading a file into records

The name after `into` decides how a file is read:

```
1. try file read path into sales
   otherwise:
   1. print: "Could not read {path}"
   2. stop
```

- The name gets its type from the records the file can see, as in 23.6 (`sales` → `list of Sale`), or from a type written after it: `into content: text`.
- A `list of` a record: the file is CSV and its header row names the fields (as `csv.parse`). A file whose name does not end in `.csv` (any case) is a read failure.
- Anything else, and a name that matches no record, is the raw text, as before.
- A file that cannot be read and a file that is not valid CSV go to the same `otherwise`.
- `csv.parse(…)` stays in the library.

### 23.11 Adding to a list: `add`

```
1. add RegionTotal to totals:
   - region
   - orders: count of region_sales
   - revenue: sum of amount in region_sales
```

- `add <value> to <list>` replaces `append`.
- `add <Record> to <list>:` builds the record from the bullets under it: `name: value`, as in a record definition, and a bare `name` takes the value of that name. An enum variant with fields is built the same way.
- `fmt` writes `add X { … } to list` in this form. Elsewhere a record is still written `Sale { region: "EU", amount: 120.00 EUR }` (OPEN_QUESTIONS Q432).

### 23.12 Notes

A `>` line that is not directly above a heading is a note for people: `fmt` keeps it, everything else ignores it (OPEN_QUESTIONS Q433). The examples do not use notes.

### 23.13 Removed syntax

There is no backward compatibility. Each removed form is an error that shows the new form:

| code | written | message |
|---|---|---|
| E060 | `# Run`, `# Start` | `# Run` was removed. Start the entry file with `# Program: <name>`; name the command it starts with in `- start: <command>`. |
| E061 | `## Run` | `## Run` was removed. Write the command's steps under `- steps:` as a numbered list. |
| E062 | `- args:` | `args:` was renamed. Write: `- in: <inputs>` |
| E063 | `- effect:` (function, tool, Rust block) | `effect:` was removed. Write: `- touches: <what>`, for example `- touches: nothing` |
| E064 | `set x = v` | `set` was removed. Write: `<value> into <name>` |
| E065 | `… -> x` | `->` was replaced by `into`. Write: `<step> into <name>` |
| E066 | `append x to list` | `append` was renamed. Write: `add <value> to <list>` |
| E067 | `- for:` in a rule | `for:` was removed. Put the record last in the heading: `# Rule: <name> <Record>` |
| E068 | `>` directly above a heading | A description goes under its heading as a plain paragraph, not in a `>` line above it. |
| E069 | `# Import: <file>` | File imports moved to a `# Uses` section. Write: `- [<label>](<path>)` under `# Uses`. |

Each message quotes the line rewritten in the new form where it can (`set by_region = group sales by region` → "Write: group sales by region into by_region").

### 23.14 Not changed

- `group … by …`, `for each`, the collection words, `try` / `otherwise`, `stop`, `print`, `guard`, `match`, `respond`, `return`.
- Agent trigger headings (`## Every day at 09:00`, `## When …`), `## Do <name>` blocks, service routes and tool verb headings stay headings; only their steps are numbered (OPEN_QUESTIONS Q434).
- `# Import: crate <name>` for libraries (OPEN_QUESTIONS Q435), `# Binding` files and their `- effect:` line, `tools.toml`.
- The hard limits (18.3): the nesting limit counts steps, from the first step under `- steps:` (or under a `##` heading); `- steps:` itself is not a level. At most 3 levels.
