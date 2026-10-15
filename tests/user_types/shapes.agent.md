# Enum: Shape
Describes shapes and a menu: two enums, a type alias and every kind of match.

- Circle: radius: int
- Rect: width: int, height: int
- Empty

# Enum: Menu
- Empty
- Item: name: text, price: money

# Record: Entry
- label: text
- shape: Shape

# Type: Entries
- is: list of Entry

# Function: describe
- in: shape: Shape
- out: text
- touches: nothing
- steps:
  1. match shape:
     - Circle c:
       1. return "circle r={c.radius}"
     - Rect r:
       1. return "rect {r.width}x{r.height}"
     - Empty:
       1. return "nothing"

# Function: roundness
- in: shape: Shape
- out: text
- touches: nothing
- steps:
  1. "angular" into word
  2. match shape:
     - Circle c:
       1. "round" into word
     - Rect r:
       1. if r.width = r.height:
          1. "square" into word
     - otherwise:
       1. "plain" into word
  3. return word

# Function: price_of
- in: menu: Menu
- out: money
- touches: nothing
- steps:
  1. match menu:
     - Item i:
       1. return i.price
     - Menu.Empty:
       1. return 0 EUR

# Function: or_zero
- in: n: maybe int
- out: int
- touches: nothing
- steps:
  1. match n:
     - some value:
       1. return value
     - none:
       1. return 0

# Function: find_entry
- in: rows: Entries, label: text
- out: maybe Entry
- touches: nothing
- steps:
  1. return first entry in rows where entry.label = label

# Function: make_entry
- in: label: text
- out: Entry
- touches: nothing
- steps:
  1. return Entry { label: label, shape: Shape.Empty }

# Function: read_count
- in: path: text
- out: result of int
- steps:
  1. try read file path as int into n
  2. return n

# Function: basics
- steps:
  1. print: describe(Circle { radius: 3 })
  2. print: describe(Shape.Rect { width: 2, height: 5 })
  3. print: describe(Shape.Empty)
  4. Item { name: "tea", price: 3.50 EUR } into tea
  5. print: "{price_of(tea)} / {price_of(Menu.Empty)}"
  6. print: "{or_zero(7)}"

# Command: demo
- in: entries_path: text, count_path: text
- steps:
  1. do basics
  2. try read file entries_path as Entries into rows
     otherwise:
     1. print: "no entries"
     2. stop
  3. for each entry in rows:
     1. print: "{entry.label}: {describe(entry.shape)} ({roundness(entry.shape)})"
     2. if entry.shape = Shape.Empty:
        1. print: "{entry.label} is empty"
  4. match find_entry(rows, "b"):
     - some found:
       1. print: "found {found.label}"
     - none:
       1. print: "no entry b"
  5. match read_count(count_path):
     - ok total:
       1. print: "count {total}"
     - error problem:
       1. print: "unreadable"
  6. try read_count(count_path) into n
     otherwise:
     1. 0 into n
  7. print: "n {n}"
  8. if find_entry(rows, "c") is some hit:
     1. print: "hit {hit.label}"
     else:
     1. print: "no hit"
  9. make_entry("z") into made
  10. print: made.label
