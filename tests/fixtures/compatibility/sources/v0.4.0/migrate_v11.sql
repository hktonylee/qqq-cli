CREATE TABLE task_dependencies (
 task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
 prerequisite_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
 PRIMARY KEY(task_id,prerequisite_id),
 CHECK(task_id != prerequisite_id)
);
CREATE INDEX task_dependencies_prerequisite ON task_dependencies(prerequisite_id,task_id);
PRAGMA user_version=11;
