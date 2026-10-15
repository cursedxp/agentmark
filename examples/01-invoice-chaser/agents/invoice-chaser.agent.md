# Uses
- [constants](../constants.agent.md)
- [Invoice](../records/invoice.agent.md)
- [overdue rule](../rules/overdue.agent.md)

# Agent: invoice-chaser
Chases unpaid invoices once a day. Never changes amounts; only sends reminders.

- model: claude
- tools: email, crm, log

## Every day at 09:00
Finds every overdue invoice and reminds its customer.

1. for each overdue invoice in crm.invoices:
   1. do remind

## Do remind
Has the model write one reminder, checks it, sends it and marks the invoice as reminded.

1. ask model with invoice: "Write a short, polite payment reminder. Keep the amount exactly as given." into reminder
2. guard: no amounts changed in reminder
3. guard: reminder length < max_reminder_length
4. email send customer: "{reminder}"
5. crm update invoice: status = "reminded"
6. log: "Reminded {customer} about {amount}"
