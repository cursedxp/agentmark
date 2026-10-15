# AgentMark in your editor

**In short**
- This page sets up your editor so it understands AgentMark: colours, errors as you type, completion and a formatter.
- It is for anyone who edits `.agent.md` files. Every editor works; pick yours from the table.
- After reading it your editor shows the same errors as `agentmark check`. In VS Code you also get a panel that answers "what does this program touch?".

**Jump to:** [Pick your editor](#pick-your-editor) · [Install the language server](#install-the-language-server) · [Settings](#vs-code-settings) · [The touch panel](#what-does-this-program-touch) · [agentmark.toml](#agentmarktoml) · [What the server does](#what-the-language-server-does)

AgentMark files are Markdown, so any editor opens them. AgentMark is a Markdown programming language. The editor support covers all of it:

* records, enums and functions;
* commands, services and tools;
* the built-in library and library handles;
* agents, which are one more kind of declaration.

## Pick your editor

| your editor | what you get | go to |
|---|---|---|
| VS Code | highlighting, errors, completion, formatter, touch panel | [VS Code](#vs-code) |
| Neovim | errors, completion, formatter, tree-sitter highlighting | [Neovim](#neovim) |
| Helix | errors, completion, formatter, tree-sitter highlighting | [Helix](#helix) |
| Zed | tree-sitter highlighting (through an extension) | [Zed](#zed) |
| other LSP editor | errors, completion, formatter | [Other editors](#other-editors) |

There are three pieces, and they work alone:

* **`agentmark-lsp`** is a language server. It lives in `crates/agentmark-lsp`. It runs the same checker as `agentmark check`, so the messages are the same. Any editor with Language Server Protocol support can use it.
* **The VS Code extension** lives in `editors/vscode`. It gives highlighting, the language-server client, a schema for `agentmark.toml` and the touch panel.
* **A tree-sitter grammar** lives in `editors/tree-sitter-agentmark`. It highlights files in editors that use tree-sitter: Zed, Neovim and Helix.

## Install the language server

**Already ran `install.sh`?** Then `agentmark-lsp` is installed and you can skip this section. See the README for the installer.

Otherwise you need a Rust toolchain and the AgentMark source. The source is `~/.agentmark/src` after `install.sh`, or a clone of https://github.com/cursedxp/agentmark. From that folder:

1. Run the install command.

   ```sh
   cargo install --locked --path crates/agentmark-lsp
   ```

2. Expect `agentmark-lsp` in `~/.cargo/bin`, next to `agentmark`. The README shows where `agentmark` lives.

The server speaks the protocol on standard input and output and takes no arguments.

**Next:** your editor section below.

## VS Code

Needs Node.js and the language server above. The extension also works without the server: highlighting and the panel keep working, and the extension says so when it cannot find `agentmark-lsp`.

1. Go to the extension folder of the AgentMark source and build the package.

   ```sh
   cd ~/.agentmark/src
   cd editors/vscode
   npm install
   npx @vscode/vsce package
   ```

2. Expect `agentmark-0.4.0.vsix` in the same folder.
3. Install it from the command line, or from the Extensions view. In the view, open the `...` menu and pick "Install from VSIX...".

   ```sh
   code --install-extension agentmark-0.4.0.vsix
   ```

4. Open a folder that holds an `agentmark.toml`, or any `.agent.md` file.
5. Expect the extension to start. Errors and warnings appear as you type.

**Next:** [VS Code settings](#vs-code-settings), then [the touch panel](#what-does-this-program-touch).

### VS Code settings

Both settings are machine-scoped. A workspace cannot set them. They are ignored in an untrusted workspace.

So a downloaded project cannot make your editor run a program it brought along.

| setting | default | meaning |
|---|---|---|
| `agentmark.server.path` | `agentmark-lsp` | the language server binary; give the full path if it is not on your `PATH` |
| `agentmark.cli.path` | `agentmark` | the CLI the touch panel runs (`check --json`) |

The paths below are examples. On Linux the home folder is `/home/you` instead of `/Users/you`.

```json
{
  "agentmark.server.path": "/Users/you/.cargo/bin/agentmark-lsp",
  "agentmark.cli.path": "/Users/you/.cargo/bin/agentmark"
}
```

**Format on save** is on by default for AgentMark files (`"[agentmark]": { "editor.formatOnSave": true }`). The server answers the editor's format request with the canonical style, the same text `agentmark fmt` writes. A saved file never fails `agentmark fmt --check`. To turn it off for yourself, set `"[agentmark]": { "editor.formatOnSave": false }` in your user settings.

Two commands are in the command palette:

* **AgentMark: Show what this program can touch** opens the panel. It is also a button in the title bar of every `.agent.md` file.
* **AgentMark: Restart language server** is for after you reinstall `agentmark-lsp`.

### What does this program touch

The panel shows the answer to the first question a careful reviewer asks. It runs `agentmark check --json` on the project that holds the open file. That project is the nearest folder with an `agentmark.toml`. The panel prints the result in plain words.

While the panel is open it refreshes whenever you save a `.agent.md` file, `agentmark.toml` or `tools.toml`. The JSON it reads is described in [docs/check-json.md](check-json.md).

For `examples/01-invoice-chaser`, `agentmark check` prints this report of what each part touches. The panel is built from the same data (`check --json`, which keeps the four internal classes below).

```console cwd=repo
$ agentmark check examples/01-invoice-chaser
What each part touches
  invoice-chaser / Every day at 09:00: touches AI, crm, email, log
  invoice-chaser / Do remind: touches AI, crm, email, log
  overdue: touches nothing

ok: 0 errors, 0 warnings.
```

The panel shows it as text. The real panel prints the absolute path of the project where the sample says `examples/01-invoice-chaser`.

```text
What can this program touch

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

Per command, service, function, tool, rule and agent, the panel shows:

* the **effect class** (the panel and `check --json` keep these four internal names): `pure` (computes only), `local` (this machine: logs, files), `network` (outside systems), `model` (asks an AI model) or `unverified` (a library nobody approved yet);
* a plain sentence for each kind of access, for example "can reach the network: crm, email" or "asks the AI model";
* the tools it can reach, and the same for each block, with a link to its line. A block is a trigger such as `Every day at 09:00`, or a `## Do` block;
* **Needs your approval**: libraries whose binding still says `- effect: unverified`;
* **Trusted Rust blocks**: every `## Rust` block, because the compiler cannot verify those;
* **Problems**: the errors and warnings of `check`, each linked to its line.

A reviewer reads this page before approving an agent. It shows that the program did not ask for anything it does not need. Nothing here was written by hand. It is computed from the code.

## Neovim

Needs Neovim 0.11 or newer and the language server above.

1. Add this to your config.

   ```lua
   vim.filetype.add({ pattern = { [".*%.agent%.md"] = "agentmark" } })
   vim.lsp.config("agentmark", {
     cmd = { "agentmark-lsp" },
     filetypes = { "agentmark" },
     root_markers = { "agentmark.toml" },
   })
   vim.lsp.enable("agentmark")
   ```

2. Open a `.agent.md` file inside a folder that holds `agentmark.toml`.
3. Expect errors, completion and formatting from the server.

For colours, add the tree-sitter grammar. This needs `nvim-treesitter`, `main` branch.

Config files do not expand `~`, so the path below is written as `/Users/you/.agentmark/src`. Write your own absolute path there. Use the path of your clone if you did not use the script.

4. Register the parser.

   ```lua
   vim.api.nvim_create_autocmd("User", {
     pattern = "TSUpdate",
     callback = function()
       require("nvim-treesitter.parsers").agentmark = {
         install_info = { path = "/Users/you/.agentmark/src/editors/tree-sitter-agentmark" },
       }
     end,
   })
   vim.treesitter.language.register("agentmark", "agentmark")
   vim.api.nvim_create_autocmd("FileType", { pattern = "agentmark", callback = function() vim.treesitter.start() end })
   ```

5. From `~/.agentmark/src`, copy the queries to where Neovim looks for them.

   ```sh
   mkdir -p ~/.config/nvim/queries/agentmark
   cp editors/tree-sitter-agentmark/queries/*.scm ~/.config/nvim/queries/agentmark/
   ```

6. Run `:TSInstall agentmark` and reopen the file.
7. Expect coloured headings, steps and strings. A fenced `rust` block is coloured as Rust.

**Next:** [What the language server does](#what-the-language-server-does).

## Helix

Needs the language server above.

1. Add the server and the language to `languages.toml`.

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

2. Open a `.agent.md` file. Expect errors, completion and formatting.

For colours, add the tree-sitter grammar. Use your own absolute path in place of `/Users/you/.agentmark/src`, because config files do not expand `~`.

3. Declare the grammar next to the language entry.

   ```toml
   [[grammar]]
   name = "agentmark"
   source = { path = "/Users/you/.agentmark/src/editors/tree-sitter-agentmark" }
   ```

4. From `~/.agentmark/src`, build it and copy the queries into Helix's runtime folder.

   ```sh
   hx --grammar build
   mkdir -p ~/.config/helix/runtime/queries/agentmark
   cp editors/tree-sitter-agentmark/queries/*.scm ~/.config/helix/runtime/queries/agentmark/
   ```

5. Reopen the file. Expect coloured headings and steps.

**Next:** [What the language server does](#what-the-language-server-does).

## Zed

Zed starts a language server from an extension, not from a settings file. So this section sets up highlighting only. Zed loads grammars through extensions too.

1. Make a folder for the extension. Put an `extension.toml` in it that names the grammar. Use your own absolute path in place of `/Users/you/.agentmark/src`.

   ```toml
   id = "agentmark"
   name = "AgentMark"
   version = "0.1.0"
   schema_version = 1

   [grammars.agentmark]
   repository = "file:///Users/you/.agentmark/src"
   rev = "HEAD"
   path = "editors/tree-sitter-agentmark"
   ```

2. Add a language folder `languages/agentmark/` in the extension.
3. Put a `config.toml` in it: `name = "AgentMark"`, `grammar = "agentmark"`, `path_suffixes = ["agent.md"]`.
4. Copy the two query files from `editors/tree-sitter-agentmark/queries/` into the same language folder.
5. Run the command "zed: install dev extension" and pick the extension folder.
6. Open a `.agent.md` file. Expect coloured headings and steps.

## Other editors

Point any LSP client at the `agentmark-lsp` binary for files ending in `.agent.md`. Set the project root to the folder that holds `agentmark.toml`. The Neovim and Helix examples above show the shape.

Adjust them to the version of your editor.

After setup, open a `.agent.md` file and expect the same errors as `agentmark check`.

## Tree-sitter grammar

`editors/tree-sitter-agentmark` is a tree-sitter grammar for `.agent.md` files. The generated parser is committed as `src/parser.c`, so you do not need the tree-sitter CLI to use it.

The grammar name is `agentmark`, its scope is `text.agentmark`, its file type is `agent.md`. It ships two query files in `queries/`:

* `highlights.scm`, the highlighting;
* `injections.scm`, which highlights a fenced `rust` block as Rust. That block is the body of a `## Rust` heading.

To change the grammar itself, edit `editors/tree-sitter-agentmark/grammar.js`. Then run these in that folder:

1. `npm install`
2. `npm run generate`
3. `npm test`

The test also parses every file under `examples/`.

## agentmark.toml

The VS Code extension validates `agentmark.toml` through the TOML validation of the Even Better TOML extension, using `editors/vscode/schemas/agentmark.schema.json`. There is nothing to configure. An unknown key is flagged, and a lint rule name gets completion.

This is the whole file, with every section the schema knows:

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
| `[crates]` | how a bound library is built, for the binder and the generated `Cargo.toml`: `sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio"] }` (and `default-features = false`); without an entry the crate's defaults apply (section 15.4 of [the reference](reference.md)) |
| `[lsp] model_actions` | `true` turns on the optional model-backed code actions below; off by default |
| `[models]` | environment name to model driver, picked by `AGENTMARK_ENV` (section 19 of [the reference](reference.md)); `[models.local] base_url` is the server for `local:<model>` drivers |

## What the language server does

`agentmark-lsp` offers these features in any editor:

| feature | what you get |
|---|---|
| diagnostics | the errors and warnings of `agentmark check` and `agentmark lint`, with the same text, as you type |
| completion | typed after `.` or a space: heading kinds (`Agent:`, `Record:`, `Enum:`, `Tool:` ...), triggers, steps, tools and their verbs (the built-in `file` tool and your own `# Tool` declarations included), records, enums and their variants, rules, constants, bound libraries, the modules of the built-in library (`text.` offers `lower`, `split`, ...), the methods of a handle after the variable's name, and the fields of a variable after `x.` |
| go to definition | from a name to its declaration, across `# Uses` links |
| hover | on a heading: what the block, or the whole agent, command or tool, touches; on a record field: its type, and whether the type was inferred from the name |
| formatting | `textDocument/formatting` returns the canonical style (same as `agentmark fmt`) |
| code actions | quick fixes for the mechanical lint warnings (add a missing description to a part, which is what `missing-doc` asks for; add a comment above a guard; extract a repeated literal into a `# Constant`) and for "did you mean" suggestions |
| model actions | only with `[lsp] model_actions = true` and a model available: `AGENTMARK_MODEL` if set, else the `[models]` entry that `AGENTMARK_ENV` names, else Claude with `ANTHROPIC_API_KEY` (section 19 of [the reference](reference.md)). The actions are "Explain this block" and "Suggest a guard for this answer" on an `ask model` line |

The server reads the project from disk and the open buffers from the editor, so unsaved edits are checked too. It never writes a file by itself. Every fix is an edit the editor applies and you can undo.
