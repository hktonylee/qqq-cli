use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    fmt,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

static JSON_OUTPUT: AtomicBool = AtomicBool::new(false);

pub fn set_json_output(enabled: bool) {
    JSON_OUTPUT.store(enabled, Ordering::Relaxed);
}

pub fn json_output() -> bool {
    JSON_OUTPUT.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Code {
    TaskNotFound,
    OwnershipMismatch,
    InvalidTransition,
    InvalidArgument,
    InvalidFilter,
    DbBusy,
    ContentConflict,
    ProjectNotFound,
    ConfigError,
    IoError,
    EditorError,
    DispatchError,
    DatabaseError,
    CommandError,
}

#[derive(Debug)]
pub struct Info {
    pub code: Code,
    pub message: String,
    pub details: Map<String, Value>,
    human: String,
}

impl Info {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            code,
            human: message.clone(),
            message,
            details: Map::new(),
        }
    }

    pub fn detail(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.details.insert(name.into(), value.into());
        self
    }

    pub fn human(mut self, message: impl Into<String>) -> Self {
        self.human = message.into();
        self
    }

    pub fn missing_task(id: i64) -> Self {
        Self::new(Code::TaskNotFound, format!("Task {id} not found")).detail("task_id", id)
    }

    pub fn invalid_argument(argument: &str, message: impl Into<String>) -> Self {
        Self::new(Code::InvalidArgument, message).detail("argument", argument)
    }

    pub fn transition(
        id: i64,
        actual: &str,
        expected: &[&str],
        message: impl Into<String>,
    ) -> Self {
        Self::new(Code::InvalidTransition, message)
            .detail("task_id", id)
            .detail("actual_status", actual)
            .detail("expected_statuses", expected.to_vec())
    }
}

impl fmt::Display for Info {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.human)
    }
}
impl std::error::Error for Info {}

#[derive(Debug)]
pub struct Recovery {
    pub local_draft: PathBuf,
    pub current_text: Option<PathBuf>,
    pub attachments_manifest: Option<PathBuf>,
}

impl fmt::Display for Recovery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "local draft kept at {}",
            self.local_draft.display()
        )
    }
}
impl std::error::Error for Recovery {}
