# AgentMark language reference (v0.1)

This is the whole language in one file, written so that a person, an editor plugin or an AI assistant
can read it once and then write correct AgentMark. It follows `SPEC.md` section by section and in the
same order, so section numbers here are the specification's numbers (the specification has no
section 12). Where the specification left a question open, the choice that was made is stated in the
text and listed in `OPEN_QUESTIONS.md` as a `Q` number.

Every section starts with an example. Code blocks marked as programs are complete and are checked by
the test-suite (`tests/docs.rs`); pieces are shown without a heading line and are parsed there.
Whatever this file shows as output is the real output of the tools.

Rules that hold everywhere, so they are not repeated below:

* One statement is one line, at most 120 characters. The text between the quotes of a string does not
  count (Q35).
* Indentation is exactly 2 spaces per level. A tab is an error.
* A file ends in `.agent.md`. Names are case-sensitive, headings are not.

## 0. Architecture: three layers

A complete program that uses all three layers is the weekly report. The agent (layer 3) wakes up on
Monday, counts invoices with two rules, asks a model for two sentences, checks them and mails them.

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

| layer | what lives there | where it is described |
|---|---|---|
| 3. Agent layer | `# Agent`, `# Rule`, triggers (`## Every ...`), `ask model`, `guard` | sections 2 to 4 |
| 2. Core language | `# Record`, `# Function`, `# Service`, `# Command`, `# Constant`, control flow, `try`, bindings | sections 13 to 16 |
| 1. Escape hatch | `## Rust` blocks: verbatim Rust inside any block | section 17 |

Layer 3 is sugar over layer 2. Before the program is turned into Rust, every agent construct is
rewritten into layer 2: a rule becomes a pure function returning yes or no, a trigger block becomes an
*entry* that runs on a schedule or an event, a `## Do` block becomes a function named
`<agent>__<block>`, `ask model` becomes a model call, and a `guard` becomes "evaluate, write a trace
line, stop if it failed". After that step no agent, rule, trigger or guard remains (the code that does
it is `src/lower/agent_lower.rs`). The generator that writes Rust never sees an agent.

The rule for growing the language: a feature enters layer 2 only after it has appeared in layer 1
three times.

## 1. File structure

A file is a sequence of **declarations**. Each starts with a level-1 heading `# Kind: name`. Under it,
`- key: value` lines are **properties**, `## heading` lines start **blocks**, and the lines of a block
are **statements**.

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

* Declaration kinds: `Record`, `Rule`, `Agent`, `Function`, `Service`, `Command`, `Binding`,
  `Constant`, `Import`, `Run`. Anything else is an error that suggests the closest kind.
* Properties come before the first `##` heading. Statements sit at indentation 0 directly under
  a `##` heading, and every nested body (`for each`, `if`, `else`, `otherwise`, `parallel`) is indented
  by one more 2-space step (Q16). Function bodies are the exception: they are indented under the
  `- effect:` property, as section 13.1 shows.
* Statements are list items (`- `), except `otherwise:` and (in older files) bare `for each ...:` and
  `stop` lines. The formatter writes every statement as a list item (Q51).
* A line starting with `>` is a comment. A run of comment lines directly above a `#` or `##` heading
  is that heading's documentation. A run at the top of a file followed by a blank line is the
  documentation of the file. All other comments belong to the next statement, field or property
  (Q17, Q28).
* Identifiers are letters, digits, `-` and `_`, starting with a letter. Declaration, rule, record and
  type names may contain `-` (`very-overdue`). Variables, fields and paths inside expressions may not,
  because there `-` is subtraction and needs a space on both sides (`today - 1 month`, Q19, Q29).
* Blank lines are ignored. A line that is neither a heading, a comment, a `- ` item, a fence nor a
  control line is an error (Q22).

## 2. Declarations

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

A field without a type gets one from the **last word** of its name. Words are separated by `_`, `-` or
a change from lower to upper case (`dueDate`), and the whole word has to match (Q32):

| last word | type |
|---|---|
| `date`, `at`, `on` | `date` |
| `amount`, `price`, `total`, `fee` | `money` |
| `count`, `qty`, `number` | `int` |
| anything else | `text`, with warning W001 that prints the type it picked |

In the example, `shipped_at` is a `date`, `items_count` an `int`, `total` is `money`, and `remark` falls
back to `text` and produces the warning. `agentmark check` lists every guessed type (the warning itself goes to
standard error, the list to standard output):

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

Writing the type is always allowed and always clearer.

### 2.2 Rule

A rule is a named, pure yes/no question about one record. A rule has no side effects, and anything
effectful inside it is an error.

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

Inside `when:`, a bare field name (`due`) is a field of the record the rule is `for:`. A rule is used
by a filtered loop (`for each overdue invoice in ...`, section 4.1) and by `count of overdue invoices in
...` (section 5). `when:` must be a yes/no value.

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

| key | required | meaning |
|---|---|---|
| `model` | yes | the model driver string: `claude`, `claude:<model>`, `mock` (section 19) |
| `tools` | no | comma-separated tool names the agent may use (default: none) |
| `memory` | no | `none` (default) or `file`: keep state between runs |
| `timeout` | no | a duration; default `5 minutes` per block execution |

