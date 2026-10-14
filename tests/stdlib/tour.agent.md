> Every module of the built-in library, and the file tool, in one program (SPEC 22.3).
# Record: Sale
- region: text
- product: text
- amount: money
- sold: date

# Record: Note
- id: int
- title: text
- tag: maybe text

> Text and regex functions.
# Command: strings
- args: line: text

## Run
- print: "{text.upper(text.trim(line))}|{text.lower("ABC")}|{text.length("häh")}"
- print: "[{text.pad("ab", 5)}] [{text.repeat("-", 4)}] {text.replace("a-b-c", "-", "+")}"
- set parts = text.split("a,b,,c", ",")
- set rows = text.lines("x\ny\n")
- print: "{count of parts} parts, {count of rows} lines"
- print: "{text.contains("hello", "ell")} {text.starts_with("hello", "he")} {text.ends_with("hello", "x")}"
- print: "{regex.matches("IMG_1.jpg", "^IMG_[0-9]+[.]jpg$")} {regex.replace("**bold**", "[*][*]([a-z]+)[*][*]", "<b>$1</b>")}"
- if regex.find("order 4711 shipped", "[0-9]+") is some found:
  - print: "found {found}"
- if regex.captures("IMG_20261003_141500.jpg", "^IMG_([0-9]+)_([0-9]+)") is some groups:
  - print: "{count of groups} groups"
- if regex.captures("nothing", "^IMG_([0-9]+)") is some groups:
  - print: "unexpected {count of groups}"
- else:
  - print: "no captures"
> Number, money, date and path functions.
# Command: values
- args: unused: text

## Run
- try number.parse("  42 ") -> answer
- print: "{answer} {number.max(answer, 50)} {number.min(answer, 50)} {number.abs(0 - answer)}"
- print: "{number.round(1235, 0 - 1)} {number.format(answer, 2)}"
- try money.parse("12.35 EUR") -> price
- print: "{money.format(price)} {number.round(price, 1)} {number.max(price, 20.00 EUR)}"
- try date.parse("03.10.2026", "DD.MM.YYYY") -> day
- print: "{date.format(day, "YYYY-MM-DD")} {date.format(day, "%d %B %Y")} {date.days_between(day, 2026-10-14)}"
- try date.parse("20261399", "%Y%m%d") -> never
  otherwise:
    - print: "not a date"
- try number.parse("4x") -> never_number
  otherwise:
    - print: "not a number"
- print: "{path.join("photos", "a.jpg")} {path.name("p/a.tar.gz")} {path.stem("p/a.tar.gz")}"
- print: "{path.extension("p/a.tar.gz")} {path.parent("p/a.tar.gz")}"

> CSV and JSON in both directions, and standard input.
# Command: data
- args: label: text

## Run
- read input -> typed
- try file read "sales.csv" -> content
- try do csv.parse(content) as list of Sale -> sales
- for each sale in sales:
  - print: "{sale.region} {sale.product} {sale.amount} {date.format(sale.sold, "DD.MM.YYYY")}"
- file write "sales-copy.csv": csv.encode(sales)
- try json.parse(typed) as list of Note -> notes
  otherwise:
    - print: "{label}: standard input is not a list of notes"
    - stop
- for each note in notes:
  - if note.tag is some tag:
    - print: "{note.id} {note.title} #{tag}"
  - else:
    - print: "{note.id} {note.title}"
- print: "{json.encode(notes)}"
- try do json.parse("\{\"id\": 7, \"title\": \"seven\"\}") as Note -> one
- print: "{one.title} {csv.encode(sales)}"
- try do csv.parse("region,product\nEU,Widget\n") as list of Sale -> broken
  otherwise:
    - print: "{label}: a sales file needs amount and sold columns"

> The file tool: every verb, traced.
# Command: files
- args: folder: text

## Run
- file write "log.txt": "first"
- file append "log.txt": "second"
- try file read "log.txt" -> written
- print: "{text.length(written)} characters"
- file exists "log.txt" -> there
- file list folder -> names
- for each name in names:
  - print: "{folder}: {name}"
- file rename "log.txt" to path.join(folder, "log-1.txt")
- file exists "log.txt" -> still
- print: "{there} {still}"
- file delete path.join(folder, "log-1.txt")
- try file read "missing.txt" -> nothing
  otherwise:
    - print: "missing.txt is not there"
