# Uses
- [Invoice](../records/invoice.agent.md)

# Command: import-csv
Loads invoices from a CSV file into the application state. With dry_run it only reads and counts.

- in: path: text, dry_run: bool = false
- steps:
  1. try read file path as list of Invoice into invoices
     otherwise:
     1. print: "Could not read {path}"
     2. stop
  2. for each invoice in invoices:
     1. if not dry_run:
        1. add invoice to state.invoices
  3. print: "Imported {count of invoices} rows"
