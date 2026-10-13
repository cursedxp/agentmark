# Open questions

Each entry: the question, the choice applied, and why. SPEC.md is never edited.

## Q1 — Where do `tests/errors/` and `tests/docs.rs` live?
**Choice:** the repository root is both the `agentmark` package and the workspace, so these paths are literal.
**Why:** PROMPT.md names them at the root; a virtual workspace would move them under `crates/`.

## Q2 — Which `## Rust:` blocks are visible where?
In example 05, `## Rust: fingerprint` sits after `# Function: net_amount` but is called from `# Service: api`.
**Choice:** a `## Rust: <name>` block defines a module-level function named `<name>`; its parent declaration only groups it.
**Why:** it is the only reading under which example 05 resolves.

## Q3 — `stop` inside `for each` (example 03)
**Choice:** follow SPEC 4.1 literally — `stop` ends the current block (the whole entry), not just the iteration.
**Why:** the spec defines it so; fixtures for 03 are written to that semantics.

## Q4 — Email target `customer`
SPEC 4.3 writes `email send customer`, the record has `customer` (a name) and `customer_email`, and the expected trace has `"to":"ap@mueller.de"`.
**Choice:** a target word `w` resolves to field `w_email` of the in-scope record if it exists, else field `w`.
**Why:** matches the given fixture without changing the example.

## Q5 — crm target words vs. sources
`crm update invoice` names a target `invoice` while the manifest lists `sources = ["invoices", …]`.
**Choice:** a crm target word is the singular of a listed source; trace reports `source` as the plural and `id` as the in-scope record's `id`.

## Q6 — Trace schema
Only one expected trace exists. **Choice:** the exact per-kind schema in DESIGN.md §5.1, derived from that fixture; `block` is the heading of the entry that started the run.

## Q7 — Money rounding
**Choice:** `money * int / int` rounds half away from zero at the currency's minor unit.
**Why:** commercial rounding is what an invoice reader expects; the spec is silent.

## Q8 — `bytes` type
SPEC 15.4 uses `-> bytes` but section 6 has no `bytes`. **Choice:** add `bytes` (`Vec<u8>`) as a built-in type used by bindings.

## Q9 — Commands that use `state`
Example 05's `import-csv` appends to `state.invoices` but a `# Command` has no `state` property.
**Choice:** a Command may use the `state` record of the project's single `# Service`; in mock runs the state is loaded from `fixtures/<name>/state.<field>.json` when present, else empty.

## Q10 — Binder metadata without nightly
rustdoc JSON needs nightly. **Choice:** `binder/rust` runs `cargo +nightly rustdoc` when available, else reads `AGENTMARK_RUSTDOC_JSON`; tests use checked-in trimmed rustdoc JSON so `cargo test` works on stable and offline.

## Q11 — Errors that are semantic, listed under step 2
Step 2 asks for error files covering unknown tool, effect violation, unit mismatch, … which need the semantic pass.
**Choice:** step 2 creates all `tests/errors/*.agent.md` files and the diagnostic renderer; syntax errors pass in step 2, semantic ones are switched on in step 3.

## Q12 — `ModelClient::complete` return type (step 5)
DESIGN 5 says `Result<String>`, but the trace's `prompt_prefix` is "the fixture key that matched", which only the mock knows.
**Choice:** `complete(&ModelRequest) -> Result<ModelResponse { text, matched_prefix }>`; `Ctx::ask_model` returns only the text and traces `matched_prefix`, falling back to the first 60 chars of the prompt for real providers.
**Why:** keeps the trait provider-neutral while letting the mock report the matched key.

## Q13 — `no_amounts_changed` input type (step 5)
Brief says `input_json`. **Choice:** it takes `&serde_json::Value`; money values are every `"<n> <CUR>"` found in any string anywhere in the value. Amounts compare by minor units, so `1200 EUR` equals `1200.00 EUR`.
**Why:** the guard's input is the in-scope record serialized to JSON; a generator can `serde_json::to_value` it.

