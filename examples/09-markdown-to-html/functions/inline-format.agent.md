# Import: ../constants.agent.md

> Turns the marks inside a line into HTML: & is escaped first, then bold, italic and links.
# Function: inline_format
- in: content: text
- out: text
- effect: pure
  - set escaped = text.replace(content, "&", "&amp;")
  - set bold = regex.replace(escaped, bold_pattern, "<strong>$1</strong>")
  - set italic = regex.replace(bold, italic_pattern, "<em>$1</em>")
  - return regex.replace(italic, link_pattern, "<a href=\"$2\">$1</a>")
