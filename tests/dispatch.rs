#![cfg(unix)]
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

struct Project {
    dir: TempDir,
}
impl Project {
    fn new() -> Self {
        let p = Self {
            dir: TempDir::new().unwrap(),
        };
        fs::create_dir_all(p.dir.path().join(".config/qqq")).unwrap();
        p.config("[herdr]\nnext-to-new-agent = true\n");
        let script = p.dir.path().join("herdr");
        fs::write(&script, r#"#!/usr/bin/env python3
import sys,os,json,sqlite3
from pathlib import Path
a=sys.argv[1:]
with open('calls','a') as f: f.write(json.dumps(a)+'\n')
if a[:1]==['--session']: a=a[2:]
step=a[1] if a[0]=='agent' else a[0]
if os.environ.get('FAIL_AT')==step:
    print('fake failure',file=sys.stderr);sys.exit(1)
def pane(name):
    p={'pane_id':name,'workspace_id':'workspace','tab_id':'tab-'+name,'terminal_id':'terminal-'+name,'agent':'codex'}
    if not os.environ.get('NO_ID'): p['agent_session']={'agent':'codex','kind':'id','value':'session-'+name}
    if os.environ.get('NO_TERMINAL'): p.pop('terminal_id')
    return p
if a[:2]==['tab','create']:
    name=a[a.index('--env')+1].split('=',1)[1]
    Path(name).write_text(json.dumps(pane(name)))
    result={'root_pane':pane(name),'tab':{'tab_id':'tab-'+name}}
elif a[:2]==['agent','start']: result={}
elif a[:2]==['agent','get']:
    if os.environ.get('RECLAIM_ON_GET'):
        db=sqlite3.connect('.qqq/qqq.db')
        visible = a[2] if os.environ.get('RECLAIM_ALIAS') else 'replacement'
        db.execute("UPDATE tasks SET claim_key='replacement',harness_name='other',harness_session=? WHERE claim_key=?", (visible,a[2]))
        db.commit()
    result={'agent':pane(a[2])}
elif a[:2]==['pane','current']: result={'pane':pane('caller')}
elif a[:2]==['agent','list']:
    result={'agents':[json.loads(p.read_text()) for p in Path('.').glob('qqq-dispatch-*')]}
elif a[:2]==['agent','prompt']:
    db=sqlite3.connect('.qqq/qqq.db')
    row=db.execute('SELECT t.claim_key,h.link_json,t.harness_name,t.harness_session,t.orchestrator_name,t.orchestrator_session FROM tasks t JOIN herdr_links h ON h.task_id=t.id WHERE t.claim_key=?',(a[2],)).fetchone()
    assert row is not None,'link missing before prompt'
    expected_session=('terminal-' if os.environ.get('NO_ID') else 'session-')+a[2]
    expected=tuple(os.environ.get('EXPECT_'+key, value) for key,value in zip(['HARNESS_NAME','HARNESS_SESSION','ORCHESTRATOR_NAME','ORCHESTRATOR_SESSION'], ['codex',expected_session,'herdr','default']))
    assert row[2:] == expected, 'identity missing before prompt'
    result={}
else: raise Exception(a)
print(json.dumps({'result':result}))
"#).unwrap();
        fs::set_permissions(script, fs::Permissions::from_mode(0o755)).unwrap();
        p.ok(&["init"]);
        p
    }
    fn config(&self, value: &str) {
        fs::write(self.dir.path().join(".config/qqq/config.toml"), value).unwrap();
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
        c.current_dir(self.dir.path())
            .arg("--json")
            .env("HOME", self.dir.path())
            .env("HERDR_ENV", "1")
            .env("HERDR_WORKSPACE_ID", "workspace")
            .env("HERDR_PANE_ID", "caller")
            .env_remove("QQQ_SESSION")
            .env_remove("CODEX_THREAD_ID")
            .env_remove("CODEX_SESSION_ID")
            .env_remove("HERDR_SOCKET_PATH")
            .env_remove("FAIL_AT")
            .env_remove("NO_ID")
            .env_remove("NO_TERMINAL")
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.dir.path().display(),
                    std::env::var("PATH").unwrap()
                ),
            );
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
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn calls(&self) -> Vec<Vec<String>> {
        fs::read_to_string(self.dir.path().join("calls"))
            .unwrap_or_default()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect()
    }
}

