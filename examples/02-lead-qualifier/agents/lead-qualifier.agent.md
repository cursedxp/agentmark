# Import: ../constants.agent.md
# Import: ../records/lead.agent.md
# Import: ../records/qualification.agent.md

> Scores every inbound lead and alerts sales about the hot ones. Never contacts the lead itself.
# Agent: lead-qualifier
- model: claude:haiku
- tools: crm, email, log

## When crm receives lead
- ask model with lead: scoring_prompt as Qualification -> q
- guard: q matches Qualification
- crm update lead: score = q.score
- if q.score >= hot_lead_score:
  - email send address "sales@optimotion.dev": "Hot lead: {lead.company} ({q.score}/10). {q.reason}"
- else:
  - log: "Lead {lead.company} scored {q.score}, no action"
