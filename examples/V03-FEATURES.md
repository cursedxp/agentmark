# v0.3 feature checklist for examples 06 to 10

Written in step 25 (authored ahead of the compiler). For each example: the SPEC section 22 constructs it uses,
with SPEC.md line numbers and where in the example they appear. An implementer ticks a row when `agentmark check`,
`build` and the fixture run work for that construct. Constructs from v0.1/v0.2 (functions, commands, services,
`try`/`otherwise`, `guard`, `respond`, interpolation, constants) are not listed.

SPEC line map: 22.1 types 635-673 (enum 638-641, construct 647, match 652-670) · 22.2 collections 679-697 ·
22.3 library 703-726 (modules 705-712, `file` 717-723) · 22.4 handles 733-741 · 22.6 default command 760 ·
22.7 examples and fixtures 766-774.

## 06-sales-report (`sales-report <path> <out>`)

| construct | SPEC | where |
|---|---|---|
| default command from a single-command `# Run` | 760 | `main.agent.md` |
| `file read path -> content` with `try` | 717 | `commands/sales-report.agent.md` |
| `csv.parse(t) as list of T` | 712 | same, `try do csv.parse(...)` (Gap 2) |
| `group rows by region` and `for each region, rows in map` | 686, 697 | `functions/region-totals.agent.md` |
| `count of rows`, `sum of amount in rows` (money) | 687-688 | same; command (`sum of amount in sales`) |
| `sort totals by revenue descending` | 685 | same |
| `empty list of T`, `list of a, b, c`, `append x to list` | 681, 697 | same; command |
| `join lines with "\n"` | 693 | command |
| `text.pad`, `text.repeat` | 705 | `functions/table-row.agent.md`, command |
| `file write path: content` | 718 | command |
| record construction `RegionTotal { ... }` | (Gap 1) | `functions/region-totals.agent.md` |

## 07-file-renamer (`rename-photos <folder> [--dry-run]`)

| construct | SPEC | where |
|---|---|---|
| default command, positional arg plus bool flag `--dry-run` | 760, 769 | `commands/rename-photos.agent.md` |
| `file list folder -> names` (sorted names) | 720 | command |
| `regex.captures(t, p)` as `maybe list of text`, `is some` | 706 | `functions/photo-stamp.agent.md` |
| `join groups with ""` | 693 | same |
| `date.parse(t, pattern)` with `try do` (Gap 2), `date.format(d, pattern)` | 709 | same |
| `path.extension`, `path.join` | 710 | command |
| `file rename old to new` | 721 | command |
| `file` trace lines (`rename`) | 726 | `fixtures/07` has no trace file; runner may add one |

## 08-notes-api (`# Service: notes`, SQLite through `sqlx`)

| construct | SPEC | where |
|---|---|---|
| `# Import: crate sqlx`, binder handles: constructor to handle (`-> pool`) | 730-733 | `services/notes.agent.md`, `functions/ensure-schema.agent.md` |
| method statements `- <handle> <method> args [-> var]` | 740 | all routes (`statement bind ...`, `... execute pool`) |
| `self`-consuming method chain: `statement bind a -> with_title`, `with_title bind b -> insert` | 739 | `POST /notes` |
| handle passed to a function: `in: pool: handle of sqlx.SqlitePool` | 738 | `ensure_schema(pool)` |
| `json.parse(t) as T` and `as list of T` | 711 | `GET /notes`, `GET /notes/{id}` |
| `match` on a `result of T` (`ok` / `error`) | 666-670 | `GET /notes`, `GET /notes/{id}` |
| JSON bodies in and out (`read body as NewNote`, `respond 201 { ... }`) | v0.2, 770 | `POST /notes` |

### Binder items 08 relies on (crate `sqlx`, SQLite only; verbs follow the v0.2 naming rule)

