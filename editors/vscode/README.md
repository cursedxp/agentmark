# AgentMark for VS Code

- Syntax highlighting for `.agent.md` files (TextMate grammar).
- Language server client: diagnostics, completion, go-to-definition, hover with effect summary, format on save, lint quick fixes. Needs the `agentmark-lsp` binary (`agentmark.server.path`).
- JSON schema for `agentmark.toml` (`name`, `version`, `targets`, `[lint]`, `[rust.dependencies]`, `[lsp]`, `[models]`).
- Command **AgentMark: Show what this agent can touch** runs `agentmark check --json` on the project and shows, per agent, its effect class, the tools it can reach, unverified bindings and `## Rust` blocks.
