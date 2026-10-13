# agentmark-rt public API (for the code generator)

All items are re-exported at the crate root (`use agentmark_rt::*;`). Every fallible call returns `Result<_, RtError>`.

## Ctx and clock
- `Ctx::new(trace: Trace, model: Arc<dyn ModelClient>, tools: Arc<dyn Tools>, clock: Clock) -> Ctx` - bundle the parts.
- `Ctx::mock(fixtures_dir, trace: Trace) -> Result<Ctx, RtError>` - `--mock --fixtures <dir>`: loads `model.json` + `<tool>.<source>.json`; clock = model.json `today`.
- `ctx.enter(unit: &str, entry: &str) -> Ctx` - clone with trace labels (`unit` = agent/service/command, `entry` = heading of the entry that started the run).
- `ctx.ask_model(prompt: &str, context: Option<Value>).await -> Result<String, RtError>` - call the model, then trace one `model` line.
- `ctx.tools.call(tool, verb, &args_json).await -> Result<Value, RtError>` - `list`/`get` return fixture data (args `{"source","id"}`), other verbs return null. Not traced.
- `ctx.clock.today() -> NaiveDate`, `ctx.clock.now() -> DateTime<Utc>`; `Clock::system()`, `Clock::fixed(date)`.
- Trace helpers (use `ctx.unit`/`ctx.entry`): `trace_model(prefix, context)`, `trace_guard(name, pass)`, `trace_tool_email(verb, target, to)`, `trace_tool_crm(verb, source, id, set: Option<Value>)`, `trace_tool_http(verb, url, body: Option<Value>)`, `trace_tool_log(message)`, `trace_http(method, path, status)`, `trace_print(message)`, `trace_error(message)`.

## Trace
- `Trace::to_file(path)`, `Trace::stdout()`, `Trace::memory()`; `trace.lines() -> Vec<String>` (memory only); `trace.record(&Value)`.
- Typed per-kind methods mirror DESIGN 5.1 exactly (the `Ctx::trace_*` helpers above call them).

## Model
- `trait ModelClient { async fn complete(&self, &ModelRequest) -> Result<ModelResponse, RtError> }`; `ModelRequest::new(prompt)`; `ModelResponse { text, matched_prefix }`.
- `MockModel::from_file(path)`, `MockModel::new(today, Vec<(prefix, response)>)`, `.today()`; longest-prefix match.
- `AnthropicModel::from_env(model_id)` (reads `ANTHROPIC_API_KEY`), `AnthropicModel::new(key, model_id)`.

## Tools
- `trait Tools { async fn call(&self, tool, verb, &Value) -> Result<Value, RtError> }`; `MockTools::load(dir)`, `MockTools::empty()`.

## Money
- `Money { minor: i64, currency: Currency }`, `Money::new`, `"1200.00 EUR".parse::<Money>()`, `Display`, serde as string.
- `checked_add`, `checked_sub` (-> `MoneyError::CurrencyMismatch`/`Overflow`), `mul_int(i64)`, `div_int(i64)`, `mul_div(num, den)` (all round half away from zero), `try_cmp(&Money) -> Result<Ordering, MoneyError>`; `<`/`>` are false across currencies.
- `Currency::{EUR, USD, GBP, CHF, JPY}`.

## Guards
- `no_amounts_changed(output_text: &str, input_json: &Value) -> bool`, `extract_money(text) -> Vec<Money>`.

## Scheduler
- `Schedule::{Every(chrono::Duration), EveryDayAt{hour, minute}, Weekly{weekday, hour, minute}}`; `schedule.next_fire(now: NaiveDateTime) -> NaiveDateTime` (pure, strictly after `now`).
- `run_schedule(schedule, || async { ... }).await` - loop forever on tokio; `run_once(|| async { ... }).await` - `--once` mode.
- `Weekday` is re-exported from chrono.
