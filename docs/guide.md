# AgentMark guide

*For anyone who runs a business and has never written a program.*

By the end of this page you will have written a small agent that finds overdue invoices, asks an AI
to write a polite reminder, double-checks what the AI wrote, and sends it. You will also know how to
read the "what can this agent touch?" report, which is the part a careful manager cares about most.

It takes about ten minutes. You need:

* a text editor (Notepad, TextEdit, anything),
* the `agentmark` program installed on your computer,
* an empty folder to work in.

Everything you write goes in files that end in `.agent.md`. They are ordinary Markdown: headings,
bullet points, and plain sentences. If you can write a checklist, you can read and write them.

## 1. Describe the thing you keep track of

An agent works with *things*: invoices, leads, tickets. The first step is to say what an invoice looks
like. In AgentMark that is a **record**. Create a file called `invoice-chaser.agent.md` and type this:

```agentmark name=invoice-chaser.agent.md
> An invoice as stored in the CRM.
# Record: Invoice
- id: text
- customer: text
- customer_email: text
- amount: money
- due: date
- status: text
```

Read it out loud. "A record called Invoice. It has an id, a customer, a customer email, an amount of
money, a due date and a status." That is all it says.

* A line starting with `>` is a comment. It is for people, and the computer ignores it. The comment
  right above a heading is the description of that heading.
* A line starting with `#` begins a new part of the file. The word before the colon says what kind
  of part it is, the word after says what it is called.
* Lines starting with `-` are the details.
* After each name comes its kind of value: `text` for words, `money` for an amount with a currency
  (`1200.00 EUR`), `date` for a day (`2026-10-13`). There are a few more: `int` for whole numbers,
  `bool` for yes or no, `datetime` for a day and a time, and `duration` for a length of time such
  as `14 days`.

If you leave the kind out, AgentMark guesses it from the last word of the name. For example, a name ending in `date`, `at` or `on`
is a date, one ending in `amount`, `price`, `total` or `fee` is money, one ending in `count`, `qty` or
`number` is a whole number, and everything else is text. When it cannot tell, it says so and tells you which kind it picked. Writing
the kind yourself is always the clearer choice.

## 2. Say what "overdue" means

A **rule** is a question about one record that can only be answered "yes" or "no". Rules never do
anything. They only look. Add this to the same file:

```agentmark name=invoice-chaser.agent.md
> An invoice as stored in the CRM.
# Record: Invoice
- id: text
- customer: text
- customer_email: text
- amount: money
- due: date
- status: text

> Unpaid and more than 14 days past due.
# Rule: overdue
- for: Invoice
- when: today > due + 14 days and status != "paid"
```

"The rule `overdue` is about an Invoice. It is true when today is later than the due date plus 14 days,
and the status is not paid." Amounts of time are real values, so `due + 14 days` works and
`due + amount` would not (more on mistakes in section 7).

Now run the first **safety check**. In your folder, in a terminal, type:

```console
$ agentmark check invoice-chaser.agent.md
Effects (what each block can touch)
  rule overdue: pure

ok: 0 errors, 1 warning.
```

The report starts with "what each block can touch". The word `pure` means: it only looks at data it
already has and touches nothing. The warning is a polite hint that nothing uses the rule yet.
We fix that next.

## 3. Write an agent that only reads

An **agent** is the worker. It has a name, an AI model to ask, and a list of **tools** it may use.
A tool is a system the agent can reach: your CRM, your email, a log. The agent can use *only* the
tools on its list. Which tools exist at all, and what each one can do, is written once by whoever set
up your computer, in a file called `tools.toml` next to your agent. A typical one looks like this:

```toml file=examples/tools.toml
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

You do not need to change that file. Read it as a menu: the CRM can be asked to `list`, `get` and
`update` things, email can only `send`, the log writes a line. Next to each is the kind of reach it
has: `network` means it talks to another system, `local` means it stays on this computer.

Now add the agent. This first version is harmless on purpose: it only writes to the log.

```agentmark name=invoice-chaser.agent.md
> An invoice as stored in the CRM.
# Record: Invoice
- id: text
- customer: text
- customer_email: text
- amount: money
- due: date
- status: text

