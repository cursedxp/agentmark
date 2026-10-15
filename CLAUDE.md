# AgentMark

AgentMark is a programming language written as Markdown (`*.agent.md`) that compiles to Rust. Rust workspace; the CLI is `agentmark` with subcommands `check`, `fmt`, `lint`, `doc`, `bind`, `build`, `run`, `trace`.

## Where things are

- `src/` compiler and CLI (parser, sema, fmt, lint, codegen); `crates/agentmark-rt` runtime, `crates/agentmark-lsp` language server
- `tests/` integration tests (`docs.rs` tests the code samples in the docs; `examples_e2e.rs` builds and runs the examples and checks traces)
- `examples/01..10` example projects (people copy these: they must stay fmt-clean, lint-clean, check-clean)
- `docs/*.md` (guide, reference, libraries, connect, editor, internals, check-json), `README.md`, `CHANGELOG.md`, `STATUS.md`, `templates/`
- `docs/dev/OPEN_QUESTIONS.md` decision log; `SPEC.md` is read-only (chmod a-w), never edit it
- `editors/vscode` (TextMate grammar), `editors/tree-sitter-agentmark`
- `tools/docs-site` renders README + docs into one HTML page; `tools/guard` holds the guardrail scripts

## Build and test

Cargo's target dir is shared machine-wide (`~/.cargo/config.toml` -> `~/.cargo/shared-target`). In a worktree or when other sessions build, avoid lock waits with `CARGO_TARGET_DIR=$PWD/target-guard` (gitignored by `target-*/`).

    cargo test --workspace                      # must pass with 0 warnings
    A=target/debug/agentmark                    # or $CARGO_TARGET_DIR/debug/agentmark
    tools/guard/check-examples.sh $A fmt lint check   # fmt --check, lint, check on every example
    cargo test --test examples_e2e              # example traces
    cd tools/docs-site && npm ci && npm run build && npm run check-links

## Definition of done

For ANY change to `src/`, `crates/`, `editors/`, `tools/` or language behavior, all of these hold before you say it is done:

a. `cargo test --workspace` passes with 0 warnings.
b. `fmt --check`, `lint` and `check` are clean on all examples (`tools/guard/check-examples.sh`).
c. `docs/*.md`, `README.md`, `CHANGELOG.md` (Unreleased section), `templates/` and `STATUS.md` are updated for every user-visible change.
d. `docs/dev/OPEN_QUESTIONS.md` has an entry for every new decision.
e. The docs site is rebuilt (`tools/docs-site`, `npm run build`, `npm run check-links`) and republished to the existing artifact https://claude.ai/artifact/Jot7s4UkQ8MDZEKjPVkAP1 . Always the SAME url, never create a new artifact.
f. Editors (VS Code grammar, tree-sitter) are updated if syntax changed.

## Guardrails

- Use the `/ship` skill (`.claude/skills/ship/SKILL.md`) before declaring any feature or fix done, and for releases. It runs these steps in order and ends with a checklist.
- A Stop hook (`.claude/hooks/stop-guard.sh`, registered in `.claude/settings.json`) blocks stopping when code changed without docs, or when the examples fail fmt/lint. If a change really needs no docs, `touch .claude/no-docs-needed` (gitignored). Test the hook with `tools/guard/test-stop-guard.sh`.
- CI (`.github/workflows/ci.yml`) runs the same checks plus the docs-site build and link check.

## Workflow

Code is delegated to Sonnet subagents working in worktrees (`task/NN-name` branches, small commits). The main session coordinates, reviews every diff, runs `/ship`, and merges. Subagents do not push or merge.
