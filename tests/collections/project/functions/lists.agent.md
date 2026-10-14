# Import: ../records/sale.agent.md

> Filters, maps, sorts and slices of a list of sales.
# Function: list_words
- in: sales: list of Sale
- effect: local
  - set eu = sales where region = "EU"
  - print: "eu rows: {count of eu}"
  - set amounts = map sales to amount
  - print: "amounts {count of amounts}, sum {sum of amounts}"
  - set line_totals = map sales to amount * qty
  - print: "line totals {sum of line_totals}"
  - set ranked = sort sales by amount descending
  - for each row in ranked with index i:
    - print: "{i}. {row.region} {row.amount}"
  - set cheapest_first = sort sales by amount
  - print: "cheapest {join (map cheapest_first to region) with ","}"
  - print: "by region {join (map (sort sales by region) to region) with " "}"
  - print: "big {count of sales where amount > 5 EUR}"
  - print: "top {count of (take 2 from sales)}, rest {count of (skip 4 from sales)}"
  - print: "none left {count of (skip 10 from sales)}, nothing {count of (take 0 from sales)}"