An agent may call only tools listed in `tools`; calling another is an error ("The tool "email" is not
listed in the tools of agent ..."). The same list limits reading a tool's source, as in
`for each ... in crm.invoices`. Services, commands and functions have no `tools` line and may use every
tool of the manifest; a function's declared `effect` is what bounds it (Q38).

## 3. Blocks (triggers)

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

| heading | fires when |
|---|---|
| `## Every <duration>` | periodically, for example `Every 30 minutes`, `Every 1 hour` |
| `## Every day at <HH:MM>` | daily at local time, 24-hour clock |
| `## Every <weekday> at <HH:MM>` | weekly; `Monday` to `Sunday` |
| `## When <tool> receives <event>` | an inbound event of a tool; the event word must be in the tool's `events` list |
| `## On start` | once when the program starts |
| `## Do <name>` | never by itself: a named action, called by `do <name>` |

* Duration units: `second`, `minute`, `hour`, `day`, `week`, `month`, `year`, singular or plural.
* Blocks of one agent run one after the other. Two triggers firing together are queued, never run in
  parallel.
* Schedules use naive local time without daylight-saving handling. `Every` fires an interval after
  the previous tick (Q15).
* The heading is the block's **label**. It is what the effect summary and the trace report, so the
  formatter rewrites a loosely typed heading into the canonical one (`## every  MONDAY at 08:00`
  becomes `## Every Monday at 08:00`, Q50).
* `## When tool receives event` binds the event's record to a variable named after the event
  (`ticket`). The record `Ticket` must be visible in the file (the event word, capitalised).
* A `## Do` block has no parameter list. It works on the record "in front of" its caller: the
  innermost `for each` record, or the event record. All callers must hand it the same record; the
  first caller's variable name becomes the name inside the block (Q30). A `## Do` block that nobody
  calls is a warning.

## 4. Statements

Statements are list items (`- `). Control statements end in `:` and own the more-indented lines below.

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

The control forms, each shown once in this section or as noted:

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

* `for each <var> in <source>:` runs the body once per item. The source is a tool source
  (`crm.invoices`), a list variable, or a list in `state`.
* `for each <rule> <var> in <source>:` keeps only the items for which the rule is true, and its body
  sees the record's fields as bare names (`id` above).
* `if <condition>:` with an optional `else:` directly after it, at the same indentation. The condition
  must be yes/no. `if <expr> is some <name>:` is the `maybe` unwrap (section 14).
* `do <name>` calls a `## Do` block of the same agent. `do f(args) -> x` calls a function
  (section 13.1).
* **`stop` ends the current block, not only the current loop iteration** (Q3). It is how an agent says
  "enough for this run". This is deliberate, and the third example shows it:

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

In this agent the first ticket that already has a priority ends the whole 15-minute run. That is the
specified behaviour; use a negated `if` around the rest of the body when you want to skip one item only.

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

```agentmark fragment
- ask model: "<prompt>" -> answer
- ask model: "<prompt>" as Qualification -> verdict
- ask model with lead: "<prompt>" -> answer
- ask model with lead: "<prompt>" as Qualification -> verdict
```

* `ask model` is effectful (class `model`). The result is `text` unless `as <Record>` is given; then it is
  a value of that record.
* `with <var>` passes a value as context. The request the model receives is the prompt text, then a
  blank line, then the context as JSON; without `with` it is the prompt alone (Q156). A mock model matches
  on the prompt prefix, so `with` does not disturb fixtures.
* A reply that is not valid JSON for the record (not JSON, a field missing, a field of the wrong type)
  is not a runtime error: `q matches Qualification` is then false, the guard fails, and the block stops
  (Q157). So always write the `matches` guard after `as <Record>`.
* The prompt may be a string with `{holes}` or the name of a constant of type text
  (`ask model with ticket: triage_prompt as Triage -> t`).
* The agent's `model:` line selects the driver (section 19). `ask model` also works outside an agent, in a function
  declared `effect: model`, in a route or in a command; there is simply no `model:` line to say which driver, and the
  IR records none for such a call (Q92).

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

The shape is `- <tool> <verb> [<target>]: <payload> [-> var]`.

| piece | forms |
|---|---|
| tool and verb | both from `tools.toml`; an unknown verb is an error with "Did you mean ...?". A tool without verbs, like `log`, is written `- log: "..."` |
| target | a word from the tool's `targets` (`email send customer`, `email send address "x@y.z"`), or the singular of one of its sources (`crm update invoice`), or a URL (`http get "https://..."`) |
| payload, text | `"Your invoice {amount} is overdue."` |
| payload, record | `{ id: invoice.id }` |
| payload, assignments | `status = "reminded"`, `score = q.score, status = "scored"` |
| payload, a value | `crm update invoice: inv` (a whole record) |
| result | `-> name` keeps the result of `get`/`list`/`post`, typed by the manifest when it can |

How target words are resolved (Q4, Q5):

* `email send customer` reads the field `customer_email` of the record in front of the block when it
  exists, otherwise `customer`. The trace reports the word `customer` as the target and the address as `to`.
* `crm update invoice` names the source `invoices` (the plural of the word) and takes the `id` of the
  record in front of the block. When several variables of that record are in scope the call is an error
  ("Which invoice?"); name the one you mean with a value after the word.
* A tool's name alone is never a value, so a field called `email` is a field.

Reading a tool's source, `for each ... in crm.invoices`, is an effect of that tool (`network(crm)`).

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

A guard is a pure check. If it fails and no `else` is given, the current block stops and the failure is logged with the
guard's source line. With `else <statement>`, that statement runs *instead of stopping*, and the block carries on
afterwards unless the statement ends it: `else respond 404` in a route does (section 13.3), `else stop` does, and
`else set reminder = "..."` as above does not, which makes it a way to fall back to a default when the check fails.
The trace line says `fail` in both cases.

Built-in helpers:

| helper | true when |
|---|---|
| `no amounts changed in <var>` | the model output holds no money value that was not in its input (the `with` value of the call that produced it, else its prompt). `1200 EUR` equals `1200.00 EUR` (Q13, Q93) |
| `<var> is not empty` | the text, list or `maybe` value has something in it. After the guard, a `maybe` value counts as present |
| `<var> matches <Record>` | the value has the shape of the record |
| `<var> length < <int>` | the length of a text or list; any comparison works (`>`, `<=`, `=`, ...) |

Every guard, in an agent, a service, a command or a function, writes one trace line with its source text
and `pass` or `fail`. Guards may use constants, which are inlined into that text (Q90). The lint rule
`unnamed-guard` asks for a `>` comment above a guard whose condition is longer than 60 characters.

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

`set <var> = <expression>` creates the variable or changes it. A `maybe T` variable may be given a plain `T`,
never the other way round (Q40).

## 5. Expressions

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

**Literals**

| form | example |
|---|---|
| text | `"text"`; escapes `\"`, `\\`, `\n`, `\{`, `\}` |
| int | `42` |
| money | `12.50 EUR`, `0 EUR`, `1200 JPY`; integer minor units, never a float (Q20) |
| date | `2026-10-13` (a date that does not exist is an error, never arithmetic) |
| duration | `14 days`, `1 month`, `15 minutes` |
| bool | `true`, `false` |

A decimal needs a currency (`3.5` is an error) and may not have more digits than the currency's minor unit
(`12.505 EUR` is an error): no silent rounding can enter a program (Q20).

**Names and fields.** `invoice.amount` is a field of a variable; a bare `amount` is a field of the record in
front of the block (inside `for each` over a record, in a rule, in an event block). A name resolves as: variable, bare
field, constant, and only with a dot a tool source `crm.invoices` (Q41).

**Interpolation.** In a string, `{customer}` and `{invoice.amount}` insert values. A hole holds a full expression.

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

**Built-ins:** `today`, `now`, `count of <source>` (list length), `count of <rule> <plural> in <source>`
(how many pass the rule), `first <var> in <source> where <condition>` (a `maybe` of the item).

**Units are real types.** Money, dates, durations and numbers combine only in sensible ways:

| expression | type |
|---|---|
| `money + money`, `money - money` | `money` (a currency mismatch between two written literals is an error; otherwise it is checked at run time) |
| `money * int`, `money / int` | `money`, rounded half away from zero at the currency's minor unit (Q7) |
| `date + duration`, `date - duration` | `date` |
| `date - date`, `datetime - datetime` | `duration` |
| `12 EUR + 3 days` | error: "Cannot add a duration to money" |
| `money / money`, `money * money`, `text + text` | error; use interpolation for text |

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

The built-in types:

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
| `result of <T>` | `Result<T, AgentError>`, function results only (section 14) |
| `handle of <crate>.<Type>` | the library's own `<crate>::<Type>` (v0.3, section 15.4) |

* A record's name is also a type. Record names are unique in a project (Q31).
* Any value that can be absent must be declared `maybe`. There is no null.
* Where a binding takes `bytes`, a `text` argument is accepted and means its UTF-8 bytes. No other
  position coerces (Q151).
* The type of a record literal `{ id: x }` is an object type; it is accepted wherever a record with those fields is.

## 7. Tool manifest

Tools are declared in `tools.toml` next to the source file (for a project: at its root). This is the
manifest every example project uses:

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

| key | meaning |
|---|---|
| table name (`[email]`) | the tool's name, as written in `tools:` and in statements |
| `verbs` | the words allowed after the tool name; an empty list means the tool is written `- log: "..."` |
| `targets` | words allowed after the verb (`email send customer`) |
| `sources` | names readable as `tool.source` (`crm.invoices`); each is plural, and its singular names a record (`Invoice`) and is also the target word of `update` (Q5) |
| `events` | names usable in `## When <tool> receives <event>`; each names a record (`lead` is `Lead`) |
| `effect` | `local`, `network` or `model`. `pure` is not allowed for a tool (Q40) |

An unknown tool, verb, target or source is an error with a suggestion; a tool the agent does not list in
`tools:` is an error (section 2.3). `agentmark.toml` is separate: it holds the project's `name`,
`version`, `targets` and `[lint]` settings (section 9).

The generated program defines one trait per tool, and the runtime ships `MockTools`, an implementation
that returns fixture data and records calls. The user supplies real implementations for production.

## 8. Effects

The report that matters most is the effect summary: for every block, everything it can touch. This is the
real report for the invoice service, which has no agent and no model call:

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

Every statement has an effect class, ordered from least to most reach: `pure`, `local`, `network`,
`model`. A fifth, `unverified`, marks calls into a library nobody approved yet (section 15.3).

| class | what it is | where it comes from |
|---|---|---|
| `pure` | computes from values it already has | expressions, rules, guards, reading `state`, `set`, `return`, `respond`, `read body`, `append` to a local list |
| `local` | touches this machine | a `local` tool (`log`), `print`, `read file` (source `file`), `append ... to state.<field>` (source `state`), a function or Rust block declared `local` |
| `network` | talks to another system | a `network` tool, reading a tool source such as `crm.invoices` |
| `model` | asks a model | `ask model` |

Rules and consequences:

* `# Rule` bodies and `guard` conditions must be `pure`. A rule that calls a `network` function is an error.
* A function's body must not exceed its declared `effect`. A function that calls an unapproved library
  cannot claim `pure`, and `check` says so.
* Rust blocks and bindings contribute their declared effect. It is trusted, because the checker cannot
  look inside Rust.
* The summary line is `unit / heading: model, network(email, crm), local(log)`: the class names the
  highest reach, the brackets name the tools in order of first use. Agents, services and commands print
  `unit / heading:`; functions and rules print `function name:` and `rule name:` (Q116).
* The summary is also written into the generated Rust file as `// Effects: ...` at the top.
* `agentmark check --json` prints the same facts as one JSON document for editors and CI; its layout is
  documented in `docs/check-json.md`.

## 9. Compilation

```console cwd=repo
$ agentmark fmt --check examples/01-invoice-chaser
5 files are canonical.
```

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
`run` and `trace` build the project into a cache folder, then run every entry once; `trace` prints the trace on stdout
(the folder `fixtures/01-invoice-chaser` holds the test data and the trace this run must produce):

```console cwd=repo
$ agentmark trace examples/01-invoice-chaser --mock --fixtures fixtures/01-invoice-chaser
{"kind":"model","agent":"invoice-chaser","block":"Every day at 09:00","prompt_prefix":"Write a short, polite payment reminder","context":{"id":"INV-1"}}
{"kind":"guard","agent":"invoice-chaser","name":"no amounts changed in reminder","result":"pass"}
{"kind":"guard","agent":"invoice-chaser","name":"reminder length < 800","result":"pass"}
{"kind":"tool","tool":"email","verb":"send","target":"customer","to":"ap@mueller.de"}
{"kind":"tool","tool":"crm","verb":"update","source":"invoices","id":"INV-1","set":{"status":"reminded"}}
{"kind":"tool","tool":"log","message":"Reminded Müller GmbH about 1200.00 EUR"}
```

`run` does not print the trace; the built-in `log` tool writes its lines to standard error (`[log] ...`), and `print`
writes to standard output unless the trace owns it:

```console cwd=repo stderr
$ agentmark run examples/01-invoice-chaser --mock --fixtures fixtures/01-invoice-chaser
[log] Reminded Müller GmbH about 1200.00 EUR
```

`build` only writes the Cargo project (`-o <dir>`, required) and then runs `cargo check` on it unless `--no-check` is given.

* The pipeline is `source -> lexer -> parser -> AST -> semantic check -> L2 lowering -> IR -> Rust code`. The IR is
  independent of the target. A new target is one new folder under `src/codegen/` (see `docs/internals.md`);
  only Rust exists in v0.1 (`--target rust`, the default).
* `check` exits 1 while an unverified library is left, unless `--allow-unverified` is given; the listing is printed
  either way (Q112). `build`, `run` and `trace` apply the same rule: they refuse unapproved libraries.
  With `--allow-unverified` (all four commands) an unverified library counts as its `proposed-effect` for the effect
  checks, so a `pure` function that calls a library proposed `pure` passes; each such library gets one warning
  (W008) saying so, and nothing is approved (Q203). Without the flag, such a function is an error.
* Exit codes: 0 fine; 1 the project has problems (errors, denied lint rules, files that are not canonical with
  `--check`, unapproved libraries, a failed bind); 2 the command itself cannot run (bad arguments,
  a path that does not exist or is not a `.agent.md` file, no model for `bind`). Diagnostics go to stderr, reports
  and counts to stdout (Q113).
* `check`, `build`, `run` and `trace` auto-bind: every `# Import: crate x` that has no binding file gets one before the
  program is checked, and `binding x: generated` is printed on standard error. Existing bindings are used as they are
  (Q137, Q201). If binding is impossible (no API key, no mock, offline without cached metadata) the message is
  `agentmark: cannot bind "x": <why>` and the program is checked without it; a crate that a statement or an agent's
  `tools:` line needs then gets error E219 on its import line.
* The generated program calls real systems through the project's own file `rust/connect.rs` (section 9.1).
* A project is a folder (normally with an `agentmark.toml` at its root). A single `.agent.md` file is also a project, with fewer rules (Q34).

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

Services and commands get a file each (an axum router, a clap subcommand). The generated code reads like the
source: one function per block, the block heading as its doc comment, and a comment
`// <file>.agent.md:<line>` before every statement. The `Cargo.toml` pins a library to the exact version in its
binding header (Q155).

The main program takes `--once` (run every scheduled entry once, in `# Run` order, then each event entry once per
fixture item), `--schedule` (run for real on the schedule), `--mock` with `--fixtures <dir>` (pretend tools and
model) and `--trace <file|->`.

**Real tools: `rust/connect.rs`.** In a real run (no `--mock`) every tool call goes through `Connected`, which `connect()` in the user's file
`rust/connect.rs` (beside `agentmark.toml`; next to the source file for a single-file project) builds. `build`, `run` and `trace` create the
file from a stub when it is missing (the stub's methods fail with `not connected yet: implement Crm::list in rust/connect.rs`), never overwrite
it, and copy it to `src/connect.rs` of the output on every build; with `--mock` it is used if it exists and never created (Q135). `log` is built in.
The example projects ship the stubs: `examples/01-invoice-chaser/rust/connect.rs`. `docs/libraries.md` section 7 shows one.

### 9.2 Runtime crate `agentmark-rt`

`crates/agentmark-rt` is in the same repository and provides: `Money` and `Currency`; the scheduler; the
`ModelClient` trait with an Anthropic implementation and `MockModel`; `MockTools` and the fixture loader; the
`Trace` recorder (one JSON line per tool or model call); and `Ctx`, which bundles them. Money text is
`"1200.00 EUR"`: EUR, USD, GBP, CHF with two decimals, JPY with none; parsing a number with more decimals than the
currency has is an error, never silent rounding (Q14).

## 10. Error messages

Every error points at a line and says what was expected, in plain words, with a caret under the offending
token, and a suggestion when a known word is within edit distance 2:

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

* The distance is Damerau-Levenshtein (a swapped pair of letters is one edit), at most 2 and less than the number
  of letters of the typed word. A word that already matches exactly is never a suggestion (Q26).
* The parser suggests only from closed sets of keywords (block headings, statement words, time units, type
  constructors). Tool, verb, target and field typos are the semantic check's job (Q27).
* A mistake in AgentMark source never shows a Rust compiler error. If the Rust written for a program does not
  build, that is reported as `internal compiler error: generated Rust did not compile` with the generated file, and
  is a bug of AgentMark. Errors inside your own `## Rust` or binding bodies are mapped back to the `.agent.md` line.
* JSON diagnostics (`check --json`) carry a stable `code`. Codes: `E0xx` syntax and structure (E001 tab, E002
  odd indentation, E003 unclosed fence, E004 unreadable line, E010 to E012 properties, E020 unknown declaration,
  E030 to E032 structure, E040 syntax, E050 import syntax), `E1xx` project files (E100 to E105 `tools.toml`, E110
  to E113 files and imports including cycles, E114 `agentmark.toml`), `E2xx` meaning (E221 model driver (unknown or unsupported provider), E200 duplicate name, E201
  unknown type, E202 unknown name, E203 type mismatch, E204 `maybe` not unwrapped, E205 effect in a pure place,
  E206 effect above the declared one, E207 unknown tool, verb, target or source, E208 tool not listed, E209
  call, E210 statement, E211 hard limit, E212 `parallel` write conflict, E213 `do`, E214 `# Run`, E215 agent
  properties, E216 constants, E217 duplicate field, E218 two services with state), and warnings `W001` guessed
  type, `W002` unused import, `W003` unused declaration, `W004` library without binding, `W005` unapproved
  library, `W006` several runnable declarations in one file, `W007` long line caused by string text, `W008` a library whose effect is assumed from its proposal (`--allow-unverified`). E219 is a crate import that has no binding although a statement or an agent's `tools:` line uses it. E220 is an on-demand library item the model could not bind (15.2). E360 to E366 are about handles (15.4): E360 an unknown handle type, E361 a handle used after it was used up, E362 a handle read where nothing takes it, E363 a handle in a list, a `maybe` or a record, E365 a mode word or `self` in the wrong place or a handle given twice to one call, E366 a `try … otherwise` that cannot leave when it makes a handle.

## 11. Testing contract

For every program there is a folder of fixtures that makes its behaviour reproducible without any real system.
This is the fixture folder of the invoice chaser:

```text
fixtures/01-invoice-chaser/
  crm.invoices.json        input data of the tool source crm.invoices
  model.json               canned model answers, plus what "today" is
  expected.trace.jsonl     the exact sequence of tool and model calls the agent must make
```

The input is one file per tool source, `<tool>.<source>.json` (`crm.invoices.json`). The same file also feeds
events: each item of `crm.leads.json` fires `## When crm receives lead` once (Q154). The model file answers
`ask model`; the longest matching prompt prefix wins:

```json file=fixtures/02-lead-qualifier/model.json
{
  "today": "2026-10-13",
  "responses": [
    {"prompt_prefix": "Score this inbound lead from 1 to 10", "response": "{\"score\": 9, \"reason\": \"Budget and timeline are stated.\"}"}
  ]
}
```

A service folder adds `requests.json` (`[{method, path, body?, expect_body?}]`), a command adds `command.json`
(`{command, args}`) and `state.<field>.json` seeds the application state (Q153). The program runs with the fixture
folder as its working directory. The expected trace of the invoice service shows all the kinds of line a service
and a command write:

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

The trace is JSON lines. Tests compare the parsed objects line by line, never generated source text:

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

`agent` is the name of the agent, service or command; `block` is the heading of the entry that started the run (a
`## Do` block called from `## Every day at 09:00` reports `Every day at 09:00`). Reading a source with `for each` is
not traced.

One mock response exists per prompt, so one run cannot take both branches of an `if` on a model answer. The
fixture folder covers the main path; every other branch lives in `variants/<case>/` with the same set of files,
and a test harness runs each as its own fixture folder (Q150). `cargo test` builds every example, runs it with
`--mock --fixtures` and compares the trace.

## 13. Core language: functions and modules (layer 2)

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

| line | meaning |
|---|---|
| `- in: name: Type, other: Type` | parameters (optional) |
| `- out: Type` | the result type (optional for a function that returns nothing) |
| `- effect: pure` | **mandatory**: `pure`, `local`, `network` or `model`. The checker verifies the body does not exceed it |
| indented lines | the body, under the `effect` line (Q16) |

* A function is called in an expression, `net_amount(invoice, 19)`, or as a statement, `- do net_amount(invoice, 19) -> n`.
* `return <expr>` ends the function with a value. A function with `out:` and no `return` is an error; `return` outside a
  function is an error, and so is `stop` inside one (Q40).
* Recursion is allowed. Closures are not (v0.1).
* Declared names are unique per project, whatever file they sit in (Q31).

### 13.2 Module and import

One file is one module; files in subfolders are nested modules. A file sees **only** what it imports.

```agentmark fragment
# Import: ./billing/rules.agent.md as billing
# Import: ./records/invoice.agent.md
# Import: ./rules/*.agent.md
# Import: ./billing as billing
# Import: crate serde_json
# Import: crate reqwest as http_client
```

* A name from an aliased import is written `billing.overdue(invoice)`: the checker looks up the full dotted name (Q37).
  A file that needs a record from an aliased folder also imports the record file plainly, because the parser accepts
  qualified names in expressions and calls but not in type positions.
* Crate imports make the library available to `## Rust` blocks and to bindings (section 15).
* Import cycles are an error with the cycle printed. Unused imports and unused declarations are warnings.
* A crate import without a binding file is warning W004 ("run `agentmark bind sha2`"), never an error (Q36).

### 13.3 Service and Command

Two more top-level declarations turn AgentMark into a general application language. No model is involved
unless the author writes `ask model`. This single file is a complete program with a service and a command:

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

A **service** has `listen:` (address and port) and `state:` (the record that holds its memory). Each `## METHOD /path`
block is a route; `GET`, `POST`, `PUT`, `PATCH` and `DELETE` exist, and each `{name}` in the path becomes a text variable of
the block. It lowers to an axum router over shared state.

A **command** has `args:` (`name: Type`, with an optional default `= value`) and exactly one `## Run` block. It lowers to a
clap subcommand. A command may use the `state` record of the project's single service; a second service with `state:`
is an error (Q9, Q41).

Statements for services and commands:

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

## 14. Errors

There is no exception mechanism. A statement that can fail is written with `try`:

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

Without `otherwise`, a failed `try` stops the current block and records the error in the trace (`error` line). With
`otherwise:` the indented lines run instead; `otherwise:` is the only child of its `try` line (Q18). The variable a
`try ... -> x` introduces is visible after the `try`. When `otherwise` neither assigns it nor stops, the type's default
value is used (Q94).

A function whose `out` is `result of <T>` returns `Result<T, AgentError>` in Rust, and its callers must use `try`:

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

Inside a `result of T` function, `return v` takes a plain `T` (it is wrapped in `Ok`), and a `try` without `otherwise` that
fails does not trace-and-stop as it does everywhere else: the error is handed to the caller. The caller writes
`try do f(x) -> h`; afterwards `h` is a plain `T`, also when `otherwise` assigns it (Q200). Calling such a function
without `try`, or inside an expression, is an error. A failure has exactly one source, a failing `try`.

`maybe` values are unwrapped the same way, with `is some`:

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

Outside an `is some`, `is not empty` or `guard`, using a `maybe` value where a plain one is needed is an error.
After `guard: x is not empty`, `x` counts as present for the rest of the block.

## 15. Library access: automatic bindings

**Principle: using a library must never require writing Rust.** A user who writes `# Import: crate printpdf` gets
statements such as `- pdf render doc -> bytes`. AgentMark generates and caches the **binding** itself. A binding is an
ordinary layer-2 file: reviewable, diffable and editable by hand. This one is written in the same format as the
generated ones:

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

A statement then reads `- pdf render title -> bytes`: the binding name, the item name, the arguments, the result.

Used from a program, with the binding file above in place, the call is an ordinary statement:

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

The form is `- <binding> <item> <arguments> -> var`; the effect of the call is the binding's `- effect:` (here `local(pdf)`).

### 15.1 Flow

```text
# Import: crate printpdf    -> is bindings/printpdf.agent.md there and current?
                               no  -> run the binder, write the file, continue
                               yes -> use it
```

Bindings live in `bindings/<crate>.agent.md` (or `.agentmark/bindings/`). `agentmark bind <crate>` runs the binder
explicitly; `agentmark bind --all` refreshes every binding. `check`, `build`, `run` and `trace` run it for a crate that has
no binding file yet, so the workflow is `# Import`, `check`, approve one line. `docs/libraries.md` walks through it with real output.

### 15.2 The binder

The binder is target-agnostic by design: `src/binder/rust/` reads the library's metadata (rustdoc JSON for Rust; a TypeScript
target would read `.d.ts` files) and `src/binder/core/` turns it into AgentMark declarations. Three tiers apply, in order, to
every public function and method. A public type of the library that AgentMark has no type for is a handle (15.4): its
constructors and methods are bound too, by rules and confirmed by the type checker, so `&mut self` receivers are no longer a reason
to skip.

| tier | condition | result |
|---|---|---|
| direct | every parameter and the return type map 1:1 onto AgentMark types, no generics, no closures | binding written by rules, no model call |
| assisted | generics with one obvious instantiation, `Result`/`Option` wrapping, `async` | the binder asks the model for the Rust body, then type-checks it against the declared signature before accepting it |
| skipped | traits with associated types, lifetimes in the public API, `unsafe`, closures, types AgentMark cannot hold, anything the assisted tier fails to type-check | listed under `## Not bound` with the reason |
| on-demand | the user writes a plain statement against an item listed under `## Not bound` | the binder writes the body from the statement and the rustdoc, saves it as `unverified`, and continues |

**The on-demand tier.** `check`, `build`, `run` and `trace` look for statements of the form `<library> <item> value ...`
whose item the library's binding lists under `## Not bound`. For each such item (once, however many statements use it) the binder asks
the model (the same `AGENTMARK_MODEL`) for the body, from the statement as written and the item's rustdoc. When the item has no
AgentMark signature (it was skipped, not assisted) the model proposes one; its parameter count must match the values in the statement.
The body is type-checked with the same checker as the assisted tier. Two attempts are made, the second told what was wrong with the first;
each is one line on stderr: `binding sha2: writing sha224_digest on demand… done`, or `… failed`, the second try marked `(try 2 of 2)`.
When every item succeeds, the binding file is rewritten once: the items move out of `## Not bound` into bound items (their `>` line ends in
"Written on demand; not reviewed yet."), `- effect:` becomes `unverified` and `- proposed-effect:` never goes down. The previous file is kept
as `.prev`. The build then continues under the rules of 15.3. When an item fails twice, the binding file is not touched, nothing is built, and
the error is E220 at the item's name in the author's statement:

```text
fingerprint.agent.md:8
  - sha2 sha224_digest payload -> digest
         ^^^^^^^^^^^^^
  Could not bind "sha224_digest" of sha2 on demand: the code the model wrote did not pass the type check, twice.
  Nothing was changed. Use a ## Rust block for "sha224_digest", or bind it by hand in bindings/sha2.agent.md. Details for a bug report: target/agentmark/ondemand-sha2-sha224_digest.log
```

The middle sentence depends on the cause: "the code the model wrote did not pass the type check, twice." (shown), "the model could not be
reached: <reason>." (API error, timeout, no answer), or "the signature the model chose does not fit your statement (1 value, 2 parameters)."
Several libraries are handled together: no binding file is changed unless every library succeeded. Only a statement in a file that imports the
library (`# Import: crate sha2`) counts. A hand-written binding (no generated header) is bound against the version in `Cargo.lock` next to
`bindings/`, else the rustdoc JSON supplied; if neither is known the error says so, it never guesses the newest.

If the tier cannot start at all (no model, no rustdoc), the command prints `agentmark: cannot write the library code for "sha2" on demand: <why>`
and stops with exit code 1 (`check` goes on to report the unknown item). Q230 to Q235 record the edge cases.

The binder never silently drops an item: everything is bound or listed as not bound. The precise edge cases are
decisions Q70 to Q82; for example, closures stay skipped because AgentMark has no function type (Q75).

### 15.3 Effects and trust

A generated binding is born `- effect: unverified`, with the binder's guess as `- proposed-effect: network` (from
heuristics: `std::fs` means `local`, `reqwest` or `tokio::net` means `network`, otherwise `pure`; Q81).

