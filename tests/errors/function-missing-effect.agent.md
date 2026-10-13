> tests/errors/function-missing-effect.agent.md:10
>   # Function: net_amount
>               ^^^^^^^^^^
>   The function "net_amount" does not say what effect it has.
>   Add "- effect: pure" (or local, network, model) under the heading; the compiler checks the body against it.

# Record: Invoice
- amount: money

# Function: net_amount
- in: invoice: Invoice, vat: int
- out: money
