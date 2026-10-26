DROP TRIGGER task_description_revision;
DROP TRIGGER task_image_insert_revision;
DROP TRIGGER task_image_delete_revision;
DROP TRIGGER task_image_update_revision;

CREATE TABLE tasks_v14 (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 description TEXT NOT NULL CHECK(length(trim(description))>0),
 status TEXT NOT NULL DEFAULT 'new' CHECK(status IN ('new','in_progress','completed','error','blocked')),
 claim_key TEXT,
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 parent_id INTEGER REFERENCES tasks(id),
 harness_name TEXT,
 harness_session TEXT,
 orchestrator_name TEXT,
 orchestrator_session TEXT,
 priority INTEGER NOT NULL DEFAULT 0 CHECK(priority BETWEEN -100 AND 100),
 archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1)),
 content_revision INTEGER NOT NULL DEFAULT 1 CHECK(content_revision>0 AND typeof(content_revision)='integer'),
 tags TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(tags) AND json_type(tags)='array'),
 CHECK ((status='in_progress' AND claim_key IS NOT NULL) OR (status!='in_progress' AND claim_key IS NULL))
);
INSERT INTO tasks_v14 SELECT * FROM tasks;
UPDATE sqlite_sequence SET seq=MAX(seq,COALESCE((SELECT seq FROM sqlite_sequence WHERE name='tasks'),0))
 WHERE name='tasks_v14';
DROP TABLE tasks;
ALTER TABLE tasks_v14 RENAME TO tasks;
CREATE UNIQUE INDEX active_session ON tasks(claim_key) WHERE status='in_progress';
CREATE INDEX new_queue_priority ON tasks(priority DESC,id) WHERE status='new' AND archived=0;

CREATE TABLE events_v14 (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 task_id INTEGER NOT NULL REFERENCES tasks(id),
 session TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('claim','release','complete','error','blocked','archive','unarchive','reopen')),
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
INSERT INTO events_v14 SELECT * FROM events;
UPDATE sqlite_sequence SET seq=MAX(seq,COALESCE((SELECT seq FROM sqlite_sequence WHERE name='events'),0))
 WHERE name='events_v14';
DROP TABLE events;
ALTER TABLE events_v14 RENAME TO events;
CREATE INDEX events_task ON events(task_id,id);

CREATE TRIGGER task_description_revision AFTER UPDATE OF description ON tasks
WHEN OLD.description IS NOT NEW.description
BEGIN
 UPDATE tasks SET content_revision=content_revision+1 WHERE id=NEW.id;
END;
CREATE TRIGGER task_image_insert_revision AFTER INSERT ON images
BEGIN
 UPDATE tasks SET content_revision=content_revision+1 WHERE id=NEW.task_id;
END;
CREATE TRIGGER task_image_delete_revision AFTER DELETE ON images
BEGIN
 UPDATE tasks SET content_revision=content_revision+1 WHERE id=OLD.task_id;
END;
CREATE TRIGGER task_image_update_revision AFTER UPDATE ON images
WHEN OLD.task_id IS NOT NEW.task_id OR OLD.name IS NOT NEW.name
 OR OLD.media_type IS NOT NEW.media_type OR OLD.bytes IS NOT NEW.bytes
BEGIN
 UPDATE tasks SET content_revision=content_revision+1 WHERE id=OLD.task_id;
 UPDATE tasks SET content_revision=content_revision+1 WHERE id=NEW.task_id AND NEW.task_id!=OLD.task_id;
END;
PRAGMA user_version=14;
