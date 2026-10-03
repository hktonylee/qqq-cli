use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command.current_dir(root).env("HOME", root);
    for variable in [
        "QQQ_SESSION",
        "CODEX_THREAD_ID",
        "CODEX_SESSION_ID",
        "HERDR_ENV",
        "HERDR_PANE_ID",
        "EDITOR",
    ] {
        command.env_remove(variable);
    }
    command
}

fn run(root: &Path, args: &[&str]) -> Output {
    command(root).arg("--json").args(args).output().unwrap()
}

fn ok(root: &Path, args: &[&str]) -> Value {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn error(output: Output, code: &str) -> Value {
    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "Error leaked stdout: {:?}",
        output.stdout
    );
    let error: Value = serde_json::from_slice(&output.stderr).unwrap_or_else(|failure| {
        panic!(
            "Expected one JSON error: {failure}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(error["code"], code, "{error}");
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty())
    );
    assert!(error["details"].is_object());
    error
}

fn project() -> TempDir {
    let root = TempDir::new().unwrap();
    ok(root.path(), &["init"]);
    root
}

#[test]
fn json_missing_tasks_and_project_have_structured_context() {
    let root = project();
    for args in [
        vec!["show", "42"],
        vec!["edit", "42", "-d", "Draft"],
        vec!["archive", "42"],
        vec!["delete", "42"],
        vec!["message", "42", "Hello"],
        vec!["complete", "42", "--session", "caller"],
    ] {
        let result = error(run(root.path(), &args), "TASK_NOT_FOUND");
        assert_eq!(result["details"]["task_id"], 42);
        assert_eq!(result["details"]["command"], args[0]);
    }
    let result = error(run(root.path(), &["show", "-1"]), "TASK_NOT_FOUND");
    assert_eq!(result["details"]["task_reference"], -1);
    let missing = TempDir::new().unwrap();
    error(run(missing.path(), &["list"]), "PROJECT_NOT_FOUND");
}

#[test]
fn json_parser_failures_keep_status_and_separate_streams() {
    let root = project();
    for args in [
        vec!["--wat"],
        vec!["no-such-command"],
        vec!["show"],
        vec!["show", "wat"],
        vec!["add", "Text", "--priority", "101"],
        vec!["edit", "1", "--expected-revision", "0"],
        vec!["list", "--status", "pending"],
        vec!["show", "1", "--export-image", "1"],
    ] {
        let output = run(root.path(), &args);
        assert_eq!(output.status.code(), Some(2));
        error(output, "INVALID_ARGUMENT");
    }
    for args in [["show", "wat", "--json"], ["--json", "show", "wat"]] {
        error(
            command(root.path()).args(args).output().unwrap(),
            "INVALID_ARGUMENT",
        );
    }
}

#[test]
fn json_argument_filter_and_state_errors_have_stable_codes() {
    let root = project();
    ok(root.path(), &["add", "Text"]);
    error(run(root.path(), &["add", "   "]), "INVALID_ARGUMENT");
    error(run(root.path(), &["show", "0"]), "INVALID_ARGUMENT");
    error(
        run(
            root.path(),
            &[
                "edit",
                "1",
                "--force",
                "--set-status",
                "error",
                "--reason",
                "bad",
            ],
        ),
        "INVALID_ARGUMENT",
    );
    for args in [
        vec!["list", "--filter", "id =="],
        vec![
            "next",
            "--local",
            "--session",
            "caller",
            "--filter",
            "unknown_variable",
        ],
        vec!["list", "--watch", "--filter", "os.execute('bad')"],
    ] {
        let result = error(run(root.path(), &args), "INVALID_FILTER");
        assert_eq!(result["details"]["argument"], "--filter");
    }
    let result = error(run(root.path(), &["reopen", "1"]), "INVALID_TRANSITION");
    assert_eq!(result["details"]["task_id"], 1);
    assert_eq!(result["details"]["actual_status"], "new");
    assert_eq!(
        result["details"]["expected_statuses"],
        serde_json::json!(["completed"])
    );
    error(
        run(root.path(), &["complete", "1", "--session", "caller"]),
        "INVALID_TRANSITION",
    );
    error(
        run(root.path(), &["delete", "1", "--yes"]),
        "INVALID_TRANSITION",
    );
}

#[test]
fn json_ownership_errors_never_expose_claim_keys_or_identity_values() {
    let root = project();
    ok(root.path(), &["add", "Text"]);
    let output = command(root.path())
        .args(["--json", "next", "--local"])
        .env("CODEX_THREAD_ID", "owner-private-thread")
        .env("CODEX_SESSION_ID", "owner-private-display")
        .output()
        .unwrap();
    assert!(output.status.success());
    let before = ok(root.path(), &["show", "1"]);
    for args in [
        vec!["complete", "1"],
        vec!["edit", "1", "--set-status", "new", "-d", "Rejected"],
        vec!["edit", "1", "--set-status", "error", "--reason", "Rejected"],
    ] {
        let output = command(root.path())
            .arg("--json")
            .args(args)
            .env("CODEX_THREAD_ID", "caller-private-thread")
            .env("CODEX_SESSION_ID", "caller-private-display")
            .output()
            .unwrap();
        let result = error(output, "OWNERSHIP_MISMATCH");
        assert_eq!(result["details"]["task_id"], 1);
        assert_eq!(result["details"]["actual_status"], "in_progress");
        let encoded = result.to_string();
        for private in [
            "claim_key",
            "owner-private-thread",
            "owner-private-display",
            "caller-private-thread",
            "caller-private-display",
        ] {
            assert!(
                !encoded.contains(private),
                "Private value leaked: {encoded}"
            );
        }
        assert_eq!(ok(root.path(), &["show", "1"]), before);
    }
    let human = command(root.path())
        .args(["complete", "1", "--session", "human-caller"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&human.stderr).contains("not claimed by session human-caller"));
}

#[test]
fn json_content_conflicts_expose_revisions_without_description_or_mutation() {
    let root = project();
    let loaded = ok(root.path(), &["add", "Private original text"]);
    let newer = ok(root.path(), &["edit", "1", "-d", "Private newer text"]);
    let expected = loaded["content_revision"].as_i64().unwrap().to_string();
    let before = ok(root.path(), &["show", "1"]);
    let result = error(
        run(
            root.path(),
            &[
                "edit",
                "1",
                "-d",
                "Local",
                "--expected-revision",
                &expected,
                "--priority",
                "50",
            ],
        ),
        "CONTENT_CONFLICT",
    );
    assert_eq!(result["details"]["task_id"], 1);
    assert_eq!(
        result["details"]["expected_revision"],
        loaded["content_revision"]
    );
    assert_eq!(
        result["details"]["current_revision"],
        newer["content_revision"]
    );
    assert!(!result.to_string().contains("Private"));
    assert_eq!(ok(root.path(), &["show", "1"]), before);
    ok(root.path(), &["archive", "1"]);
    ok(root.path(), &["delete", "1", "--yes"]);
    let removed = error(
        run(
            root.path(),
            &["edit", "1", "-d", "Local", "--expected-revision", &expected],
        ),
        "TASK_NOT_FOUND",
    );
    assert_eq!(removed["details"]["task_id"], 1);
}

#[test]
fn json_database_errors_hide_arbitrary_trigger_diagnostics() {
    let root = project();
    ok(root.path(), &["add", "Keep"]);
    let connection = rusqlite::Connection::open(root.path().join(".qqq/qqq.db")).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_private BEFORE UPDATE OF description ON tasks BEGIN SELECT RAISE(ABORT, 'private-owner-token and full private task text'); END").unwrap();
    let output = run(root.path(), &["edit", "1", "-d", "Changed"]);
    let result = error(output, "DATABASE_ERROR");
    assert_eq!(result["details"]["sqlite_code"], 19);
    assert_eq!(result["details"]["sqlite_extended_code"], 1811);
    assert!(!result.to_string().contains("private"));
    let human = command(root.path())
        .args(["edit", "1", "-d", "Changed"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&human.stderr).contains("full private task text"));
    assert_eq!(
        ok(root.path(), &["show", "1"])["task"]["description"],
        "Keep"
    );
}

#[cfg(unix)]
#[test]
fn json_herdr_errors_hide_raw_subprocess_output() {
    use std::os::unix::fs::PermissionsExt;
    let root = project();
    ok(root.path(), &["add", "Keep"]);
    let executable = root.path().join("herdr");
    fs::write(
        &executable,
        "#!/bin/sh\nprintf 'private owner token private task text\\n' >&2\nexit 9\n",
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let mut paths = vec![root.path().to_owned()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(paths).unwrap();
    let args = [
        "herdr",
        "link",
        "1",
        "--agent",
        "codex",
        "--agent-session",
        "private-session",
    ];
    let output = command(root.path())
        .arg("--json")
        .args(args)
        .env("PATH", &path)
        .output()
        .unwrap();
    let result = error(output, "DISPATCH_ERROR");
    assert_eq!(result["details"]["reason"], "herdr_command_failed");
    assert_eq!(result["details"]["exit_code"], 9);
    assert!(!result.to_string().contains("private"));
    let human = command(root.path())
        .args(args)
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&human.stderr).contains("private owner token"));
}

#[test]
fn json_database_contention_is_retryable_and_keeps_database_intact() {
    let root = project();
    ok(root.path(), &["add", "Keep"]);
    let connection = rusqlite::Connection::open(root.path().join(".qqq/qqq.db")).unwrap();
    connection.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let result = error(run(root.path(), &["add", "Rejected"]), "DB_BUSY");
    assert_eq!(result["details"]["sqlite_code"], 5);
    connection.execute_batch("ROLLBACK").unwrap();
    assert_eq!(ok(root.path(), &["list"]).as_array().unwrap().len(), 1);
}

#[test]
fn json_alias_config_and_io_failures_emit_one_envelope() {
    let root = project();
    error(
        run(root.path(), &["config", "--get", "alias.missing"]),
        "CONFIG_ERROR",
    );
    ok(root.path(), &["config", "alias.bad", "!echo secret"]);
    error(run(root.path(), &["bad"]), "INVALID_ARGUMENT");
    ok(
        root.path(),
        &["config", "alias.showjson", "--", "--json show"],
    );
    error(
        command(root.path())
            .args(["showjson", "999"])
            .output()
            .unwrap(),
        "TASK_NOT_FOUND",
    );
    ok(
        root.path(),
        &["config", "alias.invalidjson", "--", "--json show wat"],
    );
    error(
        command(root.path()).arg("invalidjson").output().unwrap(),
        "INVALID_ARGUMENT",
    );
    error(
        run(root.path(), &["add", "Image", "--image", "missing.png"]),
        "IO_ERROR",
    );
    fs::write(root.path().join(".config/qqq/config.toml"), "[broken").unwrap();
    error(run(root.path(), &["config", "--list"]), "CONFIG_ERROR");
}

#[test]
fn json_new_commands_and_snapshot_errors_share_contract() {
    let root = project();
    fs::write(root.path().join("batch.json"), "{broken").unwrap();
    let result = error(
        run(root.path(), &["import", "batch.json"]),
        "INVALID_ARGUMENT",
    );
    assert_eq!(result["details"]["reason"], "invalid_import_json");
    assert_eq!(result["details"]["command"], "import");
    error(run(root.path(), &["import", "missing.json"]), "IO_ERROR");
    ok(root.path(), &["add", "Keep"]);
    let result = error(
        run(
            root.path(),
            &[
                "next",
                "--explain",
                "--filter",
                "like(description, '%', 'xx')",
            ],
        ),
        "INVALID_FILTER",
    );
    assert_eq!(result["details"]["reason"], "evaluation_failed");
    fs::write(root.path().join("exists.tar"), "keep").unwrap();
    error(run(root.path(), &["backup", "exists.tar"]), "COMMAND_ERROR");
    let missing = TempDir::new().unwrap();
    error(run(missing.path(), &["restore", "missing.tar"]), "IO_ERROR");
    error(run(missing.path(), &["status"]), "PROJECT_NOT_FOUND");
    assert_eq!(
        ok(root.path(), &["show", "1"])["task"]["description"],
        "Keep"
    );
}

#[test]
fn json_removed_external_editor_task_has_conflict_and_recovery_paths() {
    let root = project();
    ok(root.path(), &["add", "Keep"]);
    fs::write(
        root.path().join("editor.sh"),
        r#"
"$QQQ_JSON_TEST_BINARY" --json archive 1 >/dev/null
"$QQQ_JSON_TEST_BINARY" --json delete 1 --yes >/dev/null
printf 'Local retained text' > "$1"
"#,
    )
    .unwrap();
    let output = command(root.path())
        .args(["--json", "edit", "1", "--edit"])
        .env("EDITOR", "sh editor.sh")
        .env("TMPDIR", root.path())
        .env("QQQ_JSON_TEST_BINARY", env!("CARGO_BIN_EXE_qqq"))
        .output()
        .unwrap();
    let result = error(output, "CONTENT_CONFLICT");
    assert_eq!(result["details"]["reason"], "task_removed");
    assert_eq!(result["details"]["task_id"], 1);
    assert!(result["details"]["current_revision"].is_null());
    assert!(result["details"]["recovery"]["current_text"].is_null());
    let draft = result["details"]["recovery"]["local_draft"]
        .as_str()
        .unwrap();
    assert_eq!(fs::read_to_string(draft).unwrap(), "Local retained text");
    assert!(ok(root.path(), &["list"]).as_array().unwrap().is_empty());
}

#[test]
fn json_help_version_doctor_empty_queue_and_success_keep_existing_contract() {
    let root = project();
    for args in [["--help"], ["--version"]] {
        let output = run(root.path(), &args);
        assert!(output.status.success() && output.stderr.is_empty() && !output.stdout.is_empty());
    }
    assert!(ok(root.path(), &["next", "--local", "--session", "empty"]).is_null());
    let missing = TempDir::new().unwrap();
    let output = run(missing.path(), &["doctor"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ok"], false);
    assert_eq!(report["issues"][0]["code"], "DB_MISSING");
    assert_eq!(
        ok(root.path(), &["add", "Success"])["description"],
        "Success"
    );
    assert_eq!(ok(root.path(), &["show", "1"])["task"]["id"], 1);
}

#[test]
fn literal_json_values_do_not_switch_human_error_format() {
    let root = project();
    for args in [
        vec!["add", "--", "--json"],
        vec!["list", "--query", "--json"],
        vec!["list", "--filter", "--json"],
        vec!["add", "--description=--json"],
    ] {
        let output = command(root.path()).args(args).output().unwrap();
        if !output.status.success() {
            assert!(serde_json::from_slice::<Value>(&output.stderr).is_err());
            assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
        } else {
            assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
        }
    }
}

#[test]
fn expanded_alias_values_determine_json_mode() {
    let root = project();
    for (name, expansion, arguments) in [
        ("query", "list --query", vec!["--json", "--bad"]),
        ("prio", "add Text --priority", vec!["--json"]),
        ("outer", "inner", vec!["--json", "--bad"]),
    ] {
        ok(
            root.path(),
            &["config", &format!("alias.{name}"), expansion],
        );
        ok(root.path(), &["config", "alias.inner", "list --query"]);
        let output = command(root.path())
            .arg(name)
            .args(arguments)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(serde_json::from_slice::<Value>(&output.stderr).is_err());
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("error:"));
    }
}

#[test]
fn aliases_introducing_json_keep_envelope_when_expansion_fails() {
    let root = project();
    for expansion in ["--json", "--json --bad"] {
        ok(root.path(), &["config", "alias.onlyjson", "--", expansion]);
        let result = error(
            command(root.path()).arg("onlyjson").output().unwrap(),
            "INVALID_ARGUMENT",
        );
        assert_eq!(result["details"]["reason"], "missing_command");
        assert_eq!(result["details"]["alias"], "onlyjson");
    }
}

#[test]
fn json_external_editor_conflicts_keep_typed_recovery_without_prose() {
    let root = project();
    ok(root.path(), &["add", "Original"]);
    let temporary = root.path().join("temporary");
    fs::create_dir(&temporary).unwrap();
    fs::write(root.path().join("pending.png"), b"\x89PNG\r\n\x1a\nbytes").unwrap();
    fs::write(
        root.path().join("editor.sh"),
        r#"
printf 'editor stdout noise\n'
printf 'editor stderr noise\n' >&2
printf '%s' "$1" > edited-path
"$QQQ_JSON_TEST_BINARY" --json edit 1 -d 'Current from other' > /dev/null
printf 'Local draft\nFull details\n' > "$1"
rm pending.png
"#,
    )
    .unwrap();
    let output = command(root.path())
        .args(["--json", "edit", "1", "--edit", "--image", "pending.png"])
        .env("EDITOR", "sh editor.sh")
        .env("TMPDIR", &temporary)
        .env("QQQ_JSON_TEST_BINARY", env!("CARGO_BIN_EXE_qqq"))
        .output()
        .unwrap();
    let result = error(output, "CONTENT_CONFLICT");
    let recovery = &result["details"]["recovery"];
    let local = Path::new(recovery["local_draft"].as_str().unwrap());
    assert_eq!(
        fs::read_to_string(local).unwrap(),
        "Local draft\nFull details\n"
    );
    assert_eq!(
        fs::read_to_string(recovery["current_text"].as_str().unwrap()).unwrap(),
        "Current from other"
    );
    let attachments: Value = serde_json::from_slice(
        &fs::read(recovery["attachments_manifest"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fs::read(attachments[0]["path"].as_str().unwrap()).unwrap(),
        b"\x89PNG\r\n\x1a\nbytes"
    );
    assert_eq!(
        ok(root.path(), &["show", "1"])["task"]["description"],
        "Current from other"
    );
    assert!(
        ok(root.path(), &["show", "1"])["images"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn json_editor_exit_status_is_structured_and_human_diagnostics_remain_readable() {
    let root = project();
    ok(root.path(), &["add", "Original"]);
    fs::write(
        root.path().join("editor.sh"),
        "printf 'editor noise\\n' >&2\nexit 9\n",
    )
    .unwrap();
    let output = command(root.path())
        .args(["--json", "edit", "1", "--edit"])
        .env("EDITOR", "sh editor.sh")
        .output()
        .unwrap();
    let result = error(output, "EDITOR_ERROR");
    assert_eq!(result["details"]["exit_code"], 9);
    let output = command(root.path())
        .args(["edit", "1", "--edit"])
        .env("EDITOR", "sh editor.sh")
        .output()
        .unwrap();
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostic.contains("editor noise") && diagnostic.contains("Editor exited unsuccessfully")
    );
    assert_eq!(
        ok(root.path(), &["show", "1"])["task"]["description"],
        "Original"
    );
}
