# Status

## v0.1 (PROMPT steps 0–12)

`cargo test --workspace --offline` at the workspace root after step 12:

```
606 passed; 0 failed; 0 ignored — 0 warnings (fresh TMPDIR left empty)
```

What passed:

- Front end: lexer, chumsky parser, AST snapshots for every example file, 13 deliberate error files with exact SPEC 10 messages (`tests/errors/`).
- Semantic check: types, units, field inference, effects, tool manifest, `parallel` conflicts, hard limits with suggested fixes, unused imports.
- `agent_lower` (L3 → L2) and an agent-free IR with an effect set on every node.
- Runtime crate `agentmark-rt`: `Money` (integer minor units, half-away-from-zero rounding), scheduler, `ModelClient` (Anthropic + mock), `MockTools`, trace recorder.
- Rust codegen (SPEC 9.1 layout, source-map comments, user-owned `rust/connect.rs` for real tools) and the CLI: `check`, `build`, `run`, `trace`, `bind`, `fmt`, `lint`, `doc`.
- Binder: direct / assisted / skipped tiers, staleness, `unverified` approval line, `sha2` bound through the binder for example 05.
- All five examples restructured into the canonical layout; end-to-end test builds each one, runs it against every fixture and variant, and matches `expected.trace.jsonl` line by line; the `sha2` binding is generated once and cached.
- README and `docs/` (guide, reference, libraries, internals) are executed by `tests/readme.rs` and `tests/docs.rs`.

Open decisions taken while building are recorded in `OPEN_QUESTIONS.md`.
