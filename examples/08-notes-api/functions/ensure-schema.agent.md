# Uses
- [constants](../constants.agent.md)

# Import: crate sqlx

# Function: ensure_schema
Creates the notes table when it does not exist yet. Every route calls this right after connecting, because the
service has no start-up step: the database file may be new. Fails when the table cannot be created.

- in: pool: handle of sqlx.SqlitePool
- out: result of bool
- steps:
  1. sqlx query create_notes_table into statement
  2. try statement execute pool
  3. return true
