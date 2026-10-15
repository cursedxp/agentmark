#!/bin/sh
# Run `fmt --check`, `lint` and `check` on every example project with a built agentmark binary.
# Usage: tools/guard/check-examples.sh <path-to-agentmark> [fmt|lint|check ...]
# With no step names, runs fmt and lint (fast). Pass "fmt lint check" for the full gate.
# Exit 0 = all clean, 1 = something failed (output is printed).
BIN=${1:?usage: check-examples.sh <agentmark-binary> [steps]}
shift
STEPS=${*:-fmt lint}
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FAIL=0
for dir in "$ROOT"/examples/*/; do
  [ -f "$dir/main.agent.md" ] || [ -f "$dir/agentmark.toml" ] || continue
  for step in $STEPS; do
    case $step in
      fmt) ARGS="fmt --check" ;;
      lint) ARGS="lint" ;;
      check) ARGS="check" ;;
      *) echo "unknown step $step" >&2; exit 2 ;;
    esac
    # shellcheck disable=SC2086
    if ! OUT=$("$BIN" $ARGS "$dir" 2>&1); then
      echo "FAIL: agentmark $ARGS ${dir#$ROOT/}"
      echo "$OUT"
      FAIL=1
    fi
  done
done
exit $FAIL
