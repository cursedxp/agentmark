# Import: ../functions/inline-format.agent.md
# Import: ../records/block.agent.md

> The HTML for one line of Markdown. A blank line gives no HTML at all.
# Function: render_block
- in: block: Block
- out: text
- effect: pure
  - set html = ""
  - match block:
    - Heading h:
      - set html = "<h{h.level}>{inline_format(h.content)}</h{h.level}>"
    - ListItem entry:
      - set html = "<li>{inline_format(entry.content)}</li>"
    - Paragraph p:
      - set html = "<p>{inline_format(p.content)}</p>"
    - Blank:
      - set html = ""
  - return html
