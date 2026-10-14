# AgentMark guide

**In short**

- This page teaches AgentMark in small lessons. Each lesson has a goal, code, the output and a next step.
- It is for anyone who has never written a program.
- After it you can write a report program, read the "what can it touch" report and write a small agent.

**Jump to:** [Lesson 1: Describe the thing you keep track of](#lesson-1-describe-the-thing-you-keep-track-of) · [Lesson 2: Read a file and add it up](#lesson-2-read-a-file-and-add-it-up) · [Lesson 6: See what a program can touch](#lesson-6-see-what-a-program-can-touch) · [Lesson 7: Fix a mistake](#lesson-7-fix-a-mistake) · [Lesson 11: Write your own tool](#lesson-11-write-your-own-tool) · [Lesson 12: Add an agent](#lesson-12-add-an-agent) · [Lesson 15: Read ten finished programs](#lesson-15-read-ten-finished-programs) · [Lesson 16: Words used on this page](#lesson-16-words-used-on-this-page)

## Before you start

AgentMark is a way to write programs as Markdown documents: headings, bullet points and plain sentences. A program is a few files that read like a checklist. One command turns them into a single program you can run on your computer.

By the end you will have written two things:

- a **report program** that reads a sales file, adds the sales up per region and writes a report. It needs no AI and no network, only your computer and a file;
- a small **agent** that finds overdue invoices, asks an AI to write a polite reminder, double-checks what the AI wrote, and sends it. An agent is one more thing you can write in AgentMark. It is not the whole language.

You will also know how to read the "what can this touch?" report. That is the part a careful manager cares about most.

It takes about twenty minutes. You need:

- a text editor (Notepad, TextEdit, anything);
- the `agentmark` program, installed as shown in the [README](../README.md#install);
- an empty folder to work in.

Everything you write goes in files that end in `.agent.md`. They are ordinary Markdown. If you can write a checklist, you can read and write them.

## Lesson 1: Describe the thing you keep track of

**Goal:** say what one sale looks like.

A program works with *things*: sales, invoices, photos, log lines. In AgentMark you describe one of them as a **record**. Create a file called `sales.agent.md` and type this:

```agentmark name=sales.agent.md
> One line of the sales file: what was sold, where, and for how much.
# Record: Sale
- region: text
- product: text
- amount: money
```

Read it out loud: "A record called Sale. It has a region, a product and an amount of money." That is all it says.

What each kind of line does:

| Line starts with | It means |
|---|---|
| `>` | a comment, for people only. The comment right above a heading is the description of that heading |
| `#` | a new part of the file. The word before the colon is the kind of part, the word after is its name |
| `-` | a detail of that part |

After each name comes its kind of value:

| Kind | Holds | Example |
|---|---|---|
| `text` | words | `Widget` |
| `money` | an amount with a currency | `1200.00 EUR` |
| `date` | a day | `2026-10-13` |
| `int` | a whole number | `7` |
| `bool` | yes or no | |
| `datetime` | a day and a time | |
| `duration` | a length of time | `14 days` |

If you leave the kind out, AgentMark guesses it from the last word of the name:

| The name ends in | The kind is |
|---|---|
| `date`, `at`, `on` | date |
| `amount`, `price`, `total`, `fee` | money |
| `count`, `qty`, `number` | whole number |
| anything else | text |

When it cannot tell, it says so and tells you which kind it picked. Writing the kind yourself is always the clearer choice.

**Next:** [Lesson 2: Read a file and add it up](#lesson-2-read-a-file-and-add-it-up).

## Lesson 2: Read a file and add it up

**Goal:** read a sales file and print how many sales it holds and what they add up to.

A **command** is a program you start yourself and give a few values, such as the name of a file. Add one to the same file:

```agentmark name=sales.agent.md
> One line of the sales file: what was sold, where, and for how much.
# Record: Sale
- region: text
- product: text
- amount: money

> Reads a sales file and prints how much it brought in.
# Command: sales-report
- args: path: text

## Run
- try file read path -> content
  otherwise:
    - print: "Could not read {path}"
    - stop
- try csv.parse(content) as list of Sale -> sales
  otherwise:
    - print: "{path} is not a valid sales file"
    - stop
- print: "{count of sales} sales, {sum of amount in sales} in total"

# Run
- sales-report
```

Make a small sales file next to it, called `sales.csv`:

```text file=fixtures/06-sales-report/input/sales.csv name=sales.csv
region,product,amount
EU,Widget,120.00 EUR
US,Gadget,200.50 EUR
EU,Gadget,80.25 EUR
APAC,Widget,45.00 EUR
US,Widget,310.00 EUR
EU,Widget,99.75 EUR
APAC,Gadget,150.00 EUR
```

Run the program. Everything after `--` goes to the program itself:

```console
$ agentmark run sales.agent.md -- sales.csv
7 sales, 1005.50 EUR in total
```

You have written a program. The first run takes a while because AgentMark builds it. The next ones are quick.

Give it a file that does not exist and you get your own sentence back:

```console
$ agentmark run sales.agent.md -- nothing.csv
Could not read nothing.csv
```

Read the command like a recipe:

| Piece | What it does |
|---|---|
| `args: path: text` | what you give the program when you start it: here, the name of a file |
| `## Run` | the steps, top to bottom |
| `file read path -> content` | reads the file and keeps its words under the name `content` |
| `try ...` with `otherwise:` | reading can fail, for example when there is no such file, so you say what to do instead: print a sentence and `stop` |
| `csv.parse(content) as list of Sale -> sales` | turns the text into a list of `Sale` records. The first line of the file names the columns, and they must match the names in the record |
| `count of sales`, `sum of amount in sales` | **collection words**. They work on any list |
| `{...}` inside a quoted sentence | filled in with the value |
| `# Run` and `- sales-report` | say which command the program starts. With only one command, you can start the program without naming it |

**Next:** [Lesson 3: Pick out the big ones](#lesson-3-pick-out-the-big-ones).

## Lesson 3: Pick out the big ones

**Goal:** list the sales of 100 EUR or more.

A **rule** is a question about one record that can only be answered "yes" or "no". Rules never do anything. They only look. Add one, and use it in the command:

```agentmark name=sales.agent.md
> One line of the sales file: what was sold, where, and for how much.
# Record: Sale
- region: text
- product: text
- amount: money

> A sale of 100 EUR or more.
# Rule: big
- for: Sale
- when: amount >= 100 EUR

> Reads a sales file and prints how much it brought in.
# Command: sales-report
- args: path: text

## Run
- try file read path -> content
  otherwise:
    - print: "Could not read {path}"
    - stop
- try csv.parse(content) as list of Sale -> sales
  otherwise:
    - print: "{path} is not a valid sales file"
    - stop
- print: "{count of sales} sales, {sum of amount in sales} in total"
- print: "{count of big sales in sales} of them are big:"
- for each big sale in sales:
  - print: "  {region} {product} {amount}"

# Run
- sales-report
```

Read the rule: "The rule `big` is about a Sale. It is true when the amount is at least 100 euros."

In the command, `big sales in sales` means "only those sales that the rule `big` says yes to". `for each big sale in sales:` does what is indented below once for each of them. Inside, `{region}` and `{amount}` are the values of the sale at hand.

```console
$ agentmark run sales.agent.md -- sales.csv
7 sales, 1005.50 EUR in total
4 of them are big:
  EU Widget 120.00 EUR
  US Gadget 200.50 EUR
  US Widget 310.00 EUR
  APAC Gadget 150.00 EUR
```

**Next:** [Lesson 4: Add up per region with your own function](#lesson-4-add-up-per-region-with-your-own-function).

## Lesson 4: Add up per region with your own function

**Goal:** print the total of each region, biggest first.

A **function** is a piece of work with a name, so the command stays short. It takes some values in and gives one value out. This one needs a second record to hold one answer:

```agentmark name=sales.agent.md
> One line of the sales file: what was sold, where, and for how much.
# Record: Sale
- region: text
- product: text
- amount: money

> What one region brought in: how many orders and how much money.
# Record: RegionTotal
- region: text
- orders: int
- revenue: money

> A sale of 100 EUR or more.
# Rule: big
- for: Sale
- when: amount >= 100 EUR

> Adds up the sales of every region, biggest revenue first.
# Function: region_totals
- in: sales: list of Sale
- out: list of RegionTotal
- effect: pure
  - set by_region = group sales by region
  - set totals = empty list of RegionTotal
  - for each region, rows in by_region:
    - append RegionTotal { region: region, orders: count of rows, revenue: sum of amount in rows } to totals
  - return sort totals by revenue descending

> Reads a sales file and prints how much it brought in, region by region.
# Command: sales-report
- args: path: text

## Run
- try file read path -> content
  otherwise:
    - print: "Could not read {path}"
    - stop
- try csv.parse(content) as list of Sale -> sales
  otherwise:
    - print: "{path} is not a valid sales file"
    - stop
- print: "{count of sales} sales, {sum of amount in sales} in total"
- for each entry in region_totals(sales):
  - print: "{entry.region}: {entry.orders} orders, {entry.revenue}"

# Run
- sales-report
```

```console
$ agentmark run sales.agent.md -- sales.csv
7 sales, 1005.50 EUR in total
US: 2 orders, 510.50 EUR
EU: 3 orders, 300.00 EUR
APAC: 2 orders, 195.00 EUR
```

New pieces:

| Piece | What it does |
|---|---|
| `group sales by region` | sorts the sales into one pile per region. `for each region, rows in by_region:` then visits each pile with its name |
| `RegionTotal { region: region, ... }` | builds a record from values |
| `sort totals by revenue descending` | puts the biggest first |
| `effect: pure` | a promise: the function only works with the values it was given. It cannot read a file or print. AgentMark checks the promise in [Lesson 6](#lesson-6-see-what-a-program-can-touch) |

Other collection words you will meet: `where`, `map ... to`, `take 3 from`, `skip`, `first of`, `unique`, `join ... with`. The full list is in `docs/reference.md`, section 22.2.

**Next:** [Lesson 5: Write the report to a file](#lesson-5-write-the-report-to-a-file).

## Lesson 5: Write the report to a file

**Goal:** write the table to a file whose name you give, instead of printing it.

Three new things here: a second argument, `text.pad` to line up columns, and `file write`:

```agentmark name=sales.agent.md
> One line of the sales file: what was sold, where, and for how much.
# Record: Sale
- region: text
- product: text
- amount: money

> What one region brought in: how many orders and how much money.
# Record: RegionTotal
- region: text
- orders: int
- revenue: money

> A sale of 100 EUR or more.
# Rule: big
- for: Sale
- when: amount >= 100 EUR

> Adds up the sales of every region, biggest revenue first.
# Function: region_totals
- in: sales: list of Sale
- out: list of RegionTotal
- effect: pure
  - set by_region = group sales by region
  - set totals = empty list of RegionTotal
  - for each region, rows in by_region:
    - append RegionTotal { region: region, orders: count of rows, revenue: sum of amount in rows } to totals
  - return sort totals by revenue descending

> Reads a sales file and writes a revenue report per region, biggest region first.
# Command: sales-report
- args: path: text, out: text

## Run
- try file read path -> content
  otherwise:
    - print: "Could not read {path}"
    - stop
- try csv.parse(content) as list of Sale -> sales
  otherwise:
    - print: "{path} is not a valid sales file"
    - stop
- set lines = list of "Sales report", "------------"
- for each entry in region_totals(sales):
  - append "{text.pad(entry.region, 8)}{entry.revenue}" to lines
- append "------------" to lines
- append "{text.pad("Total", 8)}{sum of amount in sales}" to lines
- append "Big sales: {count of big sales in sales}" to lines
- file write out: "{join lines with "\n"}\n"
- print: "Wrote {count of lines} lines to {out}"

# Run
- sales-report
```

`agentmark trace` runs the program and prints a line for everything that mattered. It is a diary of the run. Here it shows that the program read one file and wrote another:

```console
$ agentmark trace sales.agent.md -- sales.csv report.txt
{"kind":"file","verb":"read","path":"sales.csv"}
{"kind":"file","verb":"write","path":"report.txt"}
{"kind":"print","message":"Wrote 8 lines to report.txt"}
```

The file `report.txt` now holds the report:

```text
Sales report
------------
US      510.50 EUR
EU      300.00 EUR
APAC    195.00 EUR
------------
Total   1005.50 EUR
Big sales: 4
```

**Next:** [Lesson 6: See what a program can touch](#lesson-6-see-what-a-program-can-touch).

## Lesson 6: See what a program can touch

**Goal:** know what a program can do to your computer without reading its steps.

Run the **safety check**:

```console
$ agentmark check sales.agent.md
Effects (what each block can touch)
  rule big: pure
  function region_totals: pure
  sales-report / Run: local(file, print)

ok: 0 errors, 0 warnings.
```

Each line is one part of the program. After the colon is everything that part can reach:

| It says | In plain words |
|---|---|
| `pure` | only looks at what it already has |
| `local(file, print)` | works on this computer: here, it reads and writes files and prints text |
| `network(crm, email)` | talks to other systems: here, the CRM and email |
| `model` | asks the AI |

This is the whole idea of AgentMark: you do not have to read the steps to know what a program can do. The program produces the report from its own steps, so it cannot be out of date. A part that says `pure` is checked to be pure.

The report above says: this program can read and write files on this computer and print. It cannot reach the network and cannot ask an AI.

When you review a program someone else wrote, read in this order: the report, the description comments, the steps. Two questions are worth asking:

- Does anything say `network`? Anything that sends is the first thing to look at.
- Does the description (the `>` comment) match what the report says?

For the second question there is a command. It prints the descriptions and the reach of every part as one readable page, built from the comments the author wrote next to the steps:

```console
$ agentmark doc sales.agent.md
# sales

## Commands

### sales-report

Reads a sales file and writes a revenue report per region, biggest region first.

- can touch: local(file, print)


## Records

### Sale

One line of the sales file: what was sold, where, and for how much.

- region: text
- product: text
- amount: money

### RegionTotal

What one region brought in: how many orders and how much money.

- region: text
- orders: int
- revenue: money


## Rules

### big

A sale of 100 EUR or more.

Applies to Sale. Always pure.


## Functions

### region_totals

Adds up the sales of every region, biggest revenue first.

- can touch: nothing outside itself (pure)
```

**Next:** [Lesson 7: Fix a mistake](#lesson-7-fix-a-mistake).

## Lesson 7: Fix a mistake

**Goal:** read an error message and know what to change.

AgentMark tells you the line, shows it, puts a mark under the problem and says in plain words what it expected. It never shows you programmer's error text. Two mistakes you are likely to meet.

**A typo.** It suggests the word it thinks you meant:

```agentmark invalid name=mistake.agent.md says=Unknown_field
> One line of the sales file.
# Record: Sale
- region: text
- amount: money

> Prints the total of a sales file.
# Command: sales-total
- args: path: text

## Run
- try file read path -> content
  otherwise:
    - stop
- try csv.parse(content) as list of Sale -> sales
  otherwise:
    - stop
- print: "Total: {sum of amont in sales}"

# Run
- sales-total
```

```console stderr
$ agentmark check mistake.agent.md
mistake.agent.md:17
  - print: "Total: {sum of amont in sales}"
                           ^^^^^
  Unknown field "amont" on Sale. Did you mean "amount"?
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

An error stops you from going further. A warning does not. The last line of `check` counts both.

**Next:** [Lesson 8: Tidy a file](#lesson-8-tidy-a-file).

## Lesson 8: Tidy a file

**Goal:** put a file in the standard layout and get style advice.

Two helpers, both safe to run any time:

| Command | What it does |
|---|---|
| `agentmark fmt sales.agent.md` | rewrites the file in the one standard layout, so every AgentMark file in the world looks the same |
| `agentmark fmt --check` | only says whether `fmt` would change anything |
| `agentmark lint sales.agent.md` | gives style advice: a file that has grown too long, the same amount typed in twice (it suggests a named **constant**), a part with no description |

Two limits keep files readable. They cannot be switched off:

- at most 25 steps in one block;
- at most three levels of indentation.

If a block grows past that, `check` tells you which lines to move into a function of their own.

**Next:** [Lesson 9: Split a program into a folder](#lesson-9-split-a-program-into-a-folder).

## Lesson 9: Split a program into a folder

**Goal:** turn one file into a folder with one thing per file.

One file is fine for a draft. A real program gets a folder and a small `main` file that lists what runs. This is the same sales report, spread out. It is one of the examples that ship with AgentMark (`examples/06-sales-report`):

```text
sales-report/
  agentmark.toml          the project's name and its style settings
  main.agent.md           what this program starts
  constants.agent.md      numbers and texts with a name
  records/sale.agent.md
  records/region-total.agent.md
  functions/region-totals.agent.md
  functions/table-row.agent.md
  commands/sales-report.agent.md
```

Each file starts with `# Import:` lines that say which other files it needs. A file sees only what it imports. Here are three of the files.

First, named values, so a width is written once:

```agentmark file=examples/06-sales-report/constants.agent.md
# Constant: column_width
- value: 8

> Tied to the columns: two columns of column_width plus 11 characters for the widest revenue ("1005.50 EUR").
# Constant: rule_width
- value: 27
```

A function that uses the name:

```agentmark file=examples/06-sales-report/functions/table-row.agent.md
# Import: ../constants.agent.md

> One table row: three cells, the first two padded so the columns line up.
# Function: table_row
- in: left: text, middle: text, right: text
- out: text
- effect: pure
  - return "{text.pad(left, column_width)}{text.pad(middle, column_width)}{right}"
```

And `main.agent.md`, the table of contents of the program:

```agentmark file=examples/06-sales-report/main.agent.md
# Import: ./commands/sales-report.agent.md

> Entry point. The program is one command, so `sales-report <path> <out>` runs it directly.
# Run
- sales-report
```

`agentmark check`, `run`, `doc`, `fmt` and `lint` all accept the folder in place of the file. The report for the whole project is the same kind of report you saw in [Lesson 6](#lesson-6-see-what-a-program-can-touch):

```console cwd=repo
$ agentmark check examples/06-sales-report
Effects (what each block can touch)
  sales-report / Run: local(file, print)
  function region_totals: pure
  function table_row: pure

ok: 0 errors, 0 warnings.
```

**Next:** [Lesson 10: Use someone else's library](#lesson-10-use-someone-elses-library).

## Lesson 10: Use someone else's library

**Goal:** use a building block that other people published.

Some work is built in. `text`, `regex`, `number`, `money`, `date`, `path`, `json` and `csv` have functions you can call without installing anything: `text.lower(name)`, `date.format(day, "%Y-%m-%d")`, `csv.parse(...)` as above.

For anything else, AgentMark can use existing libraries, called *crates* in the Rust world. You name the library in one line:

```agentmark fragment
# Import: crate sha2
```

Then use it in a step, such as `sha2 sha256_digest "some text" -> digest`.

What happens the first time you run `check`:

1. AgentMark writes a small description file for the library.
2. The library starts as **unapproved**, because nobody has checked what it may touch.
3. `check` names the file and the line, and refuses to let the program go live.
4. A person changes that one line from `- effect: unverified` to what is true.

The true values:

| Value | Meaning |
|---|---|
| `pure` | only looks |
| `local` | touches this computer |
| `network` | talks to other systems |

This is your decision. It is the one decision about a library that always stays with a human.

**Watch out:** while you are still trying things out, `agentmark check --allow-unverified` lets you carry on. It assumes the library's own proposal and says so in a warning.

The developer's page, `docs/libraries.md`, shows the whole thing with real output.

**Next:** [Lesson 11: Write your own tool](#lesson-11-write-your-own-tool).

## Lesson 11: Write your own tool

**Goal:** write a small tool in AgentMark and call it from a command.

A **tool** is something a program calls with a statement, such as `- slugger slug title -> s`. You can write one yourself with a `# Tool` part. It has one `## verb(params) -> type` block per verb.

Its declared effect is checked against its body, exactly like a function's. A program that only uses tools like this one and the built-in `file` tool needs no `tools.toml` and no Rust.

Save this as `slugger.agent.md`:

```agentmark name=slugger.agent.md
> Turns a title into a web address: lower case, dashes between words.
# Tool: slugger
- effect: pure

## slug(title: text) -> text
- set lowered = text.lower(title)
- return regex.replace(lowered, "[^a-z0-9]+", "-")

> Prints the address of a title: `address <title>`.
# Command: address
- args: title: text

## Run
- print: "/posts/{slugger.slug(title)}"

# Run
- address
```

Run it. This tool turns a title into a web address:

```console
$ agentmark run slugger.agent.md -- Hello_World
/posts/hello-world
```

**Next:** [Lesson 12: Add an agent](#lesson-12-add-an-agent).

## Lesson 12: Add an agent

**Goal:** write an agent that logs every overdue invoice and nothing more.

Everything so far ran without an AI. An **agent** is a program that wakes up on a schedule or when something happens. It looks at a system such as your CRM, may ask an AI to write or decide something, and acts.

An agent is built from the parts you already know: records, rules, steps. What is new is the worker (the agent), the **tools** it may use, and the guards that check what the AI says.

Start a new file, `invoice-chaser.agent.md`. It begins like the report: a record, and a rule.

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

Read the rule: "The rule `overdue` is about an Invoice. It is true when today is later than the due date plus 14 days, and the status is not paid." Amounts of time are real values, so `due + 14 days` works.

An agent has a name, an AI model to ask, and a list of **tools**. A tool here is a system the agent can reach: your CRM, your email, a log. The agent can use only the tools on its list.

Which tools exist, and what each one can do, is written once in a file called `tools.toml` next to your agent. Whoever set up your computer writes it. Reading and writing files, printing and the other things you did above are built in. They need no menu entry. A typical menu looks like this:

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

You do not need to change that file. Read it as a menu:

- the CRM can be asked to `list`, `get` and `update` things;
- email can only `send`;
- the log writes a line.

Next to each is the kind of reach it has. `network` means it talks to another system. `local` means it stays on this computer.

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

Read the agent part like a job description:

| Piece | What it does |
|---|---|
| `model: claude` | the AI it may ask questions. This version does not ask any yet |
| `tools: crm, log` | everything it may use. Nothing else |
| `## Every day at 09:00` | a **trigger**: when the work starts |
| the lines under the trigger | the **steps** |
| `for each overdue invoice in crm.invoices:` | take every invoice from the CRM, keep only those the rule `overdue` says yes to, and for each do what is indented below |
| `log: "..."` | writes one line. Inside the quotes, `{customer}` and `{amount}` are filled in with the values of the invoice at hand |

Other triggers:

| Trigger | When it starts |
|---|---|
| `## Every 15 minutes` | every 15 minutes |
| `## Every Monday at 08:00` | every Monday at 08:00 |
| `## On start` | once, when the program starts |
| `## When crm receives lead` | when the CRM reports a new lead |

Check it. This is the report you show a colleague:

```console
$ agentmark check invoice-chaser.agent.md
Effects (what each block can touch)
  rule overdue: pure
  invoice-chaser / Every day at 09:00: network(crm), local(log)

ok: 0 errors, 0 warnings.
```

In one line it says: this agent, at 09:00 each day, can reach the CRM over the network and write to the log, and nothing else. It cannot send email, because `email` is not on its list. If you try, the check stops you:

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

**Next:** [Lesson 13: Let the agent write the reminder, safely](#lesson-13-let-the-agent-write-the-reminder-safely).

## Lesson 13: Let the agent write the reminder, safely

**Goal:** have the AI write a reminder, check what it wrote, and send it.

Replace the agent part with this version:

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

| Piece | What it does |
|---|---|
| `## Do remind` | a named block of steps, like a recipe card. `do remind` in the trigger says "now follow that card, for this invoice". It keeps the trigger short and the card readable |
| `ask model with invoice: "..." -> reminder` | asks the AI. It is shown the invoice, plus your instruction in quotes. Whatever it answers is kept under the name `reminder` |
| `guard:` | a **guard** is a check on what the AI wrote. See below |
| `email send customer: "{reminder}"` | sends the text. `customer` is a *target* from the menu in `tools.toml`: the agent mails the customer's email address from the invoice |
| `crm update invoice: status = "reminded"` | marks the invoice, so it is not chased twice |
| `log: ...` | logs what happened |

The AI is clever but not careful. So before anything leaves the building, the agent checks the answer. Two guards here:

- `no amounts changed in reminder`: every amount of money in the text must also be in the invoice. If the AI wrote "1,500 EUR" where the invoice says 1200 EUR, the check fails.
- `reminder length < 800`: the text must be shorter than 800 characters.

If a guard fails, the agent stops that work and sends nothing. It writes down which guard failed and on which line. A wrong answer from the AI costs you a log entry, not an embarrassing email.

Check it:

```console
$ agentmark check invoice-chaser.agent.md
Effects (what each block can touch)
  rule overdue: pure
  invoice-chaser / Every day at 09:00: model, network(crm, email), local(log)
  invoice-chaser / Do remind: model, network(email, crm), local(log)

ok: 0 errors, 0 warnings.
```

The words in the report mean the same as before. `model` is new: this agent asks the AI.

Is there a `guard` after every `ask model`? That is the third question worth asking of any agent you review.

**Next:** [Lesson 14: Try the agent on pretend data](#lesson-14-try-the-agent-on-pretend-data).

## Lesson 14: Try the agent on pretend data

**Goal:** run the agent once with nothing real attached.

Before an agent touches real customers, run it against **test data**. Make a folder `fixtures` next to your file with two small files. The first is a pretend CRM with three invoices:

```json file=fixtures/01-invoice-chaser/crm.invoices.json name=fixtures/crm.invoices.json
[
  {"id":"INV-1","customer":"Müller GmbH","customer_email":"ap@mueller.de","amount":"1200.00 EUR","due":"2026-09-01","status":"open"},
  {"id":"INV-2","customer":"Schmidt AG","customer_email":"buch@schmidt.de","amount":"450.00 EUR","due":"2026-10-10","status":"open"},
  {"id":"INV-3","customer":"Weber KG","customer_email":"info@weber.de","amount":"800.00 EUR","due":"2026-08-15","status":"paid"}
]
```

The second says what the pretend AI answers, and what day "today" is:

```json file=fixtures/01-invoice-chaser/model.json name=fixtures/model.json
{
  "today": "2026-10-13",
  "responses": [
    {"prompt_prefix": "Write a short, polite payment reminder", "response": "Dear Müller GmbH, this is a friendly reminder that invoice INV-1 for 1200.00 EUR was due on 2026-09-01. We would appreciate payment at your earliest convenience."}
  ]
}
```

Then run the agent once. `trace` does the work against the pretend data and prints a line for every step that mattered. It is the same kind of diary you saw in [Lesson 5](#lesson-5-write-the-report-to-a-file):

```console
$ agentmark trace invoice-chaser.agent.md --mock --fixtures fixtures
{"kind":"model","agent":"invoice-chaser","block":"Every day at 09:00","prompt_prefix":"Write a short, polite payment reminder","context":{"id":"INV-1"}}
{"kind":"guard","agent":"invoice-chaser","name":"no amounts changed in reminder","result":"pass"}
{"kind":"guard","agent":"invoice-chaser","name":"reminder length < 800","result":"pass"}
{"kind":"tool","tool":"email","verb":"send","target":"customer","to":"ap@mueller.de"}
{"kind":"tool","tool":"crm","verb":"update","source":"invoices","id":"INV-1","set":{"status":"reminded"}}
{"kind":"tool","tool":"log","message":"Reminded Müller GmbH about 1200.00 EUR"}
```

Read it top to bottom:

- Only INV-1 is chased. INV-2 is not yet past its grace period and INV-3 is paid.
- For INV-1 the agent asked the AI and both guards passed.
- One email went to `ap@mueller.de`, the invoice was marked, and one line went to the log.
- Nothing was really sent anywhere.

If a guard had failed, you would see `"result":"fail"` and the diary would stop there for that invoice.

`run` does the same work without the diary. The only thing it shows is what the agent wrote to the log:

```console stderr
$ agentmark run invoice-chaser.agent.md --mock --fixtures fixtures
[log] Reminded Müller GmbH about 1200.00 EUR
```

**Watch out:** connecting the agent to your real systems, so that "send an email" really sends one, is a developer's job. It lives in one file of your project, `rust/connect.rs`. The first real run creates that file for them. The steps of your agent do not change. The README has the details: [Connecting real tools](../README.md#connecting-real-tools-rustconnectrs).

**Next:** [Lesson 15: Read ten finished programs](#lesson-15-read-ten-finished-programs).

## Lesson 15: Read ten finished programs

**Goal:** learn more by reading finished programs.

The `examples/` folder has ten of them. Reading them is the fastest way to learn.

| Folder | What it shows |
|---|---|
| `06-sales-report` | the report of this guide: a CSV file, totals per region, a report file |
| `07-file-renamer` | renaming the photos of a folder by the date in their names, with a `--dry-run` switch that only says what it would do |
| `08-notes-api` | a small web service that keeps notes in a database file |
| `09-markdown-to-html` | turning a Markdown file into a web page: lines, patterns and a list of kinds of line |
| `10-log-analyzer` | reading a log file, counting problems per service, printing the worst three |
| `01-invoice-chaser` | the agent of this guide: rule, constants, `ask model` with two guards |
| `02-lead-qualifier` | an agent that wakes up when a lead arrives and asks the AI for a *structured* answer (a score and a reason) |
| `03-support-triage` | `stop`, `if`, and a guard that only lets three exact words through |
| `04-daily-report` | counting with rules, a weekly trigger, and a report mailed to you |
| `05-invoice-service` | no AI at all: a small web service and a command, using a library |

Here is the heart of the log analyzer, a function that counts the error lines of every service and ranks the services:

```agentmark file=examples/10-log-analyzer/functions/rank-services.agent.md
# Import: ../constants.agent.md
# Import: ../records/service-errors.agent.md

> Counts the error lines of every service and ranks the services, most errors first.
# Function: rank_services
- in: error_lines: list of text
- out: list of ServiceErrors
- effect: pure
  - set by_service = group error_lines by regex.replace(item, log_pattern, "$4")
  - set ranking = empty list of ServiceErrors
  - for each service, rows in by_service:
    - append ServiceErrors { service: service, errors: count of rows } to ranking
  - return sort ranking by errors descending
```

Here is the second agent. The record it works on comes first, then the agent:

```agentmark file=examples/02-lead-qualifier/records/lead.agent.md
> An inbound lead as delivered by the CRM.
# Record: Lead
- id: text
- company: text
- message: text
- email: text
- score: maybe int
```

```agentmark file=examples/02-lead-qualifier/agents/lead-qualifier.agent.md
# Import: ../constants.agent.md
# Import: ../records/lead.agent.md
# Import: ../records/qualification.agent.md

> Scores every inbound lead and alerts sales about the hot ones. Never contacts the lead itself.
# Agent: lead-qualifier
- model: claude:haiku
- tools: crm, email, log

## When crm receives lead
- ask model with lead: scoring_prompt as Qualification -> q
- guard: q matches Qualification
- crm update lead: score = q.score
- if q.score >= hot_lead_score:
  - email send address "sales@optimotion.dev": "Hot lead: {lead.company} ({q.score}/10). {q.reason}"
- else:
  - log: "Lead {lead.company} scored {q.score}, no action"
```

Read it with the report in mind. It uses the CRM, email and the log, and it asks the AI. `scoring_prompt` and `hot_lead_score` are named values from the constants file.

Three things to notice:

- `score: maybe int` says a lead may have no score yet. Anything that can be missing must say `maybe`. There is no "empty by accident".
- `as Qualification` tells the AI to answer in the shape of the record `Qualification`. `guard: q matches Qualification` stops the agent if the answer does not have that shape.
- `email send address "..."` mails a fixed address instead of the customer.

**Next:** [Lesson 16: Words used on this page](#lesson-16-words-used-on-this-page), or `docs/reference.md` for the exact grammar.

## Lesson 16: Words used on this page

**Goal:** look up a word.

| Word | Meaning |
|---|---|
| record | the description of a thing you keep track of (`# Record:`) |
| rule | a yes/no question about one record that never does anything (`# Rule:`) |
| function | a named piece of work that takes values in and gives one value out (`# Function:`) |
| command | a program you start yourself and give a few values (`# Command:`) |
| collection word | `group`, `sort`, `sum of`, `count of` and friends: they work on whole lists |
| safety check | what `agentmark check` does: it reads the program and reports what it can touch |
| agent | a worker that wakes up on a trigger, uses tools and may ask an AI (`# Agent:`) |
| trigger | the heading that says when an agent's block runs (`## Every ...`, `## When ...`) |
| block | the steps under one heading; `## Do name` is a block you can call |
| tool | something a program calls with a statement: the built-in `file`, one you write yourself (`# Tool:`), or a system from the menu in `tools.toml` (an agent may use only the tools on its list) |
| guard | a check on what the AI wrote; if it fails the work stops and nothing more happens |
| constant | a number or text with a name (`# Constant:`) |
| library | outside building blocks a program can use; they start unapproved |
| test data | the pretend CRM and pretend AI answers a trial run uses |

**Next:** `docs/reference.md` has every statement of the language in the same order as the specification.
