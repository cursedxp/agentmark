# Import: ../records/invoice.agent.md
# Import: ../constants.agent.md

> Unpaid and more than the grace period past due.
# Rule: overdue
- for: Invoice
- when: today > due + grace_period and status != "paid"
