# Import: ../constants.agent.md
# Import: ../records/ticket.agent.md
# Import: ../records/triage.agent.md

> Classifies tickets that have no priority yet and alerts on the high ones. Never replies to customers.
# Agent: support-triage
- model: claude
- tools: crm, http, log

## Every 15 minutes
- for each ticket in crm.tickets:
  - if ticket.priority is not empty:
    - stop
  - ask model with ticket: triage_prompt as Triage -> t
  > The model must pick one of the three allowed priorities, nothing else.
  - guard: t.priority = "low" or t.priority = "medium" or t.priority = "high"
  - crm update ticket: priority = t.priority
  - if t.priority = "high":
    - http post "https://hooks.example.com/alert": { ticket: ticket.id, category: t.category }
  - log: "Triaged {ticket.id} as {t.priority}/{t.category}"
