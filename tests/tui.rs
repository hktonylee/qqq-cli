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

fn history_scenario(name: &str) {
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/tui_history_pty.py"
        ))
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
fn tui_add_history_saves_selected_task() {
    history_scenario("history_save");
}

#[test]
fn tui_add_history_skips_deleted_ids() {
    history_scenario("skip_deleted");
}

#[test]
fn tui_add_history_loads_long_task_at_top() {
    history_scenario("long_task_starts_at_top");
}

#[test]
fn tui_add_history_preserves_existing_and_adds_flagged_images() {
    history_scenario("existing_and_flagged_images");
}

#[test]
fn tui_add_history_deleted_selection_does_not_create_task() {
    history_scenario("selected_deleted");
}

#[test]
fn tui_add_history_returns_to_new_draft() {
    history_scenario("return_new");
}

#[test]
fn tui_add_history_checks_dirty_new_draft() {
    history_scenario("dirty_new_keep");
    history_scenario("dirty_new_discard");
    history_scenario("dirty_image_discard");
}

#[test]
fn tui_add_history_checks_dirty_loaded_task() {
    history_scenario("dirty_loaded_keep");
}

#[test]
fn tui_add_history_boundary_keeps_draft() {
    history_scenario("oldest_boundary");
    history_scenario("empty_boundary");
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
fn tui_blank_body_remains_editable_until_valid_save() {
    scenario("blank");
}
#[test]
fn tui_ctrl_w_deletes_previous_word() {
    scenario("ctrl_w");
}
#[test]
fn tui_edit_pins_recent_target_and_preserves_ownership() {
    scenario("edit");
}

#[test]
fn tui_edit_shift_arrows_do_not_navigate_tasks() {
    scenario("edit_shift");
}
#[test]
fn tui_no_color_keeps_plain_editor_and_terminal_restoration() {
    scenario("no_color");
}
#[test]
fn tui_dumb_terminal_keeps_plain_editor_and_terminal_restoration() {
    scenario("dumb");
}

#[test]
fn tui_escape_requires_explicit_discard_for_text_images_and_whitespace() {
    for name in [
        "escape_discard",
        "escape_image",
        "escape_whitespace",
        "escape_narrow",
    ] {
        scenario(name);
    }
}
#[test]
fn tui_escape_empty_or_deleted_buffer_exits_without_prompt() {
    for name in ["escape_empty", "escape_deleted"] {
        scenario(name);
    }
}
#[test]
fn tui_escape_keep_choices_preserve_pastes_images_and_ignore_modal_input() {
    scenario("escape_keep");
}
#[test]
fn tui_escape_confirms_unchanged_existing_content_then_resumes_editing() {
    scenario("escape_edit");
}
