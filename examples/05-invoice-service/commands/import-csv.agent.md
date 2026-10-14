# Import: ../records/invoice.agent.md

> Loads invoices from a CSV file into the application state. With dry_run it only reads and counts.
# Command: import-csv
- args: path: text, dry_run: bool = false

## Run
- try read file path as list of Invoice -> rows
  otherwise:
    - print: "Could not read {path}"
    - stop
- for each row in rows:
  - if not dry_run:
    - append row to state.invoices
- print: "Imported {count of rows} rows"
