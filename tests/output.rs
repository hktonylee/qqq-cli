use serde_json::Value;
use std::{path::Path, process::Command};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
    c.current_dir(dir)
        .env_remove("QQQ_SESSION")
        .env_remove("HERDR_ENV");
    c
}
fn text(dir: &Path, args: &[&str]) -> String {
    let out = command(dir).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
    String::from_utf8(out.stdout).unwrap()
}
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let init = text(dir.path(), &["init"]);
    assert!(init.starts_with("Database: "), "{init}");
    assert!(init.contains("qqq.db"));
    dir
}

#[test]
fn human_tasks_show_descriptions_dependencies_and_ownership() {
    let d = project();
    let p = d.path();
    assert_eq!(text(p, &["list"]), "No tasks yet.\n");
    assert_eq!(text(p, &["next", "--session", "a"]), "No ready tasks.\n");
    let added = text(p, &["add", "Build API", "-d", "First line\nSecond line"]);
    assert!(added.contains("#1 Build API"), "{added}");
    assert!(added.contains("Status: New"));
    assert!(added.contains("First line\n  Second line"));
    text(p, &["add", "Build client", "--parent", "1"]);
    let list = text(p, &["list"]);
    for part in [
        "ID",
        "STATUS",
        "PARENT",
        "TITLE",
        "Build API",
        "Build client",
        "New",
        "#1",
    ] {
        assert!(list.contains(part), "{part}: {list}");
    }
    let next = text(p, &["next", "--session", "a"]);
    assert!(next.contains("Status: In progress"));
    assert!(next.contains("Assignee: a"));
    let child = text(p, &["show", "2"]);
    assert!(child.contains("Parent: #1"));
    let changed = text(p, &["edit", "2", "--description", "New details"]);
    assert!(changed.contains("New details"));
    assert!(
        text(p, &["edit", "1", "--set-status", "new", "--session", "a"]).contains("Status: New")
    );
    text(p, &["next", "--session", "a"]);
    assert!(text(p, &["complete", "1", "--session", "a"]).contains("Status: Completed"));
}

#[test]
fn human_show_includes_messages_images_history_and_export_result() {
    let d = project();
    let p = d.path();
    text(p, &["add", "Task"]);
    let message = text(p, &["message", "1", "Useful note", "--session", "author"]);
    assert!(message.contains("Message #1"));
    assert!(message.contains("Task: #1"));
    assert!(message.contains("author"));
    assert!(message.contains("Useful note"));
    std::fs::write(p.join("test.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    let image = text(p, &["image", "add", "1", "test.png"]);
    for part in ["Image #1", "test.png", "image/png", "15 bytes"] {
        assert!(image.contains(part), "{image}");
    }
    text(p, &["next", "--session", "a"]);
    text(p, &["edit", "1", "--set-status", "new", "--session", "a"]);
    let show = text(p, &["show", "1"]);
    for part in [
        "Messages:",
        "Images:",
        "History:",
        "Useful note",
        "test.png",
        "claim",
        "release",
        "Herdr: Not linked",
        "Created:",
        "Updated:",
    ] {
        assert!(show.contains(part), "{part}: {show}");
    }
    let export = text(p, &["image", "export", "1", "out.png"]);
    assert!(export.contains("Exported image #1 to out.png"));
}

#[test]
fn global_json_flag_preserves_machine_output_and_errors() {
    let d = project();
    let p = d.path();
    let task: Value = serde_json::from_str(&text(p, &["--json", "add", "Task"])).unwrap();
    assert_eq!(task["id"], 1);
    let list: Value = serde_json::from_str(&text(p, &["list", "--json"])).unwrap();
    assert_eq!(list[0], task);
    text(p, &["next", "--session", "a"]);
    assert_eq!(text(p, &["next", "--session", "b", "--json"]), "null\n");
    for args in [vec!["show", "999"], vec!["--json", "show", "999"]] {
        let out = command(p).args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(1));
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("error: Task 999 not found"));
    }
}

#[cfg(unix)]
#[test]
fn human_herdr_output_includes_saved_identity_and_live_location() {
    use std::os::unix::fs::PermissionsExt;
    let d = project();
    let p = d.path();
    text(p, &["add", "Task"]);
    let bin = p.join("herdr");
    std::fs::write(&bin, r#"#!/bin/sh
printf '%s\n' '{"result":{"agents":[{"pane_id":"p1","workspace_id":"w1","tab_id":"t1","agent_session":{"agent":"codex","kind":"id","value":"session-1"}}]}}'
"#).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    for args in [
        vec![
            "herdr",
            "link",
            "1",
            "--agent",
            "codex",
            "--agent-session",
            "session-1",
        ],
        vec!["herdr", "find", "1"],
        vec!["show", "1"],
    ] {
        let out = command(p)
            .args(args)
            .env(
                "PATH",
                format!("{}:{}", p.display(), std::env::var("PATH").unwrap()),
            )
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let output = String::from_utf8(out.stdout).unwrap();
        for part in ["codex", "session-1", "Workspace: w1", "Tab: t1", "Pane: p1"] {
            assert!(output.contains(part), "{part}: {output}");
        }
    }
}

#[test]
fn human_rows_escape_control_characters_without_changing_json_data() {
    let d = project();
    let title = "Two\nlines\t\u{1b}[31m";
    text(d.path(), &["add", title]);
    let list = text(d.path(), &["list"]);
    assert_eq!(list.lines().count(), 2);
    assert!(!list.contains('\u{1b}'));
    let json: Value = serde_json::from_str(&text(d.path(), &["list", "--json"])).unwrap();
    assert_eq!(json[0]["title"], title);
}

#[test]
fn aliases_support_human_default_and_json_in_all_positions() {
    let d = project();
    let p = d.path();
    text(p, &["add", "Alias task"]);
    std::fs::create_dir_all(p.join(".config/qqq")).unwrap();
    std::fs::write(
        p.join(".config/qqq/config.toml"),
        "[alias]\nls = 'list'\njs = '--json ls'\n",
    )
    .unwrap();
    for args in [
        vec!["ls"],
        vec!["--json", "ls"],
        vec!["ls", "--json"],
        vec!["js"],
    ] {
        let output = command(p).env("HOME", p).args(&args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if args == ["ls"] {
            let output = String::from_utf8(output.stdout).unwrap();
            assert!(output.starts_with("ID"));
            assert!(output.contains("Alias task"));
        } else {
            let tasks: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(tasks[0]["title"], "Alias task");
        }
    }
}
