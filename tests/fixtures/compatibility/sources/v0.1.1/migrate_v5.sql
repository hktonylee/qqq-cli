ALTER TABLE tasks RENAME COLUMN assignee TO claim_key;
ALTER TABLE tasks ADD COLUMN harness_name TEXT;
ALTER TABLE tasks ADD COLUMN harness_session TEXT;
ALTER TABLE tasks ADD COLUMN orchestrator_name TEXT;
ALTER TABLE tasks ADD COLUMN orchestrator_session TEXT;
UPDATE tasks SET
 harness_name=(SELECT json_extract(link_json,'$.identity.agent') FROM herdr_links WHERE task_id=tasks.id),
 harness_session=COALESCE((SELECT json_extract(link_json,'$.identity.value') FROM herdr_links WHERE task_id=tasks.id),claim_key),
 orchestrator_name=CASE WHEN EXISTS(SELECT 1 FROM herdr_links WHERE task_id=tasks.id) THEN 'herdr' END,
 orchestrator_session=(SELECT json_extract(link_json,'$.server') FROM herdr_links WHERE task_id=tasks.id)
 WHERE status='in_progress';
PRAGMA user_version=5;
