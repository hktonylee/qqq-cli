ALTER TABLE tasks ADD COLUMN content_revision INTEGER NOT NULL DEFAULT 1
 CHECK(content_revision>0 AND typeof(content_revision)='integer');
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
PRAGMA user_version=10;