* `agentmark check` lists every unverified binding and refuses to build a release while any remain; `--allow-unverified`
  overrides for development: the library then counts as its `proposed-effect` for effect checks, with warning W008 (Q203).
* A person approves by editing that one line to `- effect: pure` (or `local`, `network`). It is once per crate version.
* A binding approved for version X is marked `unverified` again when the crate version changes.

### 15.4 Binding file format

A binding is a layer-2 declaration whose bodies are layer 1 Rust. Every body is the body of
`pub async fn <item>(<params>) -> Result<<ret>, Box<dyn std::error::Error + Send + Sync>>`: values arrive owned, the
body ends in `Ok(value)`, may use `?` and `.await`, and names the library by full path (Q70). Above every bound item
the binder writes one `>` line: the first sentence of the library's own documentation for it (Q80). The generated
header lets the binder see staleness:

```text
> generated by agentmark bind — printpdf 0.7.0 — 2026-10-13
> edit freely; re-run `agentmark bind printpdf --force` to regenerate
```

**Opaque handles (v0.3, SPEC 22.4).** A library type AgentMark has no equivalent for (a hasher, a client, a
response) is a **handle**: `handle of <crate>.<Type>`. The binder declares it where it appears in a signature and binds the constructors and the methods around it, instead of
skipping them (Q360 to Q369). The file shows how a call holds the handle:

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

