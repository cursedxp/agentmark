> Inline Markdown patterns. In each one, `[*]` is a literal star and `$1` is the text caught between the marks.
# Constant: bold_pattern
- value: "[*][*]([^*]+)[*][*]"

# Constant: italic_pattern
- value: "[*]([^*]+)[*]"

> A link is `[label](address)`: the label is the first catch, the address the second.
# Constant: link_pattern
- value: "\\[([^\\]]+)\\]\\(([^)]+)\\)"

> A run of list items, each on its own line. The whole run is caught so it can be wrapped in one list.
# Constant: list_run_pattern
- value: "((?:<li>[^\n]*</li>\n)+)"