> Unpaid and more than 14 days past due.
# Rule: overdue
- for: Invoice
- when: today > due + 14 days and status != "paid"

> Writes a line to the log for every overdue invoice. Changes nothing.
# Agent: invoice-chaser
- model: claude
- tools: crm, log

## Every day at 09:00
- for each overdue invoice in crm.invoices:
  - log: "{customer} is overdue: {amount}"
```

Read the agent part like a job description.

* **`model: claude`** is the AI it may ask questions. This version does not ask any yet.
* **`tools: crm, log`** is everything it may use. Nothing else.
* **`## Every day at 09:00`** is a **trigger**: when the work starts. Other triggers are
  `## Every 15 minutes`, `## Every Monday at 08:00`, `## On start` (once, when the program
  starts) and `## When crm receives lead` (when the CRM reports a new lead).
* The lines under the trigger are the **steps**. `for each overdue invoice in crm.invoices:` means
  "take every invoice from the CRM, keep only those the rule `overdue` says yes to, and for each do
  what is indented below".
* `log: "..."` writes one line. Inside the quotes, `{customer}` and `{amount}` are filled in with
  the values of the invoice at hand.

Check it again:

```console
$ agentmark check invoice-chaser.agent.md
Effects (what each block can touch)
  rule overdue: pure
  invoice-chaser / Every day at 09:00: network(crm), local(log)

ok: 0 errors, 0 warnings.
```

This is the report you show a colleague. In one line it says: this agent, at 09:00 each day, can reach
the CRM over the network and write to the log, and nothing else. It cannot send email, because
`email` is not on its list.

## 4. Let it write the reminder, safely

Now the real work. Replace the agent part with this version:

```agentmark name=invoice-chaser.agent.md
> An invoice as stored in the CRM.
# Record: Invoice
- id: text
- customer: text
- customer_email: text
- amount: money
- due: date
- status: text

> Unpaid and more than 14 days past due.
# Rule: overdue
- for: Invoice
- when: today > due + 14 days and status != "paid"

> Chases unpaid invoices once a day. Never changes amounts; only sends reminders.
# Agent: invoice-chaser
- model: claude
- tools: email, crm, log

## Every day at 09:00
- for each overdue invoice in crm.invoices:
  - do remind

## Do remind
- ask model with invoice: "Write a short, polite payment reminder. Keep the amount exactly as given." -> reminder
- guard: no amounts changed in reminder
- guard: reminder length < 800
- email send customer: "{reminder}"
- crm update invoice: status = "reminded"
- log: "Reminded {customer} about {amount}"
```

New pieces, one by one:

* **`## Do remind`** is a named block of steps, like a recipe card. `do remind` in the trigger says
  "now follow that card, for this invoice". It keeps the trigger short and the card readable.
* **`ask model with invoice: "..." -> reminder`** asks the AI. It is shown the invoice, plus your
  instruction in quotes. Whatever it answers is kept under the name `reminder`.
* **`guard:`** is a **safety check on what the AI wrote**. The AI is clever but not careful, so before
  anything leaves the building the agent checks the answer:
  * `no amounts changed in reminder`: every amount of money in the text must also be in the invoice.
    If the AI wrote "1,500 EUR" where the invoice says 1200 EUR, the check fails.
  * `reminder length < 800`: the text must be shorter than 800 characters.

  If a guard fails, the agent stops that work, does not send anything, and writes down which guard
  failed and on which line. That is the point: a wrong answer from the AI costs you a log entry, not
  an embarrassing email.
* **`email send customer: "{reminder}"`** sends the text. `customer` is a *target* from the menu in
  `tools.toml`: the agent mails the customer's email address from the invoice.
* **`crm update invoice: status = "reminded"`** marks the invoice, so it is not chased twice.
* The last line logs what happened.

Check it:

```console
$ agentmark check invoice-chaser.agent.md
Effects (what each block can touch)
  rule overdue: pure
  invoice-chaser / Every day at 09:00: model, network(crm, email), local(log)
  invoice-chaser / Do remind: model, network(email, crm), local(log)

ok: 0 errors, 0 warnings.
```

## 5. Read the "what can it touch" report

You have now seen the report three times. Here is how to read any of them. Each line is one block of
work. After the colon is everything that block can reach:

