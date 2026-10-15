# Status

## v0.1 (PROMPT steps 0–12)

`cargo test --workspace --offline` at the workspace root after step 12:

```
606 passed; 0 failed; 0 ignored — 0 warnings (fresh TMPDIR left empty)
```

What passed:

- Front end: lexer, chumsky parser, AST snapshots for every example file, deliberate error files with exact SPEC 10 messages (`tests/errors/`).
- Semantic check: types, units, field inference, effects, tool manifest, `parallel` conflicts, hard limits with suggested fixes, unused imports.
- `agent_lower` (L3 → L2) and an agent-free IR with an effect set on every node.
- Runtime crate `agentmark-rt`: `Money` (integer minor units, half-away-from-zero rounding), scheduler, `ModelClient`, `MockTools`, trace recorder.
- Rust codegen (SPEC 9.1 layout, source-map comments, user-owned `rust/connect.rs` for real tools) and the CLI: `check`, `build`, `run`, `trace`, `bind`, `fmt`, `lint`, `doc`.
- Binder: direct / assisted / skipped tiers, staleness, `unverified` approval line, `sha2` bound through the binder for example 05.
- All five examples in the canonical layout; the end-to-end test builds each one, runs it against every fixture and variant, and matches `expected.trace.jsonl` line by line; the `sha2` binding is generated once and cached.
- README and `docs/` are executed by `tests/readme.rs` and `tests/docs.rs`.

## v0.2 (PROMPT steps 13–18)

`cargo test --workspace --offline` at the workspace root after step 18:

```
705 passed; 0 failed; 0 ignored — 0 warnings (fresh TMPDIR left empty)
```

What passed:

- 13 — On-demand binding tier: a statement against a `## Not bound` item of `sha2` is written by the model, type-checked with `cargo check`, saved as `unverified` and listed by `check`; a model that fails twice stops the build with E220 naming the item, the cause (no answer / wrong signature / does not compile) and a log path — no output, binding files unchanged, all crates saved together or not at all.
- 14 — Model providers: `claude`, `claude:<m>`, `openai:<m>`, `local:<m>`, `mock`; `[models]` environment map chosen by `AGENTMARK_ENV`; one driver parser shared by runtime, generated code, binder and LSP; every provider tested against a local recorded HTTP server; `azure` / `bedrock` rejected at check time (E221).
- 15 — `agentmark-lsp`: diagnostics, completion, go-to-definition across imports, hover with the effect summary, format-on-save, code actions for every lint rule with a mechanical fix, opt-in model actions; still useful while the edited file has an error; tested over the protocol in-process and over stdio.
- 16 — `editors/vscode/`: TextMate grammar, LSP client, `agentmark.toml` schema (via `tomlValidation`), "what can this agent touch" panel from `check --json`; `vsce package` produces a 9-file vsix with no warnings, built by `tests/vscode_extension.rs`.
- 17 — `editors/tree-sitter-agentmark/`: grammar, highlights, Rust injection, 48 corpus cases; every example and every docs sample parses with no ERROR nodes (`tests/tree_sitter_grammar.rs`).
- 18 — `templates/CLAUDE.md`, `templates/.cursorrules` (both point at `docs/reference.md`), `docs/editor.md`; all checked by `tests/docs.rs`.

## v0.3 (SPEC section 22, tasks 20–30)

`cargo test --workspace --offline --no-fail-fast` in a clean worktree at the release commit:

```
920 passed; 0 failed; 0 ignored — 0 warnings (fresh TMPDIR left empty)
```

AgentMark is now a general-purpose Markdown programming language. Agents are one feature of it.

What passed:

- 20 — `# Enum`, `# Type` and `match` on enums, `maybe` and `result`. A missing variant is a compile error (E310). Record literals, `some x` / `none`, `try` without `do`.
- 21 — Collection words: `where`, `map`, `sort … by`, `group … by`, `sum of`, `count of`, `first` / `last`, `join`, `unique`, `take` / `skip`, `list of` / `map of`, `for each … with index`. They compile to loops, not closures.
- 22 — The built-in library is part of the language: `text`, `regex`, `number`, `money`, `date`, `path`, `json`, `csv`, the `file` tool and `read input`. Every function has an effect class.
- 23 — Library handles: `handle of sha2.Sha256` with shared, `mut` and `consume` methods. The binder now binds 42 of the 81 `sha2` items it left unbound in v0.2. `[crates]` sets a crate's features.
- 24 — `# Tool` written in AgentMark. `tools.toml` and `rust/connect.rs` are optional. A single command is the default command and takes program arguments.
- 25 — Five examples without an agent and without `## Rust`: 06 sales report, 07 file renamer, 08 notes API over SQLite through `sqlx`, 09 Markdown to HTML, 10 log analyzer. Each runs against its fixtures end to end. Binding items longer than 120 characters wrap into `- in:` / `- out:` lines, so SPEC 18.3 holds for bindings too.
- 26 — Docs and README reframed as a Markdown programming language, with the agent material last.
- 27 — `tools/docs-site` builds all docs into one HTML page.
- 28 — `install.sh` (one curl line, persistent source in `~/.agentmark/src`, safe uninstall) and an MIT `LICENSE`.
- 29 — Every doc page rewritten for readers with ADHD. Each page now has a short summary, a jump list, task headings, paragraphs of at most 3 sentences and tables for lookups. Error tables quote the exact message text. `docs/connect.md` is new.
- 30 — Version 0.3.0 and `CHANGELOG.md`.

## What failed

Nothing in the test suite. Known gaps, all recorded in `OPEN_QUESTIONS.md`:

