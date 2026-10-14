# AgentMark internals

*For a contributor. You know Rust; you want to know where things live and how to change them.*

AgentMark is one Cargo package that is also the workspace root: the library and the `agentmark` binary are
`src/lib.rs` and `src/main.rs`, and the runtime that generated programs link against is the member crate
`crates/agentmark-rt/`. The language is defined by `SPEC.md`, the architecture by `DESIGN.md`, and every
choice the specification left open is a numbered entry in `OPEN_QUESTIONS.md`. When you change behaviour, change
the code, a test, and the doc that describes it (`docs/reference.md`), and add a `Q` entry when you had to
decide something. `SPEC.md` itself is never edited to make something easier.

Working rules: run `export CARGO_TARGET_DIR=$PWD/target` before any cargo command when you use a worktree,
run `cargo test --workspace`, keep the build free of warnings, and give anything that took two attempts to
get right a regression test.

## 1. The pass pipeline

```text
source text
  -> src/syntax/lexer.rs       lines with kind and indentation (tabs and odd indentation are errors)
  -> src/syntax/parser.rs      declarations and statement trees (chumsky grammars in src/syntax/parser/)
  -> AST  (src/syntax/ast.rs)
  -> src/project.rs            files, imports, globs, cycles, agentmark.toml, tools.toml, bindings
  -> src/sema/                 names, types, effects, manifest, hard limits, parallel, unused
  -> Checked                   side tables keyed by (file, span): types, effects, resolutions
  -> src/lower/agent_lower.rs  layer 3 -> layer 2 (still an AST)
  -> src/ir/build.rs           layer 2 + Checked -> IR (src/ir/mod.rs)
  -> src/codegen/rust.rs       IR -> a Cargo project (one file per target)
```

Each arrow is a function with an owned input and output, so a pass can be tested alone, and no pass reaches
back into an earlier one. Two rules keep the pipeline honest:

* **The IR has no agent concepts.** After `agent_lower` there is no agent, rule, trigger, `ask model` or guard
  node; the lowering tests walk the layer-2 AST to assert that. The generator never sees an agent
  (SPEC 0, DESIGN 4).
* **Every IR node carries an effect set**, copied from `Checked` (or merged from children for nodes lowering invented).
  The effect summary that `check` prints is read from the semantic check (`Checked.blocks`, see `src/cli/check.rs`); the IR carries the same sets, so a future optimiser reads them there.

The pieces, in the order a program meets them:

