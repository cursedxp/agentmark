# Import: ../records/sale.agent.md

> Adds up amounts in two currencies: the sum is an error, not a guess.
# Command: mixed

## Run
- read file "mixed.csv" as list of Sale -> sales
- print: "sum {sum of amount in sales}"
