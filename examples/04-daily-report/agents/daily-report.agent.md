# Uses
- [Invoice](../records/invoice.agent.md)
- [open rule](../rules/open.agent.md)
- [overdue rule](../rules/overdue.agent.md)

# Agent: daily-report
Mails Anil a two-sentence summary of open and overdue invoices every Monday. Reads only; changes nothing.

- model: claude
- tools: crm, email, log

## Every Monday at 08:00
Counts the open and the overdue invoices, has the model write the summary, and mails it.

1. count of open invoices in crm.invoices into open_count
2. count of overdue invoices in crm.invoices into overdue_count
3. ask model: "Write a two-sentence Monday summary. Open invoices: {open_count}. Overdue: {overdue_count}." into summary
4. guard: summary is not empty
5. email send address "anil@optimotion.dev": "{summary}"
6. log: "Report sent: {open_count} open, {overdue_count} overdue"
