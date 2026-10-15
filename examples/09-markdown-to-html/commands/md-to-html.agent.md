# Uses
- [classify_line](../functions/classify-line.agent.md)
- [render_block](../functions/render-block.agent.md)
- [wrap_lists](../functions/wrap-lists.agent.md)

# Command: md-to-html
Converts a small Markdown file into HTML: headings, bullet lists, paragraphs, bold, italic and links.

- in: source: text, out: text
- steps:
  1. try file read source into markdown
     otherwise:
     1. print: "Could not read {source}"
     2. stop
  2. text.lines(markdown) into lines
  3. empty list of text into elements
  4. for each line in lines:
     1. render_block(classify_line(line)) into rendered
     2. if rendered is not empty:
        1. add rendered to elements
  5. join elements with "\n" into body
  6. file write out: wrap_lists(body)
  7. print: "Converted {count of lines} lines of {source} into {count of elements} elements"
  8. print: "Wrote {out}"
