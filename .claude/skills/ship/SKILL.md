---
name: ship
description: Use before declaring any AgentMark feature or fix done, and for releases. Runs tests, example fmt/lint/check, trace checks, docs updates, OPEN_QUESTIONS, docs-site rebuild and republish to the existing artifact, then a final checklist.
---

# ship

Run the steps in order from the repo root. Do not skip a step because "nothing changed there": say so in the checklist instead. Use `CARGO_TARGET_DIR=$PWD/target-guard` in front of cargo commands to avoid waiting on the shared target dir lock (then the binary is `target-guard/debug/agentmark`).

1. **Tests.** `CARGO_TARGET_DIR=$PWD/target-guard cargo test --workspace` . Must pass with 0 warnings (check the output for `warning:`).
2. **Examples fmt/lint/check.** `tools/guard/check-examples.sh $PWD/target-guard/debug/agentmark fmt lint check` . Exit 0 required. If fmt fails, run `agentmark fmt <example>` and review the diff.
3. **Traces.** `CARGO_TARGET_DIR=$PWD/target-guard cargo test --test examples_e2e` . (Already part of step 1; run alone when you only need to re-check traces.)
4. **Docs for user-visible changes.** Read `git diff main...HEAD` (and uncommitted changes). For every user-visible change update `docs/*.md`, `README.md`, `CHANGELOG.md` (Unreleased section), `templates/`, `STATUS.md`, and `editors/` (VS Code grammar, tree-sitter) if syntax changed. Delegate the doc writing to a Sonnet subagent with the diff and the list of files, then review its result yourself. `SPEC.md` is read-only; never edit it. Re-run `CARGO_TARGET_DIR=$PWD/target-guard cargo test --test docs` after doc edits (it tests the samples).
5. **OPEN_QUESTIONS.** Every new decision gets an entry in `docs/dev/OPEN_QUESTIONS.md`, following the format of the entries already there.
6. **Docs site.** `cd tools/docs-site && npm ci && npm run build && npm run check-links` . `check-links` verifies every `href="#..."` in the built page has a matching id; fix broken anchors in the docs (or `build.mjs`) and rebuild.
7. **Republish.** Publish `tools/docs-site/dist/agentmark-docs.html` with the Artifact tool to url `https://claude.ai/artifact/Jot7s4UkQ8MDZEKjPVkAP1` . If this session has not yet read that artifact, `action: "read"` it first, then publish with the same `url`. NEVER publish without `url` (that would create a new artifact).
8. **Release only.** Bump the version in the root `Cargo.toml`, every `crates/*/Cargo.toml` and `Cargo.lock`, `editors/vscode/package.json`, `editors/tree-sitter-agentmark/package.json` (and its lockfile/`tree-sitter.json` if present); rename `CHANGELOG.md` "Unreleased" to the version heading with the date and start a fresh empty Unreleased; rerun steps 1-7; merge the task branch to main. Then remind the user to tag and run `gh release create vX.Y.Z` themselves. Do not tag or release.

## Final checklist (tick every line in your final message)

- [ ] cargo test --workspace: pass, 0 warnings
- [ ] examples: fmt --check, lint, check clean
- [ ] traces (examples_e2e) pass
- [ ] docs/*.md, README, CHANGELOG (Unreleased), templates, STATUS updated or confirmed unaffected
- [ ] editors updated or syntax unchanged
- [ ] OPEN_QUESTIONS entry for each new decision (or none needed)
- [ ] docs site built, link check ok
- [ ] republished to https://claude.ai/artifact/Jot7s4UkQ8MDZEKjPVkAP1 (same URL)
- [ ] release only: versions bumped everywhere, merged, user reminded to tag + `gh release create`