A parameter named `self` comes first and makes the item a **method** of that handle type; its Rust name in the body is `self_`
(`self` is a keyword). A method is written with its own name (`update`) because the handle type tells methods apart; a constructor
is an ordinary item (`sha256_new`) and is called on the library, `- sha2 sha256_new -> hasher`. A method is called on the variable:

```text
- sha2 sha256_new -> hasher
- hasher update "abc"
- hasher update "def"
- hasher finalize -> digest
```

Rules (all compile errors, in the shape of section 10):

* A handle **moves**. A `consume` method, passing it to a function (`- do feed(hasher)`), and `set other = hasher` use it up; reading
  it later is E361 and names the line that used it up. A branch that only sometimes uses it up counts ("may have"); a handle that
  lives outside a loop cannot be used up inside it.
* It can be read only where something takes it: a method call, a function or binding argument, `set`, `return`. Printing,
  `{interpolating}`, comparing, putting it in a record or a request, or tracing it is E362.
* It is a single value: no `list of handle …`, `maybe handle …` or record field (E363). `try … -> h otherwise:` needs the
  `otherwise` branch to leave, since there is no handle to put in its place (E366).
* A function parameter or result of handle type owns the handle: the caller's variable is used up when it is passed. `mut` and
  `consume` belong to binding parameters only (E365).