## Q14 — Money text grammar and currencies (step 5)
SPEC lists no currencies. **Choice:** EUR, USD, GBP, CHF (2 digits), JPY (0 digits); parse accepts an optional `-`, digits, optional `.` plus 1..=exponent digits, one space, upper-case code; more decimals than the minor unit is an error, not a silent rounding. Text extraction skips `12 EURO`, and a number glued to a letter or digit.
**Why:** silent rounding on parse would change amounts a guard is meant to protect.

## Q15 — Scheduler time base and defaults (step 5)
**Choice:** schedules use naive local time (no DST handling); `Every` fires `interval` after the previous tick; `run_schedule` takes a closure, one spawn per entry is the generator's job. `AnthropicModel` defaults to `max_tokens` 1024 and takes the model id from the caller (the generator maps `claude` / `claude:<model>`).
**Why:** DST-exact local scheduling is not in the v0.1 contract; keeps the runtime small.

## Q16 — Where do statements start under a `##` heading?
SPEC 1 says "indented lines under a block are statements", but every example writes them at column 0 directly under the heading (`## Do remind` / `- ask model …`).
**Choice:** statements sit at indentation 0 under a `##` heading; nesting adds one 2-space level per `for each` / `if` / `else` / `otherwise` / `parallel` body. Function bodies are the exception SPEC 13.1 shows: they are indented under the `- effect:` property.
**Why:** it is the only reading under which all five examples parse.

## Q17 — Which `>` comments are documentation?
**Choice:** a run of `>` lines directly above a `#`/`##` heading (no blank line between) is that heading's `doc`. A run at the top of the file followed by a blank line is the file's `doc`. Any other `>` line is a plain comment; it is kept on the next statement, field or property (Q28).
**Why:** SPEC 18.7 says "immediately before"; example 05 opens with a two-line file description followed by a blank line and then an import.

## Q18 — `else:` and `otherwise:` placement
SPEC 14 writes `- else:` with a dash and `otherwise:` bare, indented under the `try` line.
**Choice:** `else:` attaches to the `if`/`if … is some` directly before it at the same indentation, with or without a leading `- `; `otherwise:` must be the (only) child of a `try` line. An `else:`/`otherwise:` anywhere else is a syntax error.

## Q19 — Subtraction and hyphens
Names may contain `-` (SPEC 1), dates are `2026-10-13`, and `->` introduces a result variable.
**Choice:** inside expressions identifiers have no `-` (only declaration names, tool names and `# Run` items do), and binary `-` needs a space on both sides (`today - 1 month`). `2026-13-01` is therefore a bad date, never silent arithmetic.

## Q20 — Money literals
**Choice:** `12.50 EUR` is parsed to integer minor units (1250) using the currency's exponent (2 for most, 0 for JPY/KRW/…, 3 for KWD/…). More decimals than the exponent allows (`12.505 EUR`) and any decimal without a currency (`3.5`) are syntax errors, so no float or silent rounding can enter the AST.

## Q21 — `do` with a tool call (SPEC 13.3)
`- do crm get invoice id -> inv` and `- do crm update invoice: inv` appear in the Service example of 13.3.
**Choice:** parsed as the ordinary `ToolCall` statement with `via_do: true`; `do name`, `do name -> x` and `do name(args) -> x` are `Do`. The two are told apart by whether a verb word follows the first name.

## Q22 — Tabs, odd indentation, error policy
**Choice:** a tab anywhere on a line outside a `>` comment or a code fence is error E001; indentation that is not a multiple of 2 is E002; a line that is neither heading, comment, `- ` item, fence nor a bare control line (`…:` / `stop`) is E004. Lexer errors are returned alone (the structure cannot be trusted after dropping lines); parser errors are all collected, one per bad line, and a property that was rejected suppresses the follow-on "missing property" report.

