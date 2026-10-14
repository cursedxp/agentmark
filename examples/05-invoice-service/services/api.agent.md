# Import: ../constants.agent.md
# Import: ../functions/invoice-fingerprint.agent.md
# Import: ../functions/net-amount.agent.md
# Import: ../records/app-state.agent.md
# Import: ../records/invoice.agent.md

> Serves invoices over HTTP: look one up, see its net amount and fingerprint, add a new one.
# Service: api
- listen: 0.0.0.0:8080
- state: AppState

## GET /invoices/{id}
- set found = first invoice in state.invoices where invoice.id = id
- guard: found is not empty else respond 404
- respond 200 found

## GET /invoices/{id}/net
- set found = first invoice in state.invoices where invoice.id = id
- guard: found is not empty else respond 404
- respond 200 { id: found.id, net: net_amount(found, vat_rate), fingerprint: invoice_fingerprint(found) }

## POST /invoices
- read body as Invoice -> inv
- guard: inv.amount > 0 EUR else respond 400
- append inv to state.invoices
- respond 201 inv
