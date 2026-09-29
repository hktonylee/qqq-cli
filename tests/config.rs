use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

struct UserConfig {
    home: TempDir,
}
impl UserConfig {
    fn new() -> Self {
        Self {
            home: TempDir::new().unwrap(),
        }
    }
    fn path(&self) -> PathBuf {
        self.home.path().join(".config/qqq/config.toml")
    }
    fn write(&self, text: &str) {
        fs::create_dir_all(self.path().parent().unwrap()).unwrap();
        fs::write(self.path(), text).unwrap();
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
        c.current_dir(self.home.path())
            .env("HOME", self.home.path())
            .env_remove("QQQ_SESSION")
            .env_remove("HERDR_ENV");
        c
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let out = self.command().arg("--json").args(args).output().unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
}

#[test]
fn config_list_get_set_unset_work_without_project_database() {
    let p = UserConfig::new();
    assert_eq!(p.ok(&["config", "--list"]), json!({}));
    assert!(!p.path().exists());
    assert_eq!(
        p.ok(&["config", "alias.ls", "list --watch"]),
        json!({"key":"alias.ls","value":"list --watch"})
    );
    assert_eq!(p.ok(&["config", "--get", "alias.ls"]), "list --watch");
    assert_eq!(p.ok(&["config", "alias.ls"]), "list --watch");
    assert_eq!(
        p.ok(&["config", "herdr.next-to-new-agent", "true"])["value"],
        true
    );
    assert_eq!(
        p.ok(&["config", "--list"]),
        json!({"alias.ls":"list --watch","herdr.next-to-new-agent":true})
    );
    assert_eq!(
        p.ok(&["config", "--unset", "alias.ls"]),
        json!({"key":"alias.ls","value":null})
    );
    assert!(!p.run(&["config", "--get", "alias.ls"]).status.success());
    assert!(!p.home.path().join("qqq.db").exists());
}

#[test]
fn config_preserves_comments_unknown_values_and_alias_strings() {
    let p = UserConfig::new();
    p.write("# Keep heading\n[alias] # Keep table comment\nls = 'list' # Keep inline comment\n[other]\nnumber = 7\nnames = ['a', 'b']\n# Keep tail\n");
    p.ok(&["config", "alias.ls", "show"]);
    let content = fs::read_to_string(p.path()).unwrap();
    for text in [
        "# Keep heading",
        "# Keep table comment",
        "# Keep inline comment",
        "number = 7",
        "names = ['a', 'b']",
        "# Keep tail",
    ] {
        assert!(content.contains(text), "{text}: {content}");
    }
    assert_eq!(p.ok(&["config", "--get", "other.number"]), 7);
    p.ok(&["config", "alias.number", "123"]);
    assert_eq!(p.ok(&["config", "alias.number"]), "123");
    p.ok(&["config", "alias.flag", "true"]);
    assert_eq!(p.ok(&["config", "alias.flag"]), "true");
    p.ok(&["config", "other.flag", "false"]);
    assert_eq!(p.ok(&["config", "other.flag"]), false);
    p.ok(&["config", "other.names", "[1, 2]"]);
    assert_eq!(p.ok(&["config", "other.names"]), json!([1, 2]));
    p.ok(&["config", "alias.\"with.dot\"", "list"]);
    assert_eq!(p.ok(&["config", "alias.\"with.dot\""]), "list");
    assert_eq!(p.ok(&["config", "--list"])["alias.\"with.dot\""], "list");
}

#[test]
fn invalid_edits_leave_config_unchanged_and_reads_do_not_create_files() {
    let p = UserConfig::new();
    for args in [
        ["config", "--get", "missing"],
        ["config", "--unset", "missing"],
    ] {
        assert!(!p.run(&args).status.success());
        assert!(!p.path().exists());
    }
    p.write("[herdr]\nnext-to-new-agent = false\n[other]\nleaf = 'value'\n");
    let before = fs::read(p.path()).unwrap();
    for args in [
        vec!["config", "herdr.next-to-new-agent", "yes"],
        vec!["config", "other.leaf.child", "invalid"],
        vec!["config", "alias..broken", "list"],
    ] {
        let out = p.run(&args);
        assert_eq!(
            out.status.code(),
            Some(1),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read(p.path()).unwrap(), before);
    }
    p.write("not valid toml[");
    let before = fs::read(p.path()).unwrap();
    assert!(!p.run(&["config", "alias.ls", "list"]).status.success());
    assert_eq!(fs::read(p.path()).unwrap(), before);
}

#[test]
fn human_output_and_argument_conflicts_are_clear() {
    let p = UserConfig::new();
    assert_eq!(
        String::from_utf8(p.run(&["config", "--list"]).stdout).unwrap(),
        "No config values set.\n"
    );
    let out = p.run(&["config", "alias.ls", "list"]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "Set alias.ls.\n");
    assert_eq!(
        String::from_utf8(p.run(&["config", "alias.ls"]).stdout).unwrap(),
        "list\n"
    );
    assert_eq!(
        String::from_utf8(p.run(&["config", "--list"]).stdout).unwrap(),
        "alias.ls=list\n"
    );
    assert_eq!(
        String::from_utf8(p.run(&["config", "--unset", "alias.ls"]).stdout).unwrap(),
        "Unset alias.ls.\n"
    );
    for args in [
        vec!["config"],
        vec!["config", "--list", "--get", "alias.ls"],
        vec!["config", "--unset", "alias.ls", "other.key"],
    ] {
        assert_eq!(p.run(&args).status.code(), Some(2));
    }
    let out = p
        .command()
        .env_remove("HOME")
        .args(["config", "--list"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn configured_alias_works_and_unsetting_herdr_restores_default() {
    let p = UserConfig::new();
    p.ok(&["config", "alias.ls", "list"]);
    p.ok(&["init"]);
    p.ok(&["add", "Task"]);
    assert_eq!(p.ok(&["ls"])[0]["title"], "Task");
    p.ok(&["config", "herdr.next-to-new-agent", "true"]);
    p.ok(&["config", "--unset", "herdr.next-to-new-agent"]);
    assert_eq!(p.ok(&["next", "--session", "worker"])["assignee"], "worker");
}

#[test]
fn concurrent_writers_keep_each_independent_setting() {
    let p = UserConfig::new();
    let children: Vec<_> = (0..10)
        .map(|i| {
            p.command()
                .args(["config", &format!("alias.a{i}"), "list"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let values = p.ok(&["config", "--list"]);
    assert_eq!(values.as_object().unwrap().len(), 10);
    for i in 0..10 {
        assert_eq!(values[format!("alias.a{i}")], "list");
    }
}

#[test]
fn config_edits_inline_tables_and_repairs_invalid_known_values() {
    let p = UserConfig::new();
    p.write("other = { kept = 7 } # retain\n[herdr]\nnext-to-new-agent = 'wrong'\n");
    assert_eq!(p.ok(&["config", "herdr.next-to-new-agent"]), "wrong");
    p.ok(&["config", "herdr.next-to-new-agent", "false"]);
    p.ok(&["config", "other.nested.leaf", "42"]);
    assert_eq!(
        p.ok(&["config", "other"]),
        json!({"kept":7,"nested":{"leaf":42}})
    );
    p.ok(&["config", "--unset", "other.nested.leaf"]);
    assert_eq!(p.ok(&["config", "other.kept"]), 7);
    assert!(fs::read_to_string(p.path()).unwrap().contains("# retain"));
    p.ok(&["config", "alias", "{ ls = 'list' }"]);
    p.ok(&["config", "alias.ls", "show"]);
    assert_eq!(p.ok(&["config", "alias.ls"]), "show");
}

#[test]
fn config_human_output_escapes_controls_and_json_keeps_original() {
    let p = UserConfig::new();
    let value = "list\n\u{1b}[31m";
    p.ok(&["config", "alias.ls", value]);
    assert_eq!(p.ok(&["config", "alias.ls"]), value);
    assert_eq!(
        String::from_utf8(p.run(&["config", "alias.ls"]).stdout).unwrap(),
        "list\\n\\u{1b}[31m\n"
    );
    let out = p.run(&["config", "--list"]);
    assert!(!out.stdout.contains(&27));
}

#[test]
fn config_validates_display_setting_and_applies_it_to_human_lists() {
    let p = UserConfig::new();
    p.ok(&["config", "display.max-completed", "0"]);
    let before = fs::read(p.path()).unwrap();
    for value in ["-1", "false", "1.5", "'oops'", "9223372036854775808"] {
        let out = p.run(&["config", "display.max-completed", "--", value]);
        assert_eq!(
            out.status.code(),
            Some(1),
            "{value}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(fs::read(p.path()).unwrap(), before);
    }
    p.ok(&["init"]);
    p.ok(&["add", "Completed task"]);
    p.ok(&["next", "--session", "worker", "--local"]);
    p.ok(&["complete", "1", "--session", "worker"]);
    assert_eq!(
        String::from_utf8(p.run(&["list"]).stdout).unwrap(),
        "No tasks to display.\n"
    );
    assert_eq!(p.ok(&["list"]).as_array().unwrap().len(), 1);
    p.ok(&["config", "--unset", "display.max-completed"]);
    assert!(
        String::from_utf8(p.run(&["list"]).stdout)
            .unwrap()
            .contains("Completed task")
    );
}

#[cfg(unix)]
#[test]
fn config_updates_preserve_symlink_and_target_permissions() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};
    let p = UserConfig::new();
    let target = p.home.path().join("dotfile.toml");
    fs::write(&target, "[alias]\nls='list'\n").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    fs::create_dir_all(p.path().parent().unwrap()).unwrap();
    symlink(&target, p.path()).unwrap();
    p.ok(&["config", "alias.ls", "show"]);
    assert!(
        fs::symlink_metadata(p.path())
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(fs::read_to_string(&target).unwrap().contains("show"));
    assert_eq!(p.ok(&["config", "alias.ls"]), "show");
    assert!(!Path::new(p.home.path()).join("qqq.db").exists());
}