| it says | in plain words |
|---|---|
| `pure` | only looks at what it already has |
| `local(log)` | writes on this computer: here, the log. Files and printing show up the same way |
| `network(crm, email)` | talks to other systems: here, the CRM and email |
| `model` | asks the AI |

The words in brackets are the tools. This is the whole idea of AgentMark: you do not have to read the
steps to know what an agent can do. The report is produced by the program from the steps, so it cannot
be out of date, and an agent cannot reach a tool that is not on its list.

When you review an agent someone else wrote, read the report first, then the description comments, and
only then the steps. Questions worth asking:

* Does it need `email`? Anything that sends is the first thing to look at.
* Is there a `guard` after every `ask model`?
* Does the description (the `>` comment) match what the report says?

There is a command for the last question. It prints the descriptions and the reach of every part as
one readable page, built from the same comments the author wrote next to the steps:

```console
$ agentmark doc invoice-chaser.agent.md
# invoice-chaser

## Agents

### invoice-chaser

Chases unpaid invoices once a day. Never changes amounts; only sends reminders.

- model: claude
- tools: email, crm, log
- can touch: model, network(crm, email), local(log)

**Every day at 09:00** (can touch: model, network(crm, email), local(log))

**Do remind** (can touch: model, network(email, crm), local(log))


## Records

### Invoice

An invoice as stored in the CRM.

- id: text
- customer: text
- customer_email: text
- amount: money
- due: date
- status: text


## Rules

### overdue

Unpaid and more than 14 days past due.

Applies to Invoice. Always pure.
```

## 6. Try it on pretend data

Before an agent touches real customers, run it against **test data**. Make a folder `fixtures` next to
your file with two small files. The first is a pretend CRM with three invoices:

```json file=fixtures/01-invoice-chaser/crm.invoices.json
[
  {"id":"INV-1","customer":"Müller GmbH","customer_email":"ap@mueller.de","amount":"1200.00 EUR","due":"2026-09-01","status":"open"},
  {"id":"INV-2","customer":"Schmidt AG","customer_email":"buch@schmidt.de","amount":"450.00 EUR","due":"2026-10-10","status":"open"},
  {"id":"INV-3","customer":"Weber KG","customer_email":"info@weber.de","amount":"800.00 EUR","due":"2026-08-15","status":"paid"}
]
```

The second says what the pretend AI answers, and what day "today" is:

```json file=fixtures/01-invoice-chaser/model.json
{
  "today": "2026-10-13",
  "responses": [
    {"prompt_prefix": "Write a short, polite payment reminder", "response": "Dear Müller GmbH, this is a friendly reminder that invoice INV-1 for 1200.00 EUR was due on 2026-09-01. We would appreciate payment at your earliest convenience."}
  ]
}
```

Then run the agent once, with nothing real attached:

```bash
agentmark run invoice-chaser.agent.md --mock --fixtures fixtures
agentmark trace invoice-chaser.agent.md --mock --fixtures fixtures
```

`run` does the work against the pretend data. `trace` does the same and also prints a line for every
step that mattered, as a diary of the run. This is what the diary looks like for the three invoices
above:

```json file=fixtures/01-invoice-chaser/expected.trace.jsonl
{"kind":"model","agent":"invoice-chaser","block":"Every day at 09:00","prompt_prefix":"Write a short, polite payment reminder","context":{"id":"INV-1"}}
{"kind":"guard","agent":"invoice-chaser","name":"no amounts changed in reminder","result":"pass"}
{"kind":"guard","agent":"invoice-chaser","name":"reminder length < 800","result":"pass"}
{"kind":"tool","tool":"email","verb":"send","target":"customer","to":"ap@mueller.de"}
{"kind":"tool","tool":"crm","verb":"update","source":"invoices","id":"INV-1","set":{"status":"reminded"}}
{"kind":"tool","tool":"log","message":"Reminded Müller GmbH about 1200.00 EUR"}
```

Read it top to bottom. Only INV-1 is chased: INV-2 is not yet past its grace period and INV-3 is paid.
For INV-1 the agent asked the AI, both safety checks passed, one email went to `ap@mueller.de`, the
invoice was marked, and one line went to the log. Nothing was really sent anywhere. If a guard had
failed, you would see `"result":"fail"` and the diary would stop there for that invoice.

