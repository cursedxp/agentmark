# AgentMark

**In short**

- AgentMark is a programming language written as Markdown. One command turns it into a standalone program.
- It is for anyone who wants programs that read like a checklist, including people who never programmed.
- After this page you can install it, run your first program and find the rest of the docs.

**Jump to:** [Example](#a-whole-program) · [Install](#install) · [Your first program in 3 steps](#your-first-program-in-3-steps) · [Where to go next](#where-to-go-next) · [The commands](#the-commands) · [Agents](#agents-ai-workflows-as-a-feature)

## A whole program

A program is a few `.agent.md` files. Headings say what a part is, bullet points say what it does, and `>` lines describe it. This one reads a sales file and prints what every region brought in:

```agentmark
> One line of the sales file: where it was sold and for how much.
# Record: Sale
- region: text
- amount: money

> Prints the revenue of every region in a sales file: `sales-summary <path>`.
# Command: sales-summary
- args: path: text

## Run
- try file read path -> content
  otherwise:
    - print: "Could not read {path}"
    - stop
- try csv.parse(content) as list of Sale -> sales
  otherwise:
    - print: "{path} is not a valid sales file"
    - stop
- set by_region = group sales by region
- for each region, rows in by_region:
  - print: "{region}: {sum of amount in rows}"
- print: "Total: {sum of amount in sales}"

# Run
- sales-summary
```

Saved as `sales-summary.agent.md`, next to the `sales.csv` of `fixtures/06-sales-report/input/`, it prints:

```text
$ agentmark run sales-summary.agent.md -- sales.csv
APAC: 195.00 EUR
EU: 300.00 EUR
US: 510.50 EUR
Total: 1005.50 EUR
```

What to notice:

- **`money`** is a type with a currency (`120.00 EUR`). Money never mixes with a plain number or another currency.
- Reading a file can fail, so it is written `try … otherwise:`. There are no exceptions and no `null`.
- A value that may be missing is `maybe`.
- `agentmark check` prints what each part can touch. It computes this from the code. Nobody declares it by hand.

It is a general-purpose language: records and enums, functions, `match`, lists and maps, a built-in library for text, patterns, numbers, dates, paths, JSON and CSV, files, commands, web services and any Rust library you can name.

An **agent** is a program that asks an AI model. Agents are one more feature built on the same parts. They are described [near the end](#agents-ai-workflows-as-a-feature).

## Install

You need git and a Rust toolchain. Use stable, edition 2021. Get it from https://rustup.rs. On macOS and Linux one command installs the compiler and the language server:

```sh
curl -fsSL https://raw.githubusercontent.com/cursedxp/agentmark/main/install.sh | sh
```

The script clones https://github.com/cursedxp/agentmark into `~/.agentmark/src`, runs `cargo install --locked` for `agentmark` and `agentmark-lsp`, and prints `agentmark --version`. Both binaries end up in `~/.cargo/bin`. Run the same command again to update.

| Setting | Effect |
|---|---|
| `AGENTMARK_HOME` | moves the folder |
| `AGENTMARK_REF` | picks a branch or tag |
| `AGENTMARK_NO_LSP=1` | skips the language server |

To remove everything, run one of these:

```sh
sh ~/.agentmark/src/install.sh --uninstall
curl -fsSL https://raw.githubusercontent.com/cursedxp/agentmark/main/install.sh | sh -s -- --uninstall
```

If you installed with a custom `AGENTMARK_HOME`, set it again for the uninstall. The uninstall removes the binaries and the `src` folder only. It refuses a folder that is not an AgentMark checkout.

**Watch out:** keep `~/.agentmark/src` where it is. A generated project depends on `crates/agentmark-rt` of the checkout the compiler was built from, by absolute path, so builds work offline. Moving or deleting that folder breaks `agentmark build`.

From source, without the script:

```sh
git clone https://github.com/cursedxp/agentmark && cd agentmark
cargo install --locked --path .
```

The same rule holds there: keep that checkout. Without installing, `cargo run -- <command> …` inside the checkout does the same.

**Next:** [Your first program in 3 steps](#your-first-program-in-3-steps).

## Your first program in 3 steps

1. Save the program above as `sales-summary.agent.md`. Copy `fixtures/06-sales-report/input/sales.csv` next to it.
2. Check it. This prints what the program can touch:

   ```text
   $ agentmark check sales-summary.agent.md
   Effects (what each block can touch)
     sales-summary / Run: local(file, print)

   ok: 0 errors, 0 warnings.
   ```

3. Run it. Everything after `--` goes to the program:

   ```text
   $ agentmark run sales-summary.agent.md -- sales.csv
   APAC: 195.00 EUR
   EU: 300.00 EUR
   US: 510.50 EUR
   Total: 1005.50 EUR
   ```

The first run takes a while because AgentMark builds the program. The next ones are quick.

The same check on `examples/06-sales-report`, the full version that also sorts the regions and writes a report file:

```console
$ agentmark check examples/06-sales-report
Effects (what each block can touch)
  sales-report / Run: local(file, print)
  function region_totals: pure
  function table_row: pure
ok: 0 errors, 0 warnings.
```

**Next:** [docs/guide.md](docs/guide.md) builds this program up in small lessons.

## Where to go next

| I want to | Go to |
|---|---|
| learn step by step | [docs/guide.md](docs/guide.md) |
| look up a statement or a type | [docs/reference.md](docs/reference.md) |
| use a Rust library | [docs/libraries.md](docs/libraries.md) |
| connect an agent to real systems | [docs/connect.md](docs/connect.md) |
| set up my editor or an AI assistant | [Editors and AI assistants](#editors-and-ai-assistants) |
| read the language specification | [SPEC.md](SPEC.md) (section 22 makes it general-purpose) |
| see how the compiler works | [DESIGN.md](DESIGN.md) |
| see decisions the spec left open | [OPEN_QUESTIONS.md](OPEN_QUESTIONS.md) |

Ten example projects live in `examples/`. `fixtures/` holds the input files, model answers, tool data and expected output and traces for running them offline.

| Example | What it is |
|---|---|
| `06-sales-report` | reads a CSV file, totals per region, writes a report file |
| `07-file-renamer` | renames the photos of a folder by the date in their names; `--dry-run` only says what it would do |
| `08-notes-api` | a REST service over SQLite, using the `sqlx` crate |
| `09-markdown-to-html` | text and pattern functions, an `# Enum` of kinds of line, `match` |
| `10-log-analyzer` | reads a log, counts problems per service, prints the top N |
| `01` to `05` | agents and services: invoice chaser, lead qualifier, support triage, daily report, invoice service |

## The commands

Every command takes a `.agent.md` file or a project folder. A project folder has an `agentmark.toml`, and a `tools.toml` when the program uses tools from outside.

The examples below run from the AgentMark source. That is `~/.agentmark/src` after `install.sh`, or your own clone. The paths under `examples/` exist there.

| Exit code | Meaning |
|---|---|
| 0 | fine |
| 1 | the project has problems |
| 2 | the command itself could not run |

Problems are printed as `file:line`, the source line, a caret and a plain-language message. A rustc error is never shown for an AgentMark mistake.

### check

Reads everything, reports errors and warnings, and prints what each block can touch. This is the line you approve.

| It says | Meaning |
|---|---|
| `pure` | touches nothing |
| `local(file, print)` | works on this computer |
| `network(crm, email)` | talks to those tools |
| `model` | asks an AI |

```console
$ agentmark check examples/10-log-analyzer
Effects (what each block can touch)
  analyze-log / Run: local(file, print)
  function rank_services: pure
ok: 0 errors, 0 warnings.
```

`--json` prints one JSON document instead. Its schema is in `docs/check-json.md`.

### build

Writes the Cargo project of a program and runs `cargo check` on it. `--no-check` only writes the files. The generated code is readable: one function per block, each statement preceded by a `// <file>.agent.md:<line>` comment.

```console
$ agentmark build examples/06-sales-report -o out
built out (8 files). Compile it with: cargo build --release --manifest-path out/Cargo.toml
```

`--target rust` is the default and the only target. See [Adding a target](#adding-a-target).

### run and trace

`run` builds the program and runs it. `trace` does the same and prints the trace on stdout, one JSON object per line: every file touched, every line printed, every tool and model call.

- A program with one `# Command` in its `# Run` makes that command the default. The arguments go after `--`: `analyze-log <path> [--top N]` here.
- A program with schedules (an agent's "every day at 09:00") fires each once, so a run ends.

```console
$ agentmark run examples/10-log-analyzer -- fixtures/10-log-analyzer/input/app.log --top 2
Log report for fixtures/10-log-analyzer/input/app.log
Lines read: 15
Entries: 13
Errors: 7  Warnings: 2  Info: 4
First error at 2026-10-12 08:15:09
Top 2 services by errors:
  payments  4
  mail      2
```

```console
$ agentmark trace examples/10-log-analyzer -- fixtures/10-log-analyzer/input/app.log --top 2
{"kind":"file","verb":"read","path":"fixtures/10-log-analyzer/input/app.log"}
{"kind":"print","message":"Log report for fixtures/10-log-analyzer/input/app.log"}
…
{"kind":"print","message":"  mail      2"}
```

The trace is the behavior of the program. The tests of AgentMark compare behavior, not generated source. They compare the output of every example against `fixtures/<example>/`.

A program that uses a model or tools from outside runs with `--mock --fixtures <folder>`. That answers those calls from files, so nothing touches the network. See [Agents](#agents-ai-workflows-as-a-feature).

### fmt and lint

- `fmt` rewrites files into the one canonical style. `--check` changes nothing and exits 1 when a file is not canonical.
- `lint` runs the style rules. The ones listed under `[lint] deny` in `agentmark.toml` are errors.

```console
$ agentmark fmt --check examples/06-sales-report
7 files are canonical.
$ agentmark lint examples/06-sales-report
lint: 0 errors, 0 warnings in 7 files.
```

### doc

Prints the documentation page of a project, or the cheat sheet of a bound library. The page lists commands, records, functions, services, agents and what each block can touch.

```console
$ agentmark doc examples/06-sales-report
# sales-report
…
### sales-report
Reads a sales CSV file and writes a revenue report per region, biggest region first.
…
- can touch: local(file, print)
…
```

### bind

Creates or refreshes the binding of a library, `bindings/<crate>.agent.md`. You rarely call it yourself, because `check` and `build` bind a crate the first time they meet it.

- It reads the public API from rustdoc JSON.
- What maps cleanly to AgentMark types is bound directly.
- A model proposes the rest ("assisted"), or it is listed under `## Not bound`.
- An existing binding stays on the version in its header, so nothing moves silently.

| Command | Effect |
|---|---|
| `agentmark bind <crate>` | nothing to do when the binding exists |
| `agentmark bind <crate> --version 0.10.9` | move to exactly that release |
| `agentmark bind <crate> --latest` | move to the newest release |
| `agentmark bind <crate> --force` | regenerate the same version; an approved binding becomes unverified again |
| `agentmark bind --all` | refresh every binding of the project |

Moving or regenerating marks the binding unverified until you approve it again.

A real bind needs `ANTHROPIC_API_KEY` and the network. The commands below run offline:

- `AGENTMARK_MODEL=mock:<model.json>` stands in for the model.
- `AGENTMARK_RUSTDOC_JSON` and `AGENTMARK_RUSTDOC_JSON_DIR` stand in for the rustdoc output.

The files are the ones the tests of the binder use.

```console
$ AGENTMARK_MODEL=mock:$PWD/tests/binder/model.json AGENTMARK_RUSTDOC_JSON=$PWD/tests/binder/sha2.json AGENTMARK_RUSTDOC_JSON_DIR=$PWD/tests/binder agentmark bind sha2 --project examples/05-invoice-service
sha2 0.10.9: cached, examples/05-invoice-service/bindings/sha2.agent.md is up to date (1 bound, 81 not bound).
  Effect: pure (approved).
$ agentmark doc sha2 --project examples/05-invoice-service
sha2 0.10.9 (bound 2026-10-14)
effect: pure
  sha2.sha256_digest(data: bytes) -> bytes
      Compute hash of `data`.
```

## Libraries

**Built in.** `text`, `regex`, `number`, `money`, `date`, `path`, `json` and `csv` are part of the language. Each has a fixed effect that `check` shows, and all of them are pure. They are listed in [docs/reference.md](docs/reference.md), section 22.3.

`file` is a built-in tool that needs no `tools.toml` entry. Its verbs are `file read`, `write`, `append`, `list`, `rename`, `exists` and `delete`. Reading standard input is `- read input -> text`.

**Any Rust crate.** The rest of the Rust ecosystem is available through the binder and three lines in your program. The binder is the `agentmark bind` command from the table above.

Objects a crate hands you are **handles**. A hasher and a database pool are examples.

You create a handle with a statement and call methods on it, and the checker knows which method uses it up. A handle cannot be printed, compared or stored in a record.

[docs/libraries.md](docs/libraries.md) has the whole story.

### Any crate: three lines

1. Write `# Import: crate sha2` at the top of the file and use the library in a plain statement: `- sha2 sha256_digest "{invoice.id}{invoice.amount}" -> digest`.
2. Run `agentmark check`. The first time, it generates `bindings/sha2.agent.md` and stops, because nobody has approved what the library may touch.
3. Open the binding, read the proposed effect and change `- effect: unverified` to the right class (`pure` here). `check` is green.

```console app
$ AGENTMARK_MODEL=mock:$PWD/tests/binder/model.json AGENTMARK_RUSTDOC_JSON=$PWD/tests/binder/sha2.json AGENTMARK_RUSTDOC_JSON_DIR=$PWD/tests/binder agentmark check app
binding sha2: generated
app/bindings/sha2.agent.md:5
  - effect: unverified
  The binding "sha2" is unverified: nobody approved what it may touch.
  Approve it by changing the line to "- effect: pure" (or the right class).
1 error, 1 warning: not ok.
# exit 1
# in app/bindings/sha2.agent.md: replace "- effect: unverified" with "- effect: pure"
$ agentmark check app
ok: 0 errors, 0 warnings.
```

Here `app` is a copy of `examples/05-invoice-service` without its `bindings/sha2.agent.md`.

The effect line is the only thing you approve. It is what `check` holds every caller to: a function that says `- effect: pure` and calls a library you marked `network` is an error. The pin is the version in the binding header: the generated `Cargo.toml` depends on `sha2 = "=0.10.9"`.

**Watch out:** `build`, `run` and `trace` refuse a program that uses an unapproved binding. While you develop, `--allow-unverified` on `check`, `build`, `run` and `trace` assumes the proposed effect and warns once per binding.

**Next:** [docs/libraries.md](docs/libraries.md).

## Tools you write, tools you connect

A tool is something a program calls with a statement, such as `- slugger slug title -> s`. Two kinds exist.

| Kind | Where it lives | Needs |
|---|---|---|
| written in AgentMark | a `# Tool` declaration | nothing else |
| implemented outside the program (a real CRM, an email provider) | `tools.toml` and `rust/connect.rs` | Rust code, see [Connecting real tools](docs/connect.md) |

A `# Tool` has one `## verb(params) -> type` block per verb. Its declared effect is checked against its body exactly like a function's.

A program that only uses these tools and the built-in `file` tool needs no `tools.toml` and no Rust. [docs/guide.md](docs/guide.md) shows a complete example in Lesson 11, "Write your own tool".

**Next:** [Editors and AI assistants](#editors-and-ai-assistants).

## Editors and AI assistants

Editor support is described in [docs/editor.md](docs/editor.md). It covers the VS Code extension, the language server, and the tree-sitter grammar for Zed, Neovim and Helix.

To have an AI assistant write AgentMark for you, copy one file into your project:

| Assistant | File |
|---|---|
| Claude Code | [templates/CLAUDE.md](templates/CLAUDE.md) |
| Cursor | [templates/.cursorrules](templates/.cursorrules) |

Both point the assistant at [docs/reference.md](docs/reference.md), the one file it needs.

**Next:** [Agents](#agents-ai-workflows-as-a-feature).

## Agents: AI workflows as a feature

An **agent** is a program that wakes up on a schedule or an event, uses tools, and may ask a model. It is built from the parts above, not beside them: the checker rewrites an agent into plain functions and entries before anything is generated.

What agents add is the vocabulary for the risky part:

- Every block states what it may touch (model, network, files, local state).
- `ask model` is followed by a `guard:` on the answer.
- `check` refuses an agent that does more than its `tools:` line says.

```agentmark
> Says hello every Monday.
# Agent: greeter
- model: claude
- tools: log

## Every Monday at 08:00
- ask model: "Write one friendly sentence for the week." -> line
- guard: line is not empty
- log: "{line}"
```

`examples/01-invoice-chaser` is a complete agent. `check` prints what it can touch.

```console
$ agentmark check examples/01-invoice-chaser
Effects (what each block can touch)
  invoice-chaser / Every day at 09:00: model, network(crm, email), local(log)
  invoice-chaser / Do remind: model, network(email, crm), local(log)
  rule overdue: pure
ok: 0 errors, 0 warnings.
```

`run` and `trace` answer the model and the tools from fixtures when you pass `--mock --fixtures`.

```console
$ agentmark run examples/01-invoice-chaser --mock --fixtures fixtures/01-invoice-chaser
[log] Reminded Müller GmbH about 1200.00 EUR
```

```console
$ agentmark trace examples/01-invoice-chaser --mock --fixtures fixtures/01-invoice-chaser
{"kind":"model","agent":"invoice-chaser","block":"Every day at 09:00","prompt_prefix":"Write a short, polite payment reminder","context":{"id":"INV-1"}}
{"kind":"guard","agent":"invoice-chaser","name":"no amounts changed in reminder","result":"pass"}
{"kind":"tool","tool":"email","verb":"send","target":"customer","to":"ap@mueller.de"}
{"kind":"tool","tool":"crm","verb":"update","source":"invoices","id":"INV-1","set":{"status":"reminded"}}
{"kind":"tool","tool":"log","message":"Reminded Müller GmbH about 1200.00 EUR"}
```

Without `--mock`, a run uses the real model (`ANTHROPIC_API_KEY`) and the tools you connect. Connecting them is the next section.

**Next:** Lessons 12 to 14 of [docs/guide.md](docs/guide.md) build an agent step by step.

## Connecting real tools: `rust/connect.rs`

An agent talks to real systems through one Rust file of your project, `rust/connect.rs`. Until you write it, every real call fails with "not connected yet", and `--mock` runs still work.

**Next:** [docs/connect.md](docs/connect.md) has the traits, the file and two complete examples.

## Adding a target

A **target** is a module under `src/codegen/` that reads the IR and nothing else. `src/ir/README.md` lists every node.

What the IR looks like:

- There are no agent, rule or guard nodes left. A guard is a `Let`, a trace `Effect` and an `If`.
- Triggers sit on `Entry` values (`Trigger::Every`, `DailyAt`, `Weekly`, `Event`, `Start`).
- Constants are inlined.
- Every statement and expression carries its `EffectSet`.

Steps:

1. Write `src/codegen/<target>.rs` with a `generate(&ir::Program, &Options) -> Vec<GeneratedFile>` like `codegen::rust::generate`.
2. Declare it in `src/codegen/mod.rs`.
3. Accept the new name in `build` (`src/cli/build.rs` refuses every `--target` but `rust`).

`Options` and `GeneratedFile` are defined in `src/codegen/rust.rs` today. Move them to `codegen/mod.rs` when a second target shares them.

`run` and `trace` have no `--target`. `build_and_run` in the same file calls the Rust generator and cargo directly. It then starts the binary with `--once`, plus `--mock`, `--fixtures` and `--trace -`.

A target that should run under `agentmark run` needs that function generalised: a build step, and a program that takes those flags.

A target also needs a runtime with the contract of `crates/agentmark-rt`: `Money` in integer minor units, the model client, mock tools and fixtures, the JSONL trace of DESIGN §5.1. A target is correct when the trace of every example equals `fixtures/<example>/expected.trace.jsonl`. `tests/examples_e2e.rs` runs exactly that check for Rust.

Keep the output readable: one function per block, a `<file>.agent.md:<line>` comment on every statement. Map the target's compiler errors back to `.agent.md` lines next to `report_failure`, so an AgentMark author never reads a foreign compiler's message.

## Layout

| Path | What |
|---|---|
| `src/syntax`, `src/sema`, `src/lower`, `src/ir` | lexer and parser (chumsky), semantic check and effects, agent lowering, the IR |
| `src/stdlib.rs`, `src/sema/stdlib.rs` | the built-in library and the `file` tool |
| `src/codegen/rust` | the Rust target |
| `src/binder` | `agentmark bind`: rustdoc JSON to a binding |
| `src/cli` | the commands |
| `crates/agentmark-rt` | runtime of generated programs: money, collections and the built-in library, scheduler, model client, mock tools, trace |
| `examples/`, `fixtures/` | ten projects; input files, model answers, tool data, expected output and traces |

`cargo test --workspace` runs everything offline, including the commands and code blocks of this README (`tests/readme.rs`).
