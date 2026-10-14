# AgentMark for VS Code

- Syntax highlighting for `.agent.md` files (TextMate grammar).
- Language server client: diagnostics, completion, go-to-definition, hover with effect summary, format on save, lint quick fixes. Needs the `agentmark-lsp` binary (`agentmark.server.path`).
- JSON schema for `agentmark.toml` (`name`, `version`, `targets`, `[lint]`, `[rust.dependencies]`, `[lsp]`, `[models]`).
- Command **AgentMark: Show what this agent can touch** runs `agentmark check --json` on the project and shows, per agent, its effect class, the tools it can reach, unverified bindings and `## Rust` blocks.

## Settings

| setting | default | meaning |
|---|---|---|
| `agentmark.server.path` | `agentmark-lsp` | the language server binary |
| `agentmark.cli.path` | `agentmark` | the CLI used for `check --json` |

Both are machine-scoped (a workspace cannot set them) and are ignored in untrusted workspaces.

`[agentmark]` files default to `editor.formatOnSave: true`; the server answers `textDocument/formatting` with the canonical style. `agentmark.toml` is validated through `tomlValidation` (Even Better TOML) with no manual setup. The panel (editor-title button on `.agent.md` files, or the command palette) refreshes whenever you save an AgentMark or TOML file, and describes each agent in plain words, for example "can reach the network: crm, email" or "asks the AI model".
