# Uses
- [constants](../constants.agent.md)

# Function: inline_format
Turns the marks inside a line into HTML: & is escaped first, then bold, italic and links.

- in: content: text
- out: text
- touches: nothing
- steps:
  1. text.replace(content, "&", "&amp;") into escaped
  2. regex.replace(escaped, bold_pattern, "<strong>$1</strong>") into bold
  3. regex.replace(bold, italic_pattern, "<em>$1</em>") into italic
  4. return regex.replace(italic, link_pattern, "<a href=\"$2\">$1</a>")
