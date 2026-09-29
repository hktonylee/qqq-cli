use serde_json::Value;
use std::{path::Path, process::Command};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
    c.current_dir(dir)
        .env("HOME", dir)
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
    let added = text(p, &["add", "Build API\n\nFirst line\nSecond line"]);
    assert!(added.contains("#1"), "{added}");
    assert!(added.contains("Build API"), "{added}");
    assert!(added.contains("Status: New"));
    assert!(added.contains("First line\n  Second line"));
    text(p, &["add", "Build client", "--parent", "1"]);
    let list = text(p, &["list"]);
    for part in [
        "ID",
        "STATUS",
        "TASK",
        "Build API",
        "Build client",
        "New",
        "└── Build client",
    ] {
        assert!(list.contains(part), "{part}: {list}");
    }
    assert!(!list.contains("PARENT"));
    let next = text(p, &["next", "--session", "a"]);
    assert!(next.contains("Status: In progress"));
    assert!(next.contains("Harness session: a"));
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
    text(p, &["edit", "1", "--image", "test.png"]);
    text(p, &["next", "--session", "a"]);
    text(p, &["edit", "1", "--set-status", "new", "--session", "a"]);
    let show = text(p, &["show", "1"]);
    for part in [
        "Messages:",
        "Images:",
        "History:",
        "Useful note",
        "test.png",
        "Image #1",
        "image/png",
        "15 bytes",
        "claim",
        "release",
        "Herdr: Not linked",
        "Created:",
        "Updated:",
    ] {
        assert!(show.contains(part), "{part}: {show}");
    }
    let export = text(
        p,
        &["show", "1", "--export-image", "1", "--output", "out.png"],
    );
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
    assert_eq!(json[0]["description"], title);
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
            assert_eq!(tasks[0]["description"], "Alias task");
        }
    }
}

#[test]
fn human_list_groups_nested_dependencies_with_correct_tree_branches() {
    let d = project();
    let p = d.path();
    for args in [
        vec!["add", "Project A"],
        vec!["add", "Project B"],
        vec!["add", "API", "--parent", "1"],
        vec!["add", "UI", "--parent", "1"],
        vec!["add", "Auth", "--parent", "3"],
        vec!["add", "Storage", "--parent", "3"],
        vec!["add", "App", "--parent", "2"],
        vec!["add", "Cleanup"],
        vec!["add", "Views", "--parent", "4"],
    ] {
        text(p, &args);
    }
    text(p, &["next", "--session", "tree"]);
    text(p, &["complete", "1", "--session", "tree"]);
    text(p, &["next", "--session", "tree"]);
    let list = text(p, &["list"]);
    let rows: Vec<_> = list.lines().collect();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0], format!("{:<6} {:<12} TASK", "ID", "STATUS"));
    assert!(!list.contains("PARENT"));
    for (row, (id, title)) in rows[1..].iter().zip([
        (1, "Project A"),
        (3, "├── API"),
        (5, "│   ├── Auth"),
        (6, "│   └── Storage"),
        (4, "└── UI"),
        (9, "    └── Views"),
        (2, "Project B"),
        (7, "└── App"),
        (8, "Cleanup"),
    ]) {
        assert_eq!(row.split_whitespace().next().unwrap(), id.to_string());
        assert_eq!(&row[20..], title, "{list}");
    }
    assert!(rows[1].contains("Completed"));
    assert!(rows[7].contains("In progress"));
    let json: Value = serde_json::from_str(&text(p, &["--json", "list"])).unwrap();
    let ids: Vec<_> = json
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, (1..=9).collect::<Vec<_>>());
    assert_eq!(json[4]["parent_id"], 3);
    assert!(!list.contains('\u{1b}'));
}

#[test]
fn nested_previews_escape_terminal_controls_and_json_keeps_original_text() {
    let d = project();
    let p = d.path();
    text(p, &["add", "Root"]);
    let body = "Child\t\u{1b}[31m\nline";
    text(p, &["add", body, "--parent", "1"]);
    let list = text(p, &["list"]);
    assert_eq!(list.lines().count(), 3);
    assert!(list.contains("└── Child\\t\\u{1b}[31m"), "{list}");
    assert!(!list.contains('\u{1b}'));
    let json: Value = serde_json::from_str(&text(p, &["list", "--json"])).unwrap();
    assert_eq!(json[1]["description"], body);
}

#[test]
fn human_list_renders_long_dependency_chains() {
    let d = project();
    let p = d.path();
    let conn = rusqlite::Connection::open(p.join("qqq.db")).unwrap();
    conn.execute_batch(
        "WITH RECURSIVE ids(id) AS (SELECT 1 UNION ALL SELECT id+1 FROM ids WHERE id<512)
        INSERT INTO tasks(id,description,parent_id) SELECT id,'Task '||id,NULLIF(id-1,0) FROM ids;",
    )
    .unwrap();
    let list = text(p, &["list"]);
    let rows: Vec<_> = list.lines().collect();
    assert_eq!(rows.len(), 513);
    assert_eq!(
        &rows[512][20..],
        format!("{}└── Task 512", "    ".repeat(510))
    );
}