* A library method that is `async` is awaited by the generated code: every binding body is the body of an `async fn`.
* The effect of a method call is the binding's, like any other binding call (section 15.3).
* A handle owns whatever the Rust value borrows. A library type with lifetimes (`Query<'q, DB>`), or with type parameters that
  have one instantiation in the crate as configured (`DB = Sqlite` when only the `sqlite` feature is on), is still a handle: the
  items around it go to the assisted tier, the model writes the glue (copying or leaking what the call borrows), and the binding
  carries a block with the Rust type it stands for:

````text
## Handle: sqlx.query.Query
  ```rust
  sqlx::query::Query<'static, sqlx::Sqlite, <sqlx::Sqlite as sqlx::Database>::Arguments<'static>>
  ```
````

* A Rust integer wider than `int` (`u64`, `usize`) converts with `try_from(..)?`, which fails the call on overflow.
* The features a library is bound and built with come from `agentmark.toml`, for the binder and the generated `Cargo.toml` alike:

```toml
[crates]
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio"] }
```

### 15.5 Sharing

Bindings are plain files, so a community registry is a git repository of `bindings/*.agent.md`. The file format needs
no change for that. It is out of scope for v0.1.

## 16. Concurrency

v0.1 runs blocks one after the other. The IR carries an effect graph so that a later version can run independent
pure and local statements concurrently. Explicit opt-in works now:

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

