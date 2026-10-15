# Uses
- [Invoice](../records/invoice.agent.md)

# Function: net_amount
The invoice amount after taking off VAT.

- in: invoice, vat: int
- out: money
- touches: nothing
- steps:
  1. return invoice.amount - invoice.amount * vat / 100
