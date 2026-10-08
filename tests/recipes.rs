use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
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

fn graph() -> Value {
    json!({"version":1,"tasks":[
        {"key":"publish","description":"Publish","parent":{"key":"prepare"},
         "depends_on":[{"key":"tests"}]},
        {"key":"tests","description":"Test","parent":{"key":"prepare"}},
        {"key":"prepare","description":"Prepare"}
    ]})
}

#[test]
fn recipe_graph_preserves_existing_refs_and_maps_each_new_graph_independently() {
    let f = Fixture::new();
    f.ok(&["add", "Existing"]);
    let mut value = graph();
    value["tasks"][0]["depends_on"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":1}));
    f.recipe("release", &value);
    let preview = f.ok(&["add", "--template", "release", "--dry-run"]);
    assert_eq!(
        preview["creation_order"],
        json!(["prepare", "tests", "publish"])
    );
    assert_eq!(preview["tasks"][0]["prerequisite_ids"], json!([null, 1]));
    for _ in 0..2 {
        let applied = f.ok(&["add", "--template", "release"]);
        let map = &applied["mapping"];
        let publish = map["publish"].as_i64().unwrap();
        let detail = f.ok(&["show", &publish.to_string()]);
        assert_eq!(detail["task"]["parent_id"], map["prepare"]);
        assert_eq!(
            applied["tasks"][0]["prerequisite_ids"],
            json!([map["tests"], 1])
        );
        let conn = f.db();
        for id in [map["tests"].as_i64().unwrap(), 1] {
            assert_eq!(
                conn.query_row(
                    "SELECT count(*) FROM task_dependencies WHERE task_id=? AND prerequisite_id=?",
                    [publish, id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
        }
    }
    assert_eq!(f.ok(&["list", "--all"]).as_array().unwrap().len(), 7);
}

#[test]
fn parameter_errors_and_name_discovery_fail_without_database_changes() {
    let f = Fixture::new();
    f.recipe(
        "bug",
        &json!({"version":1,"parameters":[{"name":"component"}],
        "tasks":[{"key":"bug","description":"${component}"}]}),
    );
    let before = f.bytes();
    for args in [
        vec!["add", "--template", "bug"],
        vec![
            "add",
            "--template",
            "bug",
            "--var",
            "component=a",
            "--var",
            "component=b",
        ],
        vec!["add", "--template", "bug", "--var", "unknown=a"],
        vec!["add", "--template", "bug", "--var", "component"],
        vec!["add", "--template", "bug", "--var", "=a"],
        vec!["add", "--template", "bug", "--var", "1invalid=a"],
        vec!["add", "--template", "absent"],
        vec!["add", "--template", "../bug"],
        vec!["add", "--template", "/absolute"],
        vec!["add", "--template", "bug.json"],
        vec!["add", "--template", ""],
        vec!["add", "--template", "bug", "--var", "component="],
    ] {
        let out = f.run(&args);
        assert!(!out.status.success(), "{args:?}");
        let error: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(error["code"], "INVALID_ARGUMENT", "{args:?}: {error}");
        assert_eq!(f.bytes(), before, "{args:?}");
    }
    assert_eq!(f.ok(&["list", "--all"]), json!([]));
}

#[test]
fn invalid_recipe_schema_fields_expansion_and_graphs_are_atomic() {
    let f = Fixture::new();
    let before = f.bytes();
    let valid = json!({"version":1,"tasks":[{"key":"task","description":"Task"}]});
    let mut invalid = vec![
        json!({"version":2,"tasks":[]}),
        json!({"version":1,"tasks":[],"unknown":true}),
        json!({"version":1,"parameters":[{"name":"x","default":null}],"tasks":[]}),
        json!({"version":1,"parameters":[{"name":"x","default":false}],"tasks":[]}),
        json!({"version":1,"parameters":[{"name":"x"},{"name":"x"}],"tasks":[]}),
        json!({"version":1,"parameters":[{"name":"bad-name"}],"tasks":[]}),
        json!({"version":1,"tasks":[{"key":"a","description":"A"},{"key":"a","description":"B"}]}),
        json!({"version":1,"tasks":[{"key":"a","description":"A","parent":{"key":"b"}},
                                    {"key":"b","description":"B","depends_on":[{"key":"a"}]}]}),
    ];
    for (field, value) in [
        ("description", json!(" ")),
        ("description", json!("${missing}")),
        ("description", json!("${}")),
        ("description", json!("${bad-name}")),
        ("description", json!("${unclosed")),
        ("description", json!(5)),
        ("tags", json!(["bad,tag"])),
        ("tags", json!(["${missing}"])),
        ("priority", json!(101)),
        ("priority", json!("5")),
        ("parent", json!({"key":"missing"})),
        ("parent", json!({"id":999})),
        ("parent", json!({"id":1,"key":"task"})),
        ("depends_on", json!([{"key":"task"}])),
        ("depends_on", json!([{"id":999}])),
        ("unknown", json!(true)),
        ("key", json!("")),
    ] {
        let mut v = valid.clone();
        v["tasks"][0][field] = value;
        invalid.push(v);
    }
    for value in invalid {
        f.recipe("bad", &value);
        let out = f.run(&["add", "--template", "bad"]);
        assert!(!out.status.success(), "{value}");
        assert!(out.stdout.is_empty());
        let _: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(f.bytes(), before, "{value}");
    }
    for raw in [
        r#"{"version":1,"version":1,"tasks":[]}"#,
        r#"{"version":1,"parameters":[{"name":"x","name":"x"}],"tasks":[]}"#,
        r#"{"version":1,"tasks":[{"key":"x","description":"A","description":"B"}]}"#,
        r#"{"version":1,"tasks":[{"key":"x","description":"A","parent":{"id":1,"id":1}}]}"#,
        "{invalid",
    ] {
        fs::write(f.path().join(".qqq-recipes/bad.json"), raw).unwrap();
        let out = f.run(&["add", "--template", "bad"]);
        assert!(!out.status.success(), "{raw}");
        assert_eq!(f.bytes(), before);
    }
}

#[test]
fn recipe_flags_reject_ordinary_add_inputs_and_require_template() {
    let f = Fixture::new();
    f.recipe("plain", &graph());
    let before = f.bytes();
    for extras in [
        vec!["text"],
        vec!["--description", "text"],
        vec!["--edit"],
        vec!["--stdin"],
        vec!["--parent", "1"],
        vec!["--depends-on", "1"],
        vec!["--priority", "0"],
        vec!["--image", "missing.png"],
        vec!["--tag", "label"],
    ] {
        let mut args = vec!["add", "--template", "plain"];
        args.extend(extras);
        let out = f.run(&args);
        assert!(!out.status.success(), "{args:?}");
        let error: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(
            error["details"]["reason"], "ArgumentConflict",
            "{args:?}: {error}"
        );
        assert_eq!(f.bytes(), before);
    }
    for args in [vec!["add", "--var", "name=value"], vec!["add", "--dry-run"]] {
        assert!(!f.run(&args).status.success());
        assert_eq!(f.bytes(), before);
    }
}

#[test]
fn nearest_project_discovery_ignores_cwd_shadowing_and_outer_recipes() {
    let f = Fixture::new();
    f.recipe(
        "plain",
        &json!({"version":1,"tasks":[{"key":"root","description":"Root"}]}),
    );
    let nested = f.path().join("nested");
    fs::create_dir_all(nested.join(".qqq-recipes")).unwrap();
    fs::write(
        nested.join(".qqq-recipes/plain.json"),
        json!({"version":1,"tasks":[{"key":"shadow","description":"Shadow"}]}).to_string(),
    )
    .unwrap();
    let before = f.bytes();
    let run = |args: &[&str]| {
        f.command()
            .current_dir(&nested)
            .args(args)
            .output()
            .unwrap()
    };
    let out = run(&["add", "--template", "plain", "--dry-run"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let preview: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(preview["tasks"][0]["description"], "Root");
    assert_eq!(f.bytes(), before);
    assert!(run(&["init"]).status.success());
    let out = run(&["add", "--template", "plain", "--dry-run"]);
    assert!(out.status.success());
    let preview: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(preview["tasks"][0]["description"], "Shadow");
    f.recipe("outer", &graph());
    assert!(!run(&["add", "--template", "outer"]).status.success());
    assert_eq!(f.bytes(), before);
    let missing = TempDir::new().unwrap();
    assert!(
        !f.command()
            .current_dir(missing.path())
            .args(["add", "--template", "plain", "--dry-run"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(!missing.path().join(".qqq").exists());
}

#[test]
fn preview_and_invalid_recipe_leave_old_schema_and_delete_recovery_untouched() {
    let f = Fixture::new();
    f.recipe("plain", &graph());
    let staging = f.path().join(".qqq/.delete-staging");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("marker"), "pending").unwrap();
    assert!(
        f.run(&["add", "--template", "plain", "--dry-run"])
            .status
            .success()
    );
    assert_eq!(
        fs::read_to_string(staging.join("marker")).unwrap(),
        "pending"
    );
    f.db()
        .execute_batch("DROP TABLE claim_processes; PRAGMA user_version=12;")
        .unwrap();
    let before = f.bytes();
    assert!(
        !f.run(&["add", "--template", "plain", "--dry-run"])
            .status
            .success()
    );
    f.recipe(
        "bad",
        &json!({"version":1,"parameters":[{"name":"required"}],"tasks":[]}),
    );
    assert!(!f.run(&["add", "--template", "bad"]).status.success());
    assert_eq!(f.bytes(), before);
    assert_eq!(
        f.db()
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        12
    );
    assert_eq!(
        fs::read_to_string(staging.join("marker")).unwrap(),
        "pending"
    );
}

#[test]
fn insertion_failure_rolls_back_entire_recipe_and_id_allocation() {
    let f = Fixture::new();
    f.ok(&["add", "Existing"]);
    let conn = f.db();
    conn.execute_batch(
        "CREATE TRIGGER fail_second BEFORE INSERT ON tasks WHEN NEW.description='Fail second'
        BEGIN SELECT RAISE(ABORT,'injected'); END;",
    )
    .unwrap();
    f.recipe(
        "fail",
        &json!({"version":1,"tasks":[
        {"key":"first","description":"First"},
        {"key":"second","description":"Fail second","parent":{"key":"first"}}]}),
    );
    let before = f.bytes();
    assert!(!f.run(&["add", "--template", "fail"]).status.success());
    assert_eq!(f.bytes(), before);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM tasks", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM task_dependencies", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM events", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(f.ok(&["add", "Next"])["id"], 2);
}

#[test]
fn concurrent_recipe_applications_commit_independent_complete_graphs() {
    let f = Fixture::new();
    f.recipe("release", &graph());
    let spawn = || {
        f.command()
            .args(["add", "--template", "release"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let a = spawn();
    let b = spawn();
    let mut ids = std::collections::BTreeSet::new();
    for child in [a, b] {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let report: Value = serde_json::from_slice(&out.stdout).unwrap();
        let map = &report["mapping"];
        for key in ["prepare", "tests", "publish"] {
            assert!(ids.insert(map[key].as_i64().unwrap()));
        }
        assert_eq!(report["tasks"][0]["parent_id"], map["prepare"]);
        assert_eq!(
            report["tasks"][0]["prerequisite_ids"],
            json!([map["tests"]])
        );
        let publish = map["publish"].as_i64().unwrap();
        assert_eq!(
            f.db()
                .query_row(
                    "SELECT prerequisite_id FROM task_dependencies WHERE task_id=?",
                    [publish],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            map["tests"].as_i64().unwrap()
        );
    }
    assert_eq!(ids, (1..=6).collect());
}
