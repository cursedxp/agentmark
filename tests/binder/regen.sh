#!/bin/sh
# Regenerate tests/binder/{sha2,chrono,reqwest}.json: rustdoc JSON built with the installed
# nightly from the pinned crate versions (offline, from the local cargo registry cache), then
# trimmed by trim.py to what the binder reads. Needs `cargo +nightly` and python3.
#
#   sh tests/binder/regen.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/src"
: > "$scratch/src/lib.rs"
cat > "$scratch/Cargo.toml" <<'EOF'
[package]
name = "agentmark_binder_fixtures"
version = "0.0.0"
edition = "2021"

[workspace]

[dependencies]
sha2 = "=0.10.9"
chrono = { version = "=0.4.45", default-features = false }
reqwest = { version = "=0.12.28", default-features = false }
EOF
cd "$scratch"
for c in sha2 chrono reqwest; do
  cargo +nightly rustdoc --offline --quiet -p "$c" --target-dir "$scratch/target" -- -Z unstable-options --output-format json
  python3 "$here/trim.py" "$scratch/target/doc/$c.json" "$here/$c.json"
done
