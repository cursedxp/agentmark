# Uses
- [Block](../records/block.agent.md)

# Function: classify_line
Decides what kind of Markdown line this is and strips the marks that say so.

- in: line: text
- out: Block
- touches: nothing
- steps:
  1. text.trim(line) into clean
  2. if clean = "":
     1. return Blank
  3. if text.starts_with(clean, "## "):
     1. return Heading { level: 2, content: regex.replace(clean, "^## ", "") }
  4. if text.starts_with(clean, "# "):
     1. return Heading { level: 1, content: regex.replace(clean, "^# ", "") }
  5. if text.starts_with(clean, "- "):
     1. return ListItem { content: regex.replace(clean, "^- ", "") }
  6. return Paragraph { content: clean }
