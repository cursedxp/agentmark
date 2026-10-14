#!/usr/bin/env bash
# Parses every example .agent.md file and every ```agentmark block in the docs
# with the tree-sitter grammar and fails on any ERROR or MISSING node. Also
# runs the grammar's own corpus (`tree-sitter test`).
#
# Needs node and npx (it fetches tree-sitter-cli from npm on first use).
set -euo pipefail

GRAMMAR_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(cd "$GRAMMAR_DIR/../.." && pwd)"
TS=(npx --yes tree-sitter-cli@0.25)

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# 1. Collect inputs: every example file plus every agentmark code block in docs.
# examples 06-10 are v0.3 programs written ahead of the grammar (step 25)
# TODO(steps 20-24): remove this exclusion when the grammar knows the v0.3 syntax
find "$REPO_ROOT/examples" -name '*.agent.md' | grep -Ev '/examples/(06|07|08|09|10)-' | sort > "$WORK/files.txt"

mkdir -p "$WORK/blocks"
node - "$REPO_ROOT" "$WORK/blocks" <<'NODE' >> "$WORK/files.txt"
const fs = require('fs');
const path = require('path');
const [root, out] = process.argv.slice(2);
const sources = [path.join(root, 'README.md')];
const docs = path.join(root, 'docs');
if (fs.existsSync(docs)) {
  for (const f of fs.readdirSync(docs)) if (f.endsWith('.md')) sources.push(path.join(docs, f));
}
for (const src of sources.filter((s) => fs.existsSync(s))) {
  const lines = fs.readFileSync(src, 'utf8').split('\n');
  const base = path.basename(src, '.md');
  let n = 0;
  for (let i = 0; i < lines.length; i++) {
    const open = /^(`{3,})agentmark(\s.*)?$/.exec(lines[i]);
    if (!open) continue;
    const close = new RegExp('^`{' + open[1].length + ',}\\s*$');
    const body = [];
    for (i++; i < lines.length && !close.test(lines[i]); i++) body.push(lines[i]);
    const file = path.join(out, `${base}-${++n}.agent.md`);
    fs.writeFileSync(file, body.join('\n') + '\n');
    console.log(file);
  }
}
NODE

# A file with no final newline must parse too (editors allow it).
printf '# Record: R\n- a: text' > "$WORK/no-final-newline.agent.md"
echo "$WORK/no-final-newline.agent.md" >> "$WORK/files.txt"

total=$(wc -l < "$WORK/files.txt" | tr -d ' ')
echo "check-examples: parsing $total files"

# 2. Parse them all; any ERROR/MISSING node in the tree is a failure.
cd "$GRAMMAR_DIR"
"${TS[@]}" parse --paths "$WORK/files.txt" > "$WORK/parse.out" 2> "$WORK/parse.err" || true
if grep -nE '\(ERROR|\(MISSING|MISSING ' "$WORK/parse.out" > "$WORK/bad.txt"; then
  echo "check-examples: FAILED, the grammar left ERROR/MISSING nodes:" >&2
  cat "$WORK/bad.txt" >&2
  grep -E '^[^ ].*agent\.md' "$WORK/parse.out" | head -40 >&2 || true
  exit 1
fi
parsed=$(grep -c '^(source_file' "$WORK/parse.out" || true)
if [ "$parsed" -ne "$total" ]; then
  echo "check-examples: FAILED, expected $total parse trees, got $parsed" >&2
  cat "$WORK/parse.err" >&2
  exit 1
fi
echo "check-examples: $parsed files parse without ERROR or MISSING nodes"

# 3. The queries must compile against the grammar and produce captures.
for q in highlights injections; do
  sample="$REPO_ROOT/examples/05-invoice-service/functions/invoice-fingerprint.agent.md"
  if ! "${TS[@]}" query "queries/$q.scm" "$sample" > "$WORK/query-$q.out" 2>&1 \
     || grep -qE 'Query error|Query compilation failed' "$WORK/query-$q.out"; then
    echo "check-examples: FAILED, queries/$q.scm does not compile" >&2
    cat "$WORK/query-$q.out" >&2
    exit 1
  fi
done
grep -q 'capture' "$WORK/query-highlights.out" || { echo "check-examples: FAILED, highlights captured nothing" >&2; exit 1; }
grep -q 'injection.content' "$WORK/query-injections.out" || { echo "check-examples: FAILED, the rust fence is not injected" >&2; exit 1; }
echo "check-examples: highlight and injection queries compile"

# 3b. The collection words (SPEC 22.2) are highlighted as keywords.
printf '# Function: f\n- effect: local\n  - set ranked = sort rows by amount descending\n  - put 1 at "EU" in totals\n  - for each row in ranked with index i:\n    - stop\n' > "$WORK/collections.agent.md"
"${TS[@]}" query queries/highlights.scm "$WORK/collections.agent.md" > "$WORK/query-collections.out" 2>&1 || true
for w in sort by descending put at index; do
  grep -qE "capture: [0-9]+ - keyword, .*text: .$w." "$WORK/query-collections.out" \
    || { echo "check-examples: FAILED, \"$w\" is not highlighted as a keyword" >&2; exit 1; }
done
echo "check-examples: collection words are highlighted"

# 4. The grammar's own corpus.
"${TS[@]}" test 2>&1 | grep -vE 'parser directories|init-config|configuration file|language grammars|^$'
echo "check-examples: ok"
