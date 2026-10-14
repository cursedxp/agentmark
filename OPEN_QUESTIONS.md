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
**Choice:** a `## Do` block receives the record that is "in front of" its caller — the innermost `for each` record, the event record of `## When tool receives event`, or none. The first caller's variable name is the block's parameter name (`Checked.blocks[..].param`); other callers may use a different variable name as long as the **record** is the same (`for each j in crm.invoices: do show` after `for each i …` is fine). A different record is an error ("works on `i` (Invoice), but here it is called with `l` (Lead)"). A `## Do` block nobody calls is a warning (unused action); it is checked leniently: a bare name passes if some record has a field of that name, otherwise it is reported.
**Why:** it is the only reading under which example 01 resolves; deciding by call sites keeps the source free of parameter lists.

## Q31 — Declared names are unique per project (step 3)
Types carry a record by name (`Type::Record("Invoice")`), generated Rust has one namespace per kind, and scopes only decide visibility.
**Choice:** two declarations of the same kind and name anywhere in the project are an error at the second ("already declared in file:line"), even if no file imports both.
**Why:** keeps `Type` and the side tables simple and avoids two `Invoice` types silently comparing equal.

## Q32 — Field-type inference reads whole words (step 3, approved by the controller)
SPEC 2.1 says "names ending in `date`, `at`, `on`"; letter-wise `reason`, `format`, `description` would become dates.
**Choice:** the rule applies to the last word of the name; words are separated by `_`, `-` or a lower-to-upper case change (`due_date`, `createdAt`, `paid_on`, `date`, `totalAmount`). Only the fallback to `text` (no rule matched) is a warning (W001), as SPEC 2.1 says "inference failure"; successful inference is silent but `FieldInfo.inferred` is true and every inferred field (with its type and whether it fell back) is in `Checked.inferred_fields` so `check` can print them. DESIGN §3 was amended to say the same.

## Q33 — "Statements per block" counts nested statements (step 3)
**Choice:** every statement inside the block counts, nested ones included (a `for each` with 25 statements inside is 26). The suggested fix names the lines from the top-level statement that contains the 25th statement to the end of the block.
**Why:** the limit exists so a block fits on a screen; counting only the top level would let any block hide behind one `for each`.

## Q34 — "Declarations per file" (step 3)
SPEC 18.3: "1 of `# Agent` / `# Service` / `# Command`; records and rules may share a file only in `records/` and `rules/`". Example 05 holds a Service **and** a Command in one file, and examples 02–05 put records, rules and an agent into one file; all must check.
**Choice:** a **project folder** applies the row literally: at most ONE of Agent/Service/Command per file in total (an error at the second), several records in one file only under `records/`, several rules only under `rules/`, records and rules never in the same file. A **single-file project** is a whole program in one file and is exempt from the whole row; when the file holds more than one runnable declaration it gets one warning (W006, "single-file draft; split into a project folder").

