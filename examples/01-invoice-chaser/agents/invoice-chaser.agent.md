# Import: ../constants.agent.md
# Import: ../records/invoice.agent.md
# Import: ../rules/overdue.agent.md

> Chases unpaid invoices once a day. Never changes amounts; only sends reminders.
# Agent: invoice-chaser
- model: claude
- tools: email, crm, log

## Every day at 09:00
- for each overdue invoice in crm.invoices:
  - do remind

## Do remind
- ask model with invoice: "Write a short, polite payment reminder. Keep the amount exactly as given." -> reminder
- guard: no amounts changed in reminder
- guard: reminder length < max_reminder_length
- email send customer: "{reminder}"
- crm update invoice: status = "reminded"
- log: "Reminded {customer} about {amount}"
