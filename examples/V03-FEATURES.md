# v0.3 feature checklist for examples 06 to 10

The constructs are the v0.3 ones. The samples in the tables are written in v0.4 syntax (SPEC section 23): numbered steps, `into`, `add`, `# Program` and `# Uses`. The SPEC line numbers are those of the v0.3 text.

Written in step 25 (authored ahead of the compiler). For each example: the SPEC section 22 constructs it uses,
with SPEC.md line numbers and where in the example they appear. An implementer ticks a row when `agentmark check`,
`build` and the fixture run work for that construct. Constructs from v0.1/v0.2 (functions, commands, services,
`try`/`otherwise`, `guard`, `respond`, interpolation, constants) are not listed.

SPEC line map (SPEC.md after the section 22 decisions): 22.1 types 635-677 (enum 638-641, match 652-670, records and
`some`/`none` 673-675) · 22.2 collections 683-701 · 22.3 library 707-732 (modules 709-716, `try` on results 718, `file`
723-729) · 22.4 handles 736-748 · 22.6 default command 768 · 22.7 examples and fixtures 772-782.

## 06-sales-report (`sales-report <path> <out>`)

| construct | SPEC | where |
|---|---|---|
| default command: the program has exactly one command (`# Program`, `- start:`) | 768 | `main.agent.md` |
| `try file read path into sales` (a list of records reads a CSV file) | 723 | `commands/sales-report.agent.md` |
| `csv.parse(t) as list of T` | 716 | still in the library; the example reads the file typed instead |
| `group sales by region into by_region` and `for each region, region_sales in by_region` | 690, 701 | `functions/region-totals.agent.md` |
| `count of region_sales`, `sum of amount in region_sales` (money) | 691-692 | same; command (`sum of amount in sales`) |
| `sort totals by revenue descending` | 689 | same |
| `empty list of T into x`, `list of a, b, c into x`, `add x to list` | 685, 701 | same; command |
| `join lines with "\n"` | 697 | command |
| `text.pad`, `text.repeat` | 709 | `functions/table-row.agent.md`, command |
| `file write out: content` | 724 | command |
| record construction `add RegionTotal to totals:` with one bullet per field | (22.1, line 673; 23.11) | `functions/region-totals.agent.md` |

## 07-file-renamer (`rename-photos <folder> [--dry-run]`)

| construct | SPEC | where |
|---|---|---|
| default command, positional arg plus bool flag `--dry-run` | 768, 777 | `commands/rename-photos.agent.md` |
| `file list folder into names` (sorted names) | 726 | command |
| `regex.captures(t, p)` as `maybe list of text`, `is some` | 710 | `functions/photo-stamp.agent.md` |
| `join groups with ""` | 697 | same |
| `date.parse(t, pattern)` with `try` , `date.format(d, pattern)` | 713 | same |
| `maybe text` result with `some x` / `none`, `is some` on a call | 675, 718 | `functions/photo-stamp.agent.md`, command |
| `path.extension`, `path.join` | 714 | command |
| `file rename old to new` | 727 | command |
| `file` trace lines (`rename`) | 732 | `fixtures/07` has no trace file; runner may add one |

## 08-notes-api (`# Service: notes`, SQLite through `sqlx`)

| construct | SPEC | where |
|---|---|---|
| `# Import: crate sqlx`, binder handles: constructor to handle (`into pool`) | 736-739 | `services/notes.agent.md`, `functions/ensure-schema.agent.md` |
| method steps `<handle> <method> args [into var]` | 746 | all routes (`statement bind ...`, `... execute pool`) |
| `self`-consuming method chain: `statement bind a into with_title`, `with_title bind b into insert` | 745 | `POST /notes` |
| handle passed to a function: `in: pool: handle of sqlx.SqlitePool` | 744 | `ensure_schema(pool)` |
| `json.parse(t) as T` and `as list of T` | 715 | `GET /notes`, `GET /notes/{id}` |
| `match` on a `result of T` (`ok` / `error`) | 666-670 | `GET /notes`, `GET /notes/{id}` |
| JSON bodies in and out (`read body as NewNote into note`, `respond 201 { ... }`) | v0.2, 778 | `POST /notes` |

### Binder items 08 relies on (crate `sqlx`, SQLite only; verbs follow the v0.2 naming rule)

