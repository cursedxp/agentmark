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
# digest is sha2's dependency (the home of the `Digest` trait sha2 re-exports): built in the same
# scratch crate so it has the version sha2 was built against.
cargo +nightly rustdoc --offline --quiet -p digest --target-dir "$scratch/target" -- -Z unstable-options --output-format json
python3 "$here/trim.py" "$scratch/target/doc/digest.json" "$here/digest.json" Digest

# sqlx 0.8 (example 08): the facade plus the two crates it re-exports from, built with the features of
# examples/08-notes-api/agentmark.toml. Needs the crates in the local cargo cache (fetch them once
# with network access). The facade's rustdoc JSON only names the items it re-exports; the binder merges
# the other two into it (Q411).
scratch=$(mktemp -d)
mkdir -p "$scratch/src"
: > "$scratch/src/lib.rs"
cat > "$scratch/Cargo.toml" <<'EOF'
[package]
name = "agentmark_binder_fixtures"
version = "0.0.0"
edition = "2021"

[workspace]

[dependencies]
sqlx = { version = "=0.8.6", features = ["sqlite", "runtime-tokio"] }
EOF
cd "$scratch"
for c in sqlx sqlx-core sqlx-sqlite; do
  cargo +nightly rustdoc --offline --quiet -p "$c" --target-dir "$scratch/target" -- -Z unstable-options --output-format json
  python3 "$here/trim.py" "$scratch/target/doc/$(echo "$c" | tr - _).json" "$here/$(echo "$c" | tr - _).json"
done
rm -rf "$scratch"
