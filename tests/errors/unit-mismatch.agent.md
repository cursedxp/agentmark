> tests/errors/unit-mismatch.agent.md:13
>   - when: amount + 3 days > 5 EUR
>           ^^^^^^^^^^^^^^^
>   Cannot add a duration to money.
>   Money only combines with money; durations combine with dates and other durations.

# Record: Invoice
- amount: money
- due: date

# Rule: odd
- for: Invoice
- when: amount + 3 days > 5 EUR
