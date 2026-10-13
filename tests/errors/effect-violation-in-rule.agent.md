> tests/errors/effect-violation-in-rule.agent.md:19
>   - when: live_status(invoice) = "open"
>           ^^^^^^^^^^^^^^^^^^^^
>   A rule must be pure, but "live_status" has the effect network.
>   Rules are pure checks: they may only read the record's own fields and call pure functions.

# Record: Invoice
- id: text
- status: text

# Function: live_status
- in: invoice: Invoice
- out: text
- effect: network
  - return invoice.status

# Rule: is_open
- for: Invoice
- when: live_status(invoice) = "open"
