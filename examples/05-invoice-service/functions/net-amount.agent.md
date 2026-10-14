# Import: ../records/invoice.agent.md

> The invoice amount after taking off VAT.
# Function: net_amount
- in: invoice: Invoice, vat: int
- out: money
- effect: pure
  - return invoice.amount - invoice.amount * vat / 100