Statements inside `parallel:` must not write the same variable; the checker reports a conflict. All generated functions
are `async` and use `tokio` from the start.

## 17. Escape hatch: `## Rust` blocks (layer 1)

Any agent, service, command or function may contain a block `## Rust: <name>` whose body is a fenced `rust` block.
Variables in scope are available by name, and the block must end with a value of its declared type:

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

* The heading is `## Rust: <name>` or `## Rust: <name> -> <type>`. `- in:` lists the parameters and `- effect:` is
  **mandatory**. The effect is trusted: the checker cannot verify arbitrary Rust. `agentmark check` lists every Rust
  block in its summary so a reviewer sees where trust is extended.
* A Rust block defines a module-level function called `<name>`; the declaration it sits under only groups it (Q2).
  Any function in the project can call it like a function: `hash_payload(payload)`.
* The block is wrapped in its own `fn` with `#[allow(unused)]` and a source-mapping comment, so a Rust error inside it
  points at the `.agent.md` line.
* Rust blocks may `use` any crate imported with `# Import: crate`.

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

`# Run` is the only declaration allowed in `main.agent.md` besides imports. Each item is an imported `# Agent`,
`# Service` or `# Command`:

```agentmark file=examples/05-invoice-service/main.agent.md
# Import: ./commands/import-csv.agent.md
# Import: ./services/api.agent.md

> Entry point. The API runs as a service; the CSV import is a subcommand.
# Run
- api
- import-csv
```

The binary starts all listed agents and services; commands become subcommands. A project with no `# Run` compiles
to a library, not a binary. Unknown keys in `agentmark.toml` are errors with a suggestion (Q40). `[lsp] model_actions = true` lets the editor
server offer its optional model-backed actions (Q250); it is off by default.

### 18.2 Imports

```agentmark fragment
# Import: ./records/invoice.agent.md
# Import: ./rules/*.agent.md
# Import: ./billing as billing
# Import: crate printpdf
```

A file sees only what it imports; there is no global scope. Paths are relative to the importing file. A glob imports
every matching file. An import cycle is an error, and the cycle is printed.

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

### 18.4 Soft rules (`agentmark lint`)

Warnings by default. `agentmark.toml` can promote any to an error with `[lint] deny = ["..."]`; an unknown rule name is an
error with a suggestion.

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

| rule | fires on |
|---|---|
| `long-file` | a file over 150 lines |
| `magic-value` | a money, duration or URL literal used inline more than once in a file (zero amounts are exempt); name it with a `# Constant` |
| `unnamed-guard` | a guard whose condition is longer than 60 characters and has no `>` comment line above it |
| `duplicate-rule` | two rules with identical `when:` expressions, compared structurally across files |
| `prompt-in-loop` | `ask model` inside `for each` without a later `guard` on its result in the same loop body |
| `missing-doc` | an agent, service or command with no `>` description directly above its heading |
| `vague-name` | an identifier called `data`, `tmp`, `x`, `result` or `thing` where it is introduced |

`fmt` and `lint` skip `bindings/` (Q111).

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

Constants are pure, imported like anything else, and inlined at compile time. They can hold any literal: a number, a
text, money, a duration, a date.

### 18.6 Formatter

`agentmark fmt` rewrites a file into the one canonical form: 2-space indent, one blank line between declarations,
properties before blocks, imports sorted (local files first, then folders, then crates), trailing whitespace removed.
This file is not canonical:

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

and this is what `agentmark fmt messy.agent.md` writes:

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

There are no formatting options. A few spellings are fixed (Q51): statements are always list items, a block heading is
printed from its parsed trigger, money prints decimals only when not whole (`12.50 EUR`, `14 EUR`), durations are singular
for 1, and `do f()` prints as `do f`. Comments survive (Q52). `fmt --check` fails CI when a file is not canonical.

### 18.7 Documentation comments

A `>` comment immediately before a declaration or block is its documentation. `agentmark doc` collects these into one page
per project, so the text an operations manager reads is the text the author wrote next to the code:

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

`agentmark doc <crate>` prints the cheat sheet of a bound library instead (section 15). If the argument names something on disk it is a
project, otherwise a library; `--project` says where to look for `bindings/` (Q114).

## 19. Model providers

`- model:` is a driver string, never a vendor lock. The runtime's `ModelClient` trait has one implementation per provider and the
language is unaware of which is in use.

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

| string | implementation |
|---|---|
| `claude`, `claude:<model>` | Anthropic Messages API (key from `ANTHROPIC_API_KEY`; `haiku`, `sonnet`, `opus` or a full id) |
| `openai:<model>` | OpenAI Chat Completions (key from `OPENAI_API_KEY`; `OPENAI_BASE_URL` points it at a proxy) |
| `local:<model>` | any OpenAI-compatible local server (Ollama, LM Studio, vLLM); base URL from `agentmark.toml` |
| `mock`, `mock:<path>` | fixture-driven, for tests: `model.json` in `$AGENTMARK_FIXTURES` (default: the current folder), or the file named |
| `azure:<deployment>`, `bedrock:<model>` | enterprise routes: not supported in this version; `check` says so at the `- model:` line |

```agentmark invalid
> Enterprise routes are not supported in this version.
# Agent: bank
- model: azure:my-deployment
- tools: log

## On start
- log: "hello"
```

`check` rejects a driver it does not know the same way (`Unknown model provider "opnai". Did you mean "openai"?`), and `openai:` or
`local:` without a model name.

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

Which model answers is decided at run time, strongest first (Q240):

1. `AGENTMARK_MODEL=<driver>`: one driver for the whole program (`openai:gpt-4o`, `local:kat-coder`, `mock:<path to model.json>`).
2. `AGENTMARK_ENV=<name>`: the `[models]` entry of that name, again one driver for every agent. `AGENTMARK_ENV=dev` runs
   the program on the local model, `AGENTMARK_ENV=test` on `model.json`, without touching an `.agent.md` file. An environment that is not
   in the table is an error that lists the ones that are; `production` without an entry means "no override".
3. Otherwise each agent uses its own `- model:` line. Unset `AGENTMARK_ENV` is therefore not "production": nothing is overridden, so a
   `- model: openai:gpt-4o` agent keeps OpenAI even if `production = "claude"`. A plain `claude` entry keeps an agent's own Claude model
   (`claude:haiku` stays haiku) and moves agents of other providers to Claude.

The generated program carries the table; the variables are read when it runs. `agentmark bind` and the on-demand tier use the same
rules with the project's `agentmark.toml` (and Claude when nothing is set), so a company that forbids external APIs can run the
whole toolchain on a local model. The local server's address is, strongest first, `AGENTMARK_LOCAL_BASE_URL`, `[models.local] base_url`,
then `http://localhost:11434/v1` (Ollama); `LOCAL_API_KEY`, if set, is sent as a bearer token. The agent's own `model:` line
describes the program, not the toolchain (Q115).

## 20. v0.2 scope: editor and tooling

Nothing here changes the language. Built after v0.1 passes all its tests, in this order:

1. The on-demand binding tier (section 15.2, last row): built.
2. Model providers `openai`, `local` and the `[models]` table (section 19): built. `azure` and `bedrock` stay unsupported.
3. A language server, `agentmark-lsp`, in the same workspace, reusing the checker as a library: diagnostics with the same messages,
   completion, go-to-definition across imports, hover with a block's effect summary, format-on-save, code actions for the mechanical
   lint fixes.
4. A VS Code extension (`editors/vscode/`): grammar, language-server client, an `agentmark.toml` schema and a "what can this agent
   touch" panel that reads `check --json`.
5. A tree-sitter grammar (`editors/tree-sitter-agentmark/`).
6. AI-assistant compatibility: this file stays the single file an LLM needs, and `templates/` holds a `CLAUDE.md` and `.cursorrules`
   that point at it.

