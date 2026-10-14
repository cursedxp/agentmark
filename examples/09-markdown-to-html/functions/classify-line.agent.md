# Import: ../records/block.agent.md

> Decides what kind of Markdown line this is and strips the marks that say so.
# Function: classify_line
- in: line: text
- out: Block
- effect: pure
  - set clean = text.trim(line)
  - if clean = "":
    - return Blank
  - if text.starts_with(clean, "## "):
    - return Heading { level: 2, content: regex.replace(clean, "^## ", "") }
  - if text.starts_with(clean, "# "):
    - return Heading { level: 1, content: regex.replace(clean, "^# ", "") }
  - if text.starts_with(clean, "- "):
    - return ListItem { content: regex.replace(clean, "^- ", "") }
  - return Paragraph { content: clean }
