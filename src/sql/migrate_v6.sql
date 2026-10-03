CREATE TABLE images_v6 (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 task_id INTEGER NOT NULL REFERENCES tasks(id),
 name TEXT NOT NULL,
 media_type TEXT NOT NULL,
 bytes INTEGER NOT NULL CHECK(bytes>=0)
);
INSERT INTO images_v6(id,task_id,name,media_type,bytes)
 SELECT id,task_id,name,media_type,length(data) FROM images;
UPDATE sqlite_sequence SET seq=MAX(seq,COALESCE((SELECT seq FROM sqlite_sequence WHERE name='images'),0)) WHERE name='images_v6';
DROP TABLE images;
ALTER TABLE images_v6 RENAME TO images;
CREATE INDEX images_task ON images(task_id,id);
PRAGMA user_version=6;
