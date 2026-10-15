# AgentMark project

This project is written in AgentMark, a programming language written as Markdown: files ending in `.agent.md` that `agentmark build` compiles to a Rust binary. It is general-purpose (records, enums, functions, commands, services, collection words, a built-in library, files); agents that ask an AI model are one feature of it, used only when the project needs them. You are writing AgentMark, not Rust.

## The one source of truth

Read `docs/reference.md` of the AgentMark source (`~/.agentmark/src` after `install.sh`) before you write anything. It is the whole language in one file: every declaration, every step form, every error message, with examples that are tested. Do not guess syntax from other languages and do not invent steps. If `docs/reference.md` does not show it, it does not exist; ask the human.

If this project lives outside the AgentMark source, keep a copy or a link of that file next to the project (for example `docs/agentmark-reference.md`) and read that one.

## Workflow

1. Edit one `.agent.md` file.
2. Run `agentmark check .` (or the file) straight away. Fix every error before touching anything else. The messages say what is wrong and often what to write instead.
3. Run `agentmark fmt .` so the files are in the one canonical style. `agentmark fmt --check .` only reports.
4. Run `agentmark lint .` and fix the warnings, or ask before adding to `[lint] deny`.
5. Run and test: `agentmark run . -- <arguments>` runs a command program (the arguments after `--` go to it); `agentmark trace . -- <arguments>` prints what the program did, one JSON object per line. A program with a model or outside tools adds `--mock --fixtures fixtures/<name>` so nothing touches the network. Compare traces and output, not generated Rust.
6. `agentmark doc .` prints a readable page of the project, including what each part touches. `agentmark check .` prints the same "What each part touches" report; read it after every change and tell the human when a part starts touching something new.

Run `check` after every edit, not at the end. A file that was green five edits ago tells you nothing.

## Rules

* Never write Rust, except inside a `## Rust: <name>` block when the human asked for it or no bound library does the job. Never edit generated code under `target/` or an `-o` output folder; change the `.agent.md` source.
* Never edit `rust/connect.rs` beyond filling in the tool implementations the human asked for.
* The `- effect:` line in `bindings/*.agent.md` is for humans to approve. Never change `- effect: unverified` there, and never loosen a block's tools to silence `check`. Show the human what the library can touch and let them approve it. `--allow-unverified` is for exploring, not for finishing.
* A `## Rust` block needs a `- touches:` line. Declare what it honestly touches (`nothing`, `files`, `print`, `AI` or a tool name; never less to pass `check`) and tell the human, because that line is trusted, not verified. A `# Function` or `# Tool` needs no `- touches:` line: `check` works it out, and a line you write is a promise that `check` verifies.
* Keep a block small: one step per line, at most 120 characters, at most 3 levels of nesting (`check` enforces this). Write a description, a plain paragraph directly under the heading, for every part: record, enum, function, command, tool, rule, agent, trigger and `## Do` block. A `>` line is a note for people and never a description; a `>` line directly above a heading is an error.
* Write steps as a numbered list (`1.` on every line is fine; `fmt` renumbers) and properties as bullets. Name a result with `into`: `group sales by region into by_region`, `try file read path into sales`. The old forms are errors: `set`, `->` after a step, `append`, `args:`, `## Run`, `# Run`, `- for:` in a rule, `# Import: ./file`, `- effect:` on a function. Their messages show the replacement.
* A rule names its record last in the heading: `# Rule: big Sale`. Add to a list with `add RegionTotal to totals:` and one bullet per field. The entry file starts with `# Program: <name>`, a description and `- start: [command](#command-name)`, then a `# Uses` list of links (`- [Sale](./records/sale.agent.md)`).
* Tools that live outside the program come from `tools.toml`. Do not call a tool or a verb that is not listed there; add it to the manifest only when the human asks. The `file` tool and the library (`text`, `regex`, `number`, `money`, `date`, `path`, `json`, `csv`) are built in and need no entry. A small helper you can write in AgentMark is a `# Tool` or a `# Function`, not a `tools.toml` entry.
* In an agent, follow every `ask model` with a `guard:` on its answer (the lint `prompt-in-loop` wants it inside a `for each`). Name a money, duration or URL literal used more than once as a `# Constant` (the examples keep them in `constants.agent.md`).
* No secrets in files. Keys come from the environment (`ANTHROPIC_API_KEY`).

## Where things live

```
agentmark.toml      name, version, targets, [lint], [models], [crates] (library features), [rust.dependencies]
tools.toml          the tools a program may use, their verbs and effect classes (`tools.toml` and library bindings keep the word `effect`; everything else says `touches`)
main.agent.md       `# Program`, its description, `- start:` and the `# Uses` links of what runs
records/            one # Record (or # Enum) per file
functions/          one # Function per file
commands/           one # Command per file
services/           one # Service per file
rules/              one # Rule per file (agents and filters)
agents/             one # Agent per file, only when the project has one
bindings/           generated by `agentmark bind`; reviewed by a human, committed
fixtures/           model answers, tool data and expected traces for --mock; for a program that is not an agent run.json (args, stdin), input/ and expected.stdout
rust/connect.rs     the real tool implementations (written once, then yours)
```

A new command, service or agent is a new file plus a link in the `# Uses` list of `main.agent.md`; a new command also goes in `- start:` when it is the one the program starts. A program with exactly one command and no agent or service is started as `my-tool <arguments>`.

## When you are stuck

Say what you tried and what `check` answered. Do not work around the checker.