When the diary looks right, the same file is turned into a real program that does exactly this
against your real systems. That step is for whoever installs it for you; it never changes what the
agent does, because the steps are the same.

## 7. When something is wrong

AgentMark tells you the line, shows it, puts a mark under the problem and says in plain words what it
expected. It never shows you programmer's error text. Three that you are likely to meet:

**A typo.** It suggests the word it thinks you meant:

```agentmark invalid name=mistake.agent.md says=Unknown_target
> Mails a reminder.
# Agent: reminder
- model: claude
- tools: email, log

## Every day at 09:00
- email send custmer: "Please pay."
```

```console stderr
$ agentmark check mistake.agent.md
mistake.agent.md:7
  - email send custmer: "Please pay."
               ^^^^^^^
  Unknown target "custmer" for tool email. Did you mean "customer"?
```

**A tool that is not on the list.** This is the safety net doing its job:

```agentmark invalid name=mistake.agent.md says=is_not_listed_in_the_tools
> Mails a reminder.
# Agent: reminder
- model: claude
- tools: log

## Every day at 09:00
- email send address "ap@example.com": "Please pay."
```

```console stderr
$ agentmark check mistake.agent.md
mistake.agent.md:7
  - email send address "ap@example.com": "Please pay."
    ^^^^^
  The tool "email" is not listed in the tools of agent "reminder".
  Add it to the "- tools:" line, for example "- tools: log, email".
```

**Mixing things that do not mix.** Units are real: money and days are different kinds of things.

```agentmark invalid name=mistake.agent.md says=Cannot_add_money_to_a_date
# Record: Invoice
- amount: money
- due: date

> Wrong on purpose.
# Rule: odd
- for: Invoice
- when: today > due + amount
```

```console stderr
$ agentmark check mistake.agent.md
mistake.agent.md:6
  # Rule: odd
          ^^^
  Unused rule: nothing refers to "odd".

mistake.agent.md:8
  - when: today > due + amount
                  ^^^^^^^^^^^^
  Cannot add money to a date.
  Money only combines with money; durations combine with dates and other durations.
```

An error stops you from going further, a warning does not. The last line of `check` counts both.

## 8. Keep it tidy

Two more helpers, both safe to run any time:

* `agentmark fmt invoice-chaser.agent.md` rewrites the file in the one standard layout, so every
  AgentMark file in the world looks the same. `agentmark fmt --check` only says whether it would
  change anything.
* `agentmark lint invoice-chaser.agent.md` gives style advice: a file that has grown too long, the
  same amount typed in twice (it suggests a named **constant**), an agent with no description, an
  `ask model` inside a loop without a `guard` after it.

Two limits keep files readable, and they cannot be switched off: at most 25 steps in one block and at
most three levels of indentation. If a block grows past that, `check` tells you which lines to move
into a `## Do` card of their own.

## 9. When it grows: a folder instead of a file

One file is fine for a draft. A real project gets a folder, one thing per file, and a small `main`
file that lists what runs. This is the same agent you just built, spread out; it is one of the
examples that ship with AgentMark (`examples/01-invoice-chaser`):

```text
invoice-chaser/
  agentmark.toml          the project's name and its style settings
  tools.toml              the menu of tools (section 3)
  main.agent.md           what this program starts
  constants.agent.md      numbers and texts with a name
  records/invoice.agent.md
  rules/overdue.agent.md
  agents/invoice-chaser.agent.md
```

Each file starts with `# Import:` lines that say which other files it needs; a file sees only what it
imports. Here are three of the files. First, named values, so "14 days" is written once:

```agentmark file=examples/01-invoice-chaser/constants.agent.md
# Constant: grace_period
- value: 14 days

# Constant: max_reminder_length
- value: 800
```

The rule now uses the name:

```agentmark file=examples/01-invoice-chaser/rules/overdue.agent.md
# Import: ../constants.agent.md
# Import: ../records/invoice.agent.md

> Unpaid and more than the grace period past due.
# Rule: overdue
- for: Invoice
- when: today > due + grace_period and status != "paid"
```

And `main.agent.md` is the table of contents of the program:

