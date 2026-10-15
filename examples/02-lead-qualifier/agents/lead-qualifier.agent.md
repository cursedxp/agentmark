# Uses
- [constants](../constants.agent.md)
- [Lead](../records/lead.agent.md)
- [Qualification](../records/qualification.agent.md)

# Agent: lead-qualifier
Scores every inbound lead and alerts sales about the hot ones. Never contacts the lead itself.

- model: claude:haiku
- tools: crm, email, log

## When crm receives lead
1. ask model with lead: scoring_prompt as Qualification into q
2. guard: q matches Qualification
3. crm update lead: score = q.score
4. if q.score >= hot_lead_score:
   1. email send address "sales@optimotion.dev": "Hot lead: {lead.company} ({q.score}/10). {q.reason}"
   else:
   1. log: "Lead {lead.company} scored {q.score}, no action"