| stage | files | what it hands on |
|---|---|---|
| lexer | `src/syntax/lexer.rs` | `Vec<Line>` with kind (`Heading1`, `Heading2`, `Comment`, `Item`, `Control`, fences, blank), indentation in 2-space units, verbatim fenced code |
| parser | `src/syntax/parser.rs`, `src/syntax/parser/heading.rs`, `src/syntax/parser/stmt.rs`, `src/syntax/parser/expr.rs` | `ast::File`. The first file builds the declaration and statement *tree* from indentation; the three submodules are chumsky parsers over the text of one line (headings, statements, expressions) |
| diagnostics | `src/diag.rs` | `Diagnostic { code, severity, span, message, help, did_you_mean }`, the SPEC 10 renderer, Damerau-Levenshtein suggestions |
| project | `src/project.rs`, `src/manifest.rs` | `Project`: parsed files, resolved imports, `Config` (the project's config file), `Manifest` (the tool manifest), load-time diagnostics |
| semantic check | `src/sema/mod.rs` and its modules | `Result<Checked, Vec<Diagnostic>>`; see `src/sema/README.md` for the full interface |
| lowering | `src/lower/agent_lower.rs` | `Lowered`: the layer-2 AST |
| IR | `src/ir/mod.rs`, `src/ir/build.rs` | `ir::Program`; see `src/ir/README.md` |
| generator | `src/codegen/mod.rs`, `src/codegen/rust.rs` and `src/codegen/rust/` (statements and expressions in `src/codegen/rust/body.rs`, the files of the project in `src/codegen/rust/files.rs`, the entry point in `src/codegen/rust/main_rs.rs`, identifiers in `src/codegen/rust/names.rs`, types in `src/codegen/rust/ty.rs`) | `Vec<GeneratedFile>`: the Cargo project of SPEC 9.1; `generate()` in `src/codegen/rust.rs` |
| runtime | `crates/agentmark-rt/src/` | `Money`, scheduler, `ModelClient`, `MockTools`, `Trace`, `Ctx`; see `crates/agentmark-rt/API.md` |
| tools around it | `src/fmt.rs`, `src/lint.rs`, `src/cli/docgen.rs`, `src/binder/` | formatter, lint rules, documentation page, library binder |
| command line | `src/cli.rs`, `src/cli/check.rs`, `src/cli/build.rs` (build, run, trace), `src/cli/fmtlint.rs`, `src/cli/docgen.rs`, `src/cli/bind.rs`, `src/cli/files.rs` | one function per command; each prints its report and returns the exit code |

`src/lib.rs` exports every pass, so a language server, an editor integration or a test can use the checker
as a library:

```rust
let project = agentmark::project::Project::load("examples/01-invoice-chaser");
let checked = agentmark::sema::check(&project);                    // Result<Checked, Vec<Diagnostic>>
let l2      = agentmark::lower::agent_lower(&project, checked.as_ref().unwrap());
let program = agentmark::ir::build::build(&project, checked.as_ref().unwrap(), &l2);
```

`Project::from_str` and `Project::from_memory` build in-memory projects (tests, editor buffers).

### 1.1 Why the semantic check is side tables, not a new tree

`sema::check` does not produce a typed AST. It leaves the AST alone and returns tables keyed by `NodeKey { file,
span }`: `expr_types`, `expr_effects`, `stmt_effects`, `resolutions` (what a name stands for), `tool_calls` (what
each tool call resolved to), `unwraps` (where a `maybe` is known to be present) and so on. Later passes look up
what they need. This keeps the parser output stable for snapshots and lets the formatter and linter run on
syntax alone, without the checker. The cost: every node a later pass wants to look up needs a *unique span*. When
lowering invents nodes it gives them `col == 0` spans (`lower::is_synthetic`), and the IR builder merges the
effects of their children instead of looking them up.

## 2. Where each section of the specification lives

| SPEC | topic | implemented in | tested in |
|---|---|---|---|
| 1 | file structure, indentation, comments | `src/syntax/lexer.rs`, `src/syntax/parser.rs` | `tests/syntax_errors.rs`, `tests/parse_structure.rs`, `tests/parse_snapshots.rs` |
| 2.1 | records, field-type inference | `src/sema/decls.rs`, `src/sema/types.rs` | `tests/sema_types.rs` |
| 2.2 | rules | `src/sema/body.rs`, `src/lower/agent_lower.rs` (rule becomes a pure function) | `tests/sema_effects.rs`, `tests/ir_lower.rs` |
| 2.3, 3 | agent properties, triggers | `src/syntax/parser/heading.rs`, `src/sema/decls.rs`, `src/lower/agent_lower.rs`, scheduler in `crates/agentmark-rt/src/schedule.rs` | `tests/parse_structure.rs`, `tests/ir_lower.rs`, `crates/agentmark-rt/tests/schedule.rs` |
| 4 | statements | `src/syntax/parser/stmt.rs`, `src/sema/stmt.rs`, `src/ir/build.rs` | `tests/parse_structure.rs`, `tests/sema_scope.rs`, `tests/ir_lower.rs` |
| 4.4 | guards, `no amounts changed` | `src/sema/expr.rs`, `src/lower/agent_lower.rs` (agents), `src/ir/build.rs` (everywhere else, Q90), `crates/agentmark-rt/src/guard.rs` | `tests/ir_lower.rs`, `crates/agentmark-rt/tests/` |
| 5, 6 | expressions, units, types | `src/syntax/parser/expr.rs`, `src/sema/expr.rs`, `src/sema/types.rs`, `crates/agentmark-rt/src/money.rs` | `tests/sema_types.rs`, `crates/agentmark-rt/tests/money.rs` |
| 7 | tool manifest | `src/manifest.rs`, `src/sema/tools.rs` | `tests/sema_manifest.rs`, `tests/errors.rs` |
| 8 | effects, the summary | `src/sema/effects.rs`, `src/ir/mod.rs` (`effect_summary`), `src/cli/check.rs` | `tests/sema_effects.rs`, `tests/cli.rs` |
| 9 | commands | `src/cli.rs`, `src/cli/*.rs`, generator in `src/codegen/rust.rs` and `src/codegen/rust/` | `tests/cli.rs`, `tests/codegen.rs`, `tests/examples_e2e.rs` |
| 9.2 | runtime crate | `crates/agentmark-rt/src/` | `crates/agentmark-rt/tests/` |
| 10 | error messages | `src/diag.rs`, messages in the pass that finds the problem | `tests/errors.rs`, `tests/errors/*.agent.md`, `tests/syntax_errors.rs` |
| 11 | fixtures and traces | `fixtures/`, `crates/agentmark-rt/src/trace.rs`, `crates/agentmark-rt/src/tools.rs` | `crates/agentmark-rt/tests/fixtures_01.rs`, `crates/agentmark-rt/tests/trace_kinds.rs` |
| 13.1 | functions | `src/sema/decls.rs` (signatures), `src/sema/body.rs`, `src/ir/build.rs` | `tests/sema_types.rs`, `tests/sema_effects.rs` |
| 13.2, 18.2 | modules, imports | `src/project.rs`, `src/sema/scope.rs`, `src/sema/unused.rs` | `tests/sema_scope.rs` |
| 13.3 | services, commands | `src/syntax/parser.rs`, `src/ir/build.rs`, generator in `src/codegen/rust.rs` | `tests/ir_lower.rs` |
| 14 | `try`, `maybe` | `src/sema/stmt.rs`, `src/ir/build.rs` (`Try`, `IfSome`) | `tests/sema_types.rs`, `tests/ir_lower.rs` |
| 15 | library bindings | `src/binder/` (section 5 below), `src/cli/bind.rs` | `tests/binder.rs`, `tests/binder/` |
| 16 | `parallel` | `src/sema/parallel.rs` | `tests/sema_effects.rs` |
| 17 | `## Rust` blocks | `src/syntax/parser.rs` (`rust_block`), `src/sema/decls.rs`, `src/ir/build.rs` (`RustFn`) | `tests/sema_effects.rs`, `tests/parse_structure.rs` |
| 18.1 | project layout | `src/project.rs` | `tests/sema_scope.rs`, `tests/sema_examples.rs` |
| 18.3 | hard limits | `src/sema/limits.rs` | `tests/sema_limits.rs` |
| 18.4 | lint | `src/lint.rs` | `tests/lint.rs` |
| 18.5 | constants | `src/sema/decls.rs`, inlined by `src/ir/build.rs` | `tests/sema_review.rs` |
| 18.6 | formatter | `src/fmt.rs` | `tests/fmt.rs`, `tests/fmt/messy.agent.md` |
| 18.7 | documentation page | `src/cli/docgen.rs` | `tests/cli.rs` |
| 19 | model providers | `crates/agentmark-rt/src/model.rs` | `crates/agentmark-rt/tests/anthropic.rs` |
| 20.3 | language server | `crates/agentmark-lsp/src/` (section 9 below) | `crates/agentmark-lsp/tests/` |
| 22.3 | built-in library, the `file` tool, `read input` | the table `src/stdlib.rs`, checker `src/sema/stdlib.rs`, Rust spelling `src/codegen/rust/stdlib.rs`, runtime `crates/agentmark-rt/src/stdlib/` | `tests/sema_stdlib.rs`, `tests/stdlib_syntax.rs`, `tests/stdlib_e2e.rs`, `crates/agentmark-rt/src/stdlib/` (unit tests) |

The documentation is tested too: `tests/docs.rs` runs every sample in `docs/*.md`. Its header comment states which
fenced blocks are complete programs and which are fragments, and the wrapper a fragment is parsed in.

## 3. How to add a statement

Take an imaginary `wait <duration>` as the example, a statement that pauses a block. Work through the passes in
order. The compiler finds most of what you forget, because `StmtKind` is matched exhaustively in every pass.

1. **AST.** Add a variant to `StmtKind` in `src/syntax/ast.rs` (`Wait(Expr)`), with the span it needs.
2. **Parser.** Add the line grammar in `src/syntax/parser/stmt.rs`: a chumsky parser inside `head()`, producing a
   `Head::Simple(Stmt)`. Add the leading word to the `STATEMENT_WORDS` list in that file, so it can never be taken for a
   tool name. If the statement owns an indented body, you also teach the tree builder in `src/syntax/parser.rs` about
   it, as `for each` and `if` are. Mistakes should say something: use the keyword helpers so a typo gets a
   suggestion (Q27).
3. **Formatter.** `src/fmt.rs` prints every statement from its AST; add the canonical text (`wait 5 minutes`). The
   idempotence and AST-equivalence tests in `tests/fmt.rs` need a sample that uses it.
4. **Lint.** `src/lint.rs` walks statements to find expressions (for `magic-value`, `vague-name`, ...); add the new
   variant to that match.
5. **Semantic check.** `src/sema/stmt.rs`: type the pieces (`wait` wants a `duration`), record the statement's effect
   (the sources and classes are in `src/sema/effects.rs`), and decide what the statement may be next to. If it can be
   written to a variable, tell `src/sema/parallel.rs`. `src/sema/limits.rs` already counts every statement.
6. **Lowering.** `src/lower/agent_lower.rs` rebuilds statements; add the variant so lowering copies it (it only
   needs real work if the statement is layer 3 sugar). Rewrite any expression it holds with `self.expr`.
7. **IR.** Add the layer-2 form to `src/ir/mod.rs`: an `EffectCall` variant if it is an effect (the usual case),
   or a `StmtKind` variant for control flow. Build it in `src/ir/build.rs` and make sure its effect is merged
   (see the `Print` handling for the pattern, including `union_effects`).
8. **Generator.** Teach `src/codegen/rust.rs` to emit it, with the `// <file>.agent.md:<line>` comment before it.
   If it is observable (the tests compare behaviour), add a trace kind to `crates/agentmark-rt/src/trace.rs` and to the
   table in `docs/reference.md` section 11.
9. **Tests.** A parse snapshot sample if an example uses it (`tests/parse_snapshots.rs`), a case in
   `tests/parse_structure.rs`, one error file in `tests/errors/` for the commonest mistake (the file's header
   comment holds the exact expected message), a semantic test in the matching `tests/sema_*.rs`, a lowering or IR
   test in `tests/ir_lower.rs`, a formatter sample, and a fixture whose expected trace shows it working.
10. **Docs.** Add the form to `docs/reference.md` (the section it belongs to, and Appendix A) with a complete
    example, and to the list of forms in `reference_shows_every_statement_form` in `tests/docs.rs`. Record any
    choice the specification did not settle in `OPEN_QUESTIONS.md`.

A statement that is pure sugar for existing layer-2 statements should not enter layer 2 at all: lower it in
`src/lower/agent_lower.rs` and let the generator stay unchanged. That is the point of the three layers.

## 4. How to add a target

The IR is target-independent; a target is a consumer of `ir::Program` (and `Checked` for names). Adding one means
adding files, not changing passes:

1. **Generator.** One new module next to `src/codegen/rust.rs` (declared in `src/codegen/mod.rs`), exporting a function from `ir::Program` to a set of
   output files (`Vec<GeneratedFile>`), modelled on `generate()` in `src/codegen/rust.rs`. It reads only the IR. The conventions to keep are the ones
   that make output reviewable: one function per block, the block heading as a doc comment, a source-map comment
   before every statement, and the effect summary at the top of each file.
2. **Runtime.** Generated programs need what `crates/agentmark-rt` gives Rust ones: money arithmetic with rounding half
   away from zero, a scheduler, a model client, mock tools reading `fixtures/`, and a trace writer that produces
   exactly the lines of `docs/reference.md` section 11. For a new language, that is a new runtime library.
3. **Wire-up.** Register the target name for `agentmark build --target <name>` in `src/cli.rs` and `src/cli/build.rs`; `targets = ["rust"]` in
   the `targets` key of the project's config file (parsed into `Config.targets` in `src/project.rs`) lists the targets a project builds.
4. **Binder.** Library access is target-specific metadata. `src/binder/core/` is target-agnostic and must stay that
   way (nothing in it may mention Rust); a new target adds `src/binder/<target>/` that produces
   `binder::core::CrateItems` from the target's metadata (`.d.ts` files for TypeScript) and implements the
   `TypeChecker` trait of `src/binder/core/typecheck.rs`. `src/binder/rust/` is the model to copy.
5. **Tests.** Build every example for the new target and compare traces with `fixtures/*/expected.trace.jsonl`:
   the traces are the contract (tests compare behaviour, never generated source text). Add the target to the
   `docs/reference.md` sections 9 and 15.

## 5. How the binder tiers work

`src/binder/` turns a library's public API into a `# Binding` file (SPEC 15). Its own overview is
`src/binder/README.md`; this is the map for changing it.

```text
src/binder/mod.rs             bind(), doc_cheatsheet(), BindOptions, BindOutcome
src/binder/core/model.rs      the item model: Item { name, path, summary, shape }, Shape, Sig, Ty
src/binder/core/pipeline.rs   run_tiers(): every Item lands in exactly one tier
src/binder/core/writer.rs     render_binding(), the header, freshness(), not_bound_entry()
src/binder/core/effects.rs    propose_effect(): the proposed-effect guess
src/binder/core/typecheck.rs  the TypeChecker trait
src/binder/rust/rustdoc.rs    rustdoc JSON -> CrateItems (classifies each item's Shape)
src/binder/rust/types.rs      Rust type <-> AgentMark type mapping and the direct-tier bodies
src/binder/rust/checker.rs    CargoChecker: wraps a body in a function and runs cargo check
src/binder/rust/fetch.rs      builds rustdoc JSON (cargo +nightly rustdoc) or reads the env override
```

**The division of labour.** The target decides an item's `Shape` *before any model is involved*: `Shape::Direct`
carries a ready body (the target wrote it by rules), `Shape::Assisted` carries the signature and why it is not
direct, `Shape::Skipped` carries the reason. For Rust that classification is in `src/binder/rust/rustdoc.rs`
using the type mapping of `src/binder/rust/types.rs`. `run_tiers` in `src/binder/core/pipeline.rs` then does the
target-independent part: take direct items as they are; for assisted ones ask the `ModelClient` (the signature, the
item's docs, the AgentMark style guide), have the `TypeChecker` accept or reject the returned body, and on failure move
the item to the skipped list with the reason; list every skip. It also keeps `placements`, one entry per input item,
which the tests use to prove nothing is dropped.

**Where the tier boundary is** is a decision list (Q75 to Q78): direct requires not `async`, not `Result`, no
generics, receivers `self` or `&self` only, integers up to `i64`, no floats; closures are skipped because AgentMark
has no function type; generics are instantiated from their bounds with a small fixed table. To move an item between
tiers, change the classification in `src/binder/rust/rustdoc.rs` and record the decision.

**The on-demand tier** is the fourth step and lives next to the others: `src/binder/core/ondemand.rs` (prompt, two attempts, the
signature the model proposes when the item has none, the type check), `add_on_demand` in `src/binder/core/writer.rs` (moves the items
out of `## Not bound`, resets `- effect:` to `unverified`), `produce_on_demand` and `commit_on_demand` in `src/binder/mod.rs` (produce reads the binding and the rustdoc, computes the new text; commit replaces the files, and the CLI commits only when every crate succeeded) and `src/cli/ondemand.rs` (finds the statements, prints the E220 error and writes the compiler text to `target/agentmark/ondemand-<crate>-<item>.log`). `check` and `build` call it after
the automatic first binding and before the semantic check. Keep the invariant that code a model wrote is type-checked before it
is accepted and is never silently wrong; a new tier adds a step in the same shape.

**Trust is a file property.** `render_binding` always writes `- effect: unverified` plus the proposal;
`freshness` decides what `bind` does with an existing file: a header whose version matches is a cache hit (also
when approved); a different version regenerates (as `unverified`, old file kept as `.prev`); a file with no
generated header is hand-written and is left alone unless `force`. `check` reads the effect through the normal
parser, so the gate is in the CLI (`src/cli/check.rs`), not the binder.

**Tests** use trimmed rustdoc JSON (`tests/binder/sha2.json`, `tests/binder/chrono.json`, `tests/binder/reqwest.json`, `tests/binder/digest.json`),
regenerated by `tests/binder/regen.sh` with a nightly toolchain, and a canned `MockModel` file
(`tests/binder/model.json`), so `cargo test --test binder` works offline on stable. Three tests run the real
`cargo check --offline`. Moving to a newer rustdoc format means re-running `tests/binder/regen.sh` and checking
`SUPPORTED_FORMAT` in `src/binder/rust/rustdoc.rs` (Q71).

## 6. Real tools: the project's connect file

Generated programs reach real systems through one seam. `src/codegen/rust/files.rs` writes the generated tools file: one
`#[async_trait]` trait per tool with verbs, an impl of each for `agentmark_rt::MockTools`, and `Connected`, which implements
`agentmark_rt::Tools` by dispatching `(tool, verb)` to its fields. Generated blocks only call `ctx.tools.call(...)`, so mock and
real runs share all code. The user's side is a file named rust/connect.rs in the *project*, beside the project's config file (next to the
source file for a single file): `src/cli/build.rs` creates it from the stub template when it is missing, never overwrites it,
copies it into the generated project on every build, and maps a rustc error in it back to the project file (Q135, amended in
step 10). `--mock` runs use the file when it exists and never create it. The committed stubs are
`examples/01-invoice-chaser/rust/connect.rs` and the same file in the other example projects. A change to what a tool trait
looks like must keep older connect files compiling or say how to migrate them.

## 7. How the tests are organised

| tests | what they pin |
|---|---|
| `tests/parse_snapshots.rs` | the AST of every file under `examples/` as `insta` YAML in `tests/snapshots/`; a parser change shows as a diff |
| `tests/parse_structure.rs`, `tests/syntax_errors.rs` | what specific constructs mean; the exact wording of syntax errors |
| `tests/errors.rs` + `tests/errors/*.agent.md` | one deliberate mistake per file; the header comment is the exact expected message, so the file documents the contract |
| `tests/sema_*.rs` | the semantic check by topic: scope, types, effects, limits, manifest, examples, review regressions |
| `tests/ir_lower.rs` | lowering and the IR: no agent concept survives, effects and names are right |
| `tests/fmt.rs`, `tests/lint.rs` | idempotence and AST equivalence of `fmt`; one positive and one negative case per lint rule |
| `tests/cli.rs` | the `agentmark` binary as a process: exit codes, streams, output |
| `tests/binder.rs` | the binder with recorded metadata and a canned model |
| `tests/codegen.rs`, `tests/examples_e2e.rs`, `tests/support/mod.rs` | the generated programs are built with cargo and judged by their traces (every example against `fixtures/*/expected.trace.jsonl`) |
| `tests/docs.rs` | the documentation: every sample, the guide's vocabulary, the paths on this page |
| `crates/agentmark-rt/tests/` | runtime: money rounding, scheduler, trace lines, fixtures |
| `crates/agentmark-lsp/tests/` | the language server driven over the protocol in-process: one test per capability, every code action, model actions off and on, the binary over stdio |

Conventions: tests compare behaviour (traces), not generated source text; a bug that took two tries gets a
regression test; test output stays free of warnings; shared helpers for the semantic tests are in
`tests/common/mod.rs`.

## 8. Open questions and decisions

`OPEN_QUESTIONS.md` is the memory of the project. A decision has a question, the choice, and why. The numbers are
grouped by the step that made them: Q1 to Q11 setup, Q12 to Q15 runtime, Q16 to Q29 syntax, Q30 to Q41 semantic
check, Q50 to Q56 formatter and lint, Q70 to Q82 binder, Q90 to Q95 lowering and IR, Q110 to Q117 command line, Q130 to Q140 code generation, Q150
to Q158 examples and fixtures, Q170 to Q175 documentation, Q190 and up review fixes and library trust. `docs/reference.md` cites the entries that change what a
user sees. When you face a new ambiguity: write the entry, apply the choice, continue.

## 9. The language server

`crates/agentmark-lsp/` is the workspace member that builds `agentmark-lsp`, a Language Server Protocol server on
stdio (`lsp-server` for the transport, `lsp-types` for the messages; synchronous, one thread). It uses the compiler
only as a library: `Project::load_with` through an overlay of the open buffers, `sema::check`, `lint`, `fmt`.
Nothing in `src/` knows about it except `project::Config.lsp_model_actions` (Q250).

| file | what it does |
|---|---|
| `crates/agentmark-lsp/src/server.rs` | the message loop, one handler per method, the per-project analysis cache, diagnostics publishing |
| `crates/agentmark-lsp/src/workspace.rs` | open buffers over the disk (`OverlayFs`), the project of a document, `Analysis` (load, check, lint, the last good check) |
| `crates/agentmark-lsp/src/diagnostics.rs` | `Diagnostic` to LSP: the SPEC 10 message, range, code, source |
| `crates/agentmark-lsp/src/complete.rs` | completion: context from the text left of the cursor, names from the ASTs, types from the checker |
| `crates/agentmark-lsp/src/nav.rs` | go-to-definition (through `Checked.resolutions` and `declarations`, then by name) and hover (effect summary of a block, types) |
| `crates/agentmark-lsp/src/actions.rs` | quick fixes of the lint rules and of "Did you mean", and the model actions |
| `crates/agentmark-lsp/src/index.rs`, `crates/agentmark-lsp/src/text.rs`, `crates/agentmark-lsp/src/uri.rs` | declarations visible from a file and the word under the cursor; character and UTF-16 positions; `file://` URIs |

Three things to know before changing it. The checker returns nothing when there is any error, which is the normal
state while typing, so every feature has a plan for that (Q252); keep it when you add a feature. Positions are
characters in the compiler and UTF-16 units in the protocol: always go through `crates/agentmark-lsp/src/text.rs`. A new lint rule that has a
mechanical fix gets an arm in `lint_fix` in `crates/agentmark-lsp/src/actions.rs` and a test in `crates/agentmark-lsp/tests/actions.rs` that
applies the edit and checks that the finding is gone and the project still checks (Q255). The decisions behind all of
it are Q250 to Q258.
