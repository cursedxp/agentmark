# Record: Lead
- id: text
- company: text
- message: text
- email: text
- score: maybe int

# Record: Qualification
- score: int
- reason: text

# Agent: lead-qualifier
- model: claude:haiku
- tools: crm, email, log

## When crm receives lead
- ask model with lead: "Score this inbound lead from 1 to 10 for a web agency and explain why in one sentence." as Qualification -> q
- guard: q matches Qualification
- crm update lead: score = q.score
- if q.score >= 7:
  - email send address "sales@optimotion.dev": "Hot lead: {lead.company} ({q.score}/10). {q.reason}"
- else:
  - log: "Lead {lead.company} scored {q.score}, no action"
