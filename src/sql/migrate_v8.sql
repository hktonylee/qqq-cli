ALTER TABLE tasks ADD COLUMN archived INTEGER NOT NULL DEFAULT 0
 CHECK(archived IN (0,1));
DROP INDEX new_queue_priority;
CREATE INDEX new_queue_priority ON tasks(priority DESC,id)
 WHERE status='new' AND archived=0;

CREATE TABLE events_v8 (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 task_id INTEGER NOT NULL REFERENCES tasks(id),
 session TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('claim','release','complete','error','archive','unarchive')),
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
INSERT INTO events_v8(id,task_id,session,action,created_at)
 SELECT id,task_id,session,action,created_at FROM events;
UPDATE sqlite_sequence SET seq=MAX(seq,COALESCE((SELECT seq FROM sqlite_sequence WHERE name='events'),0))
 WHERE name='events_v8';
DROP TABLE events;
ALTER TABLE events_v8 RENAME TO events;
CREATE INDEX events_task ON events(task_id,id);
PRAGMA user_version=8;
