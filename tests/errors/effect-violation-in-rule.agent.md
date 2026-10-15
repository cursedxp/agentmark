> tests/errors/effect-violation-in-rule.agent.md:23
>   - when: live_status(invoice) = "open"
>           ^^^^^^^^^^^^^^^^^^^^
>   A rule may touch nothing, but "live_status" touches crm.
>   Rules are plain checks: they may only read the record's own fields and call functions that touch nothing.

# Record: Invoice
- id: text
- status: text

# Function: live_status
Asks the CRM for the live status.

- in: invoice: Invoice
- out: text
- touches: crm
- steps:
  1. return invoice.status

# Rule: is_open Invoice
Open on the CRM.

- when: live_status(invoice) = "open"
