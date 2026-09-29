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
elif a[:2]==['agent','get']: result={'agent':pane(a[2])}
elif a[:2]==['pane','current']: result={'pane':pane('caller')}
elif a[:2]==['agent','list']:
    result={'agents':[json.loads(p.read_text()) for p in Path('.').glob('qqq-dispatch-*')]}
elif a[:2]==['agent','prompt']:
    db=sqlite3.connect('qqq.db')
    row=db.execute('SELECT t.assignee,h.link_json FROM tasks t JOIN herdr_links h ON h.task_id=t.id WHERE t.assignee=?',(a[2],)).fetchone()
    assert row is not None,'link missing before prompt'
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
fn dispatch_claims_for_new_agent_links_before_prompt_and_can_find_session() {
    let p = Project::new();
    p.ok(&["add", "Task", "-d", "Details"]);
    let task = p.ok(&["next", "--session", "caller"]);
    let owner = task["assignee"].as_str().unwrap();
    assert_ne!(owner, "caller");
    assert!(owner.starts_with("qqq-dispatch-"));
    let detail = p.ok(&["show", "1"]);
    assert_eq!(
        detail["herdr"]["identity"]["value"],
        format!("session-{owner}")
    );
    let calls = p.calls();
    assert_eq!(calls.len(), 4);
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
    assert!(calls[3][3].contains(owner));
    assert_eq!(p.ok(&["herdr", "find", "1"])["pane_id"], owner);
    p.ok(&["complete", "1", "--session", owner]);
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
        assert_eq!(p.ok(&["next", "--session", "caller"])["assignee"], "caller");
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
        let detail = p.ok(&["show", "1"]);
        if step == "prompt" {
            assert_eq!(detail["task"]["status"], "in_progress");
            assert!(!detail["herdr"].is_null());
            assert!(String::from_utf8_lossy(&out.stderr).contains("may have been delivered"));
        } else {
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
    let owner = detail["task"]["assignee"].as_str().unwrap();
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
        assert_eq!(p.calls(), vec![vec!["pane", "current", "--current"]]);
    }
}

#[test]
fn local_mode_bypasses_invalid_dispatch_config() {
    let p = Project::new();
    p.ok(&["add", "Task"]);
    p.config("[herdr]\nnext-to-new-agent = 'wrong'\n");
    assert_eq!(
        p.ok(&["next", "--local", "--session", "local"])["assignee"],
        "local"
    );
    assert!(p.calls().is_empty());
}