| verb used | Rust API | kind |
|---|---|---|
| `sqlx sqlite_pool_connect url into pool` | `SqlitePool::connect(&str)` async, `Result` | constructor, returns `handle of sqlx.SqlitePool` |
| `sqlx query sql into statement` | `sqlx::query::<Sqlite>(&str)` | constructor, handle of a query |
| `sqlx query_scalar sql into statement` | `sqlx::query_scalar::<Sqlite, String>(&str)` | constructor, handle; rows are `text` |
| `statement bind value into next` | `Query::bind(self, T)` for text (and `bind` of int if wanted) | consumes self, returns the handle |
| `statement execute pool` | `Query::execute(&Pool)` async, `Result<SqliteQueryResult>` | `&self` pool argument, returns result handle |
| `outcome last_insert_rowid into id` | `SqliteQueryResult::last_insert_rowid()` | `&self`, `i64` as `int` |
| `outcome rows_affected into removed` | `SqliteQueryResult::rows_affected()` | `&self`, `u64` as `int` |
| `try listing fetch_one pool into encoded` | `QueryScalar::fetch_one(&Pool)` async, `Result<String>` | consumes self |
| `try lookup fetch_optional pool into found` | `QueryScalar::fetch_optional(&Pool)` async | consumes self |

The generic parts of sqlx (`DB`, output type) have one instantiation under the `sqlite` feature set in
`agentmark.toml [crates]`, so the assisted tier binds them (SPEC 22.4, line 747); there is no hand-edited fallback.
`u64` results (`rows_affected`) convert with a checked conversion. Function `ensure_schema` leaves out `- touches:`, so `check` works out what it touches from the binding.

## 09-markdown-to-html (`md-to-html <source> <out>`)

| construct | SPEC | where |
|---|---|---|
| `# Enum` with fields and a bare variant | 638-641 | `records/block.agent.md` |
| enum construction `Heading { level: 2, content: ... }` and bare `Blank` | 647 | `functions/classify-line.agent.md` |
| `match` over an enum, binders `h`, `entry`, `p`, bare `Blank` | 652-658, 677 | `functions/render-block.agent.md` |
| `text.trim`, `text.starts_with`, `text.replace`, `text.lines` | 709 | classify, inline-format, command |
| `regex.replace` with `$1`, `$2` | 710 | `functions/inline-format.agent.md`, `wrap-lists.agent.md`, classify |
| `join elements with "\n" into body` | 697 | command |
| string building by interpolation | 22.7 | render-block, wrap-lists |
| `file read` (try), `file write` | 723-724 | command |

## 10-log-analyzer (`analyze-log <path> [--top N]`)

| construct | SPEC | where |
|---|---|---|
| default command with an int option `--top 2` | 768 | `commands/analyze-log.agent.md` |
| `text.lines` | 709 | command |
| `rows where <condition>` over a `list of text` using `item` | 687, 681 | command |
| `count of rows where ...` | 692 | command |
| `first of rows` as `maybe`, `is some` | 694 | command |
| `regex.captures`, `regex.matches`, `regex.replace` | 710 | command, `functions/rank-services.agent.md` |
| `group lines by <expression on item>` | 690, 681 | `rank-services` |
| `count of service_lines`, `sort ... descending` (stable), `add ... to ranking:`, `for each k, v in map` | 692, 689, 701 | `rank-services` |
| `take top from ranked` | 695 | command |
| `text.pad` | 709 | command |
| record construction `add ServiceErrors to ranking:` | (22.1, line 673; 23.11) | `rank-services` |

## Gaps: all decided

| was a gap | decision (SPEC) | used in |
|---|---|---|
| record construction | `Sale { region: "EU", ... }` or, as a list item, `add Sale to sales:` with bullets (22.1, line 673; 23.11). Every non-`maybe` field given | 06, 08 (`respond 201 Note { ... }`), 10 |
| `try` on built-ins that return `result` | `try <call> into var`, `do` optional, with `otherwise:`; or `match <call>` (22.3, line 718). The examples write `try` without `do`. | 06, 07 |
| `is some` on a call | allowed on any `maybe` expression (22.3, line 718) | 07, 10 |
| absent values | `some x` / `none` (22.1, line 675); `photo_stamp` in 07 returns `maybe text` | 07 |
| unused branch name | optional: `- error:` (22.1, line 675) | 08 |
| `requests.json` status | `expect_status` and `expect_body` (22.7, line 782) | 08 |
| variants inherit `input/` and `run.json` | 22.7, line 782 | 07 dry-run |
| `expected/` | the working directory must hold exactly those files (22.7, line 782): 06 and 09 list their input file too | 06, 07, 09 |
| `regex.replace` | replaces every match (22.3, line 710): 09 has two `**...**` in one line | 09 |
| sqlx generics, lifetimes, `u64` | handles own lifetimes, single-instantiation generics (`DB = Sqlite`), checked `u64` (22.4, line 747) | 08 |
| crate features | `agentmark.toml [crates]` (22.4, line 748); 08 declares `sqlx` with `sqlite` and `runtime-tokio` | 08 |

Still open and recorded as questions: Q400 to Q403 (pad, lines, write/append, regex dialect, path/date, flags).
Not exercised in v0.3: typed row access through handles (08 reads rows as JSON text, see `notes.agent.md`).
