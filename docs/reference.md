# AgentMark language reference

**In short**
- This page lists every form of the AgentMark language, section by section, in the order of `SPEC.md`.
- It is for people, editor plugins and AI assistants that write AgentMark.
- After reading it you can write a correct statement for any task and look up its error.

**Jump to:** [Cheat sheet](#cheat-sheet) · [Statements](#4-statements) · [Expressions](#5-expressions) · [Functions, commands, services](#13-core-language-functions-and-modules-layer-2) · [Errors](#14-errors) · [Collections, library, tools](#22-v03-a-general-purpose-markdown-language) · [Statement index](#appendix-a-statement-index)

## Cheat sheet

The 20 statement forms you will write most. Appendix A lists all of them.

| form | what it does | read |
|---|---|---|
| `set total = total + 1` | creates or changes a variable | [4.5](#45-assignment) |
| `for each invoice in crm.invoices:` | repeats the body for every item | [4.1](#41-control) |
| `if amount > 1000 EUR:` then `else:` | picks a branch | [4.1](#41-control) |
| `match shape:` | takes an enum, a `maybe` or a `result` apart | [22.1](#221-user-defined-types) |
| `stop` | ends the current block | [4.1](#41-control) |
| `do remind` or `do f(x) -> y` | calls a `## Do` block or a function | [4.1](#41-control), [13.1](#131-function) |
| `return x * 2` | gives back the value of a function | [13.1](#131-function) |
| `guard: reply is not empty` | checks a condition and stops when it fails | [4.4](#44-guards) |
| `ask model: "Name a color." -> answer` | asks a model | [4.2](#42-model) |
| `crm update invoice: status = "paid"` | calls a tool | [4.3](#43-tool-calls) |
| `try http get "https://x.example" -> r` | runs a call that can fail, with `otherwise:` as the fallback | [14](#14-errors) |
| `print: "Total {total}"` | writes a line to standard output | [13.3](#133-service-and-command) |
| `read file path as list of Row -> rows` | reads a file into a typed value | [13.3](#133-service-and-command) |
| `file write "out.txt": body` | writes, appends, lists or renames files | [22.3](#223-the-built-in-library) |
| `append row to rows` | adds to the end of a list | [22.2](#222-collection-words) |
| `set top = sort rows by amount descending` | sorts, groups, sums and filters lists | [22.2](#222-collection-words) |
| `text.trim(name)` | calls the built-in library | [22.3](#223-the-built-in-library) |
| `pdf render title -> bytes` | calls a bound Rust library | [15](#15-library-access-automatic-bindings) |
| `respond 404` | ends a service route with a status | [13.3](#133-service-and-command) |
| `parallel:` | runs the indented statements at the same time | [16](#16-concurrency) |

## How to read this page

This is the whole language in one file. It follows `SPEC.md` section by section, so the section numbers here are the specification's numbers. The specification has no section 12.

Every section opens with a small complete example, then a list of rules, then the details. Code blocks marked as programs are checked by `tests/docs.rs`, and pieces without a heading line are parsed there. Output shown here is the real output of the tools.

Where the specification left a question open, the text states the choice and `OPEN_QUESTIONS.md` lists it as a `Q` number. Appendix B groups the numbers by topic.

**Rules that hold everywhere**
- One statement is one line, at most 120 characters. The text between the quotes of a string does not count, per Q35.
- Indentation is exactly 2 spaces per level. A tab is an error.
- A file ends in `.agent.md`.
- Names are case-sensitive. Headings are not.

## Language at a glance

```agentmark
# Enum: Shape
- Circle: radius: int
- Rect: width: int, height: int
- Empty

> Area of a shape, rounded down.
# Function: area
- in: shape: Shape
- out: int
- effect: pure
  - match shape:
    - Circle c:
      - return c.radius * c.radius * 3
    - Rect r:
      - return r.width * r.height
    - Empty:
      - return 0

> Prints the areas of three shapes, biggest first.
# Command: areas
- args: label: text = "Areas"

## Run
- set shapes = list of Rect { width: 2, height: 5 }, Circle { radius: 2 }, Empty
- set sizes = map shapes to area(item)
- set ranked = sort sizes by item descending
- set words = map ranked to "{item}"
- print: "{label}: {join words with ", "}"

# Run
- areas
```

**Rules**
- AgentMark is a general-purpose programming language written as Markdown. `agentmark build` turns a project into one standalone binary.
- A program is made of declarations: `# Record`, `# Enum`, `# Function`, `# Command`, `# Service`, `# Tool` and `# Constant`. Each is a heading with `-` lines under it.
- The program above uses no agent, no tool and no outside library. It needs only the built-in ones.
- Agents are one feature, not the center. Before anything is generated, every agent construct is rewritten into functions and entries, as section 0 shows.
- The agent sections keep the places the specification gives them. If you never write an agent, read sections 2.1, 4.1, 4.3, 4.5, 5 to 8 and 13 to 18 first.

**Where each part is described**

| you want | read |
|---|---|
| records, rules, constants, files and imports | [2.1](#21-record), [2.2](#22-rule), [1](#1-file-structure), [18](#18-code-organization-and-readability) |
| functions, commands, services | [13](#13-core-language-functions-and-modules-layer-2) |
| enums, `match`, collection words, the built-in library, the `file` tool, user-defined tools, default commands | [22](#22-v03-a-general-purpose-markdown-language) |
| `try`, `maybe`, errors | [14](#14-errors) |
| Rust libraries and `## Rust` blocks | [15](#15-library-access-automatic-bindings), [17](#17-escape-hatch--rust-blocks-layer-1) |
| types and units such as money, date and duration | [6](#6-types) |
| what a block can touch: `pure`, `local`, `network`, `model` | [8](#8-effects) |
| agents: `# Agent`, triggers, `ask model`, `guard`, model providers | [2.3](#23-agent), [3](#3-blocks-triggers), [4.2](#42-model), [4.4](#44-guards), [19](#19-model-providers) |

## 0. Architecture: three layers

This program is the weekly report. On Monday the agent counts invoices with two rules, asks a model for two sentences, checks them and mails them.

```agentmark
# Record: Invoice
- id: text
- customer: text
- amount: money
- due: date
- status: text

# Rule: overdue
- for: Invoice
- when: today > due + 14 days and status != "paid"

# Rule: open
- for: Invoice
- when: status != "paid"

# Agent: daily-report
- model: claude
- tools: crm, email, log

## Every Monday at 08:00
- set open_count = count of open invoices in crm.invoices
- set overdue_count = count of overdue invoices in crm.invoices
- ask model: "Write a two-sentence Monday summary. Open invoices: {open_count}. Overdue: {overdue_count}." -> summary
- guard: summary is not empty
- email send address "anil@optimotion.dev": "{summary}"
- log: "Report sent: {open_count} open, {overdue_count} overdue"
```

**Rules**
- Layer 3 is the agent layer, layer 2 is the core language and layer 1 is the escape hatch.
- Layer 3 is sugar over layer 2. Layer 1 is Rust written inside layer 2.
- A feature enters layer 2 only after it has appeared in layer 1 three times.

| layer | what lives there | where it is described |
|---|---|---|
| 3. Agent layer | `# Agent`, `# Rule`, triggers (`## Every ...`), `ask model`, `guard` | sections 2 to 4 |
| 2. Core language | `# Record`, `# Enum`, `# Function`, `# Service`, `# Command`, `# Tool`, `# Constant`, control flow, `match`, collection words, the built-in library, `try`, library bindings | sections 13 to 16 and 22 |
| 1. Escape hatch | `## Rust` blocks: verbatim Rust inside any block | section 17 |

**How layer 3 becomes layer 2**

Before the program is turned into Rust, every agent construct is rewritten into layer 2.
- A rule becomes a pure function that returns yes or no.
- A trigger block becomes an **entry** that runs on a schedule or on an event.
- A `## Do` block becomes a function named `<agent>__<block>`.
- `ask model` becomes a model call.
- A `guard` becomes: evaluate, write a trace line, stop if it failed.

After this step no agent, rule, trigger or guard remains. The code that does it is `src/lower/agent_lower.rs`, and the generator that writes Rust never sees an agent.

**Next:** [1. File structure](#1-file-structure)

## 1. File structure

A file is a sequence of **declarations**. Each starts with a level-1 heading, and the lines under it are properties and blocks.

```agentmark
> Counts open invoices. The comment above a heading documents it.
# Agent: counter
- model: claude
- tools: crm, log

> This comment documents the block.
## Every day at 09:00
> A plain comment, about the next statement only.
- log: "counting"
```

**Rules**
- A declaration starts with a level-1 heading `# Kind: name`.
- `- key: value` lines under it are **properties**. `## heading` lines start **blocks**, and the lines of a block are **statements**.
- Declaration kinds are `Record`, `Enum`, `Type`, `Rule`, `Agent`, `Function`, `Service`, `Command`, `Tool`, `Binding`, `Constant`, `Import` and `Run`. Any other word is an error that suggests the closest kind.
- Properties come before the first `##` heading.
- Statements sit at indentation 0 directly under a `##` heading, per Q16. Every nested body is indented by one more 2-space step.
- Nested bodies are those of `for each`, `if`, `else`, `otherwise` and `parallel`.
- Function bodies are the exception. They are indented under the `- effect:` property, as section [13.1](#131-function) shows.

**Statements and comments**
- Statements are list items that start with `- `. The exceptions are `otherwise:` and, in older files, bare `for each ...:` and `stop` lines.
- The formatter writes every statement as a list item, per Q51.
- A line that starts with `>` is a comment.
- A run of comment lines directly above a `#` or `##` heading documents that heading.
- A run at the top of a file followed by a blank line documents the file.
- All other comments belong to the next statement, field or property, per Q17 and Q28.

**Names and blank lines**
- Identifiers are letters, digits, `-` and `_`, and start with a letter.
- Declaration, rule, record and type names may contain `-`, as in `very-overdue`.
- Variables, fields and paths inside expressions may not contain `-`. There it is subtraction and needs a space on both sides, as in `today - 1 month`, per Q19 and Q29.
- Blank lines are ignored.
- A line that is not a heading, a comment, a `- ` item, a fence or a control line is an error, per Q22.

**Next:** [2. Declarations](#2-declarations)

## 2. Declarations

Three declarations carry most programs: records hold data, rules ask yes/no questions about it and agents act on it.

### 2.1 Record

A record describes a data shape. Fields are list items with an optional type.

```agentmark name=invoice.agent.md
# Record: Invoice
- id: text
- customer: text
- amount: money
- due: date
- paid_on: maybe date
- shipped_at
- items_count
- total
- remark

> Lists invoices.
# Agent: lister
- model: claude
- tools: crm, log

## Every day at 09:00
- for each invoice in crm.invoices:
  - log: "{invoice.id} {invoice.total} {invoice.remark}"
```

**Rules**
- A field without a type gets one from the last word of its name.
- Words are separated by `_`, `-` or a change from lower to upper case, as in `dueDate`.
- The whole word has to match, per Q32.
- Writing the type is always allowed and always clearer.

| last word | type |
|---|---|
| `date`, `at`, `on` | `date` |
| `amount`, `price`, `total`, `fee` | `money` |
| `count`, `qty`, `number` | `int` |
| anything else | `text`, with warning W001 that prints the type it picked |

In the example, `shipped_at` is a `date`, `items_count` is an `int` and `total` is `money`. `remark` falls back to `text` and produces the warning.

`agentmark check` lists every guessed type. The warning itself goes to standard error and the list goes to standard output:

```console
$ agentmark check invoice.agent.md
Effects (what each block can touch)
  lister / Every day at 09:00: network(crm), local(log)

Field types guessed from names
  Invoice.shipped_at: date (from the field name; invoice.agent.md:7)
  Invoice.items_count: int (from the field name; invoice.agent.md:8)
  Invoice.total: money (from the field name; invoice.agent.md:9)
  Invoice.remark: text (no naming rule matched, so it is text; write the type to be sure; invoice.agent.md:10)

ok: 0 errors, 1 warning.
```

### 2.2 Rule

A rule is a named, pure yes/no question about one record.

```agentmark
# Record: Invoice
- id: text
- due: date
- status: text

> Unpaid and more than 14 days past due.
# Rule: overdue
- for: Invoice
- when: today > due + 14 days and status != "paid"

> Logs the overdue invoices.
# Agent: lister
- model: claude
- tools: crm, log

## Every day at 09:00
- for each overdue invoice in crm.invoices:
  - log: "{id} is overdue"
```

**Rules**
- A rule has no side effects. Anything effectful inside it is an error.
- Inside `when:`, a bare field name such as `due` is a field of the record the rule is `for:`.
- `when:` must be a yes/no value.
- A filtered loop uses a rule, as in `for each overdue invoice in ...`. See section [4.1](#41-control).
- `count of overdue invoices in ...` uses a rule too. See section [5](#5-expressions).

### 2.3 Agent

An agent is the executable unit.

```agentmark
> Mails a note once an hour, remembers nothing, and gives itself ten minutes per run.
# Agent: notifier
- model: claude:haiku
- tools: email, log
- memory: file
- timeout: 10 minutes

## Every 1 hour
- email send address "ops@example.com": "Still alive"
- log: "sent"
```

**Rules**
- An agent may call only tools listed in its `tools` line. Calling another is an error: `The tool "email" is not listed in the tools of agent ...`.
- The same list limits reading a tool's source, as in `for each ... in crm.invoices`.
- Services, commands and functions have no `tools` line and may use every tool of the manifest.
- A function's declared `effect` is what bounds it, per Q38.

**Properties**

| key | required | meaning |
|---|---|---|
| `model` | yes | the model driver string: `claude`, `claude:<model>`, `mock` (section [19](#19-model-providers)) |
| `tools` | no | comma-separated tool names the agent may use; default none |
| `memory` | no | `none` (default) or `file`: keep state between runs |
| `timeout` | no | a duration; default `5 minutes` per block execution |

**Next:** [Section 3](#3-blocks-triggers)

## 3. Blocks (triggers)

A block is a `##` heading with statements under it. The heading says when the block runs.

```agentmark
# Record: Ticket
- id: text
- subject: text

> Shows every trigger heading once.
# Agent: triggers
- model: claude:haiku
- tools: crm, log

## On start
- log: "started"

## Every 30 minutes
- do ping

## Every day at 09:00
- log: "daily"

## Every Friday at 17:30
- log: "weekly"

## When crm receives ticket
- log: "New ticket {ticket.subject}"

## Do ping
- log: "ping"
```

**Rules**
- The heading is the block's **label**. The effect summary and the trace report it.
- The formatter rewrites a loosely typed heading into the canonical one. `## every  MONDAY at 08:00` becomes `## Every Monday at 08:00`, per Q50.
- Blocks of one agent run one after the other. Two triggers that fire together are queued, never run in parallel.
- Schedules use naive local time without daylight-saving handling. `Every` fires one interval after the previous tick, per Q15.
- Duration units are `second`, `minute`, `hour`, `day`, `week`, `month` and `year`, singular or plural.

**Trigger headings**

| heading | fires when |
|---|---|
| `## Every <duration>` | periodically, for example `Every 30 minutes` or `Every 1 hour` |
| `## Every day at <HH:MM>` | daily at local time, 24-hour clock |
| `## Every <weekday> at <HH:MM>` | weekly; `Monday` to `Sunday` |
| `## When <tool> receives <event>` | an inbound event of a tool; the event word must be in the tool's `events` list |
| `## On start` | once when the program starts |
| `## Do <name>` | never by itself: a named action, called by `do <name>` |

**Event blocks**
- `## When tool receives event` binds the event's record to a variable named after the event, such as `ticket`.
- The record must be visible in the file. Its name is the event word, capitalised, so `Ticket`.

**Do blocks**
- A `## Do` block has no parameter list. It works on the record in front of its caller: the innermost `for each` record or the event record.
- All callers must hand it the same record. The first caller's variable name becomes the name inside the block, per Q30.
- A `## Do` block that nobody calls is a warning.

**Next:** [4. Statements](#4-statements)

## 4. Statements

A statement is one list item. Control statements end in `:` and own the more-indented lines below them.

### 4.1 Control

```agentmark
# Record: Invoice
- id: text
- amount: money
- due: date
- status: text

# Rule: overdue
- for: Invoice
- when: today > due + 14 days and status != "paid"

> Escalates big overdue invoices, reminds for the rest.
# Agent: router
- model: claude
- tools: crm, log

## Every day at 09:00
- for each overdue invoice in crm.invoices:
  - if invoice.amount > 1000 EUR:
    - do escalate
  - else:
    - do remind

## Do escalate
- log: "Escalate {id}"

## Do remind
- log: "Remind {id}"
```

**Rules**
- `for each <var> in <source>:` runs the body once per item. The source is a tool source such as `crm.invoices`, a list variable or a list in `state`.
- `for each <rule> <var> in <source>:` keeps only the items for which the rule is true. Its body sees the record's fields as bare names, like `id` above.
- `if <condition>:` takes an optional `else:` directly after it at the same indentation. The condition must be yes/no.
- `if <expr> is some <name>:` unwraps a `maybe`. See section [14](#14-errors).
- `do <name>` calls a `## Do` block of the same agent. `do f(args) -> x` calls a function, as section [13.1](#131-function) shows.
- `stop` ends the current block, not only the current loop iteration, per Q3.

**The control forms**

```agentmark fragment
- for each invoice in crm.invoices:
  - log: "{invoice.id}"
- for each overdue invoice in crm.invoices:
  - log: "{id}"
- if invoice.amount > 1000 EUR:
  - log: "big"
- else:
  - log: "small"
- if lead.score is some s:
  - log: "{s}"
- stop
```

**Watch out:** `stop` ends the whole block, not one pass of the loop. This is deliberate, because it is how an agent says "enough for this run".

The third example shows it:

```agentmark file=examples/03-support-triage/agents/support-triage.agent.md
# Import: ../constants.agent.md
# Import: ../records/ticket.agent.md
# Import: ../records/triage.agent.md

> Classifies tickets that have no priority yet and alerts on the high ones. Never replies to customers.
# Agent: support-triage
- model: claude
- tools: crm, http, log

## Every 15 minutes
- for each ticket in crm.tickets:
  - if ticket.priority is not empty:
    - stop
  - ask model with ticket: triage_prompt as Triage -> t
  > The model must pick one of the three allowed priorities, nothing else.
  - guard: t.priority = "low" or t.priority = "medium" or t.priority = "high"
  - crm update ticket: priority = t.priority
  - if t.priority = "high":
    - http post "https://hooks.example.com/alert": { ticket: ticket.id, category: t.category }
  - log: "Triaged {ticket.id} as {t.priority}/{t.category}"
```

In this agent the first ticket that already has a priority ends the whole 15-minute run. That is the specified behaviour. To skip one item only, wrap the rest of the body in a negated `if`.

### 4.2 Model

```agentmark
# Record: Lead
- id: text
- company: text

# Record: Qualification
- score: int
- reason: text

> Asks the model three ways.
# Agent: asker
- model: claude:haiku
- tools: crm, log

## When crm receives lead
- ask model: "Name one famous company." -> plain
- ask model with lead: "Describe this lead in one sentence." -> description
- ask model with lead: "Score this lead from 1 to 10." as Qualification -> q
- guard: q matches Qualification
- log: "{plain} / {description} / {q.score}: {q.reason}"
```

**Rules**
- `ask model` is effectful, with class `model`. The result is `text` unless `as <Record>` is given, and then it is a value of that record.
- `with <var>` passes a value as context. The model receives the prompt text, a blank line, then the context as JSON. Without `with` it receives the prompt alone, per Q156.
- A mock model matches on the prompt prefix, so `with` does not disturb fixtures.
- A reply that is not valid JSON for the record is not a runtime error. Not JSON, a missing field and a field of the wrong type all make `q matches Qualification` false, so the guard fails and the block stops, per Q157.
- So always write the `matches` guard after `as <Record>`.
- The prompt may be a string with `{holes}` or the name of a constant of type text, as in `ask model with ticket: triage_prompt as Triage -> t`.
- The agent's `model:` line selects the driver. See section [19](#19-model-providers).
- `ask model` also works outside an agent: in a function declared `effect: model`, in a route or in a command. There is no `model:` line there to say which driver, and the IR records none for such a call, per Q92.

**The four forms**

```agentmark fragment
- ask model: "<prompt>" -> answer
- ask model: "<prompt>" as Qualification -> verdict
- ask model with lead: "<prompt>" -> answer
- ask model with lead: "<prompt>" as Qualification -> verdict
```

### 4.3 Tool calls

```agentmark
# Record: Ticket
- id: text
- subject: text
- priority: maybe text

> Shows the shapes of a tool call.
# Agent: caller
- model: claude
- tools: crm, email, http, log

## Every 15 minutes
- for each ticket in crm.tickets:
  - crm get ticket id -> fresh
  - crm update ticket: priority = "low", subject = "changed"
  - http get "https://example.com/status" -> status
  - http post "https://example.com/hook": { id: ticket.id, subject: ticket.subject }
  - email send address "ops@example.com": "Ticket {ticket.subject}"
  - log: "done {ticket.id}"
```

**Rules**
- The shape is `- <tool> <verb> [<target>]: <payload> [-> var]`.
- The tool and the verb both come from `tools.toml`. An unknown verb is an error with "Did you mean ...?".
- A tool without verbs, like `log`, is written `- log: "..."`.
- `-> name` keeps the result of `get`, `list` or `post`, typed by the manifest when it can.
- Reading a tool's source, as in `for each ... in crm.invoices`, is an effect of that tool: `network(crm)`.

**Pieces of a call**

| piece | forms |
|---|---|
| target | a word from the tool's `targets` (`email send customer`, `email send address "x@y.z"`), or the singular of one of its sources (`crm update invoice`), or a URL (`http get "https://..."`) |
| payload, text | `"Your invoice {amount} is overdue."` |
| payload, record | `{ id: invoice.id }` |
| payload, assignments | `status = "reminded"`, `score = q.score, status = "scored"` |
| payload, a value | `crm update invoice: inv` (a whole record) |

**How target words are resolved**

The rules are Q4 and Q5.
- `email send customer` reads the field `customer_email` of the record in front of the block when it exists, otherwise `customer`. The trace reports the word `customer` as the target and the address as `to`.
- `crm update invoice` names the source `invoices`, the plural of the word. It takes the `id` of the record in front of the block.
- When several variables of that record are in scope, the call is an error: "Which invoice?". Name the one you mean with a value after the word.
- A tool's name alone is never a value, so a field called `email` is a field.
- A target word may be a field path of a variable, as in `- statement bind draft.title -> next`, per Q410.

### 4.4 Guards

```agentmark
# Record: Invoice
- id: text
- amount: money
- customer: text

> Mails a reminder only when the model left the amounts alone.
# Agent: careful
- model: claude
- tools: crm, email, log

## Do remind
- ask model with invoice: "Write a short, polite payment reminder." -> reminder
- guard: no amounts changed in reminder
- guard: reminder is not empty
- guard: reminder length < 800
- guard: reminder length > 20 else set reminder = "Please settle your open invoice."
- email send customer: "{reminder}"

## Every day at 09:00
- for each invoice in crm.invoices:
  - do remind
```

**Rules**
- A guard is a pure check. If it fails and no `else` is given, the current block stops and the failure is logged with the guard's source line.
- With `else <statement>`, that statement runs instead of stopping. The block carries on afterwards unless the statement ends it.
- `else respond 404` in a route ends the block, as in section [13.3](#133-service-and-command). `else stop` ends it too.
- `else set reminder = "..."` does not end the block, so it is a way to fall back to a default when the check fails.
- The trace line says `fail` in both cases.
- Every guard writes one trace line with its source text and `pass` or `fail`. This holds in an agent, a service, a command and a function.
- Guards may use constants, which are inlined into that text, per Q90.
- The lint rule `unnamed-guard` asks for a `>` comment above a guard whose condition is longer than 60 characters.

**Built-in helpers**

| helper | true when |
|---|---|
| `no amounts changed in <var>` | the model output holds no money value that was not in its input (the `with` value of the call that produced it, else its prompt). `1200 EUR` equals `1200.00 EUR` (Q13, Q93) |
| `<var> is not empty` | the text, list or `maybe` value has something in it. After the guard, a `maybe` value counts as present |
| `<var> matches <Record>` | the value has the shape of the record |
| `<var> length < <int>` | the length of a text or list; any comparison works (`>`, `<=`, `=`, ...) |

### 4.5 Assignment

```agentmark
# Record: Invoice
- id: text
- amount: money
- status: text

> Counts.
# Agent: totals
- model: claude
- tools: crm, log

## Every day at 09:00
- set total = 0 EUR
- for each invoice in crm.invoices:
  - set total = total + invoice.amount
- log: "Total {total}"
```

**Rules**
- `set <var> = <expression>` creates the variable or changes it.
- A `maybe T` variable may be given a plain `T`, never the other way round, per Q40.

**Next:** [5. Expressions](#5-expressions)

## 5. Expressions

An expression computes a value. It appears after `set`, in conditions, in the `{holes}` of a string and in call arguments.

```agentmark
# Record: Invoice
- id: text
- customer: text
- amount: money
- due: date
- status: text

# Rule: overdue
- for: Invoice
- when: today > due + 14 days and status != "paid"

> Shows the expression forms.
# Agent: expressions
- model: claude
- tools: crm, log

## Every day at 09:00
- set open_count = count of overdue invoices in crm.invoices
- set net = 120.50 EUR - 20.50 EUR * 2 / 4
- set next_check = today + 14 days
- set a_month_ago = today - 1 month
- set late = open_count > 3 or not open_count = 0
- set first_late = first invoice in crm.invoices where invoice.status = "open"
- log: "{open_count} invoices, {net}, due on {next_check}, since {a_month_ago}, {late}"
```

**Rules**
- A name resolves in this order: variable, bare field, constant. Only a name with a dot can be a tool source such as `crm.invoices`, per Q41.
- `invoice.amount` is a field of a variable. A bare `amount` is a field of the record in front of the block. That record is the item of a `for each`, the record of a rule or the record of an event block.
- In a string, `{customer}` and `{invoice.amount}` insert values. A hole holds a full expression.
- A text hole holds a single value. A list, a map or a record cannot be written into text as a whole.
- A decimal needs a currency, so `3.5` is an error. It may not have more digits than the currency's minor unit, so `12.505 EUR` is an error too. No silent rounding enters a program, per Q20.
- Money, dates, durations and numbers combine only in the ways the unit table below lists.

**Literals**

| form | example |
|---|---|
| text | `"text"`; escapes `\"`, `\\`, `\n`, `\{`, `\}` |
| int | `42` |
| money | `12.50 EUR`, `0 EUR`, `1200 JPY`; integer minor units, never a float (Q20) |
| date | `2026-10-13`; a date that does not exist is an error, never arithmetic |
| duration | `14 days`, `1 month`, `15 minutes` |
| bool | `true`, `false` |

**Operators**, from loosest to tightest:

| level | operators |
|---|---|
| 1 | `or` |
| 2 | `and` |
| 3 | `not` |
| 4 | `=`, `!=`, `<`, `>`, `<=`, `>=`, `is empty`, `is not empty`, `matches <Record>` |
| 5 | `+`, `-` (spaces on both sides of `-`) |
| 6 | `*`, `/` |
| 7 | `<expr> length` |

**Built-ins**
- `today` and `now` give the current date and the current moment.
- `count of <source>` is the length of a list.
- `count of <rule> <plural> in <source>` is how many items pass the rule.
- `first <var> in <source> where <condition>` is a `maybe` of the item.

**Units are real types**

| expression | type |
|---|---|
| `money + money`, `money - money` | `money`; a currency mismatch between two written literals is an error, otherwise it is checked at run time |
| `money * int`, `money / int` | `money`, rounded half away from zero at the currency's minor unit (Q7) |
| `date + duration`, `date - duration` | `date` |
| `date - date`, `datetime - datetime` | `duration` |
| `12 EUR + 3 days` | error: "Cannot add a duration to money." |
| `money / money`, `money * money`, `text + text` | error; use interpolation for text |

**Negative numbers**
- A `-` directly before the digits makes a negative `int` or `money` literal, such as `-5`, `-12.50 EUR` and `number.round(x, -1)`.
- A `-` directly before a name or a parenthesis negates it and means `x * -1`, as in `-n` and `-(a + b)`.
- Subtraction keeps its spaces, as in `a - b`. `a -1` is not a subtraction, per Q408.
- `agentmark fmt` prints a negation as written, per Q419.

**Watch out:** subtraction needs a space on both sides, because inside an expression `-` is never part of a name.

**Errors**

| code | message | fix |
|---|---|---|
| E203 | a value has the wrong type; the message names the types involved (all 8 messages below the table) | money only combines with money and durations combine with dates and other durations; other E203 messages are in section [10](#10-error-messages) |
| E400 | `A value of type T cannot be written into text as a whole.` | `join` a list of text, write a map entry by entry, or name the record's fields, per Q409 |

**E203 messages**

*Arithmetic and units*

- `Cannot add T to T.`
- `Cannot subtract T from T.`
- `Cannot multiply T by T.`
- `Cannot divide T by T.`
- `Cannot compare T with T.`
- `Cannot add <currency> money and <currency> money.`
- `Cannot subtract <currency> money and <currency> money.`
- `Cannot compare <currency> money and <currency> money.`

**Next:** [6. Types](#6-types)

## 6. Types

```agentmark
# Record: Showcase
- name: text
- visits: int
- price: money
- opened: date
- checked_at: datetime
- grace: duration
- active: bool
- tags: list of text
- nickname: maybe text
- history: list of maybe date

> Describes a showcase in one line.
# Function: describe
- in: s: Showcase
- out: text
- effect: pure
  - return "{s.name} has {s.visits} visits and costs {s.price}"
```

**Rules**
- A record's name is also a type. Record names are unique in a project, per Q31.
- Any value that can be absent must be declared `maybe`. There is no null.
- Where a binding takes `bytes`, a `text` argument is accepted and means its UTF-8 bytes. No other position coerces, per Q151.
- The type of a record literal `{ id: x }` is an object type. It is accepted wherever a record with those fields is.

**Built-in types**

| type | Rust representation |
|---|---|
| `text` | `String` |
| `int` | `i64` |
| `money` | `Money { minor: i64, currency: Currency }` (integer minor units, no floats) |
| `date` | `chrono::NaiveDate` |
| `datetime` | `chrono::DateTime<Utc>` |
| `duration` | `chrono::Duration` |
| `bool` | `bool` |
| `bytes` | `Vec<u8>` (added for bindings, Q8) |
| `list of <T>` | `Vec<T>` |
| `maybe <T>` | `Option<T>` |
| `result of <T>` | `Result<T, AgentError>`, function results only (section [14](#14-errors)) |
| `handle of <crate>.<Type>` | the library's own `<crate>::<Type>` (v0.3, section [15.4](#154-binding-file-format)) |

**Next:** [7. Tool manifest](#7-tool-manifest)

## 7. Tool manifest

Tools are declared in `tools.toml` next to the source file. For a project it sits at the root. This is the manifest every example project uses:

```toml file=examples/01-invoice-chaser/tools.toml
[email]
verbs = ["send"]
targets = ["customer", "address"]
effect = "network"

[crm]
verbs = ["list", "get", "update"]
sources = ["invoices", "leads", "tickets"]
events = ["lead", "ticket"]
effect = "network"

[http]
verbs = ["get", "post"]
effect = "network"

[log]
verbs = []
effect = "local"
```

**Rules**
- An unknown tool, verb, target or source is an error with a suggestion.
- A tool that an agent does not list in `tools:` is an error. See section [2.3](#23-agent).
- `agentmark.toml` is a separate file. It holds `name`, `version`, `targets` and `[lint]` settings, as section [18.1](#181-project-layout) shows, plus `[models]` and `[crates]`, as sections [19](#19-model-providers) and [15.4](#154-binding-file-format) show.
- The generated program defines one trait per tool. The runtime ships `MockTools`, which returns fixture data and records calls.
- You supply the real implementations for production, in `rust/connect.rs`. See section [9.1](#91-generated-rust-project-layout).

**Keys of a tool table**

| key | meaning |
|---|---|
| table name (`[email]`) | the tool's name, as written in `tools:` and in statements |
| `verbs` | the words allowed after the tool name; an empty list means the tool is written `- log: "..."` |
| `targets` | words allowed after the verb (`email send customer`) |
| `sources` | names readable as `tool.source` (`crm.invoices`); each is plural, and its singular names a record (`Invoice`) and is also the target word of `update` (Q5) |
| `events` | names usable in `## When <tool> receives <event>`; each names a record (`lead` is `Lead`) |
| `effect` | `local`, `network` or `model`; `pure` is not allowed for a tool (Q40) |

**Next:** [8. Effects](#8-effects)

## 8. Effects

The report that matters most is the effect summary. It lists, for every block, everything the block can touch. This is the real report for the invoice service, which has no agent and no model call:

```console cwd=repo
$ agentmark check examples/05-invoice-service
Effects (what each block can touch)
  import-csv / Run: local(file, print, state)
  function invoice_fingerprint: pure
  function net_amount: pure
  api / GET /invoices/{id}: pure
  api / GET /invoices/{id}/net: pure
  api / POST /invoices: local(state)

Rust blocks (trusted, not checked by AgentMark)
  fingerprint in invoice_fingerprint: declared pure (examples/05-invoice-service/functions/invoice-fingerprint.agent.md:13)

ok: 0 errors, 0 warnings.
```

**Rules**
- Every statement has an effect class, ordered from least to most reach: `pure`, `local`, `network`, `model`.
- A fifth class, `unverified`, marks calls into a library nobody approved yet. See section [15.3](#153-effects-and-trust).
- `# Rule` bodies and `guard` conditions must be `pure`. A rule that calls a `network` function is an error.
- A function's body must not exceed its declared `effect`. A function that calls an unapproved library cannot claim `pure`, and `check` says so.
- Rust blocks and bindings contribute their declared effect. It is trusted, because the checker cannot look inside Rust.

**Classes**

| class | what it is | where it comes from |
|---|---|---|
| `pure` | computes from values it already has | expressions, rules, guards, reading `state`, `set`, `return`, `respond`, `read body`, `append` to a local list |
| `local` | touches this machine | a `local` tool (`log`), `print`, `read file` (source `file`), `append ... to state.<field>` (source `state`), a function or Rust block declared `local` |
| `network` | talks to another system | a `network` tool, reading a tool source such as `crm.invoices` |
| `model` | asks a model | `ask model` |

**Reading the summary**
- A summary line looks like `unit / heading: model, network(email, crm), local(log)`.
- The class names the highest reach. The brackets name the tools in order of first use.
- Agents, services and commands print `unit / heading:`. Functions and rules print `function name:` and `rule name:`, per Q116.
- The summary is also written into the generated Rust file as `// Effects: ...` at the top.
- `agentmark check --json` prints the same facts as one JSON document for editors and CI. Its layout is in `docs/check-json.md`.

**Next:** [9. Compilation](#9-compilation)

## 9. Compilation

Check that every file is in the canonical style:

```console cwd=repo
$ agentmark fmt --check examples/01-invoice-chaser
5 files are canonical.
```

**Rules**
- The pipeline is `source -> lexer -> parser -> AST -> semantic check -> L2 lowering -> IR -> Rust code`. The IR is independent of the target.
- A new target is one new folder under `src/codegen/`. See `docs/internals.md`.
- Only Rust exists in v0.1. It is `--target rust`, the default.
- A project is a folder, normally with an `agentmark.toml` at its root. A single `.agent.md` file is also a project, with fewer rules, per Q34.
- The generated program calls real systems through the project's own file `rust/connect.rs`. See section [9.1](#91-generated-rust-project-layout).

**The commands**

```text
agentmark check <file|dir> [--json] [--allow-unverified]   validate; print the effect summary, guessed types, unapproved libraries
agentmark fmt   <file|dir> [--check]                       rewrite into the canonical style (section 18.6)
agentmark lint  <file|dir>                                 style rules (section 18.4)
agentmark doc   [<dir|file|crate>] [--project <dir>]       the documentation page of a project, or a library's cheat sheet
agentmark bind  <crate> [--force] [--project <dir>]        create or refresh a library binding (section 15)
agentmark bind  --all                                      refresh every binding of the project
agentmark build <file|dir> --target rust -o <dir>          write a Cargo project (auto-binds imported crates first)
agentmark run   <file|dir> --mock --fixtures <dir>         build, then run once against MockTools and the fixtures
agentmark trace <file|dir> --mock --fixtures <dir>         the same, printing the tool-call trace (JSON lines)
```

`check`, `fmt`, `lint`, `doc` and `bind` read and judge the program. `build`, `run` and `trace` produce a program from it.

**Trace, run and build**

1. Trace a run. `trace` builds the project into a cache folder, runs every entry once and prints the trace on standard output. The folder `fixtures/01-invoice-chaser` holds the test data and the trace this run must produce.

   ```console cwd=repo
   $ agentmark trace examples/01-invoice-chaser --mock --fixtures fixtures/01-invoice-chaser
   {"kind":"model","agent":"invoice-chaser","block":"Every day at 09:00","prompt_prefix":"Write a short, polite payment reminder","context":{"id":"INV-1"}}
   {"kind":"guard","agent":"invoice-chaser","name":"no amounts changed in reminder","result":"pass"}
   {"kind":"guard","agent":"invoice-chaser","name":"reminder length < 800","result":"pass"}
   {"kind":"tool","tool":"email","verb":"send","target":"customer","to":"ap@mueller.de"}
   {"kind":"tool","tool":"crm","verb":"update","source":"invoices","id":"INV-1","set":{"status":"reminded"}}
   {"kind":"tool","tool":"log","message":"Reminded Müller GmbH about 1200.00 EUR"}
   ```

2. Run without the trace. `run` does not print the trace. The built-in `log` tool writes its lines to standard error as `[log] ...`, and `print` writes to standard output unless the trace owns it.

   ```console cwd=repo stderr
   $ agentmark run examples/01-invoice-chaser --mock --fixtures fixtures/01-invoice-chaser
   [log] Reminded Müller GmbH about 1200.00 EUR
   ```

3. Build only. `build` writes the Cargo project to `-o <dir>`, which is required. It then runs `cargo check` on it unless `--no-check` is given.

**Exit codes**

| code | meaning |
|---|---|
| 0 | fine |
| 1 | the project has problems: errors, denied lint rules, files that are not canonical with `--check`, unapproved libraries, a failed bind |
| 2 | the command itself cannot run: bad arguments, a path that does not exist or is not a `.agent.md` file, no model for `bind` |

Diagnostics go to standard error. Reports and counts go to standard output, per Q113.

**Unverified libraries**
- `check` exits 1 while an unverified library is left, unless `--allow-unverified` is given. The listing is printed either way, per Q112.
- `build`, `run` and `trace` apply the same rule: they refuse unapproved libraries.
- With `--allow-unverified` on any of the four commands, an unverified library counts as its `proposed-effect` for the effect checks. A `pure` function that calls a library proposed `pure` then passes.
- Each such library gets one warning, W008, and nothing is approved, per Q203. Without the flag, such a function is an error.

**Automatic binding**
- `check`, `build`, `run` and `trace` bind first. Every `# Import: crate x` that has no binding file gets one before the program is checked, and `binding x: generated` is printed on standard error.
- Existing bindings are used as they are, per Q137 and Q201.
- If binding is impossible, the message is `agentmark: cannot bind "x": <why>` and the program is checked without it. Binding is impossible with no API key, no mock, or no network and no cached metadata.

| code | message | fix |
|---|---|---|
| E219 | `The crate "x" has no binding yet, but this file uses it.` | fix the reason binding failed, then run `agentmark bind x`; the error sits on the import line of a crate that a statement or an agent's `tools:` line needs |

### 9.1 Generated Rust project layout

```text
out/
  Cargo.toml              depends on agentmark-rt (runtime crate) + tokio + chrono + serde
  src/main.rs             CLI: --once, --schedule, --mock, --fixtures <dir>, --trace <file|->
  src/agent_<name>.rs     one file per agent; one async fn per block
  src/records.rs          structs for every # Record, with serde derives
  src/rules.rs            one fn per # Rule (and per # Function)
  src/tools.rs            one trait per tool of tools.toml, a MockTools impl, and `Connected`
  src/bindings.rs         bound library items and `## Rust` blocks
  src/connect.rs          a copy of the project's rust/connect.rs (see below)
```

**Rules**
- Services and commands get a file each: an axum router and a clap subcommand.
- The generated code reads like the source. It has one function per block, the block heading as its doc comment, and a comment `// <file>.agent.md:<line>` before every statement.
- The `Cargo.toml` pins a library to the exact version in its binding header, per Q155.

**Flags of the generated program**

| flag | does |
|---|---|
| `--once` | runs every scheduled entry once, in `# Run` order, then each event entry once per fixture item |
| `--schedule` | runs for real on the schedule |
| `--mock` with `--fixtures <dir>` | pretends the tools and the model |
| `--trace <file\|->` | writes the trace to a file, or to standard output with `-` |

**Real tools: `rust/connect.rs`**
- In a real run, without `--mock`, every tool call goes through `Connected`. The function `connect()` in the user's file `rust/connect.rs` builds it.
- The file sits beside `agentmark.toml`. In a single-file project it sits next to the source file.
- `build`, `run` and `trace` create the file from a stub when it is missing. The stub's methods fail with `not connected yet: implement Crm::list in rust/connect.rs`.
- They never overwrite the file, and they copy it to `src/connect.rs` of the output on every build.
- With `--mock` the file is used if it exists and never created, per Q135.
- `log` is built in.
- The example projects ship the stubs, such as `examples/01-invoice-chaser/rust/connect.rs`. `docs/libraries.md` section 7 shows one.

### 9.2 Runtime crate `agentmark-rt`

`crates/agentmark-rt` is in the same repository. It provides:
- `Money` and `Currency`.
- the scheduler.
- the `ModelClient` trait with an Anthropic implementation, and `MockModel`.
- `MockTools` and the fixture loader.
- the `Trace` recorder, which writes one JSON line per tool or model call.
- `Ctx`, which bundles them.

Money text is `"1200.00 EUR"`. EUR, USD, GBP and CHF have two decimals and JPY has none. Parsing a number with more decimals than the currency has is an error, never silent rounding, per Q14.

**Next:** [10. Error messages](#10-error-messages)

## 10. Error messages

Every error points at a line, says in plain words what was expected and puts a caret under the offending token. It adds a suggestion when a known word is within edit distance 2.

Messages are the exact text check prints; `x`, `N` and `T` stand for the names, numbers and types in your program. `<kind>` stands for a kind of declaration, `<effect>` for an effect class and `<currency>` for a currency code.

```agentmark invalid name=typo.agent.md says=Unknown_target
> Mails a reminder.
# Agent: reminder
- model: claude
- tools: email, log

## Every day at 09:00
- email send custmer: "Please pay."
```

```console stderr
$ agentmark check typo.agent.md
typo.agent.md:7
  - email send custmer: "Please pay."
               ^^^^^^^
  Unknown target "custmer" for tool email. Did you mean "customer"?
```

**Rules**
- The distance is Damerau-Levenshtein, so a swapped pair of letters is one edit. It is at most 2 and less than the number of letters of the typed word.
- A word that already matches exactly is never a suggestion, per Q26.
- The parser suggests only from closed sets of keywords: block headings, statement words, time units and type constructors.
- Tool, verb, target and field typos are the job of the semantic check, per Q27.
- A mistake in AgentMark source never shows a Rust compiler error. If the generated Rust does not build, you see `internal compiler error: generated Rust did not compile` with the generated file, and it is a bug of AgentMark.
- Errors inside your own `## Rust` or binding bodies are mapped back to the `.agent.md` line.
- JSON diagnostics from `check --json` carry a stable `code`.

**Syntax and structure: E0xx**

| code | message | fix |
|---|---|---|
| E001 | `Tabs are not allowed.` or `Tabs are not allowed in AgentMark files.` | use 2 spaces for each indentation level |
| E002 | `Indentation must be 2 spaces per level, but this line is indented N spaces.` or `Headings must start at the beginning of the line.` | indent in steps of 2 spaces; start headings in column 1 |
| E003 | ``This code fence is never closed. Add a line with just ``` after the code.`` | close the fence |
| E004 | `This line is missing the ":" at its end.` or `This line is not part of the language: statements start with "- " and headings with "# " or "## ".` | end a line that owns statements with `:`; put explanations in a `>` comment |
| E010 | `Unknown property "x" for a <kind>.` | use one of the properties the message lists; the last words of the message name the declaration, such as a Type, a Rule, an Agent or a Rust block |
| E011 | `The property "x" is given twice.` | remove one of the lines |
| E012 | a declaration lacks a required line or block; the message names it (all 6 messages below the table) | add the line or block the message names; the first form also exists for a type, a constant, an agent and a binding, each with its own required line |
| E020 | `Unknown declaration "x".` | start with one of the kinds in section [1](#1-file-structure); the message suggests the closest |
| E030 | `This line is not inside a declaration. Start with a heading such as "# Agent: name".` | start with a heading |
| E031 | `This line is indented N spaces, but only N are possible here. Indent by one level (2 spaces) at a time.` | indent one level at a time |
| E032 | a line has the wrong shape for its place; the message shows the right shape (all 23 messages below the table) | follow the shape the message shows |
| E040 | the line is not valid syntax; the message says what was expected (all 5 messages below the table) | use a statement or heading from Appendix A; the message suggests the closest |
| E050 | `A glob import cannot be renamed with "as".` or `"x" is not a .agent.md file. To import a folder, give it a name: "# Import: x as <name>".` | name a folder import with `as <name>` and do not rename a glob |

**E012 messages**

- `The rule "x" has no "when" line.`
- `The function "x" does not say what effect it has.`
- `The command "x" has no "## Run" block.`
- `The Rust block "x" does not declare its effect.`
- `The tool "x" does not say what effect it has.`
- `The tool "x" has no verbs.`

**E032 messages**

- `An import is a single line and has nothing under it.`
- `A Record has no "##" blocks.`
- `A property starts with "- ", like "- model: claude".`
- `A field starts with "- ", like "- amount: money".`
- `A variant starts with "- ", like "- Circle: radius: int" or "- Empty".`
- `A "# Run" entry starts with "- " and names an agent, service or command.`
- `Nothing can be indented under "x:".`
- `Nothing can be indented under a field.`
- `Nothing can be indented under a variant.`
- `Nothing can be indented under this statement.`
- `Only "otherwise:" can be indented under a "try" line.`
- `Statements go under the "effect" line, not under "x".`
- `A Function has no blocks except "## Rust: name".`
- `A Command has only one "## Run" block.`
- `A handle block is written "## Handle: <crate>.<Type>", for example "## Handle: sqlx.Query".`
- `` "x" needs its Rust code in a fenced block (```rust ... ```). ``
- `A "match" needs branches indented under it, like "- Circle c:" or "- otherwise:".`
- `A branch of "match" starts with "- ", like "- Circle c:".`
- `A "match" has only one "otherwise:" branch.`
- `"otherwise:" must be the last branch of a "match".`
- `"else:" must come directly after an "if" at the same indentation.`
- `"otherwise:" must be indented under a "try" line.`
- `This is not a statement.`

**E040 messages**

- `Unknown statement "x".`
- `Unknown block heading "x".`
- `Unknown word "x" after "ask". The statement is "ask model: ...".`
- `Expected T here, but found T.`
- `This line is missing the ":" at its end.`

**Project files: E1xx**

| code | message | fix |
|---|---|---|
| E100 | `This is not valid TOML: <reason>.` | each tool is a table, a `[name]` line followed by `key = value` lines |
| E101 | `Unknown key "x" for tool "x".` | use one of the keys the message lists |
| E102 | `The tool "x" does not say what effect it has.` | add `effect = "local"` or `effect = "network"` |
| E103 | `"x" is not an effect class for a tool.` | use `local`, `network` or `model` |
| E104 | `"x" of tool "x" must be a list of words.` | write it like `verbs = ["one", "two"]` |
| E105 | `"x" must be a tool table, written as [x].` | every top-level entry of `tools.toml` is one tool |
| E110 | `Cannot find the project "x".` or `Cannot read the file "x".` | check the path |
| E111 | `Cannot find the file "x".` or `Cannot find the folder "x".` | fix the path of the `# Import` line |
| E112 | `Import cycle: a -> b -> a.` | move what both files need into a third file that imports neither |
| E113 | `The pattern "x" matches no files.` | fix the pattern |
| E114 | `agentmark.toml` is not valid; the message names the key (all 9 messages below the table) | fix the key the message names |

**E114 messages**

- `This is not valid TOML: <reason>.`
- `"x" in agentmark.toml must be text.`
- `"targets" in agentmark.toml must be a list of words, like ["rust"].`
- `"deny" under [lint] must be a list of rule names.`
- `"lsp" in agentmark.toml must be a table, like [lsp] model_actions = true.`
- `"rust" in agentmark.toml must be a table with [rust.dependencies].`
- `"crates" in agentmark.toml must be a table, like [crates] sqlx = { version = "0.8", features = ["sqlite"] }.`
- `"models" in agentmark.toml must be a table, like [models] test = "mock".`
- `Unknown key "x" in agentmark.toml.`

**Meaning: E2xx**

| code | message | fix |
|---|---|---|
| E200 | `The record "x" is already declared in file:line.` | names are unique across the project, so rename one of them; the first word of the message names the kind of declaration |
| E201 | `Unknown type "x".` | declare it or import its file |
| E202 | a name, field, rule or record cannot be found (all 7 messages below the table) | declare the record or import the file that does; names come from `set` and `->` lines above, loop variables, constants and imported declarations |
| E203 | a value has the wrong type; the message names the types involved (all 33 messages below the table) | use a value of the type the message names |
| E204 | a value that can be empty is used as a plain value, or a plain value is unwrapped (all 4 messages below the table) | check it first with `if <value> is some x:` or `guard: <value> is not empty` |
| E205 | `A rule must be pure, but "x" has the effect <effect>.` | move the effectful call out; in a guard, a constant or a collection word the message starts `A guard must be pure`, `A constant must be pure` or `An expression inside a collection word must be pure` |
| E206 | `The function "x" says "- effect: pure", but this line has the effect <effect>.` | raise the declared `effect`, or move the line out; the message names a function, a tool or a Rust block |
| E207 | a tool, source, event, verb, target or handle method is unknown or used wrongly (all 16 messages below the table) | use a name from `tools.toml`; the message suggests the closest |
| E208 | `The tool "x" is not listed in the tools of agent "x".` | add it to the `- tools:` line |
| E209 | a function or action is unknown, called the wrong way, or given the wrong number of values (all 9 messages below the table) | `do` calls a `## Do` block or a visible function; use `try` for a `result of T` |
| E210 | a statement is in a place where it does not work (all 5 messages below the table) | use `return` in a function and `respond` in a route |
| E211 | a hard limit of section [18.3](#183-hard-limits) is broken (all 8 messages below the table) | split the file or the block; see section [18.3](#183-hard-limits) |
| E212 | `Two statements of this "parallel" block both write "x" (the first is on line N).` or `Two statements of this "parallel" block both use the handle "x" (the first is on line N).` | give each its own variable, or run them one after the other |
| E213 | `The block "x" takes no values.` or `The block "x" returns nothing, so there is no result to store.` or `The block "x" works on T, but here it is called with T.` | call a `## Do` block from places that have the same record in front of them |
| E214 | `Unknown agent, service or command "x".` | import it into this file first |
| E215 | `Unknown tool "x".` | declare the tool in `tools.toml` |
| E216 | `The constant "x" is defined in terms of itself.` | break the loop |
| E217 | `The record "x" has the field "x" twice.` | remove one |
| E218 | `Only one service may have a "state:" ("x" in file:N has one), because commands use it.` | keep the state in one service, or pass data through a tool |
| E219 | `The crate "x" has no binding yet, but this file uses it.` | run `agentmark bind x` |
| E220 | `Could not bind "x" of x on demand: <why>` | use a `## Rust` block, or bind it by hand; see section [15.2](#152-the-binder) |
| E221 | the model driver is unknown, unsupported or incomplete (all 4 messages below the table) | use a driver of section [19](#19-model-providers) |

**E202 messages**

- `"x" holds T, but no record "x" is visible in this file.`
- `The event "x" arrives as a record, but no record "x" is visible in this file.`
- `Unknown field "x" on T.`
- `Unknown rule "x".`
- `Unknown name "x".`
- `The variant "x" has no field "x".`
- `Unknown field "x" on this record.`

**E203 messages**

*Arithmetic and units*

- `Cannot add T to T.`
- `Cannot subtract T from T.`
- `Cannot multiply T by T.`
- `Cannot divide T by T.`
- `Cannot compare T with T.`
- `Cannot add <currency> money and <currency> money.`
- `Cannot subtract <currency> money and <currency> money.`
- `Cannot compare <currency> money and <currency> money.`

*Tools and bindings*

- `"x.y" is a whole list; loop over it to read the fields of its items.`
- `A target value must be text, but this is T.`
- `The field "x" of T is T, but this is T.`
- `The tool x takes a T here, but this is T.`
- `The tool x takes text here, but this is T.`
- `"x" of "x y" must be T, but this is T.`

*Fields*

- `"x" is a T, which has no fields of its own, so ".y" cannot be read.`
- `"x" is T, which has no fields, so ".y" cannot be read.`

*Values, assignments and returns*

- `The prompt must be text, but this is T.`
- `"as" needs a record the model's answer is checked against, but "x" is T.`
- `"x" holds T, but this is T.`
- `A file path must be text, but this is T.`
- `This list holds T, but this is T.`
- `"append" needs a list, but this is T.`
- `This returns T, but the function says it returns "T".`

*Conditions and checks*

- `This is T, which can never be empty.`
- `"matches" needs a record, but "x" is T.`
- `Only text and lists have a length, but this is T.`
- `"no amounts changed" checks text the model wrote, but this is T.`
- `"x" needs a yes/no condition, but this is T.`
- `"x" needs a list, but this is T.`
- `The rule "x" is about T, but the list holds T.`
- `"x" compares two values of the same kind, but the first is T and this is T.`
- `"x" needs a whole number, but this is T.`
- `The text between the parts must be text, but this is T.`

**E204 messages**

- `This is T, which is never empty, so "is some" is not needed.`
- `Declare the result as "maybe T" and check it with "is some" or "is not empty" before using it.`
- `This value can be empty (T), so it cannot be used yet.`
- `"x" can be empty here, so "y" cannot be read from it yet.`

**E207 messages**

*Tools*

- `Unknown tool "x".`
- `"x" is a tool, not a value.`
- `Unknown source "x" for tool x.`
- `Unknown event "x" for tool x.`
- `The tool x has no verbs, but "x" was given.`
- `Unknown verb "x" for tool x.`
- `The tool x needs a verb.`
- `The tool x has no targets, so "x" is not expected here.`
- `Unknown target "x" for tool x.`
- `Which x? x, x are both in scope.`
- `"x" needs a record in front of this block with a "x_email" or "x" field.`
- `"x y" takes N values (a, b), but N were given.`

*Bindings and handles*

- `The binding x needs a verb.`
- `The handle "x" (T) has no method "x".`
- `Unknown verb "x" for binding x.`
- `"x" is a handle, so the line needs a method after it.`

**E209 messages**

*Functions and actions*

- `Unknown action or function "x".`
- `Unknown function "x".`
- `The function "x" can fail (it returns "T"), so it has to be called with "try".`
- `The function "x" can fail (it returns "result of T"), so it cannot be used inside an expression.`
- `The function "x" returns nothing, so there is no result to store.`
- `The function "x" returns nothing, so it cannot be used as a value.`
- `"x" takes N values (a, b), but N were given.`

*Verbs of user-defined tools*

- `The verb "x" of "x" can fail (it returns "T"), so it has to be called with "try".`
- `The verb "x" can fail (it returns "T"), so it cannot be used inside an expression.`

**E210 messages**

- `"stop" ends a block, but this is a function.`
- `"respond" only works inside a service route.`
- `"read body" only works inside a service route.`
- `"return" only works inside a function.`
- `The function "x" has no "- out:", so it cannot return a value.`

**E211 messages**

- `This line is N characters long; the limit is N.`
- `A file may hold only one agent, service or command, but the command "x" comes after the agent "x".`
- `Several records in one file are only allowed in a "records/" folder.`
- `Several rules in one file are only allowed in a "rules/" folder.`
- `Records and rules cannot share a file.`
- `The block "x" has no statements.`
- `The block "x" has N statements; the limit is N.`
- `Statements are nested N levels deep; the limit is N.`

**E221 messages**

- `Unknown model provider "x".`
- `The model provider "x" is not supported in this version.`
- `"x:" needs a model name, like "x:model".`
- `The model is empty.`

**Warnings**

| code | message | fix |
|---|---|---|
| W001 | `The field "x" has no type and its name gives no hint, so it is inferred as T.` | write the type |
| W002 | `Unused import: nothing from "x" is used in this file.` or `Unused import: the crate "x" is not used in this file.` | remove the import line |
| W003 | `Unused record: nothing refers to "x".` or `Unused action: no block calls "do x".` | remove it or use it; the first word of the message names the kind of declaration |
| W004 | `The crate "x" has no binding yet.` | run `agentmark bind x`; `## Rust` blocks can already use the crate |
| W005 | `The binding "x" is unverified: nobody approved what it may touch.` | approve the binding's `- effect:` line, as section [15.3](#153-effects-and-trust) shows |
| W006 | `This file holds N runnable declarations (agent, service or command): it is a single-file draft.` | split it into a project folder with one declaration per file |
| W007 | `This line is N characters long; only the text of its strings makes it longer than N.` | move the long text into a `# Constant` and use its name |
| W008 | `The binding "x" is unverified: its effect is assumed to be "x" from the proposal (--allow-unverified).` | approve the binding |
| W300 | `Every case is handled above, so this "otherwise:" can never run.` | remove the branch |

**Where the other codes are**
- E360 to E367 are about handles. See the table in section [15.4](#154-binding-file-format).
- E320 to E329 are about collection words. See section [22.2](#222-collection-words).
- E340 to E347 are about the built-in library and the `file` tool. See section [22.3](#223-the-built-in-library).
- E380 to E387 are about user-defined tools. See section [22.5](#225-user-defined-tools).
- E400 is about text holes. See section [5](#5-expressions).

**Next:** [11. Testing contract](#11-testing-contract)

## 11. Testing contract

Every program has a folder of fixtures that makes its behaviour reproducible without any real system. This is the fixture folder of the invoice chaser:

```text
fixtures/01-invoice-chaser/
  crm.invoices.json        input data of the tool source crm.invoices
  model.json               canned model answers, plus what "today" is
  expected.trace.jsonl     the exact sequence of tool and model calls the agent must make
```

**Rules**
- The input is one file per tool source, named `<tool>.<source>.json`, such as `crm.invoices.json`.
- The same file also feeds events. Each item of `crm.leads.json` fires `## When crm receives lead` once, per Q154.
- The model file answers `ask model`. The longest matching prompt prefix wins.
- A service folder adds `requests.json`, a list of `{method, path, body?, expect_body?}`.
- A command adds `command.json`, which holds `{command, args}`.
- `state.<field>.json` seeds the application state, per Q153.
- The program runs with the fixture folder as its working directory.

**The model file**

```json file=fixtures/02-lead-qualifier/model.json
{
  "today": "2026-10-13",
  "responses": [
    {"prompt_prefix": "Score this inbound lead from 1 to 10", "response": "{\"score\": 9, \"reason\": \"Budget and timeline are stated.\"}"}
  ]
}
```

**The expected trace**

The expected trace of the invoice service shows all the kinds of line that a service and a command write:

```json file=fixtures/05-invoice-service/expected.trace.jsonl
{"kind":"guard","agent":"api","name":"found is not empty","result":"pass"}
{"kind":"http","method":"GET","path":"/invoices/INV-1","status":200}
{"kind":"guard","agent":"api","name":"found is not empty","result":"fail"}
{"kind":"http","method":"GET","path":"/invoices/INV-404","status":404}
{"kind":"guard","agent":"api","name":"found is not empty","result":"pass"}
{"kind":"http","method":"GET","path":"/invoices/INV-1/net","status":200}
{"kind":"guard","agent":"api","name":"inv.amount > 0 EUR","result":"pass"}
{"kind":"http","method":"POST","path":"/invoices","status":201}
{"kind":"guard","agent":"api","name":"inv.amount > 0 EUR","result":"fail"}
{"kind":"http","method":"POST","path":"/invoices","status":400}
{"kind":"print","message":"Imported 3 rows"}
```

The trace is JSON lines. Tests compare the parsed objects line by line, never generated source text.

| `kind` | fields |
|---|---|
| `model` | `agent`, `block`, `prompt_prefix` (the fixture key that matched), `context` (only with `ask model with v`: `{"id": v.id}`) |
| `guard` | `agent`, `name` (the guard's source text, constants inlined), `result` (`"pass"` or `"fail"`) |
| `tool`, email | `tool`, `verb`, `target`, `to` |
| `tool`, crm | `tool`, `verb`, `source`, `id`, and for `update` also `set` (the assigned fields, or the whole record) |
| `tool`, http | `tool`, `verb`, `url`, `body` (when there is one) |
| `tool`, log | `tool`, `message` |
| `http` | `method`, `path` (the concrete path, not the route template), `status` |
| `print` | `message` |
| `error` | `agent`, `block`, `message` (a `try` without `otherwise` that failed) |

**Reading a trace**
- `agent` is the name of the agent, service or command.
- `block` is the heading of the entry that started the run. A `## Do` block called from `## Every day at 09:00` reports `Every day at 09:00`.
- Reading a source with `for each` is not traced.

**Variants**
- One mock response exists per prompt, so one run cannot take both branches of an `if` on a model answer.
- The fixture folder covers the main path. Every other branch lives in `variants/<case>/` with the same set of files, and a test harness runs each as its own fixture folder, per Q150.
- `cargo test` builds every example, runs it with `--mock --fixtures` and compares the trace.

**Next:** [13. Core language: functions and modules](#13-core-language-functions-and-modules-layer-2)

## 13. Core language: functions and modules (layer 2)

Layer 2 has nothing to do with agents. It holds functions, modules, services and commands.

### 13.1 Function

```agentmark
# Record: Invoice
- id: text
- amount: money

> The invoice amount after taking off VAT.
# Function: net_amount
- in: invoice: Invoice, vat: int
- out: money
- effect: pure
  - return invoice.amount - invoice.amount * vat / 100

> Logs the net amount of every invoice, calling the function two ways.
# Agent: calc
- model: claude
- tools: crm, log

## Every day at 09:00
- for each invoice in crm.invoices:
  - do net_amount(invoice, 19) -> n
  - log: "{n} or {net_amount(invoice, 19)}"
```

**Rules**
- A function is called in an expression, as in `net_amount(invoice, 19)`, or as a statement, as in `- do net_amount(invoice, 19) -> n`.
- `return <expr>` ends the function with a value.
- A function with `out:` and no `return` is an error. `return` outside a function is an error, and so is `stop` inside one, per Q40.
- Recursion is allowed. Closures are not allowed in v0.1.
- Declared names are unique per project, whatever file they sit in, per Q31.

**Lines of a function**

| line | meaning |
|---|---|
| `- in: name: Type, other: Type` | parameters (optional) |
| `- out: Type` | the result type (optional for a function that returns nothing) |
| `- effect: pure` | **mandatory**: `pure`, `local`, `network` or `model`. The checker verifies the body does not exceed it |
| indented lines | the body, under the `effect` line (Q16) |

| code | message | fix |
|---|---|---|
| E210 | a statement is in a place where it does not work (all 5 messages below the table) | use `return <value>` to leave a function; routes answer with `respond` |

**E210 messages**

- `"stop" ends a block, but this is a function.`
- `"respond" only works inside a service route.`
- `"read body" only works inside a service route.`
- `"return" only works inside a function.`
- `The function "x" has no "- out:", so it cannot return a value.`

### 13.2 Module and import

```agentmark fragment
# Import: ./billing/rules.agent.md as billing
# Import: ./records/invoice.agent.md
# Import: ./rules/*.agent.md
# Import: ./billing as billing
# Import: crate serde_json
# Import: crate reqwest as http_client
```

**Rules**
- One file is one module. Files in subfolders are nested modules.
- A file sees only what it imports.
- A name from an aliased import is written `billing.overdue(invoice)`. The checker looks up the full dotted name, per Q37.
- A file that needs a record from an aliased folder also imports the record file plainly. The parser accepts qualified names in expressions and calls but not in type positions.
- Crate imports make the library available to `## Rust` blocks and to bindings. See section [15](#15-library-access-automatic-bindings).
- Import cycles are an error, and the cycle is printed. Unused imports and unused declarations are warnings.
- A crate import without a binding file is warning W004 and never an error, per Q36. The warning says to run `agentmark bind sha2`.

### 13.3 Service and Command

Two more top-level declarations turn AgentMark into a general application language. This single file is a complete program with a service and a command:

````agentmark
> Non-agent example: an HTTP service plus a CLI command, using a crate binding.
> Proves that Layer 2 + Layer 1 can build an ordinary Rust application.

# Import: crate sha2

# Record: Invoice
- id: text
- customer: text
- amount: money
- due: date
- status: text

# Record: AppState
- invoices: list of Invoice

# Function: net_amount
- in: invoice: Invoice, vat: int
- out: money
- effect: pure
  - return invoice.amount - invoice.amount * vat / 100

## Rust: fingerprint -> text
- in: invoice: Invoice
- effect: pure
  ```rust
  use sha2::{Digest, Sha256};
  format!("{:x}", Sha256::digest(format!("{}{}", invoice.id, invoice.amount).as_bytes()))
  ```

# Service: api
- listen: 0.0.0.0:8080
- state: AppState

## GET /invoices/{id}
- set found = first invoice in state.invoices where invoice.id = id
- guard: found is not empty else respond 404
- respond 200 found

## GET /invoices/{id}/net
- set found = first invoice in state.invoices where invoice.id = id
- guard: found is not empty else respond 404
- respond 200 { id: found.id, net: net_amount(found, 19), fingerprint: fingerprint(found) }

## POST /invoices
- read body as Invoice -> inv
- guard: inv.amount > 0 EUR else respond 400
- append inv to state.invoices
- respond 201 inv

# Command: import-csv
- args: path: text, dry_run: bool = false

## Run
- try read file path as list of Invoice -> rows
  otherwise:
    - print: "Could not read {path}"
    - stop
- for each row in rows:
  - if not dry_run:
    - append row to state.invoices
- print: "Imported {count of rows} rows"
````

**Rules**
- No model is involved unless the author writes `ask model`.
- A **service** has `listen:` for the address and port and `state:` for the record that holds its memory.
- Each `## METHOD /path` block is a route. `GET`, `POST`, `PUT`, `PATCH` and `DELETE` exist.
- Each `{name}` in a path becomes a text variable of the block.
- A service lowers to an axum router over shared state.
- A **command** has `args:` and exactly one `## Run` block. Each argument is `name: Type` with an optional default `= value`.
- A command lowers to a clap subcommand.
- A command may use the `state` record of the project's single service. A second service with `state:` is error E218, per Q9 and Q41.

**Statements for services and commands**

```agentmark fragment
## POST /invoices
- read body as Invoice -> inv
- guard: inv.amount > 0 EUR else respond 400
- append inv to state.invoices
- respond 201 inv
- respond 204
```

```agentmark fragment
## Run
- read file path as list of Invoice -> rows
- print: "Imported {count of rows} rows"
```

| statement | meaning |
|---|---|
| `respond <status>` / `respond <status> <expr>` | end the route with that HTTP status and an optional JSON body |
| `read body as <Type> -> var` | parse the request body as that type |
| `read file <path> as <Type> -> var` | read a file into that type (the example reads a CSV file into a `list of Invoice`); effect `local(file)` |
| `append <value> to <list>` | add to a list, usually `state.<field>`; on `state` it is `local(state)` |
| `print: "text"` | write a line to the output; effect `local(print)` |
| `return <expr>` | the value of a function |

Reading `state` is pure. In a mock run the state is seeded from `fixtures/<name>/state.<field>.json` when present.

**Next:** [14. Errors](#14-errors)

## 14. Errors

A statement that can fail is written with `try`. This agent falls back to a fixed text when a web call fails:

```agentmark
# Record: Invoice
- id: text
- amount: money

> Reads rates from the web, falls back to a fixed text when that fails.
# Agent: rates
- model: claude
- tools: http, log

## Every day at 06:00
- try http get "https://api.example.com/rates" -> rates
  otherwise:
    - log: "Rates unavailable, using cached"
    - set rates = "cached"
- log: "Rates: {rates}"
```

**Rules**
- There is no exception mechanism.
- Without `otherwise`, a failed `try` stops the current block and records the error in the trace as an `error` line.
- With `otherwise:`, the indented lines run instead. `otherwise:` is the only child of its `try` line, per Q18.
- The variable that `try ... -> x` introduces is visible after the `try`.
- When `otherwise` neither assigns it nor stops, the type's default value is used, per Q94.

**Functions that can fail**

A function whose `out` is `result of <T>` returns `Result<T, AgentError>` in Rust. Its callers must use `try`:

```agentmark
# Function: half
- in: n: int
- out: result of int
- effect: pure
  - return n / 2

> Halves a number, or uses 0 when the function fails.
# Function: half_or_zero
- in: n: int
- out: int
- effect: pure
  - try do half(n) -> h
    otherwise:
      - set h = 0
  - return h
```

- Inside such a function, `return v` takes a plain `T`, which is wrapped in `Ok`.
- A `try` without `otherwise` that fails does not trace and stop there, as it does everywhere else. The error is handed to the caller.
- The caller writes `try do f(x) -> h`. Afterwards `h` is a plain `T`, also when `otherwise` assigns it, per Q200.
- Calling such a function without `try`, or inside an expression, is an error.
- A failure has exactly one source, a failing `try`.

**Values that can be absent**

A `maybe` value is unwrapped with `is some`:

```agentmark
# Record: Lead
- id: text
- score: maybe int

> Logs the score of every new lead, or says it has none.
# Agent: scorer
- model: claude
- tools: crm, log

## When crm receives lead
- if lead.score is some s:
  - log: "scored {s}"
- else:
  - log: "unscored"
```

- Outside an `is some`, an `is not empty` or a `guard`, using a `maybe` value where a plain one is needed is an error.
- After `guard: x is not empty`, `x` counts as present for the rest of the block.

**Errors**

| code | message | fix |
|---|---|---|
| E204 | a value that can be empty is used as a plain value, or a plain value is unwrapped (all 4 messages below the table) | check it first with `if <value> is some x:` or `guard: <value> is not empty` |
| E209 | `The function "x" can fail (it returns "T"), so it has to be called with "try".` or `The function "x" can fail (it returns "result of T"), so it cannot be used inside an expression.` | write `- try x(...) -> value`, optionally with an `otherwise:` branch; the other E209 messages are in section [10](#10-error-messages) |

**E204 messages**

- `This is T, which is never empty, so "is some" is not needed.`
- `Declare the result as "maybe T" and check it with "is some" or "is not empty" before using it.`
- `This value can be empty (T), so it cannot be used yet.`
- `"x" can be empty here, so "y" cannot be read from it yet.`

**Next:** [15. Library access](#15-library-access-automatic-bindings)

## 15. Library access: automatic bindings

Using a library must never require writing Rust. You write `# Import: crate printpdf`, and AgentMark generates and caches the **binding** itself. This binding file is written in the same format as the generated ones:

````agentmark name=bindings/printpdf.agent.md
> Invoice titles as PDF files.
# Binding: pdf
- crate: printpdf
- effect: local

> Renders a title into a one-page PDF.
## render(title: text) -> bytes
  ```rust
  Ok(printpdf_render(&title))
  ```

## Not bound
- save_to_file: takes a writer (printpdf::PdfDocument::save)
````

A statement then reads `- pdf render title -> bytes`: the binding name, the item name, the arguments, the result. With the binding file above in place, the call is an ordinary statement:

```agentmark
# Import: crate printpdf

# Record: Invoice
- id: text
- amount: money

> Renders a PDF title for every invoice.
# Agent: printer
- model: claude
- tools: crm, log

## Every day at 09:00
- for each invoice in crm.invoices:
  - pdf render "{invoice.id}" -> document
  - log: "rendered {invoice.id}"
```

**Rules**
- A binding is an ordinary layer-2 file. It is reviewable, diffable and editable by hand.
- The form of a call is `- <binding> <item> <arguments> -> var`.
- The effect of the call is the binding's `- effect:`. Here it is `local(pdf)`.

### 15.1 Flow

```text
# Import: crate printpdf    -> is bindings/printpdf.agent.md there and current?
                               no  -> run the binder, write the file, continue
                               yes -> use it
```

**Rules**
- Bindings live in `bindings/<crate>.agent.md`, or in `.agentmark/bindings/`.
- `agentmark bind <crate>` runs the binder explicitly. `agentmark bind --all` refreshes every binding.
- `check`, `build`, `run` and `trace` run the binder for a crate that has no binding file yet. So the workflow is `# Import`, `check`, approve one line.
- `docs/libraries.md` walks through it with real output.

### 15.2 The binder

A statement against an item that the binding lists under `## Not bound` makes the binder write that item on demand:

```text
- sha2 sha224_digest payload -> digest
```

**Rules**
- The binder is target-agnostic by design. `src/binder/rust/` reads the library's metadata, which is rustdoc JSON for Rust. A TypeScript target would read `.d.ts` files.
- `src/binder/core/` turns the metadata into AgentMark declarations.
- The first three tiers apply, in order, to every public function and method. The fourth runs on demand.
- A public type that AgentMark has no type for is a handle, as section [15.4](#154-binding-file-format) shows. Its constructors and methods are bound too, by rules and confirmed by the type checker. So `&mut self` receivers are no longer a reason to skip.
- The binder never silently drops an item. Everything is bound or listed as not bound.
- The edge cases are decisions Q70 to Q82. For example, closures stay skipped because AgentMark has no function type, per Q75.

**The tiers**

| tier | condition | result |
|---|---|---|
| direct | every parameter and the return type map 1:1 onto AgentMark types, no generics, no closures | binding written by rules, no model call |
| assisted | generics with one obvious instantiation, `Result`/`Option` wrapping, `async` | the binder asks the model for the Rust body, then type-checks it against the declared signature before accepting it |
| skipped | traits with associated types, lifetimes in the public API, `unsafe`, closures, types AgentMark cannot hold, anything the assisted tier fails to type-check | listed under `## Not bound` with the reason |
| on-demand | the user writes a plain statement against an item listed under `## Not bound` | the binder writes the body from the statement and the rustdoc, saves it as `unverified`, and continues |

**The on-demand tier**
- `check`, `build`, `run` and `trace` look for statements of the form `<library> <item> value ...` whose item the library's binding lists under `## Not bound`. Only a statement in a file that imports the library counts, such as `# Import: crate sha2`.
- The binder asks the model for the body of each such item, once however many statements use it. The model is the same `AGENTMARK_MODEL`, and it sees the statement as written and the item's rustdoc.
- When the item has no AgentMark signature, because it was skipped and not assisted, the model proposes one. Its parameter count must match the values in the statement.
- The body is type-checked with the same checker as the assisted tier.
- Two attempts are made, and the second is told what was wrong with the first. Each is one line on standard error: `binding sha2: writing sha224_digest on demand… done` or `… failed`, with the second try marked `(try 2 of 2)`.
- When every item succeeds, the binding file is rewritten once. The items move out of `## Not bound` into bound items, and their `>` line ends in "Written on demand; not reviewed yet."
- `- effect:` becomes `unverified`, and `- proposed-effect:` never goes down. The previous file is kept as `.prev`, and the build continues under the rules of section [15.3](#153-effects-and-trust).
- Several libraries are handled together. No binding file is changed unless every library succeeded.
- A hand-written binding has no generated header. It is bound against the version in `Cargo.lock` next to `bindings/`, else against the rustdoc JSON supplied. If neither is known, the error says so and never guesses the newest.
- If the tier cannot start at all, because there is no model or no rustdoc, the command prints `agentmark: cannot write the library code for "sha2" on demand: <why>` and stops with exit code 1. `check` goes on to report the unknown item.
- Q230 to Q235 record the edge cases.

**When an item fails twice**

The binding file is not touched, nothing is built, and the error is E220 at the item's name in the author's statement:

```text
fingerprint.agent.md:8
  - sha2 sha224_digest payload -> digest
         ^^^^^^^^^^^^^
  Could not bind "sha224_digest" of sha2 on demand: the code the model wrote did not pass the type check, twice.
  Nothing was changed. Use a ## Rust block for "sha224_digest", or bind it by hand in bindings/sha2.agent.md. Details for a bug report: target/agentmark/ondemand-sha2-sha224_digest.log
```

| code | message | fix |
|---|---|---|
| E220 | `Could not bind "x" of x on demand: the code the model wrote did not pass the type check, twice.` or `Could not bind "x" of x on demand: the model could not be reached: <reason>.` or `Could not bind "x" of x on demand: the signature the model chose does not fit your statement (1 value, 2 parameters).` | use a `## Rust` block for the item, or bind it by hand in `bindings/sha2.agent.md`; the reason of the second form is an API error, a timeout or no answer, so run again when the model can be reached |

**Facade crates and long items**
- A facade crate such as `sqlx` mostly re-exports other crates, as in `pub use sqlx_core::query::query`. The binder builds their rustdoc JSON too and merges it into the facade's, so what the facade exports is bound under the facade's name.
- A generic type with a concrete alias, such as `SqlitePool = Pool<Sqlite>`, is bound through the alias. The type-erased `Any` driver is never the one instantiation, per Q411 and Q412.
- A binding item whose heading would pass 120 characters is written `## name`, followed by `- in: a: T, b: U` lines and `- out: T`. These are the words of a `# Function`, with one parameter per line when they do not fit together.
- An item that does not fit even so stays under `## Not bound`, per Q416.

### 15.3 Effects and trust

A generated binding is born unverified, with the binder's guess beside it:

```text
- effect: unverified
- proposed-effect: network
```

**Rules**
- The guess comes from heuristics. `std::fs` means `local`, `reqwest` or `tokio::net` means `network`, and anything else means `pure`, per Q81.
- `agentmark check` lists every unverified binding and refuses to build a release while any remain.
- `--allow-unverified` overrides this for development. The library then counts as its `proposed-effect` for effect checks, with warning W008, per Q203.
- A person approves by editing that one line to `- effect: pure`, `local` or `network`. This is needed once per crate version.
- A binding approved for version X is marked `unverified` again when the crate version changes.

### 15.4 Binding file format

A binding file starts with a generated header that lets the binder see staleness:

```text
> generated by agentmark bind — printpdf 0.7.0 — 2026-10-13
> edit freely; re-run `agentmark bind printpdf --force` to regenerate
```

**Rules**
- A binding is a layer-2 declaration whose bodies are layer 1 Rust.
- Every body is the body of `pub async fn <item>(<params>) -> Result<<ret>, Box<dyn std::error::Error + Send + Sync>>`.
- Values arrive owned. The body ends in `Ok(value)`, may use `?` and `.await`, and names the library by full path, per Q70.
- Above every bound item the binder writes one `>` line, which is the first sentence of the library's own documentation for it, per Q80.
**Opaque handles**

A library type that AgentMark has no equivalent for, such as a hasher, a client or a response, is a **handle**. It is written `handle of <crate>.<Type>`, as v0.3 section 22.4 describes. The binder declares it where it appears in a signature and binds the constructors and the methods around it, instead of skipping them, per Q360 to Q369.

The file shows how a call holds the handle:

```text
## sha256_new() -> handle of sha2.Sha256
## update(self: mut handle of sha2.Sha256, data: bytes)
## chain_update(self: consume handle of sha2.Sha256, data: bytes) -> handle of sha2.Sha256
## finalize(self: consume handle of sha2.Sha256) -> bytes
```

| written | Rust | the call |
|---|---|---|
| `handle of …` (parameter) | `&T` | reads it |
| `mut handle of …` | `&mut T` | changes it |
| `consume handle of …` | `T` | uses it up |
| `handle of …` (result) | `T` | hands you a new one |

A parameter named `self` comes first and makes the item a **method** of that handle type. Its Rust name in the body is `self_`, because `self` is a keyword. A method is written with its own name, such as `update`, because the handle type tells methods apart.

A constructor is an ordinary item such as `sha256_new`. It is called on the library, as in `- sha2 sha256_new -> hasher`. A method is called on the variable:

```text
- sha2 sha256_new -> hasher
- hasher update "abc"
- hasher update "def"
- hasher finalize -> digest
```

**Handle rules**

All of these are compile errors, in the shape of section [10](#10-error-messages).
- A handle **moves**. A `consume` method uses it up. So does passing it to a function that uses it up or hands it back, as in `- do feed(hasher)`, and so does `set other = hasher`.
- A function that only calls borrowing methods on a handle it is given only lends it, and the caller keeps it, per Q414.
- A branch that only sometimes uses a handle up counts as "may have". A handle that lives outside a loop cannot be used up inside it.
- A handle can be read only where something takes it: a method call, a function or binding argument, `set` or `return`.
- A handle is a single value. There is no `list of handle …`, `maybe handle …` or record field.
- `try … -> h otherwise:` needs the `otherwise` branch to leave, since there is no handle to put in its place.
- A function parameter or result of handle type owns the handle. The caller's variable is used up when it is passed.
- `mut` and `consume` belong to binding parameters only.
- A library method that is `async` is awaited by the generated code, because every binding body is the body of an `async fn`.
- The effect of a method call is the binding's, like any other binding call. See section [15.3](#153-effects-and-trust).

| code | message | fix |
|---|---|---|
| E360 | `A handle type is written with its crate, but "x" has none.` or `No binding declares the handle type "x".` or `No item of this binding uses the handle "x".` | write it like `handle of sha2.Sha256`; handle types come from the bindings in `bindings/`, which `agentmark bind <crate>` writes |
| E361 | `"x" was used up on line N ("text"), so it cannot be used again here.` or `"x" is used up here, but this line is inside a loop and runs again with the same "x".` | keep using it before that line, or make a new handle inside the loop, or use it up after the loop |
| E362 | `"x" is T, a library value that cannot be printed, compared, saved, sent or traced.` or `This is T, a library value that cannot be printed, compared, saved, sent or traced.` | use it with a call that takes it, or take a value out of it first with `... -> name` |
| E363 | `A handle cannot be in a list: T.` or `A handle cannot be in a maybe: T.` or `A record is saved and sent as JSON, so it cannot have a field of T.` | keep one variable per handle |
| E365 | a mode word, `self` or a borrowed handle is used wrongly (all 4 messages below the table) | write `mut` and `consume` only in a binding's parameter list, `self` first as `self: mut handle of sha2.Sha256`, and store a handle in a variable before the call |
| E366 | `"x" is a handle, and there is none to use when this step fails.` | end the `otherwise:` branch with `stop` or `return`, or drop it |
| E367 | `x comes from outside the program, and a handle cannot: it is a live library value (T).` | read plain values from outside the program |

**E365 messages**

- `"mut" in front of "handle of" only belongs in a binding's parameter list.`
- `"self" must be the first parameter of a binding item, and a handle.`
- `"x" is given twice in one call.`
- `"x" borrows its handle, so give it a variable, not a new value.`

E364 is intentionally unused. An unknown method of a handle is E207.

**Handles that borrow**

A handle owns whatever the Rust value borrows. This holds for a library type with lifetimes, such as `Query<'q, DB>`. It also holds for a type whose parameters have one instantiation in the crate as configured, such as `DB = Sqlite` when only the `sqlite` feature is on.

The items around such a type go to the assisted tier. The model writes the glue, copying or leaking what the call borrows, and the binding carries a block with the Rust type it stands for:

````text
## Handle: sqlx.query.Query
  ```rust
  sqlx::query::Query<'static, sqlx::Sqlite, <sqlx::Sqlite as sqlx::Database>::Arguments<'static>>
  ```
````

**Types and features**
- A Rust integer wider than `int`, such as `u64` or `usize`, converts with `try_from(..)?`, which fails the call on overflow.
- The features a library is bound and built with come from `agentmark.toml`, for the binder and the generated `Cargo.toml` alike:

```toml
[crates]
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio"] }
```

### 15.5 Sharing

Bindings are plain files, so a community registry is a git repository of `bindings/*.agent.md`. The file format needs no change for that, and the registry is out of scope for v0.1.

**Next:** [16. Concurrency](#16-concurrency)

## 16. Concurrency

Blocks run one after the other. A `parallel:` statement is the explicit opt-in to run statements at the same time:

```agentmark
# Record: Invoice
- id: text
- amount: money

> Fetches rates and invoices at the same time.
# Function: load_all
- out: list of Invoice
- effect: network
  - parallel:
    - http get "https://api.example.com/rates" -> rates
    - crm list invoice -> invoices
  - return invoices

> Runs it at start.
# Agent: loader
- model: claude
- tools: crm, http, log

## On start
- do load_all -> all
- log: "{count of all} invoices"
```

**Rules**
- v0.1 runs blocks one after the other. The IR carries an effect graph, so a later version can run independent pure and local statements concurrently.
- Statements inside `parallel:` must not write the same variable. The checker reports a conflict as E212.
- All generated functions are `async` and use `tokio` from the start.

**Next:** [17. Escape hatch](#17-escape-hatch--rust-blocks-layer-1)

## 17. Escape hatch: `## Rust` blocks (layer 1)

Any agent, service, command or function may contain a block `## Rust: <name>` whose body is a fenced `rust` block. Variables in scope are available by name, and the block must end with a value of its declared type:

````agentmark
# Import: crate sha2

> Fingerprints a payload with SHA-256.
# Function: fingerprint_of
- in: payload: text
- out: text
- effect: pure
  - return hash_payload(payload)

## Rust: hash_payload -> text
- in: payload: text
- effect: pure
  ```rust
  use sha2::{Digest, Sha256};
  format!("{:x}", Sha256::digest(payload.as_bytes()))
  ```
````

**Rules**
- The heading is `## Rust: <name>` or `## Rust: <name> -> <type>`.
- `- in:` lists the parameters. `- effect:` is **mandatory**.
- The effect is trusted, because the checker cannot verify arbitrary Rust. `agentmark check` lists every Rust block in its summary, so a reviewer sees where trust is extended.
- A Rust block defines a module-level function called `<name>`. The declaration it sits under only groups it, per Q2.
- Any function in the project can call it like a function, as in `hash_payload(payload)`.
- The block is wrapped in its own `fn` with `#[allow(unused)]` and a source-mapping comment. So a Rust error inside it points at the `.agent.md` line.
- Rust blocks may `use` any crate imported with `# Import: crate`.

| code | message | fix |
|---|---|---|
| E012 | `The Rust block "x" does not declare its effect.` | add `- effect: pure`, or `local`, `network`, `model`; the compiler cannot check Rust, so it trusts this line; the other E012 messages are in section [10](#10-error-messages) |

**Next:** [18. Code organization](#18-code-organization-and-readability)

## 18. Code organization and readability

AgentMark is Markdown so that a person reads a program like a document, and the language enforces it.

### 18.1 Project layout

```text
my-project/
  agentmark.toml          name, version, targets, lint settings, models
  tools.toml              tool manifest (section 7)
  main.agent.md           entry point: imports + a single `# Run`
  records/                one # Record per file:  records/invoice.agent.md
  rules/                  one # Rule per file:     rules/overdue.agent.md
  agents/                 one # Agent per file:    agents/invoice-chaser.agent.md
  functions/              one # Function per file
  bindings/               generated by the binder; reviewed, committed
  fixtures/               test data and expected traces
  docs/                   optional prose; not compiled
```

`agentmark.toml` is small:

```toml file=examples/01-invoice-chaser/agentmark.toml
name = "invoice-chaser"
version = "0.1.0"
targets = ["rust"]

[lint]
deny = ["prompt-in-loop", "missing-doc"]
```

`# Run` is the only declaration allowed in `main.agent.md` besides imports. Each item is an imported `# Agent`, `# Service` or `# Command`:

```agentmark file=examples/05-invoice-service/main.agent.md
# Import: ./commands/import-csv.agent.md
# Import: ./services/api.agent.md

> Entry point. The API runs as a service; the CSV import is a subcommand.
# Run
- api
- import-csv
```

**Rules**
- The binary starts all listed agents and services. Commands become subcommands.
- A project with no `# Run` compiles to a library, not a binary.
- Unknown keys in `agentmark.toml` are errors with a suggestion, per Q40.
- `[lsp] model_actions = true` lets the editor server offer its optional model-backed actions, per Q250. It is off by default.

### 18.2 Imports

```agentmark fragment
# Import: ./records/invoice.agent.md
# Import: ./rules/*.agent.md
# Import: ./billing as billing
# Import: crate printpdf
```

**Rules**
- A file sees only what it imports. There is no global scope.
- Paths are relative to the importing file.
- A glob imports every matching file.
- An import cycle is an error, and the cycle is printed.

### 18.3 Hard limits

These are errors and cannot be switched off, because they are what keep a file readable.

```agentmark invalid name=deep.agent.md says=nested_4_levels_deep
# Record: Invoice
- id: text
- amount: money
- status: text

> Walks too deep.
# Agent: deep
- model: claude
- tools: crm, log

## Every day at 09:00
- for each invoice in crm.invoices:
  - if invoice.status = "open":
    - if invoice.amount > 100 EUR:
      - if invoice.amount > 500 EUR:
        - log: "big"
```

```console stderr
$ agentmark check deep.agent.md
deep.agent.md:15
        - if invoice.amount > 500 EUR:
          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  Statements are nested 4 levels deep; the limit is 3.
  Move lines 15–16 into a "## Do <name>" block and call it with "do <name>".
```

| rule | limit |
|---|---|
| nesting depth of statements | 3 (`for each`, `if`, `if` is the maximum) |
| statements per block | 25, counting nested ones too (Q33) |
| declarations per file | in a project folder: one of agent, service or command per file; several records only in `records/`, several rules only in `rules/`, never records and rules together. A single file is a whole program and is exempt, with warning W006 (Q34) |
| line length | 120 characters; the text between string quotes does not count (Q35), and a line that is long only because of such text is warning W007 |
| block with no statements | error |

The code for a broken limit is E211. See section [10](#10-error-messages).

### 18.4 Soft rules (`agentmark lint`)

```agentmark name=loose.agent.md
# Record: Invoice
- id: text
- data: text
- amount: money
- status: text

# Agent: loose
- model: claude
- tools: crm, email, log

## Every day at 09:00
- for each invoice in crm.invoices:
  - ask model with invoice: "Summarise." -> result
  - email send address "a@b.de": "{result}"
  - if invoice.amount > 100 EUR:
    - log: "over 100 EUR"
```

```console stderr
$ agentmark lint loose.agent.md
loose.agent.md:3
  - data: text
    ^^^^
  "data" does not say what it holds. [vague-name]
  Name it after its content, like "invoice_rows" or "reminder_text".

loose.agent.md:7
  # Agent: loose
    ^^^^^^^^^^^^
  The agent "loose" has no description. [missing-doc]
  Put a "> description" comment on the line directly above its heading.

loose.agent.md:13
    - ask model with invoice: "Summarise." -> result
      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  "ask model" runs for every item of a "for each" loop and nothing checks its result "result". [prompt-in-loop]
  Add "- guard: ..." on "result" after it: the model can answer differently for every item (SPEC 4.4).

loose.agent.md:13
    - ask model with invoice: "Summarise." -> result
                                              ^^^^^^
  "result" does not say what it holds. [vague-name]
  Name it after its content, like "invoice_rows" or "reminder_text".
```

**Rules**
- Lint rules are warnings by default.
- `agentmark.toml` can promote any rule to an error with `[lint] deny = ["..."]`.
- An unknown rule name is an error with a suggestion.
- `fmt` and `lint` skip `bindings/`, per Q111.

| rule | fires on |
|---|---|
| `long-file` | a file over 150 lines |
| `magic-value` | a money, duration or URL literal used inline more than once in a file (zero amounts are exempt); name it with a `# Constant` |
| `unnamed-guard` | a guard whose condition is longer than 60 characters and has no `>` comment line above it |
| `duplicate-rule` | two rules with identical `when:` expressions, compared structurally across files |
| `prompt-in-loop` | `ask model` inside `for each` without a later `guard` on its result in the same loop body |
| `missing-doc` | an agent, service or command with no `>` description directly above its heading |
| `vague-name` | an identifier called `data`, `tmp`, `x`, `result` or `thing` where it is introduced |

### 18.5 Constants

```agentmark
# Constant: grace_period
- value: 14 days

# Constant: sales_inbox
- value: "sales@optimotion.dev"

# Record: Invoice
- id: text
- due: date

# Rule: overdue
- for: Invoice
- when: today > due + grace_period

> Mails sales about overdue invoices.
# Agent: alerter
- model: claude
- tools: crm, email

## Every day at 09:00
- for each overdue invoice in crm.invoices:
  - email send address sales_inbox: "{id} is overdue"
```

**Rules**
- Constants are pure and imported like anything else.
- They are inlined at compile time.
- They can hold any literal: a number, a text, money, a duration or a date.

### 18.6 Formatter

`agentmark fmt` rewrites a file into the one canonical form. This file is not canonical:

```agentmark fragment name=messy.agent.md
# Record:   Invoice
- id: text
- amount: money



> Logs.
# Agent: tidy
- tools: crm, log
- model: claude

## every day at 09:00
for each invoice in crm.invoices:
  - log: "{invoice.id}"
```

```console
$ agentmark fmt --check messy.agent.md
not canonical: messy.agent.md
1 of 1 file is not canonical; run "agentmark fmt" to fix it.
```

This is what `agentmark fmt messy.agent.md` writes:

```agentmark
# Record: Invoice
- id: text
- amount: money

> Logs.
# Agent: tidy
- model: claude
- tools: crm, log

## Every day at 09:00
- for each invoice in crm.invoices:
  - log: "{invoice.id}"
```

**Rules**
- The canonical form has a 2-space indent, one blank line between declarations, properties before blocks and no trailing whitespace.
- Imports are sorted: local files first, then folders, then crates.
- There are no formatting options.
- A few spellings are fixed, per Q51. Statements are always list items, and a block heading is printed from its parsed trigger.
- Money prints decimals only when it is not whole, as in `12.50 EUR` and `14 EUR`. Durations are singular for 1, and `do f()` prints as `do f`.
- Comments survive, per Q52.
- `fmt --check` fails CI when a file is not canonical.

### 18.7 Documentation comments

A `>` comment immediately before a declaration or block is its documentation. `agentmark doc` collects these into one page per project, so the text an operations manager reads is the text the author wrote next to the code:

```console cwd=repo
$ agentmark doc examples/04-daily-report
# daily-report

## Agents

### daily-report

Mails Anil a two-sentence summary of open and overdue invoices every Monday. Reads only; changes nothing.

- model: claude
- tools: crm, email, log
- can touch: model, network(crm, email), local(log)

**Every Monday at 08:00** (can touch: model, network(crm, email), local(log))


## Records

### Invoice

An invoice as stored in the CRM.

- id: text
- customer: text
- amount: money
- due: date
- status: text


## Rules

### open

Not paid yet, whether or not it is late.

Applies to Invoice. Always pure.

### overdue

Unpaid and more than the grace period past due.

Applies to Invoice. Always pure.
```

**Rules**
- `agentmark doc <crate>` prints the cheat sheet of a bound library instead. See section [15](#15-library-access-automatic-bindings).
- If the argument names something on disk, it is a project. Otherwise it is a library.
- `--project` says where to look for `bindings/`, per Q114.

**Next:** [19. Model providers](#19-model-providers)

## 19. Model providers

`- model:` is a driver string, never a vendor lock. The runtime's `ModelClient` trait has one implementation per provider, and the language is unaware of which is in use.

```agentmark
# Record: Lead
- id: text

> Uses the cheap model for classification.
# Agent: classifier
- model: claude:haiku
- tools: crm, log

## When crm receives lead
- ask model with lead: "Is this lead a company or a person? One word." -> kind
- log: "{kind}"
```

**Rules**
- The driver string picks the provider. The table below lists the strings.
- `check` rejects a driver it does not know, such as `Unknown model provider "opnai". Did you mean "openai"?`. It also rejects `openai:` or `local:` without a model name.
- `azure` and `bedrock` are not supported in this version, and `check` says so at the `- model:` line.

| string | implementation |
|---|---|
| `claude`, `claude:<model>` | Anthropic Messages API (key from `ANTHROPIC_API_KEY`; `haiku`, `sonnet`, `opus` or a full id) |
| `openai:<model>` | OpenAI Chat Completions (key from `OPENAI_API_KEY`; `OPENAI_BASE_URL` points it at a proxy) |
| `local:<model>` | any OpenAI-compatible local server (Ollama, LM Studio, vLLM); base URL from `agentmark.toml` |
| `mock`, `mock:<path>` | fixture-driven, for tests: `model.json` in `$AGENTMARK_FIXTURES` (default: the current folder), or the file named |
| `azure:<deployment>`, `bedrock:<model>` | enterprise routes: not supported in this version; `check` says so at the `- model:` line |

This agent is rejected:

```agentmark invalid
> Enterprise routes are not supported in this version.
# Agent: bank
- model: azure:my-deployment
- tools: log

## On start
- log: "hello"
```

| code | message | fix |
|---|---|---|
| E221 | the model driver is unknown, unsupported or incomplete (all 4 messages below the table) | use a driver from the table above; for an unknown name the message adds a suggestion such as `Did you mean "openai"?` |

**E221 messages**

- `Unknown model provider "x".`
- `The model provider "x" is not supported in this version.`
- `"x:" needs a model name, like "x:model".`
- `The model is empty.`

### The `[models]` table and `AGENTMARK_ENV`

`agentmark.toml` maps environments to drivers and says where the local server is:

```toml
[models]
production = "claude"
test = "mock"
dev = "local:kat-coder"

[models.local]
base_url = "http://localhost:11434/v1"
```

Which model answers is decided at run time. The sources are listed strongest first, per Q240:

1. `AGENTMARK_MODEL=<driver>` sets one driver for the whole program. Examples are `openai:gpt-4o`, `local:kat-coder` and `mock:<path to model.json>`.
2. `AGENTMARK_ENV=<name>` picks the `[models]` entry of that name, again as one driver for every agent. `AGENTMARK_ENV=dev` runs the program on the local model and `AGENTMARK_ENV=test` runs it on `model.json`, without touching an `.agent.md` file.
3. Otherwise each agent uses its own `- model:` line.

**Rules**
- An environment that is not in the table is an error that lists the ones that are. `production` without an entry means "no override".
- Unset `AGENTMARK_ENV` is not "production". Nothing is overridden, so a `- model: openai:gpt-4o` agent keeps OpenAI even if `production = "claude"`.
- A plain `claude` entry keeps an agent's own Claude model, so `claude:haiku` stays haiku. It moves agents of other providers to Claude.
- The generated program carries the table. The variables are read when it runs.
- `agentmark bind` and the on-demand tier use the same rules with the project's `agentmark.toml`, and Claude when nothing is set. So a company that forbids external APIs can run the whole toolchain on a local model.
- The local server's address is, strongest first, `AGENTMARK_LOCAL_BASE_URL`, then `[models.local] base_url`, then `http://localhost:11434/v1`, which is Ollama.
- `LOCAL_API_KEY`, if set, is sent as a bearer token.
- The agent's own `model:` line describes the program, not the toolchain, per Q115.

**Next:** [20. v0.2 scope](#20-v02-scope-editor-and-tooling)

## 20. v0.2 scope: editor and tooling

Nothing here changes the language. This was built after v0.1 passed all its tests, in this order:

1. The on-demand binding tier, the last row of the table in section [15.2](#152-the-binder): built.
2. The model providers `openai` and `local` and the `[models]` table, section [19](#19-model-providers): built. `azure` and `bedrock` stay unsupported.
3. A language server, `agentmark-lsp`, in the same workspace. It reuses the checker as a library. It offers diagnostics with the same messages, completion, go-to-definition across imports, hover with a block's effect summary, format-on-save and code actions for the mechanical lint fixes.
4. A VS Code extension in `editors/vscode/`: grammar, language-server client, an `agentmark.toml` schema and a "what can this agent touch" panel that reads `check --json`.
5. A tree-sitter grammar in `editors/tree-sitter-agentmark/`.
6. AI-assistant compatibility. This file stays the single file an LLM needs, and `templates/` holds a `CLAUDE.md` and `.cursorrules` that point at it.

**Next:** [21. Out of scope for v0.2](#21-out-of-scope-for-v02)

## 21. Out of scope for v0.2

- Automatic parallelism. Only explicit `parallel:` exists.
- Several agents talking to each other.
- Targets other than Rust.
- Closures and generics in the core language.

**Next:** [22. v0.3](#22-v03-a-general-purpose-markdown-language)

## 22. v0.3: a general-purpose Markdown language

Section 22 of the specification makes the core a general-purpose language. A program that has nothing to do with agents finds its tools here.

| section | what it adds |
|---|---|
| [22.1](#221-user-defined-types) | user-defined types and `match` |
| [22.2](#222-collection-words) | collection words |
| [22.3](#223-the-built-in-library) | the built-in library and the `file` tool |
| [22.4](#224-opaque-library-types) | opaque library handles |
| [22.5](#225-user-defined-tools) | user-defined tools |
| [22.6](#226-programs) | programs with a default command |

**Rules**
- Everything in section 22 is layer 2.
- Every sample in it is a program that uses no agent and no `## Rust` block.
- The agent sections 2.2, 2.3, 3, 4.2 and 4.4 stay where the specification puts them. An agent is built from the same parts.

### 22.1 User-defined types

An `# Enum` lists variants, and a `# Type` gives another name to a type. `match` takes an enum value apart, one branch per variant:

```agentmark
# Enum: Shape
- Circle: radius: int
- Rect: width: int, height: int
- Empty

# Type: Sizes
- is: list of int

> Says what a shape is in one line.
# Function: describe
- in: shape: Shape
- out: text
- effect: pure
  - match shape:
    - Circle c:
      - return "circle with radius {c.radius}"
    - Rect r:
      - return "rectangle {r.width} by {r.height}"
    - Empty:
      - return "nothing"

> Adds up a list of sizes.
# Function: total
- in: sizes: Sizes
- out: int
- effect: pure
  - return count of sizes

> Describes three shapes.
# Command: show

## Run
- print: describe(Circle { radius: 3 })
- print: describe(Shape.Rect { width: 2, height: 5 })
- print: describe(Shape.Empty)
```

**Rules**
- A variant may carry named fields. A value of such a variant is written `Circle { radius: 3 }`, and a variant without fields is just its name, `Empty`.
- Inside a branch of `match`, the name after the variant, such as `c`, holds that variant's fields.
- A `# Type` and the type it names are interchangeable.
- The names of records, enums and types share one namespace. Two of them with the same name are an error.
- Variant names are unique inside one enum.
- Two enums may both have a variant `Empty`. The name then needs its enum, as in `Shape.Empty`, or `billing.Shape.Empty` through a module alias. A bare `Empty` is an error that lists the choices.
- Inside a `match`, the branches are looked up in the enum being matched, so they never need the qualifier.
- A variable or a constant with the same name as a variant wins over the variant.
- A variant's fields are given by name, each exactly once. A field of type `maybe` may be left out.
- A variant cannot have a field called `kind`, because of the JSON form below.
- Enum values compare with `=` and `!=`. They cannot be written into text as a whole, such as `"{shape}"`, and they have no fields of their own. Read a field inside the branch of the variant that has it.
- An alias is replaced by what it names wherever it is written, so it never shows up in an error message or in the generated program. An alias that is defined in terms of itself is an error.

**Matching a `maybe` or a `result`**

`match` also takes a `maybe` and a `result`:

```agentmark
# Record: Row
- id: text

# Function: find
- in: rows: list of Row, id: text
- out: maybe Row
- effect: pure
  - return first row in rows where row.id = id

# Function: read_count
- in: path: text
- out: result of int
- effect: local
  - try read file path as int -> n
  - return n

> Looks a row up and reads a count; says what happened to each.
# Command: check
- args: rows_path: text, count_path: text

## Run
- try read file rows_path as list of Row -> rows
  otherwise:
    - stop
- match find(rows, "a"):
  - some row:
    - print: "found {row.id}"
  - none:
    - print: "no such row"
- match read_count(count_path):
  - ok total:
    - print: "count {total}"
  - error problem:
    - print: "could not read: {problem}"
```

- On a `maybe`, the branches are `some x` and `none`. On a `result`, they are `ok x` and `error e`, where `e` is the failure's message as text.
- A `result` is matched at the call, as above, or stored first with `- set parsed = read_count(path)` and matched once. Nothing else can read a stored result, per Q300.
- A `maybe` value is written `some x` or `none`.
- In a branch, the name after the variant, or after `ok` or `error`, is optional when the value is not used, as in `- error:`.
- A function that returns `result of T` is also used with `try`, without `do`. `try do f(x) -> v` means the same.
- `if ... is some x:` works on any `maybe` expression, including a call.

**Building a record**

A record is built the same way as a variant, with every field that is not `maybe` given:

```agentmark
# Record: Sale
- region: text
- amount: money

# Function: parse_sale
- in: path: text
- out: result of Sale
- effect: local
  - try read file path as Sale -> sale
  - return sale

# Function: find_sale
- in: sales: list of Sale, region: text
- out: maybe Sale
- effect: pure
  - return first sale in sales where sale.region = region

> Reads a sale, builds another and looks one up.
# Command: sales
- args: path: text, list_path: text

## Run
- try parse_sale(path) -> sale
  otherwise:
    - set sale = Sale { region: "none", amount: 0 EUR }
- print: "{sale.region} {sale.amount}"
- try read file list_path as list of Sale -> sales
  otherwise:
    - stop
- if find_sale(sales, "EU") is some eu:
  - print: "EU {eu.amount}"
```

**Every case must be handled**
- Every case must be handled, or an `- otherwise:` branch must say what happens to the rest.
- `otherwise:` is the last branch. One that can never run is a warning.
- A misspelled variant in a branch or in a value gets the usual suggestion: `Shape has no variant "Circel". Did you mean "Circle"?`
- `match` is a statement. It is pure unless its branches are not.
- `match` counts as one level of nesting, whatever the number of branches. The limit is 3.

The error names what is missing:

```agentmark invalid name=shapes.agent.md says=does_not_handle_the_variant_"Empty"_of_Shape
# Enum: Shape
- Circle: radius: int
- Empty

> Says whether a shape is round.
# Command: round

## Run
- set shape = Circle { radius: 3 }
- match shape:
  - Circle c:
    - print: "round"
```

```console stderr
$ agentmark check shapes.agent.md
shapes.agent.md:10
  - match shape:
    ^^^^^^^^^^^^
  This "match" does not handle the variant "Empty" of Shape.
  Add a branch for it ("- Empty:"), or an "- otherwise:" branch for everything else.
```

**JSON.** An enum value is an object whose `kind` field names the variant, followed by the variant's fields: `{"kind":"Circle","radius":3}` and `{"kind":"Empty"}`, per Q301. That is what `read file`, `read body` and `respond` read and write.

**Errors**

| code | message | fix |
|---|---|---|
| E300 | `The record "x" has the same name as the enum in file:line.` | rename one; the kinds in the message vary |
| E301 | `The enum "x" has no variants.` | list the variants under the heading, like `- Circle: radius: int` or `- Empty` |
| E302 | `The enum "x" has the variant "x" twice.` | remove or rename one |
| E303 | `A variant cannot have a field called "kind": the JSON of an enum value uses it to say which variant it is.` or `The variant "x" has the field "x" twice.` | pick another name, for example `type` or `label` |
| E304 | `The type "x" is defined in terms of itself.` | break the loop |
| E305 | `x has no variant "x".` or `Unknown enum "x".` or `Unknown variant "x".` | write a variant bare, or after its enum as in `Shape.Empty` |
| E306 | a variant or record is written with the wrong fields (all 6 messages below the table) | write `Circle { radius: 3 }` with every field that is not `maybe` |
| E307 | `"x" is a variant of several enums here: a, b.` | say which one you mean, as in `Shape.Empty` |
| E308 | `"match" works on an enum, a "maybe" or a "result", but this is T.` | use `if` for a yes/no or a comparison |
| E309 | `This "match" is on x, but the branch names "x".` or `"x" is not a branch of x.` | use the branches the message lists |
| E310 | `This "match" does not handle the variant "x" of x.` or `This "match" does not handle the variants "x", "x" of x.` or `This "match" does not handle the case "x" of the maybe.` | add a branch for it, such as `- Empty:`, or an `- otherwise:` branch for everything else |
| E311 | `This "match" already has a branch for "x".` | remove the second branch |
| E312 | `The variant "x" has no fields, so there is nothing to name "x".` or `"x" carries no value, so there is nothing to name "x".` | write just `- x:` |
| E313 | a result is read instead of matched (all 4 messages below the table) | write `- match <call>:` with the branches `ok value` and `error problem`, or use `try` |
| E314 | `A T cannot be written into text as a whole.` | use `- match <value>:` and write what each variant carries |
| E315 | `The variant "x" of x holds a x inside itself, which would never end.` | break the loop |
| E316 | `"x" stands for the fields of T, not for a value of T.` | read one of its fields |
| E317 | `This "none" does not say what it is a "none" of.` | use `none` where a `maybe` value is expected, such as `return none` in a function that returns `maybe int` |
| W300 | `Every case is handled above, so this "otherwise:" can never run.` | remove the branch |

**E306 messages**

- `The variant "x" of x has fields, so it is written with them.`
- `The field "x" is given twice.`
- `The x has no fields.`
- `The x has no field "x".`
- `The x needs a value for "x".`
- `The x needs values for "x", "x".`

**E313 messages**

- `The result in "x" is never matched, so its failure would be lost.`
- `"x" holds a result, which can only be matched.`
- `A result can only be matched: it cannot be read like other values.`
- `A result is made by a call and matched where it is stored; it cannot be passed in.`

**Next:** [22.2 Collection words](#222-collection-words)

### 22.2 Collection words

Lists are `list of T`. Maps are `map of K to V`, with keys of type `text`, `int`, `date`, `datetime` or `bool`.

```agentmark
# Record: Sale
- region: text
- amount: money
- qty: int

# Function: eu_total
- in: sales: list of Sale
- out: money
- effect: pure
  - set eu = sales where region = "EU"
  - return sum of amount in eu
```

**Rules**
- The words are expressions unless marked as statements below. They are pure: they build a new value and never change their input.
- Inside the expression after `where`, `by`, `to` and `of … in`, the current element is called `item`.
- When the element is a record, its fields are in scope by their bare names, as in a `for each` over a record. So `rows where region = "EU"` and `rows where item.region = "EU"` mean the same.
- For a list of text or numbers only `item` exists, as in `names where item != "a"`.
- Results that can be empty are `maybe` values. Open them with `is some` before you use them.
- `append`, `put` and `remove` change a variable or a `state.…` path, never a computed value. `put` and `remove` are statement words, so a tool cannot have those names.

**Expression forms**

| form | result |
|---|---|
| `list of 1, 2, 3` · `empty list of text` | a list |
| `map of "EU" to 1, "US" to 2` · `empty map of text to int` | a map |
| `rows where region = "EU"` | the elements for which the condition holds |
| `map rows to amount` | a list of the expression's values |
| `sort rows by amount` · `sort rows by amount descending` | the list in order (stable: equal keys keep their order) |
| `group rows by region` | `map of K to list of T`, in key order |
| `sum of amount in rows` · `sum of numbers` | a number, or money of one currency; an empty list adds up to 0 |
| `count of rows` · `count of rows where amount > 5 EUR` | an `int` |
| `min of amount in rows` · `max of …` | `maybe` of the value: empty for an empty list |
| `first of rows` · `last of rows` | `maybe` of the element |
| `take 10 from rows` · `skip 2 from rows` | the list without its tail or head |
| `unique rows` · `unique region in rows` | every value once, in order of first occurrence |
| `join names with ", "` | text; the list must be a list of text |
| `totals at "EU"` | `maybe` of the value: empty when the key is absent |
| `keys of totals` · `values of totals` | a list, keys in order |

**Statement forms**

| statement | meaning |
|---|---|
| `append x to rows` | add to the end of a list |
| `put v at k in m` | set the entry of key `k` (replaces an existing one) |
| `remove k from m` | delete the entry of key `k`; nothing happens when it is absent |
| `for each row in rows with index i:` | `i` counts the position from 1 |
| `for each region, total in totals:` | the entries of a map, in key order |

More words in use:

```agentmark
# Record: Sale
- region: text
- amount: money

# Function: region_lines
- in: sales: list of Sale
- out: list of text
- effect: pure
  - set lines = empty list of text
  - set by_region = group sales by region
  - for each region, rows in by_region:
    - append "{region}: {count of rows} sales, {sum of amount in rows}" to lines
  - return lines

# Function: biggest_first
- in: sales: list of Sale
- out: list of Sale
- effect: pure
  - return take 3 from (sort sales by amount descending)

# Function: ages_after_birthday
- out: map of text to int
- effect: pure
  - set ages = map of "ann" to 31, "bob" to 25
  - put 32 at "ann" in ages
  - remove "bob" from ages
  - return ages
```

Open a result that can be empty before you use it:

```agentmark fragment
- if max of qty in sales is some most:
  - print: "largest quantity {most}"
- if totals at "EU" is empty:
  - print: "nothing sold in the EU"
```

**How the words read**
- **A source is a name, a call or a parenthesis:** `sort (rows where amount > 5 EUR) by amount` needs the parentheses. `rows where …` and `totals at key` hang off a name or a call.
- `keys of` and `values of` also stand where a source is expected, as in `sum of values of totals`.
- **A literal takes every comma after it:** `f(list of 1, 2, 3)` passes one list, and `f((list of 1, 2), 3)` passes two arguments. A `list of Sale` after `as` or `->` is a type, since the position decides.
- **`where` runs to the end of the expression**, so `rows where a = 1 and b = 2` filters on both.
- **Money:** `sum of`, `sort` and `min` or `max` over money need one currency. Two currencies are a run-time error, `currency mismatch: EUR vs USD`, never a guess. The sum of an empty list of money is `0 EUR`.
- **Maps** iterate, print and serialise in key order. As JSON a map is an object, as in `{"DE": 1, "EU": 2}`.
- **Purity:** The expression that runs per element may read values and call pure functions, nothing else. `names where shout(item) = "a"` is an error when `shout` prints. Use a `for each` loop for that.
- **Names:** A variable with the name of a field of the element is an error inside the word. After `set region = "US"`, the expression `sales where region = "EU"` fails. Write `item.region` for the field, or rename the variable.
- **`item`:** A variable called `item` is hidden by the element inside the expression. Inside a word, `item` is the innermost element. So `nested where (count of item where item > 1) > 1` counts the inner `item` in the second one.
- **Copies:** A parameter or loop variable that is appended to is a copy. The caller keeps its list.

A list with two kinds of value is an error:

```agentmark invalid
# Function: f
- effect: pure
  - set mixed = list of 1, "two"
```

The decisions behind these rules are Q320 to Q328 in `OPEN_QUESTIONS.md`.

**Errors**

| code | message | fix |
|---|---|---|
| E320 | `Every x must be the same kind of value: the first is T, but this is T.` | use one kind of value |
| E321 | `A map cannot be keyed by T.` or `"group … by" needs a key that is text, a number, a date or a yes/no value, but this is T.` | key by text, a number, a date, a datetime or a yes/no value |
| E322 | `"x" needs values it can put in order, but this is T.` | numbers, money, text, dates and durations have an order; records and lists do not |
| E323 | `"sum of" adds numbers or money, but these are T.` | sum the numbers or amounts of a field, as in `sum of amount in rows` |
| E324 | `"x" needs a list, but this is T.` or `"x" needs a map, but this is T.` | use `values of` or `keys of` to get a list out of a map; write a map like `map of "EU" to 1, "US" to 2` |
| E325 | `"join" glues a list of text, but this is a list of T.` | turn the values into text first, for example `map rows to "{item.id}"` |
| E326 | `The keys of this map are T, but this is T.` or `This map holds T, but this is T.` | use keys and values of the map's kinds |
| E327 | `"x" is a variable here and also a field of T, so it is unclear which one is meant.` | write `item.x` for the field, or rename the variable |
| E328 | `"x" changes a variable, but this is not one.` | name the value first with `set`, change that, and use it afterwards |
| E329 | `This line nests more than N levels of parentheses or braces.` | name the inner part with `set` on a line of its own |

**Next:** [22.3 The built-in library](#223-the-built-in-library)

### 22.3 The built-in library

The library is part of the language, not a binding. This command uses four of its modules, `text`, `money`, `date` and `number`:

```agentmark
> A shipping label: street padded, price as money, deadline as a date.
# Function: label_line
- in: street: text, price: money, deadline: date
- out: text
- effect: pure
  - return "{text.pad(street, 16)}{money.format(price)}  by {date.format(deadline, "DD.MM.YYYY")}"

> Reads a whole number and says how many days remain until a date.
# Command: remaining
- args: answer: text, deadline: text

## Run
- try number.parse(answer) -> days
  otherwise:
    - print: "{answer} is not a number"
    - stop
- try date.parse(deadline, "YYYY-MM-DD") -> due
  otherwise:
    - print: "{deadline} is not a date like 2026-10-31"
    - stop
- print: "{label_line("Main street 1", 12.50 EUR, due)}"
- print: "{date.days_between(today, due) - days} days to spare"
```

**Rules**
- Library functions are written `module.name(arguments)`, are all `pure`, and need no `# Import`.
- There are eight modules, listed in the table below.
- `agentmark check` lists the same table, and a wrong name says which functions the module has. An editor completes the names after `text.`.
- A function that can fail returns `result of T`. These are `number.parse`, `money.parse`, `date.parse`, `json.parse` and `csv.parse`.
- Call such a function with `try`, which takes the call directly. A leading `do` is allowed. You can also call it with `match`, as section [22.1](#221-user-defined-types) shows.
- Calling it any other way is error E341. After `try`, the variable has the plain `T`.
- Library errors have plain messages that name the value, such as `"4x" is not a whole number`, `line 3, column 2: …` for a CSV cell and `cannot read "x.txt": it does not exist`.

**Modules**

| module | functions |
|---|---|
| `text` | `split(t, sep)`, `lines(t)`, `trim(t)`, `lower(t)`, `upper(t)`, `replace(t, from, to)`, `contains(t, part)`, `starts_with(t, p)`, `ends_with(t, p)`, `length(t)`, `pad(t, width)`, `repeat(t, n)` |
| `regex` | `matches(t, pattern)`, `find(t, pattern)`, `captures(t, pattern)`, `replace(t, pattern, with)` |
| `number` | `round(x, places)`, `parse(t)`, `min(a, b)`, `max(a, b)`, `abs(x)`, `format(x, places)` |
| `money` | `parse(t)`, `format(m)` |
| `date` | `parse(t, pattern)`, `format(d, pattern)`, `days_between(a, b)` |
| `path` | `join(a, b)`, `name(p)`, `stem(p)`, `extension(p)`, `parent(p)` |
| `json` | `parse(t) as T`, `encode(v)` |
| `csv` | `parse(t) as list of T`, `encode(rows)` |

**What each function gives**
- **text:** `split` gives the pieces between `sep`. `lines` works like Rust `str::lines`, with no empty entry for a final newline. `replace` replaces every occurrence.
- **text:** `length` counts characters. `pad` adds spaces on the right up to `width` characters and never cuts longer text. `repeat` writes `t` `n` times.
- **regex:** the Rust `regex` syntax applies, and a match is found anywhere in the text. `matches` is a `bool`, `find` is `maybe text` and `captures` is `maybe list of text`.
- **regex:** `captures` gives the capture groups of the first match, without the whole match. A group that took no part is empty text.
- **regex:** `replace` replaces every match, with `$1` standing for group 1. A literal pattern that does not compile is an error when you check the program.
- **number:** it works on `int`. `min`, `max`, `abs` and `round` also work on `money` of one currency.
- **number:** `round(x, places)` rounds half away from zero. For money it rounds to `places` decimals, so `round(12.35 EUR, 1)` is `12.40 EUR`. For a whole number only a negative `places` changes it, so `round(1235, -1)` is `1240`.
- **number:** `format(x, places)` writes `places` decimals, so `format(12, 2)` is `12.00`.
- **money:** `money.format(m)` writes the amount like a literal, as in `12.50 EUR`. `money.parse` reads one.
- **date:** `date.format(d, pattern)` and `date.parse(t, pattern)` take a pattern. A pattern that contains a `%` is strftime, such as `%Y-%m-%d` or `%Y%m%d`.
- **date:** any other pattern is written with `YYYY`, `YY`, `MMMM`, `MMM`, `MM`, `DD`, `dddd`, `ddd`, `HH`, `mm` and `ss`. Other characters stand for themselves, and `[text]` protects letters, as in `DD.MM.YYYY HH:mm`.
- **date:** `format` also takes a `datetime`. `days_between(a, b)` is the number of days from `a` to `b`, negative when `b` is earlier.
- **path:** it works on text only. `extension("a/b.jpg")` is `jpg`, without the dot, and empty when there is none. `parent("a/b.jpg")` is `a`.
- **json:** `json.parse(t) as T` reads JSON into a record, a list or any type. Field names are the JSON keys, money is text like `"12.50 EUR"` and a date is `"2026-10-13"`.
- **csv:** `csv.parse(t) as list of T` reads one record per row, and the first row names the fields.
- **json and csv:** `encode` writes the other way, as compact JSON or as CSV with a header row.

**A result used as a plain value is an error:**

```agentmark invalid
> A result must be handled with try.
# Command: wrong
- args: answer: text

## Run
- set days = number.parse(answer)
- print: "{days}"
```

```text
wrong.agent.md:6
  - set days = number.parse(answer)
               ^^^^^^^^^^^^^^^^^^^^
  "number.parse" can fail (it returns "result of int"), so it has to be called with "try".
  Write "- try number.parse(...) -> value", optionally with an "otherwise:" branch.
```

#### Files and standard input

This command reads a sales CSV and writes the rows back as JSON:

```agentmark
> Reads a sales CSV (region, product, amount) and writes the rows back as JSON.
# Record: Sale
- region: text
- product: text
- amount: money

# Command: sales-to-json
- args: source: text

## Run
- try file read source -> content
  otherwise:
    - print: "Could not read {source}"
    - stop
- try csv.parse(content) as list of Sale -> sales
  otherwise:
    - print: "{source} is not a sales file"
    - stop
- file write path.join(path.parent(source), "sales.json"): json.encode(sales)
- print: "Wrote {count of sales} sales"
```

**Rules**
- Files are the built-in tool `file`, with effect `local`. `check` shows it as `local(file)`.
- It needs no entry in `tools.toml`. A `[file]` table there is error E347.
- Paths are used as written, so a relative path starts at the working directory.
- `- read input -> text` reads all of standard input, with effect `local(input)`.
- Every `file` call is written to the trace before it runs, so a test sees what a program touched even when it failed.

| statement | does |
|---|---|
| `file read <path> -> content` | the file as `text`; it can fail, so use `try` |
| `file write <path>: <text>` | writes exactly the text (replaces the file) |
| `file append <path>: <line>` | adds the line and a newline (creates the file) |
| `file list <folder> -> names` | `list of text`, sorted |
| `file rename <old> to <new>` | moves or renames; an error when `<new>` exists |
| `file exists <path> -> present` | `bool`, for a file or a folder |
| `file delete <path>` | a file or an empty folder |

A trace shows the calls like this:

```json
{"kind":"file","verb":"write","path":"report.txt"}
{"kind":"file","verb":"rename","path":"report.txt","to":"archive/report-1.txt"}
```

**Errors**

| code | message | fix |
|---|---|---|
| E340 | `The module "x" has no function "x".` | use one of the functions the message lists |
| E341 | `"x.x" can fail (it returns "T"), so it has to be called with "try".` or `"file x" can fail (the file may not exist), so it has to be written with "try".` | write `- try x.x(...) -> value`, optionally with an `otherwise:` branch |
| E342 | `json` or `csv` is used without a usable type, or with a handle (all 5 messages below the table) | write the type after the call, as in `csv.parse(content) as list of Sale` |
| E343 | `"x" is not a valid pattern: <reason>` or `"x" is not a date pattern. Use YYYY MM DD HH mm ss (for example "DD.MM.YYYY HH:mm") or % codes ("%Y-%m-%d").` | fix the pattern |
| E344 | `The built-in tool "file" has no verb "x".` | use one of the verbs the message lists |
| E345 | a `file` statement has the wrong shape; the message shows the right one (all 7 messages below the table) | write the statement in the form the message shows, such as `file rename <old> to <new>` |
| E347 | `"x" is a built-in tool, so it cannot be declared in tools.toml.` | remove the `[file]` table |

**E342 messages**

- `"x.x" needs to know what to read: write the type after the call.`
- `"x" cannot write T: a handle cannot be turned into text.`
- `"csv.encode" writes a list of records (one per row), but this is T.`
- `"x.parse" cannot read T: a handle comes from a library call, not from text.`
- `"csv.parse" reads a list of records (one per row), but "as T" is not one.`

**E345 messages**

- `"file x" takes only a path.`
- `"file x" needs the new path after "to", not after a colon.`
- `"file x" needs the text after a colon, not after "to".`
- `"file x" needs the new path: "to <new>".`
- `"file x" needs the text after a colon.`
- `"file x" gives something back: name it with "-> name".`
- `"file x" gives nothing back, so there is nothing to name.`

**Next:** [22.4 Opaque library types](#224-opaque-library-types)

### 22.4 Opaque library types

A **handle** is created by a library statement, used by method statements whose first word is the variable, and passed to your own functions:

```text
- sha2 sha256_new -> hasher
- hasher update "hello "
- hasher finalize -> digest
```

**Rules**
- When the binder meets a Rust type that has no AgentMark equivalent, it declares the type as `handle of <crate>.<Type>`. It binds the constructors and methods instead of skipping the API around them.
- A method that takes `self` by value consumes the handle. A later use is an error that names the line.
- A handle cannot be printed, compared, stored in a record or traced.
- The rules and the error messages are in section [15.4](#154-binding-file-format), and the workflow is in `docs/libraries.md`.
- Crate features come from the `[crates]` table of `agentmark.toml`.

**Next:** [22.5 User-defined tools](#225-user-defined-tools)

### 22.5 User-defined tools

A tool can be written in AgentMark itself. `# Tool: name` has an `- effect:` line and one `## verb(params) -> type` block per verb:

```agentmark
> Words for file names.
# Tool: slugger
- effect: pure

> Lower case, dashes between words: "Hello World" becomes "hello-world".
## slug(title: text) -> text
- set lowered = text.lower(title)
- return regex.replace(lowered, "[^a-z0-9]+", "-")

> Prints the file name of a title, twice: as an expression and as a statement.
# Command: file-name
- args: title: text

## Run
- print: "{slugger.slug(title)}.md"
- slugger slug title -> same
- print: "{same}.md"
```

**Rules**
- The block of a verb holds the statements of the verb.
- A tool is called like any tool, as a statement. When it is `pure`, it can also be called as an expression.
- The declared effect is checked against the body exactly like a `# Function`. A verb may not do more than its tool says.
- A call has the effect of what the body does, at least the effect the tool declares.
- `check` lists one line per verb, such as `ledger / add: local(file)`.
- A verb with a `rust` fence instead of statements is layer 1, like a `## Rust` block. Its effect is the declared one, and it is trusted.
- A tool that only uses other `# Tool` declarations and the built-in `file` tool needs no `tools.toml` and no `rust/connect.rs`.
- Both files stay for tools implemented outside the program, such as a real CRM or an email provider. A name cannot be used by both.
- A tool can be listed in an agent's `- tools:` like any other.

**How a call is written**

| form | when |
|---|---|
| `- tool verb a b -> result` | any tool. The words and values after the verb are the arguments, in the order of the parameters; `-> result` is optional. One more value may follow a colon (`- notes save "a.txt": body`). |
| `tool.verb(a, b)` | an expression, only for a `pure` tool whose verb returns a value. A tool of an imported folder is written with its module name (`tools.slugger.slug(title)`); the statement form takes the module too (`- tools.ledger add title`). |

**Verbs that call each other, and a Rust verb**

````agentmark
> A plain text ledger in the working directory.
# Tool: ledger
- effect: local

## add(entry: text)
- file append "ledger.txt": entry

> The same entry twice: a verb may call the verbs of its own tool.
## add_twice(entry: text, mark: text)
- ledger add entry
- ledger add "{mark} {entry}"

# Tool: caps
- effect: pure

> Written in Rust, which is what a verb may do when AgentMark has no word for it.
## shout(word: text) -> text
  ```rust
  word.to_uppercase()
  ```

> Writes the shouted title to the ledger.
# Command: record
- args: title: text

## Run
- set loud = caps.shout(title)
- ledger add_twice loud "again"
````

Every call is written to the trace before the body runs, like a tool call. `args` holds the verb's parameters by name:

```json
{"kind":"tool","tool":"ledger","verb":"add","args":{"entry":"HELLO"}}
```

**A verb that exceeds its tool's effect**

This is an error on the line that does it:

```agentmark invalid
> A pure tool cannot print.
# Tool: greeter
- effect: pure

## greet(name: text)
- print: "hello {name}"
```

```text
greeter.agent.md:6
  - print: "hello {name}"
    ^^^^^^^^^^^^^^^^^^^^^
  The tool "greeter" says "- effect: pure", but this line has the effect local (print).
  Change the tool to "- effect: local", or move this line out of the tool.
```

**Errors**

| code | message | fix |
|---|---|---|
| E206 | `The tool "x" says "- effect: pure", but this line has the effect <effect>.` | change the tool to `- effect: local`, or move the line out of the tool |
| E209 | `The verb "x" of "x" can fail (it returns "T"), so it has to be called with "try".` or `The verb "x" can fail (it returns "T"), so it cannot be used inside an expression.` | write `- try tool verb ... -> value`, optionally with an `otherwise:` branch |
| E380 | `The tool "x" has the same name as x.` | a tool name must be unique, so rename this one; it cannot be named like a tool of `tools.toml`, like `file`, like a binding, or like an agent, command or service |
| E381 | `The tool "x" has the verb "x" twice.` | remove or rename one |
| E382 | `The tool "x" has the effect T, so "x(...)" cannot be used inside an expression.` | call it as a statement: `- tool verb ... -> value` |
| E383 | `The tool x needs a verb.` or `Unknown verb "x" for tool x.` | use one of the verbs the message lists |
| E384 | `The tool "x" takes values, not "field = value" assignments.` | write the values after the verb: `- name verb a b`, and one value after a colon if you like |
| E385 | `The verb "x" of "x" returns nothing, so there is no result to store.` or `The verb "x" returns nothing, so it cannot be used as a value.` | call it as a statement |
| E386 | `The verb "x" of the tool "x" says it returns a value but never does.` | end the body with `- return <value>`, or remove the `-> type` from the heading |
| E387 | `The argument "x" would be the flag "--x", which every program already has, and this command is the program's default.` | rename the argument, for example `x_value` |
| W380 | `The tool "x" has the name of a library module, so "x.something(...)" is easy to misread.` | a call `x.f(...)` means this tool only when it has the verb `f`; consider another name |

**Next:** [22.6 Programs](#226-programs)

### 22.6 Programs

A project whose `# Run` lists a single `# Command` and no agent or service makes that command the binary's default:

```agentmark
> Says how many copies of a title it would print: `label <title> [<copies>] [--dry-run]`.
# Command: label
- args: title: text, copies: int = 1, dry_run: bool = false

## Run
- if dry_run:
  - print: "would print {title} x{copies}"
  - stop
- print: "{title} x{copies}"

# Run
- label
```

`label "Hello" 3`, `label --copies 3 --title Hello` and `label Hello --copies 3` do the same. `label Hello --dry-run` sets `dry_run`.

**Rules**
- `my-tool <args>` runs the default command without a subcommand name.
- Each argument is positional, in the order it is declared, or `--name value`. The two mix: the positional values go, in declared order, to the arguments not given by name.
- A `bool` argument is the flag `--name`.
- Too many values is a usage error. This includes a value for an argument already given by name.
- `text`, `int`, `money`, `date` and `datetime` arguments are written as they are. Money is written as `"12.50 EUR"`.
- Any other value is written as JSON: a list `["a","b"]`, an enum `{"kind":"Empty"}` or a record.
- A name written with underscores also takes dashes, so `--dry-run` and `--dry_run` both work.
- An argument with a default may be left out. A missing argument without a default is a usage error with exit code 2, and it shows the usage line.
- With several commands, or an agent or service next to the command, the commands stay subcommands, as in `my-tool import-csv …`.
- `agentmark run <project> -- <arguments>` and `agentmark trace <project> -- <arguments>` build the project and give the arguments after `--` to the program.

**Next:** [22.7 Examples and testing contract](#227-v03-examples-and-testing-contract)

### 22.7 v0.3 examples and testing contract

Five examples show the v0.3 language. None is an agent, and none has a `## Rust` block.

| example | shows |
|---|---|
| `06-sales-report` | CSV, `group by`, `sum of`, money, a report file |
| `07-file-renamer` | `file list`, patterns, dates, `file rename`, a `--dry-run` flag |
| `08-notes-api` | a REST service over SQLite through library handles |
| `09-markdown-to-html` | text and pattern functions, an `# Enum`, `match` |
| `10-log-analyzer` | `lines`, patterns, `group by`, `count of`, `take … from` |

The fixture folder of a program that is not an agent looks like this:

```text
fixtures/07-file-renamer/
  run.json                 {"args": ["photos"]}
  input/photos/            copied into a fresh working directory
  expected.stdout          what the program prints
  expected/photos/         compared with the working directory after the run
  variants/dry-run/        only what differs from the parent
```

**Rules**
- `run.json` holds `{"args": […]}` and an optional `"stdin"`.
- An `input/` folder is copied into a fresh working directory before the run.
- The expected results are `expected.stdout`, an optional `expected/` folder compared file by file with the working directory after the run, and an optional `expected.trace.jsonl`.
- A folder under `variants/<case>/` holds only what differs from its parent.
- Services use `requests.json`, where an entry may carry `expect_status` and `expect_body`.
- The program replays the requests with `--once --mock`. It exits with an error for the first answer whose status or body differs, per Q413.

### 22.8 Out of scope for v0.3

- Closures, first-class functions and user generics. Collection words never take a function value.
- Async user code. Handles to async library APIs are awaited by the generated code.
- Operator overloading. Every operator keeps the meaning of section [5](#5-expressions).

## Appendix A. Statement index

Every statement form of the language, with where it is explained. The [cheat sheet](#cheat-sheet) lists the most common ones.

| form | section |
|---|---|
| `for each <var> in <source>:` | [4.1](#41-control) |
| `for each <rule> <var> in <source>:` | [4.1](#41-control) |
| `if <condition>:` / `else:` | [4.1](#41-control) |
| `if <expr> is some <name>:` / `else:` | [14](#14-errors) |
| `stop` | [4.1](#41-control) |
| `do <name>` / `do <name>(args) -> var` | [4.1](#41-control), [13.1](#131-function) |
| `ask model[ with <var>]: "<prompt>"[ as <Record>] -> var` | [4.2](#42-model) |
| `<tool> <verb> [<target>]: <payload>[ -> var]` | [4.3](#43-tool-calls) |
| `guard: <condition>[ else <statement>]` | [4.4](#44-guards) |
| `set <var> = <expression>` | [4.5](#45-assignment) |
| `return <expr>` | [13.1](#131-function) |
| `respond <status>[ <expr>]` | [13.3](#133-service-and-command) |
| `read body as <Type> -> var` | [13.3](#133-service-and-command) |
| `read file <path> as <Type> -> var` | [13.3](#133-service-and-command) |
| `read input -> var` | [22.3](#223-the-built-in-library) |
| `file <verb> <path>[ to <new>\|: <text>][ -> var]` | [22.3](#223-the-built-in-library) |
| `try [do] <call>(args)[ as <Type>] -> var` (a built-in or user function that returns `result of T`) | [14](#14-errors), [22.3](#223-the-built-in-library) |
| `print: "<text>"` | [13.3](#133-service-and-command) |
| `<binding> <item> <arguments>[ -> var]` (a library call) | [15](#15-library-access-automatic-bindings) |
| `<handle> <method> [<arguments>][ -> var]` (a method on a library handle) | [15.4](#154-binding-file-format), [22.4](#224-opaque-library-types) |
| `<tool> <verb> <arguments>[: <value>][ -> var]` (a user-defined tool) | [22.5](#225-user-defined-tools) |
| `<tool>.<verb>(arguments)` (an expression, pure tools only) | [22.5](#225-user-defined-tools) |
| `put <value> at <key> in <map>` / `remove <key> from <map>` | [22.2](#222-collection-words) |
| `append <value> to <list>` | [13.3](#133-service-and-command), [22.2](#222-collection-words) |
| `for each <var> in <list> with index <i>:` / `for each <key>, <value> in <map>:` | [22.2](#222-collection-words) |
| `try <statement>` + `otherwise:` | [14](#14-errors) |
| `parallel:` | [16](#16-concurrency) |
| `match <value>:` with `<Variant> <name>:`, `some <name>:`, `none:`, `ok <name>:`, `error <name>:`, `otherwise:` | [22.1](#221-user-defined-types) |
| `## Rust: <name>[ -> <type>]` | [17](#17-escape-hatch--rust-blocks-layer-1) |

## Appendix B. Decisions the specification left open

These are the choices made while implementing, grouped by topic. Each one is written out in `OPEN_QUESTIONS.md`.

| topic | questions |
|---|---|
| syntax details (statement columns, comments, `else`, hyphens, money literals) | Q16 to Q22, Q27 to Q29, Q50 to Q53 |
| names, scope and imports | Q25, Q30, Q31, Q36, Q37, Q41 |
| types and units | Q7, Q8, Q13, Q14, Q32, Q40, Q151 |
| limits and style | Q33 to Q35, Q54 to Q56, Q111 |
| effects | Q38, Q39, Q81, Q112, Q116 |
| tool targets and the trace | Q4 to Q6, Q153, Q154 |
| model use | Q12, Q93, Q156, Q157 |
| binder | Q70 to Q82, Q110, Q117 |
| lowering and the IR | Q90 to Q95 |
| command line | Q113 to Q115 |
| fixtures | Q150 to Q158 |
| the built-in library and the `file` tool | Q340 to Q349 |
| tools written in AgentMark and the default command | Q380 to Q389 |