## Q23 — Semantic error files fix the messages step 3 must produce
`tests/errors/*.agent.md` for unknown tool/verb/target, unlisted tool, effect violation in a rule, unit mismatch, missing `maybe` and unknown field are `#[ignore]`d; their headers are the contract. Each file contains exactly one mistake and its header lists exactly one diagnostic (so `unknown-tool` must not also report "not listed"). Choices made here: effect violation = a `# Rule` calling a `network` function (`live_status(invoice)`); missing `maybe` = a function whose `out:` is `Invoice` but returns `first … where …`; unit mismatch = `amount + 3 days`. If step 3 finds a message impractical it may edit the header, but must keep the SPEC 10 shape.

## Q24 — `## Not bound` items
SPEC 15.2 says skipped items are listed "with the reason" but gives no line format.
**Choice:** `- item_name: reason`; the reason is optional.

## Q25 — Foreign `## Run` block name
A `# Command` takes exactly one `## Run` block (plus any `## Rust: name` blocks, which are hoisted); any other `##` heading in a command is an error.

## Q26 — Did-you-mean distance
SPEC 10 says "within edit distance 2" without naming the metric.
**Choice:** Damerau-Levenshtein (a swapped pair of letters is one edit, as in `sned`/`send`), distance at most 2 and strictly less than the number of letters in the typed word, so a one-letter name is never "corrected" into an unrelated one. Exact matches are not suggestions; ties go to the first candidate (plural unit words are listed first, so `minuts` suggests `minutes`).

## Q27 — Which typos the parser itself suggests
The parser knows no tool or record names, so it only suggests from closed keyword sets: block headings (`Every`, `When`, `On`, `Do`, `Rust`), statement words, the word after `ask`, time units, type constructors (`maybe`, `list`, `result`), HTTP methods and declaration kinds. Tool, verb, target and field typos belong to the semantic pass.

## Q28 — Plain comments in the AST
**Choice:** `>` lines that are not documentation are kept as `comments` on the next statement, record field, or declaration property (`prop_comments`), so `fmt` can preserve them and `lint` (`unnamed-guard`) can see them. Comments with nothing after them in their declaration are dropped.

## Q29 — Names with hyphens
**Choice:** declaration names, rule names (`for each very-overdue invoice`, `count of very-overdue invoices`), record names and type names accept `-`; variables, fields and paths inside expressions do not (Q19).

Note: my entries Q16-Q25 (and Q26-Q29) were numbered from Q16 because the runtime branch used Q12-Q15.

## Q30 — What does a `## Do` block work on? (step 3)
Example 01: `## Every day at 09:00` runs `for each overdue invoice in crm.invoices: - do remind`, and `## Do remind` writes `ask model with invoice: …`, `{customer}`, `{amount}` although it has no parameter. DESIGN 4.1 lowers it to `Call(invoice_chaser__remind, invoice)`.
**Choice:** a `## Do` block receives the record variable that is "in front of" its caller — the innermost `for each` record, the event record of `## When tool receives event`, or none. The first caller decides; any other caller with a different variable or record is an error ("works on `i` (Invoice), but here it is called with `l` (Lead)"). A `## Do` block nobody calls is only a warning (unused action) and its unresolvable names are not reported. `Checked.blocks[..].param` records the choice for `agent_lower`.
**Why:** it is the only reading under which example 01 resolves; deciding by call sites keeps the source free of parameter lists.

## Q31 — Declared names are unique per project (step 3)
Types carry a record by name (`Type::Record("Invoice")`), generated Rust has one namespace per kind, and scopes only decide visibility.
**Choice:** two declarations of the same kind and name anywhere in the project are an error at the second ("already declared in file:line"), even if no file imports both.
**Why:** keeps `Type` and the side tables simple and avoids two `Invoice` types silently comparing equal.

## Q32 — Field-type inference reads whole words (step 3)
SPEC 2.1 says "names ending in `date`, `at`, `on`"; letter-wise `reason`, `format`, `description` would become dates.
**Choice:** the rule applies to the last word of the name, words separated by `_` or `-` (`due_date`, `created_at`, `paid_on`, `date`, `total_amount`). Only the fallback to `text` (no rule matched) is a warning (W001), as SPEC 2.1 says "inference failure"; successful inference is silent but `FieldInfo.inferred` is true.

