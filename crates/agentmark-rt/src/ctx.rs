use crate::error::RtError;
use crate::model::{MockModel, ModelClient, ModelRequest};
use crate::tools::{MockTools, Tools};
use crate::trace::Trace;
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;

/// Source of "today" / "now": the system clock or a fixed mock date.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    fixed: Option<NaiveDate>,
}

impl Clock {
    /// The real system clock.
    pub fn system() -> Clock {
        Clock { fixed: None }
    }

    /// A clock frozen at a date (from `model.json` `"today"`).
    pub fn fixed(date: NaiveDate) -> Clock {
        Clock { fixed: Some(date) }
    }

    /// Today's date (UTC for the system clock).
    pub fn today(&self) -> NaiveDate {
        self.fixed.unwrap_or_else(|| Utc::now().date_naive())
    }

    /// The current instant (midnight UTC of the fixed date for a fixed clock).
    pub fn now(&self) -> DateTime<Utc> {
        match self.fixed {
            Some(d) => d.and_hms_opt(0, 0, 0).expect("midnight is valid").and_utc(),
            None => Utc::now(),
        }
    }
}

/// Everything a generated block needs: trace, model, tools, clock and the current trace labels.
#[derive(Clone)]
pub struct Ctx {
    /// The shared trace recorder.
    pub trace: Trace,
    /// The model provider.
    pub model: Arc<dyn ModelClient>,
    /// The tool backend.
    pub tools: Arc<dyn Tools>,
    /// The clock.
    pub clock: Clock,
    /// Trace label of the enclosing unit (agent / service / command name).
    pub unit: String,
    /// Heading of the entry that started the run (e.g. `Every day at 09:00`).
    pub entry: String,
}

impl Ctx {
    /// Bundle the parts; labels start empty (set them with [`Ctx::enter`]).
    pub fn new(trace: Trace, model: Arc<dyn ModelClient>, tools: Arc<dyn Tools>, clock: Clock) -> Ctx {
        Ctx { trace, model, tools, clock, unit: String::new(), entry: String::new() }
    }

    /// Mock context from a fixtures folder (`model.json`, `<tool>.<source>.json`); clock from `"today"`.
    pub fn mock(fixtures_dir: impl AsRef<Path>, trace: Trace) -> Result<Ctx, RtError> {
        let dir = fixtures_dir.as_ref();
        let model_path = dir.join("model.json");
        let model = if model_path.exists() { MockModel::from_file(model_path)? } else { MockModel::default() };
        let clock = model.today().map(Clock::fixed).unwrap_or_else(Clock::system);
        let tools = MockTools::load(dir)?;
        Ok(Ctx::new(trace, Arc::new(model), Arc::new(tools), clock))
    }

    /// A copy of this context labelled with the given unit name and entry heading.
    pub fn enter(&self, unit: &str, entry: &str) -> Ctx {
        Ctx { unit: unit.into(), entry: entry.into(), ..self.clone() }
    }

    /// Ask the model, then trace one `model` line (matched fixture prefix, or the prompt start).
    pub async fn ask_model(&self, prompt: &str, context: Option<Value>) -> Result<String, RtError> {
        let resp = self.model.complete(&ModelRequest::new(prompt)).await?;
        let prefix = resp.matched_prefix.unwrap_or_else(|| prompt.chars().take(60).collect());
        self.trace_model(&prefix, context);
        Ok(resp.text)
    }

    /// Trace a `model` line with the current labels.
    pub fn trace_model(&self, prompt_prefix: &str, context: Option<Value>) {
        self.trace.model(&self.unit, &self.entry, prompt_prefix, context);
    }

    /// Trace a `guard` line with the current unit label.
    pub fn trace_guard(&self, name: &str, pass: bool) {
        self.trace.guard(&self.unit, name, pass);
    }

    /// Trace an email tool call.
    pub fn trace_tool_email(&self, verb: &str, target: &str, to: &str) {
        self.trace.tool_email(verb, target, to);
    }

    /// Trace a crm tool call (`set` only for update).
    pub fn trace_tool_crm(&self, verb: &str, source: &str, id: &str, set: Option<Value>) {
        self.trace.tool_crm(verb, source, id, set);
    }

    /// Trace an http tool call (`body` if any).
    pub fn trace_tool_http(&self, verb: &str, url: &str, body: Option<Value>) {
        self.trace.tool_http(verb, url, body);
    }

    /// Trace a log tool call.
    pub fn trace_tool_log(&self, message: &str) {
        self.trace.tool_log(message);
    }

    /// Trace a served http request.
    pub fn trace_http(&self, method: &str, path: &str, status: u16) {
        self.trace.http(method, path, status);
    }

    /// Trace a `print` statement.
    pub fn trace_print(&self, message: &str) {
        self.trace.print(message);
    }

    /// Trace an `error` line with the current labels.
    pub fn trace_error(&self, message: &str) {
        self.trace.error(&self.unit, &self.entry, message);
    }
}
