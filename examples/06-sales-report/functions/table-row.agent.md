# Import: ../constants.agent.md

> One table row: three cells, the first two padded so the columns line up.
# Function: table_row
- in: left: text, middle: text, right: text
- out: text
- effect: pure
  - return "{text.pad(left, column_width)}{text.pad(middle, column_width)}{right}"
