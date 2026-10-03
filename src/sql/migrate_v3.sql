ALTER TABLE tasks RENAME COLUMN owner_session TO assignee;
PRAGMA user_version=3;
