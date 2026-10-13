# Claude Code prompt — build the AgentMark compiler

Paste everything below this line into Claude Code, run from the folder that contains SPEC.md, examples/ and fixtures/.

---

You are building **AgentMark**, a markup-based language that compiles to standalone Rust binaries. The complete language specification is in `SPEC.md`. Read it fully before writing any code. Five example programs are in `examples/`: `01-invoice-chaser/` is a full multi-file project in the canonical layout (SPEC section 18); `02`–`05` are single-file drafts you will restructure into that layout in step 9. The tool manifest is `examples/tools.toml`, and one complete test fixture is in `fixtures/01-invoice-chaser/`.

## Goal

A Rust workspace with two crates:

- `agentmark` — the compiler, the binder, and the CLI (`check`, `build`, `run`, `trace`, `bind`), as in SPEC sections 9 and 15.
- `agentmark-rt` — the runtime crate that generated projects depend on, as in SPEC section 9.2.

`cargo test` at the workspace root must build every example in `examples/`, run it against its fixtures with `--mock`, and compare the produced trace with `fixtures/<name>/expected.trace.jsonl`. Tests compare behavior (the trace), never generated source text.

## Architecture requirements (non-negotiable)

Follow SPEC section 0 exactly:

1. Separate passes with separate test fixtures: `lexer → parser → AST → semantic check → agent_lower (L3→L2) → IR → codegen/rust`.
2. The IR must contain **no agent concepts**. `# Agent`, triggers, `ask model`, and `guard` are lowered into core constructs (functions, scheduled entry points, effectful calls, conditionals) before codegen sees them.
3. The IR must be target-independent and must already carry an effect class on every node (SPEC section 8) so that `check` can print the effect summary and a future `parallel` optimizer can use it.
4. `## Rust` blocks and `# Binding` bodies pass through verbatim with source-map comments pointing back to the `.agent.md` line.
5. Error messages follow SPEC section 10: file:line, a caret under the offending token, plain-language explanation, a "did you mean" suggestion when an identifier is within edit distance 2 of a known one. Never surface a rustc error for a mistake in AgentMark source.

## Suggested crates

- Parsing: `chumsky` (or `pest` if you prefer grammar files). Do not hand-roll a parser.
- CLI: `clap` with derive.
- Runtime: `tokio`, `chrono`, `serde`, `serde_json`, `reqwest` (Anthropic client), `axum` (for `# Service`), `thiserror`.
- Money: implement `Money { minor: i64, currency: Currency }` in `agentmark-rt`; no floats anywhere in money arithmetic.

## Work in this order

0. Add a Rust `.gitignore` and commit it.
1. **Read SPEC.md and all five examples.** Write a short `DESIGN.md` summarizing the AST, IR, and effect model you intend to implement, with one paragraph on how each example lowers to IR. Commit it and continue; I will read it in the morning.
2. Lexer + parser for the full grammar (Layers 3, 2, and 1). Snapshot tests: every example parses to a stable AST; every deliberate syntax error in `tests/errors/*.agent.md` (you create these — at least 10 covering tabs, bad indentation, unknown tool, unknown verb, unlisted tool in agent, effect violation in a Rule, unit mismatch, missing `maybe`, missing `effect` on a Function, unknown field) produces the exact error message you document in the test.
3. Semantic check: types (SPEC section 6), type inference for record fields (2.1), effects (8), tool manifest validation (7), `parallel` write conflicts (16).
4. `agent_lower`: Layer 3 → Layer 2. Unit tests that lower each agent example and assert the IR contains no agent-specific node types.
5. `agentmark-rt`: `Money`, scheduler, `ModelClient` trait with `AnthropicModel` and `MockModel`, `MockTools` with the fixture loader, `Trace` recorder. Unit-test `Money` thoroughly (rounding, currency mismatch is an error, parsing `"1200.00 EUR"`).
6. `codegen/rust`: emit a complete Cargo project per SPEC 9.1. `# Service` → axum router, `# Command` → clap subcommand, `# Binding` → impl block, `## Rust` → wrapped fn.
7. CLI: `check`, `build`, `run --mock --fixtures <dir>`, `trace --mock --fixtures <dir>`, plus the readability tooling from SPEC section 18: `fmt` (one canonical style, `--check` mode), `lint` (every rule in 18.4, configurable via `agentmark.toml`), and the hard limits in 18.3 enforced inside `check`. Tests: a deliberately messy file is reformatted to a stored snapshot; each lint rule has one positive and one negative case; each hard limit has a test proving the error message includes the suggested fix.
8. **The binder** (SPEC section 15) — this is a core feature, not an extra. Layout: `binder/core` (target-agnostic: tiering, binding-file writer, staleness, `unverified` effect handling) and `binder/rust` (rustdoc JSON reader). Implement the three tiers in order: direct (rule-based, no model), assisted (`ModelClient` with type-check of the returned body), skipped (listed under `## Not bound`). Tests: run the binder against `sha2`, `chrono`, and `reqwest` with the `MockModel`; assert every public item lands in exactly one tier, generated files parse as valid AgentMark, and `check` reports them as `unverified`. Add `agentmark bind` to the CLI and make `build` auto-bind any `# Import: crate` without a cached binding.
9. Restructure examples 02–05 into the canonical project layout (SPEC 18.1: `records/`, `rules/`, `agents/`, `main.agent.md` with `# Run`, `agentmark.toml`), using `# Constant` for repeated literals and a `> description` on every agent/service/command. Then create fixtures for examples 02–05 following the pattern in `fixtures/01-invoice-chaser/`. For 05 (service + command), the trace records HTTP requests made by an integration test against the generated axum app, plus the `import-csv` command run on a small CSV you add to the fixture. Example 05 must obtain `sha2` through the binder, not through a hand-written `## Rust` block; keep the `## Rust` block only for the `fingerprint` formatting logic.
10. Integration test: for each example, `build` → `cargo build` the output → `trace --mock` → diff against `expected.trace.jsonl`. All five must pass, and the build log must show the `sha2` binding being generated and cached once.
11. `README.md` with: what AgentMark is (3 sentences), install, the CLI commands (check, build, run, trace, bind, fmt, lint, doc), the three-line workflow for using any crate (`# Import` → `check` → approve the effect line), how to write a real tool implementation against the generated traits, and how to add a new target in one paragraph.
12. **Documentation**, in `docs/`, written for three different readers. Each file is a tutorial first and a reference second: every concept is introduced with a complete, runnable example from `examples/` before any table or rule appears.
    - `docs/guide.md` — for someone who has never programmed (an operations manager). Starts from a blank file and reaches a working agent in under 10 minutes. Explains records, rules, triggers, `ask model`, guards, and "what does the agent touch" (`check` output). Never uses the words "compiler", "IR", "binding", or "effect class"; says "library", "what it can access", "safety check" instead.
    - `docs/reference.md` — the full language, one section per SPEC section, same order, with every statement form shown once. This is the file an editor plugin or an LLM would read to write correct AgentMark.
    - `docs/libraries.md` — how to use any Rust crate: the three-line workflow, what the approval line means, what to do when something lands under `## Not bound`, and how `## Rust` blocks work for that case. Written for a developer who knows another language but not Rust.
    - `docs/internals.md` — for a contributor: the pass pipeline, where each SPEC section is implemented (file paths), how to add a statement, how to add a target, how the binder tiers work.
    - **Generated binding docs**: every binding file the binder produces must carry, above each bound statement, a one-line description taken from the crate's own rustdoc for that item, so `- pdf render doc -> bytes` is self-explanatory when a user opens the file. `agentmark doc <crate>` prints this as a plain-text cheat sheet.
    - Every code sample in `docs/` is extracted and compiled by a test (`tests/docs.rs`), so the documentation cannot silently rot.

