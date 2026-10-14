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

## What failed

Nothing in the test suite. Known gaps, all recorded in `OPEN_QUESTIONS.md`:

- `azure` and `bedrock` providers (SPEC 20.2) are not implemented; `check` rejects them with a clear message (Q245).
- The Neovim, Helix and Zed snippets in `docs/editor.md` could not be run on this machine (Q290).
- Tree-sitter does not model indentation; statements in a block are siblings, which is enough for highlighting (Q260).
- `memory: file` is accepted but not implemented; `build` prints a note (Q138). `parallel:` blocks run sequentially (Q139).

## OPEN_QUESTIONS.md

139 entries (Q1–Q291, with gaps between reserved ranges). Each records the question, the choice applied, and why. The ones most worth a human look:

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

1. Ship the toolchain without the source checkout: publish `agentmark-rt` (generated projects depend on it by absolute path today, Q222) and release prebuilt `agentmark` / `agentmark-lsp` binaries the VS Code extension can download.
2. Close the v0.2 remainder: `azure` and `bedrock` providers, `memory: file`, and the opt-in `parallel:` executor for independent pure/local statements, which the IR's effect sets already support.
3. Exercise the real-world path end to end: a live run against the Anthropic API and an HTTP-backed `rust/connect.rs`, behind an opt-in test flag, plus the binder's assisted tier against a real model on a larger crate (e.g. `reqwest`) to measure how much it binds.
