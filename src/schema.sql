CREATE TABLE tasks (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 title TEXT NOT NULL CHECK(length(trim(title))>0), description TEXT NOT NULL DEFAULT '',
 status TEXT NOT NULL DEFAULT 'new' CHECK(status IN ('new','in_progress','completed')),
 owner_session TEXT,
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 CHECK ((status='in_progress' AND owner_session IS NOT NULL) OR (status!='in_progress' AND owner_session IS NULL))
);
CREATE UNIQUE INDEX active_session ON tasks(owner_session) WHERE status='in_progress';
CREATE INDEX new_queue ON tasks(id) WHERE status='new';
CREATE TABLE messages (
 id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL REFERENCES tasks(id), body TEXT NOT NULL, session TEXT,
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX messages_task ON messages(task_id,id);
CREATE TABLE images (id INTEGER PRIMARY KEY AUTOINCREMENT,task_id INTEGER NOT NULL REFERENCES tasks(id),name TEXT NOT NULL,media_type TEXT NOT NULL,data BLOB NOT NULL);
CREATE INDEX images_task ON images(task_id,id);
CREATE TABLE events (id INTEGER PRIMARY KEY AUTOINCREMENT,task_id INTEGER NOT NULL REFERENCES tasks(id),session TEXT NOT NULL,action TEXT NOT NULL CHECK(action IN ('claim','release','complete')),created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));
CREATE INDEX events_task ON events(task_id,id);
CREATE TABLE herdr_links (task_id INTEGER PRIMARY KEY REFERENCES tasks(id),link_json TEXT NOT NULL);
PRAGMA user_version=1;