- There are no prebuilt binaries. `install.sh` builds from source, and generated projects depend on the runtime in that source folder (Q222).
- `shellcheck` was not run on `install.sh`, and a second real-cargo install run (the update path) was not tested.
- Example 05 keeps its approved v0.2 `sha2` binding on purpose (Q369).
- `fmt` detects a written negation through span equality, which is correct and tested but brittle (Q419).

- `azure` and `bedrock` providers (SPEC 20.2) are not implemented; `check` rejects them with a clear message (Q245).
- The Neovim, Helix and Zed snippets in `docs/editor.md` could not be run on this machine (Q290).
- Tree-sitter does not model indentation; statements in a block are siblings, which is enough for highlighting (Q260).
- `memory: file` is accepted but not implemented; `build` prints a note (Q138). `parallel:` blocks run sequentially (Q139).

## OPEN_QUESTIONS.md

215 entries (Q1–Q424, with gaps between reserved ranges; 76 are from v0.3). Each records the question, the choice applied, and why. The v0.3 ones most worth a human look:

- Q300 — a stored `result` can be matched once (E313).
- Q369 — example 05's `sha2` binding is not regenerated.
- Q412 / Q418 — how the binder picks a concrete type for `sqlx` generics; matched by full path.
- Q416 — the wrapped binding-item form, chosen over exempting bindings from the 120-character limit.
- Q419 — `fmt` keeps the written form of a negation.
- Q422 — the real-install test runs only with `AGENTMARK_TEST_INSTALL=1`.

From v0.1 and v0.2:

- Q3 — `stop` ends the whole entry, not the current loop iteration (example 03 is written to that).
- Q4 / Q5 — how `email send customer` and `crm update invoice` resolve their address and source.
- Q32 — field-type inference uses whole-word suffixes (`createdAt` → date), not letter suffixes.
- Q34 / Q35 — "one runnable declaration per file" applies literally to project folders; long prompt strings warn instead of failing the 120-character limit.
- Q151 — text is accepted where a library expects `bytes` (UTF-8), so `sha2` needs no Rust.
- Q155 — the binding header's crate version is the lock.
- Q190 / Q203 — `--allow-unverified` treats an unapproved library as its proposed effect, with a warning.
- Q200 — `result of T` functions: a failed `try` without `otherwise` propagates to the caller.
- Q240 — model precedence: `AGENTMARK_MODEL` > `AGENTMARK_ENV` map > the agent's own `- model:`.

## Next three things

1. Ship the toolchain without the source checkout: publish `agentmark-rt` (generated projects depend on it by absolute path today, Q222) and release prebuilt `agentmark` / `agentmark-lsp` binaries through a GitHub Actions release workflow, so `install.sh` can download instead of build and the VS Code extension can fetch the server.
2. Close the v0.2 remainder: `azure` and `bedrock` providers, `memory: file`, and the opt-in `parallel:` executor for independent pure/local statements, which the IR's effect sets already support.
3. Exercise the real-world path end to end: a live run against the Anthropic API and an HTTP-backed `rust/connect.rs`, behind an opt-in test flag, plus the binder's assisted tier against a real model on a larger crate (e.g. `reqwest`) to measure how much it binds.

## v0.4 (SPEC section 23, task 31)

`cargo test --workspace --offline --no-fail-fast` on task/31:

```
1023 passed; 0 failed; 0 ignored — 0 warnings
```

AgentMark files now read as plain Markdown documents: a heading is a part, the paragraph under it is its description, bullets are properties, numbered lists are steps, links are what a program uses. Behavior is unchanged: every example's runtime trace is byte-identical to its `expected.trace.jsonl`.

Acceptance:

1. `cargo test --workspace` passes offline (above).
2. All ten examples run against their fixtures with identical traces and outputs (`tests/examples_e2e.rs`, 12 passed).
3. `agentmark fmt --check` and `agentmark lint` pass on every example with 0 warnings; `check` reports 0 errors, 0 warnings.
4. No removed syntax remains in docs, examples or templates, except where a sentence or table shows an old form next to its replacement (SPEC 23.13, reference section 23, the two templates' "these old forms are errors" line) and the `- effect:` line of bindings (Q430).
5. Every example part has a description under its heading, properties are bullets, steps are numbered, `# Uses` entries are relative links.
6. Every OPEN item of the brief is in `OPEN_QUESTIONS.md` (Q430–Q440, Q434–Q436 and Q438 among them); choices made while implementing are Q441–Q463.

What changed:

- Front end: lexer with numbered items and description paragraphs, Markdown-structure parser (`# Program`, `# Uses` links, rule headings, `- steps:`, `else:`/`otherwise:` inside steps, `add … to …:` with field bullets, `into`), errors E060–E073 for every removed form with the rewritten line in the message.
- Check: names as types in `in:`/`out:` (E230), `touches:` as a checked, optional promise (E206, E232), typed `file read … into <records>` (CSV), `# Program` start and units.
- Reports: `check` and `doc` say "What each part touches" (`region_totals: touches nothing`); `check --json` keeps the classes.
- Tools: `fmt` renumbers, reindents, writes `add` bullets and `start:` links; lint, binder file format, language server, tree-sitter and VS Code grammars, all docs and templates on v0.4.

Known gaps (see OPEN_QUESTIONS.md): `- start:` with several commands does not yet make a default command (Q441); only CSV for typed reads (Q440); headings vs `- steps:` stay inconsistent between triggers/routes/verbs and commands/functions (Q434); no migration tool ships (Q454); six old clippy warnings outside v0.4 code remain.
