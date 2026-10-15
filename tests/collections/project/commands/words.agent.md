# Uses
- [lists](../functions/lists.agent.md)
- [literals](../functions/literals.agent.md)
- [maps](../functions/maps.agent.md)
- [picks](../functions/picks.agent.md)
- [sale](../records/sale.agent.md)

# Command: words
Prints the result of every collection word for the sales in sales.csv.

- steps:
  1. read file "sales.csv" as list of Sale into sales
  2. do list_words(sales)
  3. do pick_words(sales)
  4. do literal_words
  5. do map_words(sales)
