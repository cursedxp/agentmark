> Non-agent example: an HTTP service plus a CLI command, using a crate binding.
> Proves that Layer 2 + Layer 1 can build an ordinary Rust application.

# Import: crate sha2

# Record: Invoice
- id: text
- customer: text
- amount: money
- due: date
- status: text

# Record: AppState
- invoices: list of Invoice

# Function: net_amount
- in: invoice: Invoice, vat: int
- out: money
- effect: pure
  - return invoice.amount - invoice.amount * vat / 100

## Rust: fingerprint -> text
- in: invoice: Invoice
- effect: pure
  ```rust
  use sha2::{Digest, Sha256};
  format!("{:x}", Sha256::digest(format!("{}{}", invoice.id, invoice.amount).as_bytes()))
  ```

# Service: api
- listen: 0.0.0.0:8080
- state: AppState

## GET /invoices/{id}
- set found = first invoice in state.invoices where invoice.id = id
- guard: found is not empty else respond 404
- respond 200 found

## GET /invoices/{id}/net
- set found = first invoice in state.invoices where invoice.id = id
- guard: found is not empty else respond 404
- respond 200 { id: found.id, net: net_amount(found, 19), fingerprint: fingerprint(found) }

## POST /invoices
- read body as Invoice -> inv
- guard: inv.amount > 0 EUR else respond 400
- append inv to state.invoices
- respond 201 inv

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
