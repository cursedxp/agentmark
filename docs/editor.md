# AgentMark in your editor

AgentMark files are Markdown, so any editor opens them. This page is about making the editor understand them: colours, errors as you type, completion, a formatter, and in VS Code a panel that answers the question a careful manager asks first, "what can this agent touch?".

There are three pieces, and they work alone:

* **`agentmark-lsp`**, a language server (`crates/agentmark-lsp`). It runs the same checker as `agentmark check`, so the messages are the same. Any editor with Language Server Protocol support can use it.
* **The VS Code extension** (`editors/vscode`). Highlighting, the language-server client, a schema for `agentmark.toml` and the touch panel.
* **A tree-sitter grammar** (`editors/tree-sitter-agentmark`) for editors that highlight with tree-sitter: Zed, Neovim and Helix.

## 1. Install

### The language server

You need a Rust toolchain. From a checkout of this repository:

```sh
cargo install --path crates/agentmark-lsp
```

`agentmark-lsp` ends up in `~/.cargo/bin`, next to `agentmark` (installed with `cargo install --path .`, see the README). It speaks the protocol on standard input and output and takes no arguments.

### The VS Code extension

You need Node.js. Build the package once, from the extension folder:

```sh
cd editors/vscode
npm install
npx @vscode/vsce package
```

That writes `agentmark-0.1.0.vsix` in the same folder. Install it from the command line or from the Extensions view (the `...` menu, "Install from VSIX..."):

```sh
code --install-extension agentmark-0.1.0.vsix
```

Open a folder that holds an `agentmark.toml`, or any `.agent.md` file, and the extension starts. If `agentmark-lsp` cannot be found the extension says so; highlighting and the panel keep working without it.

## 2. Settings

Both settings are machine-scoped: a workspace cannot set them, and they are ignored in an untrusted workspace (a downloaded project cannot make your editor run a program it brought along).

| setting | default | meaning |
|---|---|---|
| `agentmark.server.path` | `agentmark-lsp` | the language server binary; give the full path if it is not on your `PATH` |
| `agentmark.cli.path` | `agentmark` | the CLI the touch panel runs (`check --json`) |

```json
{
  "agentmark.server.path": "/Users/you/.cargo/bin/agentmark-lsp",
  "agentmark.cli.path": "/Users/you/.cargo/bin/agentmark"
}
```

**Format on save** is on by default for AgentMark files (`"[agentmark]": { "editor.formatOnSave": true }`). The server answers the editor's format request with the canonical style, the same text `agentmark fmt` writes, so a saved file never fails `agentmark fmt --check`. To turn it off for yourself, set `"[agentmark]": { "editor.formatOnSave": false }` in your user settings.

Two commands are in the command palette:

* **AgentMark: Show what this agent can touch** opens the panel (also a button in the title bar of every `.agent.md` file).
* **AgentMark: Restart language server** after you reinstall `agentmark-lsp`.

## 3. What can this agent touch

The panel runs `agentmark check --json` on the project that holds the open file (the nearest folder with an `agentmark.toml`) and shows the answer in plain words. While the panel is open it refreshes whenever you save a `.agent.md` file, `agentmark.toml` or `tools.toml`. The JSON it reads is described in [docs/check-json.md](check-json.md).

Per agent, service, command, rule and function it shows:

* the **effect class**: `pure` (computes only), `local` (this machine: logs, files), `network` (outside systems), `model` (asks an AI model) or `unverified` (a library nobody approved yet);
* a plain sentence for each kind of access, for example "can reach the network: crm, email" or "asks the AI model";
* the tools it can reach, and the same for each block (a trigger such as `Every day at 09:00`, or a `## Do` block) with a link to its line;
* **Needs your approval**: libraries whose binding still says `- effect: unverified`;
* **Trusted Rust blocks**: every `## Rust` block, because the compiler cannot verify those;
* **Problems**: the errors and warnings of `check`, each linked to its line.

For `examples/01-invoice-chaser`, `agentmark check` prints this summary (the panel is built from the same data):