## Phase 2 — v0.2 (only after every v0.1 test passes)

Implement SPEC section 20 in order, one commit per item, each with its own tests:

13. On-demand binding tier (SPEC 15.2 last row). Test: a statement against a `## Not bound` item of `sha2` builds with `MockModel` returning a valid body, is saved as `unverified`, and `check` lists it; a `MockModel` returning invalid Rust twice produces the documented error, never a broken build.
14. Model providers (SPEC 19): `openai`, `local`, plus the `[models]` environment map in `agentmark.toml`. Test each with a recorded HTTP fixture; no live calls in tests.
15. `agentmark-lsp` crate reusing the compiler as a library: diagnostics, completion, go-to-definition, hover with effect summary, format-on-save, code actions for mechanical lint fixes. Test with the `lsp-types` protocol directly, no editor needed.
16. `editors/vscode/`: TextMate grammar, LSP client, `agentmark.toml` JSON schema, "what can this agent touch" panel from `check --json`. Must package with `vsce package` without errors.
17. `editors/tree-sitter-agentmark/` grammar, tested against every example file.
18. `templates/CLAUDE.md` and `templates/.cursorrules` pointing at `docs/reference.md`; extend `docs/` with `editor.md` (installing the extension, what the panel shows).

## Unattended run

I will not be at the keyboard. Do not stop to ask questions. When a decision is needed, write it to `OPEN_QUESTIONS.md` with your choice and reasoning, apply the choice, and continue. If something blocks you completely (missing toolchain, network refused), write `BLOCKED.md` explaining exactly what is needed, commit, and move on to the next step that does not depend on it. Finish with `STATUS.md`: what passed, what failed, what is in `OPEN_QUESTIONS.md`, and the three things you would do next.

## Rules while working

- Do not change SPEC.md to make an example easier. If the spec is ambiguous or contradictory, write the question into `OPEN_QUESTIONS.md` with your proposed resolution, pick the resolution, and continue. I will review that file.
- Do not skip steps or merge them. Commit after each numbered step with a message naming the step.
- Keep generated code readable: a human must be able to open `src/agent_invoice_chaser.rs` and recognize the original `.agent.md`.
- If a step takes you more than one attempt to get right, add a regression test for whatever went wrong.
- Library use must stay invisible to the AgentMark author: they write `# Import: crate X`, use it in plain statements, and approve one effect line. If any step makes that flow require Rust knowledge, you have drifted from the spec.
- After step 12 and again after step 18, run `cargo test` at the workspace root and record the summary line in `STATUS.md`.

Start with step 0 now and run through step 18 without stopping.
