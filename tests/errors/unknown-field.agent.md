> tests/errors/unknown-field.agent.md:13
>   - when: amout > 100 EUR
>           ^^^^^
>   Unknown field "amout" on Invoice. Did you mean "amount"?

# Record: Invoice
- id: text
- amount: money

# Rule: big Invoice
A big invoice.

- when: amout > 100 EUR
