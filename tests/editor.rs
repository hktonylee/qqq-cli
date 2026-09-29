#![cfg(unix)]

use serde_json::Value;
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .arg("--json")
        .current_dir(dir)
        .env("HOME", dir)
        .env_remove("EDITOR");
    command
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    assert!(
        command(dir.path())
            .arg("init")
            .output()
            .unwrap()
            .status
            .success()
    );
    dir
}

fn editor(dir: &Path, body: &str) -> String {
    let path = dir.join("test editor.sh");
    fs::write(&path, format!("set -eu\n{body}\n")).unwrap();
    format!("sh '{}' --wait", path.display())
}

#[test]
fn add_uses_editor_with_arguments_and_keeps_stdout_json() {
    let dir = project();
    let editor = editor(
        dir.path(),
        r#"
test "$1" = --wait
printf '%s' "$2" > edited-path
printf '  Edited title  \r\n\r\nDetails\r\nSecond line\r\n' > "$2"
echo 'editor output'
"#,
    );
    let output = command(dir.path())
        .arg("add")
        .env("EDITOR", editor)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let task: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(task["title"], "Edited title");
    assert_eq!(task["description"], "Details\nSecond line");
    assert!(String::from_utf8_lossy(&output.stderr).contains("editor output"));
    let path = fs::read_to_string(dir.path().join("edited-path")).unwrap();
    assert!(!Path::new(&path).exists());
    let saved = command(dir.path()).args(["show", "1"]).output().unwrap();
    let saved: Value = serde_json::from_slice(&saved.stdout).unwrap();
    assert_eq!(saved["task"], task);
}

#[test]
fn edit_flag_prefills_content_and_supports_atomic_editor_saves() {
    let dir = project();
    let editor = editor(
        dir.path(),
        r#"
cp "$2" initial-content
printf 'New title\n\nNew details\n' > "$2.new"
mv "$2.new" "$2"
"#,
    );
    let output = command(dir.path())
        .args(["add", "Old title", "--description", "Old details", "--edit"])
        .env("EDITOR", editor)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("initial-content")).unwrap(),
        "Old title\n\nOld details\n"
    );
    let task: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(task["title"], "New title");
    assert_eq!(task["description"], "New details");
}

#[test]
fn editor_errors_leave_queue_empty_and_clean_temp_files() {
    for (body, expected) in [
        ("exit 7", "Editor exited unsuccessfully"),
        ("printf '   \\nbody' > \"$2\"", "Task title cannot be empty"),
        ("rm \"$2\"", "Failed to read edited task"),
    ] {
        let dir = project();
        let editor = editor(
            dir.path(),
            &format!("printf '%s' \"$2\" > edited-path\n{body}"),
        );
        let output = command(dir.path())
            .arg("add")
            .env("EDITOR", editor)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        let path = fs::read_to_string(dir.path().join("edited-path")).unwrap();
        assert!(!Path::new(&path).exists());
        let tasks = command(dir.path()).arg("list").output().unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&tasks.stdout).unwrap(),
            serde_json::json!([])
        );
    }
}

#[test]
fn editor_requires_configuration_but_inline_add_does_not() {
    let dir = project();
    for value in [None, Some(""), Some("  ")] {
        let mut cmd = command(dir.path());
        if let Some(value) = value {
            cmd.env("EDITOR", value);
        }
        let output = cmd.arg("add").output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("Set EDITOR"));
    }
    let output = command(dir.path())
        .args(["add", "Inline", "-d", "Details"])
        .env("EDITOR", "does-not-exist-qqq")
        .output()
        .unwrap();
    assert!(output.status.success());
    let task: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(task["title"], "Inline");
    assert_eq!(task["description"], "Details");
}

