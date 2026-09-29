use std::process::Command;

fn scenario(name: &str) {
    let output = Command::new("python3")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/tui_pty.py"))
        .arg(env!("CARGO_BIN_EXE_qqq"))
        .arg(name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn tui_saves_large_paste_and_image_with_clean_json_stdout() {
    scenario("save");
}
#[test]
fn tui_cancel_leaves_database_unchanged_and_restores_terminal() {
    scenario("cancel");
}
#[test]
fn tui_blank_title_remains_editable_until_valid_save() {
    scenario("blank");
}
#[test]
fn tui_edit_pins_recent_target_and_preserves_ownership() {
    scenario("edit");
}
