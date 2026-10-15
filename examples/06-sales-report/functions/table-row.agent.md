# Uses
- [constants](../constants.agent.md)

# Function: table_row
One table row: three cells, the first two padded so the columns line up.

- in: left: text, middle: text, right: text
- out: text
- touches: nothing
- steps:
  1. return "{text.pad(left, column_width)}{text.pad(middle, column_width)}{right}"