## Q35 — Line length ignores the text of string literals (step 3)
Examples 02 and 03 contain lines of 133 and 129 characters (long model prompts); all examples must check with zero errors.
**Choice:** for the 120-character limit the text between the quotes of a string literal does not count (the quotes do). `>` comment lines and fenced Rust count in full. When the counted length is over 120 the error's caret starts at the first counted character past the limit; when only string text pushes the raw line past 120 there is a warning (W007) suggesting a `# Constant`.
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
**Choice:** `ask model` is class `model`; `print`, `read file` and `append … to state.…` are class `local` with the sources `print`, `file`, `state`; **reading** `state` is pure (it is the program's own memory, like a local variable); `append` to a local list is pure; `respond` / `read body` are pure. A tool read such as `crm.invoices` has the tool's manifest effect.

## Q40 — Other choices made by the semantic pass (step 3)
- `maybe T` is stored by a plain `T` (wrapping), never the other way; `x = y` accepts `maybe T` against `T`.
- `+` on text is an error (use interpolation); `money / money` and `money * money` are errors; `date - date` and `datetime - datetime` are durations; currency mismatch is reported only when both sides are written literals (everything else is checked at run time, DESIGN 3).
- `stop` inside a function and `return` outside one are errors; a function with `out:` and no `return` is an error.
- A tool's manifest `effect` may be `local`, `network` or `model`, never `pure`.
- `agentmark.toml` unknown keys are errors with did-you-mean (keys: `name`, `version`, `targets`, `lint`).

## Q41 — Resolution order and partial record literals (step 3 review)
**Choice:** a name resolves as variable, then bare field of the record in front of the block, then constant, and only with a dot as a tool source (`crm.invoices`); a lone tool name is never a value, so a field called `email` is a field. A record literal must give every field that is not `maybe`, and every key must exist (misspelled keys get a did-you-mean). Plain-value payloads are checked: text tools (`email`, `log`) take text, a crm-like tool takes the target's record; binding arguments are checked against the bound function's parameters. More than one service with `state:` is an error (commands use it, Q9).

## Q50 — `fmt` and the heading text of a block (step 7)
`Block::label` keeps the heading exactly as written ("used as the trace label"), but the formatter prints the heading from the parsed `Trigger` (`## Every day at 09:00`, weekday capitalised, `1 hour`).
**Choice:** a non-canonical heading (`## every  MONDAY at 08:00`) is rewritten, so its `label` (and the trace label) changes with it. Canonical headings, which is every one in `examples/`, keep their label. The AST-equivalence tests therefore ignore `label` and spans.
**Why:** a formatter that kept the typed heading could not be canonical; the trace label follows the file, as it always did.

## Q51 — Canonical spelling the AST cannot tell apart (step 7)
The AST has one value for several spellings. **Choices:** statements are always list items (`- for each ...:`, `- if ...:`, `- else:`; examples 01 and 03 write `for each` bare); `otherwise:` stays the one bare control line (SPEC 14); money prints with decimals only when not whole (`12.50 EUR`, `14 EUR`, `1200 JPY`); durations are singular for 1 (`1 day`); a record field without a type prints as `- name`; a raw `}` in a string is not escaped, `{` is; `do f()` prints as `do f`; imports sort by path inside their group (local files and globs, then folders, then crates), so `examples/01-invoice-chaser` imports are re-ordered; props print in the order the language documents them.
**Why:** one style needs one pick; SPEC 13.3 uses the dash form for control lines and the majority of its samples do.

## Q52 — Comments the parser drops (step 7)
The parser keeps doc comments and comments directly above a statement, field or property (Q17, Q28) but drops the rest: after the last line of a block/property list, above `else:`/`otherwise:`, above `# Run` items and `## Not bound` items, and before the first heading when not the file doc.
**Choice:** `fmt` finds them again from the lexer (same attach rules as the parser) and writes each one right after the closest printed line that precedes it in the source, at the smaller of its own and that line's indentation. Idempotent; no AST change. A comment at the end of a loop or `if` body is not dropped by the parser but attaches to the next statement of the enclosing level (Q17 pending-comment rule); `fmt` keeps that text as is. A comment that was dropped can be attached to the next statement after a re-parse of the output, which does not change the text.
**Why:** the brief says `fmt` must preserve all comments, and changing the AST would touch the reviewed parser and the other task's constructors.

## Q53 — Inline comment after an import (step 7)
SPEC 18.2 shows `# Import: ./rules/*.agent.md  > glob, imports every rule`, which the parser accepts and discards.
**Choice:** `fmt` re-reads that trailing `> ...` from the source line and prints it after two spaces.

## Q54 — Where do lint findings point, and how long is a file (step 7)
**Choice:** a finding's `code` is the rule id and its message ends with `[rule-id]`; severity is warning unless the rule is in `[lint] deny`. `long-file` counts source lines when called through `lint_source`; through `lint(&[File])` (the AST has no line count) it uses the last source line any AST node points at, so trailing blank/comment lines are not counted.

## Q55 — Reading of the soft rules (step 7)
- `magic-value`: counted per file; zero amounts (`0 EUR`) are exempt; `# Constant` values do not count; URLs are found inside plain and interpolated strings; trigger/`timeout` durations are properties, not inline literals.
- `unnamed-guard`: the length is that of the canonical text of the condition (`fmt`'s), longer than 60 characters; a `> comment` directly above the statement (or the `try` line) satisfies it.
- `duplicate-rule`: `when:` expressions are compared structurally across all files linted together, ignoring spacing and redundant parentheses (via the canonical text); the `for:` record is not compared (SPEC says "identical `when:`").
- `prompt-in-loop`: the result variable must appear in a later `guard` of the same loop body (branches included, nested loops excluded); an `ask model` in a nested loop is reported once, for its innermost loop.
- `missing-doc`: needs a `>` doc comment directly above the heading (SPEC 18.7); a file-level description does not count.
- `vague-name`: checks where a name is introduced (declaration names, fields, params, `set`, `-> var`, loop variables, `is some` bindings), once per name per declaration.

## Q56 — `[lint]` config errors (step 7)
**Choice:** `LintConfig::from_toml_str` reads only the `[lint] deny` table and ignores the rest of `agentmark.toml`; an unknown rule id is an error pointing at the entry, with a did-you-mean among the seven rules. Adds the `toml` dependency (the manifest task needs it too).

## Q70 — What a binding body is the body of (step 8, binder)
SPEC 15.4 shows `Ok(pdf.save_to_bytes()?)` but never says what function the body sits in.
**Choice:** every binding body is the body of `pub async fn <item>(<params as Rust types>) -> Result<<ret>, Box<dyn std::error::Error + Send + Sync>>`. AgentMark values arrive owned (text=`String`, int=`i64`, bool, bytes=`Vec<u8>`, date=`chrono::NaiveDate`, datetime=`chrono::DateTime<chrono::Utc>`, duration=`chrono::Duration`, list=`Vec`, maybe=`Option`); the body returns `Ok(value)` and may use `?` and `.await`. No `use` lines: bodies name the library by full path. The type checker compiles exactly this wrapper, so codegen can emit the same one.

## Q71 — rustdoc JSON parsed as `serde_json::Value`, one format version
`rustdoc-types` is not in the offline registry cache and its version has to equal the nightly's `format_version` anyway. **Choice:** `binder/rust` reads rustdoc JSON as `serde_json::Value`, touching only the fields it needs, and refuses any `format_version` other than 61 (today's nightly) with a message telling the user to regenerate, instead of guessing at a newer layout. Moving to a new nightly means re-running `tests/binder/regen.sh` and bumping `SUPPORTED_FORMAT` after checking the reader.

## Q72 — Which public things are "items"
SPEC says "every public function/method" but a crate's API also includes trait impls. **Choice:** items are public free functions (reachable through public modules and `pub use`), public inherent methods, and methods of trait impls on public types unless the trait is std plumbing (`Clone`, `Debug`, `Add`, `From`, ... list in `rustdoc.rs`); blanket, synthetic and negative impls are ignored; provided trait methods an impl does not override are not in rustdoc's impl list and are not seen. Public traits, type aliases of foreign types and re-exports of foreign items are not callable: they are listed under `## Not bound` (`trait_*`, `alias_*`, `reexport_*`). Constants and statics are not listed.

## Q73 — Opaque handle types cannot be bound; foreign-trait methods on local aliases are (sha2)
AgentMark has no type for `Client`, `Response`, a hasher state or `NaiveDateTime`. **Choice:** an item whose receiver, a parameter or the return type has no AgentMark type is skipped with that reason (listed, never dropped). `reqwest::get` returns `Response` -> skipped, so reqwest binds nothing. A builder-pattern chain (`Client::new().get(url).send()`) is the same problem seen from the other side: every link in it is an opaque handle, so the "builder patterns" row of SPEC 15.2 is unreachable unless the chain's start and end are AgentMark types; the binder does not invent a signature for it (that is the v0.2 on-demand tier).
sha2 is different: its hashing lives in `digest::Digest`, which sha2 re-exports (`pub use digest::Digest`) and whose implementors are sha2's public type aliases (`Sha256 = digest::CoreWrapper<..>`). **Choice:** when a crate re-exports a trait from crate C and has public type aliases whose target type is C's, the binder also reads C's rustdoc JSON (built in the same scratch crate: `cargo +nightly rustdoc -p digest`; `tests/binder/digest.json`) and, for each (alias, receiver-less trait method) pair, emits an item `<alias>_<method>` with `Self` = the alias and path `<sha2::Sha256 as sha2::Digest>::digest`, documented from the trait. Result: `sha256_digest(data: bytes) -> bytes` is an assisted item (model body type-checked against the real sha2). Methods with a `self` receiver (`update`, `finalize`, ...) need a hasher value and are one aggregated `## Not bound` line. That an alias really implements the trait is assumed from "the trait is re-exported next to an alias of the trait's crate's type" and verified only by the assisted body's `cargo check`. Foreign-trait items are always assisted (never direct), so their bodies are always type-checked. If the dependency's rustdoc cannot be had, the failure is written into the re-export's `## Not bound` line, not swallowed. Without nightly the dependency JSON can be supplied as `<crate>.json` in the directory named by `AGENTMARK_RUSTDOC_JSON_DIR`. The same rule applies to any crate with that shape; local types with local impls were already covered (Q72).

## Q74 — Item names and receivers
Binding item names must be unique within a `# Binding` and SPEC gives no rule. **Choice:** `<path below the crate, snake_case, joined by _>`: `NaiveDate::from_ymd_opt` -> `naive_date_from_ymd_opt`, free `retry::for_host` -> `retry_for_host`; a second item with the same name gets `_2`. A method's receiver becomes the first parameter, named after its type (`naive_date: date`). Parameters named `_x` lose the underscore; duplicates get a numeric suffix.

## Q75 — Closures stay skipped
SPEC 15.2 lists "closures with one argument" as assisted, but AgentMark has no function type to declare the closure parameter with, so no signature can be written for the model to implement. **Choice:** items taking a closure (`Fn*` bounds, `fn` pointers) are skipped with "takes a closure and AgentMark has no function type". Revisit if a function/rule-reference type is ever added.

## Q76 — Re-binding and hand edits
SPEC 15.3 re-marks an approved binding `unverified` on a version change, but says nothing about the user's edits. **Choice:** a stale binding is regenerated (new file is `unverified`, header has the new version); whenever an existing file is overwritten (stale or `--force`) the old one is first copied to `<crate>.agent.md.prev` next to it, so approvals and hand edits are recoverable. A file without the generated header (hand-written) counts as `Unmanaged`: left alone unless `force`. The `.prev` name does not match `*.agent.md`, so project loading ignores it.

## Q77 — Where the direct tier stops
SPEC: direct = "every parameter and the return type map 1:1, no generics, no closures". **Choice:** direct additionally requires: not `async`, not `Result`-returning (Result wrapping is the assisted row), receiver `self`/`&self` only, integers of `i8..i64`/`u8..u32` (converted with `try_from(..)?` / `i64::from`; `u64`, `usize`, `i128` are not 1:1 with `i64`), no `f32`/`f64` (money must stay integer minor units), strings/bytes by value or shared borrow, `Option`/`Vec` of those. `Option<T>` returns are direct (`maybe T`). Everything else that can still be declared in AgentMark types is assisted.

## Q78 — "Single obvious instantiation" table
Assisted generics need a rule for what the declared AgentMark type is. **Choice:** a generic parameter or `impl Trait` is instantiated from its bounds: `AsRef<str>`/`Into<String>`/`ToString`/`Display`/`IntoUrl`/`AsRef<Path>` -> `String` (text); `AsRef<[u8]>`/`Into<Vec<u8>>` -> `Vec<u8>` (bytes); `TimeZone` -> `chrono::Utc`. Marker bounds (`Send`, `Sized`, `Clone`, ...) are ignored; any other bound, or bounds that disagree, means no obvious instantiation and the item is skipped. `Result<T, _>`, `impl Future<Output = T>` and `async` unwrap to `T`; `Url`, `Uri`, `PathBuf` map to text and `GenericArray`/`Output` to bytes on the way out.

## Q79 — `## Not bound` lines name the AgentMark item, not the Rust path
Q24 fixes the line as `- item: reason`, and the parser splits at the first `:`, which a Rust path (`a::b`) contains. **Choice:** the item is the AgentMark item name (`naive_date_year`); the Rust path goes in the reason in parentheses. Reasons are flattened to one line.

## Q80 — Doc line when rustdoc has none
Trait-impl methods carry no docs in rustdoc JSON (the docs live on the trait). **Choice:** the `>` line above every bound item is the first sentence of the item's docs with markdown links flattened; when there is none it reads ``Calls `<library path>`.`` so the "doc line above every bound item" rule always holds.

## Q81 — Effect proposal inputs
SPEC 15.3 says "uses std::fs -> local, reqwest/tokio::net -> network, otherwise pure" without saying what is scanned. **Choice:** the crate name, the rendered signatures of every item (bound or not) and every generated body, against a small hint list per target (Rust: `reqwest`, `tokio::net`, `std::net`, `hyper`, ... -> network; `std::fs`, `tokio::fs`, `std::process`, `std::env`, `std::path::Path` -> local). Reading the clock (`Utc::now`) is not treated as local. It is a proposal only; the binding is born `unverified` either way.

## Q82 — Network and offline in the binder
The brief wants tests to work offline; real use may need to download a crate. **Choice:** every cargo call the binder makes (resolve version, build rustdoc JSON, type-check scratch crate) takes `--offline` iff `BindOptions::offline`/`FetchSpec::offline` is set; default is online. Fixtures were built with `default-features = false` for `reqwest` and `chrono` so they resolve from the offline cache; the public API of the fixtures is therefore the no-default-features API.

Q82 addendum: `AGENTMARK_RUSTDOC_JSON` (main crate) and `AGENTMARK_RUSTDOC_JSON_DIR` (dependencies) treat an empty value as unset everywhere (`ENV_JSON=""` used to disable one code path but not another). Dependency lookup order: files in `BindOptions::foreign_rustdoc`, the `foreign_fetch` hook, the JSON dir, then a build next to the main JSON (only when the binder built that itself and `AGENTMARK_RUSTDOC_JSON` is unset).
