# Uses
- [inline_format](../functions/inline-format.agent.md)
- [Block](../records/block.agent.md)

# Function: render_block
The HTML for one line of Markdown. A blank line gives no HTML at all.

- in: block
- out: text
- touches: nothing
- steps:
  1. "" into html
  2. match block:
     - Heading h:
       1. "<h{h.level}>{inline_format(h.content)}</h{h.level}>" into html
     - ListItem entry:
       1. "<li>{inline_format(entry.content)}</li>" into html
     - Paragraph p:
       1. "<p>{inline_format(p.content)}</p>" into html
     - Blank:
       1. "" into html
  3. return html
