# Uses
- [sale](../records/sale.agent.md)

# Function: pick_words
Single picks out of a list: first, last, smallest, largest, distinct.

- in: sales: list of Sale
- steps:
  1. empty list of Sale into nothing
  2. if first of sales is some head:
     1. print: "first {head.region}"
  3. if last of sales is some tail:
     1. print: "last {tail.region}"
  4. if first of nothing is empty:
     1. print: "first of nothing is empty"
  5. if min of amount in sales is some low:
     1. print: "min {low}"
  6. if max of qty in sales is some most:
     1. print: "max qty {most}"
  7. if max of region in sales is some late:
     1. print: "max region {late}"
  8. if min of amount in nothing is empty:
     1. print: "min of nothing is empty"
  9. print: "sum of nothing {sum of amount in nothing}"
  10. print: "regions {join (unique region in sales) with ", "}"
  11. print: "distinct {count of (unique (map sales to qty))}"
  12. print: "no regions [{join (map nothing to region) with ","}]"
