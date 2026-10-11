use serde_json::Value;
use std::process::Output;

/// A valid next response exits 1 for no pickup, otherwise 0.
pub fn json(output: Output) -> Value {
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let task: Value = serde_json::from_slice(&output.stdout).expect("JSON next response");
    assert_eq!(
        output.status.code(),
        Some(if task.is_null() { 1 } else { 0 })
    );
    task
}
