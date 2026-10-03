CREATE TABLE tasks_v4 (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 description TEXT NOT NULL CHECK(length(trim(description))>0),
 status TEXT NOT NULL DEFAULT 'new' CHECK(status IN ('new','in_progress','completed','error')),
 assignee TEXT,
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 parent_id INTEGER REFERENCES tasks(id),
 CHECK ((status='in_progress' AND assignee IS NOT NULL) OR (status!='in_progress' AND assignee IS NULL))
);
INSERT INTO tasks_v4(id,description,status,assignee,created_at,updated_at,parent_id)
 SELECT id,description,status,assignee,created_at,updated_at,parent_id FROM tasks;
UPDATE sqlite_sequence SET seq=MAX(seq,COALESCE((SELECT seq FROM sqlite_sequence WHERE name='tasks'),0)) WHERE name='tasks_v4';
DROP TABLE tasks;
ALTER TABLE tasks_v4 RENAME TO tasks;
CREATE UNIQUE INDEX active_session ON tasks(assignee) WHERE status='in_progress';
CREATE INDEX new_queue ON tasks(id) WHERE status='new';

CREATE TABLE events_v4 (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 task_id INTEGER NOT NULL REFERENCES tasks(id), session TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('claim','release','complete','error')),
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
INSERT INTO events_v4(id,task_id,session,action,created_at)
 SELECT id,task_id,session,action,created_at FROM events;
UPDATE sqlite_sequence SET seq=MAX(seq,COALESCE((SELECT seq FROM sqlite_sequence WHERE name='events'),0)) WHERE name='events_v4';
DROP TABLE events;
ALTER TABLE events_v4 RENAME TO events;
CREATE INDEX events_task ON events(task_id,id);
PRAGMA user_version=4;
