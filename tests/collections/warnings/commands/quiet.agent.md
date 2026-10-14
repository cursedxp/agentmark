> Every loop variable and element name below is unused on purpose.
# Command: quiet

## Run
- set xs = list of 1, 2, 3
- set ones = map xs to 1
- set n = count of xs where true
- set m = map of "a" to 1
- for each x in xs with index i:
  - print: "row"
- for each k, v in m:
  - print: "entry {k}"
- for each y in xs:
  - print: "again"
- print: "{count of ones} {n} {sum of 1 in xs} {count of (unique 7 in xs)}"