```console cwd=repo
$ agentmark check examples/01-invoice-chaser
Effects (what each block can touch)
  invoice-chaser / Every day at 09:00: model, network(crm, email), local(log)
  invoice-chaser / Do remind: model, network(email, crm), local(log)
  rule overdue: pure

ok: 0 errors, 0 warnings.
```

and the panel shows, as text (the real panel prints the absolute path of the project where the sample says `examples/01-invoice-chaser`):

```text
What can this agent touch

Check passed for examples/01-invoice-chaser · 0 error(s), 0 warning(s)

invoice-chaser  agent  [model]
  - can reach the network: crm, email
  - asks the AI model
  - uses this machine: log
  model, network(crm, email), local(log)
  Can touch: model  crm  email  log
  Block                      Effect   Reaches
  Every day at 09:00         model    model  crm  email  log
  Do remind                  model    model  email  crm  log

overdue  rule  [pure]
  - pure: touches nothing outside itself
  pure
  Can touch: nothing
  Block                      Effect   Reaches
  overdue                    pure     nothing
```

This is the page a reviewer reads before approving an agent: it did not ask for anything it does not need, and nothing here was written by hand, it is computed from the code.

## 4. agentmark.toml

The extension validates `agentmark.toml` through the TOML validation of the Even Better TOML extension, using `editors/vscode/schemas/agentmark.schema.json`. Nothing to configure. An unknown key is flagged, and a lint rule name gets completion. The whole file, with every section the schema knows:

```toml name=agentmark.toml
name = "invoice-chaser"
version = "0.1.0"
targets = ["rust"]

[lint]
deny = ["prompt-in-loop", "missing-doc"]

[rust.dependencies]
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }

[crates]
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio"] }

[lsp]
model_actions = false

[models]
production = "claude"
test = "mock"
dev = "local:kat-coder"

[models.local]
base_url = "http://localhost:11434/v1"
```

| key | meaning |
|---|---|
| `name`, `version` | the project and the binary it builds |
| `targets` | `["rust"]`, the only target in v0.1 |
| `[lint] deny` | lint rules that are errors instead of warnings: `long-file`, `magic-value`, `unnamed-guard`, `duplicate-rule`, `prompt-in-loop`, `missing-doc`, `vague-name` (see section 18.4 of [the reference](reference.md)) |
| `[rust.dependencies]` | copied into the `[dependencies]` of the generated `Cargo.toml` |
| `[crates]` | extra Rust crates a `## Rust` block or binding may use (section 22.4 of [the reference](reference.md)): a version string, or a table with `version` and optional `features` |
| `[lsp] model_actions` | `true` turns on the optional model-backed code actions below; off by default |
| `[models]` | environment name to model driver, picked by `AGENTMARK_ENV` (section 19 of [the reference](reference.md)); `[models.local] base_url` is the server for `local:<model>` drivers |

## 5. Language server features

What `agentmark-lsp` offers, in any editor:

| feature | what you get |
|---|---|
| diagnostics | the errors and warnings of `agentmark check` and `agentmark lint`, with the same text, as you type |
| completion | typed after `.` or a space: heading kinds (`Agent:`, `Record:` ...), triggers, statements, tools and their verbs, records, rules, constants, bound libraries, and the fields of a variable after `x.` |
| go to definition | from a name to its declaration, across `# Import:` lines |
| hover | on a heading: the effect summary of the block or of the whole agent; on a record field: its type, and whether the type was inferred from the name |
| formatting | `textDocument/formatting` returns the canonical style (same as `agentmark fmt`) |
| code actions | quick fixes for the mechanical lint warnings (add a description above an agent, service or command, which is what `missing-doc` asks for; add a comment above a guard; extract a repeated literal into a `# Constant`) and for "did you mean" suggestions |
| model actions | only with `[lsp] model_actions = true` and a model available: `AGENTMARK_MODEL` if set, else the `[models]` entry that `AGENTMARK_ENV` names, else Claude with `ANTHROPIC_API_KEY` (section 19 of [the reference](reference.md)). The actions are "Explain this block" and "Suggest a guard for this answer" on an `ask model` line |

