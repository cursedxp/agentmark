# Uses
- [ledger](../tools/ledger.agent.md)
- [slugger](../tools/slugger.agent.md)

# Command: label
Writes a title to the ledger as a file-name word, repeated `count` times.

- in: title: text, count: int = 1, dry_run: bool = false
- steps:
  1. slugger.slug(title) into slug
  2. if dry_run:
     1. print: "would write {slug}"
     2. stop
  3. ledger add "{slugger.shout(slug)} x{count}"
  4. ledger add_twice slug "again"
  5. print: "wrote {slug}"
