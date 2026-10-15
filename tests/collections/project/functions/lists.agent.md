# Uses
- [sale](../records/sale.agent.md)

# Function: list_words
Filters, maps, sorts and slices of a list of sales.

- in: sales: list of Sale
- steps:
  1. sales where region = "EU" into eu
  2. print: "eu rows: {count of eu}"
  3. map sales to amount into amounts
  4. print: "amounts {count of amounts}, sum {sum of amounts}"
  5. map sales to amount * qty into line_totals
  6. print: "line totals {sum of line_totals}"
  7. sort sales by amount descending into ranked
  8. for each row in ranked with index i:
     1. print: "{i}. {row.region} {row.amount}"
  9. sort sales by amount into cheapest_first
  10. print: "cheapest {join (map cheapest_first to region) with ","}"
  11. print: "by region {join (map (sort sales by region) to region) with " "}"
  12. print: "big {count of sales where amount > 5 EUR}"
  13. print: "top {count of (take 2 from sales)}, rest {count of (skip 4 from sales)}"
  14. print: "none left {count of (skip 10 from sales)}, nothing {count of (take 0 from sales)}"