## 21. Out of scope for v0.2

* Automatic parallelism (explicit `parallel:` only).
* Several agents talking to each other.
* Targets other than Rust.
* Closures and generics in the core language.

## 22. v0.3: a general-purpose Markdown language

Section 22 of the specification turns the core (Layer 2) into a general-purpose language: user-defined types and `match`,
collection words, a built-in library, opaque library handles and user-defined tools. Each subsection below is added as the
construct lands in the compiler, with checked samples like every other section.

### 22.1 User-defined types

An `# Enum` lists variants; a variant may carry named fields. A `# Type` gives another name to a type: the two are
interchangeable. A value of a variant with fields is written `Circle { radius: 3 }`, a variant without fields is just its
name, `Empty`. `match` takes a value apart: one branch per variant, and inside a branch the name after the variant
(`c`) holds that variant's fields.

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

* The names of records, enums and types share one namespace. Two of them with the same name are an error.
* Variant names are unique inside one enum. Two enums may both have a variant `Empty`; the name then needs its enum,
  `Shape.Empty` (or `billing.Shape.Empty` through a module alias), and a bare `Empty` is an error that lists the choices.
  Inside a `match` the branches are looked up in the enum being matched, so they never need the qualifier.
* A variable or a constant with the same name as a variant wins over the variant.
* A variant's fields are given by name, each exactly once; a field of type `maybe` may be left out. A variant cannot have a
  field called `kind` (see the JSON form below).
* Enum values compare with `=` and `!=`. They cannot be written into text as a whole (`"{shape}"`) and have no fields of
  their own: read a field inside the branch of the variant that has it.
* An alias is replaced by what it names wherever it is written, so it never shows up in an error message or in the
  generated program. An alias that is defined in terms of itself is an error.

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

* On a `maybe`, the branches are `some x` and `none`. On a `result`, they are `ok x` and `error e`; `e` is the failure's
  message as text. A `result` is matched at the call, as above, or stored first with `- set parsed = read_count(path)` and matched once; nothing else can read a stored result (Q300).
A `maybe` value is written `some x` or `none`. In a branch the name after the variant (or after `ok` / `error`) is optional when the
value is not used: `- error:`. A function that returns `result of T` is also used with `try`, without `do` (`try do f(x) -> v` means
the same), and `if ... is some x:` works on any `maybe`
expression, including a call. A record is built the same way as a variant, with every field that is not `maybe` given:

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

* Every case must be handled, or an `- otherwise:` branch must say what happens to the rest. `otherwise:` is the last
  branch; one that can never run is a warning. The error names what is missing:

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

* A misspelled variant in a branch or in a value gets the usual suggestion: `Shape has no variant "Circel". Did you mean
  "Circle"?`
* `match` is a statement. It is pure unless its branches are not, and it counts as one level of nesting (the limit of 3)
  whatever the number of branches.

**JSON.** An enum value is an object whose `kind` field names the variant, followed by the variant's fields:
`{"kind":"Circle","radius":3}`, `{"kind":"Empty"}` (Q301). That is what `read file`, `read body` and `respond` read and write.
### 22.2 Collection words

Lists are `list of T`, maps are `map of K to V` (keys: `text`, `int`, `date`, `datetime` or `bool`). The words below are
expressions unless marked as statements. They are pure: they build a new value and never change their input.

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

Inside the expression after `where`, `by`, `to` and `of … in`, the current element is called `item`, and when it is a record its
fields are in scope by their bare names, as in a `for each` over a record. `rows where region = "EU"` and
`rows where item.region = "EU"` mean the same. For a list of text or numbers only `item` exists:
`names where item != "a"`.

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

Statements:

| statement | meaning |
|---|---|
| `append x to rows` | add to the end of a list |
| `put v at k in m` | set the entry of key `k` (replaces an existing one) |
| `remove k from m` | delete the entry of key `k`; nothing happens when it is absent |
| `for each row in rows with index i:` | `i` counts the position from 1 |
| `for each region, total in totals:` | the entries of a map, in key order |

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

Results that can be empty are `maybe` values, so they are opened with `is some` before they are used:

```agentmark fragment
- if max of qty in sales is some most:
  - print: "largest quantity {most}"
- if totals at "EU" is empty:
  - print: "nothing sold in the EU"
```

How the words read, in the places where a reader could hesitate:

* **A source is a name, a call or a parenthesis.** `sort (rows where amount > 5 EUR) by amount` needs the parentheses; `rows
  where …` and `totals at key` hang off a name or a call. `keys of` and `values of` also stand where a source is expected:
  `sum of values of totals`.
* **A literal takes every comma after it.** `f(list of 1, 2, 3)` passes one list; write `f((list of 1, 2), 3)` for two
  arguments. `list of Sale` after `as` or `-> ` is a type, not a literal; the position decides.
* **`where` runs to the end of the expression**, so `rows where a = 1 and b = 2` filters on both.
* **Money.** `sum of`, `sort` and `min`/`max` over money need one currency: two currencies are a run-time error
  (`currency mismatch: EUR vs USD`), never a guess. The sum of an empty list of money is `0 EUR`.
* **Maps** iterate, print and serialise in key order; as JSON a map is an object (`{"DE": 1, "EU": 2}`).
* **Purity.** The expression that runs per element may read values and call pure functions, nothing else.
  `names where shout(item) = "a"` is an error when `shout` prints; use a `for each` loop for that.
* **Names.** A variable that has the name of a field of the element is an error inside the word (`set region = "US"`, then `sales where region = "EU"`): write `item.region` for the field, or rename the variable. A variable called `item` is hidden by the element inside the expression. Inside a word, `item` is the innermost element: `nested where (count of item where item > 1) > 1` counts the
  inner `item` in the second one. A parameter or loop variable that is appended to is a copy; the caller keeps its list.

```agentmark invalid
# Function: f
- effect: pure
  - set mixed = list of 1, "two"
```

Decisions behind these rules are `Q320` to `Q328` in `OPEN_QUESTIONS.md`; the errors are `E320` to `E329`. `append`, `put` and `remove` change a variable or a `state.…` path, never a computed value; `put` and `remove` are statement words, so a tool cannot have those names.

### 22.3 The built-in library

The library is part of the language, not a binding. Its functions are written `module.name(arguments)`, are all
`pure`, and need no `# Import`. Eight modules:

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

`agentmark check` lists the same table (a wrong name says which functions the module has), and an editor completes
it after `text.`. What each function gives:

* `text`: `split` the pieces between `sep`; `lines` like Rust `str::lines` (no empty entry for a final newline);
  `replace` every occurrence; `length` counts characters; `pad` adds spaces on the right up to `width` characters and
  never cuts longer text; `repeat` writes `t` `n` times.
* `regex` uses the Rust `regex` syntax and finds a match anywhere in the text. `matches` is a `bool`; `find` is
  `maybe text`; `captures` is `maybe list of text`: the capture groups of the first match, without the whole match
  (a group that took no part is empty text); `replace` replaces every match, `$1` standing for group 1. A literal
  pattern that does not compile is an error when you check the program.
* `number` works on `int` (and `min`, `max`, `abs` and `round` also on `money` of one currency). `round(x, places)`
  rounds half away from zero: for money to `places` decimals (`round(12.35 EUR, 1)` is `12.40 EUR`), for a whole
  number only a negative `places` changes it (`round(1235, -1)` is `1240`). `format(x, places)` writes `places`
  decimals (`format(12, 2)` is `12.00`).
