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
- `OpenAiModel::from_env(model)` (`OPENAI_API_KEY`), `::new(key, model)`, `.with_base_url(url)`; `LocalModel::new(base_url, model, Option<key>)`. Both post a Chat Completions request to `{base}/chat/completions`.
- `Driver::parse("claude" | "claude:<m>" | "openai:<m>" | "local:<m>" | "mock" | "mock:<path>")`, `DriverError` (`azure`/`bedrock` -> `NotSupported`), `request_model_id(driver)`, `DRIVER_HELP`, `PROVIDERS`.
- `Router::from_env(ProviderConfig)` / `Router::new(config, getenv)`: a `ModelClient` that picks the provider per request (`AGENTMARK_MODEL`, then `AGENTMARK_ENV` through `ProviderConfig.environments`, then the request's model id); `.forced()`, `.mock_today()`, `.prepare(&Driver)`. See OPEN_QUESTIONS Q240-Q242.

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
- `ctx.with_model(id) -> Ctx` - copy that asks `id` (sent as `ModelRequest.model`); `Ctx.model_id` holds it. `MockModel` responses in model.json may carry an optional `"model"`: such a response only answers requests made for exactly that model id (a way to assert which agent used which model).

## Collection words (step 21, `agentmark_rt::collections`)
- `sorted(Vec<(K, T)>, descending) -> Result<Vec<T>, RtError>` - stable sort by key; equal keys keep their order in both directions. `K: SortKey` (`i64`, `String`, `bool`, `NaiveDate`, `DateTime<Utc>`, `Duration`, `Money`).
- `extreme(Vec<K>, max) -> Result<Option<K>, RtError>` - smallest or largest, the first of equals; `None` when empty.
- `sum_money(Vec<Money>) -> Result<Money, RtError>` - one currency; two currencies are `RtError::Money(CurrencyMismatch)`; an empty list is `0 EUR`. Money keys of different currencies also make `sorted` and `extreme` fail instead of guessing.
- `unique(Vec<T: PartialEq + Serialize>) -> Vec<T>` - first occurrence kept; linear (values are compared by their JSON form).

## Built-in library (step 22, SPEC 22.3)
- `agentmark_rt::stdlib::{text, regex, number, money, date, path, json, csv, file}` plus `stdlib::read_input()`. Pure functions take `&str`/values and return values; the ones the language types as `result of T` (`number::parse`, `money::parse`, `date::parse`, `json::parse::<T>`, `csv::parse::<T>` -> `Vec<T>`) return `Result<_, RtError>` with `RtError::Lib(plain message)`; `regex::*`, `number::{round,min,max,abs}`, `json::encode`, `csv::encode` return `Result` too (bad pattern, currency mismatch, too large, unencodable) and generated code writes `?`.
- `number::{round,min,max,abs}` are generic over `number::Num` (`i64` and `Money`); `date::format` over `date::DateLike` (`NaiveDate`, `DateTime<Utc>`); patterns: strftime when they contain `%`, else `YYYY MM DD HH mm ss MMM MMMM ddd dddd`, `[literal]` (`date::to_strftime`, `date::check`, `regex::check` give the compiler its compile-time check).
- `stdlib::file::{read, write, append, list, rename, exists, delete}(ctx, …)`: the built-in `file` tool; each call is traced first with `ctx.trace_file(verb, path, to)` -> `{"kind":"file","verb":…,"path":…}` (`to` only for rename). `Trace::file` is the typed method.
