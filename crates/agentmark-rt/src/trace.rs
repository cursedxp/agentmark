use crate::error::RtError;
use serde_json::{json, Map, Value};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

enum Sink {
    File(std::fs::File),
    Stdout,
    Memory(Vec<String>),
    Writer(Box<dyn Write + Send>),
}

/// Append-only JSONL trace recorder (file, stdout, or in-memory); cheap to clone and share.
#[derive(Clone)]
pub struct Trace {
    sink: Arc<Mutex<Sink>>,
}

fn obj(kind: &str, fields: Vec<(&str, Value)>) -> Value {
    let mut m = Map::new();
    m.insert("kind".into(), json!(kind));
    for (k, v) in fields {
        m.insert(k.into(), v);
    }
    Value::Object(m)
}

impl Trace {
    fn with(sink: Sink) -> Trace {
        Trace { sink: Arc::new(Mutex::new(sink)) }
    }

    /// Record to a file (truncated on open).
    pub fn to_file(path: impl AsRef<Path>) -> Result<Trace, RtError> {
        Ok(Trace::with(Sink::File(std::fs::File::create(path)?)))
    }

    /// Record to standard output.
    pub fn stdout() -> Trace {
        Trace::with(Sink::Stdout)
    }

    /// Record in memory (for tests); read back with [`Trace::lines`].
    pub fn memory() -> Trace {
        Trace::with(Sink::Memory(Vec::new()))
    }

    /// Record to any writer (e.g. a socket); write errors are returned from the recording calls.
    pub fn from_writer(w: Box<dyn Write + Send>) -> Trace {
        Trace::with(Sink::Writer(w))
    }

    /// The lines recorded so far (empty unless this is a memory trace).
    pub fn lines(&self) -> Vec<String> {
        match &*self.sink.lock().unwrap_or_else(|e| e.into_inner()) {
            Sink::Memory(v) => v.clone(),
            _ => Vec::new(),
        }
    }

    /// Append one raw JSON value as a line; I/O or lock failures are returned, never swallowed.
    pub fn record(&self, value: &Value) -> Result<(), RtError> {
        let line = serde_json::to_string(value).map_err(|e| RtError::Trace(e.to_string()))?;
        let mut sink = self.sink.lock().map_err(|_| RtError::Trace("trace lock poisoned".into()))?;
        let io = |e: std::io::Error| RtError::Trace(e.to_string());
        match &mut *sink {
            Sink::File(f) => {
                writeln!(f, "{line}").map_err(io)?;
                f.flush().map_err(io)?;
            }
            Sink::Stdout => {
                let mut out = std::io::stdout().lock();
                writeln!(out, "{line}").map_err(io)?;
                out.flush().map_err(io)?;
            }
            Sink::Writer(w) => {
                writeln!(w, "{line}").map_err(io)?;
                w.flush().map_err(io)?;
            }
            Sink::Memory(v) => v.push(line),
        }
        Ok(())
    }

    /// Trace a model call: `prompt_prefix` is the matched fixture key; `context` only for `ask model with v`.
    pub fn model(&self, agent: &str, block: &str, prompt_prefix: &str, context: Option<Value>) -> Result<(), RtError> {
        let mut f = vec![("agent", json!(agent)), ("block", json!(block)), ("prompt_prefix", json!(prompt_prefix))];
        if let Some(c) = context {
            f.push(("context", c));
        }
        self.record(&obj("model", f))
    }

    /// Trace a guard result (`pass` / `fail`) with the guard's source text as `name`.
    pub fn guard(&self, agent: &str, name: &str, pass: bool) -> Result<(), RtError> {
        self.record(&obj(
            "guard",
            vec![("agent", json!(agent)), ("name", json!(name)), ("result", json!(if pass { "pass" } else { "fail" }))],
        ))
    }

    /// Trace an email tool call.
    pub fn tool_email(&self, verb: &str, target: &str, to: &str) -> Result<(), RtError> {
        self.record(&obj(
            "tool",
            vec![("tool", json!("email")), ("verb", json!(verb)), ("target", json!(target)), ("to", json!(to))],
        ))
    }

    /// Trace a crm tool call; `set` only for `update`.
    pub fn tool_crm(&self, verb: &str, source: &str, id: &str, set: Option<Value>) -> Result<(), RtError> {
        let mut f = vec![("tool", json!("crm")), ("verb", json!(verb)), ("source", json!(source)), ("id", json!(id))];
        if let Some(s) = set {
            f.push(("set", s));
        }
        self.record(&obj("tool", f))
    }

    /// Trace an http tool call; `body` only if the call has one.
    pub fn tool_http(&self, verb: &str, url: &str, body: Option<Value>) -> Result<(), RtError> {
        let mut f = vec![("tool", json!("http")), ("verb", json!(verb)), ("url", json!(url))];
        if let Some(b) = body {
            f.push(("body", b));
        }
        self.record(&obj("tool", f))
    }

    /// Trace a log tool call.
    pub fn tool_log(&self, message: &str) -> Result<(), RtError> {
        self.record(&obj("tool", vec![("tool", json!("log")), ("message", json!(message))]))
    }

    /// Trace one request served by a `# Service`.
    pub fn http(&self, method: &str, path: &str, status: u16) -> Result<(), RtError> {
        self.record(&obj("http", vec![("method", json!(method)), ("path", json!(path)), ("status", json!(status))]))
    }

    /// Trace a `print` statement.
    pub fn print(&self, message: &str) -> Result<(), RtError> {
        self.record(&obj("print", vec![("message", json!(message))]))
    }

    /// Trace a failed `try` without `otherwise`.
    pub fn error(&self, agent: &str, block: &str, message: &str) -> Result<(), RtError> {
        self.record(&obj("error", vec![("agent", json!(agent)), ("block", json!(block)), ("message", json!(message))]))
    }
}
