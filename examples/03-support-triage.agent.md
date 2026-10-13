# Record: Ticket
- id: text
- subject: text
- body: text
- priority: maybe text

# Record: Triage
- priority: text
- category: text

# Agent: support-triage
- model: claude
- tools: crm, http, log

## Every 15 minutes
for each ticket in crm.tickets:
  - if ticket.priority is not empty:
    - stop
  - ask model with ticket: "Classify priority as low, medium or high and category as billing, technical or other." as Triage -> t
  - guard: t.priority = "low" or t.priority = "medium" or t.priority = "high"
  - crm update ticket: priority = t.priority
  - if t.priority = "high":
    - http post "https://hooks.example.com/alert": { ticket: ticket.id, category: t.category }
  - log: "Triaged {ticket.id} as {t.priority}/{t.category}"
