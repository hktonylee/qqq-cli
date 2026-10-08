#[allow(dead_code)]
#[path = "../src/archive.rs"]
mod archive;
#[allow(dead_code)]
#[path = "../src/db.rs"]
mod db;
#[allow(dead_code)]
#[path = "../src/dependencies.rs"]
mod dependencies;
#[allow(dead_code)]
#[path = "../src/tui/draft.rs"]
pub mod draft;
#[allow(dead_code)]
#[path = "../src/errors.rs"]
mod errors;
#[allow(dead_code)]
#[path = "../src/herdr.rs"]
mod herdr;
#[allow(dead_code)]
#[path = "../src/identity.rs"]
mod identity;
#[allow(dead_code)]
#[path = "../src/images.rs"]
mod images;
#[allow(dead_code)]
#[path = "../src/preflight.rs"]
mod preflight;
#[allow(dead_code)]
#[path = "../src/process.rs"]
mod process;
#[allow(dead_code)]
#[path = "../src/snapshot/mod.rs"]
mod snapshot;
#[allow(dead_code)]
#[path = "../src/sql_filter/mod.rs"]
mod sql_filter;
#[allow(dead_code)]
#[path = "../src/tags.rs"]
mod tags;
mod tui {
    pub use crate::draft;
}

use db::Db;
use draft::{Composition, Draft};
use images::{ImageInput, ImageStore};
use std::process::{Command, Stdio};