* `money.format(m)` writes the amount like a literal (`12.50 EUR`); `money.parse` reads one.
* `date.format(d, pattern)` and `date.parse(t, pattern)`: a pattern that contains a `%` is strftime
  (`%Y-%m-%d`, `%Y%m%d`); any other pattern is written with `YYYY` `YY` `MMMM` `MMM` `MM` `DD` `dddd` `ddd` `HH` `mm`
  `ss`, other characters stand for themselves and `[text]` protects letters (`DD.MM.YYYY HH:mm`). `format` also takes a
  `datetime`. `days_between(a, b)` is the number of days from `a` to `b`, negative when `b` is earlier.
* `path` works on text only: `extension("a/b.jpg")` is `jpg` (no dot, empty when there is none), `parent("a/b.jpg")` is `a`.
* `json.parse(t) as T` reads JSON into a record, a list or any type (field names are the JSON keys; money is text like
  `"12.50 EUR"`, a date `"2026-10-13"`). `csv.parse(t) as list of T` reads one record per row, the first row naming the
  fields. `encode` writes the other way: compact JSON, or CSV with a header row.

A function that can fail returns `result of T` (`number.parse`, `money.parse`, `date.parse`, `json.parse`, `csv.parse`).
Call it with `try`, which takes the call directly (a leading `do` is allowed), or with `match` (section 22.1); calling
it any other way is error E341. After `try` the variable has the plain `T`:

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

The error for a result used as a plain value:

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

Files are the built-in tool `file`, with effect `local` (shown as `local(file)` by `check`). It needs no entry in
`tools.toml` (a `[file]` table there is error E347). Paths are used as written, so a relative path starts at the working directory.

| statement | does |
|---|---|
| `file read <path> -> content` | the file as `text`; it can fail, so use `try` |
| `file write <path>: <text>` | writes exactly the text (replaces the file) |
| `file append <path>: <line>` | adds the line and a newline (creates the file) |
| `file list <folder> -> names` | `list of text`, sorted |
| `file rename <old> to <new>` | moves or renames; an error when `<new>` exists |
| `file exists <path> -> present` | `bool`, for a file or a folder |
| `file delete <path>` | a file or an empty folder |

`- read input -> text` reads all of standard input (effect `local(input)`). Every `file` call is written to the trace
before it runs, so a test sees what a program touched even when it failed:

```json
{"kind":"file","verb":"write","path":"report.txt"}
{"kind":"file","verb":"rename","path":"report.txt","to":"archive/report-1.txt"}
```

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

Library errors have plain messages that name the value (`"4x" is not a whole number`, `line 3, column 2: …`
for a CSV cell, `cannot read "x.txt": it does not exist`). Error codes: E340 unknown function, E341 result without
`try`, E342 `json.parse` / `csv.parse` without a usable `as` type, E343 a literal pattern that cannot work, E344 unknown
`file` verb, E345 a `file` statement with a missing or extra part, E347 `[file]` in `tools.toml`.

### 22.5 User-defined tools

A tool can be written in AgentMark itself. `# Tool: name` has an `- effect:` line and one `## verb(params) -> type` block
per verb; the block holds the statements of the verb. The tool is called like any tool, as a statement, or, when it is
`pure`, as an expression:

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

How a call is written:

| form | when |
|---|---|
| `- tool verb a b -> result` | any tool. The words and values after the verb are the arguments, in the order of the parameters; `-> result` is optional. One more value may follow a colon (`- notes save "a.txt": body`). |
| `tool.verb(a, b)` | an expression, only for a `pure` tool whose verb returns a value. A tool of an imported folder is written with its module name (`tools.slugger.slug(title)`); the statement form takes the module too (`- tools.ledger add title`). |

The declared effect is checked against the body exactly like a `# Function`: a verb may not do more than its tool says, and
a call has the effect of what the body does, at least the effect the tool declares. `check` lists one line per verb
(`ledger / add: local(file)`). A verb with a `rust` fence instead of statements is Layer 1, like a `## Rust` block: its
effect is the declared one and is trusted:

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

A tool that only uses other `# Tool` declarations and the built-in `file` tool needs no `tools.toml` and no
`rust/connect.rs`: both stay for tools implemented outside the program (a real CRM, an email provider), and a name cannot
be used by both. Every call is written to the trace, before the body runs, like a tool call:

```json
{"kind":"tool","tool":"ledger","verb":"add","args":{"entry":"HELLO"}}
```

`args` holds the verb's parameters by name. A tool can be listed in an agent's `- tools:` like any other.

A verb that exceeds the effect of its tool is an error on the line that does it:

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

Error codes: E380 a tool named like a tool of `tools.toml`, like `file` or like a binding, E381 a verb twice in one tool, E382
an expression call of a tool that is not `pure`, E383 an unknown verb (or none), E384 `field = value` after a tool
statement, E385 the result of a verb that returns nothing, E386 a verb that says it returns a value and never does, E387 an argument of the default command named like a flag every program has (`--once`, `--mock`, …). A tool named like a library module is warned about (W380); a tool also cannot be named like an agent, command, service or binding (E380). The
effect error is E206, as for functions.

### 22.6 Programs

A project whose `# Run` lists a single `# Command` (and no agent or service) makes that command the binary's default:
`my-tool <args>` runs it without a subcommand name. Each argument is positional, in the order it is declared, or
`--name value`, and the two mix: the positional values go, in declared order, to the arguments not given by name. A `bool` argument is the flag `--name`. Too many values (including one for an argument already given by name) is a usage error. `text`, `int`, `money` (`"12.50 EUR"`), `date` and `datetime` arguments are written as they are; any other value (a list `["a","b"]`, an enum `{"kind":"Empty"}`, a record) is written as JSON. A name written with underscores also takes dashes (`--dry-run`,
`--dry_run`). An argument with a default may be left out:

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

`label "Hello" 3`, `label --copies 3 --title Hello` and `label Hello --copies 3` do the same; `label Hello --dry-run`
sets `dry_run`. A missing argument without a default is a usage error (exit code 2) that shows the usage line. With
several commands, or an agent or service next to the command, the commands stay subcommands (`my-tool import-csv …`).
`agentmark run <project> -- <arguments>` and `agentmark trace <project> -- <arguments>` build the project and give the
arguments after `--` to the program.

## Appendix A. Statement index

Every statement form of the language, with where it is explained:

| form | section |
|---|---|
| `for each <var> in <source>:` | 4.1 |
| `for each <rule> <var> in <source>:` | 4.1 |
| `if <condition>:` / `else:` | 4.1 |
| `if <expr> is some <name>:` / `else:` | 14 |
| `stop` | 4.1 |
| `do <name>` / `do <name>(args) -> var` | 4.1, 13.1 |
| `ask model[ with <var>]: "<prompt>"[ as <Record>] -> var` | 4.2 |
| `<tool> <verb> [<target>]: <payload>[ -> var]` | 4.3 |
| `guard: <condition>[ else <statement>]` | 4.4 |
| `set <var> = <expression>` | 4.5 |
| `return <expr>` | 13.1 |
| `respond <status>[ <expr>]` | 13.3 |
| `read body as <Type> -> var` | 13.3 |
| `read file <path> as <Type> -> var` | 13.3 |
| `append <value> to <list>` | 13.3 |
| `read input -> var` | 22.3 |
| `file <verb> <path>[ to <new>\|: <text>][ -> var]` | 22.3 |
| `try [do] <module.function>(args)[ as <Type>] -> var` | 22.3 |
| `print: "<text>"` | 13.3 |
| `<binding> <item> <arguments>[ -> var]` (a library call) | 15 |
| `try <statement>` + `otherwise:` | 14 |
| `parallel:` | 16 |
| `match <value>:` with `<Variant> <name>:`, `some <name>:`, `none:`, `ok <name>:`, `error <name>:`, `otherwise:` | 22.1 |
| `## Rust: <name>[ -> <type>]` | 17 |

## Appendix B. Decisions the specification left open

The choices made while implementing, by topic. Each is written out in `OPEN_QUESTIONS.md`.

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