The server reads the project from disk and the open buffers from the editor, so unsaved edits are checked too. It never writes a file by itself: every fix is an edit the editor applies and you can undo.

### Other editors

Point any LSP client at the `agentmark-lsp` binary for files ending in `.agent.md`, with the project root at the folder that holds `agentmark.toml`. Examples (adjust to the version of your editor):

Neovim 0.11 or newer:

```lua
vim.filetype.add({ pattern = { [".*%.agent%.md"] = "agentmark" } })
vim.lsp.config("agentmark", {
  cmd = { "agentmark-lsp" },
  filetypes = { "agentmark" },
  root_markers = { "agentmark.toml" },
})
vim.lsp.enable("agentmark")
```

Helix, in `languages.toml`:

```toml
[language-server.agentmark-lsp]
command = "agentmark-lsp"

[[language]]
name = "agentmark"
scope = "text.agentmark"
file-types = [{ glob = "*.agent.md" }]
roots = ["agentmark.toml"]
language-servers = ["agentmark-lsp"]
```

Zed starts a language server from an extension, not from a settings file; see the Zed paragraph below.

## 6. Tree-sitter: Zed, Neovim, Helix

`editors/tree-sitter-agentmark` is a tree-sitter grammar for `.agent.md` files. The generated parser is committed (`src/parser.c`), so you do not need the tree-sitter CLI to use it. It ships two query files in `queries/`:

* `highlights.scm`, the highlighting;
* `injections.scm`, which highlights a fenced `rust` block (a `## Rust` body) as Rust.

The grammar name is `agentmark`, its scope is `text.agentmark`, its file type is `agent.md`. Use an absolute path to your checkout where `/path/to/agentmark` appears below.

**Neovim** (with `nvim-treesitter`, `main` branch): register the parser, install it, and copy the queries where Neovim looks for them.

```lua
vim.api.nvim_create_autocmd("User", {
  pattern = "TSUpdate",
  callback = function()
    require("nvim-treesitter.parsers").agentmark = {
      install_info = { path = "/path/to/agentmark/editors/tree-sitter-agentmark" },
    }
  end,
})
vim.treesitter.language.register("agentmark", "agentmark")
vim.api.nvim_create_autocmd("FileType", { pattern = "agentmark", callback = function() vim.treesitter.start() end })
```

```sh
mkdir -p ~/.config/nvim/queries/agentmark
cp editors/tree-sitter-agentmark/queries/*.scm ~/.config/nvim/queries/agentmark/
```

Then run `:TSInstall agentmark`.

**Helix**: declare the grammar next to the language entry of section 5, build it, and copy the queries into Helix's runtime folder.

```toml
[[grammar]]
name = "agentmark"
source = { path = "/path/to/agentmark/editors/tree-sitter-agentmark" }
```

```sh
hx --grammar build
mkdir -p ~/.config/helix/runtime/queries/agentmark
cp editors/tree-sitter-agentmark/queries/*.scm ~/.config/helix/runtime/queries/agentmark/
```

**Zed**: Zed loads grammars through extensions. Make a folder with an `extension.toml` that names the grammar and a language folder that holds the queries, then install it with the command "zed: install dev extension".

```toml
id = "agentmark"
name = "AgentMark"
version = "0.1.0"
schema_version = 1

[grammars.agentmark]
repository = "file:///path/to/agentmark"
rev = "HEAD"
path = "editors/tree-sitter-agentmark"
```

The language folder is `languages/agentmark/` with a `config.toml` (`name = "AgentMark"`, `grammar = "agentmark"`, `path_suffixes = ["agent.md"]`) and a copy of the two query files.

To change the grammar itself, edit `editors/tree-sitter-agentmark/grammar.js` and run `npm install`, `npm run generate` and `npm test` in that folder. The test also parses every file under `examples/`.
