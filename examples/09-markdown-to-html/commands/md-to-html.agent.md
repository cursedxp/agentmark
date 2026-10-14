# Import: ../functions/classify-line.agent.md
# Import: ../functions/render-block.agent.md
# Import: ../functions/wrap-lists.agent.md

> Converts a small Markdown file into HTML: headings, bullet lists, paragraphs, bold, italic and links.
# Command: md-to-html
- args: source: text, out: text

## Run
- try file read source -> markdown
  otherwise:
    - print: "Could not read {source}"
    - stop
- set lines = text.lines(markdown)
- set elements = empty list of text
- for each line in lines:
  - set rendered = render_block(classify_line(line))
  - if rendered is not empty:
    - append rendered to elements
- set body = join elements with "\n"
- file write out: wrap_lists(body)
- print: "Converted {count of lines} lines of {source} into {count of elements} elements"
- print: "Wrote {out}"
