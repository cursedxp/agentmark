# Uses
- [constants](../constants.agent.md)
- [ensure_schema](../functions/ensure-schema.agent.md)
- [NewNote](../records/new-note.agent.md)
- [Note](../records/note.agent.md)

# Import: crate sqlx

# Service: notes
A small notes API: create, list, read and delete notes. Everything is stored in SQLite.
Every route opens the database, makes sure the table exists, and then runs its query.
Rows are not read column by column: SQLite's json_object turns a row into JSON text and `json.parse` reads it.
(Typed row access is not exercised in v0.3.)

- listen: 0.0.0.0:8081

## POST /notes
1. read body as NewNote into draft
2. guard: draft.title is not empty else respond 400
3. try sqlx sqlite_pool_connect database_url into pool
   otherwise:
   1. respond 503
4. try ensure_schema(pool)
   otherwise:
   1. respond 503
5. sqlx query "insert into notes (title, body) values (?, ?)" into statement
6. statement bind draft.title into with_title
7. with_title bind draft.body into insert
8. try insert execute pool into outcome
   otherwise:
   1. respond 500
9. outcome last_insert_rowid into id
10. respond 201 Note { id: id, title: draft.title, body: draft.body }

## GET /notes
1. try sqlx sqlite_pool_connect database_url into pool
   otherwise:
   1. respond 503
2. try ensure_schema(pool)
   otherwise:
   1. respond 503
3. sqlx query_scalar "select json_group_array({note_json}) from (select * from notes order by id)" into listing
4. try listing fetch_one pool into encoded
   otherwise:
   1. respond 500
5. json.parse(encoded) as list of Note into parsed
6. match parsed:
   - ok notes:
     1. respond 200 notes
   - error:
     1. respond 500

## GET /notes/{id}
1. try sqlx sqlite_pool_connect database_url into pool
   otherwise:
   1. respond 503
2. try ensure_schema(pool)
   otherwise:
   1. respond 503
3. sqlx query_scalar "select {note_json} from notes where id = ?" into statement
4. statement bind id into lookup
5. try lookup fetch_optional pool into found
   otherwise:
   1. respond 500
6. guard: found is not empty else respond 404
7. json.parse(found) as Note into parsed
8. match parsed:
   - ok note:
     1. respond 200 note
   - error:
     1. respond 500

## DELETE /notes/{id}
1. try sqlx sqlite_pool_connect database_url into pool
   otherwise:
   1. respond 503
2. try ensure_schema(pool)
   otherwise:
   1. respond 503
3. sqlx query "delete from notes where id = ?" into statement
4. statement bind id into removal
5. try removal execute pool into outcome
   otherwise:
   1. respond 500
6. outcome rows_affected into removed
7. guard: removed > 0 else respond 404
8. respond 204
