# agentmark-rt public API (for the code generator)

All items are re-exported at the crate root (`use agentmark_rt::*;`). Every fallible call returns `Result<_, RtError>`; this includes all `Trace` methods and `Ctx::trace_*` (write failures are returned, never swallowed: use `?`).

## Ctx and clock
- `Ctx::new(trace: Trace, model: Arc<dyn ModelClient>, tools: Arc<dyn Tools>, clock: Clock) -> Ctx` - bundle the parts.
- `Ctx::mock(fixtures_dir, trace: Trace) -> Result<Ctx, RtError>` - `--mock --fixtures <dir>`: loads `model.json` + `<tool>.<source>.json`; clock = model.json `today`, which is required (error if missing).
- `ctx.enter(unit: &str, entry: &str) -> Ctx` - clone with trace labels (`unit` = agent/service/command, `entry` = heading of the entry that started the run).
- `ctx.ask_model(prompt: &str, context: Option<Value>).await -> Result<String, RtError>` - call the model, then trace one `model` line; `prompt_prefix` is the matched fixture key for the mock, and for a real provider the first 60 characters of the prompt.
- `ctx.tools.call(tool, verb, &args_json).await -> Result<Value, RtError>` - `list`/`get` return fixture data (args `{"source","id"}`), other verbs return null. Not traced.
- `ctx.clock.today() -> NaiveDate`, `ctx.clock.now() -> DateTime<Utc>`; `Clock::system()`, `Clock::fixed(date)`.
- Trace helpers (use `ctx.unit`/`ctx.entry`): `trace_model(prefix, context)`, `trace_guard(name, pass)`, `trace_tool_email(verb, target, to)`, `trace_tool_crm(verb, source, id, set: Option<Value>)`, `trace_tool_http(verb, url, body: Option<Value>)`, `trace_tool_log(message)`, `trace_http(method, path, status)`, `trace_print(message)`, `trace_error(message)`.

## Trace
- `Trace::to_file(path)`, `Trace::stdout()`, `Trace::memory()`, `Trace::from_writer(Box<dyn Write + Send>)`; `trace.lines() -> Vec<String>` (memory only); `trace.record(&Value) -> Result<(), RtError>`.
- Typed per-kind methods mirror DESIGN 5.1 exactly (the `Ctx::trace_*` helpers above call them).

## Model
- `trait ModelClient { async fn complete(&self, &ModelRequest) -> Result<ModelResponse, RtError> }`; `ModelRequest::new(prompt)`; `ModelResponse { text, matched_prefix }`.
- `MockModel::from_file(path)`, `MockModel::new(today, Vec<(prefix, response)>)`, `.today()`; longest-prefix match.
- `AnthropicModel::from_env(model_id)` (reads `ANTHROPIC_API_KEY`), `AnthropicModel::new(key, model_id)` (120s request timeout; non-2xx status becomes `RtError::Model` including the status).

## Tools
- `trait Tools { async fn call(&self, tool, verb, &Value) -> Result<Value, RtError> }`; `MockTools::load(dir)`, `MockTools::empty()`.

## Money
- `Money { minor: i64, currency: Currency }`, `Money::new`, `"1200.00 EUR".parse::<Money>()`, `Display`, serde as string.
- `checked_add`, `checked_sub` (-> `MoneyError::CurrencyMismatch`/`Overflow`), `mul_int(i64)`, `div_int(i64)`, `mul_div(num, den)` (all round half away from zero), `try_cmp(&Money) -> Result<Ordering, MoneyError>`; `<`/`>` are false across currencies.
- `Currency::{EUR, USD, GBP, CHF, JPY}`.

## Guards
- `no_amounts_changed(output_text: &str, input_json: &Value) -> bool` - fails closed: any number next to a currency code or symbol (`EUR 5`, `5 EUR`, `€5`) must parse exactly and appear in the input, else false; `extract_money(text) -> Vec<Money>` - lenient, skips ambiguous amounts.

## Scheduler
- `Schedule::every(chrono::Duration)`, `Schedule::every_day_at(hour, minute)`, `Schedule::weekly(weekday, hour, minute)` -> `Result<Schedule, RtError>` (interval > 0, hour < 24, minute < 60); `schedule.next_fire(after: NaiveDateTime) -> NaiveDateTime` (pure, strictly after `after`; pass the previous fire time).
- `run_schedule(schedule, || async { ... }).await` - loop forever on tokio; `run_once(|| async { ... }).await` - `--once` mode.
- `Weekday` is re-exported from chrono.

## Helpers for generated code (step 6)
- `ctx.ask_model_about(prompt, subject: Option<&Value>, schema_hint: Option<&str>).await -> Result<String, RtError>` - `ask_model` with the prompt followed by a blank line and the subject as compact JSON (without a subject the prompt is just the literal) and an optional answer-format hint; the `model` trace line reports `context` as `{"id": subject.id}` (the subject itself when it has no `id`), none without a subject. The matched fixture prefix is still the prompt's start.
- `parse_model_json::<T>(text) -> Result<T, RtError>` - parse a model answer as JSON (tolerates a ```json fence or prose around the object); `RtError::Model` when it does not fit `T`.
- `read_data::<T>(path) -> Result<T, RtError>` - `read file p as T`: `.csv` (header row = field names; cells are text, numbers/booleans only if the type refuses text) or JSON.
