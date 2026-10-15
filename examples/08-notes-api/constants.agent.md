# Constant: database_url
The SQLite file that holds the notes. `mode=rwc` creates it on first use.

- value: "sqlite://notes.db?mode=rwc"

# Constant: create_notes_table
The one table of the application.

- value: "create table if not exists notes (id integer primary key, title text not null, body text not null)"

# Constant: note_json
One note as a JSON object, the same shape the API returns.

- value: "json_object('id', id, 'title', title, 'body', body)"
