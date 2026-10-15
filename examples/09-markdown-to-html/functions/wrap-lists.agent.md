# Uses
- [constants](../constants.agent.md)

# Function: wrap_lists
Puts every run of list items between <ul> and </ul>, and ends the page with a newline.

- in: body: text
- out: text
- touches: nothing
- steps:
  1. "{body}\n" into page
  2. return regex.replace(page, list_run_pattern, "<ul>\n$1</ul>\n")
