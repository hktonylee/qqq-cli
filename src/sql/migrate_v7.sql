ALTER TABLE tasks ADD COLUMN priority INTEGER NOT NULL DEFAULT 0
 CHECK(priority BETWEEN -100 AND 100);
DROP INDEX new_queue;
CREATE INDEX new_queue_priority ON tasks(priority DESC,id) WHERE status='new';
PRAGMA user_version=7;
