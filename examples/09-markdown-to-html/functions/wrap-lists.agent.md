# Import: ../constants.agent.md

> Puts every run of list items between <ul> and </ul>, and ends the page with a newline.
# Function: wrap_lists
- in: body: text
- out: text
- effect: pure
  - set page = "{body}\n"
  - return regex.replace(page, list_run_pattern, "<ul>\n$1</ul>\n")