#[test]
fn named_views_share_dispatch_preview_and_claim_without_empty_or_invalid_tabs() {
    let p = Project::new();
    p.ok(&[
        "view",
        "save",
        "Worker",
        "--tag",
        "UI",
        "--readiness",
        "ready",
    ]);
    p.ok(&["add", "Partial", "--tag", "UI", "--priority", "100"]);
    assert!(
        p.ok(&["next", "--view", "Worker", "--tag", "bug"])
            .is_null()
    );
    assert!(
        !p.calls()
            .iter()
            .any(|call| call.first().map(String::as_str) == Some("tab"))
    );
    let before = p.calls();
    assert!(!p.run(&["next", "--view", "Missing"]).status.success());
    assert_eq!(p.calls(), before);
    p.ok(&[
        "add",
        "Matched",
        "--tag",
        "UI",
        "--tag",
        "bug",
        "--priority",
        "5",
    ]);
    assert_eq!(
        p.ok(&["next", "--dry-run", "--view", "Worker", "--tag", "bug"])["id"],
        2
    );
    assert_eq!(p.calls(), before);
    assert_eq!(p.ok(&["next", "--view", "Worker", "--tag", "bug"])["id"], 2);
    assert_eq!(
        p.calls()
            .iter()
            .filter(|call| call.first().map(String::as_str) == Some("tab"))
            .count(),
        1
    );
    assert_eq!(p.ok(&["show", "1"])["task"]["status"], "new");
    let calls = p.calls();
    let db = std::fs::read(p.dir.path().join(".qqq/qqq.db")).unwrap();
    std::fs::write(
        p.dir.path().join(".qqq-views.json"),
        "{\"version\":2,\"views\":[]}",
    )
    .unwrap();
    let out = p.run(&["next", "--view", "Worker"]);
    assert!(!out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stderr).unwrap()["code"],
        "CONFIG_ERROR"
    );
    assert_eq!(p.calls(), calls);
    assert_eq!(std::fs::read(p.dir.path().join(".qqq/qqq.db")).unwrap(), db);
}

