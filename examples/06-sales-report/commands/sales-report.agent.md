# Uses
- [constants](../constants.agent.md)
- [region_totals](../functions/region-totals.agent.md)
- [table_row](../functions/table-row.agent.md)
- [Sale](../records/sale.agent.md)

# Command: sales-report
Reads a sales CSV file and writes a revenue report per region, biggest region first.

- in: path: text, out: text
- steps:
  1. try file read path into sales
     otherwise:
     1. print: "Could not read {path}"
     2. stop
  2. region_totals(sales) into totals
  3. text.repeat("-", rule_width) into rule
  4. list of "Sales report", rule, table_row("Region", "Orders", "Revenue") into lines
  5. for each entry in totals:
     1. add table_row(entry.region, "{entry.orders}", "{entry.revenue}") to lines
  6. add rule to lines
  7. add table_row("Total", "{count of sales}", "{sum of amount in sales}") to lines
  8. join lines with "\n" into report
  9. file write out: "{report}\n"
  10. print: "Wrote {count of totals} regions to {out}"