fn run_json(dir: &Path, args: &[&str]) -> Value {
    let output = command(dir).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn edit_arguments_patch_fields_and_preserve_task_metadata() {
    let dir = project();
    let p = dir.path();
    run_json(p, &["add", "Parent"]);
    run_json(p, &["add", "Old", "-d", "Details", "--parent", "1"]);
    let before = run_json(p, &["show", "2"]);
    let changed = run_json(p, &["edit", "2", "--description", ""]);
    assert_eq!(changed["title"], "Old");
    assert_eq!(changed["description"], "");
    let changed = run_json(p, &["edit", "2", "--title", "New"]);
    assert_eq!(changed["title"], "New");
    assert_eq!(changed["description"], "");
    for field in ["id", "parent_id", "created_at", "status", "assignee"] {
        assert_eq!(changed[field], before["task"][field]);
    }
    let changed = run_json(p, &["edit", "2", "--title", "Both", "-d", "Updated"]);
    assert_eq!(changed["title"], "Both");
    assert_eq!(changed["description"], "Updated");
    let output = command(p)
        .args(["edit", "2", "--title", "  ", "-d", "Lost"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(run_json(p, &["show", "2"])["task"], changed);
    let output = command(p)
        .args(["describe", "2", "Removed"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn edit_opens_prefilled_editor_and_persists_both_fields() {
    let dir = project();
    run_json(dir.path(), &["add", "Old", "-d", "Details"]);
    let editor = editor(
        dir.path(),
        r#"
cp "$2" initial-content
printf 'New\n\nUpdated\n' > "$2.new"
mv "$2.new" "$2"
echo editor-output
"#,
    );
    let output = command(dir.path())
        .args(["edit", "1"])
        .env("EDITOR", editor)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("initial-content")).unwrap(),
        "Old\n\nDetails\n"
    );
    let task: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(task["title"], "New");
    assert_eq!(task["description"], "Updated");
    assert_eq!(run_json(dir.path(), &["show", "1"])["task"], task);
}

#[test]
fn edit_failures_preserve_existing_task_and_clean_draft() {
    for body in ["exit 7", "printf '\\nInvalid' > \"$2\"", "rm \"$2\""] {
        let dir = project();
        let original = run_json(dir.path(), &["add", "Old", "-d", "Details"]);
        let editor = editor(
            dir.path(),
            &format!("printf '%s' \"$2\" > edited-path\n{body}"),
        );
        let output = command(dir.path())
            .args(["edit", "1"])
            .env("EDITOR", editor)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(run_json(dir.path(), &["show", "1"])["task"], original);
        let path = fs::read_to_string(dir.path().join("edited-path")).unwrap();
        assert!(!Path::new(&path).exists());
    }
}

#[test]
fn edit_missing_task_skips_editor_and_existing_task_requires_editor() {
    let dir = project();
    let editor = editor(dir.path(), "touch editor-started");
    let output = command(dir.path())
        .args(["edit", "42"])
        .env("EDITOR", editor)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Task 42 not found"));
    assert!(!dir.path().join("editor-started").exists());
    let output = command(dir.path())
        .args(["edit", "42", "-d", "Missing"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    run_json(dir.path(), &["add", "Old"]);
    let output = command(dir.path()).args(["edit", "1"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Set EDITOR"));
}

#[test]
fn interactive_edit_rejects_multiline_title_without_mutating_task() {
    for title in ["First\nSecond", "First\rSecond"] {
        let dir = project();
        let original = run_json(dir.path(), &["add", title, "-d", "Details"]);
        let editor = editor(dir.path(), "touch editor-started");
        let output = command(dir.path())
            .args(["edit", "1"])
            .env("EDITOR", editor)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("use --title or --description"));
        assert!(!dir.path().join("editor-started").exists());
        assert_eq!(run_json(dir.path(), &["show", "1"])["task"], original);
        let updated = run_json(dir.path(), &["edit", "1", "-d", "Updated"]);
        assert_eq!(updated["title"], title);
        assert_eq!(updated["description"], "Updated");
    }
}

#[test]
fn edit_recent_selects_creation_order_across_statuses_and_id_gaps() {
    let dir = project();
    let p = dir.path();
    for title in ["First", "Removed", "Newest"] {
        run_json(p, &["add", title]);
    }
    let conn = rusqlite::Connection::open(p.join("qqq.db")).unwrap();
    conn.execute("DELETE FROM tasks WHERE id=2", []).unwrap();
    conn.execute("UPDATE tasks SET created_at='same timestamp'", [])
        .unwrap();
    for id in ["1", "3"] {
        run_json(p, &["next", "--session", "recent-edit"]);
        run_json(p, &["complete", id, "--session", "recent-edit"]);
    }
    assert_eq!(run_json(p, &["edit", "-1", "--title", "Latest"])["id"], 3);
    let older = run_json(p, &["edit", "-2", "-d", "Changed older"]);
    assert_eq!(older["id"], 1);
    assert_eq!(older["status"], "completed");
    assert_eq!(run_json(p, &["edit", "-1", "-d", "Still latest"])["id"], 3);
    assert_eq!(run_json(p, &["edit", "1", "--title", "By ID"])["id"], 1);
}

#[test]
fn edit_recent_invalid_references_skip_editor_and_preserve_tasks() {
    let dir = project();
    let p = dir.path();
    let editor = editor(p, "touch editor-started");
    for populated in [false, true] {
        if populated {
            run_json(p, &["add", "Unchanged"]);
        }
        let before = run_json(p, &["list"]);
        for reference in ["0", "-2", "-9223372036854775808", "99"] {
            for direct in [false, true] {
                let mut cmd = command(p);
                cmd.args(["edit", reference]).env("EDITOR", &editor);
                if direct {
                    cmd.args(["--title", "Lost"]);
                }
                let out = cmd.output().unwrap();
                assert_eq!(
                    out.status.code(),
                    Some(1),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert!(out.stdout.is_empty());
                assert!(!p.join("editor-started").exists());
                assert_eq!(run_json(p, &["list"]), before);
            }
        }
    }
}

#[test]
fn edit_recent_pins_target_before_editor_creates_another_task() {
    let dir = project();
    let p = dir.path();
    run_json(p, &["add", "Original", "-d", "Original details"]);
    let editor = editor(
        p,
        r#"
cp "$2" initial-content
"$QQQ_TEST_BIN" add "Created during edit" >/dev/null
printf 'Edited original\n\nNew details\n' > "$2"
"#,
    );
    let out = command(p)
        .args(["edit", "-1"])
        .env("EDITOR", editor)
        .env("QQQ_TEST_BIN", env!("CARGO_BIN_EXE_qqq"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let edited: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(edited["id"], 1);
    assert_eq!(edited["title"], "Edited original");
    assert_eq!(
        fs::read_to_string(p.join("initial-content")).unwrap(),
        "Original\n\nOriginal details\n"
    );
    assert_eq!(
        run_json(p, &["show", "2"])["task"]["title"],
        "Created during edit"
    );
}

#[test]
fn explicit_edit_flag_opens_external_editor_with_field_prefills() {
    let dir = project();
    run_json(dir.path(), &["add", "Original", "-d", "Original details"]);
    let editor = editor(
        dir.path(),
        "cat \"$2\" > prefilled\nprintf 'Edited\\n\\nSaved\\n' > \"$2\"",
    );
    let output = command(dir.path())
        .args([
            "edit",
            "1",
            "--edit",
            "--title",
            "Prefill",
            "-d",
            "Prefilled details",
        ])
        .env("EDITOR", editor)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("prefilled")).unwrap(),
        "Prefill\n\nPrefilled details\n"
    );
    let task: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(task["title"], "Edited");
}

#[test]
fn external_editor_flag_conflicts_with_status_update() {
    let dir = project();
    let output = command(dir.path())
        .args(["edit", "1", "--edit", "--set-status", "new"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
}
