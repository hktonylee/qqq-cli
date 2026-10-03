use serde::Deserialize;
use serde_json::Value;
use std::{path::Path, process::Command};
use tempfile::TempDir;

fn cli(dir: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_qqq"))
        .args(args)
        .current_dir(dir)
        .env("HOME", dir)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    cli(dir.path(), &["init"]);
    dir
}

#[test]
fn show_groups_description_and_aligns_complete_metadata() {
    let dir = project();
    let p = dir.path();
    cli(p, &["add", "Parent"]);
    let body = "Build 界\n\nDetails\t\u{1b}[31m";
    cli(p, &["add", body, "--parent", "1"]);
    let shown = cli(p, &["show", "-1"]);
    assert!(
        shown.starts_with(
            "#2 · New\n\nDescription:\n  Build 界\n  \n  Details\\t\\u{1b}[31m\n\nMessages: None\n\nDetails:\n"
        ),
        "{shown}"
    );
    let mut columns = Vec::new();
    for label in [
        "Parent:",
        "Created:",
        "Updated:",
        "Harness name:",
        "Harness session:",
        "Orchestrator name:",
        "Orchestrator session:",
    ] {
        let row = shown
            .lines()
            .find(|row| row.trim_start().starts_with(label))
            .unwrap();
        columns.push(
            row.find(':').unwrap() + 1 + row.split_once(':').unwrap().1.len()
                - row.split_once(':').unwrap().1.trim_start().len(),
        );
    }
    assert!(
        columns.iter().all(|&column| column == 25),
        "{columns:?}: {shown}"
    );
    assert!(shown.contains("\n\nAssignment:\n"));
    for empty in [
        "Messages: None",
        "Images: None",
        "History: None",
        "Herdr: Not linked",
    ] {
        assert!(shown.contains(empty), "{empty}: {shown}");
    }
    assert!(!shown.contains('\u{1b}'));
    let json: Value = serde_json::from_str(&cli(p, &["show", "-1", "--json"])).unwrap();
    assert_eq!(json["task"]["description"], body);
    assert_eq!(json["task"]["parent_id"], 1);
    assert_eq!(json["task"]["harness_session"], Value::Null);
}

#[test]
fn show_separates_message_headers_from_multiline_bodies() {
    let dir = project();
    let p = dir.path();
    cli(p, &["add", "Task"]);
    cli(
        p,
        &["message", "1", "First\n\nDetails", "--session", "author"],
    );
    cli(p, &["message", "1", "Second"]);
    let shown = cli(p, &["show", "1"]);
    assert!(
        shown.starts_with("#1 · New\n\nDescription:\n  Task\n\nMessages: (2)\n  #1 · author · "),
        "{shown}"
    );
    assert!(
        shown.contains("\n    First\n    \n    Details\n\n  #2 · - · "),
        "{shown}"
    );
    assert!(shown.contains("\n    Second\n\nDetails:"), "{shown}");
}

#[derive(Deserialize)]
struct TerminalOutput {
    screen: String,
    plain: String,
}
fn terminal(dir: &Path, mode: &str) -> TerminalOutput {
    let output = Command::new("python3")
        .args([
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/show_pty.py"),
            env!("CARGO_BIN_EXE_qqq"),
            dir.to_str().unwrap(),
            mode,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn terminal_show_colors_semantics_and_plain_modes_preserve_full_details() {
    let dir = project();
    let p = dir.path();
    std::fs::write(p.join("image.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    cli(
        p,
        &[
            "add",
            &format!("Long 界 body\n{}", "Details ".repeat(50)),
            "--image",
            "image.png",
        ],
    );
    cli(
        p,
        &["message", "1", "Note\n\t\u{1b}[31m", "--session", "author"],
    );
    for (status, color) in [
        ("New", None),
        ("In progress", Some("36")),
        ("Error", Some("31")),
        ("Completed", Some("90")),
    ] {
        match status {
            "In progress" => {
                cli(p, &["next", "--local", "--session", "worker"]);
            }
            "Error" => {
                cli(
                    p,
                    &[
                        "edit",
                        "1",
                        "--set-status",
                        "error",
                        "--reason",
                        "Failed",
                        "--session",
                        "worker",
                    ],
                );
            }
            "Completed" => {
                cli(p, &["edit", "1", "--set-status", "new"]);
                cli(p, &["next", "--local", "--session", "worker"]);
                cli(
                    p,
                    &["edit", "1", "--set-status", "new", "--session", "worker"],
                );
                cli(p, &["next", "--local", "--session", "worker"]);
                cli(p, &["complete", "1", "--session", "worker"]);
            }
            _ => {}
        }
        let plain = cli(p, &["show", "1"]);
        let colored = terminal(p, "color");
        assert_eq!(colored.plain, plain);
        assert!(colored.screen.contains("\u{1b}[1;36mDescription:\u{1b}[0m"));
        assert!(colored.screen.contains("\u{1b}[2mCreated:"));
        assert!(
            colored
                .screen
                .contains("\u{1b}[1mImage #1\u{1b}[0m: image.png")
        );
        assert!(
            colored
                .screen
                .contains("\u{1b}[2m(image/png, 15 bytes)\u{1b}[0m")
        );
        assert!(colored.screen.contains("\n    Note\n    \\t\\u{1b}[31m"));
        if let Some(color) = color {
            assert!(
                colored
                    .screen
                    .contains(&format!("\u{1b}[{color}m{status}\u{1b}[0m"))
            );
        } else {
            assert!(colored.screen.starts_with("\u{1b}[1m#1\u{1b}[0m · New\n"));
        }
        if status == "Completed" {
            for (action, color) in [
                ("claim", "36"),
                ("release", "33"),
                ("error", "31"),
                ("complete", "90"),
            ] {
                assert!(
                    colored.screen.contains(&format!("\u{1b}[{color}m{action}")),
                    "{action}: {}",
                    colored.screen
                );
            }
            for mode in ["no_color", "dumb"] {
                let output = terminal(p, mode);
                assert!(!output.screen.contains('\u{1b}'));
                assert_eq!(output.screen, plain);
            }
            let output = terminal(p, "json");
            assert!(!output.screen.contains('\u{1b}'));
            let actual: Value = serde_json::from_str(&output.screen).unwrap();
            let expected: Value = serde_json::from_str(&cli(p, &["show", "1", "--json"])).unwrap();
            assert_eq!(actual, expected);
        }
    }
}
