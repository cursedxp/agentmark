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
