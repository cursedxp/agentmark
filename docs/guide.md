# AgentMark guide

**In short**

- This page teaches AgentMark in small lessons. Each lesson has a goal, code, the output and a next step.
- It is for anyone who has never written a program.
- After it you can write a report program, read the "what does it touch" report and write a small agent.

**Jump to:** [Lesson 1: Describe the thing you keep track of](#lesson-1-describe-the-thing-you-keep-track-of) · [Lesson 2: Read a file and add it up](#lesson-2-read-a-file-and-add-it-up) · [Lesson 6: See what a program touches](#lesson-6-see-what-a-program-touches) · [Lesson 7: Fix a mistake](#lesson-7-fix-a-mistake) · [Lesson 11: Write your own tool](#lesson-11-write-your-own-tool) · [Lesson 12: Add an agent](#lesson-12-add-an-agent) · [Lesson 15: Read ten finished programs](#lesson-15-read-ten-finished-programs) · [Lesson 16: Words used on this page](#lesson-16-words-used-on-this-page)

## Before you start

AgentMark is a way to write programs as Markdown documents: headings, plain sentences, bullet points and numbered steps. A program is a few files that read like a checklist. One command turns them into a single program you can run on your computer.

By the end you will have written two things:

- a **report program** that reads a sales file, adds the sales up per region and writes a report. It needs no AI and no network, only your computer and a file;
- a small **agent** that finds overdue invoices, asks an AI to write a polite reminder, double-checks what the AI wrote, and sends it. An agent is one more thing you can write in AgentMark. It is not the whole language.

You will also know how to read the "what does each part touch?" report. That is the part a careful manager cares about most.

It takes about twenty minutes. You need:

- a text editor (Notepad, TextEdit, anything);
- the `agentmark` program, installed as shown in the [README](../README.md#install);
- an empty folder to work in.

Everything you write goes in files that end in `.agent.md`. They are ordinary Markdown. If you can write a checklist, you can read and write them.

## Lesson 1: Describe the thing you keep track of

**Goal:** say what one sale looks like.

A program works with *things*: sales, invoices, photos, log lines. In AgentMark you describe one of them as a **record**. Create a file called `sales.agent.md` and type this:

```agentmark name=sales.agent.md
# Record: Sale
One line of the sales file: what was sold, where, and for how much.

- region: text
- product: text
- amount: money
```

Read it out loud: "A record called Sale. One line of the sales file: what was sold, where, and for how much. It has a region, a product and an amount of money." That is all it says.

An `.agent.md` file is ordinary Markdown, and every Markdown shape means one thing in AgentMark:

| In the file | It means |
|---|---|
| a heading (`#`) | a **part** of the program. The word before the colon is the kind of part, the word after is its name |
| the paragraph right under a heading | the **description** of that part, in plain words |
| a bullet (`-`) | a **detail** of that part |
| a number (`1.`, `2.`, ...) | a **step**. Steps happen in order, top to bottom |

A line that starts with `>` is a note for people. Nothing reads it but you.

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

When it cannot tell, it says so and asks you to write the kind. Writing the kind yourself is always the clearer choice.

**Next:** [Lesson 2: Read a file and add it up](#lesson-2-read-a-file-and-add-it-up).

## Lesson 2: Read a file and add it up

**Goal:** read a sales file and print how many sales it holds and what they add up to.

A **command** is a program you start yourself and give a few values, such as the name of a file. Every program also gets a **program** part at the top: its name, what it does and which command it starts. Put this above your record, and add the command below it:

```agentmark name=sales.agent.md
# Program: sales
Reads a sales file and prints how much it brought in.

- start: [sales-report](#command-sales-report)

# Record: Sale
One line of the sales file: what was sold, where, and for how much.

- region: text
- product: text
- amount: money

# Command: sales-report
Reads a sales file and prints how much it brought in.

- in: path: text
- steps:
  1. try file read path into sales
     otherwise:
     1. print: "Could not read {path}"
     2. stop
  2. print: "{count of sales} sales, {sum of amount in sales} in total"
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
| `start: [sales-report](#command-sales-report)` | says which command the program starts. It is a link to the command's heading |
| `in: path: text` | what you give the program when you start it: here, the name of a file |
| `steps:` and the numbers | the steps, top to bottom |
| `try file read path into sales` | reads the file and keeps its content under the name `sales`. Because `Sale` is a record, the file is read as a table: its first line names the columns, and they must match the names in the record, so `sales` is a list of `Sale` |
| `otherwise:` | reading can fail, for example when there is no such file or the table is broken, so you say what to do instead: print a sentence and `stop` |
| `count of sales`, `sum of amount in sales` | **collection words**. They work on any list |
| `{...}` inside a quoted sentence | filled in with the value |

**Next:** [Lesson 3: Pick out the big ones](#lesson-3-pick-out-the-big-ones).

## Lesson 3: Pick out the big ones

**Goal:** list the sales of 100 EUR or more.

A **rule** is a question about one record that can only be answered "yes" or "no". Rules never do anything. They only look. The last word of the rule's heading is the record it is about. Add one, and use it in the command:

```agentmark name=sales.agent.md
# Program: sales
Reads a sales file and prints how much it brought in.

- start: [sales-report](#command-sales-report)

# Record: Sale
One line of the sales file: what was sold, where, and for how much.

- region: text
- product: text
- amount: money

# Rule: big Sale
A sale of 100 EUR or more.

- when: amount >= 100 EUR

# Command: sales-report
Reads a sales file and prints how much it brought in.

- in: path: text
- steps:
  1. try file read path into sales
     otherwise:
     1. print: "Could not read {path}"
     2. stop
  2. print: "{count of sales} sales, {sum of amount in sales} in total"
  3. print: "{count of big sales in sales} of them are big:"
  4. for each big sale in sales:
     1. print: "  {region} {product} {amount}"
```

Read the rule: "The rule `big` is about a Sale. A sale of 100 EUR or more. It is true when the amount is at least 100 euros."

In the command, `big sales in sales` means "only those sales that the rule `big` says yes to". `for each big sale in sales:` does the numbered steps below it once for each of them. Inside, `{region}` and `{amount}` are the values of the sale at hand.

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
# Program: sales
Reads a sales file and prints how much it brought in, region by region.

- start: [sales-report](#command-sales-report)

# Record: Sale
One line of the sales file: what was sold, where, and for how much.

- region: text
- product: text
- amount: money

# Record: RegionTotal
What one region brought in: how many orders and how much money.

- region: text
- orders: int
- revenue: money

# Rule: big Sale
A sale of 100 EUR or more.

- when: amount >= 100 EUR

# Function: region_totals
Adds up the sales of every region, biggest revenue first.

- in: sales
- out: region totals
- steps:
  1. group sales by region into by_region
  2. empty list of RegionTotal into totals
  3. for each region, region_sales in by_region:
     1. add RegionTotal to totals:
        - region
        - orders: count of region_sales
        - revenue: sum of amount in region_sales
  4. return sort totals by revenue descending

# Command: sales-report
Reads a sales file and prints how much it brought in, region by region.

- in: path: text
- steps:
  1. try file read path into sales
     otherwise:
     1. print: "Could not read {path}"
     2. stop
  2. print: "{count of sales} sales, {sum of amount in sales} in total"
  3. for each entry in region_totals(sales):
     1. print: "{entry.region}: {entry.orders} orders, {entry.revenue}"
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
| `in: sales` and `out: region totals` | the names say what they are. `sales` is a list of `Sale` records, and `region totals` is a list of `RegionTotal` records. When a name does not say it, write the kind: `in: path: text` |
| `group sales by region into by_region` | sorts the sales into one pile per region and keeps the piles under the name `by_region`. `for each region, region_sales in by_region:` then visits each pile with its name |
| `empty list of RegionTotal into totals` | starts an empty list |
| `add RegionTotal to totals:` and the bullets under it | builds one `RegionTotal` and puts it on the list. A bullet that is only a name, such as `region`, takes the value of that name |
| `sort totals by revenue descending` | puts the biggest first |

Other collection words you will meet: `where`, `map ... to`, `take 3 from`, `skip`, `first of`, `unique`, `join ... with`. The full list is in `docs/reference.md`, section 22.2.

**Next:** [Lesson 5: Write the report to a file](#lesson-5-write-the-report-to-a-file).

## Lesson 5: Write the report to a file

**Goal:** write the table to a file whose name you give, instead of printing it.

Three new things here: a second input, `text.pad` to line up columns, and `file write`:

```agentmark name=sales.agent.md
# Program: sales
Reads a sales file and writes a revenue report per region.

- start: [sales-report](#command-sales-report)

# Record: Sale
One line of the sales file: what was sold, where, and for how much.

- region: text
- product: text
- amount: money

# Record: RegionTotal
What one region brought in: how many orders and how much money.

- region: text
- orders: int
- revenue: money

# Rule: big Sale
A sale of 100 EUR or more.

- when: amount >= 100 EUR

# Function: region_totals
Adds up the sales of every region, biggest revenue first.

- in: sales
- out: region totals
- steps:
  1. group sales by region into by_region
  2. empty list of RegionTotal into totals
  3. for each region, region_sales in by_region:
     1. add RegionTotal to totals:
        - region
        - orders: count of region_sales
        - revenue: sum of amount in region_sales
  4. return sort totals by revenue descending

# Command: sales-report
Reads a sales file and writes a revenue report per region, biggest region first.

- in: path: text, out: text
- steps:
  1. try file read path into sales
     otherwise:
     1. print: "Could not read {path}"
     2. stop
  2. list of "Sales report", "------------" into lines
  3. for each entry in region_totals(sales):
     1. add "{text.pad(entry.region, 8)}{entry.revenue}" to lines
  4. add "------------" to lines
  5. add "{text.pad("Total", 8)}{sum of amount in sales}" to lines
  6. add "Big sales: {count of big sales in sales}" to lines
  7. join lines with "\n" into report
  8. file write out: "{report}\n"
  9. print: "Wrote {count of lines} lines to {out}"
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

**Next:** [Lesson 6: See what a program touches](#lesson-6-see-what-a-program-touches).

## Lesson 6: See what a program touches

**Goal:** know what a program can do to your computer without reading its steps.

Run the **safety check**:

```console
$ agentmark check sales.agent.md
What each part touches
  big: touches nothing
  region_totals: touches nothing
  sales-report: touches files, print

ok: 0 errors, 0 warnings.
```

Each line is one part of the program. After "touches" comes everything that part can reach. This is the **touches** report, and it is the first thing to read when you review a program.

| It says | In plain words |
|---|---|
| `touches nothing` | only looks at what it already has |
| `touches files, print` | works on this computer: here, it reads and writes files and prints text |
| `touches crm, email` | talks to other systems: here, the CRM and email |
| `touches AI, ...` | asks the AI (always listed first) |

This is the whole idea of AgentMark: you do not have to read the steps to know what a program can do. The report is worked out from the steps themselves, so it cannot be out of date.

The report above says: this program can read and write files on this computer and print. It cannot reach another system and cannot ask an AI.

You can also promise something yourself. Write `- touches: nothing` under a function, and the safety check stops you if a step in it reads a file or prints. The promise is checked, not trusted.

When you review a program someone else wrote, read in this order: the touches report, the descriptions, the steps. Three questions are worth asking:

- Does anything touch a system that sends (email, crm, http)? Anything that sends is the first thing to look at.
- Does the description match what it touches? A part described as "adds up the sales" that touches email is a red flag.
- Is there a guard after every `ask model`? (You will meet guards in [Lesson 13](#lesson-13-let-the-agent-write-the-reminder-safely).)

For the second question there is a command. It prints the descriptions and the touches of every part as one readable page, built from the descriptions the author wrote under each heading:

```console
$ agentmark doc sales.agent.md
# sales

Reads a sales file and writes a revenue report per region.

## Commands

### sales-report

Reads a sales file and writes a revenue report per region, biggest region first.

sales-report: touches files, print


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

Applies to Sale.

big: touches nothing


## Functions

### region_totals

Adds up the sales of every region, biggest revenue first.

region_totals: touches nothing
```

**Next:** [Lesson 7: Fix a mistake](#lesson-7-fix-a-mistake).

## Lesson 7: Fix a mistake

**Goal:** read an error message and know what to change.

AgentMark tells you the line, shows it, puts a mark under the problem and says in plain words what it expected. It never shows you programmer's error text. Two mistakes you are likely to meet.

**A typo.** It suggests the word it thinks you meant:

```agentmark invalid name=mistake.agent.md says=Unknown_field
# Program: sales-total
Prints the total of a sales file.

- start: [sales-total](#command-sales-total)

# Record: Sale
One line of the sales file.

- region: text
- amount: money

# Command: sales-total
Prints the total of a sales file.

- in: path: text
- steps:
  1. try file read path into sales
     otherwise:
     1. stop
  2. print: "Total: {sum of amont in sales}"
```

```console stderr
$ agentmark check mistake.agent.md
mistake.agent.md:20
    2. print: "Total: {sum of amont in sales}"
                              ^^^^^
  Unknown field "amont" on Sale. Did you mean "amount"?
```

**Mixing things that do not mix.** Units are real: money and days are different kinds of things.

```agentmark invalid name=mistake.agent.md says=Cannot_add_money_to_a_date
# Record: Invoice
An invoice with an amount and a due day.

- amount: money
- due: date

# Rule: odd Invoice
Wrong on purpose.

- when: today > due + amount
```

```console stderr
$ agentmark check mistake.agent.md
mistake.agent.md:7
  # Rule: odd Invoice
          ^^^
  Unused rule: nothing refers to "odd".

mistake.agent.md:10
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
| `agentmark fmt sales.agent.md` | rewrites the file in the one standard layout, so every AgentMark file in the world looks the same. It also renumbers your steps 1, 2, 3, ... |
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

Each file starts with a `# Uses` list that links the other files it needs. A file sees only what it links. Here are three of the files.

First, named values, so a width is written once:

```agentmark file=examples/06-sales-report/constants.agent.md
# Constant: column_width
How wide the first two columns of the report are, in characters.

- value: 8

# Constant: rule_width
Tied to the columns: two columns of column_width plus 11 characters for the widest revenue ("1005.50 EUR").

- value: 27
```

A function that uses the name:

```agentmark file=examples/06-sales-report/functions/table-row.agent.md
# Uses
- [constants](../constants.agent.md)

# Function: table_row
One table row: three cells, the first two padded so the columns line up.

- in: left: text, middle: text, right: text
- out: text
- touches: nothing
- steps:
  1. return "{text.pad(left, column_width)}{text.pad(middle, column_width)}{right}"
```

And `main.agent.md`, the table of contents of the program:

```agentmark file=examples/06-sales-report/main.agent.md
# Program: sales-report
Reads a sales file and writes a revenue report, region by region.

# Uses
- [sales-report command](./commands/sales-report.agent.md)
```

`agentmark check`, `run`, `doc`, `fmt` and `lint` all accept the folder in place of the file. The report for the whole project is the same kind of report you saw in [Lesson 6](#lesson-6-see-what-a-program-touches):

```console cwd=repo
$ agentmark check examples/06-sales-report
What each part touches
  sales-report: touches files, print
  region_totals: touches nothing
  table_row: touches nothing

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

Then use it in a step, such as `sha2 sha256_digest "some text" into digest`.

What happens the first time you run `check`:

1. AgentMark writes a small description file for the library.
2. The library starts as **unapproved**, because nobody has checked what it may touch.
3. `check` names the file and the line, and refuses to let the program go live.
4. A person changes that one line from `- effect: unverified` to what is true. (Library files keep the older word `effect` for this line. Everywhere else the word is "touches".)

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

A **tool** is something a program calls with a step, such as `slugger slug title into s`. You can write one yourself with a `# Tool` part. It has one `## verb(params) -> type` block per verb, each with its own description and numbered steps.

What it touches is worked out from its body, exactly like a function's. A program that only uses tools like this one and the built-in `file` tool needs no `tools.toml` and no Rust.

Save this as `slugger.agent.md`:

```agentmark name=slugger.agent.md
# Program: slugger-demo
Prints the web address of a title.

- start: [address](#command-address)

# Tool: slugger
Turns a title into a web address: lower case, dashes between words.

## slug(title: text) -> text
Lower-cases the title and replaces every run of other characters with a dash.

1. text.lower(title) into lowered
2. return regex.replace(lowered, "[^a-z0-9]+", "-")

# Command: address
Prints the address of a title: `address <title>`.

- in: title: text
- steps:
  1. print: "/posts/{slugger.slug(title)}"
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

An agent is built from the parts you already know: records, rules, steps. What is new is the worker, which is the agent. The other new parts are the **tools** it may use and the guards that check what the AI says.

Start a new file, `invoice-chaser.agent.md`. It begins like the report: a record, and a rule.

```agentmark name=invoice-chaser.agent.md
# Record: Invoice
An invoice as stored in the CRM.

- id: text
- customer: text
- customer_email: text
- amount: money
- due: date
- status: text

# Rule: overdue Invoice
Unpaid and more than 14 days past due.

- when: today > due + 14 days and status != "paid"
```

Read the rule: "The rule `overdue` is about an Invoice (the last word of its heading). It is true when today is later than the due date plus 14 days, and the status is not paid." Amounts of time are real values, so `due + 14 days` works.

An agent has a name, an AI model to ask, and a list of **tools**. A tool here is a system the agent can reach: your CRM, your email, a log. The agent can use only the tools on its list.

Which tools exist, and what each one can do, is written once in a file called `tools.toml` next to your agent. Whoever set up your computer writes it.

Reading and writing files, printing and the other things you did above are built in. They need no menu entry. A typical menu looks like this:

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
# Program: invoice-chaser
Chases unpaid invoices once a day.

# Record: Invoice
An invoice as stored in the CRM.

- id: text
- customer: text
- customer_email: text
- amount: money
- due: date
- status: text

# Rule: overdue Invoice
Unpaid and more than 14 days past due.

- when: today > due + 14 days and status != "paid"

# Agent: invoice-chaser
Writes a line to the log for every overdue invoice. Changes nothing.

- model: claude
- tools: crm, log

## Every day at 09:00
Logs every overdue invoice.

1. for each overdue invoice in crm.invoices:
   1. log: "{customer} is overdue: {amount}"
```

Read the agent part like a job description:

| Piece | What it does |
|---|---|
| `model: claude` | the AI it may ask questions. This version does not ask any yet |
| `tools: crm, log` | everything it may use. Nothing else |
| `## Every day at 09:00` | a **trigger**: when the work starts |
| the paragraph under the trigger | what the block does, in plain words |
| the numbered lines under it | the **steps** |
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
What each part touches
  overdue: touches nothing
  invoice-chaser / Every day at 09:00: touches crm, log

ok: 0 errors, 0 warnings.
```

In one line it says: this agent, at 09:00 each day, touches the CRM and the log, and nothing else. It cannot send email, because `email` is not on its list. If you try, the check stops you:

```agentmark invalid name=mistake.agent.md says=is_not_listed_in_the_tools
# Agent: reminder
Mails a reminder.

- model: claude
- tools: log

## Every day at 09:00
Mails the accounts office.

1. email send address "ap@example.com": "Please pay."
```

```console stderr
$ agentmark check mistake.agent.md
mistake.agent.md:10
  1. email send address "ap@example.com": "Please pay."
     ^^^^^
  The tool "email" is not listed in the tools of agent "reminder".
  Add it to the "- tools:" line, for example "- tools: log, email".
```

**Next:** [Lesson 13: Let the agent write the reminder, safely](#lesson-13-let-the-agent-write-the-reminder-safely).

## Lesson 13: Let the agent write the reminder, safely

**Goal:** have the AI write a reminder, check what it wrote, and send it.

Replace the agent part with this version (the record and the rule stay as they are):

```agentmark name=invoice-chaser.agent.md
# Program: invoice-chaser
Chases unpaid invoices once a day with a polite reminder.

# Record: Invoice
An invoice as stored in the CRM.

- id: text
- customer: text
- customer_email: text
- amount: money
- due: date
- status: text

# Rule: overdue Invoice
Unpaid and more than 14 days past due.

- when: today > due + 14 days and status != "paid"

# Agent: invoice-chaser
Chases unpaid invoices once a day. Never changes amounts; only sends reminders.

- model: claude
- tools: email, crm, log

## Every day at 09:00
Finds every overdue invoice and reminds its customer.

1. for each overdue invoice in crm.invoices:
   1. do remind

## Do remind
Has the model write one reminder, checks it, sends it and marks the invoice as reminded.

1. ask model with invoice: "Write a short, polite payment reminder. Keep the amount exactly as given." into reminder
2. guard: no amounts changed in reminder
3. guard: reminder length < 800
4. email send customer: "{reminder}"
5. crm update invoice: status = "reminded"
6. log: "Reminded {customer} about {amount}"
```

New pieces, one by one:

| Piece | What it does |
|---|---|
| `## Do remind` | a named block of steps, like a recipe card. `do remind` in the trigger says "now follow that card, for this invoice". It keeps the trigger short and the card readable |
| `ask model with invoice: "..." into reminder` | asks the AI. It is shown the invoice, plus your instruction in quotes. Whatever it answers is kept under the name `reminder` |
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
What each part touches
  overdue: touches nothing
  invoice-chaser / Every day at 09:00: touches AI, crm, email, log
  invoice-chaser / Do remind: touches AI, crm, email, log

ok: 0 errors, 0 warnings.
```

The words in the report mean the same as before. `AI` is new: this agent asks the AI.

Is there a `guard` after every `ask model`? That is the third review question from [Lesson 6](#lesson-6-see-what-a-program-touches), and the report is where you start: look for `AI` in it, then look for the guard.

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

**Watch out:** connecting the agent to your real systems, so that "send an email" really sends one, is a developer's job.

It lives in one file of your project, `rust/connect.rs`. The first real run creates that file for them.

The steps of your agent do not change. The details are in [Connecting real tools](connect.md).

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
# Uses
- [constants](../constants.agent.md)
- [ServiceErrors](../records/service-errors.agent.md)

# Function: rank_services
Counts the error lines of every service and ranks the services, most errors first.

- in: error_lines: list of text
- out: list of ServiceErrors
- touches: nothing
- steps:
  1. group error_lines by regex.replace(item, log_pattern, "$4") into by_service
  2. empty list of ServiceErrors into ranking
  3. for each service, service_lines in by_service:
     1. add ServiceErrors to ranking:
        - service
        - errors: count of service_lines
  4. return sort ranking by errors descending
```

Here is the second agent. The record it works on comes first, then the agent:

```agentmark file=examples/02-lead-qualifier/records/lead.agent.md
# Record: Lead
An inbound lead as delivered by the CRM.

- id: text
- company: text
- message: text
- email: text
- score: maybe int
```

```agentmark file=examples/02-lead-qualifier/agents/lead-qualifier.agent.md
# Uses
- [constants](../constants.agent.md)
- [Lead](../records/lead.agent.md)
- [Qualification](../records/qualification.agent.md)

# Agent: lead-qualifier
Scores every inbound lead and alerts sales about the hot ones. Never contacts the lead itself.

- model: claude:haiku
- tools: crm, email, log

## When crm receives lead
Scores the new lead. A hot lead goes to sales by email; any other lead is only logged.

1. ask model with lead: scoring_prompt as Qualification into q
2. guard: q matches Qualification
3. crm update lead: score = q.score
4. if q.score >= hot_lead_score:
   1. email send address "sales@optimotion.dev": "Hot lead: {lead.company} ({q.score}/10). {q.reason}"
   else:
   1. log: "Lead {lead.company} scored {q.score}, no action"
```

Read it with the touches report in mind. It touches AI, the CRM, email and the log. `scoring_prompt` and `hot_lead_score` are named values from the constants file, which the `# Uses` list links.

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
| step | one numbered line of work. Steps happen in order |
| command | a program you start yourself and give a few values (`# Command:`) |
| collection word | `group`, `sort`, `sum of`, `count of` and friends: they work on whole lists |
| part | one heading of a file: a record, a rule, a function, a command, an agent ... |
| description | the plain paragraph right under a heading that says what the part is for |
| touches | what a part reaches: files, print, a system such as the CRM, or the AI. `agentmark check` reports it for every part |
| safety check | what `agentmark check` does: it reads the program and reports what each part touches |
| agent | a worker that wakes up on a trigger, uses tools and may ask an AI (`# Agent:`) |
| trigger | the heading that says when an agent's block runs (`## Every ...`, `## When ...`) |
| block | the steps under one heading; `## Do name` is a block you can call |
| tool | something a program calls with a statement: the built-in `file`, one you write yourself (`# Tool:`), or a system from the menu in `tools.toml` (an agent may use only the tools on its list) |
| guard | a check on what the AI wrote; if it fails the work stops and nothing more happens |
| constant | a number or text with a name (`# Constant:`) |
| library | outside building blocks a program can use; they start unapproved |
| test data | the pretend CRM and pretend AI answers a trial run uses |

**Next:** `docs/reference.md` has every statement of the language in the same order as the specification.
