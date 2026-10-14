> A plain text ledger in the working directory.
# Tool: ledger
- effect: local

## add(entry: text)
- file append "ledger.txt": entry

> Adds the entry twice: a verb may call the verbs of its own tool.
## add_twice(entry: text, mark: text)
- ledger add entry
- ledger add "{mark} {entry}"
