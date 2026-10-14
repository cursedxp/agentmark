# AgentMark

AgentMark is a programming language written as Markdown. A program is a few `.agent.md` files that read like a document: headings say what a part is, bullet points say what it does, `>` lines are its description. `agentmark build` turns them into one standalone binary, with no runtime to install next to it.

It is a general-purpose language: records and enums, functions, `match`, lists and maps with words such as `group`, `sort` and `sum of`, a built-in library for text, patterns, numbers, dates, paths, JSON and CSV, files, commands with arguments, web services, and any Rust library you can name. Agents (programs that ask an AI model) are one more feature built on the same parts, described [at the end](#agents-ai-workflows-as-a-feature) of this page.

This is a whole program. It reads a sales file and prints what every region brought in (`examples/06-sales-report` is the full version, which also sorts the regions and writes a report file):

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

Some things to notice. `money` is a type with a currency (`120.00 EUR`), and money never mixes with a plain number or another currency. Reading a file can fail, so it is written with `try … otherwise:`; there are no exceptions and no `null` (a value that may be missing is `maybe`). `agentmark check` reads the program and prints what each part can touch, computed from the code, not declared by hand. Here it is on `examples/06-sales-report`, the full version of this program, which reads and writes files and prints, nothing else:

```console
$ agentmark check examples/06-sales-report
Effects (what each block can touch)
  sales-report / Run: local(file, print)
  function region_totals: pure
  function table_row: pure
ok: 0 errors, 0 warnings.
```

The language is specified in [SPEC.md](SPEC.md) (section 22 is the part that makes it general-purpose), the compiler is described in [DESIGN.md](DESIGN.md), and decisions the spec left open are written down in [OPEN_QUESTIONS.md](OPEN_QUESTIONS.md). New to programming? Start with [docs/guide.md](docs/guide.md), which builds a report program step by step and adds an agent at the end. Every construct, with a tested example each, is in [docs/reference.md](docs/reference.md). Ten example projects live in `examples/`; `fixtures/` holds the input files, model answers, tool data and expected output and traces for running them offline.

| example | what it is |
|---|---|
| `06-sales-report` | reads a CSV file, totals per region, writes a report file |
| `07-file-renamer` | renames the photos of a folder by the date in their names; `--dry-run` only says what it would do |
| `08-notes-api` | a REST service over SQLite, using the `sqlx` crate |
| `09-markdown-to-html` | text and pattern functions, an `# Enum` of kinds of line, `match` |
| `10-log-analyzer` | reads a log, counts problems per service, prints the top N |
| `01` to `05` | agents and services: invoice chaser, lead qualifier, support triage, daily report, invoice service |

## Install

You need git and a Rust toolchain (stable, edition 2021; get it from https://rustup.rs). On macOS and Linux one command installs the compiler and the language server:

```sh
curl -fsSL https://raw.githubusercontent.com/cursedxp/agentmark/main/install.sh | sh
```

The script clones https://github.com/cursedxp/agentmark into `~/.agentmark/src`, runs `cargo install --locked` for `agentmark` and `agentmark-lsp`, and prints `agentmark --version`. Both binaries end up in `~/.cargo/bin`. Run the same command again to update. `AGENTMARK_HOME` moves the folder, `AGENTMARK_REF` picks a branch or tag, and `AGENTMARK_NO_LSP=1` skips the language server. To remove everything, run `sh ~/.agentmark/src/install.sh --uninstall`.

Keep `~/.agentmark/src` where it is: a generated project depends on `crates/agentmark-rt` of the checkout the compiler was built from, by absolute path (so builds work offline). Moving or deleting that folder breaks `agentmark build`.

From source, without the script:

```sh
git clone https://github.com/cursedxp/agentmark && cd agentmark
cargo install --locked --path .
```

The same rule holds there: keep that checkout. Without installing, `cargo run -- <command> …` inside the checkout does the same.

## The commands

Every command takes a `.agent.md` file or a project folder (a folder with an `agentmark.toml`, and a `tools.toml` when the program uses tools from outside). Exit code 0 is fine, 1 means the project has problems, 2 means the command itself could not run. Problems are printed as `file:line`, the source line, a caret and a plain-language message; a rustc error is never shown for an AgentMark mistake. The commands below are run from the AgentMark source (`~/.agentmark/src` after `install.sh`, or your own clone), so the paths under `examples/` exist.

### check

Reads everything, reports errors and warnings, and prints what each block can touch. This is the line you approve: a block that says `pure` touches nothing, `local(file, print)` works on this computer, `network(crm, email)` talks to those tools, `model` asks an AI.

```console
$ agentmark check examples/10-log-analyzer
Effects (what each block can touch)
  analyze-log / Run: local(file, print)
  function rank_services: pure
ok: 0 errors, 0 warnings.
```

`--json` prints one JSON document instead (schema: `docs/check-json.md`).

### build

Writes the Cargo project of a program, and runs `cargo check` on it. `--no-check` only writes the files. The generated code is readable: one function per block, each statement preceded by a `// <file>.agent.md:<line>` comment.

```console
$ agentmark build examples/06-sales-report -o out
built out (8 files). Compile it with: cargo build --release --manifest-path out/Cargo.toml
```

`--target rust` is the default and the only target; see "Adding a target" below.

### run and trace

`run` builds the program and runs it. A program with one `# Command` in its `# Run` makes that command the default: the arguments go after `--`, `analyze-log <path> [--top N]` here. A program with schedules (an agent's "every day at 09:00") fires each once, so a run ends. `trace` does the same and prints the trace on stdout, one JSON object per line: every file touched, every line printed, every tool and model call.

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

The trace is the behavior of the program, and the tests of AgentMark compare behavior, not generated source: the output of every example against `fixtures/<example>/`. A program that uses a model or tools from outside runs with `--mock --fixtures <folder>`, which answers those calls from files instead (see "Agents" below), so nothing touches the network.

### fmt and lint

`fmt` rewrites files into the one canonical style (`--check` changes nothing and exits 1 when a file is not canonical). `lint` runs the style rules; the ones listed under `[lint] deny` in `agentmark.toml` are errors.

```console
$ agentmark fmt --check examples/06-sales-report
7 files are canonical.
$ agentmark lint examples/06-sales-report
lint: 0 errors, 0 warnings in 7 files.
```

### doc

Prints the documentation page of a project (commands, records, functions, services, agents, what each block can touch), or the cheat sheet of a bound library.

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

Creates or refreshes the binding of a library, `bindings/<crate>.agent.md` (you rarely call it yourself: `check` and `build` bind a crate the first time they meet it). It reads the public API from rustdoc JSON; what maps cleanly to AgentMark types is bound directly, the rest is proposed by a model ("assisted") or listed under `## Not bound`. An existing binding stays on the version in its header, so nothing moves silently:

- `agentmark bind <crate>`: nothing to do when the binding exists
- `agentmark bind <crate> --version 0.10.9`: move to exactly that release
- `agentmark bind <crate> --latest`: move to the newest release
- `agentmark bind <crate> --force`: regenerate the same version (overwrites an approved binding, which becomes unverified again)
- `agentmark bind --all`: refresh every binding of the project

Moving or regenerating marks the binding unverified until you approve it again. A real bind needs `ANTHROPIC_API_KEY` and the network. The commands below run offline: `AGENTMARK_MODEL=mock:<model.json>` stands in for the model and `AGENTMARK_RUSTDOC_JSON(_DIR)` for the rustdoc output; the files are the ones the tests of the binder use.

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

**Built in.** `text`, `regex`, `number`, `money`, `date`, `path`, `json` and `csv` are part of the language, with a fixed effect each that `check` shows (all of them are pure). `file` is a built-in tool (`file read`, `write`, `append`, `list`, `rename`, `exists`, `delete`) that needs no `tools.toml` entry; reading standard input is `- read input -> text`. They are listed in [docs/reference.md](docs/reference.md), section 22.3.

**Any Rust crate.** The rest of the Rust ecosystem is available too, through the binder (`agentmark bind`, above) and three lines in your program. Objects a crate hands you (a hasher, a database pool) are `handle`s: you create one with a statement, call methods on it, and the checker knows which method uses it up. A handle cannot be printed, compared or stored in a record. [docs/libraries.md](docs/libraries.md) has the whole story.

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

(`app` is a copy of `examples/05-invoice-service` without its `bindings/sha2.agent.md`. While you develop, `--allow-unverified` on `check`, `build`, `run` and `trace` assumes the proposed effect and warns once per binding; `build`, `run` and `trace` without the flag refuse a program that uses an unapproved binding.) The effect line is the only thing you approve: it is what `check` holds every caller to, so a function that says `- effect: pure` and calls a library you marked `network` is an error. The pin is the version in the binding header: the generated `Cargo.toml` depends on `sha2 = "=0.10.9"`.

## Tools you write, tools you connect

A tool is something a program calls with a statement: `- slugger slug title -> s`. Two kinds exist.

**A tool written in AgentMark** is a `# Tool` declaration with one `## verb(params) -> type` block per verb. Its declared effect is checked against its body exactly like a function's. A program that only uses these and the built-in `file` tool needs no `tools.toml` and no Rust:

```agentmark
> Turns a title into a web address: lower case, dashes between words.
# Tool: slugger
- effect: pure

## slug(title: text) -> text
- set lowered = text.lower(title)
- return regex.replace(lowered, "[^a-z0-9]+", "-")

> Prints the address of a title: `address <title>`.
# Command: address
- args: title: text

## Run
- print: "/posts/{slugger.slug(title)}"

# Run
- address
```

**A tool implemented outside the program** (a real CRM, an email provider) is listed in `tools.toml` and implemented in `rust/connect.rs`; it comes up with agents, which use such tools most, in [Connecting real tools](#connecting-real-tools-rustconnectrs) below.

## Editors and AI assistants

Editor support (VS Code extension, language server, tree-sitter grammar for Zed, Neovim and Helix) is described in [docs/editor.md](docs/editor.md). To have an AI assistant write AgentMark for you, copy [templates/CLAUDE.md](templates/CLAUDE.md) (Claude Code) or [templates/.cursorrules](templates/.cursorrules) (Cursor) into your project; both point the assistant at [docs/reference.md](docs/reference.md), the one file it needs.

## Agents: AI workflows as a feature

An agent is a program that wakes up on a schedule or an event, uses tools, and may ask a model. It is built from the parts above, not beside them: the checker rewrites an agent into plain functions and entries before anything is generated. What agents add is the vocabulary for the risky part. Every block states what it may touch (model, network, files, local state), `ask model` is followed by a `guard:` on the answer, and `check` refuses an agent that does more than its `tools:` line says.

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

`examples/01-invoice-chaser` is a complete agent. `check` prints what it can touch, and `run` and `trace` answer the model and the tools from fixtures when you pass `--mock --fixtures`:

```console
$ agentmark check examples/01-invoice-chaser
Effects (what each block can touch)
  invoice-chaser / Every day at 09:00: model, network(crm, email), local(log)
  invoice-chaser / Do remind: model, network(email, crm), local(log)
  rule overdue: pure
ok: 0 errors, 0 warnings.
```

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

Without `--mock`, a run uses the real model (`ANTHROPIC_API_KEY`) and the tools you connect (next). The chapter that adds an agent step by step is the second half of [docs/guide.md](docs/guide.md).

### Connecting real tools: `rust/connect.rs`

`tools.toml` lists the tools a program may use and their verbs:

```toml
[crm]
verbs = ["list", "get", "update"]
sources = ["invoices", "leads", "tickets"]
events = ["lead", "ticket"]
effect = "network"

# … [email], [http] and [log] follow
```

For every tool that has verbs, `agentmark build` generates one trait in `src/tools.rs` with one method per verb. A method takes the call's arguments as JSON and returns the answer as JSON:

```rust
#[async_trait]
pub trait Crm: Send + Sync {
    async fn list(&self, args: Value) -> Result<Value, RtError>;
    async fn get(&self, args: Value) -> Result<Value, RtError>;
    async fn update(&self, args: Value) -> Result<Value, RtError>;
}
```

`Email` (`send`) and `Http` (`get`, `post`) are generated the same way; `log` has no verbs and is built in (stderr and the trace). `--mock` runs answer from fixtures, so mock and real runs share all other code. For a real run, `Connected { crm: Box<dyn Crm>, email: Box<dyn Email>, http: Box<dyn Http> }` is what the program uses, and you build it in **`rust/connect.rs`**, a file of your project beside `agentmark.toml` (for a single file: `rust/` next to it). `build`, `run` and `trace` create it once, with a stub for every tool whose methods fail with `not connected yet: implement Crm::list in rust/connect.rs`, and never change it again. Every build copies it to `src/connect.rs` of the generated project, so deleting the output loses nothing; a compiler error in it is reported against your file. When `tools.toml` gains a tool, add it to `Connected` there.

The arguments are exactly what the generated code sends: `crm list` gets `{"source": "invoices"}` and answers an array of rows shaped like the record; `crm update` gets `{"source": "invoices", "id": "INV-1", "set": {"status": "reminded"}}`; `email send` gets `{"target": "customer", "to": "ap@mueller.de", "text": "…"}`.

A real backend usually needs a client crate. `agentmark.toml` has an optional `[rust.dependencies]` table that is copied verbatim into the `[dependencies]` of the generated `Cargo.toml` (each entry is a version text or a table, as in Cargo; naming a dependency the generated project already has, such as `serde_json`, `tokio` or an imported crate, is an error):

```toml agentmark.toml
[rust.dependencies]
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

With that, `Crm::list` can be HTTP-backed. This is a complete `rust/connect.rs` that reads `GET <base>/<source>` (the base URL from the environment) and leaves the other methods to implement; `build` compiles it, nothing in the tests calls a network:

```rust http
use agentmark_rt::RtError;
use async_trait::async_trait;
use serde_json::Value;

use crate::tools::{Connected, Crm, Email, Http};

/// The CRM behind an HTTP API: `GET <base>/invoices` answers the rows as a JSON array.
struct HttpCrm {
    base: String,
    client: reqwest::Client,
}

#[async_trait]
impl Crm for HttpCrm {
    // args: {"source": "invoices"}
    async fn list(&self, args: Value) -> Result<Value, RtError> {
        let source = args["source"].as_str().unwrap_or_default();
        let fail = |e: reqwest::Error| RtError::Tool(format!("crm list {source}: {e}"));
        let reply = self.client.get(format!("{}/{source}", self.base)).send().await.map_err(fail)?;
        reply.error_for_status().map_err(fail)?.json::<Value>().await.map_err(fail)
    }
    async fn get(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Crm::get in rust/connect.rs".into()))
    }
    async fn update(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Crm::update in rust/connect.rs".into()))
    }
}

/// Nobody connected the other tools yet: every call says what to implement.
struct NotConnected;

#[async_trait]
impl Email for NotConnected {
    async fn send(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Email::send in rust/connect.rs".into()))
    }
}

#[async_trait]
impl Http for NotConnected {
    async fn get(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Http::get in rust/connect.rs".into()))
    }
    async fn post(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Http::post in rust/connect.rs".into()))
    }
}

pub fn connect() -> Connected {
    Connected {
        crm: Box::new(HttpCrm {
            base: std::env::var("CRM_URL").unwrap_or_else(|_| "http://localhost:8080".into()),
            client: reqwest::Client::new(),
        }),
        email: Box::new(NotConnected),
        http: Box::new(NotConnected),
    }
}
```

The in-memory version below is what `examples/01-invoice-chaser` needs to run end to end offline: an in-memory CRM (two invoices, updated in place) and an "email provider" that prints. `http` stays a stub because this program never calls it.

```rust file=rust/connect.rs
//! Connect the tools to the real world (this file is yours; `build` never overwrites it).

use std::sync::Mutex;

use agentmark_rt::RtError;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{Connected, Crm, Email, Http};

/// A CRM that lives in memory: two invoices, updated in place.
struct MemoryCrm {
    invoices: Mutex<Vec<Value>>,
}

impl MemoryCrm {
    fn new() -> Self {
        let invoices = vec![
            json!({"id": "INV-1", "customer": "Müller GmbH", "customer_email": "ap@mueller.de",
                   "amount": "1200.00 EUR", "due": "2026-09-01", "status": "open"}),
            json!({"id": "INV-2", "customer": "Schmidt AG", "customer_email": "buch@schmidt.de",
                   "amount": "450.00 EUR", "due": "2026-12-10", "status": "open"}),
        ];
        MemoryCrm { invoices: Mutex::new(invoices) }
    }
}

#[async_trait]
impl Crm for MemoryCrm {
    // args: {"source": "invoices"}  ->  the rows of that source
    async fn list(&self, args: Value) -> Result<Value, RtError> {
        match args["source"].as_str() {
            Some("invoices") => Ok(Value::Array(self.invoices.lock().unwrap().clone())),
            other => Err(RtError::Tool(format!("crm list: no source {other:?}"))),
        }
    }
    // args: {"source": "invoices", "id": "INV-1"}  ->  one row
    async fn get(&self, args: Value) -> Result<Value, RtError> {
        let rows = self.invoices.lock().unwrap();
        rows.iter()
            .find(|r| r["id"] == args["id"])
            .cloned()
            .ok_or_else(|| RtError::Tool(format!("crm get: no row {}", args["id"])))
    }
    // args: {"source": "invoices", "id": "INV-1", "set": {"status": "reminded"}}
    async fn update(&self, args: Value) -> Result<Value, RtError> {
        let mut rows = self.invoices.lock().unwrap();
        let row = rows
            .iter_mut()
            .find(|r| r["id"] == args["id"])
            .ok_or_else(|| RtError::Tool(format!("crm update: no row {}", args["id"])))?;
        for (field, value) in args["set"].as_object().into_iter().flatten() {
            row[field] = value.clone();
        }
        Ok(row.clone())
    }
}

/// An email "provider" that prints what it would send.
struct PrintEmail;

#[async_trait]
impl Email for PrintEmail {
    // args: {"target": "customer", "to": "ap@mueller.de", "text": "..."}
    async fn send(&self, args: Value) -> Result<Value, RtError> {
        println!("-> {}: {}", args["to"].as_str().unwrap_or("?"), args["text"].as_str().unwrap_or(""));
        Ok(json!({"sent": true}))
    }
}

/// Nobody connected `http` yet: every call says what to implement.
struct NotConnected;

#[async_trait]
impl Http for NotConnected {
    async fn get(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Http::get in rust/connect.rs".into()))
    }
    async fn post(&self, _args: Value) -> Result<Value, RtError> {
        Err(RtError::Tool("not connected yet: implement Http::post in rust/connect.rs".into()))
    }
}

/// The tools of a real run.
pub fn connect() -> Connected {
    Connected {
        crm: Box::new(MemoryCrm::new()),
        email: Box::new(PrintEmail),
        http: Box::new(NotConnected),
    }
}
```

With this file in `examples/01-invoice-chaser/rust/`, `agentmark run examples/01-invoice-chaser` reads the invoices from `MemoryCrm`, asks the real model (set `ANTHROPIC_API_KEY`), prints the reminder through `PrintEmail` and updates the row. To try it without a key, point `AGENTMARK_MODEL` at a model file: `AGENTMARK_MODEL=mock:fixtures/01-invoice-chaser/model.json` makes the generated program answer from it, in the same syntax the binder uses. The program is a copy of the example with that file in `rust/`:

```console connect
$ AGENTMARK_MODEL=mock:$PWD/fixtures/01-invoice-chaser/model.json agentmark run app
-> ap@mueller.de: Dear Müller GmbH, this is a friendly reminder that invoice INV-1 for 1200.00 EUR was due on 2026-09-01. …
[log] Reminded Müller GmbH about 1200.00 EUR
$ AGENTMARK_MODEL=mock:$PWD/fixtures/01-invoice-chaser/model.json agentmark trace app
{"kind":"tool","tool":"email","verb":"send","target":"customer","to":"ap@mueller.de"}
{"kind":"tool","tool":"crm","verb":"update","source":"invoices","id":"INV-1","set":{"status":"reminded"}}
```

Errors you return become `error` lines in the trace and stop the block, like any failing tool.

## Adding a target

A target is a module under `src/codegen/` that reads the IR and nothing else. `src/ir/README.md` lists every node: there are no agent, rule or guard nodes left (a guard is a `Let`, a trace `Effect` and an `If`), triggers sit on `Entry` values (`Trigger::Every`, `DailyAt`, `Weekly`, `Event`, `Start`), constants are inlined, and every statement and expression carries its `EffectSet`. Write `src/codegen/<target>.rs` with a `generate(&ir::Program, &Options) -> Vec<GeneratedFile>` like `codegen::rust::generate`, declare it in `src/codegen/mod.rs`, and accept the new name in `build` (`src/cli/build.rs` refuses every `--target` but `rust`). `Options` and `GeneratedFile` are defined in `src/codegen/rust.rs` today; move them to `codegen/mod.rs` when a second target shares them. `run` and `trace` have no `--target`: `build_and_run` in the same file calls the Rust generator and cargo directly, then starts the binary with `--once` (plus `--mock`, `--fixtures`, `--trace -`), so a target that should run under `agentmark run` needs that function generalised: a build step, and a program that takes those flags. A target also needs a runtime with the contract of `crates/agentmark-rt` (`Money` in integer minor units, the model client, mock tools and fixtures, the JSONL trace of DESIGN §5.1), because a target is correct when the trace of every example equals `fixtures/<example>/expected.trace.jsonl`: `tests/examples_e2e.rs` runs exactly that check for Rust. Keep the output readable (one function per block, a `<file>.agent.md:<line>` comment on every statement) and map the target's compiler errors back to `.agent.md` lines next to `report_failure`, so an AgentMark author never reads a foreign compiler's message.

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
