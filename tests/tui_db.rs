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
#[path = "../src/identity.rs"]
mod identity;
#[allow(dead_code)]
#[path = "../src/images.rs"]
mod images;
mod tui {
    pub use crate::draft;
}

use db::Db;
use draft::{Composition, Draft};
use images::{ImageInput, ImageStore};

fn database() -> (Db, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("qqq.db")).unwrap();
    conn.execute_batch(include_str!("../src/schema.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/migrate_v2.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/migrate_v3.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/migrate_v4.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/migrate_v5.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/migrate_v6.sql"))
        .unwrap();
    (
        Db {
            conn,
            image_store: ImageStore::new(dir.path().join("images")),
        },
        dir,
    )
}
fn composition() -> Composition {
    Composition {
        description: "Details".into(),
        images: vec![ImageInput {
            name: "ok.png".into(),
            data: b"\x89PNG\r\n\x1a\n".to_vec(),
        }],
        image_spans: Vec::new(),
    }
}
#[test]
fn pasted_images_get_stored_markdown_paths_after_image_ids_are_allocated() {
    let (mut db, dir) = database();
    let mut draft = Draft::new("See ");
    draft
        .image(ImageInput {
            name: "a]b.png".into(),
            data: b"\x89PNG\r\n\x1a\n".to_vec(),
        })
        .unwrap();
    draft
        .image(ImageInput {
            name: "second.png".into(),
            data: b"\x89PNG\r\n\x1a\n".to_vec(),
        })
        .unwrap();
    let mut composition = draft.finish().unwrap();
    composition.images.push(ImageInput {
        name: "flag.png".into(),
        data: b"\x89PNG\r\n\x1a\n".to_vec(),
    });
    let task = db.save_composition(None, None, &composition).unwrap();
    assert_eq!(
        task.description,
        "See ![a\\]b.png](.qqq/images/1/1.png)![second.png](.qqq/images/1/2.png)"
    );
    assert_eq!(
        db.show(task.id).unwrap()["images"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let references = db.image_references(task.id).unwrap();
    assert_eq!(
        (references[0].id, references[0].name.as_str()),
        (1, "a]b.png")
    );
    assert_eq!(
        (references[1].id, references[1].name.as_str()),
        (2, "second.png")
    );
    assert_eq!(
        (references[2].id, references[2].name.as_str()),
        (3, "flag.png")
    );
    assert!(dir.path().join("images/1/1.png").exists());
    assert!(dir.path().join("images/1/2.png").exists());
    assert!(dir.path().join("images/1/3.png").exists());

    let mut edit = Draft::new(&task.description);
    edit.image(ImageInput {
        name: "third.png".into(),
        data: b"\x89PNG\r\n\x1a\n".to_vec(),
    })
    .unwrap();
    let updated = db
        .save_composition(Some(task.id), None, &edit.finish().unwrap())
        .unwrap();
    assert_eq!(
        updated.description,
        "See ![a\\]b.png](.qqq/images/1/1.png)![second.png](.qqq/images/1/2.png)![third.png](.qqq/images/1/4.png)"
    );
}
#[test]
fn failed_pasted_image_save_rolls_back_link_and_file() {
    let (mut db, dir) = database();
    let task = db.add("Original", None, &[]).unwrap();
    db.conn.execute_batch("CREATE TRIGGER reject_image BEFORE INSERT ON images WHEN NEW.name='fail.png' BEGIN SELECT RAISE(ABORT,'image rejected'); END;").unwrap();
    let mut draft = Draft::new("Changed ");
    for name in ["ok.png", "fail.png"] {
        draft
            .image(ImageInput {
                name: name.into(),
                data: b"\x89PNG\r\n\x1a\n".to_vec(),
            })
            .unwrap();
    }
    assert!(
        db.save_composition(Some(task.id), None, &draft.finish().unwrap())
            .is_err()
    );
    assert_eq!(db.task(task.id).unwrap().description, "Original");
    assert!(db.image_references(task.id).unwrap().is_empty());
    assert!(!dir.path().join("images/1/1.png").exists());
}
#[test]
fn composition_saves_task_and_image_bytes_together() {
    let (mut db, dir) = database();
    let task = db.save_composition(None, None, &composition()).unwrap();
    assert_eq!(task.description, "Details");
    assert_eq!(db.show(task.id).unwrap()["images"][0]["name"], "ok.png");
    let bytes = std::fs::read(dir.path().join("images/1/1.png")).unwrap();
    assert_eq!(bytes, b"\x89PNG\r\n\x1a\n");
}
#[test]
fn composition_saves_pasteboard_block_and_image_link_together() {
    let (mut db, _dir) = database();
    let payload = format!("```\n{}", "x".repeat(1001));
    let mut draft = Draft::new("Before ");
    draft.paste(&payload);
    draft
        .image(ImageInput {
            name: "after.png".into(),
            data: b"\x89PNG\r\n\x1a\n".to_vec(),
        })
        .unwrap();
    let task = db
        .save_composition(None, None, &draft.finish().unwrap())
        .unwrap();
    assert_eq!(
        task.description,
        format!("Before \n````pasteboard\n{payload}\n````\n![after.png](.qqq/images/1/1.png)")
    );
    let loaded = Draft::from_saved(
        &task.description,
        task.id,
        &db.image_references(task.id).unwrap(),
    )
    .unwrap();
    assert!(
        loaded
            .fragments()
            .iter()
            .any(|part| part.starts_with("[Pasted Content "))
    );
    assert!(
        loaded
            .fragments()
            .iter()
            .any(|part| part == "[Image #1: after.png]")
    );
    assert_eq!(loaded.finish().unwrap().description, task.description);
}
#[test]
fn failed_image_insert_rolls_back_new_and_edited_drafts() {
    let (mut db, _dir) = database();
    db.conn.execute_batch("CREATE TRIGGER reject_image BEFORE INSERT ON images WHEN NEW.name='fail.png' BEGIN SELECT RAISE(ABORT,'image rejected'); END;").unwrap();
    let mut draft = composition();
    draft.images.push(ImageInput {
        name: "fail.png".into(),
        data: b"\x89PNG\r\n\x1a\n".to_vec(),
    });
    assert!(db.save_composition(None, None, &draft).is_err());
    assert!(db.list(None).unwrap().is_empty());
    let existing = db.add("Original\n\nOriginal details", None, &[]).unwrap();
    let before = db.show(existing.id).unwrap();
    assert!(
        db.save_composition(Some(existing.id), None, &draft)
            .is_err()
    );
    assert_eq!(db.show(existing.id).unwrap(), before);
}
#[test]
fn draft_edit_preserves_ownership_dependencies_and_existing_attachments() {
    let (mut db, _dir) = database();
    let parent = db.add("Parent", None, &[]).unwrap();
    db.next("parent", None).unwrap();
    db.complete(parent.id, "parent", None).unwrap();
    let task = db
        .save_composition(None, Some(parent.id), &composition())
        .unwrap();
    db.next("worker", None).unwrap();
    db.message(task.id, "Note", Some("worker")).unwrap();
    let updated = db
        .save_composition(Some(task.id), None, &composition())
        .unwrap();
    assert_eq!(updated.status, "in_progress");
    assert_eq!(updated.identity.harness_session.as_deref(), Some("worker"));
    assert_eq!(updated.parent_id, Some(parent.id));
    let shown = db.show(task.id).unwrap();
    assert_eq!(shown["images"].as_array().unwrap().len(), 2);
    assert_eq!(shown["messages"][0]["body"], "Note");
}
