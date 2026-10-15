# Uses
- [constants](../constants.agent.md)
- [Invoice](../records/invoice.agent.md)

# Rule: overdue Invoice
Unpaid and more than the grace period past due.

- when: today > due + grace_period and status != "paid"
