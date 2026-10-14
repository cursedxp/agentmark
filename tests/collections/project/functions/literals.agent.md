> Appending to a parameter changes a copy: the caller keeps its list.
# Function: grown
- in: xs: list of int
- out: int
- effect: pure
  - append 4 to xs
  - return count of xs

> Literals, the element name `item`, and `append`.
# Function: literal_words
- effect: local
  - set numbers = list of 3, 1, 2
  - set names = list of "b", "a", "c"
  - print: "numbers {sum of numbers}"
  - if min of item in numbers is some low:
    - print: "min {low}"
  - print: "sorted {join (sort names by item) with "+"}"
  - print: "not a {join (names where item != "a") with ""}"
  - print: "shouted {join (map names to "{item}!") with " "}"
  - append "d" to names
  - print: "{count of names} names"
  - if last of names is some tail:
    - print: "last {tail}"
  - set nobody = empty list of text
  - print: "nobody is empty: {nobody is empty}"
  - set words = list of "x", "y", "x"
  - print: "distinct words {join (unique words) with ""}"
  - print: "grown {grown(numbers)}, still {count of numbers}"
  - set nested = list of (list of 1, 2, 3), list of 4
  - print: "sums {join (map (map nested to sum of item) to "{item}") with ","}"
  - print: "inner item {count of (nested where (count of item where item > 1) > 1)}"
