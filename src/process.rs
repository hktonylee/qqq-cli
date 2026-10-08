//! Local harness identity. Failed inspection is unknown, never evidence of death.
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    process::Command,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    pub machine: String,
    pub pid: u32,
    pub started_at: String,
    pub executable: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum State {
    Alive,
    Dead,
    Unknown,
}

struct Entry {
    parent: u32,
    started_at: String,
    executable: String,
    zombie: bool,
}
pub(crate) struct Snapshot {
    machine: String,
    entries: HashMap<u32, Entry>,
}

impl Snapshot {
    pub(crate) fn read() -> Option<Self> {
        let output = Command::new("/bin/ps")
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .args(["-axo", "pid=,ppid=,lstart=,stat=,comm="])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Self::parse(machine()?, std::str::from_utf8(&output.stdout).ok()?)
    }

    fn parse(machine: String, text: &str) -> Option<Self> {
        let mut entries = HashMap::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let words: Vec<_> = line.split_whitespace().collect();
            if words.len() < 9 {
                return None;
            }
            let pid = words[0].parse::<u32>().ok()?;
            let parent = words[1].parse::<u32>().ok()?;
            let entry = Entry {
                parent,
                started_at: words[2..7].join(" "),
                executable: words[8..].join(" "),
                zombie: words[7].starts_with('Z'),
            };
            if entries.insert(pid, entry).is_some() {
                return None;
            }
        }
        if entries.is_empty() || machine.is_empty() {
            return None;
        }
        Some(Self { machine, entries })
    }

    pub(crate) fn same_machine(&self, identity: &Identity) -> bool {
        identity.machine == self.machine
    }

    pub(crate) fn state(&self, identity: &Identity) -> State {
        if !self.same_machine(identity)
            || identity.pid == 0
            || identity.pid > i32::MAX as u32
            || identity.started_at.is_empty()
            || identity.executable.is_empty()
        {
            return State::Unknown;
        }
        let Some(entry) = self.entries.get(&identity.pid) else {
            return State::Dead;
        };
        if entry.zombie {
            return State::Dead;
        }
        let Some(started_at) = start_fingerprint(identity.pid, &entry.started_at) else {
            return State::Unknown;
        };
        if identity.started_at == started_at && identity.executable == entry.executable {
            State::Alive
        } else {
            State::Dead
        }
    }

    fn ancestor(&self, agent: &str) -> Option<Identity> {
        let mut pid = std::process::id();
        let mut seen = HashSet::new();
        while pid != 0 && seen.insert(pid) {
            let entry = self.entries.get(&pid)?;
            if !entry.zombie && Path::new(&entry.executable).file_name()?.to_str()? == agent {
                return Some(Identity {
                    machine: self.machine.clone(),
                    pid,
                    started_at: start_fingerprint(pid, &entry.started_at)?,
                    executable: entry.executable.clone(),
                });
            }
            pid = entry.parent;
        }
        None
    }
}

pub(crate) fn capture(session: &str, link: Option<&crate::herdr::Link>) -> Option<Identity> {
    let codex_context = ["CODEX_THREAD_ID", "CODEX_SESSION_ID"]
        .iter()
        .any(|key| std::env::var(key).is_ok_and(|value| !value.trim().is_empty()));
    let agent = if let Some(link) = link {
        // Discovery of another pane does not make caller its owning process.
        if std::env::var("HERDR_PANE_ID").as_deref() != Ok(link.pane.pane_id.as_str()) {
            return None;
        }
        link.identity.agent.as_str()
    } else if codex_context
        && (!crate::herdr::has_context() || std::env::var("QQQ_SESSION").as_deref() == Ok(session))
    {
        "codex"
    } else {
        return None;
    };
    Snapshot::read()?.ancestor(agent)
}

#[cfg(target_os = "linux")]
fn machine() -> Option<String> {
    let machine = std::fs::read_to_string("/etc/machine-id").ok()?;
    if machine.trim().is_empty() {
        return None;
    }
    let namespace = std::fs::read_link("/proc/self/ns/pid").ok()?;
    Some(format!("linux:{}:{}", machine.trim(), namespace.to_str()?))
}

#[cfg(target_os = "macos")]
fn machine() -> Option<String> {
    let output = Command::new("/usr/sbin/ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = std::str::from_utf8(&output.stdout).ok()?;
    let line = text
        .lines()
        .find(|line| line.contains("\"IOPlatformUUID\""))?;
    let uuid = line.split('"').nth(3)?.trim();
    if uuid.is_empty() {
        return None;
    }
    Some(format!("macos:{uuid}"))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn machine() -> Option<String> {
    None
}

#[cfg(target_os = "linux")]
fn start_fingerprint(pid: u32, _: &str) -> Option<String> {
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, fields) = stat.rsplit_once(") ")?;
    let ticks = fields.split_whitespace().nth(19)?.parse::<u64>().ok()?;
    Some(format!("{}:{ticks}", boot.trim()))
}

#[cfg(not(target_os = "linux"))]
fn start_fingerprint(_: u32, timestamp: &str) -> Option<String> {
    Some(timestamp.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_or_ambiguous_process_snapshot_is_unknown() {
        for text in [
            "",
            "1 0 Mon Sep",
            "1 0 Mon Sep 28 18:06:40 2026 Ss /bin/codex\n1 0 Mon Sep 28 18:06:40 2026 Ss /bin/codex",
        ] {
            assert!(Snapshot::parse("machine".into(), text).is_none());
        }
    }
    #[test]
    fn same_machine_missing_pid_is_dead_foreign_machine_is_unknown() {
        let snapshot = Snapshot::parse(
            "machine".into(),
            "1 0 Mon Sep 28 18:06:40 2026 Ss /bin/init",
        )
        .unwrap();
        let mut identity = Identity {
            machine: "machine".into(),
            pid: 2,
            started_at: "start".into(),
            executable: "codex".into(),
        };
        assert_eq!(snapshot.state(&identity), State::Dead);
        identity.machine = "foreign".into();
        assert_eq!(snapshot.state(&identity), State::Unknown);
    }
    #[test]
    fn zombie_process_is_dead_even_with_same_pid() {
        let snapshot = Snapshot::parse(
            "machine".into(),
            "2 0 Mon Sep 28 18:06:40 2026 Z /bin/codex",
        )
        .unwrap();
        let identity = Identity {
            machine: "machine".into(),
            pid: 2,
            started_at: "start".into(),
            executable: "codex".into(),
        };
        assert_eq!(snapshot.state(&identity), State::Dead);
    }
}
