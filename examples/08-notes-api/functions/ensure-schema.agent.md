# Import: crate sqlx
# Import: ../constants.agent.md

> Creates the notes table when it does not exist yet. True when the table is there afterwards.
# Function: ensure_schema
- in: pool: handle of sqlx.SqlitePool
- out: bool
- effect: network
  - sqlx query create_notes_table -> statement
  - try statement execute pool
    otherwise:
      - return false
  - return true
