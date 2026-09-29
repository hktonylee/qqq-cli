use serde_json::Value;
use std::{
    fs,
    process::{Command, Output},
};
use tempfile::TempDir;

struct Project {
    dir: TempDir,
}
impl Project {
    fn new(config: &str) -> Self {
        let project = Self {
            dir: TempDir::new().unwrap(),
        };
        project.config(config);
        project.ok(&["init"]);
        project
    }
    fn config(&self, content: &str) {
        let dir = self.dir.path().join(".config/qqq");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.toml"), content).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_qqq"))
            .current_dir(self.dir.path())
            .env("HOME", self.dir.path())
            .env_remove("QQQ_SESSION")
            .env_remove("HERDR_ENV")
            .env_remove("EDITOR")
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn error(&self, args: &[&str], expected: &str, code: i32) {
        let out = self.run(args);
        assert_eq!(
            out.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn aliases_expand_quotes_and_append_literal_arguments() {
    let p = Project::new("[alias]\nnew = \"add --description 'two words'\"\ns = 'show'\n");
    let title = "literal $(touch marker) ; $HOME";
    let task = p.ok(&["new", title]);
    assert_eq!(task["title"], title);
    assert_eq!(task["description"], "two words");
    assert_eq!(p.ok(&["s", "1"])["task"], task);
    assert!(!p.dir.path().join("marker").exists());
}

#[test]
fn alias_chains_support_global_session_before_and_after_alias() {
    let p = Project::new(
        "[alias]\nn = 'claim'\nclaim = 'next'\ndone = 'complete'\nowned = '--session fixed next'\n",
    );
    p.ok(&["add", "First"]);
    assert_eq!(p.ok(&["--session", "agent", "n"])["owner_session"], "agent");
    assert_eq!(
        p.ok(&["done", "1", "--session=agent"])["status"],
        "completed"
    );
    p.ok(&["add", "Second"]);
    assert_eq!(p.ok(&["owned"])["owner_session"], "fixed");
}

#[test]
fn nested_commands_and_help_work_after_expansion() {
    let p = Project::new("[alias]\nimg = 'image add'\na = 'add'\n");
    p.ok(&["add", "Task"]);
    fs::write(p.dir.path().join("image.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    p.ok(&["img", "1", "image.png"]);
    let out = p.run(&["a", "--help"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("--description"));
}

#[test]
fn invalid_aliases_fail_before_db_changes() {
    for (value, expected) in [
        ("''", "empty"),
        ("'add \"'", "quoting"),
        ("'!touch marker'", "Shell aliases"),
        ("'--session x'", "command"),
    ] {
        let p = Project::new(&format!("[alias]\nbad = {value}\n"));
        p.error(&["bad"], expected, 1);
        assert_eq!(p.ok(&["list"]), serde_json::json!([]));
        assert!(!p.dir.path().join("marker").exists());
    }
}

#[test]
fn cycles_are_rejected() {
    let p = Project::new("[alias]\na = 'b'\nb = 'a'\nself = 'self'\n");
    p.error(&["a"], "cycle", 1);
    p.error(&["self"], "cycle", 1);
}

#[test]
fn malformed_config_reports_path_but_builtins_remain_available() {
    let p = Project::new("");
    for content in [
        "[alias",
        "[alias]\na = 42",
        "[alias]\na = 'list'\na = 'next'",
    ] {
        p.config(content);
        p.error(&["a"], "config.toml", 1);
        assert_eq!(p.ok(&["list"]), serde_json::json!([]));
        assert!(p.run(&["--help"]).status.success());
    }
}

#[test]
fn builtins_win_and_command_arguments_are_not_expanded() {
    let p = Project::new("[alias]\nlist = 'next'\nhelp = 'next'\nhello = 'list'\n");
    assert_eq!(p.ok(&["add", "hello"])["title"], "hello");
    assert_eq!(p.ok(&["list"]).as_array().unwrap().len(), 1);
    assert!(p.run(&["help"]).status.success());
}

#[test]
fn missing_config_and_unknown_alias_keep_clap_errors() {
    let p = Project::new("");
    fs::remove_file(p.dir.path().join(".config/qqq/config.toml")).unwrap();
    p.error(&["unknown"], "unrecognized subcommand", 2);
    assert_eq!(p.ok(&["list"]), serde_json::json!([]));
    p.config("[alias]\nx = 'unknown'\n");
    p.error(&["x"], "unrecognized subcommand", 2);
}

#[test]
fn unreadable_config_reports_path() {
    let p = Project::new("");
    let path = p.dir.path().join(".config/qqq/config.toml");
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    p.error(&["alias"], "Cannot read", 1);
    p.error(&["alias"], "config.toml", 1);
}

#[test]
fn session_values_matching_alias_names_are_not_expanded() {
    let p = Project::new("[alias]\na = 'next'\n");
    p.ok(&["add", "Task"]);
    assert_eq!(p.ok(&["--session=a", "a"])["owner_session"], "a");
    assert!(p.run(&["--version"]).status.success());
    p.error(&["--unknown", "a"], "unexpected argument", 2);
}

#[test]
fn missing_home_does_not_read_relative_config() {
    let p = Project::new("[alias]\na = 'list'\n");
    let out = Command::new(env!("CARGO_BIN_EXE_qqq"))
        .current_dir(p.dir.path())
        .env_remove("HOME")
        .arg("a")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[cfg(unix)]
#[test]
fn alias_preserves_non_utf8_file_paths() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let p = Project::new("[alias]\nimg = 'image add'\n");
    p.ok(&["add", "Task"]);
    let name = OsString::from_vec(b"image\xff.png".to_vec());
    // macOS filesystems reject invalid UTF-8 filenames. Compare failure output
    // with the built-in command instead of creating such a file.
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_qqq"))
            .current_dir(p.dir.path())
            .env("HOME", p.dir.path())
            .args(args)
            .arg(&name)
            .output()
            .unwrap()
    };
    let direct = run(&["image", "add", "1"]);
    let alias = run(&["img", "1"]);
    assert_eq!(alias.status.code(), Some(1));
    assert_eq!(alias.stderr, direct.stderr);
    assert_eq!(alias.stdout, direct.stdout);
}
