# Import: ../records/sale.agent.md

> Single picks out of a list: first, last, smallest, largest, distinct.
# Function: pick_words
- in: sales: list of Sale
- effect: local
  - set nothing = empty list of Sale
  - if first of sales is some head:
    - print: "first {head.region}"
  - if last of sales is some tail:
    - print: "last {tail.region}"
  - if first of nothing is empty:
    - print: "first of nothing is empty"
  - if min of amount in sales is some low:
    - print: "min {low}"
  - if max of qty in sales is some most:
    - print: "max qty {most}"
  - if max of region in sales is some late:
    - print: "max region {late}"
  - if min of amount in nothing is empty:
    - print: "min of nothing is empty"
  - print: "sum of nothing {sum of amount in nothing}"
  - print: "regions {join (unique region in sales) with ", "}"
  - print: "distinct {count of (unique (map sales to qty))}"
  - print: "no regions [{join (map nothing to region) with ","}]"
