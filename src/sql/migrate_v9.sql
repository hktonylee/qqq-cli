CREATE TABLE events_v9 (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 task_id INTEGER NOT NULL REFERENCES tasks(id),
 session TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('claim','release','complete','error','archive','unarchive','reopen')),
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
INSERT INTO events_v9(id,task_id,session,action,created_at)
 SELECT id,task_id,session,action,created_at FROM events;
UPDATE sqlite_sequence SET seq=MAX(seq,COALESCE((SELECT seq FROM sqlite_sequence WHERE name='events'),0))
 WHERE name='events_v9';
DROP TABLE events;
ALTER TABLE events_v9 RENAME TO events;
CREATE INDEX events_task ON events(task_id,id);
PRAGMA user_version=9;
