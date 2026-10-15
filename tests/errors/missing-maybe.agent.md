> tests/errors/missing-maybe.agent.md:17
>     1. return first invoice in invoices where invoice.id = wanted
>               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
>   This can find nothing, but the function says it returns "Invoice".
>   Declare the result as "maybe Invoice" and check it with "is some" or "is not empty" before using it.

# Record: Invoice
- id: text
- status: text

# Function: find_invoice
Finds one invoice by its id.

- in: invoices: list of Invoice, wanted: text
- out: Invoice
- steps:
  1. return first invoice in invoices where invoice.id = wanted
