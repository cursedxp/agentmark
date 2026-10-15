#!/bin/bash
# Tests for .claude/hooks/stop-guard.sh using throwaway git repos. No build, no network.
HOOK=$(cd "$(dirname "$0")/../.." && pwd)/.claude/hooks/stop-guard.sh
CHECKER=$(cd "$(dirname "$0")" && pwd)/check-examples.sh
TMP=$(mktemp -d "${TMPDIR:-/tmp}/stopguard.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
PASS=0; FAILN=0

# new_repo <name>: repo on main with one commit, then branch "work"
new_repo() {
  R=$TMP/$1; mkdir -p "$R/src" "$R/docs" "$R/examples/e1" "$R/tools/guard"
  cp "$CHECKER" "$R/tools/guard/check-examples.sh"
  ( cd "$R" && git init -q -b main . && git config user.email t@t && git config user.name t \
    && echo a > src/a.rs && echo d > docs/guide.md && echo m > examples/e1/main.agent.md \
    && printf '.claude/no-docs-needed\n' > .gitignore \
    && git add -A && git commit -qm init && git checkout -qb work )
}

# run_hook <repo> <stdin-json> [VAR=val ...]; sets RC and ERR
run_hook() {
  local repo=$1 input=$2; shift 2
  ERR=$(printf '%s' "$input" | env CLAUDE_PROJECT_DIR="$repo" AGENTMARK_BIN=/nonexistent HOME="$TMP/home" PATH=/usr/bin:/bin "$@" "$HOOK" 2>&1 >/dev/null)
  RC=$?
}

expect() { # name want_rc [grep-pattern]
  if [ "$RC" = "$2" ] && { [ -z "$3" ] || printf '%s' "$ERR" | grep -q "$3"; }; then
    echo "ok   $1 (rc=$RC)"; PASS=$((PASS+1))
  else
    echo "FAIL $1: rc=$RC want $2; stderr: $ERR"; FAILN=$((FAILN+1))
  fi
}

new_repo t1; run_hook "$R" '{}';                                   expect "no changes -> 0" 0
new_repo t2; echo x >> "$R/src/a.rs"; run_hook "$R" '{}';          expect "src change without docs -> 2" 2 "src/a.rs"
new_repo t3; echo x >> "$R/src/a.rs"; echo y >> "$R/docs/guide.md"; run_hook "$R" '{}'; expect "src + docs -> 0" 0
new_repo t4; echo x >> "$R/src/a.rs"; mkdir -p "$R/.claude"; touch "$R/.claude/no-docs-needed"
run_hook "$R" '{}'; expect "escape hatch -> 0 and says so" 0 "no-docs-needed"
new_repo t5; echo x >> "$R/src/a.rs"; run_hook "$R" '{"stop_hook_active": true}'; expect "stop_hook_active -> 0" 0
new_repo t6; mkdir -p "$R/tests"; echo x > "$R/tests/t.rs"; run_hook "$R" '{}'; expect "tests-only change -> 0" 0
# committed on the branch (not uncommitted) still counts
new_repo t7; echo x >> "$R/src/a.rs"; ( cd "$R" && git commit -qam code ); run_hook "$R" '{}'; expect "committed src change w/o docs -> 2" 2
new_repo t8; echo x >> "$R/src/a.rs"; ( cd "$R" && git commit -qam code ); echo y >> "$R/CHANGELOG.md"; run_hook "$R" '{}'; expect "committed src + uncommitted CHANGELOG -> 0" 0
# example rule with a fake binary
printf '#!/bin/sh\nexit 1\n' > "$TMP/bad"; printf '#!/bin/sh\nexit 0\n' > "$TMP/good"; chmod +x "$TMP/bad" "$TMP/good"
new_repo t9; echo x >> "$R/examples/e1/main.agent.md"
run_hook "$R" '{}' AGENTMARK_BIN="$TMP/bad";  expect "example fmt/lint failure -> 2" 2 "FAIL: agentmark fmt"
run_hook "$R" '{}' AGENTMARK_BIN="$TMP/good"; expect "example clean -> 0" 0
run_hook "$R" '{}';                           expect "no binary -> 0 with note" 0 "no agentmark binary"

echo "passed=$PASS failed=$FAILN"
[ "$FAILN" = 0 ]