| verb used | Rust API | kind |
|---|---|---|
| `sqlx sqlite_pool_connect url -> pool` | `SqlitePool::connect(&str)` async, `Result` | constructor, returns `handle of sqlx.SqlitePool` |
| `sqlx query sql -> statement` | `sqlx::query::<Sqlite>(&str)` | constructor, handle of a query |
| `sqlx query_scalar sql -> statement` | `sqlx::query_scalar::<Sqlite, String>(&str)` | constructor, handle; rows are `text` |
| `statement bind value -> next` | `Query::bind(self, T)` for text (and `bind` of int if wanted) | consumes self, returns the handle |
| `statement execute pool` | `Query::execute(&Pool)` async, `Result<SqliteQueryResult>` | `&self` pool argument, returns result handle |
| `outcome last_insert_rowid -> id` | `SqliteQueryResult::last_insert_rowid()` | `&self`, `i64` as `int` |
| `outcome rows_affected -> removed` | `SqliteQueryResult::rows_affected()` | `&self`, `u64` as `int` |
| `listing fetch_one pool -> text` | `QueryScalar::fetch_one(&Pool)` async, `Result<String>` | consumes self |
| `lookup fetch_optional pool -> maybe text` | `QueryScalar::fetch_optional(&Pool)` async | consumes self |

The generic parts of sqlx (`DB`, output type) must be fixed to `Sqlite` and `String`/`()` by the binder; if it cannot,
the binding file is edited by hand (v0.2 allows editing generated bindings). Function `ensure_schema` declares
`effect: network`; if the binding classifies sqlx as `local`, `network` still covers it.

## 09-markdown-to-html (`md-to-html <source> <out>`)

| construct | SPEC | where |
|---|---|---|
| `# Enum` with fields and a bare variant | 638-641 | `records/block.agent.md` |
| enum construction `Heading { level: 2, content: ... }` and bare `Blank` | 647 | `functions/classify-line.agent.md` |
| `match` over an enum, binders `h`, `entry`, `p`, bare `Blank` | 652-658, 673 | `functions/render-block.agent.md` |
| `text.trim`, `text.starts_with`, `text.replace`, `text.lines` | 705 | classify, inline-format, command |
| `regex.replace` with `$1`, `$2` | 706 | `functions/inline-format.agent.md`, `wrap-lists.agent.md`, classify |
| `join elements with "\n"` | 693 | command |
| string building by interpolation | 22.7 | render-block, wrap-lists |
| `file read` (try), `file write` | 717-718 | command |

## 10-log-analyzer (`analyze-log <path> [--top N]`)

| construct | SPEC | where |
|---|---|---|
| default command with an int option `--top 2` | 760 | `commands/analyze-log.agent.md` |
| `text.lines` | 705 | command |
| `rows where <condition>` over a `list of text` using `item` | 683, 677 | command |
| `count of rows where ...` | 688 | command |
| `first of rows` as `maybe`, `is some` | 690 | command |
| `regex.captures`, `regex.matches`, `regex.replace` | 706 | command, `functions/rank-services.agent.md` |
| `group lines by <expression on item>` | 686, 677 | `rank-services` |
| `count of rows`, `sort ... descending` (stable), `append`, `for each k, v in map` | 688, 685, 697 | `rank-services` |
| `take top from ranked` | 691 | command |
| `text.pad` | 705 | command |
| record construction `ServiceErrors { ... }` | (Gap 1) | `rank-services` |

## Gaps (constructs SPEC section 22 does not define; the controller decides)

1. **Record construction.** Used: `RegionTotal { region: region, orders: count of rows, revenue: sum of amount in rows }`
   (06, 10). Wanted wording for SPEC 22.1: "A record value is written `Name { field: value, ... }`, every field once, in any
   order; this is the same form as enum variant construction."
2. **`try` on built-in functions that return `result`.** Used: `try do csv.parse(content) as list of Sale -> sales` (06),
   `try do date.parse(digits, "%Y%m%d") -> day` (07), with an `otherwise:` child. Wanted wording for 22.3: "A built-in
   that returns `result of T` is called with `try do module.name(args) [as T] -> var`, exactly like a function whose `out`
   is `result of T` (section 14); `set x = module.name(args)` keeps the `result` value for a `match` (22.1)."
3. **`is some` on a call.** Used: `if regex.captures(name, camera_pattern) is some groups:` and
   `if first of error_lines is some earliest:` (07, 10). Wanted wording for section 14: "`is some` works on any
   expression of type `maybe T`, not only a variable."
4. **Fixture details** (Q404): `expect_status` in `requests.json`, variant folders inheriting `input/`.
5. **Semantics the SPEC leaves open** are recorded as Q400 to Q403 (pad, lines, write/append, regex dialect, path/date,
   flags). The examples avoid `\d`, `\w` and `{n}` in patterns because `{` opens an interpolation hole.
