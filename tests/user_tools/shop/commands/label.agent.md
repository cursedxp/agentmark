# Import: ../tools/ledger.agent.md
# Import: ../tools/slugger.agent.md

> Writes a title to the ledger as a file-name word, repeated `count` times.
# Command: label
- args: title: text, count: int = 1, dry_run: bool = false

## Run
- set slug = slugger.slug(title)
- if dry_run:
  - print: "would write {slug}"
  - stop
- ledger add "{slugger.shout(slug)} x{count}"
- ledger add_twice slug "again"
- print: "wrote {slug}"
