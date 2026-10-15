# Uses
- [sale](../records/sale.agent.md)

# Function: map_words
Maps: literals, group by, lookup, put, remove, iteration in key order.

- in: sales: list of Sale
- steps:
  1. group sales by region into by_region
  2. for each region, rows in by_region:
     1. print: "{region}: {count of rows} sales, {sum of amount in rows}"
  3. if by_region at "EU" is some eu_rows:
     1. print: "EU has {count of eu_rows}"
  4. if by_region at "ES" is empty:
     1. print: "no ES"
  5. group sales by qty into by_qty
  6. print: "qty groups {join (map keys of by_qty to "{item}") with ","}"
  7. map of "ann" to 31, "bob" to 25 into ages
  8. if ages at "ann" is some years:
     1. print: "ann {years}"
  9. put 40 at "cy" in ages
  10. put 32 at "ann" in ages
  11. remove "bob" from ages
  12. remove "zed" from ages
  13. for each name, age in ages:
      1. print: "{name} is {age}"
  14. print: "keys {join keys of ages with ","}, ages sum {sum of values of ages}"
  15. empty map of text to int into nobody
  16. print: "nobody is empty: {nobody is empty}, {count of (keys of nobody)} keys"
