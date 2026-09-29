#![cfg(unix)]

use serde_json::Value;
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command.arg("--json").current_dir(dir).env_remove("EDITOR");
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
    for field in ["id", "parent_id", "created_at", "status", "owner_session"] {
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
