# Connecting real tools

**In short**

- This page shows how an agent talks to real systems: you implement each tool in one Rust file, `rust/connect.rs`.
- It is for developers. A reader who only writes AgentMark programs can skip it.
- After it you can connect a CRM, an email provider and an HTTP client, and run an agent end to end.

**Jump to:** [List the tools](#list-the-tools-a-program-may-use) · [The generated traits](#see-the-traits-the-build-generates) · [Write rust/connect.rs](#write-rustconnectrs) · [What a call receives](#see-what-a-call-receives) · [Add a client crate](#add-a-client-crate) · [HTTP example](#example-a-crm-behind-an-http-api) · [Offline example](#example-an-in-memory-crm-that-runs-offline)

## List the tools a program may use

`tools.toml` lists the tools a program may use and their verbs:

```toml
[crm]
verbs = ["list", "get", "update"]
sources = ["invoices", "leads", "tickets"]
events = ["lead", "ticket"]
effect = "network"

# … [email], [http] and [log] follow
```

The tools in a program that has no `tools.toml` are only the built-in ones, such as `file`.

**Next:** [See the traits the build generates](#see-the-traits-the-build-generates).

## See the traits the build generates

For every tool that has verbs, `agentmark build` generates one trait in `src/tools.rs`. The trait has one method per verb. A method takes the call's arguments as JSON and returns the answer as JSON:

```rust
#[async_trait]
pub trait Crm: Send + Sync {
    async fn list(&self, args: Value) -> Result<Value, RtError>;
    async fn get(&self, args: Value) -> Result<Value, RtError>;
    async fn update(&self, args: Value) -> Result<Value, RtError>;
}
```

`Email` (`send`) and `Http` (`get`, `post`) are generated the same way. `log` has no verbs and is built in. It writes to stderr and the trace.

`--mock` runs answer from fixtures. Mock and real runs share all other code.

**Next:** [Write rust/connect.rs](#write-rustconnectrs).

## Write rust/connect.rs

For a real run, the program uses `Connected { crm: Box<dyn Crm>, email: Box<dyn Email>, http: Box<dyn Http> }`. You build it in **`rust/connect.rs`**. The file sits in your project beside `agentmark.toml`. For a single file, put `rust/` next to it.

- `build`, `run` and `trace` create the file once, with a stub for every tool. Each stub fails with `not connected yet: implement Crm::list in rust/connect.rs`.
- They never change the file again.
- Every build copies it to `src/connect.rs` of the generated project, so deleting the output loses nothing.
- A compiler error in it is reported against your file.
- When `tools.toml` gains a tool, add it to `Connected` there.

**Next:** [See what a call receives](#see-what-a-call-receives).

## See what a call receives

The arguments are exactly what the generated code sends:

| Call | Arguments |
|---|---|
| `crm list` | `{"source": "invoices"}`, answered with an array of rows shaped like the record |
| `crm update` | `{"source": "invoices", "id": "INV-1", "set": {"status": "reminded"}}` |
| `email send` | `{"target": "customer", "to": "ap@mueller.de", "text": "…"}` |

Errors you return become `error` lines in the trace. They stop the block, like any failing tool.

**Next:** [Add a client crate](#add-a-client-crate).

## Add a client crate

A real backend usually needs a client crate. `agentmark.toml` has an optional `[rust.dependencies]` table. It is copied verbatim into the `[dependencies]` of the generated `Cargo.toml`.

Each entry is a version text or a table, as in Cargo.

```toml agentmark.toml
[rust.dependencies]
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

**Watch out:** naming a dependency the generated project already has, such as `serde_json`, `tokio` or an imported crate, is an error.

**Next:** [Example: a CRM behind an HTTP API](#example-a-crm-behind-an-http-api).

## Example: a CRM behind an HTTP API

With the dependency above, `Crm::list` can be HTTP-backed. This is a complete `rust/connect.rs`. It reads `GET <base>/<source>` and takes the base URL from the environment. It leaves the other methods to implement.

`build` compiles it. Nothing in the tests calls a network.

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

**Next:** [Example: an in-memory CRM that runs offline](#example-an-in-memory-crm-that-runs-offline).

## Example: an in-memory CRM that runs offline

This version is what `examples/01-invoice-chaser` needs to run end to end offline. It has an in-memory CRM with two invoices, updated in place. It also has an "email provider" that prints. `http` stays a stub because this program never calls it.

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

With this file in `examples/01-invoice-chaser/rust/`, `agentmark run examples/01-invoice-chaser` reads the invoices from `MemoryCrm`. It asks the real model (set `ANTHROPIC_API_KEY`), prints the reminder through `PrintEmail` and updates the row.

To try it without a key, point `AGENTMARK_MODEL` at a model file. `AGENTMARK_MODEL=mock:fixtures/01-invoice-chaser/model.json` makes the generated program answer from it, in the same syntax the binder uses. The program below is a copy of the example with that file in `rust/`:

```console connect
$ AGENTMARK_MODEL=mock:$PWD/fixtures/01-invoice-chaser/model.json agentmark run app
-> ap@mueller.de: Dear Müller GmbH, this is a friendly reminder that invoice INV-1 for 1200.00 EUR was due on 2026-09-01. …
[log] Reminded Müller GmbH about 1200.00 EUR
$ AGENTMARK_MODEL=mock:$PWD/fixtures/01-invoice-chaser/model.json agentmark trace app
{"kind":"tool","tool":"email","verb":"send","target":"customer","to":"ap@mueller.de"}
{"kind":"tool","tool":"crm","verb":"update","source":"invoices","id":"INV-1","set":{"status":"reminded"}}
```

**Next:** [docs/libraries.md](libraries.md) for Rust crates, or [docs/guide.md](guide.md) for the agent lessons.
