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
