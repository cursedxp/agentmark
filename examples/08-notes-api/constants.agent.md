> The SQLite file that holds the notes. `mode=rwc` creates it on first use.
# Constant: database_url
- value: "sqlite://notes.db?mode=rwc"

> The one table of the application.
# Constant: create_notes_table
- value: "create table if not exists notes (id integer primary key, title text not null, body text not null)"

> One note as a JSON object, the same shape the API returns.
# Constant: note_json
- value: "json_object('id', id, 'title', title, 'body', body)"
