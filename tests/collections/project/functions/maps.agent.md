# Import: ../records/sale.agent.md

> Maps: literals, group by, lookup, put, remove, iteration in key order.
# Function: map_words
- in: sales: list of Sale
- effect: local
  - set by_region = group sales by region
  - for each region, rows in by_region:
    - print: "{region}: {count of rows} sales, {sum of amount in rows}"
  - if by_region at "EU" is some eu_rows:
    - print: "EU has {count of eu_rows}"
  - if by_region at "ES" is empty:
    - print: "no ES"
  - set by_qty = group sales by qty
  - print: "qty groups {join (map keys of by_qty to "{item}") with ","}"
  - set ages = map of "ann" to 31, "bob" to 25
  - if ages at "ann" is some years:
    - print: "ann {years}"
  - put 40 at "cy" in ages
  - put 32 at "ann" in ages
  - remove "bob" from ages
  - remove "zed" from ages
  - for each name, age in ages:
    - print: "{name} is {age}"
  - print: "keys {join keys of ages with ","}, ages sum {sum of values of ages}"
  - set nobody = empty map of text to int
  - print: "nobody is empty: {nobody is empty}, {count of (keys of nobody)} keys"
