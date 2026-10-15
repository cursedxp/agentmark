#!/bin/bash
# Claude Code Stop hook for AgentMark. Exit 2 = block the stop and feed stderr back to Claude.
#  - docs rule: code/editor/SPEC changes need a docs/README/CHANGELOG change too
#  - example rule: examples must stay fmt-clean and lint-clean (uses an existing binary, never builds)
# Escape hatch for the docs rule: create .claude/no-docs-needed (gitignored).
# Works with bash 3.2 (macOS) and Linux; no jq needed.

INPUT=$(cat 2>/dev/null || true)
if printf '%s' "$INPUT" | grep -Eq '"stop_hook_active"[[:space:]]*:[[:space:]]*true'; then
  exit 0
fi

ROOT=${CLAUDE_PROJECT_DIR:-$(pwd)}
cd "$ROOT" 2>/dev/null || exit 0
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || exit 0

# ---- changed files: uncommitted (not ignored) + commits on this branch not on main ----
CHANGED=$(
  {
    git status --porcelain -uall 2>/dev/null | sed -e 's/^...//' -e 's/.* -> //' -e 's/^"//' -e 's/"$//'
    for base in main origin/main; do
      if git rev-parse --verify -q "$base" >/dev/null 2>&1; then
        git diff --name-only "$base...HEAD" 2>/dev/null
        break
      fi
    done
  } | sort -u | grep -v '^$'
)
[ -z "$CHANGED" ] && exit 0

CODE=$(printf '%s\n' "$CHANGED" | grep -E '^(src/|crates/|editors/|SPEC\.md$)')
DOCS=$(printf '%s\n' "$CHANGED" | grep -E '^(docs/|README\.md$|CHANGELOG\.md$)')
STATUS=0

# ---- docs rule ----
if [ -n "$CODE" ] && [ -z "$DOCS" ]; then
  if [ -e .claude/no-docs-needed ]; then
    echo "stop-guard: docs rule skipped (.claude/no-docs-needed exists). Delete it when done." >&2
  else
    {
      echo "stop-guard: code changed but no docs changed. Changed code files:"
      printf '%s\n' "$CODE" | head -20 | sed 's/^/  - /'
      echo
      echo "Before stopping, finish the Definition of done (see CLAUDE.md, or run /ship):"
      echo "  1. docs/*.md, README.md and CHANGELOG.md (Unreleased) updated for user-visible changes"
      echo "  2. templates/ and STATUS.md updated if affected"
      echo "  3. docs/dev/OPEN_QUESTIONS.md entry for every new decision"
      echo "  4. editors (VS Code grammar, tree-sitter) updated if syntax changed"
      echo "  5. docs site rebuilt and republished to the artifact URL in CLAUDE.md"
      echo "If this change really needs no docs: touch .claude/no-docs-needed (gitignored) and stop again."
    } >&2
    STATUS=2
  fi
fi

# ---- example rule ----
if printf '%s\n' "$CHANGED" | grep -Eq '(\.agent\.md$|^src/)'; then
  BIN=""
  for c in "${AGENTMARK_BIN:-}" "${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/debug/agentmark}" \
           "$HOME/.cargo/shared-target/debug/agentmark" "$ROOT/target/debug/agentmark"; do
    if [ -n "$c" ] && [ -x "$c" ]; then BIN=$c; break; fi
  done
  [ -z "$BIN" ] && BIN=$(command -v agentmark 2>/dev/null || true)
  if [ -z "$BIN" ]; then
    echo "stop-guard: no agentmark binary found, skipping the examples fmt/lint check." >&2
  elif [ -x tools/guard/check-examples.sh ]; then
    OUT=$(tools/guard/check-examples.sh "$BIN" fmt lint 2>&1)
    if [ $? -ne 0 ]; then
      {
        echo "stop-guard: examples fail fmt --check / lint (binary: $BIN):"
        printf '%s\n' "$OUT" | head -60
        echo "Fix them (agentmark fmt <example>) or rebuild the binary if it is stale."
      } >&2
      STATUS=2
    fi
  fi
fi

exit $STATUS
