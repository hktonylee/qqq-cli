#![cfg(unix)]

use serde_json::Value;
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command.current_dir(dir).env_remove("EDITOR");
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