#[test]
fn dispatch_dry_run_never_starts_agent_or_returns_owned_task() {
    let p = Project::new();
    p.ok(&["add", "Owned"]);
    p.ok(&["next", "--local", "--session", "caller"]);
    let queued = p.ok(&["add", "Queued"]);
    let before = p.ok(&["show", "1"]);
    let output = p
        .command()
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .args(["next", "--dry-run"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preview: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(preview, queued);
    assert_eq!(
        p.ok(&[
            "next",
            "--dry-run",
            "--session",
            "caller",
            "--harness-session",
            "changed"
        ]),
        queued
    );
    assert_eq!(p.ok(&["show", "1"]), before);
    assert!(p.calls().is_empty());
    assert!(
        p.ok(&["show", "2"])["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn dispatch_tags_match_preflight_claim_and_preview_without_empty_tabs() {
    let p = Project::new();
    p.ok(&["add", "[UI] Text only", "--priority", "100"]);
    p.ok(&["add", "Partial", "--tag", "UI", "--priority", "90"]);
    assert!(p.ok(&["next", "--tag", "UI", "--tag", "bug"]).is_null());
    assert!(
        !p.calls()
            .iter()
            .any(|call| call.first().map(String::as_str) == Some("tab"))
    );
    p.ok(&[
        "add",
        "Matched",
        "--tag",
        "UI",
        "--tag",
        "bug",
        "--priority",
        "1",
    ]);
    let preview = p.ok(&[
        "next",
        "--dry-run",
        "--tag",
        "UI",
        "--tag",
        "bug",
        "--filter",
        "has_tag('UI')",
    ]);
    assert_eq!(preview["id"], 3);
    assert_eq!(preview["status"], "new");
    assert!(
        !p.calls()
            .iter()
            .any(|call| call.first().map(String::as_str) == Some("tab"))
    );
    assert_eq!(
        p.ok(&[
            "next",
            "--tag",
            "UI",
            "--tag",
            "bug",
            "--filter",
            "has_tag('UI')"
        ])["id"],
        3
    );
    assert_eq!(
        p.calls()
            .iter()
            .filter(|call| call.first().map(String::as_str) == Some("tab"))
            .count(),
        1
    );
    assert_eq!(p.ok(&["show", "1"])["task"]["status"], "new");
    assert_eq!(p.ok(&["show", "2"])["task"]["status"], "new");
    assert_eq!(
        p.ok(&[
            "next",
            "--local",
            "--session",
            "caller",
            "--filter",
            "id == 1"
        ])["id"],
        1
    );
    assert_eq!(
        p.ok(&["next", "--session", "caller", "--tag", "absent"])["id"],
        1
    );
    assert_eq!(
        p.calls()
            .iter()
            .filter(|call| call.first().map(String::as_str) == Some("tab"))
            .count(),
        1
    );
}

#[test]
fn dispatch_filters_preflight_and_atomic_claim() {
    let p = Project::new();
    p.ok(&["add", "Other", "--priority", "100"]);
    assert!(
        p.ok(&["next", "--filter", "like(description, 'Match%')"])
            .is_null()
    );
    assert!(
        !p.calls()
            .iter()
            .any(|call| call.first().map(String::as_str) == Some("tab"))
    );
    p.ok(&["add", "Match", "--priority", "1"]);
    assert_eq!(
        p.ok(&["next", "--filter", "like(description, 'Match%')"])["id"],
        2
    );
    assert_eq!(p.ok(&["show", "1"])["task"]["status"], "new");
    assert_eq!(
        p.calls()
            .iter()
            .filter(|call| call.first().map(String::as_str) == Some("tab"))
            .count(),
        1
    );
}

#[test]
fn dispatch_claims_for_new_agent_links_before_prompt_and_can_find_session() {
    let p = Project::new();
    p.ok(&["add", "Task\n\nDetails"]);
    let task = p.ok(&["next", "--session", "caller"]);
    let detail = p.ok(&["show", "1"]);
    let owner = detail["herdr"]["pane"]["pane_id"].as_str().unwrap();
    assert_eq!(task["harness_name"], "codex");
    assert_eq!(task["harness_session"], format!("session-{owner}"));
    assert_eq!(task["orchestrator_name"], "herdr");
    assert_eq!(task["orchestrator_session"], "default");
    assert_ne!(owner, "caller");
    assert!(owner.starts_with("qqq-dispatch-"));
    let detail = p.ok(&["show", "1"]);
    assert_eq!(
        detail["herdr"]["identity"]["value"],
        format!("session-{owner}")
    );
    let calls = p.calls();
    assert_eq!(calls.len(), 6);
    assert_eq!(calls[4], ["--session", "default", "agent", "list"]);
    assert_eq!(calls[5], ["--session", "default", "agent", "list"]);
    assert_eq!(&calls[0][..2], ["tab", "create"]);
    assert!(calls[0].contains(&"--no-focus".to_owned()));
    assert!(calls[0].contains(&"workspace".to_owned()));
    assert!(calls[0].contains(&p.dir.path().canonicalize().unwrap().display().to_string()));
    assert!(calls[0].contains(&format!("QQQ_SESSION={owner}")));
    assert_eq!(
        calls[1],
        vec!["agent", "start", owner, "--kind", "codex", "--pane", owner]
    );
    assert_eq!(calls[2], vec!["agent", "get", owner]);
    assert_eq!(&calls[3][..3], ["agent", "prompt", owner]);
    assert!(calls[3][3].contains("show 1"));
    assert!(calls[3][3].contains("complete 1"));
    assert!(calls[3][3].contains("inherited QQQ_SESSION"));
    assert!(!calls[3][3].contains("--session"));
    assert!(!calls[3][3].contains(owner));
    assert_eq!(p.ok(&["herdr", "find", "1"])["pane_id"], owner);
    let calls_before_inherited = p.calls().len();
    let mut inherited = p.command();
    inherited
        .env("QQQ_SESSION", owner)
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .env("PATH", "/no-herdr")
        .env("CODEX_THREAD_ID", "different-child-thread")
        .env("CODEX_SESSION_ID", "different-child-display")
        .args(["next", "--local"]);
    let output = inherited.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let existing: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(existing, task);
    let output = p
        .command()
        .env("QQQ_SESSION", owner)
        .env("PATH", "/no-herdr")
        .args(["message", "1", "Env-only worker update"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = p
        .command()
        .env("QQQ_SESSION", owner)
        .env("PATH", "/no-herdr")
        .args(["complete", "1"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let completed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(completed["status"], "completed");
    for field in [
        "harness_name",
        "harness_session",
        "orchestrator_name",
        "orchestrator_session",
    ] {
        assert_eq!(completed[field], task[field]);
    }
    assert_eq!(
        p.calls().len(),
        calls_before_inherited,
        "Downstream env-only commands invoked Herdr"
    );
    let completed_detail = p.ok(&["show", "1"]);
    assert_eq!(completed_detail["messages"][0]["session"], owner);
    assert_eq!(completed_detail["events"][1]["session"], owner);
    p.ok(&["add", "Failure case"]);
    p.ok(&["next", "--session", "caller"]);
    let error_detail = p.ok(&["show", "2"]);
    let error_owner = error_detail["herdr"]["pane"]["pane_id"].as_str().unwrap();
    let calls_before_error = p.calls().len();
    let output = p
        .command()
        .env("QQQ_SESSION", error_owner)
        .env("PATH", "/no-herdr")
        .env("CODEX_SESSION_ID", "different-child-display")
        .args([
            "edit",
            "2",
            "--set-status",
            "error",
            "--reason",
            "Env-only failure",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let failed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(failed["status"], "error");
    assert_eq!(p.calls().len(), calls_before_error);
    let failed_detail = p.ok(&["show", "2"]);
    assert_eq!(failed_detail["messages"][0]["body"], "Env-only failure");
    assert_eq!(failed_detail["events"][1]["session"], error_owner);
}

#[test]
fn dispatched_codex_assignment_uses_child_session_not_callers_environment() {
    let p = Project::new();
    p.ok(&["add", "Task"]);
    let output = p
        .command()
        .env("CODEX_SESSION_ID", "parent-codex-session")
        .args(["next", "--session", "caller"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let task: Value = serde_json::from_slice(&output.stdout).unwrap();
    let detail = p.ok(&["show", "1"]);
    assert_eq!(
        task["harness_session"],
        detail["herdr"]["identity"]["value"]
    );
    assert_ne!(task["harness_session"], "parent-codex-session");
}

#[test]
fn caller_existing_claim_wins_and_local_bypasses_dispatch() {
    let p = Project::new();
    p.ok(&["add", "First"]);
    p.ok(&["add", "Second"]);
    let first = p.ok(&["next", "--local", "--session", "caller"]);
    assert_eq!(p.ok(&["next", "--session", "caller"]), first);
    assert!(p.calls().is_empty());
    assert_eq!(p.ok(&["show", "2"])["task"]["status"], "new");
}

#[test]
fn disabled_missing_and_empty_queue_do_not_spawn() {
    for config in ["", "[herdr]\nnext-to-new-agent = false\n"] {
        let p = Project::new();
        p.config(config);
        p.ok(&["add", "Task"]);
        assert_eq!(
            p.ok(&["next", "--session", "caller"])["harness_session"],
            "caller"
        );
        assert!(p.calls().is_empty());
    }
    let p = Project::new();
    assert_eq!(p.ok(&["next", "--session", "caller"]), Value::Null);
    p.ok(&["add", "Parent"]);
    p.ok(&["add", "Child", "--parent", "1"]);
    p.ok(&["next", "--local", "--session", "parent"]);
    assert_eq!(p.ok(&["next", "--session", "caller"]), Value::Null);
    assert!(p.calls().is_empty());
}

#[test]
fn startup_errors_release_claim_prompt_errors_retain_claim_and_link() {
    for step in ["tab", "start", "get", "prompt"] {
        let p = Project::new();
        p.ok(&["add", "Task"]);
        let out = p
            .command()
            .env("FAIL_AT", step)
            .args(["next", "--session", "caller"])
            .output()
            .unwrap();
        assert!(!out.status.success(), "{step}");
        assert!(out.stdout.is_empty());
        let failure: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(failure["code"], "DISPATCH_ERROR");
        assert_eq!(failure["details"]["task_id"], 1);
        let detail = p.ok(&["show", "1"]);
        if step == "prompt" {
            assert_eq!(detail["task"]["status"], "in_progress");
            assert!(!detail["herdr"].is_null());
            assert!(String::from_utf8_lossy(&out.stderr).contains("may have been delivered"));
            assert_eq!(failure["details"]["reason"], "prompt_delivery_uncertain");
            assert_eq!(failure["details"]["delivery_possible"], true);
            assert_eq!(failure["details"]["actual_status"], "in_progress");
        } else {
            assert_eq!(failure["details"]["reason"], "startup_failed");
            assert_eq!(failure["details"]["delivery_possible"], false);
            assert_eq!(failure["details"]["actual_status"], "new");
            assert_eq!(detail["task"]["status"], "new");
            assert!(
                !p.calls()
                    .iter()
                    .any(|c| c.get(1).map(String::as_str) == Some("prompt"))
            );
        }
    }
}

#[test]
fn terminal_fallback_links_exact_terminal_and_missing_identity_aborts() {
    let p = Project::new();
    p.ok(&["add", "Task"]);
    let out = p
        .command()
        .env("NO_ID", "1")
        .args(["next", "--session", "caller"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let detail = p.ok(&["show", "1"]);
    assert_eq!(detail["herdr"]["identity"]["kind"], "terminal");
    let owner = detail["herdr"]["pane"]["pane_id"].as_str().unwrap();
    assert_eq!(
        p.ok(&["herdr", "find", "1"])["terminal_id"],
        format!("terminal-{owner}")
    );
    let p = Project::new();
    p.ok(&["add", "Task"]);
    let out = p
        .command()
        .env("NO_ID", "1")
        .env("NO_TERMINAL", "1")
        .args(["next", "--session", "caller"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(p.ok(&["show", "1"])["task"]["status"], "new");
}

#[test]
fn invalid_config_and_missing_workspace_do_not_claim_or_spawn() {
    let p = Project::new();
    p.ok(&["add", "Task"]);
    p.config("[herdr]\nnext-to-new-agent = 'yes'\n");
    assert!(!p.run(&["next", "--session", "caller"]).status.success());
    assert_eq!(p.ok(&["show", "1"])["task"]["status"], "new");
    p.config("[herdr]\nnext-to-new-agent = true\n");
    let out = p
        .command()
        .env_remove("HERDR_WORKSPACE_ID")
        .args(["next", "--session", "caller"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(p.ok(&["show", "1"])["task"]["status"], "new");
    assert!(p.calls().is_empty());
}

#[test]
fn concurrent_dispatches_claim_distinct_tasks() {
    let p = Project::new();
    for _ in 0..3 {
        p.ok(&["add", "Task"]);
    }
    let children: Vec<_> = (0..3)
        .map(|_| {
            p.command()
                .args(["next", "--session", "caller"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let mut ids = Vec::new();
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let task: Value = serde_json::from_slice(&out.stdout).unwrap();
        ids.push(task["id"].as_i64().unwrap());
    }
    ids.sort();
    assert_eq!(ids, vec![1, 2, 3]);
    assert_eq!(
        p.calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("prompt"))
            .count(),
        3
    );
}

#[test]
fn discovered_caller_existing_claim_wins_with_reported_or_terminal_identity() {
    for terminal in [false, true] {
        let p = Project::new();
        p.ok(&["add", "Existing"]);
        p.ok(&["add", "Queued"]);
        let session = if terminal {
            r#"["codex","terminal","terminal-caller"]"#
        } else {
            r#"["codex","id","session-caller"]"#
        };
        let owned = p.ok(&["next", "--local", "--session", session]);
        let mut command = p.command();
        if terminal {
            command.env("NO_ID", "1");
        }
        let out = command.arg("next").output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), owned);
        assert_eq!(p.calls(), vec![vec!["pane", "current", "--pane", "caller"]]);
    }
}

#[test]
fn local_mode_bypasses_invalid_dispatch_config() {
    let p = Project::new();
    p.ok(&["add", "Task"]);
    p.config("[herdr]\nnext-to-new-agent = 'wrong'\n");
    assert_eq!(
        p.ok(&["next", "--local", "--session", "local"])["harness_session"],
        "local"
    );
    assert!(p.calls().is_empty());
}

#[test]
fn wait_dispatches_when_work_arrives_without_spawning_for_empty_queue() {
    let p = Project::new();
    let mut child = p
        .command()
        .args(["next", "--wait", "--session", "caller"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(child.try_wait().unwrap().is_none());
    assert!(p.calls().is_empty());
    p.ok(&["add", "Arrived"]);
    let start = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > std::time::Duration::from_secs(10) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("waiting dispatch did not finish");
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let task: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(task["description"], "Arrived");
    assert!(
        task["harness_session"]
            .as_str()
            .unwrap()
            .starts_with("session-qqq-dispatch-")
    );
    assert_eq!(
        p.calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("prompt"))
            .count(),
        1
    );
}

#[test]
fn dispatch_overrides_keep_generated_token_and_real_child_link() {
    let p = Project::new();
    p.ok(&["add", "Task"]);
    let out = p
        .command()
        .env("EXPECT_HARNESS_NAME", "custom")
        .env("EXPECT_HARNESS_SESSION", "visible")
        .env("EXPECT_ORCHESTRATOR_NAME", "custom-orch")
        .env("EXPECT_ORCHESTRATOR_SESSION", "named")
        .args([
            "next",
            "--session",
            "caller",
            "--harness-name",
            "custom",
            "--harness-session",
            "visible",
            "--orchestrator-name",
            "custom-orch",
            "--orchestrator-session",
            "named",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let task: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(task["harness_session"], "visible");
    let detail = p.ok(&["show", "1"]);
    assert_eq!(detail["herdr"]["identity"]["agent"], "codex");
    assert_eq!(detail["herdr"]["server"], "default");
    p.ok(&["complete", "1", "--harness-session", "visible"]);
}

#[test]
fn dispatch_does_not_overwrite_replacement_claim_after_slow_agent_start() {
    let p = Project::new();
    p.ok(&["add", "Task"]);
    let out = p
        .command()
        .env("RECLAIM_ON_GET", "1")
        .args(["next", "--session", "caller"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let detail = p.ok(&["show", "1"]);
    assert_eq!(detail["task"]["status"], "in_progress");
    assert_eq!(detail["task"]["harness_name"], "other");
    assert_eq!(detail["task"]["harness_session"], "replacement");
    assert!(detail["herdr"].is_null());
    assert!(
        !p.calls()
            .iter()
            .any(|call| call.get(1).map(String::as_str) == Some("prompt"))
    );
}

#[test]
fn dispatch_cleanup_never_releases_alias_of_vanished_generated_token() {
    let p = Project::new();
    p.ok(&["add", "Task"]);
    let out = p
        .command()
        .env("RECLAIM_ON_GET", "1")
        .env("RECLAIM_ALIAS", "1")
        .args(["next", "--session", "caller"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let detail = p.ok(&["show", "1"]);
    assert_eq!(detail["task"]["status"], "in_progress");
    assert_eq!(detail["task"]["harness_name"], "other");
    assert!(detail["herdr"].is_null());
    assert!(
        !p.calls()
            .iter()
            .any(|call| call.get(1).map(String::as_str) == Some("prompt"))
    );
}

#[test]
fn dispatch_prerequisites_gate_preflight_and_atomic_selection() {
    let p = Project::new();
    p.ok(&["add", "API"]);
    p.ok(&["add", "UI"]);
    p.ok(&["add", "Join", "--depends-on", "1", "--depends-on", "2"]);
    assert!(
        p.ok(&["next", "--session", "caller", "--filter", "id == 3"])
            .is_null()
    );
    assert!(p.calls().is_empty());
    for (id, owner) in [("1", "api"), ("2", "ui")] {
        p.ok(&[
            "next",
            "--local",
            "--session",
            owner,
            "--filter",
            &format!("id == {id}"),
        ]);
        p.ok(&["complete", id, "--session", owner]);
    }
    assert_eq!(
        p.ok(&["next", "--session", "caller", "--filter", "id == 3"])["id"],
        3
    );
    assert!(
        p.calls()
            .iter()
            .any(|c| c.first().map(String::as_str) == Some("agent")
                && c.get(1).map(String::as_str) == Some("prompt"))
    );
}
