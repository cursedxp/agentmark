# Command: quiet
Every loop variable and element name below is unused on purpose.

- steps:
  1. list of 1, 2, 3 into xs
  2. map xs to 1 into ones
  3. count of xs where true into n
  4. map of "a" to 1 into m
  5. for each x in xs with index i:
     1. print: "row"
  6. for each k, v in m:
     1. print: "entry {k}"
  7. for each y in xs:
     1. print: "again"
  8. print: "{count of ones} {n} {sum of 1 in xs} {count of (unique 7 in xs)}"
