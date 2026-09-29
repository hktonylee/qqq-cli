#[allow(dead_code)]
#[path = "../src/db.rs"]
mod db;
#[allow(dead_code)]
#[path = "../src/tui/draft.rs"]
pub mod draft;
#[allow(dead_code)]
#[path = "../src/herdr.rs"]
mod herdr;
#[allow(dead_code)]
#[path = "../src/images.rs"]
mod images;
mod tui {
    pub use crate::draft;
}

use db::Db;
use draft::Composition;
use images::ImageInput;

fn database() -> Db {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../src/schema.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/migrate_v2.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/migrate_v3.sql"))
        .unwrap();
    Db { conn }
}
fn composition() -> Composition {
    Composition {
        title: "Title".into(),
        description: "Details".into(),
        images: vec![ImageInput {
            name: "ok.png".into(),
            data: b"\x89PNG\r\n\x1a\n".to_vec(),
        }],
    }
}
#[test]
fn composition_saves_task_and_image_bytes_together() {
    let mut db = database();
    let task = db.save_composition(None, None, &composition()).unwrap();
    assert_eq!(task.title, "Title");
    assert_eq!(db.show(task.id).unwrap()["images"][0]["name"], "ok.png");
    let bytes: Vec<u8> = db
        .conn
        .query_row("SELECT data FROM images", [], |r| r.get(0))
        .unwrap();
    assert_eq!(bytes, b"\x89PNG\r\n\x1a\n");
}
#[test]
fn failed_image_insert_rolls_back_new_and_edited_drafts() {
    let mut db = database();
    db.conn.execute_batch("CREATE TRIGGER reject_image BEFORE INSERT ON images WHEN NEW.name='fail.png' BEGIN SELECT RAISE(ABORT,'image rejected'); END;").unwrap();
    let mut draft = composition();
    draft.images.push(ImageInput {
        name: "fail.png".into(),
        data: b"\x89PNG\r\n\x1a\n".to_vec(),
    });
    assert!(db.save_composition(None, None, &draft).is_err());
    assert!(db.list(None).unwrap().is_empty());
    let existing = db.add("Original", "Original details", None).unwrap();
    let before = db.show(existing.id).unwrap();
    assert!(
        db.save_composition(Some(existing.id), None, &draft)
            .is_err()
    );
    assert_eq!(db.show(existing.id).unwrap(), before);
}
#[test]
fn draft_edit_preserves_ownership_dependencies_and_existing_attachments() {
    let mut db = database();
    let parent = db.add("Parent", "", None).unwrap();
    db.next("parent", None).unwrap();
    db.complete(parent.id, "parent").unwrap();
    let task = db
        .save_composition(None, Some(parent.id), &composition())
        .unwrap();
    db.next("worker", None).unwrap();
    db.message(task.id, "Note", Some("worker")).unwrap();
    let updated = db
        .save_composition(Some(task.id), None, &composition())
        .unwrap();
    assert_eq!(updated.status, "in_progress");
    assert_eq!(updated.assignee.as_deref(), Some("worker"));
    assert_eq!(updated.parent_id, Some(parent.id));
    let shown = db.show(task.id).unwrap();
    assert_eq!(shown["images"].as_array().unwrap().len(), 2);
    assert_eq!(shown["messages"][0]["body"], "Note");
}
