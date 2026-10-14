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

## Q90 — Guards outside an agent (step 4)
DESIGN 4.1 lowers every `guard` to `set tmp; Trace; if not tmp`, but DESIGN 7 says `agent_lower` is the identity on example 05.
**Choice (controller, review):** guards trace uniformly everywhere (SPEC 4.4; the trace's `agent` field is the unit label of the agent, service or command). `agent_lower` expands guards in agent blocks; outside agents a `Guard` stays in the L2 AST (so 05 is unchanged) and the IR builder expands it to the same `Let guard_ok_<line>; Trace(Guard); If(not tmp){else | Stop}`. `ask model` is lowered everywhere (`ModelCall`).

## Q91 — L2 AST shape and synthetic nodes (step 4)
**Choice:** the L2 AST reuses `ast` types plus `Decl::Entry`, `StmtKind::ModelCall`, `StmtKind::Trace` (the DESIGN `Call{rt::model_complete}` cannot carry the schema record). `do <action>` becomes `do <agent>__<block>` (declared names, no sanitizing; codegen maps to Rust identifiers) with the caller's record as argument; rules become `Function`s; names created by lowering are unique (a rule sharing a name with a function becomes `<name>__rule`). Nodes lowering invents have `col == 0` spans (`lower::is_synthetic`), copied nodes keep theirs so `Checked` lookups work.

## Q92 — Agent knobs in the IR (step 4)
`timeout:` and `memory: file` are agent properties with no agent in the IR. **Choice:** they are plain fields of `Entry` (`timeout` default 5 minutes, `memory_file`); the model driver string is a field of each `ModelCall`.

## Q93 — Input of `no amounts changed in x` (step 4)
SPEC does not say what "input" is. **Choice:** the `with` value of the model call that produced `x`, else that call's prompt; if `x` did not come from a model call, `x` itself (the check is trivially true). The expression is re-evaluated at the guard.

## Q94 — `try … -> x` without an assigning `otherwise` (step 4)
Sema lets `x` be used after the `try` whatever `otherwise` does. **Choice:** IR `Try.defines` names `x`; if `otherwise` neither assigns nor stops, codegen uses the type's default value (records without a default are a codegen internal error). Not reachable from examples 01–05.

## Q95 — Binding payloads and unknown result types (step 4)
**Choice:** a binding call takes only its target words/values as arguments (sema ignores a payload there; the IR drops it). A tool result sema cannot type keeps `Type::Unknown` (JSON value).

## Q110 — Binder `## Not bound` lines versus the 120-character limit
Generated bindings list unbound items as `- name: reason (library::path)`; real reasons regularly exceed 120 characters, and SPEC 18.3 says the limits cannot be disabled (sema stays strict). SPEC 15.2 says the binder never silently drops anything, so text is never cut either. **Choice:** (1) a long reason continues on `  > ` comment lines under the entry, each at most 120 characters (`core/writer.rs::not_bound_entry`; a single word longer than a line is split, nothing lost); long rustdoc summaries wrap onto further `> ` doc lines. (2) Entries are one per item: `self` methods of a foreign trait get one entry per method and alias (`sha256_finalize_reset: takes \`self\`, AgentMark cannot hold the value (...)`), never a list. (3) Alias entries read `` `crate::path::Type<…>` from crate digest; its methods live there `` (generic arguments elided). (4) Model and type-check causes are kept whole (no length cap in `pipeline.rs`). The parser appends `  > ` comment lines (indented, directly under an entry) to that entry's reason, space-joined, so the reason is the whole text (this extends Q24); `fmt` re-wraps entries with the same function, so a generated binding is canonical byte for byte. Rustdoc summaries stay one `>` line, shortened at a word boundary with `…` over 118 characters. A canned model without an answer says "mock model has no response for <item>" (no prompt fragments). Tests: `tests/binder.rs` (every method its own entry, "from crate", long type-check error across continuation lines, no line over 120, files check clean).

## Q111 — `fmt` and `lint` skip `bindings/`
Generated bindings are rewritten by `agentmark bind` (and carry the lines Q110 describes); linting them reports `vague-name` for parameters like `data` that mirror the library, and formatting would only fight the binder. **Choice:** the folder walk of `fmt` and the file set of `lint` skip every path with a `bindings` component. `check` still loads and checks them. Passing one binding file to `fmt` by path formats it.

## Q112 — What `--allow-unverified` means on `check`
SPEC 15.3: `check` lists unverified bindings and the release build refuses them. The brief gives `check` the flag. **Choice:** `check` exits 1 while any binding is `unverified` (the listing says which line to change) unless `--allow-unverified` is passed; the listing is printed either way. Rationale: `check` is the CI gate, and an unapproved library must not pass it silently. Sema keeps W005 a warning; the gate is in the CLI. `build` (codegen task) reuses the same rule.

## Q113 — Exit codes and streams
**Choice:** 0 = ok; 1 = the user's project has problems (errors, denied lints, non-canonical files with `--check`, unapproved libraries, a bind that failed); 2 = the command itself cannot run (bad arguments, missing path for `fmt`, no model for `bind`). Diagnostics go to stderr in the SPEC 10 format; reports and the final count go to stdout. `check --json` prints only JSON on stdout. A path that does not exist, or a file that is not `.agent.md` (e.g. `tools.toml`), is exit 2 with one plain message for every command (`check`, `fmt`, `lint`, `doc`); `fmt` writes through a temp file and rename.

## Q114 — `doc <target>`: project or library?
`agentmark doc [<dir>]` (SPEC 18.7) and `agentmark doc <crate>` (brief) share one argument. **Choice:** if the argument exists on disk it is a project (file or folder); otherwise it is a library name looked up as `<project>/bindings/<name>.agent.md` (`--project`, default `.`; also `.agentmark/bindings/`). No argument = the current folder. A library whose name equals a local folder must be asked for with `--project`.

## Q115 — Model and clock for `bind`
**Amended: see Q240** (the binder honours `[models]` and `AGENTMARK_ENV` now).
**Choice:** `AGENTMARK_MODEL` selects the `ModelClient`: unset or `claude` = `AnthropicModel` with model id `claude-sonnet-4-5` and `ANTHROPIC_API_KEY`; `claude:<id>` picks the id; `mock:<path>` loads a `MockModel` `model.json` (tests, offline). Anything else (`openai:…`, `local:…`) is refused with a plain message until those drivers exist (SPEC 19). `AGENTMARK_OFFLINE=1` passes `--offline` to every cargo call of the binder. The header date is the mock's `today` when it has one, else the system date (UTC). The agent's own `- model:` line and `[models]` environments are not consulted: they describe the program, not the toolchain.

## Q116 — Effect summary lines for functions and rules
SPEC 8 only shows agent blocks (`invoice-chaser / Every day at 09:00: …`). **Choice:** agents, services and commands print `unit / heading: effects` (heading as written, `GET /invoices/{id}`, `Run`); functions and rules, which have no heading, print `function name: effects` / `rule name: effects`. In JSON both are `units[]` with `kind`.

## Q117 — `bind --all` and a binding file with another name
`bind` always writes `<crate>.agent.md`. For a file whose `- crate:` differs from its name, `--all` would create a second file. **Choice:** it refuses that entry with a message asking to rename the file (exit 1); the other bindings are still refreshed. `bind <crate>` and `--all` are mutually exclusive.

## Q130 — `respond` ends the block; a guard's `else` statement does not (step 6)
**Choice:** `respond` returns from the route at once. The IR's `guard … else s` is `if not ok { s }` with no implied `stop`, so a non-terminating `else` (a `log`, say) falls through to the next statement; codegen follows the IR. A route that ends without `respond` answers 200 with no body.
**Why:** SPEC 4.4 only says a guard without `else` stops the block; routes in 05 always end the else branch with `respond`.

## Q131 — An `as Record` answer that does not fit (step 6)
**Choice:** the answer is parsed as JSON (a ```json fence or prose around it is tolerated). If the program asks `x matches Record` about that variable, an unparseable or non-fitting answer makes the guard fail (traced `fail`, block stops) and `x` holds placeholder values. Without such a guard the failure is an error of the statement (caught by `try`, else the block fails and the trace gets an `error` line).
**Why:** `matches` would otherwise be a guard that cannot fail; acting on placeholder values is only possible when the author wrote the guard.

## Q132 — Months and years in durations (step 6)
**Choice:** `1 month` is 30 days and `1 year` 365 days (`chrono::Duration` has no calendar units).
**Why:** sema types both as `duration`; calendar arithmetic needs a different type.

## Q133 — Mock runs: working folder and the optional driver files (step 6, controller note)
**Choice:** in a `--mock` run the program changes into the fixtures folder (so `read file rows.csv` finds `rows.csv` there). `state.<field>.json` seeds the state record of a service or command (Q9 extended to services); `requests.json` (`[{"method","path","body"?}]`) is sent to the services in order and `command.json` (`{"command","args"}`) runs a command, both in `--once` mode; a request's `http` trace line carries the concrete path and comes after the guard lines of that request.
**Why:** services and commands have no schedule; `--once` needs something to do for them.

## Q134 — Errors in generated code (step 6)
**Choice:** every generated function returns `Result<T, AgentError>` (`AgentError = Box<dyn Error + Send + Sync>`); `result of T` is `T` there. A `try` without `otherwise` traces `error` and stops the block. A failure that escapes (no `try`, a timeout) is printed on stderr, traced as `error`, the run continues with the next entry and exits with code 1.
**Why:** one error type means `?` everywhere, binding errors (Q70) and runtime errors alike.

## Q135 — Tool traits, `Connected` and `src/connect.rs` (step 6; controller decision)
**Choice:** `tools.rs` has one `#[async_trait]` trait per tool that has verbs (`Crm`, `Email`, …), **without** blanket impls, an impl of each for `agentmark_rt::MockTools` (fixtures answer), and `struct Connected { crm: Box<dyn Crm>, … }` which implements `agentmark_rt::Tools` by dispatching `(tool, verb)` to the field. Generated blocks call `ctx.tools.call(tool, verb, args)`, so mock and real runs share all code. `src/connect.rs` is created by `build` only when it is missing (its header says it is the user's and never overwritten) and returns `Connected` made of `NotConnected` stubs that fail with "not connected yet: implement Crm::list in src/connect.rs"; a real run (no `--mock`) uses `connect::connect()`. The `log` tool is built in (stderr + trace); `print` goes to stdout unless the trace owns stdout. A rustc error in `connect.rs` is reported as the user's own file, not as an internal error.
**Why:** SPEC 7: "the user provides real implementations"; a seam in a file the compiler never rewrites.
**Amended (step 10):** the user's file lives in the PROJECT, `rust/connect.rs` beside `agentmark.toml` (for a single-file project: `rust/` next to the source file), not in the build output. `build` creates it from the stub template when it is missing (starting from an older `src/connect.rs` of the output if there is one), never overwrites it, and copies it to `src/connect.rs` of the output on every build, so deleting the output loses nothing. `run`/`trace` without `--mock` do the same (creating the stub too, so a failing call says `implement Crm::list in rust/connect.rs`); with `--mock` the file is used if it exists and never created. A rustc error in it is reported against the project file.

## Q136 — Which model a block uses (step 6)
**Choice:** each agent's blocks use that agent's `- model:` (`claude` = `claude-sonnet-4-5`, `claude:haiku` = `claude-haiku-4-5`, `claude:<id>` = `<id>`): the entry runs on `ctx.with_model(id)` and the id travels in `ModelRequest.model`. A real run's default model is the first driver of the program. A mock response in model.json may carry `"model": "<id>"` and then answers only that model (tests use it to assert the driver). The request is the prompt, a blank line and the `with` value as JSON; for `as Record` an answer-format sentence follows.
**Why:** `Ctx` holds one provider; the request carries the model.

## Q137 — Auto-binding on `build` (step 6)
**Choice:** `build`/`run`/`trace` run the binder only for imported crates that have no binding file; an existing file is reported as `cached` without checking the crate version (that is `agentmark bind`'s job, SPEC 15.1 "stale"). The generated `Cargo.toml` pins each bound crate to `=<version of the binding header>`; a hand-written binding without a header gets `*`.
**Why:** resolving the wanted version needs cargo and possibly the network on every build.
**Reconciled with Q155:** the header version IS the pin, so an existing binding is never stale on build; staleness (SPEC 15.3) only arises when the user changes the version and re-runs `agentmark bind`, which re-marks it `unverified`.

## Q138 — Entry knobs (step 6)
**Choice:** `timeout:` is enforced per run of an entry (the trace gets an `error` line); `memory: file` has no effect in v0.1 and `build` says so (`warning: …: \`memory: file\` is not supported in v0.1 and is ignored.`). In `--once` mode event entries run once per item of `<tool>.<event>s.json`; in schedule mode they are announced as needing a real tool connection (exit 0), and `build` notes agents that have only event entries. Blocks run one at a time (a global lock), as SPEC 3 says.
**Why:** nothing in the spec defines the memory file's format or a real event source.

## Q139 — `parallel:` (step 6)
**Choice:** a list of two or more effect statements runs together (`tokio::try_join!`, the first error wins); any other content (one statement, or non-effect statements that share variables) runs in order, and `build` prints a note for each such block so nobody expects concurrency there.
**Why:** SPEC 16 allows the opt-in; non-effect statements share variables.

## Q140 — `read file p as T` (step 6)
**Choice:** `.csv` (first row = field names, cells are text, converted to numbers/booleans only when the type refuses text) or JSON; `text` and `bytes` read the file as is.
**Why:** the spec names no format.

## Q150 — One mock response per prompt: branch coverage through `variants/` (step 9)
`MockModel` matches on the prompt text only, so every `ask model` of one prompt gets the same reply; a single run cannot show both a hot and a cold lead (02), or a high and a low ticket (03).
**Choice:** the top-level fixture folder covers the main path (02: two hot leads; 03: high ticket, then `stop`). The other branches live in `fixtures/<name>/variants/<case>/` with the same file set (`model.json`, tool JSON, `expected.trace.jsonl`): 02 `cold-lead` and `bad-json` (unparseable reply, Q157), 03 `low-priority` and `invalid-priority` (guard fail), 04 `empty-summary` (guard fail), 05 `missing-csv` (`try … otherwise`). A harness that wants full branch coverage runs each variant folder as its own fixture dir; one that ignores `variants/` still gets a valid main fixture.

## Q151 — Text where a binding takes `bytes` (05)
The assisted `sha256_digest` binding takes `data: bytes` (`AsRef<[u8]>`), and the language had no text-to-bytes step.
**Choice (controller):** implicit coercion, only for the arguments of a binding call whose parameter type is `bytes`: a `text` there is its UTF-8 bytes. Sema accepts it and records the argument in `Checked.text_to_bytes`; the IR wraps it in `ExprKind::TextToBytes(inner)` (type `bytes`; codegen emits `.as_bytes().to_vec()`). Every other type is still rejected (a `money` or `int` for `bytes` is an error); no other position (functions, Rust blocks, tools) coerces. `examples/05-invoice-service/bindings/sha2.agent.md` is exactly what `bind sha2 --force` writes (sha2 0.10.9, offline fixtures) with `- effect: pure` approved; 05 calls it with text. To regenerate that file: `agentmark bind sha2 --force` (the same version as its header; `--version`/`--latest` to move, Q155).

## Q152 — Where `## Rust: fingerprint` lives in 05
Rust blocks hoist to module level but must be written under a declaration. **Choice:** `functions/invoice-fingerprint.agent.md` holds `# Function: invoice_fingerprint` (binding call + `return fingerprint(digest)`) and the `## Rust: fingerprint -> text` block, which only hex-formats bytes. The service calls `invoice_fingerprint(found)`.

## Q153 — Trace and input conventions for services and commands (05)
Written into `fixtures/05-invoice-service/`: (a) the `http` line's `path` is the concrete request path (`/invoices/INV-1`), not the route template; (b) a guard's `name` is its condition only (`found is not empty`), without `else respond 404`; (c) route guards are traced like every guard (Q90, `agent` = `api`) and their line comes before the `http` line, which is written when the response is produced; (d) `print` lines of the command follow all request lines. `requests.json` = `[{method, path, body?, expect_body?}]` (`expect_body` optional: the response JSON the test may compare); `command.json` = `{command, args}`. Both files are optional. The binary runs with the **fixture folder as working directory** and args are passed verbatim (so `invoices.csv` is a plain relative path). `state.invoices.json` seeds `AppState.invoices` for the service and the command (Q9).

## Q154 — Event entries and `--once` in fixtures (02)
`crm.leads.json` is both the fixture data and the event source: each item fires `## When crm receives lead` once with `lead` bound. `score` is `null` (a `maybe int`) in the input.

## Q155 — Dependency pin comes from the binding header
**Choice:** the version in the binding header (`> generated by agentmark bind — sha2 0.10.9 — …`) is the lock: the generated `Cargo.toml` pins `<crate> = "=<header version>"`. Changing the crate version means re-running `bind`, which re-marks the binding `unverified` (SPEC 15.3). Generated `*.agent.md.prev` backups are build leftovers and are git-ignored.
**Amended (step 10):** `agentmark bind <crate>` on an existing binding stays on the header's version (a cache hit; no silent jump to the newest release). To move: `agentmark bind <crate> --version <v>` (an exact release) or `--latest`; either regenerates and re-marks the binding `unverified`. `--force` regenerates the SAME version. A missing binding takes the newest version as before.

## Q156 — Prompt that reaches the model with `ask model with v`
**Choice:** the request prompt is the literal prompt text, then `"\n\n"`, then the context JSON (`{"id": v.id}`); without `with` it is the literal alone. `MockModel` matches by prefix, so the fixture keys (the literal, or its interpolated form as in 04) stay valid, and `prompt_prefix` in the trace is the matched key.
**Amended (step 6 review):** implemented as: the literal prompt, `"\n\n"`, then the compact JSON of the whole `with` value (the record, so the model sees the data); the trace `context` stays `{"id": v.id}`. Without `with` the prompt is the literal alone. Answer-format hints for `as Record` follow the context.

## Q157 — Unparseable reply for `as <Record>`
**Choice:** a reply that is not valid JSON for the record (not JSON, missing field, wrong type) is not a runtime error: `<var> matches <Record>` evaluates to false, so the guard line is traced with result `fail` and the block stops. The variable then holds no usable value, but nothing reads it after the guard. Fixture: `fixtures/02-lead-qualifier/variants/bad-json`.

## Q158 — Boundary of `overdue` (04)
`today > due + grace_period` is strict, so an invoice due exactly 14 days ago (`INV-6`, due 2026-09-28, today 2026-10-12) is open but not overdue: counts 5 open, 2 overdue.

## Q170 — `result of T` functions cannot be written or called in v0.1 (step 12, found while documenting SPEC 14)
SPEC 14: "A function whose `out` is `result of <T>` returns `Result<T, AgentError>`; callers must use `try`." The semantic check accepts the type but not the two halves: `return n / 2` in a function with `out: result of int` is rejected ("This returns a number, but the function says it returns "result of int""), and `try do half(n) -> h` leaves `h` typed `result of int` instead of `int`, so `- set h = 0` in the `otherwise` fails. Reproduction: a pure function `half` with `out: result of int` and a caller using `try do half(n) -> h otherwise: - set h = 0`.
**Choice:** the docs (`docs/reference.md` section 14) state the SPEC rule, show the function as a *fragment* (parsed only, never checked) and say plainly that the v0.1 check does not yet accept the pair; the working uses of `try` (tool calls, reads, bindings) are the complete examples. No source change in this step.
**Why:** a complete example the checker rejects would fail `tests/docs.rs`, and a hidden example would hide the gap. The fix belongs to `sema` (wrap a plain `T` returned by a `result of T` function; unwrap on `try`), then the fragment becomes a complete program.
**Resolved (step 12 update):** fixed by Q200; `docs/reference.md` section 14 now shows `result of T` as a complete, checked program and the `gap` fragment tag no longer exists.

## Q171 — How docs samples are tagged and tested (step 12)
**Choice:** `tests/docs.rs` documents the rules in its header. A fenced block tagged `agentmark` is a complete program (first real line is a `# ` heading; `check` must report 0 errors against `examples/tools.toml` plus the `bindings/` files named by earlier blocks of the same doc; and it must already be in `fmt`'s canonical style). `agentmark fragment` is only parsed, inside the smallest wrapper that makes its first line legal (an agent's `## Do` block, a service, a command, a function, a binding). `agentmark invalid` must fail `check`. Modifiers: `name=<path>` (a file of the tutorial project, later blocks see the latest version), `file=<repo path>` (the block must equal that file: this is how the docs quote `examples/` and `fixtures/` without drifting), `cwd=repo`, `stderr`. A `console` block starting `$ agentmark ...` is executed and its stdout (or stderr) must match.
**Why:** the brief asks that every sample be extracted and checked; complete programs, fragments and quoted files are three different promises, and a console transcript is a fourth. Using the real binary for transcripts means error messages in the guide cannot be out of date.

## Q172 — `build`, `run` and `trace` are documented before the code generator is merged (step 12)
The generator (step 6/7) is written in a parallel worktree. **Choice:** the docs describe `build/run/trace` from SPEC 9, DESIGN 4.3/5 and the step-6 brief (flags `--once`, `--schedule`, `--mock`, `--fixtures`, `--trace`, auto-binding messages, refusal of unapproved libraries) and show their output by quoting committed files (`fixtures/*/expected.trace.jsonl`, checked by `file=`), never by running them. `tests/docs.rs` does not depend on them, and `internals.md` paths under `src/codegen/` are checked only once that folder exists.
**Why:** the documentation must not block on, or guess at, a branch that is not merged; once it is, a `console` transcript for `trace` can be added.
**Superseded (step 12 update):** the generator is merged; the guide and reference show real `run`/`trace` transcripts that `tests/docs.rs` executes, and the `src/codegen/` paths of `docs/internals.md` are checked like every other path.

## Q173 — The reference has no section 12 (step 12)
SPEC.md jumps from section 11 to 13. **Choice:** `docs/reference.md` mirrors the SPEC numbering exactly (0 to 11, 13 to 21), and `tests/docs.rs` compares the list of `## N.` headings with SPEC.md's. Appendices are unnumbered.
**Why:** "one section per SPEC section, same order" is easiest to keep true, and to check, when the numbers are the same.

## Q174 — `check` does not run the binder (step 12)
SPEC 15.1 says the compiler generates a missing binding when it meets `# Import: crate X`. In this build only `bind` writes bindings; `check` on a file that uses `- sha2 sha256_digest ...` before `bindings/sha2.agent.md` exists reports "no binding yet" and also "Unknown tool "sha2"" (the second message is true but unhelpful). **Choice:** `docs/libraries.md` tells the reader to run `agentmark bind <crate>` first and names both messages; `build` is documented as the command that auto-binds (SPEC 9, brief). **Suggested follow-up (not done here):** let the "Unknown tool" error say `run "agentmark bind sha2"` when an unbound crate import in the file has that name.
**Resolved (step 12 update):** Q201 makes `check` bind like `build`; `docs/libraries.md` shows the workflow import, check, approve with a real transcript (the test passes the recorded rustdoc and a canned model through `env=` modifiers on the console block).

## Q175 — The guide's vocabulary (step 12)
PROMPT step 12 bans "compiler", "IR", "binding" and "effect class" in `docs/guide.md`. **Choice:** `tests/docs.rs` rejects those as whole words, case-insensitively, anywhere in the file including code blocks, plus "bindings", "compile", "compiles" and "compiled". The guide says "library", "what it can access" / "what each block can touch", "safety check", "test data". The real `check` message about an unapproved library contains the word "binding"; the guide therefore describes that message in its own words instead of quoting it (the developer page `docs/libraries.md` quotes it).

## Q190 — `--allow-unverified` cannot build 05 with a freshly generated binding (step 10)
The brief for the sha2 generated-once test said to build 05 without its binding using `--allow-unverified`. The flag does not get past the check here: `invoice_fingerprint` is declared `- effect: pure`, and a call to an unverified binding has effect "unverified", so sema also reports "the function says pure, but this line has the effect unverified". That second error is correct (the function's promise is not proven).
**Choice:** the test follows the real flow of SPEC 15.2/15.3 instead: build 1 generates the binding and stops (exit 1, `binding sha2: generated`, "is unverified"); the test then approves the one line (`- effect: pure`), and build 2 succeeds with `binding sha2: cached`. Generated exactly once across both logs. No compiler change.
**See also Q203:** the decision that `--allow-unverified` checks effects with the binding's proposed effect (this case no longer needs the two-build flow for effect checks).

## Q200 — `result of T`: how a function fails, how it is called (step 10)
SPEC 14 says a function with `- out: result of <T>` returns `Result<T, AgentError>` and callers use `try`, but gives no way to write the failure.
**Choice:** no new statement (`fail "…"` is not in the SPEC). Inside a `result of T` function, `return v` takes a plain `T` (wrapped in `Ok`), and a `try` without `otherwise` that fails does not trace-and-stop as everywhere else: the error is returned to the caller (`return Err(err)`), untraced. The caller's `try do f(x) -> h` then decides: `otherwise:` runs, or (no `otherwise`) the error is traced once and the block stops. After the `try`, `h` is a plain `T` (also when `otherwise` assigns it). A call without `try`, or inside an expression (`f(x)`), is an error. The call form is `try do f(x) -> h` (`do` is part of the statement grammar; `try f(x) -> h` is not).
**Why:** the one failure source the language has is a failing `try`; reusing it keeps `result of` writable without extending the grammar.

**Amended (step 20):** the canonical call form is `try f(x) -> h` (`try do f(x) -> h` is accepted), and a result can also be stored or matched; see Q300 and Q307.

## Q201 — `check` binds like `build`; what a missing binding reports (step 10)
SPEC 15.1 says the workflow is `# Import: crate X` -> `check` -> approve, but only `build` ran the binder.
**Choice:** `check` runs the same auto-bind as `build` (shared `bind_missing`; `binding X: generated` on stderr, nothing printed for cached bindings). If binding is impossible (no API key, no mock, offline without cached rustdoc) it prints `agentmark: cannot bind "X": <why>` and checks on. A crate without binding is then reported once, on its import line: an error `The crate "X" has no binding yet, but this file uses it.` (`E219`) with the `agentmark bind X` command when a statement or an agent's `tools:` line uses it as a tool, otherwise the W004 warning as before. The follow-on `Unknown tool "X"` and the false `Unused import` (a crate used as a tool) are not reported.

## Q202 — A word longer than a line in `## Not bound` (step 10; amends Q110)
Q110 split a single word that does not fit on a line and the parser joined the pieces with a space, so a 145-character path came back with a space inside it.
**Choice:** the line that ends inside a word ends in a marker `\`; the parser (`syntax::parser::join_wrapped`) joins the next `  > ` line to it without a space and drops the marker. So a real backslash at the end of a wrapped line is not taken for the marker, the trailing backslashes of every line that is followed by another are doubled by the writer and halved by the parser (read per line). The writer counts a backslash as two columns and keeps one free, so the 120 limit still holds. Round trip: `tests/binder.rs` (145-character word, backslash-heavy paths).
**Changed decoding:** a hand-written `## Not bound` continuation line that happens to end in backslashes is now read with this rule (an odd run is a join marker, pairs are halved); such lines were taken literally before. Generated files are unaffected.

## Q203 — `--allow-unverified` assumes the proposed effect (step 10; the controller's decision, written down as Q190 on the dev branch, see Q190)
**Choice:** with `--allow-unverified` (check, build, run, trace) an unverified binding counts as its `- proposed-effect:` for effect checking, so a `pure` function calling a binding proposed `pure` passes; each such binding gets one warning (W008): "its effect is assumed to be X from the proposal", help "approve it before a release build". Without the flag nothing changes (the binding is `unverified`, the build is refused). The advice when a function exceeds its effect because of an unverified call no longer says "change the function to `- effect: unverified`" (not a legal function effect, SPEC 13.1) but to approve the binding's `- effect:` line. `Checked.bindings` still reports the binding as unverified.

## Q220 — How the README is kept true (step 11)
PROMPT step 11 asks for a README whose commands work; nothing said how to keep them working.
**Choice:** `tests/readme.rs` runs the README. A ```` ```console ```` block is `$ agentmark …` lines (leading `NAME=value` env allowed, `$PWD` = repository root) followed by output fragments that must appear in stdout+stderr (a trailing `…` cuts a line, `…` alone is skipped); `# exit N` sets the expected exit code, `# in FILE: replace "A" with "B"` is the edit the reader makes by hand (used for approving the sha2 binding). The words `app` and `out` are scratch folders (```` ```console app ```` = example 05 without its sha2 binding); temp paths are scrubbed from the output before matching. Every ```` ```agentmark ```` block must `check` clean as a file next to example 01's `tools.toml`. The ```` ```rust file=rust/connect.rs ```` block is dropped into a copy of example 01 and `run` (no `--mock`, no key) must compile it and stop at "ANTHROPIC_API_KEY is not set", which proves the in-memory CRM answered `list`. Offline bind uses `tests/binder/*` through the documented `AGENTMARK_MODEL`/`AGENTMARK_RUSTDOC_JSON(_DIR)` variables.
**Why:** the real binary, mock model, no network, nothing written into the repository.

## Q221 — A real run honours `AGENTMARK_MODEL=mock:<model.json>` (step 11 follow-up; the controller's decision)
First written as "a real run cannot use a mock model": generated programs read only `ANTHROPIC_API_KEY`, so a real run with `connect.rs` could not be shown with canned model answers.
**Choice (superseded by Q241: the router does this now):** the generated `real_model()` (programs that use a model) checks `AGENTMARK_MODEL`; a value `mock:<path>` loads `agentmark_rt::MockModel::from_file(path)`, the same syntax the binder uses; a bad file makes the first question fail with the file error. Anything else (or unset) is `AnthropicModel` as before. The clock stays the system clock. `tests/readme.rs` runs the README's `connect.rs` that way and asserts the email line on stdout and the `crm update` trace line.
**Why:** one variable, one meaning, in both tools; offline demonstrations and tests of real tool code.

## Q222 — `[rust.dependencies]` in `agentmark.toml` (step 11 follow-up; the controller's decision)
Generated projects depended only on a fixed list plus imported crates (and `agentmark-rt` by absolute path of the compiler checkout), so `rust/connect.rs` could not use a client crate such as `reqwest`.
**Choice:** `agentmark.toml` has an optional `[rust.dependencies]` table (`Config.rust_dependencies`). Each entry is a version text or a TOML table; it is validated as TOML (other shapes, and unknown keys under `[rust]`, are E114 config errors) and written verbatim, as `name = <value>`, to the end of the generated `[dependencies]`. An entry named like a dependency the generated project already has (`agentmark-rt`, `async-trait`, `chrono`, `clap`, `serde`, `serde_json`, `tokio`; `axum`/`tower` with services; an imported crate or its alias) is a plain error from `build`/`run`/`trace` (exit 2) and nothing is written. The generated `Cargo.toml` is still rewritten on every build. `agentmark-rt` stays tied to the checkout (README says "keep the checkout").
**Why:** a real backend needs a client crate; a table in the one config file keeps connect.rs the only Rust the author writes.

## Q230 — Which statements trigger the on-demand tier (step 13)
SPEC 15.2 says "a plain statement against a skipped item" but not how the statement is recognised.
**Choice:** a tool-call statement `<binding> <item> value ...` (anywhere in a function, command, route or agent block, nested ones included) whose tool word is the name of a `# Binding` and whose item is listed under that binding's `## Not bound` and not among its bound items. Done by walking the AST before `sema` (`src/cli/ondemand.rs`), so sema is untouched; the item name is the verb as written.
**Why:** the item of a statement is exactly what `sema` would otherwise call "Unknown verb"; finding it earlier keeps the on-demand tier a separate step in front of the check.
**Amended (review):** the statement's own file must also import the crate (`# Import: crate X`); a binding that file does not import is not extended.

## Q231 — Where the signature of an on-demand item comes from (step 13)
Not-bound entries carry only a name and a reason; an item skipped for having no AgentMark type (`sha224_new` returns a hasher) has no signature at all.
**Choice:** the library's rustdoc is read again (same loader as `bind`). If the item was assisted-shaped (it only lacked a body, for example `sha224_digest`) its AgentMark signature is used and the model writes the body only. If it was skipped, the model proposes the signature on the first line (`## name(params) -> type`, read back with the real parser, types limited to the binding types); its parameter count must equal the number of values in the statement, otherwise the attempt fails. A wrong argument *type* is left to the normal check afterwards (Q151 conversions apply).
**Why:** the statement alone cannot say what an object-returning call should become; the checked signature keeps `sema` the judge of the author's arguments.
**Amended (review):** the version the rustdoc is read for is, in order: the binding's generated header, a `Cargo.lock` next to `bindings/` (or above `.agentmark/bindings/`), the rustdoc JSON the caller supplied, else a plain error. Never "the newest".

## Q232 — Two attempts, all or nothing, and the error shape (step 13)
**Choice:** at most two model calls per item, the second prompt carries the first complaint (compiler text, "no code", bad signature). Items of one crate are saved together: if any item fails, the binding file is not touched. The error is E220 at the verb in the author's statement, in the SPEC 10 shape, with the help line "Nothing was changed. Use a ## Rust block ...". The compiler's own text is never shown (the author does not read Rust, and it differs between toolchains); each attempt is one line `binding <crate>: writing <item> on demand… done|failed` on stderr (`(try 2 of 2)` on the second).
**Why:** a half-written binding would make the next build depend on what happened to pass; the documented message must be the same on every machine.
**Amended (review):** the middle sentence of E220 depends on the cause (type check failed twice / the model could not be reached: reason / the signature the model chose does not fit the statement: counts). The compiler text goes to `<project>/target/agentmark/ondemand-<crate>-<item>.log`, named in the help line, never inline. With several crates, every crate is produced first and the files are committed only when all succeeded (`produce_on_demand` / `commit_on_demand`).
**Model (step 18):** the on-demand tier picks its model with the same call as `agentmark bind`: `bind::model_for(bind::provider_config(&project))` (`AGENTMARK_MODEL`, else the `[models]` entry `AGENTMARK_ENV` names, else Claude; Q240). The old `bind::model_from_env` shim is gone.

## Q233 — What a successful on-demand write does to the file's trust (step 13)
**Choice:** the item moves out of `## Not bound` into a bound item whose `>` line ends in "Written on demand; not reviewed yet."; the file's `- effect:` becomes `unverified` even if it was approved (code nobody read was added), `- proposed-effect:` is recomputed and never lowered; the previous file is kept as `<crate>.agent.md.prev`; a hand-written file (no generated header) is edited the same way. There is no per-item `unverified` marker, because the effect line belongs to the file (SPEC 15.4).
**Why:** SPEC 15.3 allows an approval to cover only what was reviewed; the doc line lets a reviewer find the unreviewed items.

## Q234 — When the tier cannot run (step 13)
No model (`ANTHROPIC_API_KEY`), no rustdoc, or an unreadable binding.
**Choice:** `build`, `run` and `trace` print `agentmark: cannot write the library code for "<crate>" on demand: <why>` and stop with exit code 1; `check` prints the same and goes on, so the unknown item is still reported by sema. Nothing is written.
**Why:** matches `bind_missing` (build refuses, check reports and continues).
**Amended (review):** a hand-written binding with no known version (see Q231) is a setup error that says to add a `Cargo.lock` next to `bindings/` or the generated header line with the version.

## Q235 — Rounds and `check --json` (step 13)
`prepare` and `check` reload the project after a binding changes, so the loop is three rounds (first binding, on-demand, final check).
**Choice:** an on-demand failure in `check` replaces the semantic check: it is the only diagnostic, in the text output and in `--json` (code `E220`, `ok: false`).
**Why:** the statement that failed would otherwise also get "Unknown verb", a second message about the same line.

## Q240 — Which model wins: `- model:`, `AGENTMARK_ENV`, `AGENTMARK_MODEL` (step 14)
SPEC 19 maps environments in `[models]` but does not say how that map meets an agent's own `- model:` line.
**Choice:** strongest first: `AGENTMARK_MODEL=<driver>` (any driver string, one model for the whole program), then `AGENTMARK_ENV=<name>` looked up in `[models]` (one driver for every agent), then each agent's `- model:`. The map therefore only overrides when an environment is selected; unset `AGENTMARK_ENV` is not "production", it overrides nothing, so `- model: openai:gpt-4o` stays OpenAI although `production = "claude"`. `AGENTMARK_ENV=production` with no `production` entry also overrides nothing; any other name missing from a non-empty table is an error that lists the known names; with no `[models]` table `AGENTMARK_ENV` means nothing. An entry of plain `claude` keeps an agent's own Claude model (`claude:haiku`) and moves agents of other providers to Claude; any other forced real driver replaces every agent's model id. A forced `mock` keeps the agent's id in the request (fixtures with `"model"` still tell agents apart). The binder and the on-demand tier read the same variables and the project's `[models]`, defaulting to Claude (Q115 stays: the agent's `- model:` describes the program, not the toolchain). The table and `base_url` are baked into the generated program at build time; the two variables are read at run time.
**Users (step 18):** `agentmark bind`, the on-demand tier (`check`/`build`) and the language server's model actions all choose their model by this rule (`bind::model_for`).
**Why:** a team flips `dev`/`test` without editing agent files, while a pinned `- model:` is never silently replaced just because a table exists.

## Q241 — One runtime router, provider carried in `ModelRequest.model` (step 14)
Q136 said `Ctx` holds one provider and the request carries a model id; with several providers in one program that is not enough.
**Choice:** `agentmark_rt::Router` implements `ModelClient` and builds one client per driver on first use. `ModelRequest.model` carries the Anthropic id for `claude…` (unchanged, so traces and `model.json` fixtures keep working) and the driver string itself for `openai:<m>`, `local:<m>`, `mock`; `Driver::parse` is the one parser, used by the runtime, generated programs, `check` (E221) and `agentmark.toml` validation (E114). `Ctx` and the trace schema are untouched. The generated `real_model()` is `Router::from_env(ProviderConfig::new(..))`; the old `MODEL` constant and the hand-written `mock:` branch of Q221 are gone (the router covers both). Missing keys are reported when the first question is asked, except for a forced provider, which fails at start-up as a `NoModel` error.
**Why:** one parsing and selection rule for every tool; no change to the trace.

## Q242 — OpenAI, local and mock wire details (step 14)
**Choice:** Chat Completions only (`POST {base}/chat/completions`, one `user` message, first choice's `message.content`; a list of text parts is joined; `null` content is an error). The token limit is sent only when the request has one: `max_completion_tokens` to OpenAI (the field every current OpenAI chat model accepts) and `max_tokens` to local servers (the field they all know); there is no default limit. OpenAI: bearer `OPENAI_API_KEY`, base `https://api.openai.com/v1`, `OPENAI_BASE_URL` overrides (proxies, tests). Local: base is `AGENTMARK_LOCAL_BASE_URL`, else `[models.local] base_url`, else `http://localhost:11434/v1`; no key is sent unless `LOCAL_API_KEY` is set. Anthropic gains `ANTHROPIC_BASE_URL` for the same reason. Bare `mock` reads `model.json` from `$AGENTMARK_FIXTURES` or the current folder; `mock:<path>` reads that file (the binder's existing syntax). No retries, no streaming, no tool calling.
**Why:** the smallest request every OpenAI-compatible server accepts; the overrides make provider tests possible without live calls.

## Q243 — `azure:` and `bedrock:` are rejected at `check`, E221 (step 14)
**Choice:** a `- model:` that does not parse is error E221 at the model token (SPEC 10 shape): `The model provider "azure" is not supported in this version.` with the help line `A model is claude, claude:<model>, openai:<model>, local:<model> or mock.`; an unknown provider word gets `Did you mean "openai"?`; `openai:`/`local:` without a model name is an error. The same check runs on every `[models]` value (E114, at that line). Before this step `check` accepted any model string.
**Why:** an unsupported route must fail where it is written, not at the first question of a running program.

## Q244 — `[models]` keys (step 14)
**Choice:** every key under `[models]` whose value is text is an environment name (not only `production`/`test`/`dev`); `[models.local]` is the one sub-table (`base_url`, must start with `http://` or `https://`; any other key is an error). An environment literally named `local` therefore cannot exist. `Config` gains `models` and `local_base_url`; the lint config reader still ignores the table.
**Why:** teams have `staging`, `ci`; one reserved name keeps the provider table and the environment list in one `[models]` block as SPEC 19 writes it.

## Q245 — `azure:` and `bedrock:` stay unsupported in this version (step 14)
SPEC 20.2 lists model providers `openai`, `local`, `azure`, `bedrock`; PROMPT step 14 scopes `openai` and `local` only.
**Choice:** `azure:` and `bedrock:` are not implemented; the behaviour is the check-time error E221 at the `- model:` line (Q243), also for `[models]` values. A trailing colon with no name (`claude:`, `openai:`, `local:`, `mock:`) is the same plain error for every provider.
**Why:** an honest early error beats a half provider; they can be added as `Driver` variants later without changing programs.

## Q250 — `[lsp] model_actions` in `agentmark.toml` (step 15)
SPEC 20.3 says the model-backed editor actions are "off unless `agentmark.toml` enables them" but names no key, and the loader rejected every unknown table (E114).
**Choice:** `[lsp] model_actions = true` (default `false`). `project::Config` gets `lsp_model_actions`; `[lsp]` is an accepted key whose only field is `model_actions` (a non-boolean or an unknown field under `[lsp]` is an E114 error with the usual wording). The flag is read per project, so the server advertises the actions' capability statically and answers `model actions are off` when asked for one in a project that did not enable them. With the flag on but no model available (`AGENTMARK_MODEL=mock:<file>` or `ANTHROPIC_API_KEY`) the actions are not offered.
**Why:** one key in the one config file, same shape as `[lint]`; no second settings place in the editor.

## Q251 — Project roots, open buffers, and what gets diagnostics (step 15)
**Choice:** the project of a document is the nearest folder above it that has an `agentmark.toml` (a folder project: `check` sees every `*.agent.md` below it); without one the document is a single-file project, also when it only exists as a buffer. A file inside a folder the loader skips (`fixtures/`, `docs/`, ...) is checked on its own. Open buffers are laid over the disk (`workspace::OverlayFs` implements `project::Fs`), so an unsaved edit of an imported file changes the importer's diagnostics. The server publishes the diagnostics of the **whole project** (compiler errors and warnings, then the lint findings with source `agentmark-lint`), also for files nobody opened, and clears them when they are gone. A document that is `tools.toml` or `agentmark.toml` is analysed through its project and gets its own diagnostics. Files under `bindings/` are not linted (as `agentmark lint`, Q111).
**Why:** `agentmark check` is a project command; a file's meaning depends on the others.

## Q252 — Answers while the buffer does not check (step 15)
`sema::check` returns no `Checked` when there is any error, which is the normal state while typing.
**Choice:** the server keeps the last successful `Checked` of each project with the text of every file it came from. A line diff (`workspace::LineMap`, longest common subsequence of the lines) maps the lines of the file as it is now to the lines of the check and back: hover and go-to-definition work on every line that was not touched since the check, also in the file being edited, and a location is returned at its current line (dropped when that line was changed). Completion uses the nearest mapped line above the cursor to find the block it is in. Go-to-definition has a second path that needs no check: the word under the cursor is looked up among the declarations of the current ASTs and in the tool manifest. Completion takes names from the current ASTs of the files that parse, and from the last good check for the file that does not; types and record fields come from the last good check, or as the AST writes them. Nothing at all is offered about a project that never checked.
**Why:** an editor that goes silent on the first typo is useless; stale positions would be worse than silence.

## Q253 — Which record does `invoice.amount` mean when only the whole path has a type (step 15)
`expr_types` has the type of a whole path, `resolutions` only the first segment's variable; the record of a middle segment is not stored.
**Choice:** hover and go-to-definition on the **last** segment use the exact type of the path and look for the record that declares a field of that name and type, preferring the record the enclosing block works on (`BlockInfo.param`); on an earlier segment of a longer path the same guess is used. A wrong guess is possible only when two records have a field with the same name and type, and the record of the block wins then.
**Why:** no change to the checker's tables for a guess that is exact in practice.

## Q254 — Completion after `x.` asks the checker (step 15)
**Choice:** the buffer is checked once more with the `.partial` (and the rest of that word) removed from the cursor line; the type of the path before the dot is read from `expr_types`. Only a clean check counts. Otherwise the variable's type comes from the last good check (a bare use of the variable, or the record the block works on), `state` from the `- state:` line of the service found in the text, `tool.` lists the tool's `sources`, `module.` the names of the imported file. No completion inside the plain text of a string, inside `{...}` yes.
**Why:** exact types without writing a second expression typer in the server; one extra check per completion request.

## Q255 — Which lint rules get a code action (step 15)
**Choice:** `magic-value`: add `# Constant: fixed_amount|fixed_duration|fixed_url` (a numbered name when taken) at the end of the file and replace the literal on every line the finding lists (outside strings for amounts and durations, as `{fixed_url}` inside strings for URLs); `unnamed-guard`: insert `> TODO: say what this guard protects against.` above the guard; `prompt-in-loop`: insert `- guard: x is not empty` (or `x matches Record` when the `ask model` line says `as Record`) after the line; `missing-doc`: insert `> TODO: describe what this agent|service|command does.` above the heading. **No action** for `long-file` (where to split is the author's decision), `duplicate-rule` (which rule survives, and the places that use the other, is a decision) and `vague-name` (a rename needs a name only the author knows; a mechanical `data` -> `data_value` would only hide the finding). Also: a "Did you mean" suggestion on any compiler error is a quick fix that replaces the word. The facts a fix needs (literal, lines, variable, declaration kind) come from the new `Diagnostic.data` that `lint` fills, never from the wording of the message. The two fixes that leave a blank for the author (`missing-doc`, `unnamed-guard`) insert `> ${1:describe what this agent does}` as a snippet text edit when the client announces `experimental.snippetTextEdit` (the rust-analyzer extension: `documentChanges` with `insertTextFormat: 2`), otherwise the plain `> TODO: ...` line. Trade-off: standard LSP has no snippet edits, so plain clients get a TODO text that nothing forces the author to fill in; snippet clients land with the cursor in the blank. Every fix is tested by applying it, then checking that the project still checks and the finding is gone.
**Why:** SPEC 20.3 says "every lint rule that has a mechanical fix"; these four have one, and the stubs make the next step obvious without inventing meaning.

## Q256 — The model-backed actions (step 15)
**Choice:** "Explain this block" is a code action with a command (`agentmark.explainBlock [uri, line]`); the server asks the model with the block's text and the checker's effect summary, answers the command with the text and also sends it as `window/showMessage`. "Suggest a guard for this answer" is offered on an `ask model` line and resolved lazily (`codeAction/resolve`): the model's reply is cleaned to one `guard:` line, inserted after the statement, and refused unless the edited file still parses. The prompts start with `EXPLAIN_PROMPT` / `GUARD_PROMPT` (public constants) so a `MockModel` fixture can key on them. The call blocks the server's thread while it waits (current-thread runtime); fine for an explicit user action. Model selection (step 18, replaces the first draft that read only `AGENTMARK_MODEL=mock:` or `ANTHROPIC_API_KEY`): the binary uses `agentmark_lsp::serve_from_env`, which resolves the model per project when an action runs, with `agentmark::cli::bind::model_for(provider_config(project))`, so `AGENTMARK_MODEL`, `AGENTMARK_ENV` + `[models]`, `[models.local] base_url` and the Claude default behave exactly as for `agentmark bind` (Q240). The actions are offered only when that resolves (flag on and, e.g., a key or a mock is present); otherwise the command answers "no model is configured: <reason>". `serve(conn, Some(model))` stays for tests with a fixed model.
**Why:** an explanation is not an edit; a guard is, and the model's text is untrusted input.

## Q257 — Sync and timing (step 15)
**Choice:** full-document sync (`TextDocumentSyncKind::FULL`), positions in UTF-16 units (the LSP default; the compiler counts characters), no debounce timer: while more messages are already waiting the server only records the change and publishes diagnostics once the queue is empty. The analysis is cached per project and thrown away on any change, save, close or watched-file event; the server registers `**/*.agent.md`, `**/tools.toml`, `**/agentmark.toml` watchers when the client allows dynamic registration. A panic in the checker is caught and logged; the server keeps the last good result.
**Why:** simplest correct behaviour; projects are small and a check is milliseconds.

## Q258 — Formatting (step 15)
**Choice:** `textDocument/formatting` returns one edit replacing the whole document with `fmt::format_source`, an empty list when it is already canonical, and `null` when the file does not parse (the diagnostics already say why) or lives under `bindings/` (generated, Q111). Format-on-save is the client's `formatOnSave` calling this; the server has no `willSaveWaitUntil`.
**Why:** one style, no options (SPEC 18.6); a half-written file is never rewritten.

## Q259 — Odd requests and the end of the session (step 15 review)
**Choice:** a request for a document that is not a readable `file://` URI (`untitled:`, deleted file) is answered with `null`, not an error, and opening such a document is ignored. `exit` without a `shutdown` before it makes the process exit with code 1 (LSP), after `shutdown` with 0. Diagnostics of `agentmark.toml` / `tools.toml` that point at a line (not a word) underline the whole line.

## Q260 — tree-sitter grammar is line oriented, no external scanner (step 17)
- Question: SPEC 1 says indentation is exactly 2 spaces per level; should the tree-sitter grammar model nesting with an INDENT/DEDENT scanner?
- Choice: no. Every statement line is a sibling inside its `##` block; indentation is skipped as whitespace. Nesting, the depth limit and indentation errors stay with the compiler (`agentmark check`).
- Why: highlighting, folding by heading and Rust injection need only line-local structure; a scanner would add C code to maintain for no visible gain, and editors must tolerate half-typed code.

## Q261 — `- key: value` is a property only for the keys SPEC defines (step 17)
- Question: `- log: "x"` is a tool call in a block but looks like a property in a declaration body (a Function's indented statements live there).
- Choice: only these keys are properties: model, tools, memory, timeout, for, when, value, in, out, effect, args, listen, state, crate, proposed-effect. Any other `- word: ...` is parsed as a tool call, so `- log: ...` is highlighted as a tool everywhere. A tool named like one of those keys, written without a verb, is highlighted as a property.
- Why: no extra scanner or indentation test is needed, and the collision needs a tool called `state`, `value`, `in`, ... used without a verb.

## Q262 — Fragments and doc blocks must parse too (step 17)
- Question: docs show `fragment` blocks (statements with no declaration, `## POST` with no `# Service`) and the `invalid` blocks.
- Choice: the grammar accepts statement lines and `##` blocks before the first declaration, and the check script parses every `agentmark` fence in README.md and docs/*.md (including `fragment` and `invalid`, which are well-formed syntax).
- Dash-less lines are accepted only for control statements (for each, if, else, otherwise, parallel, try; docs/reference.md shows a bare `for each`); any other dash-less prose line is an ERROR instead of a tool call.
- Why: the grammar is for highlighting, not validation; an editor showing a half-written file must not paint it red.

## Q263 — Contextual words stay usable as names (step 17)
- Question: `first`, `count` and `no` start expressions (`first x in y where ...`, `count of ...`, `no amounts changed in ...`) but docs use `first` as a parameter and argument name.
- Choice: they are keywords only when the rest of the phrase follows; otherwise they parse as plain identifiers. Other words (`state`, `value`, `length`, ...) are only keywords where the SPEC puts them, so they stay usable as record fields and variables.
- Why: matches how the compiler reads them; avoids ERROR nodes on valid files.

## Q270 — VS Code extension layout, bundling and testing (step 16)
**Choice:** `editors/vscode/` is TypeScript. `tsc` compiles `src` and `test`; esbuild bundles `src/extension.ts` (with `vscode-languageclient`) into `dist/extension.js`, so the vsix holds 9 files and `vsce package` prints no warnings. Pure modules (`checkjson.ts`, `panel.ts`, `server.ts`) hold the logic; only `extension.ts` imports `vscode`. `npm test` (`node --test`) tokenizes the TextMate grammar over every `examples/**/*.agent.md`, validates `agentmark.toml` examples against the schema with `ajv`, and renders the panel from `check --json` output recorded from the real CLI (`test/fixtures`). The root `cargo test --test vscode_extension` copies `editors/vscode` to a temp dir and runs `npm ci`, `npm run compile`, `npm test` and `vsce package`, failing on any vsce warning; it skips with a message when npm is missing.
**Why:** an extension host cannot run in CI cheaply, and the LSP is already tested directly (step 15), so the client stays thin; a temp copy keeps the checkout free of build output.

## Q271 — `agentmark.toml` schema and `[models]` (step 16)
**Choice:** `schemas/agentmark.schema.json` covers `name`, `version`, `targets`, `[lint] deny`, `[rust.dependencies]`, `[lsp] model_actions` and `[models]`, mirroring the compiler on dev: driver `^\s*(claude(:.*\S)?|openai:.*\S|local:.*\S|mock(:.*\S)?)\s*$` (step 18: like `Driver::parse`, which trims the string and takes everything after the first colon as the name, so surrounding whitespace and names with spaces are valid) (azure and bedrock are rejected), `local` is either a driver string or a table with `base_url` matching `^https?://`. The schema is registered as `tomlValidation` (Even Better TOML's contribution point, no manual association). The extension is bundled with esbuild into `dist/extension.js`, so the vsix has 9 files and vsce prints no warnings.
**Why:** SPEC 19 defines the keys; the schema must not lag the compiler.

## Q290 — Setup snippets for Zed, Neovim and Helix are not executed (step 18)
**Choice:** `docs/editor.md` gives configuration for Neovim (`vim.lsp.config`, `nvim-treesitter` main branch with a local `path`), Helix (`languages.toml`, `hx --grammar build`) and Zed (a dev extension with a `file://` grammar). Only what the repository can check is tested: the paths exist, the `agentmark.toml` sample is accepted, the settings match `package.json`. The editors themselves are not installed in CI, so their option names (for example `install_info.path`, `file-types` globs, Zed `[grammars]` keys) follow each editor's documentation as understood at writing time and may need adjusting for a given editor version. The doc says "adjust to the version of your editor".
**Why:** running three more editors in the test-suite costs more than the snippets are worth; the fallback (any LSP client plus `agentmark-lsp`) is stated first.

## Q291 — Templates carry no copy of the reference (step 18)
**Choice:** `templates/CLAUDE.md` and `templates/.cursorrules` tell the assistant to read `docs/reference.md` and, for a project outside the checkout, to keep a copy of it (suggested name `docs/agentmark-reference.md`). The templates themselves do not embed the reference, so there is one file to keep current (SPEC 20.6).
**Why:** a pasted copy in two templates would drift; the test-suite pins that both templates name `docs/reference.md` and only show commands `agentmark --help` lists.

## Q300 — `result of T` values: stored by `set`, used only by `match` (step 20)
- Question: SPEC 22.1 and example 08 write `- set parsed = json.parse(...) as list of Note` and then `- match parsed:`, but in v0.2 a call of a `result of T` function is an error outside `try`.
- Choice: `- set x = <call of a result-returning function>` (no `try`) stores the result; `x` has type `result of T` (in Rust `Result<T, AgentError>`). A result can also be matched straight from the call, `- match f(x):`. Branches: `ok v` (the `T`), `error e` (the failure message as `text`); both names are optional. A stored result can only be the subject of a `match`, once: reading it anywhere else is E313 ("holds a result, which can only be matched"), a second `match` of it is "used up" (the handles' rule, Q361), a parameter of type `result of T` is E313 too. `try f(x) -> v` (Q200, Q307) stays the other way to use such a function. The rule is generic over any result-returning call, so the library functions of 22.3 need nothing more.
- Why: it matches the SPEC and the example without making a result an ordinary value (no copies, no printing, no passing around), which keeps the failure's single source (Q200).

## Q301 — JSON form of an enum value (step 20)
- Question: how is `Circle { radius: 3 }` written as JSON (`read file`, `read body`, `respond`, a record with an enum field)?
- Choice: internally tagged, as the brief suggested: `{"kind":"Circle","radius":3}`; a variant without fields is `{"kind":"Empty"}`. Generated Rust: one struct per variant that has fields (`ShapeCircle`) and `#[derive(Serialize, Deserialize)] #[serde(tag = "kind")] enum Shape { Circle(ShapeCircle), ..., Empty }`. A variant field cannot be called `kind` (E303), because it would collide with the tag.
- Why: the tag keeps the JSON self-describing and flat, the way people write it by hand; one struct per variant lets a branch's name (`c`) be a real value whose fields are read as `c.radius`, like a record.

## Q302 — One namespace for records, enums and type aliases (step 20)
- Choice: the three share one namespace (E300 on a clash), so `- shape: Shape` resolves unambiguously. A `# Type` is replaced by the type it names the moment it is written (`Checked::type_exprs` holds the result for every written type), so sema, the IR and the generated program never see an alias; an alias defined in terms of itself is E304. `Checked::aliases` keeps them for tools.
- Why: SPEC 22.1 says an alias is "interchangeable with what it names"; resolving it eagerly is the simplest way to make that true everywhere (assignability, `=`, `match`).
- Left for step 21: `- is: map of text to money` (the SPEC example) needs the `map of K to V` type that 22.2 introduces; until then an alias can name any type the language already has.

## Q303 — Where a variant name is looked up (step 20)
- Choice: in a value, a bare name resolves in this order: variable, bare field, constant, variant, tool source. `Shape.Empty` names the enum; `billing.Empty` and `billing.Shape.Empty` work through a module alias. A bare name that several visible enums have is E307, which lists the qualified spellings. In the branches of a `match` the name is looked up in the enum being matched, so `Empty:` needs no qualifier even when another enum has an `Empty` (a qualified branch whose qualifier is another enum is E309).
- Why: SPEC 22.1 says clashes "are resolved by qualifying"; the matched enum is known, so asking for the qualifier again would only be noise. Variables and constants win because they existed first and a program that worked in v0.2 must not change meaning.

## Q304 — What a branch's name holds, and the fixed branch words (step 20)
- Choice: in `Circle c:` the name `c` has the type "the fields of Circle" (`Type::Variant`); only `c.radius` can be read, `c` itself cannot be stored, passed or written into text (it is not a value of the enum). `some x` / `ok x` bind the inner `T`, `error e` binds the failure message as `text`. The name is optional in every branch that has a value (`- some:`, `- ok:`, `- error:`, `- Circle:`); `none` and a variant without fields must not have one (E312). `some`, `none`, `ok`, `error` are only special in a `match` on a `maybe` or `result`; as variant names they stay usable in an enum.
- Why: a branch that is not allowed to hand the payload around cannot build an invalid enum value, and the rules for `some` follow `is some x`.

## Q305 — Exhaustiveness, `otherwise:` and nesting (step 20)
- Choice: a `match` without `otherwise:` must have a branch for every variant (or both of `some`/`none`, `ok`/`error`); E310 names the missing ones. A misspelled branch is reported with its suggestion and the missing-variant error is then left out (it is almost always the same mistake). `otherwise:` must be last and only one; one that nothing can reach is the warning W300. An empty branch body is allowed (it does nothing). For the hard limits of 18.3 a `match` is one level of nesting, whatever the number of branches, and its branches' statements count towards the 25.
- Why: SPEC 22.1 requires "every variant handled or an `- otherwise:` branch"; one cascade per typo is enough.

## Q306 — Enum values in expressions (step 20)
- Choice: `=` and `!=` work between two values of the same enum (derived equality). An enum value or a branch name cannot be written into text as a whole (E314), has no fields of its own (use `match`), cannot be the `as` of `ask model` (a model answers with a record) and has no `matches` check. In a record used as the `as` type of `ask model`, an enum field's JSON hint is just the enum's name.
- Why: each of these would otherwise reach rustc as a trait error, which must never happen (SPEC 10); ordering and model output for enums need design that no v0.3 example asks for.

## Q307 — Recursive enums, record literals, `try f(x)` (step 20)
- Choice: an enum that would hold itself inline (directly, in a `maybe`, or through a record or another enum) is E315, with the hint to use `list of`; the generated Rust could not store it, and boxing would leak into every field access. A branch name used as a whole value is E316. Per the controller's note on SPEC 22: `Sale { region: "EU", amount: 120.00 EUR }` builds a record the way a variant is built (a bare name that is no variant but a record; every non-`maybe` field required, same field errors); `- try f(x) -> v` (and `try module.f(x) -> v`) is the `do`-less spelling of `try do f(x) -> v` (the formatter keeps the author's spelling); `if <expr> is some x:` already accepts any `maybe` expression, calls included. `match f(x):` with `ok`/`error` stays as in Q300. `try f(x) -> v` is the canonical spelling of Q200's `try do f(x) -> v`; messages suggest it. `none` whose type nothing decides (`set d = none`, a plain record field) is E317.
- Why: SPEC 22.1/22.3 as amended; one construction syntax for records and variants.
## Q340 — `number.*` works on `int`, and on `money` where it makes sense (step 22)
- Question: SPEC 22.3 lists `round(x, places)`, `format(x, places)`, `min`, `max`, `abs` under `number`, but v0.3 has no float type (plan: no float unless an example needs one).
- Choice: `min`, `max`, `abs` and `round` take `int` or `money` (two arguments of one kind; money of two currencies is a run-time error). `round(x, places)`: a whole number has no decimals, so `places` of 0 or more leaves it alone and a negative `places` rounds to tens, hundreds, … (`round(1235, -1)` is 1240, half away from zero); for money `places` counts decimals of the whole unit (`round(12.35 EUR, 1)` is 12.40 EUR, `round(12.50 EUR, 0)` is 13.00 EUR). `format(x, places)` takes an `int` and writes exactly `places` decimals (`format(12, 2)` is `12.00`). `number.parse(t)` takes an optional sign and ignores white space around the number; `"1.5"` is an error.
- Why: no example needs a float; money rounding is the useful meaning of `round` in this language, and it stays in integer minor units.

## Q341 — Date patterns: strftime or a readable form (step 22)
- Question: SPEC 22.3 names `date.parse(t, pattern)` and `date.format(d, pattern)` without a pattern language; the brief asked for a readable one, the v0.3 examples (07, `"%Y%m%d"`) use strftime.
- Choice: both. A pattern containing a `%` is chrono's strftime, as written (`%Y-%m-%d`, `%Y%m%d`). Any other pattern is the readable form: `YYYY` `YY` `MMMM` (October) `MMM` (Oct) `MM` `DD` `dddd` (Saturday) `ddd` (Sat) `HH` `mm` `ss`; every other character stands for itself and `[text]` protects letters (`"[Day] DD"`). `date.format` also takes a `datetime` (a `date` has time 00:00); `date.parse` gives a `date` and drops a time of day it reads. A literal pattern that is not valid is compile error E343; a computed one that is not is a run-time error in `format` and an error value from `parse`. `json.encode` / `csv.encode` refuse handle-typed values and `json.parse` / `csv.parse` cannot read a handle (E342); `csv.encode` wants a list of records. `date.days_between(a, b)` is `b - a` in days.
- Why: the examples are the contract; the readable form costs one function and suits patterns people write from memory.

## Q342 — Calls that return `result of T`: `try`, `do`, `as T` (step 22)
- Question: SPEC 22.3 writes `- try <call> -> var`, `json.parse(t) as T`, `csv.parse(t) as list of T`; the examples write `- try do csv.parse(content) as list of Sale -> sales`.
- Choice: both `- try <call> -> var` and `- try do <call> -> var` parse (`do` is optional and the formatter keeps the spelling). `as <type>` follows the call directly and only `json.parse` and `csv.parse` take it (`csv.parse` needs `list of <record>`); without it E342 says what to write. Outside `try` a call of a `result` function is E341 ("can fail … has to be called with try"). The statement is `StmtKind::CallInto { call, into, via_do }`, the typed call `ExprKind::Decode { call, ty }`; in the IR an expression `ExprKind::Std { module, name, args, decode }` and, for the failing ones in `try`, `EffectCall::Std`. `- match <call>:` over a library `result` needs the checker flag `Cx.std_ok` set where Task 20's `Cx.match_call` is set (step 20 owns `match`; `Std` calls evaluate to a Rust `Result` there, without `?`).
- Why: one spelling per SPEC and one per example; unknown functions and wrong uses fail at check time, never in rustc.

## Q343 — `regex.*`: dialect, groups, bad patterns (step 22)
- Choice: Rust `regex` syntax (no look-around; `$1` in `replace`; `$$` is a dollar sign), unanchored. `captures` returns the capture groups without the whole match (a group that took no part is empty text); a pattern without groups gives an empty list. A literal pattern, or a constant holding one, is compiled by the checker (E343); a pattern built at run time that does not compile is a run-time error with the regex message (these functions have `rt_err` in the table, so generated code writes `?`). Compiled patterns are cached per process.
- Why: Q401 of step 25 fixes the dialect the examples use; a bad pattern is a programmer error, not a value.

## Q344 — The `file` tool (step 22)
- Choice: `file` is a built-in tool of effect `local`, shown as `local(file)` in `check`. The parser reads `file <verb> <path> [to <new> | : <text>] [-> name]` itself (`StmtKind::File`; the path and the other operands are full expressions, so `file rename path.join(a, b) to path.join(a, c)` works) and `file` is a reserved statement word. A `[file]` table in `tools.toml` is E347. Verbs and their shapes are rows of `stdlib::FILE_VERBS`: `read` (gives `text`, a `result`: E341 without `try`), `write` (exactly the text; replaces), `append` (the line plus `\n`; creates the file), `list` (names of files and folders, sorted by name), `rename` (a plain error when the target already exists), `exists` (file or folder), `delete` (a file or an empty folder). Paths are used as written (relative to the working directory); there is no sandbox. Every call is traced `{"kind":"file","verb":…,"path":…}` (`rename` adds `"to"`) **before** it runs, so a failing call is traced too. Failures are `RtError::Lib` with plain text (`cannot read "x": it does not exist`).
- Why: SPEC 22.3 plus Q400; tracing the attempt shows what a program tried when it failed.

## Q345 — `read input -> text` (step 22)
- Choice: reads all of standard input as `text` (effect `local(input)`, not traced). The statement is `StmtKind::ReadInput`.
- Why: SPEC 22.3 names it; a program that reads stdin needs no tool entry.

## Q346 — `json.*` and `csv.*` decoding (step 22)
- Choice: `json.parse(t) as T` is serde: record fields are the JSON keys, `money` is text like `"12.50 EUR"`, `date` is `"2026-10-13"`, a `maybe` field may be missing or null. `csv.parse(t) as list of T` reads the first row as field names; cells are read as the field's type (money `120.00 EUR`, date `2026-10-01`, whole numbers, `true`/`false`; an empty cell is an empty `maybe`), white space around a cell is ignored, a text field keeps `007` as written. Errors are plain: `line 3, column 2: …`, `line 2: the row has 2 cells, the header has 3`. `json.encode(v)` is compact JSON; `csv.encode(rows)` writes a header from the first row's field names then one line per row (cells: text as is, numbers, bools, money/date as their text, an empty `maybe` as empty).
- Why: the generated records are already serde types; the csv crate's typed reading gives exact per-field behaviour without a second parser.

## Q347 — Library names lose to the author's names (step 22)
- Choice: `text.split(...)` is a library call only when no function of that name is imported (a module alias called `text` with a `split` function wins). A variable called `path` or `text` does not matter: only the call form `module.fn(` reaches the library.
- Why: existing programs keep their meaning when the library grows.

## Q348 — Generated projects without records or state build warning free (step 22)
- Choice: `records.rs` writes its `use serde::…` line only when there is a record, and `run_command` / `run_command_file` carry `#[allow(unused_variables)]` (`mock` is unused when no command has state). Examples 07 and 09 are record-less commands.
- Why: `build_bin` in the tests requires a warning-free build.

## Q349 — New dependencies of agentmark-rt (step 22)
- Choice: `regex` and `csv` (and its `csv-core`, fetched once from the network: the offline cache did not hold it) in `agentmark-rt`; every generated project gets them through the runtime crate. `chrono`'s `serde` feature is a dev-dependency of the runtime tests only (generated projects enable it themselves).
- Why: SPEC 22.3 needs a regex engine and CSV reading; both are small and widely used.

## Q360 — Binding-file syntax for handles and receivers (step 23)
- Question: SPEC 22.4 shows the statement forms (`- hasher update data`, `- hasher finalize -> digest`) but not how a binding file declares a handle, a method, or how a call holds the handle.
- Choice: the type is `handle of <crate>.<Type>` (dotted path below the crate: `reqwest.header.HeaderMap`; a crate name with `-` is written with `_`). A method is an item whose FIRST parameter is named `self` and has a handle type; the word in front of `handle of` says how the call holds it, in a binding parameter only: none = borrowed (`&T`, reads it), `mut` = `&mut T` (changes it), `consume` = `T` (uses it up). The same words apply to every other handle parameter (`other: consume handle of sha2.Sha256`). A returned `handle of …` is owned. The word is part of the type syntax (`TypeKind::Handle { name, mode }`) and sema refuses it anywhere else (E365). Example: `## update(self: mut handle of sha2.Sha256, data: bytes)`.
- A constructor is an ordinary item and is called on the library (`- sha2 sha256_new -> hasher`, SPEC's `- sha2 sha256 new` is read as one verb, since verbs are single words, Q74). A method is called with the variable as the first word; the verb is looked up among the items whose `self` has the variable's type, so `update` can exist for Sha224 and Sha256 in one binding. In the Rust body the receiver is `self_` (`self` is a keyword; sema's naming already adds `_`).
- Why: three borrow modes are what Rust signatures contain; spelling them in the type keeps `Sig::heading` one line and the parser's `params` rule unchanged. In user code (functions, `## Rust` blocks) a handle parameter is owned, because there is no way to say "borrow" for a user function and a move plus `return h` covers feeding a hasher in a helper. Amends Q74 for handle receivers (named `self`, not after the type).

## Q361 — Ownership: what uses a handle up, and how branches and loops join (step 23)
- Question: SPEC 22.4 says a `self`-by-value method consumes the handle and a later use is a compile error naming the consuming line.
- Choice: a handle variable is used up by (1) a `consume` receiver or `consume` argument of a binding call, (2) being passed to a user `# Function` / `## Rust` block (`do f(h)` or `f(h)` in an expression), (3) `set other = h`. `Cx.consumed` maps the variable to the span of that line; E361 prints "`h` was used up on line N ("<that line>")". A new `->`/`set` definition of the same name revives it. After an `if`/`else` the handle is used up when ANY branch that does not end in `stop`/`return` used it up ("may have been", the message still names the line). A handle that exists before a loop and is used up inside it is E361 at that line (the next round would use it again) unless the body ends in `stop`/`return`. `try` and `parallel:` need no rule: they check in order.
- Why: this is Rust's move rule on the straight-line subset AgentMark has, reported in AgentMark's own words; the conservative join costs a rare false positive and never a runtime failure. The generated Rust is therefore always valid: codegen moves the variable and never clones it (`val()` skips handles).

## Q362 — Where a handle may be read (step 23)
- Question: SPEC: a handle "cannot be printed, compared, put in a record that is serialized, or traced".
- Choice: default deny at ONE place. `path_expr` on a bare handle variable is E362 unless the caller set `Cx.handle_ok`: `set` right-hand side, arguments of `do`/calls and of binding calls, `return`. So printing, `{interpolation}`, `=`/`!=`/ordering, record literals and `respond` bodies, `ask model with`, tool payloads and targets, `append`, conditions and everything a later construct adds (Task 20's `match`) are refused without listing them. A handle is a single value: `list of handle…` / `maybe handle…` (E363), as a record field (E363); `result of handle…` is allowed (a fallible constructor in a function). `try … -> h otherwise:` is E366 unless the `otherwise` branch leaves, since `default_expr` has no handle.
- Amended (review): the check is by TYPE at every consumer, not by the shape of the expression: `expr()` refuses an expression of handle type (a call such as `mk()`) unless the caller sanctions it (`set`, arguments, `return`), and `path_expr` does the same for a bare variable. A handle cannot come from outside the program either (command argument, `read body`, `read file`: E367). When Task 20's `match` lands it needs the same consumption join as `if` (a handle used up in one arm is "may have been" used up after the match, arms that `stop`/`return` contribute nothing).
- Why: a list of live library values or a handle in a trace line would need a defined meaning for clone/serialize that the library does not give; one deny point cannot rot when new expression forms arrive.

## Q363 — Item names: unique key vs heading name (step 23)
- Question: Q74 requires unique item names; methods of six hash types all want to be called `update`.
- Choice: `Item.name` stays the unique, type-prefixed key (`sha256_update`, the key of `## Not bound`, placements, `.prev` merges, on-demand), and `Item.heading` (`Some("update")`) is what the `##` heading shows for a method; a heading without one shows `name`. IR: `BindingFn.on = Some("sha2.Sha256")` and `ir::binding_key(item, on)` (`sha2.Sha256::update`) name the Rust method; codegen assigns each binding item its own `impl` method name, so six `update`s become `update`, `update_2`, …
- Why: nothing keyed by name had to change; only rendering and the sema/IR lookup know about methods. E364 ("no such method") is intentionally unused: an unknown method of a handle is E207, the code of an unknown verb of a binding, with a did-you-mean over that type's methods.

## Q364 — Which Rust types become handles (step 23)
- Question: "a Rust type that has no AgentMark equivalent" is unbounded.
- Choice: only a public, non-generic type of the BOUND crate: a struct, enum or union reachable by a public path (the walk's `pub_paths`), or a type alias without parameters whose target is not a std type (`sha2::Sha256 = CoreWrapper<…>`). The dotted name uses the public path (`sha2.Sha256`); a signature that names the type by its canonical path (`sha2::core_api::Sha256VarCore`) is mapped to it. Types AgentMark already has (NaiveDate, DateTime<Utc>, Duration, `Url`/`Path` as text, `Output`/`GenericArray` as bytes) stay values; types of other crates, generic types and private types stay "no AgentMark type". A returned borrow of a handle, and handles inside `Vec`/`Option`/slices, are skipped with that reason. The trimmed fixtures now keep `generics` of structs and aliases (`trim.py`); a rustdoc file without them has no handles.
- Why: the scratch crate used to type-check a body depends only on the bound crate and chrono, so a handle of any other crate could not be written there; generic types would need type arguments in every path.

## Q365 — Tier rules for items with a handle (step 23)
- Question: SPEC: "assisted/direct tier rules extended".
- Choice: the rule writer produces the body (`Ok(self_.m(args))`, `<T as Trait>::m(self_, args)`, `Ok(T::new(args))`, parameters passed as they are, other values converted as before, `impl AsRef<[u8]>` passed as the `Vec<u8>` it stands for, a returned `Output<Self>` as `.to_vec()`), marks it `Shape::Direct { verify: true }`, and the pipeline runs the type checker on it; a rejected body lands under `## Not bound` with the compiler's reason, no model call. Allowed for such items: generics of the function itself, another crate's trait (Q73's "always assisted" is for items without a handle, which keep it), `where Self: Bound` on the type (rustc decides whether it holds: `Digest::reset` needs `Self: Reset`). Still assisted: `async`, `Result`-returning (so the error conversion is checked by the model's body), and generics of the `impl` itself (`impl<Tz> … for DateTime<Tz>`: the path would need type arguments). A body line that would pass 120 characters is laid out one argument per line; a heading over 120 characters is listed as not bound with that reason (SPEC 18.3, headings cannot wrap).
- Why: 42 of sha2's 81 v0.2 `## Not bound` items need exactly this and no judgement; asking a model for `Ok(self_.update(data))` 42 times costs more and is less reliable than the compiler checking a rule. The brief's ratio test (at least half bound) runs against the checked-in fixtures: 42 of 81.

## Q366 — Foreign trait on handle aliases; the alias line (step 23)
- Choice: for a crate that re-exports a trait and aliases the other crate's types (sha2/Digest, Q73) the alias is a handle. ALL the trait's methods are bound on it, receivers included (before, `self` methods were one `takes self` line per method). When at least one was emitted, the `alias_<name>` line "…its methods live there" under `## Not bound` is removed (the alias is now a handle type, visible in the signatures); with no foreign rustdoc supplied the alias line stays. `&mut` buffer parameters (`finalize_into(&mut Output<Self>)`) stay skipped ("is a mutable borrow").

## Q367 — Async library methods (step 23)
- Choice: a handle method that is `async` (`reqwest::Response::text`) is an assisted item like any async function (Q70): its body is the body of the binding's `async fn`, may `.await`, and the generated call site is `bindings::Reqwest::text(resp).await?` for every binding item already, so no new rule was needed. Tested with the real compiler for `reqwest::get` and `Response::text` and end to end with a hand-written binding body that awaits (`tokio::task::yield_now().await`).

## Q368 — Handles and the on-demand tier; editors (step 23)
- Choice: an on-demand statement (`- sha2 sha224_finalize_into x`) still works for a `## Not bound` constructor or function; a method of a handle cannot be written on demand, because the statement's first word is a variable, not the library, and the CLI looks for `<library> <item>` lines. A skipped method stays listed and a `## Rust` block remains the way. The LSP does not offer methods after a handle variable (sema's per-variable types are not in `Checked`), and no longer lists methods as verbs of the binding. tree-sitter reads `[mut|consume] handle of a.b` as ONE token (`handle_type`), so `handle`, `mut` and `consume` stay plain identifiers elsewhere; the TextMate grammar scopes the words in binding headings.

## Q369 — Examples and docs for handles (step 23)
- Choice: example 05's checked-in `bindings/sha2.agent.md` is NOT regenerated: it is the v0.2 file (`1 bound, 81 not bound`) that the README and `tests/handles.rs` quote as the baseline, and 05 does not use handles. The v0.3 examples use whatever binding their authors generate. docs/reference.md describes handles inside 15.4 (the reference mirrors SPEC's section numbers and SPEC has no 15.6); `docs/libraries.md` section 4a is the tutorial. tests/docs.rs `reference_has_one_section_per_spec_section_in_order` fails on this branch before and after step 23, because SPEC 22 (step 19) has no section in the reference yet; step 26 writes it.

## Q370 — Handles own lifetimes and single-instantiation generics; the `## Handle:` block (step 23, after the dev amendment of SPEC 22.4)
- Question: SPEC 22.4 (16e592a) says a handle owns whatever its Rust value borrows, and a generic type whose parameters have one instantiation in the crate as configured (`sqlx::query` with `sqlite`, `DB = Sqlite`) is bound through the assisted tier, not skipped.
- Choice: a public struct/enum/union may have lifetime and type parameters (const parameters: no). A lifetime is written `'static`. A type parameter is the one instantiation the signature gives after the lift: a generic with a bound resolves to the crate's only concrete implementor of that trait (`DB: Database` with a single `impl Database for Sqlite` in the rustdoc of the configured build; two implementors = no obvious instantiation, as before), and the first instantiation seen wins (a second, different one makes the item skip). Qualified paths (`<DB as Database>::Arguments<'q>`) are kept and written with `'static`. The resulting Rust type goes to `CrateItems.handle_types` and into the binding as a block, `## Handle: sqlx.query.Query` with the type in a `rust` fence (only for handles that are not plain `<crate>::<Type>`, so sha2 and chrono files do not change). Parser `Binding.handles`, sema `BindingInfo.handles` (E360 for a block no item uses), IR `Program.handle_rust`, codegen `Names.handle_rust` and the scratch crate's contract header all read it.
- Items that have lifetimes or associated types AND a handle go to the assisted tier with "the handle owns what the Rust value borrows" (the prompt names the Rust type and says to copy, clone or leak what the call borrows); the same items without a handle are still skipped with the old reasons ("lifetimes in the public API"). If lifting fails for another reason while the item has lifetimes, the old lifetime reason is reported (it is the first thing to fix).
- Why: the Rust type cannot be derived from the handle's name once it has parameters, and the generated project needs it in every signature; a block in the binding keeps the file the only source. Tested with a synthetic `sqlx`-shaped rustdoc (`tests/handles.rs`); the real sqlx binding is built end to end with example 08.

## Q371 — `[crates]` in `agentmark.toml` (step 23)
- Choice: `[crates] <name> = "<req>"` or `{ version = "<req>", features = [...], default-features = false }`. The binder (rustdoc build, the scratch crate that type-checks bodies, the dependency rustdoc) and the generated `Cargo.toml` use the features and `default-features`; a crate without an entry keeps its defaults, as before. `version` is accepted and kept in `Config.crates`, but the exact version still comes from the binding header (Q155), so `Cargo.toml` pins `=<header version>`. Unknown keys of an entry are an error with a suggestion; the VS Code schema knows the table. `BindOptions.crate_features` is per crate, because `bind --all` binds several crates with one option set.

## Q372 — Integers wider than `int` (step 23)
- Choice: `u64`, `usize` and `isize` parameters convert with `T::try_from(x)?` and results with `i64::try_from(x)?` in the rule-written (direct) bodies, so a value that does not fit fails the call instead of wrapping; inside `Vec`/`Option` the conversion runs in a closure that returns the error. Such items used to go to the assisted tier. `u128`/`i128` still have no AgentMark type. A rule-written body that is still over 120 characters after one-argument-per-line layout goes to the assisted tier.

## Q373 — Review fixes to handles (step 23)
- `fmt` prints `## Handle:` blocks. A handle made again with `->` inside a branch or loop is a new variable of that block (the generated code declares a new one): sema shadows it and restores the outer one's state when the block ends (a handle used up outside stays used up). Single-implementor instantiation is keyed by the trait's resolved path, tried only after the v0.2 lift fails and kept only if the item then involves a handle; the Rust type of a handle is recorded only for an item that is bound or goes to the model. `parallel:` counts a handle in any argument, not only as receiver.

## Q400 — `text.pad`, `text.lines`, `file write` / `file append` (step 25)
- Question: SPEC 22.3 names `pad(t, width)`, `lines(t)`, `file write`, `file append` without saying exactly what they do.
- Choice used by examples 06, 09, 10: `pad` adds spaces on the right up to `width` and never cuts text that is longer; `lines` splits like Rust `str::lines` (no empty entry for a final newline, an empty line in the middle is kept); `file write path: content` writes exactly `content` (examples add `\n` themselves); `file append path: line` adds `line` plus a newline.
- Why: the expected outputs in `fixtures/06`, `09`, `10` depend on it.

## Q401 — Regex dialect inside text literals (step 25)
- Question: SPEC 5 allows only the escapes `\"`, `\\`, `\n`, `\{`, `\}` in text, and a `{` starts an interpolation hole; regexes need `\d` and `{4}`.
- Choice: the examples never use `\d`, `\w` or `{n}`. They write `[0-9]`, `[a-z]`, `[.]`, and `\\[` where a literal bracket is needed. `regex.captures` returns the capture groups only (not the whole match), as `maybe list of text`; `regex.replace(t, pattern, with)` replaces every match and uses `$1` for group 1 (Rust `regex` syntax); `regex.find` and `captures` take the first match.
- Why: keeps the patterns readable and independent of an escape rule the SPEC does not state.

## Q402 — `path.*` and `date.*` details (step 25)
- Choice: `path.extension("a/b.jpg")` is `jpg` (no dot); `path.join("photos", "x.jpg")` is `photos/x.jpg`; `date.parse(t, pattern)` and `date.format(d, pattern)` use strftime patterns (chrono), so `%Y%m%d` parses `20261003` and `20261399` is an error.
- Why: `fixtures/07` depends on it.

## Q403 — Command flags and defaults (step 25)
- Choice: `dry_run: bool = false` is passed as `--dry-run` with no value; `top: int = 3` as `--top 2`; positional arguments come first in declared order (SPEC 22.6).
- Why: `fixtures/07/variants/dry-run/run.json` and `fixtures/10/run.json`.

## Q404 — Fixture conventions for non-agent programs (step 25)
- Decided by SPEC 22.7: variant folders inherit what they lack; `expect_status` in `requests.json`; `expected/` is the exact file set of the working directory (06 and 09 therefore list their input file as well).
- Choice (still ours): programs that create files use paths relative to the fresh working directory (08 creates `notes.db` there); 08 has no `expected.trace.jsonl` because the trace lines of the sqlx calls are not known yet.

## Q405 — Record construction and `try` on library calls (step 25)
- Decided by SPEC 22.1 and 22.3: `Name { field: value }` builds a record; `try <call> -> var` works on built-ins that return `result` (`do` optional).
- Choice: the examples write `try csv.parse(content) as list of Sale -> sales` and `try date.parse(digits, digit_date_format) -> day` without `do`, everywhere.

## Q406 — 08 notes API: what the sqlx binding must provide (step 25)
- Choice: see the table "Binder items 08 relies on" in `examples/V03-FEATURES.md`. Verbs follow the v0.2 naming rule (`SqlitePool::connect` becomes `sqlite_pool_connect`). Rows are never read through handles: the SQL builds JSON text (`json_object`, `json_group_array`) and the program parses it with `json.parse(...) as Note`.
- Why: sqlx row access is generic (`row.get::<T, _>`); JSON text keeps the binding to a handful of monomorphic items.

## Q407 — Example layout details (step 25)
- Choice: the `# Enum: Block` of 09 lives in `records/` (types live with records; 18.3 allows several declarations there). The non-agent examples have no `tools.toml` (SPEC 22.5: not needed with only `file` and `# Tool`). Service 08 has no `state:` (optional).
