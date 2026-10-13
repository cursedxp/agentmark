# Record: Invoice
- id: text
- customer: text
- amount: money
- due: date
- status: text

# Rule: overdue
- for: Invoice
- when: today > due + 14 days and status != "paid"

# Rule: open
- for: Invoice
- when: status != "paid"

# Agent: daily-report
- model: claude
- tools: crm, email, log

## Every Monday at 08:00
- set open_count = count of open invoices in crm.invoices
- set overdue_count = count of overdue invoices in crm.invoices
- ask model: "Write a two-sentence Monday summary. Open invoices: {open_count}. Overdue: {overdue_count}." -> summary
- guard: summary is not empty
- email send address "anil@optimotion.dev": "{summary}"
- log: "Report sent: {open_count} open, {overdue_count} overdue"
