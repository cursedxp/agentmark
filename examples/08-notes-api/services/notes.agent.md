# Import: crate sqlx
# Import: ../constants.agent.md
# Import: ../functions/ensure-schema.agent.md
# Import: ../records/new-note.agent.md
# Import: ../records/note.agent.md

> A small notes API: create, list, read and delete notes. Everything is stored in SQLite.
> Every route opens the database, makes sure the table exists, and then runs its query.
> Rows are not read column by column: SQLite's json_object turns a row into JSON text and `json.parse` reads it.
> (Typed row access is not exercised in v0.3.)
# Service: notes
- listen: 0.0.0.0:8081

## POST /notes
- read body as NewNote -> draft
- guard: draft.title is not empty else respond 400
- try sqlx sqlite_pool_connect database_url -> pool
  otherwise:
    - respond 503
- try ensure_schema(pool)
  otherwise:
    - respond 503
- sqlx query "insert into notes (title, body) values (?, ?)" -> statement
- statement bind draft.title -> with_title
- with_title bind draft.body -> insert
- try insert execute pool -> outcome
  otherwise:
    - respond 500
- outcome last_insert_rowid -> id
- respond 201 Note { id: id, title: draft.title, body: draft.body }

## GET /notes
- try sqlx sqlite_pool_connect database_url -> pool
  otherwise:
    - respond 503
- try ensure_schema(pool)
  otherwise:
    - respond 503
- sqlx query_scalar "select json_group_array({note_json}) from (select * from notes order by id)" -> listing
- try listing fetch_one pool -> encoded
  otherwise:
    - respond 500
- set parsed = json.parse(encoded) as list of Note
- match parsed:
  - ok notes:
    - respond 200 notes
  - error:
    - respond 500

## GET /notes/{id}
- try sqlx sqlite_pool_connect database_url -> pool
  otherwise:
    - respond 503
- try ensure_schema(pool)
  otherwise:
    - respond 503
- sqlx query_scalar "select {note_json} from notes where id = ?" -> statement
- statement bind id -> lookup
- try lookup fetch_optional pool -> found
  otherwise:
    - respond 500
- guard: found is not empty else respond 404
- set parsed = json.parse(found) as Note
- match parsed:
  - ok note:
    - respond 200 note
  - error:
    - respond 500

## DELETE /notes/{id}
- try sqlx sqlite_pool_connect database_url -> pool
  otherwise:
    - respond 503
- try ensure_schema(pool)
  otherwise:
    - respond 503
- sqlx query "delete from notes where id = ?" -> statement
- statement bind id -> removal
- try removal execute pool -> outcome
  otherwise:
    - respond 500
- outcome rows_affected -> removed
- guard: removed > 0 else respond 404
- respond 204
