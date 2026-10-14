# Import: ../functions/lists.agent.md
# Import: ../functions/literals.agent.md
# Import: ../functions/maps.agent.md
# Import: ../functions/picks.agent.md
# Import: ../records/sale.agent.md

> Prints the result of every collection word for the sales in sales.csv.
# Command: words

## Run
- read file "sales.csv" as list of Sale -> sales
- do list_words(sales)
- do pick_words(sales)
- do literal_words
- do map_words(sales)
