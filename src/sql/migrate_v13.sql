CREATE TABLE claim_processes (
 task_id INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
 claim_key TEXT NOT NULL,
 claim_event_id INTEGER NOT NULL,
 process_json TEXT NOT NULL CHECK(json_valid(process_json))
);
PRAGMA user_version=13;
