ALTER TABLE tasks ADD COLUMN parent_id INTEGER REFERENCES tasks(id);
PRAGMA user_version=2;
