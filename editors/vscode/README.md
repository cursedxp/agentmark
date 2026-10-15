# AgentMark for VS Code

- Syntax highlighting for `.agent.md` files (TextMate grammar): `# Program` and `# Uses`, descriptions under headings (plain text), numbered steps, `into`, `add … to …:` with its field bullets, and `- touches:`, `- steps:`, `- start:`.
- Language server client: diagnostics, completion, go-to-definition, hover with what a part touches, format on save, lint quick fixes. Needs the `agentmark-lsp` binary (`agentmark.server.path`).
- JSON schema for `agentmark.toml` (`name`, `version`, `targets`, `[lint]`, `[rust.dependencies]`, `[lsp]`, `[models]`).
- Command **AgentMark: Show what this program can touch** runs `agentmark check --json` on the project and shows, per command, service, function, tool and agent, what it touches (`touches AI, crm, email, log` or `touches nothing`), unverified bindings and `## Rust` blocks.

## Settings

| setting | default | meaning |
|---|---|---|
| `agentmark.server.path` | `agentmark-lsp` | the language server binary |
| `agentmark.cli.path` | `agentmark` | the CLI used for `check --json` |

Both are machine-scoped (a workspace cannot set them) and are ignored in untrusted workspaces.

`[agentmark]` files default to `editor.formatOnSave: true`; the server answers `textDocument/formatting` with the canonical style. `agentmark.toml` is validated through `tomlValidation` (Even Better TOML) with no manual setup. The panel (editor-title button on `.agent.md` files, or the command palette) refreshes whenever you save an AgentMark or TOML file, and says it for each part in plain words, the same way `agentmark check` does: "touches nothing", "touches files, print", "touches AI, crm, email, log" (the AI first, then the rest in alphabetical order).
