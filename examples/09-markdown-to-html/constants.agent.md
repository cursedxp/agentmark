# Constant: bold_pattern
Inline Markdown patterns. In each one, `[*]` is a literal star and `$1` is the text caught between the marks.

- value: "[*][*]([^*]+)[*][*]"

# Constant: italic_pattern
- value: "[*]([^*]+)[*]"

# Constant: link_pattern
A link is `[label](address)`: the label is the first catch, the address the second.

- value: "\\[([^\\]]+)\\]\\(([^)]+)\\)"

# Constant: list_run_pattern
A run of list items, each on its own line. The whole run is caught so it can be wrapped in one list.

- value: "((?:<li>[^\n]*</li>\n)+)"
