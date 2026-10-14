# Import: crate sqlx
# Import: ../constants.agent.md

> Creates the notes table when it does not exist yet. Every route calls this right after connecting, because the
> service has no start-up step: the database file may be new. Fails when the table cannot be created.
# Function: ensure_schema
- in: pool: handle of sqlx.SqlitePool
- out: result of bool
- effect: network
  - sqlx query create_notes_table -> statement
  - try statement execute pool
  - return true
