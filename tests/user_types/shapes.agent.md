> Describes shapes and a menu: two enums, a type alias and every kind of match.
# Enum: Shape
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
- effect: pure
  - match shape:
    - Circle c:
      - return "circle r={c.radius}"
    - Rect r:
      - return "rect {r.width}x{r.height}"
    - Empty:
      - return "nothing"

# Function: roundness
- in: shape: Shape
- out: text
- effect: pure
  - set word = "angular"
  - match shape:
    - Circle c:
      - set word = "round"
    - Rect r:
      - if r.width = r.height:
        - set word = "square"
    - otherwise:
      - set word = "plain"
  - return word

# Function: price_of
- in: menu: Menu
- out: money
- effect: pure
  - match menu:
    - Item i:
      - return i.price
    - Menu.Empty:
      - return 0 EUR

# Function: or_zero
- in: n: maybe int
- out: int
- effect: pure
  - match n:
    - some value:
      - return value
    - none:
      - return 0

# Function: find_entry
- in: rows: Entries, label: text
- out: maybe Entry
- effect: pure
  - return first entry in rows where entry.label = label

# Function: make_entry
- in: label: text
- out: Entry
- effect: pure
  - return Entry { label: label, shape: Shape.Empty }

# Function: read_count
- in: path: text
- out: result of int
- effect: local
  - try read file path as int -> n
  - return n

# Function: basics
- effect: local
  - print: describe(Circle { radius: 3 })
  - print: describe(Shape.Rect { width: 2, height: 5 })
  - print: describe(Shape.Empty)
  - set tea = Item { name: "tea", price: 3.50 EUR }
  - print: "{price_of(tea)} / {price_of(Menu.Empty)}"
  - print: "{or_zero(7)}"

# Command: demo
- args: entries_path: text, count_path: text

## Run
- do basics
- try read file entries_path as Entries -> rows
  otherwise:
    - print: "no entries"
    - stop
- for each entry in rows:
  - print: "{entry.label}: {describe(entry.shape)} ({roundness(entry.shape)})"
  - if entry.shape = Shape.Empty:
    - print: "{entry.label} is empty"
- match find_entry(rows, "b"):
  - some found:
    - print: "found {found.label}"
  - none:
    - print: "no entry b"
- match read_count(count_path):
  - ok total:
    - print: "count {total}"
  - error problem:
    - print: "unreadable"
- try read_count(count_path) -> n
  otherwise:
    - set n = 0
- print: "n {n}"
- if find_entry(rows, "c") is some hit:
  - print: "hit {hit.label}"
- else:
  - print: "no hit"
- set made = make_entry("z")
- print: made.label
