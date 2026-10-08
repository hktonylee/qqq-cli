use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

struct Fixture(TempDir);
impl Fixture {
    fn new() -> Self {
        let f = Self(TempDir::new().unwrap());
        f.ok(&["init"]);
        fs::create_dir(f.path().join(".qqq-recipes")).unwrap();
        f
    }
    fn path(&self) -> &Path {
        self.0.path()
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
        c.current_dir(self.path())
            .arg("--json")
            .env_remove("QQQ_SESSION")
            .env_remove("CODEX_THREAD_ID")
            .env_remove("CODEX_SESSION_ID")
            .env_remove("HERDR_ENV")
            .env_remove("HERDR_PANE_ID")
            .env("PATH", self.path());
        c
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn recipe(&self, name: &str, value: &Value) {
        fs::write(
            self.path()
                .join(".qqq-recipes")
                .join(format!("{name}.json")),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap();
    }
    fn db(&self) -> Connection {
        Connection::open(self.path().join(".qqq/qqq.db")).unwrap()
    }
    fn bytes(&self) -> Vec<u8> {
        fs::read(self.path().join(".qqq/qqq.db")).unwrap()
    }
}

#[test]
fn single_recipe_previews_without_ids_then_applies_defaults_and_metadata() {
    let f = Fixture::new();
    f.recipe(
        "bug",
        &json!({"version":1,
        "parameters":[{"name":"component"},{"name":"severity","default":"normal"}],
        "tasks":[{"key":"bug", "description":"Fix ${component}\nSeverity: ${severity}",
            "tags":["bug","${component}"],"priority":5}]}),
    );
    let before = f.bytes();
    let preview = f.ok(&[
        "add",
        "--template",
        "bug",
        "--var",
        "component=authλ",
        "--dry-run",
    ]);
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["count"], 1);
    assert_eq!(preview["mapping"], json!({}));
    assert_eq!(preview["creation_order"], json!(["bug"]));
    assert_eq!(preview["tasks"][0]["id"], Value::Null);
    assert_eq!(
        preview["tasks"][0]["description"],
        "Fix authλ\nSeverity: normal"
    );
    assert_eq!(f.bytes(), before);
    let applied = f.ok(&["add", "--template", "bug", "--var", "component=authλ"]);
    assert_eq!(applied["mapping"], json!({"bug":1}));
    assert_eq!(applied["dry_run"], false);
    let detail = f.ok(&["show", "1"]);
    assert_eq!(detail["task"]["description"], "Fix authλ\nSeverity: normal");
    assert_eq!(detail["task"]["tags"], json!(["bug", "authλ"]));
    assert_eq!(detail["task"]["priority"], 5);
    assert_eq!(detail["task"]["status"], "new");
    assert_eq!(detail["task"]["content_revision"], 1);
    assert_eq!(detail["events"], json!([]));
    assert_eq!(
        f.db()
            .query_row("SELECT count(*) FROM claim_processes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn recipe_values_are_literal_single_pass_text_with_explicit_dollar_escaping() {
    let f = Fixture::new();
    let value = "  auth=λ🙂\r\n\"quoted\" ${severity} $(touch unwanted) `echo hi`  ";
    f.recipe("literal", &json!({"version":1,
        "parameters":[{"name":"component"},{"name":"severity","default":"${component}"}],
        "tasks":[{"key":"literal", "description":"${component}\nDefault:${severity}\nEscaped:$${component} $$ $HOME"}]}));
    let assignment = format!("component={value}");
    let applied = f.ok(&["add", "--template", "literal", "--var", &assignment]);
    let expected = format!("{value}\nDefault:${{component}}\nEscaped:${{component}} $ $HOME");
    assert_eq!(applied["tasks"][0]["description"], expected);
    assert_eq!(f.ok(&["show", "1"])["task"]["description"], expected);
    assert!(!f.path().join("unwanted").exists());
}

#[test]
fn parameterless_recipe_keeps_import_defaults_and_can_be_applied_repeatedly() {
    let f = Fixture::new();
    f.recipe(
        "plain",
        &json!({"version":1,"tasks":[{"key":"work","description":"Task"}]}),
    );
    assert_eq!(
        f.ok(&["add", "--template", "plain"])["mapping"],
        json!({"work":1})
    );
    assert_eq!(
        f.ok(&["add", "--template", "plain"])["mapping"],
        json!({"work":2})
    );
    let task = f.ok(&["show", "2"]);
    assert_eq!(task["task"]["priority"], 0);
    assert_eq!(task["task"]["tags"], json!([]));
    assert_eq!(task["task"]["parent_id"], Value::Null);
}
