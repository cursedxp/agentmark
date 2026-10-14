# Changelog

All notable changes to AgentMark are listed here. The format follows Keep a Changelog.

Only 0.3.0 is a published release. 0.1.0 and 0.2.0 are the earlier milestones of the same history.

## [0.3.0] - 2026-10-14

AgentMark is now a general-purpose programming language written as Markdown. Agents are one feature of it, not the point of it.

### Added

- Names for your own data. `# Enum` declares variants that can carry fields. `# Type` declares an alias. `match` branches over an enum, a `maybe` or a `result`, and a missing variant is a compile error.
- Collection words: `list of`, `map of`, `where`, `map ... to`, `sort ... by`, `group ... by`, `sum of`, `count of`, `min of`, `max of`, `first of`, `last of`, `take`, `skip`, `unique`, `join ... with`, `keys of` and `values of`.
- Record literals, `some` and `none`, and `try` without `do`.
- A built-in library with the modules `text`, `regex`, `number`, `money`, `date`, `path`, `json` and `csv`. Every function has a fixed effect class that `check` shows.
- A built-in `file` tool with `read`, `write`, `append`, `list`, `rename`, `exists` and `delete`. It needs no `tools.toml` entry, and every call is traced.
- `read input` reads standard input.
- Library handles. A Rust type with no AgentMark equivalent becomes `handle of <crate>.<Type>`, and the binder binds its constructors and methods. A handle owns what the Rust value borrows. Handles consumed by a method cannot be used again.
- A `[crates]` table in `agentmark.toml` sets crate features for the binder and for the generated project.
- Better `sha2` binding: 42 of the 81 items that v0.2 listed under `## Not bound` are now bound.
- A `sqlx` binding that example 08 runs on, through the `sqlite` feature.
- Wrappable binding items. An item that is not bound directly can be bound in a wrapped form.
- `# Tool`: a tool written in AgentMark itself, with its declared effect checked against its body. `tools.toml` and `rust/connect.rs` are optional.
- A default command. A project with one `# Command` runs it as `my-tool <args>`. Command arguments are positional or `--name value`.
- Examples 06 to 10 (sales report, file renamer, notes API, Markdown to HTML, log analyzer). None is an agent and none contains `## Rust`.
- `install.sh`, which installs `agentmark` and `agentmark-lsp` and can uninstall them. A `LICENSE` file.
- Editor support for the new syntax: TextMate scopes, tree-sitter handle blocks, LSP completion.
- `docs/connect.md`, which covers `rust/connect.rs`.
- A single-page documentation site generator in `tools/docs-site`.

### Changed

- The README, the guide, the editor guide, the internals page, the check-json page and the libraries page are rewritten for fast reading. Short paragraphs, task-shaped headings, a summary block on every page.
- Generated names are safe from clashes with argument names.
- Every toolchain crate and editor package is at version 0.3.0. Your own project versions in `agentmark.toml` are not affected.

### Known gaps

- The `azure` and `bedrock` providers are not implemented. `check` rejects them with E221.
- `memory: file` is accepted but not implemented. `build` prints a note.
- `parallel:` blocks run one statement after another.
- There are no prebuilt binaries. You install from source with `install.sh`, and generated projects depend on `agentmark-rt` by absolute path.
- The Neovim, Helix and Zed snippets in `docs/editor.md` were not run.
- `shellcheck` was not run on `install.sh`.
- Tree-sitter does not model indentation. Statements in a block are siblings, which is enough for highlighting.

## [0.2.0]

Tooling around the language.

### Added

- An on-demand binding tier. A statement against a `## Not bound` item is written by the model, checked with `cargo check` and saved as `unverified`. A model that fails twice stops the build with E220.
- Model providers: `claude`, `claude:<m>`, `openai:<m>`, `local:<m>` and `mock`. A `[models]` map is chosen by `AGENTMARK_ENV`.
- `agentmark-lsp`: diagnostics, completion, go-to-definition across imports, hover with the effect summary, format on save and code actions for lint rules.
- A VS Code extension with a TextMate grammar, an LSP client, an `agentmark.toml` schema and a panel that shows what an agent can touch.
- A tree-sitter grammar with highlights, Rust injection and 48 corpus cases.
- `templates/CLAUDE.md` and `templates/.cursorrules`, and `docs/editor.md`.

## [0.1.0]

The first compiler.

### Added

- A front end: lexer, parser and the semantic check for types, units, effects, tool manifests and `parallel` conflicts.
- An agent-free IR with an effect set on every node.
- The runtime crate `agentmark-rt`: exact `Money`, a scheduler, a model client, mock tools and a trace recorder.
- Rust code generation and the commands `check`, `build`, `run`, `trace`, `bind`, `fmt`, `lint` and `doc`.
- A binder with direct, assisted and skipped tiers, staleness checks and the `unverified` approval line. `sha2` is bound through it for example 05.
- Five examples that build and run against recorded fixtures.
- A README and a `docs/` folder whose samples are run by the test suite.
