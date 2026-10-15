> tests/errors/removed-effect-on-function.agent.md:15
>   - effect: pure
>     ^^^^^^
>   "effect:" was removed. Write: - touches: nothing
>   List what it touches: nothing, files, print, AI, or the tools it uses (crm, email). The line may also be left out: the checker works it out.

# Record: Invoice
- amount: money

# Function: net_amount
Takes the VAT off an invoice.

- in: invoice: Invoice, vat: int
- out: money
- effect: pure
- steps:
  1. return invoice.amount
