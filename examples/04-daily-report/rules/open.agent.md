# Import: ../records/invoice.agent.md

> Not paid yet, whether or not it is late.
# Rule: open
- for: Invoice
- when: status != "paid"
