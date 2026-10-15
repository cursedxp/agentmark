# Uses
- [constants](../constants.agent.md)
- [Ticket](../records/ticket.agent.md)
- [Triage](../records/triage.agent.md)

# Agent: support-triage
Classifies tickets that have no priority yet and alerts on the high ones. Never replies to customers.

- model: claude
- tools: crm, http, log

## Every 15 minutes
Looks at every ticket that has no priority yet. The model must pick one of the three allowed priorities (low,
medium or high), nothing else.

1. for each ticket in crm.tickets:
   1. if ticket.priority is not empty:
      1. stop
   2. ask model with ticket: triage_prompt as Triage into t
   3. guard: t.priority = "low" or t.priority = "medium" or t.priority = "high"
   4. crm update ticket: priority = t.priority
   5. if t.priority = "high":
      1. http post "https://hooks.example.com/alert": { ticket: ticket.id, category: t.category }
   6. log: "Triaged {ticket.id} as {t.priority}/{t.category}"
