# Function: grown
Appending to a parameter changes a copy: the caller keeps its list.

- in: xs: list of int
- out: int
- touches: nothing
- steps:
  1. add 4 to xs
  2. return count of xs

# Function: literal_words
Literals, the element name `item`, and `append`.

- steps:
  1. list of 3, 1, 2 into numbers
  2. list of "b", "a", "c" into names
  3. print: "numbers {sum of numbers}"
  4. if min of item in numbers is some low:
     1. print: "min {low}"
  5. print: "sorted {join (sort names by item) with "+"}"
  6. print: "not a {join (names where item != "a") with ""}"
  7. print: "shouted {join (map names to "{item}!") with " "}"
  8. add "d" to names
  9. print: "{count of names} names"
  10. if last of names is some tail:
      1. print: "last {tail}"
  11. empty list of text into nobody
  12. print: "nobody is empty: {nobody is empty}"
  13. list of "x", "y", "x" into words
  14. print: "distinct words {join (unique words) with ""}"
  15. print: "grown {grown(numbers)}, still {count of numbers}"
  16. list of (list of 1, 2, 3), list of 4 into nested
  17. print: "sums {join (map (map nested to sum of item) to "{item}") with ","}"
  18. print: "inner item {count of (nested where (count of item where item > 1) > 1)}"
