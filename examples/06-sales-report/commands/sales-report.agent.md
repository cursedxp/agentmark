# Import: ../constants.agent.md
# Import: ../functions/region-totals.agent.md
# Import: ../functions/table-row.agent.md
# Import: ../records/sale.agent.md

> Reads a sales CSV file and writes a revenue report per region, biggest region first.
# Command: sales-report
- args: path: text, out: text

## Run
- try file read path -> content
  otherwise:
    - print: "Could not read {path}"
    - stop
- try do csv.parse(content) as list of Sale -> sales
  otherwise:
    - print: "{path} is not a valid sales file"
    - stop
- set totals = region_totals(sales)
- set rule = text.repeat("-", rule_width)
- set lines = list of "Sales report", rule, table_row("Region", "Orders", "Revenue")
- for each entry in totals:
  - append table_row(entry.region, "{entry.orders}", "{entry.revenue}") to lines
- append rule to lines
- append table_row("Total", "{count of sales}", "{sum of amount in sales}") to lines
- set report = join lines with "\n"
- file write out: "{report}\n"
- print: "Wrote {count of totals} regions to {out}"
