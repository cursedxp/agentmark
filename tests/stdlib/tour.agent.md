# Program: tour
Every module of the built-in library, and the file tool, in one program (SPEC 22.3).

# Record: Sale
A sale, read from a CSV file.

- region: text
- product: text
- amount: money
- sold: date

# Record: Note
A note, read from JSON.

- id: int
- title: text
- tag: maybe text

# Command: strings
Text and regex functions.

- in: line: text
- steps:
  1. print: "{text.upper(text.trim(line))}|{text.lower("ABC")}|{text.length("häh")}"
  2. print: "[{text.pad("ab", 5)}] [{text.repeat("-", 4)}] {text.replace("a-b-c", "-", "+")}"
  3. text.split("a,b,,c", ",") into parts
  4. text.lines("x\ny\n") into rows
  5. print: "{count of parts} parts, {count of rows} lines"
  6. print: "{text.contains("hello", "ell")} {text.starts_with("hello", "he")} {text.ends_with("hello", "x")}"
  7. print: "{regex.matches("IMG_1.jpg", "^IMG_[0-9]+[.]jpg$")} {regex.replace("**bold**", "[*][*]([a-z]+)[*][*]", "<b>$1</b>")}"
  8. if regex.find("order 4711 shipped", "[0-9]+") is some found:
     1. print: "found {found}"
  9. if regex.captures("IMG_20261003_141500.jpg", "^IMG_([0-9]+)_([0-9]+)") is some groups:
     1. print: "{count of groups} groups"
  10. if regex.captures("nothing", "^IMG_([0-9]+)") is some groups:
      1. print: "unexpected {count of groups}"
      else:
      1. print: "no captures"

# Command: values
Number, money, date and path functions.

- in: unused: text
- steps:
  1. try number.parse("  42 ") into answer
  2. print: "{answer} {number.max(answer, 50)} {number.min(answer, 50)} {number.abs(0 - answer)}"
  3. print: "{number.round(1235, 0 - 1)} {number.format(answer, 2)}"
  4. try money.parse("12.35 EUR") into price
  5. print: "{money.format(price)} {number.round(price, 1)} {number.max(price, 20.00 EUR)}"
  6. try date.parse("03.10.2026", "DD.MM.YYYY") into day
  7. print: "{date.format(day, "YYYY-MM-DD")} {date.format(day, "%d %B %Y")} {date.days_between(day, 2026-10-14)}"
  8. try date.parse("20261399", "%Y%m%d") into never
     otherwise:
     1. print: "not a date"
  9. try number.parse("4x") into never_number
     otherwise:
     1. print: "not a number"
  10. print: "{path.join("photos", "a.jpg")} {path.name("p/a.tar.gz")} {path.stem("p/a.tar.gz")}"
  11. print: "{path.extension("p/a.tar.gz")} {path.parent("p/a.tar.gz")}"
  12. print: "{date.format(now, "YYYY-MM-DD HH:mm")}"

# Command: data
CSV and JSON in both directions, and standard input.

- in: label: text
- steps:
  1. read input into typed
  2. try file read "sales.csv" into content
  3. try do csv.parse(content) as list of Sale into sales
  4. for each sale in sales:
     1. print: "{sale.region} {sale.product} {sale.amount} {date.format(sale.sold, "DD.MM.YYYY")}"
  5. file write "sales-copy.csv": csv.encode(sales)
  6. try json.parse(typed) as list of Note into notes
     otherwise:
     1. print: "{label}: standard input is not a list of notes"
     2. stop
  7. for each note in notes:
     1. if note.tag is some tag:
        1. print: "{note.id} {note.title} #{tag}"
        else:
        1. print: "{note.id} {note.title}"
  8. print: "{json.encode(notes)}"
  9. json.encode({ id: 1, name: label }) into blob
  10. print: "{blob}"
  11. try do json.parse("\{\"id\": 7, \"title\": \"seven\"\}") as Note into one
  12. print: "{one.title} {csv.encode(sales)}"
  13. try do csv.parse("region,product\nEU,Widget\n") as list of Sale into broken
      otherwise:
      1. print: "{label}: a sales file needs amount and sold columns"

# Command: files
The file tool: every verb, traced.

- in: folder: text
- steps:
  1. file write "log.txt": "first"
  2. file append "log.txt": "second"
  3. try file read "log.txt" into written
  4. print: "{text.length(written)} characters"
  5. file exists "log.txt" into there
  6. file list folder into names
  7. for each name in names:
     1. print: "{folder}: {name}"
  8. file rename "log.txt" to path.join(folder, "log-1.txt")
  9. file exists "log.txt" into still
  10. print: "{there} {still}"
  11. file delete path.join(folder, "log-1.txt")
  12. try file read "missing.txt" into nothing
      otherwise:
      1. print: "missing.txt is not there"

# Command: pattern
A pattern the program is given can only be checked when it runs.

- in: pattern: text
- steps:
  1. print: "{regex.matches("abc", pattern)}"