fn database() -> (Db, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("qqq.db")).unwrap();
    conn.execute_batch(include_str!("../src/sql/schema.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/sql/migrate_v2.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/sql/migrate_v3.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/sql/migrate_v4.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/sql/migrate_v5.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/sql/migrate_v6.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/sql/migrate_v7.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/sql/migrate_v8.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/sql/migrate_v9.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/sql/migrate_v10.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/sql/migrate_v11.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/sql/migrate_v12.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/sql/migrate_v13.sql"))
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
fn force_completion_preserves_content_and_history_for_unfinished_tasks() {
    for state in ["new", "in_progress", "error"] {
        let (mut db, dir) = database();
        let parent = db.add("Parent", None, &[]).unwrap();
        db.next("parent", None).unwrap();
        db.complete(parent.id, "parent", None).unwrap();
        let prerequisite = db.add("Prerequisite", None, &[]).unwrap();
        db.next("prerequisite", None).unwrap();
        db.complete(prerequisite.id, "prerequisite", None).unwrap();
        let task = db
            .add_with_dependencies(
                "Original content",
                Some(parent.id),
                &composition().images,
                7,
                &[prerequisite.id],
            )
            .unwrap();
        db.message(task.id, "Keep note", Some("foreign")).unwrap();
        if state != "new" {
            db.next("foreign", None).unwrap();
        }
        let link = herdr::Link {
            server: Some("test".into()),
            identity: herdr::AgentSession {
                agent: "codex".into(),
                kind: "id".into(),
                value: "foreign".into(),
            },
            pane: herdr::Pane {
                pane_id: "w1:p1".into(),
                workspace_id: "w1".into(),
                tab_id: "w1:t1".into(),
                cwd: None,
                foreground_cwd: None,
                agent_session: None,
                terminal_id: None,
                agent: None,
            },
        };
        db.set_link_with_identity(task.id, &link, &identity::Identity::default(), None)
            .unwrap();
        if state == "error" {
            db.edit_with_priority(
                task.id,
                None,
                Some(db::EditTransition::Error {
                    session: "foreign",
                    reason: "Keep failure note",
                    harness_name: None,
                }),
                &[],
                None,
                None,
            )
            .unwrap();
        }
        if state != "in_progress" {
            db.set_archived(task.id, true, "manual").unwrap();
        }
        let before = db.show(task.id).unwrap();
        let image = dir.path().join(format!("images/{}/1.png", task.id));
        let image_bytes = std::fs::read(&image).unwrap();
        let completed = db.force_complete(task.id, "operator").unwrap();
        let after = db.show(task.id).unwrap();
        assert_eq!(completed.status, "completed");
        for field in [
            "description",
            "content_revision",
            "priority",
            "archived",
            "parent_id",
            "prerequisites",
            "created_at",
        ] {
            assert_eq!(
                after["task"][field], before["task"][field],
                "{state}: {field}"
            );
        }
        for field in ["messages", "images", "herdr"] {
            assert_eq!(after[field], before[field], "{state}: {field}");
        }
        for field in [
            "harness_name",
            "harness_session",
            "orchestrator_name",
            "orchestrator_session",
        ] {
            assert_eq!(
                after["task"][field], before["task"][field],
                "{state}: {field}"
            );
        }
        let claim: Option<String> = db
            .conn
            .query_row("SELECT claim_key FROM tasks WHERE id=?", [task.id], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(claim.is_none());
        let mut events = after["events"].as_array().unwrap().clone();
        let forced = events.pop().unwrap();
        assert_eq!(forced["action"], "complete");
        assert_eq!(forced["session"], "operator");
        assert_eq!(events, *before["events"].as_array().unwrap());
        assert_eq!(std::fs::read(image).unwrap(), image_bytes);
    }
}

#[test]
fn force_completion_rejects_missing_completed_and_blank_actor_without_writes() {
    let (mut db, _dir) = database();
    let task = db.add("Original", None, &[]).unwrap();
    let before = db.show(task.id).unwrap();
    assert!(db.force_complete(task.id, " ").is_err());
    assert_eq!(db.show(task.id).unwrap(), before);
    let missing = db.force_complete(999, "operator").err().unwrap();
    assert!(matches!(
        missing.downcast_ref::<errors::Info>().unwrap().code,
        errors::Code::TaskNotFound
    ));
    db.force_complete(task.id, "operator").unwrap();
    let completed = db.show(task.id).unwrap();
    let error = db.force_complete(task.id, "operator").err().unwrap();
    assert!(matches!(
        error.downcast_ref::<errors::Info>().unwrap().code,
        errors::Code::InvalidTransition
    ));
    assert_eq!(db.show(task.id).unwrap(), completed);
}

#[test]
fn force_completion_rolls_back_status_and_ownership_if_audit_insert_fails() {
    let (mut db, _dir) = database();
    let task = db.add("Original", None, &[]).unwrap();
    db.next("foreign", None).unwrap();
    let before = db.show(task.id).unwrap();
    db.conn.execute_batch("CREATE TRIGGER reject_force_complete BEFORE INSERT ON events WHEN NEW.action='complete' BEGIN SELECT RAISE(ABORT,'audit rejected'); END;").unwrap();
    assert!(db.force_complete(task.id, "operator").is_err());
    assert_eq!(db.show(task.id).unwrap(), before);
    assert_eq!(
        db.owned_with_name("foreign", None).unwrap().unwrap().id,
        task.id
    );
}

#[test]
fn force_reopen_preserves_content_and_history_and_clears_assignment() {
    for state in ["completed", "in_progress", "error"] {
        let (mut db, dir) = database();
        let parent = db.add("Parent", None, &[]).unwrap();
        db.force_complete(parent.id, "operator").unwrap();
        let prerequisite = db.add("Prerequisite", None, &[]).unwrap();
        db.force_complete(prerequisite.id, "operator").unwrap();
        let task = db
            .add_with_dependencies(
                "Original content",
                Some(parent.id),
                &composition().images,
                7,
                &[prerequisite.id],
            )
            .unwrap();
        db.next("foreign", None).unwrap();
        db.message(task.id, "Keep note", Some("foreign")).unwrap();
        db.conn
            .execute(
                "UPDATE tasks SET harness_name='codex',harness_session='foreign',
            orchestrator_name='herdr',orchestrator_session='saved' WHERE id=?",
                [task.id],
            )
            .unwrap();
        if state == "completed" {
            db.complete(task.id, "foreign", None).unwrap();
        } else if state == "error" {
            db.edit_with_priority(
                task.id,
                None,
                Some(db::EditTransition::Error {
                    session: "foreign",
                    reason: "Keep failure note",
                    harness_name: None,
                }),
                &[],
                None,
                None,
            )
            .unwrap();
        }
        let before = db.show(task.id).unwrap();
        if state != "completed" {
            assert!(db.reopen(task.id, "operator").is_err());
            assert_eq!(db.show(task.id).unwrap(), before);
        }
        let image = dir.path().join(format!("images/{}/1.png", task.id));
        let image_bytes = std::fs::read(&image).unwrap();
        assert_eq!(db.force_reopen(task.id, "operator").unwrap().status, "new");
        let after = db.show(task.id).unwrap();
        for field in [
            "description",
            "content_revision",
            "priority",
            "archived",
            "parent_id",
            "prerequisites",
            "tags",
            "created_at",
        ] {
            assert_eq!(
                after["task"][field], before["task"][field],
                "{state}: {field}"
            );
        }
        for field in ["messages", "images", "herdr"] {
            assert_eq!(after[field], before[field], "{state}: {field}");
        }
        for field in [
            "harness_name",
            "harness_session",
            "orchestrator_name",
            "orchestrator_session",
        ] {
            assert!(after["task"][field].is_null(), "{state}: {field}");
        }
        assert!(db.owned_with_name("foreign", None).unwrap().is_none());
        let mut events = after["events"].as_array().unwrap().clone();
        let forced = events.pop().unwrap();
        assert_eq!(forced["action"], "reopen");
        assert_eq!(forced["session"], "operator");
        assert_eq!(events, *before["events"].as_array().unwrap());
        assert_eq!(std::fs::read(image).unwrap(), image_bytes);
    }
}

#[test]
fn force_reopen_keeps_status_archive_and_dependency_guards() {
    let (mut db, _dir) = database();
    let task = db.add("Fresh", None, &[]).unwrap();
    let before = db.show(task.id).unwrap();
    assert!(db.force_reopen(task.id, "operator").is_err());
    assert!(db.force_reopen(task.id, " ").is_err());
    assert_eq!(db.show(task.id).unwrap(), before);
    let missing = db.force_reopen(999, "operator").err().unwrap();
    assert!(matches!(
        missing.downcast_ref::<errors::Info>().unwrap().code,
        errors::Code::TaskNotFound
    ));
    for guard in ["self", "parent", "prerequisite"] {
        let (mut db, _dir) = database();
        let dependency = db.add("Dependency", None, &[]).unwrap();
        let dependencies = [dependency.id];
        let task = db
            .add_with_dependencies(
                "Finished",
                (guard == "parent").then_some(dependency.id),
                &[],
                0,
                if guard == "prerequisite" {
                    &dependencies
                } else {
                    &[]
                },
            )
            .unwrap();
        db.force_complete(task.id, "operator").unwrap();
        db.set_archived(
            if guard == "self" {
                task.id
            } else {
                dependency.id
            },
            true,
            "operator",
        )
        .unwrap();
        let before = db.show(task.id).unwrap();
        assert!(db.force_reopen(task.id, "operator").is_err(), "{guard}");
        assert_eq!(db.show(task.id).unwrap(), before, "{guard}");
    }
}

#[test]
fn force_reopen_rolls_back_assignment_when_audit_fails() {
    let (mut db, _dir) = database();
    let task = db.add("Original", None, &[]).unwrap();
    db.next("foreign", None).unwrap();
    let before = db.show(task.id).unwrap();
    db.conn
        .execute_batch(
            "CREATE TRIGGER reject_force_reopen BEFORE INSERT ON events
        WHEN NEW.action='reopen' BEGIN SELECT RAISE(ABORT,'audit rejected'); END;",
        )
        .unwrap();
    assert!(db.force_reopen(task.id, "operator").is_err());
    assert_eq!(db.show(task.id).unwrap(), before);
    assert_eq!(
        db.owned_with_name("foreign", None).unwrap().unwrap().id,
        task.id
    );
}

#[test]
fn loaded_compositions_conflict_without_losing_local_paste_or_image_payloads() {
    let (mut db, dir) = database();
    let task = db.add("Original", None, &[]).unwrap();
    let first = db.content_snapshot(task.id).unwrap();
    let second = db.content_snapshot(task.id).unwrap();
    let mut local = Draft::from_saved(&first.task.description, task.id, &first.references).unwrap();
    local.paste(&"Full\nmultiline\npaste\n".repeat(70));
    local
        .image(ImageInput {
            name: "local.png".into(),
            data: b"\x89PNG\r\n\x1a\nlocal".to_vec(),
        })
        .unwrap();
    let composition = local.finish().unwrap();
    let external = Draft::new("Newer content").finish().unwrap();
    db.edit_composition_guarded(
        task.id,
        &external,
        None,
        None,
        Some(second.task.content_revision),
    )
    .unwrap();
    let before = db.show(task.id).unwrap();
    let error = db
        .edit_composition_guarded(
            task.id,
            &composition,
            None,
            None,
            Some(first.task.content_revision),
        )
        .err()
        .unwrap();
    let conflict = error.downcast_ref::<db::ContentConflict>().unwrap();
    assert_eq!(
        conflict.current.as_ref().unwrap().description,
        "Newer content"
    );
    assert_eq!(db.show(task.id).unwrap(), before);
    assert!(!dir.path().join("images/1").exists());
    assert!(composition.description.contains("Full\nmultiline\npaste"));
    assert_eq!(composition.images[0].data, b"\x89PNG\r\n\x1a\nlocal");
    let saved = db
        .edit_composition_guarded(
            task.id,
            &composition,
            None,
            None,
            Some(conflict.current.as_ref().unwrap().revision),
        )
        .unwrap();
    assert!(saved.description.contains("pasteboard"));
    assert!(saved.description.contains(".qqq/images/1/1.png"));
    assert_eq!(
        std::fs::read(dir.path().join("images/1/1.png")).unwrap(),
        composition.images[0].data
    );
}

#[test]
fn attachment_changes_invalidate_snapshots_even_when_text_is_unchanged() {
    let (mut db, _dir) = database();
    let task = db.add("Same text", None, &[]).unwrap();
    let original = db.content_snapshot(task.id).unwrap();
    let attached = db
        .edit(task.id, None, None, &composition().images, None)
        .unwrap();
    assert!(attached.content_revision > original.task.content_revision);
    let snapshot = db.content_snapshot(task.id).unwrap();
    assert_eq!(snapshot.references.len(), 1);
    assert_eq!(snapshot.task.description, original.task.description);
    assert!(
        db.edit_composition_guarded(
            task.id,
            &Draft::new("Local").finish().unwrap(),
            None,
            None,
            Some(original.task.content_revision)
        )
        .is_err()
    );
    db.conn
        .execute(
            "UPDATE images SET name='renamed.png' WHERE task_id=?",
            [task.id],
        )
        .unwrap();
    let renamed = db.task(task.id).unwrap();
    assert!(renamed.content_revision > attached.content_revision);
    db.conn
        .execute("DELETE FROM images WHERE task_id=?", [task.id])
        .unwrap();
    assert!(db.task(task.id).unwrap().content_revision > renamed.content_revision);
}

#[test]
fn removed_task_rejects_guarded_save_before_creating_images() {
    let (mut db, dir) = database();
    let task = db.add("Original", None, &[]).unwrap();
    db.conn
        .execute("DELETE FROM tasks WHERE id=?", [task.id])
        .unwrap();
    let local = composition();
    let error = db
        .edit_composition_guarded(task.id, &local, None, None, Some(task.content_revision))
        .err()
        .unwrap();
    let conflict = error.downcast_ref::<db::ContentConflict>().unwrap();
    assert!(conflict.current.is_none());
    assert!(db.list(None).unwrap().is_empty());
    assert!(!dir.path().join("images/1").exists());
    assert_eq!(local.images.len(), 1);
}

#[test]
fn content_snapshots_never_mix_text_and_attachment_revisions() {
    let (mut db, dir) = database();
    db.conn
        .busy_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    db.add("0", None, &[]).unwrap();
    let path = dir.path().join("qqq.db");
    let writer = std::thread::spawn(move || {
        let mut conn = rusqlite::Connection::open(path).unwrap();
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        for index in 1..=80 {
            let tx = conn
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            tx.execute(
                "UPDATE tasks SET description=? WHERE id=1",
                [index.to_string()],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO images(task_id,name,media_type,bytes) VALUES (1,?,'image/png',8)",
                [index.to_string()],
            )
            .unwrap();
            tx.commit().unwrap();
        }
    });
    for _ in 0..200 {
        let snapshot = db.content_snapshot(1).unwrap();
        let index: usize = snapshot.task.description.parse().unwrap();
        assert_eq!(snapshot.references.len(), index);
        assert_eq!(snapshot.task.content_revision, 1 + 2 * index as i64);
        if let Some(reference) = snapshot.references.last() {
            assert_eq!(reference.name, snapshot.task.description);
        }
    }
    writer.join().unwrap();
}

#[test]
fn task_messages_preserve_body_author_order_and_show_contract() {
    let (mut db, dir) = database();
    let task = db.add("Selected", None, &[]).unwrap();
    assert!(db.task_messages(task.id).unwrap().is_empty());
    db.message(task.id, "First\nDetails", Some("worker"))
        .unwrap();
    db.message(task.id, "Second", None).unwrap();
    let messages = db.task_messages(task.id).unwrap();
    assert_eq!(messages.len(), 2);
    assert!(messages[0].id < messages[1].id);
    assert_eq!(messages[0].body, "First\nDetails");
    assert_eq!(messages[0].session.as_deref(), Some("worker"));
    assert!(messages[1].session.is_none());
    assert!(messages[0].created_at.ends_with('Z'));
    assert_eq!(
        db.show(task.id).unwrap()["messages"],
        serde_json::to_value(messages).unwrap()
    );
    assert_eq!(db.show_task(&task).unwrap(), db.show(task.id).unwrap());
    // Simulate deletion after TUI listed task, before reading its detail sections.
    let task = db.list(None).unwrap().pop().unwrap();
    let version = db.data_version().unwrap();
    let external = rusqlite::Connection::open(dir.path().join("qqq.db")).unwrap();
    external
        .execute("DELETE FROM messages WHERE task_id=?", [task.id])
        .unwrap();
    external
        .execute("DELETE FROM tasks WHERE id=?", [task.id])
        .unwrap();
    assert!(db.show(task.id).is_err());
    let snapshot = db.show_task(&task).unwrap();
    assert_eq!(snapshot["task"]["id"], task.id);
    assert_eq!(snapshot["task"]["description"], "Selected");
    for section in ["messages", "images", "events"] {
        assert_eq!(snapshot[section], serde_json::json!([]));
    }
    assert!(snapshot["herdr"].is_null());
    assert_ne!(db.data_version().unwrap(), version);
    assert!(db.list(None).unwrap().is_empty());
}

#[test]
fn task_navigation_skips_archived_unless_included() {
    let (mut db, _dir) = database();
    db.add("First", None, &[]).unwrap();
    db.add("Hidden", None, &[]).unwrap();
    db.add("Third", None, &[]).unwrap();
    db.set_archived(2, true, "cli").unwrap();
    assert_eq!(db.adjacent_task(None, true, false).unwrap().unwrap().0, 3);
    assert_eq!(
        db.adjacent_task(Some(3), true, false).unwrap().unwrap().0,
        1
    );
    assert_eq!(
        db.adjacent_task(Some(1), false, false).unwrap().unwrap().0,
        3
    );
    assert_eq!(db.adjacent_task(Some(3), true, true).unwrap().unwrap().0, 2);
}

#[test]
fn legacy_open_keeps_foreign_keys_enabled() {
    if std::env::var_os("QQQ_TEST_LEGACY_FK_CHILD").is_none() {
        return;
    }
    let (db, _) = Db::open(false).unwrap();
    let enabled: i64 = db
        .conn
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .unwrap();
    assert_eq!(enabled, 1);
}

#[test]
fn concurrent_legacy_opens_keep_foreign_keys_enabled() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".qqq")).unwrap();
    let conn = rusqlite::Connection::open(dir.path().join(".qqq/qqq.db")).unwrap();
    for migration in [
        include_str!("../src/sql/schema.sql"),
        include_str!("../src/sql/migrate_v2.sql"),
        include_str!("../src/sql/migrate_v3.sql"),
        include_str!("../src/sql/migrate_v4.sql"),
        include_str!("../src/sql/migrate_v5.sql"),
    ] {
        conn.execute_batch(migration).unwrap();
    }
    conn.execute("INSERT INTO tasks(description) VALUES ('Legacy')", [])
        .unwrap();
    conn.execute(
        "INSERT INTO images(task_id,name,media_type,data) VALUES (1,'legacy.png','image/png',?1)",
        [vec![1_u8; 4 * 1024 * 1024]],
    )
    .unwrap();
    drop(conn);

    let children: Vec<_> = (0..6)
        .map(|_| {
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "legacy_open_keeps_foreign_keys_enabled"])
                .env("QQQ_TEST_LEGACY_FK_CHILD", "1")
                .current_dir(dir.path())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
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
