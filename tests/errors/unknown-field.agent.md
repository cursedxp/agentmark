> tests/errors/unknown-field.agent.md:12
>   - when: amout > 100 EUR
>           ^^^^^
>   Unknown field "amout" on Invoice. Did you mean "amount"?

# Record: Invoice
- id: text
- amount: money

# Rule: big
- for: Invoice
- when: amout > 100 EUR
