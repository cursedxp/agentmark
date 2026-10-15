# Tool: ledger
A plain text ledger in the working directory.

- touches: local

## add(entry: text)
1. file append "ledger.txt": entry

## add_twice(entry: text, mark: text)
Adds the entry twice: a verb may call the verbs of its own tool.

1. ledger add entry
2. ledger add "{mark} {entry}"