```agentmark file=examples/01-invoice-chaser/main.agent.md
# Import: ./agents/invoice-chaser.agent.md

> Entry point. Lists what runs in this binary.
# Run
- invoice-chaser
```

`agentmark check`, `doc`, `fmt` and `lint` all accept the folder in place of the file. The report for
the whole project is the same kind of report you saw in section 4:

```console cwd=repo
$ agentmark check examples/01-invoice-chaser
Effects (what each block can touch)
  invoice-chaser / Every day at 09:00: model, network(crm, email), local(log)
  invoice-chaser / Do remind: model, network(email, crm), local(log)
  rule overdue: pure

ok: 0 errors, 0 warnings.
```

## 10. Using someone else's library

Sometimes an agent needs something no tool on the menu offers: turning an invoice into a PDF,
computing a checksum. For that, AgentMark can use existing libraries (the building blocks other
people publish, called *crates* in the Rust world). You name the library in one line:

```agentmark fragment
# Import: crate sha2
```

and AgentMark writes a small description file for it, with plain-sounding statements such as
`sha2 sha256_digest "some text" -> digest`. Nobody has checked yet what a new library may touch, so it
starts as **unapproved**. `check` says so, names the file and the line, and refuses to let the agent go
live until a person changes that one line from `- effect: unverified` to what is true: `pure` (only
looks), `local` (touches this computer) or `network` (talks to other systems). It is your decision, and
it is the one decision about a library that always stays with a human. While you are still trying
things out, `agentmark check --allow-unverified` lets you carry on. The developer's page,
`docs/libraries.md`, explains the rest.

## 11. Five agents to read

The `examples/` folder has five finished programs. Reading them is the fastest way to learn more:

| folder | what it shows |
|---|---|
| `01-invoice-chaser` | the agent of this guide: rule, constants, `ask model` with two guards |
| `02-lead-qualifier` | an agent that wakes up when a lead arrives and asks the AI for a *structured* answer (a score and a reason) |
| `03-support-triage` | `stop`, `if`, and a safety check that only lets three exact words through |
| `04-daily-report` | counting with rules, a weekly trigger, and a report mailed to you |
| `05-invoice-service` | no AI at all: a small web service and a command, using a library |

Here is the second one in full, a complete agent on one page. Read it with the report in mind. It uses
the CRM, email and the log, and it asks the AI:

```agentmark file=examples/02-lead-qualifier.agent.md
# Record: Lead
- id: text
- company: text
- message: text
- email: text
- score: maybe int

# Record: Qualification
- score: int
- reason: text

# Agent: lead-qualifier
- model: claude:haiku
- tools: crm, email, log

## When crm receives lead
- ask model with lead: "Score this inbound lead from 1 to 10 for a web agency and explain why in one sentence." as Qualification -> q
- guard: q matches Qualification
- crm update lead: score = q.score
- if q.score >= 7:
  - email send address "sales@optimotion.dev": "Hot lead: {lead.company} ({q.score}/10). {q.reason}"
- else:
  - log: "Lead {lead.company} scored {q.score}, no action"
```

Three things to notice. `score: maybe int` says a lead may have no score yet: anything that can be missing
must say `maybe`, there is no "empty by accident". `as Qualification` tells the AI to answer in the shape
of the record `Qualification`, and `guard: q matches Qualification` stops the agent if the answer does not
have that shape. And `email send address "..."` mails a fixed address instead of the customer.

## 12. Words used on this page

| word | meaning |
|---|---|
| record | the description of a thing you keep track of (`# Record:`) |
| rule | a yes/no question about one record that never does anything (`# Rule:`) |
| agent | the worker: a model, a list of tools, and blocks of steps (`# Agent:`) |
| trigger | the heading that says when a block runs (`## Every ...`, `## When ...`) |
| block | the steps under one heading; `## Do name` is a block you can call |
| tool | a system the agent may reach, from the menu in `tools.toml` |
| guard | a safety check; if it fails the work stops and nothing more happens |
| safety check | what `agentmark check` does, and the plain word for a guard |
| constant | a number or text with a name (`# Constant:`) |
| library | outside building blocks the agent can use; they start unapproved |
| test data | the pretend CRM and pretend AI answers a trial run uses |

Next, if you want the exact grammar: `docs/reference.md` has every statement of the language in the
same order as the specification.
