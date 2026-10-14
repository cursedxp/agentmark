# binder

Turns a library's public API into an AgentMark `# Binding` file (SPEC 15, DESIGN 6) so authors
use libraries without writing target-language code.

```
src/binder/
  core/   target-agnostic: item model, tier pipeline, binding-file writer, staleness, effects
  rust/   rustdoc JSON reader, type mapping, cargo type checker, rustdoc fetch
  mod.rs  bind(), doc_cheatsheet(), BindOptions, BindOutcome
```

`core` knows nothing about Rust; a TypeScript target would add `binder/ts/` that produces the same
`CrateItems` and a `TypeChecker`.

## API for the CLI

```rust
use agentmark::binder::{bind, doc_cheatsheet, BindOptions};

let mut opts = BindOptions::new(".agentmark/bindings", model /* Arc<dyn ModelClient> */, today);
opts.force = false;          // `agentmark bind <crate> --force`
opts.version = None;         // exact version, else rustdoc JSON's / cargo's
let out = bind("chrono", &opts)?;
// out.path        .agentmark/bindings/chrono.agent.md
// out.generated   false = cache hit, file untouched
// out.counts      Counts { direct, assisted, skipped }  (direct/assisted are 0 on a cache hit)
// out.bound       items in the file
// out.effect      "unverified" until a human edits the line
// out.reason      Some(Missing | Stale { from } | Forced) when generated

let sheet: String = doc_cheatsheet(&out.path)?;   // `agentmark doc <crate>`
```

`check` support: `core::read_effect(text)` / the parser's `Binding.effect` tell which bindings are
still `unverified`.

Foreign traits (sha2): when a crate re-exports a trait from another crate and aliases that crate's
types, `bind` also builds that crate's rustdoc JSON (`rust::fetch_dep_rustdoc_json`, or
`BindOptions::foreign_rustdoc` in tests) and binds the trait's receiver-less methods on the aliases
(`sha256_digest(data: bytes) -> bytes`; Q73).

Other entry points: `rust::fetch_rustdoc_json(crate, version)` (builds rustdoc JSON with
`cargo +nightly rustdoc`, else reads `AGENTMARK_RUSTDOC_JSON`), `rust::read_crate(json)`,
`core::run_tiers`, `core::freshness(path, version)`, `rust::CargoChecker`.

## The three tiers

| tier | rule (Rust target) | who writes the body |
|---|---|---|
| direct | non-generic, non-async, non-`Result`, every type maps 1:1 (ints up to `i64`, wider ones (`u64`, `usize`) with a checked conversion, bool, text, bytes, `NaiveDate`, `DateTime<Utc>`, `Duration`, `Vec`/`Option` of those), `self`/`&self` only | rules (`rust/types.rs` conversions), no model |
| assisted | everything else whose parameters and return can still be declared in AgentMark types: generics with one obvious instantiation, `Result`, `async` | `ModelClient`, then `cargo check` of the body against the declared signature (scratch crate, one attempt) |
| skipped | unsafe, lifetimes, associated types, closures, `&mut` of a type that is not a handle, a type with no AgentMark type, a rejected or missing model body, a heading over 120 characters | listed under `## Not bound` with the reason |

## Handles (SPEC 22.4)

A public, non-generic type of the bound crate (struct, enum, union, or a type alias such as `sha2::Sha256`) that has no AgentMark type is a
**handle**, `handle of <crate>.<path>` (`Ty::Handle`). Items that hold one are bound too: a constructor (`sha256_new() -> handle of sha2.Sha256`),
and methods with a receiver, whose first parameter is `self` and whose heading carries the method name (`## update(self: mut handle of sha2.Sha256, data: bytes)`;
`BoundItem.heading` vs the unique `name`, which stays the key of `## Not bound`). `&self` is `handle of`, `&mut self` is `mut handle of`,
`self` is `consume handle of`; other parameters take the same words (`&T`, `&mut T`, `T`); a returned handle is owned, a returned borrow, and handles inside
`Vec`/`Option`, are skipped. In the Rust body the receiver is `self_`.

Tier: an item with a handle is written by rules (`Shape::Direct { verify: true }`: generics with one obvious instantiation, another crate's trait and
`where Self: Trait` are allowed) and the type checker has the last word; `async` and `Result`-returning ones, and generics of the `impl` itself, go to the
assisted tier. A foreign trait re-exported by the crate (`sha2::Digest`) gets all its methods on the crate's handle aliases (the alias's `## Not bound` line goes).
Decisions: OPEN_QUESTIONS Q360-Q369.

Nothing is dropped: every item is bound or listed. Decisions behind the edge cases are in
OPEN_QUESTIONS Q70-Q82.

## On demand (the fourth tier)

A plain statement against a `## Not bound` item makes `check`/`build` call `bind_on_demand(crate, &[OnDemandUse], &opts, &mut log)`:
the model writes the body (and, for a skipped item with no AgentMark signature, the signature) from the statement and the rustdoc,
the same `TypeChecker` accepts it, two attempts. All items are saved together by `core::add_on_demand` (items leave `## Not bound`,
`- effect: unverified`, `.prev` kept) or the file is left alone (`OnDemandError::Failed`). The CLI side, `src/cli/ondemand.rs`, finds the
statements and prints error E220. `bind_on_demand` is `produce_on_demand` (model + type check, computes the new text, writes nothing) followed by `commit_on_demand` (atomic replace, `.prev` after all succeeded); the CLI produces every crate first and commits only if all succeeded. A failure's details (compiler text) go to `<project>/target/agentmark/ondemand-<crate>-<item>.log`. Decisions: OPEN_QUESTIONS Q230-Q235; tests: `tests/ondemand.rs`.

## Output

`bindings/<crate>.agent.md`: generated header (`> generated by agentmark bind — <crate> <version> — <date>`),
`- crate:`, `- effect: unverified`, `- proposed-effect: <pure|local|network>`, then per item a one-line
`>` description (first rustdoc sentence) above `## name(params) -> type` and the fenced Rust body.
The body is the body of `pub async fn name(..) -> Result<T, Box<dyn Error + Send + Sync>>` (Q70).

## Staleness and overwriting

- header version = wanted version: cache hit, file untouched (also for approved `- effect: pure`).
- header version differs: regenerated as `unverified` (the old file is kept as `<crate>.agent.md.prev`).
- no header (hand-written): left alone.
- `force`: regenerate regardless, even an approved binding (old file kept as `.prev`); the version stays the header's.
- `version` / `latest`: move an existing binding (`agentmark bind <crate> --version <v>` / `--latest`); without them an existing binding keeps its header version (Q155).

## Fixtures and tests

`tests/binder/{sha2,chrono,reqwest}.json` are trimmed rustdoc JSON, regenerated with
`sh tests/binder/regen.sh` (needs `cargo +nightly`, python3 and the pinned crates in the local
cargo cache; runs `trim.py`). `tests/binder/model.json` holds the canned assisted bodies for
`MockModel`. `cargo test --test binder` uses a fake type checker except for three tests that run
the real `cargo check --offline` (a sha2 body accepted/rejected, every direct chrono body compiles,
rustdoc build of sha2).

`tests/handles.rs` covers handles end to end: the sha2 ratio (at least half of v0.2's 81 `## Not bound` items are bound, 42 of them), the real `cargo check` of every handle body, sema's ownership errors and an incremental hash that builds and prints the SHA-256 of "hello world". `trim.py` keeps `generics` of structs and aliases, which tells a handle (no type parameters) from a generic type.

Moving to a newer nightly: re-run `regen.sh`, check `SUPPORTED_FORMAT` in `rust/rustdoc.rs` (Q71).