## Q33 — "Statements per block" counts nested statements (step 3)
**Choice:** every statement inside the block counts, nested ones included (a `for each` with 25 statements inside is 26). The suggested fix names the lines from the top-level statement that contains the 25th statement to the end of the block.
**Why:** the limit exists so a block fits on a screen; counting only the top level would let any block hide behind one `for each`.

## Q34 — "Declarations per file" (step 3)
SPEC 18.3: "1 of `# Agent` / `# Service` / `# Command`; records and rules may share a file only in `records/` and `rules/`". Example 05 holds a Service **and** a Command in one file, and examples 02–05 put records, rules and an agent into one file; all must check.
**Choice:** at most one agent, at most one service and at most one command per file (05 passes). The record/rule placement limits apply to **folder projects** only (several records in one file only under `records/`, several rules only under `rules/`, records and rules never in the same file); a single-file project is a whole program in one file and is exempt.

## Q35 — Line length ignores the text of string literals (step 3)
Examples 02 and 03 contain lines of 133 and 129 characters (long model prompts); all examples must check with zero errors.
**Choice:** for the 120-character limit the text between the quotes of a string literal does not count (the quotes do); everything else, `>` comments and fenced Rust included, counts in full. The error's caret starts at column 121.
**Why:** a prompt is prose that cannot be broken over lines; the limit stays meaningful for code.

## Q36 — Crate import without a binding file (step 3)
Example 05 has `# Import: crate sha2` and no binding file (the binder comes in step 8).
**Choice:** the import is known but unbound: warning W004 ("has no binding yet… run `agentmark bind sha2`"), never an error; `## Rust` blocks may use the crate. A crate import is "unused" (W002) when neither a binding nor any `## Rust` block / binding body of the file mentions `crate::`. Binding files are looked up at `bindings/<crate>.agent.md`, then `.agentmark/bindings/<crate>.agent.md` (hyphens also tried as underscores). Bindings are not tools: an agent need not list them in `tools:`.

## Q37 — Module-qualified names (step 3)
`# Import: ./billing as billing` makes names `billing.overdue`. The parser accepts qualified names in expressions (calls, constants) but only simple names in type positions, `for:`, `as`, and rule filters.
**Choice:** sema resolves every qualified name it is given by looking up the full dotted name in the file's scope (so `billing.overdue(i)` works); a file that needs a record from an aliased folder imports the record file plainly as well. Fixing the parser to accept `billing.Invoice` in types is a step-2 follow-up, not done here.

## Q38 — Tools in services, commands and functions (step 3)
SPEC 2.3 restricts tool calls to the agent's `tools:` line, but `# Service`, `# Command` and `# Function` have no such line.
**Choice:** the restriction applies to agent blocks (tool calls and tool-source reads). Services, commands and functions may use any tool of the manifest; a function's effect declaration is what bounds it.

## Q39 — Built-in effect sources (step 3)
**Choice:** `ask model` is class `model`; `print`, `read file` and `append … to state.…` are class `local` with the sources `print`, `file`, `state`; `append` to a local list is pure; `respond` / `read body` are pure. A tool read such as `crm.invoices` has the tool's manifest effect.

## Q40 — Other choices made by the semantic pass (step 3)
- `maybe T` is stored by a plain `T` (wrapping), never the other way; `x = y` accepts `maybe T` against `T`.
- `+` on text is an error (use interpolation); `money / money` and `money * money` are errors; `date - date` and `datetime - datetime` are durations; currency mismatch is reported only when both sides are written literals (everything else is checked at run time, DESIGN 3).
- `stop` inside a function and `return` outside one are errors; a function with `out:` and no `return` is an error.
- A tool's manifest `effect` may be `local`, `network` or `model`, never `pure`.
- `agentmark.toml` unknown keys are errors with did-you-mean (keys: `name`, `version`, `targets`, `lint`).
