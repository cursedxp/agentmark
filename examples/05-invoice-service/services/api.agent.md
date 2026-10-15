# Uses
- [constants](../constants.agent.md)
- [invoice_fingerprint](../functions/invoice-fingerprint.agent.md)
- [net_amount](../functions/net-amount.agent.md)
- [AppState](../records/app-state.agent.md)
- [Invoice](../records/invoice.agent.md)

# Service: api
Serves invoices over HTTP: look one up, see its net amount and fingerprint, add a new one.

- listen: 0.0.0.0:8080
- state: AppState

## GET /invoices/{id}
1. first invoice in state.invoices where invoice.id = id into found
2. guard: found is not empty else respond 404
3. respond 200 found

## GET /invoices/{id}/net
1. first invoice in state.invoices where invoice.id = id into found
2. guard: found is not empty else respond 404
3. respond 200 { id: found.id, net: net_amount(found, vat_rate), fingerprint: invoice_fingerprint(found) }

## POST /invoices
1. read body as Invoice into inv
2. guard: inv.amount > 0 EUR else respond 400
3. add inv to state.invoices
4. respond 201 inv
