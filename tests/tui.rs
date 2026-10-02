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

fn dashboard_scenario(name: &str) {
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/tui_dashboard_pty.py"
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
fn tui_dashboard_actions_popup_keeps_background_and_restores_editor() {
    dashboard_scenario("actions_popup");
    dashboard_scenario("actions_popup_no_color");
}

#[test]
fn tui_dashboard_keeps_details_and_editor_layout_stable() {
    dashboard_scenario("details");
    dashboard_scenario("details_no_color");
    dashboard_scenario("layout_new");
}

#[test]
fn tui_dashboard_details_scroll_independently() {
    dashboard_scenario("details_scroll");
}

#[test]
fn tui_dashboard_details_refresh_without_replacing_dirty_draft() {
    dashboard_scenario("details_refresh");
    dashboard_scenario("details_deleted");
}

#[test]
fn tui_dashboard_edits_and_adds_without_leaving_screen() {
    dashboard_scenario("save");
    dashboard_scenario("save_json");
}

#[test]
fn tui_dashboard_navigation_follows_displayed_tree_order() {
    dashboard_scenario("tree_navigation");
}

#[test]
fn tui_dashboard_cursor_ends_loaded_and_saved_tasks() {
    dashboard_scenario("cursor_end");
}

#[test]
fn tui_dashboard_save_keeps_new_task_open() {
    dashboard_scenario("save_selected");
}

#[test]
fn tui_dashboard_after_save_new_config_controls_new_tasks_only() {
    dashboard_scenario("after_save_open_saved");
    dashboard_scenario("after_save_open_new");
    dashboard_scenario("after_save_default_restored");
}

#[test]
fn tui_dashboard_after_save_open_new_preserves_failed_draft() {
    dashboard_scenario("after_save_open_new_error");
}

#[test]
fn tui_dashboard_ctrl_p_opens_child_draft_with_selected_parent() {
    dashboard_scenario("child");
    dashboard_scenario("child_open_new");
    dashboard_scenario("child_dirty");
    dashboard_scenario("child_no_selection");
}

#[test]
fn tui_dashboard_shift_enter_inserts_newline() {
    dashboard_scenario("shift_enter_text");
}

#[test]
fn tui_dashboard_child_save_error_preserves_parent_and_draft() {
    dashboard_scenario("child_error");
}

#[test]
fn tui_dashboard_escape_and_ctrl_c_return_to_new_before_exit() {
    dashboard_scenario("escape_selected");
    dashboard_scenario("ctrl_c_selected");
    dashboard_scenario("ctrl_c_dirty_selected");
    dashboard_scenario("escape_dirty_selected");
    dashboard_scenario("ctrl_c_dirty_selected_filter_editor");
    dashboard_scenario("ctrl_c_dirty_selected_filter_focused");
    dashboard_scenario("ctrl_c_selected_filter_menu");
    dashboard_scenario("ctrl_c_filter_empty");
}

#[test]
fn tui_dashboard_ctrl_c_confirms_dirty_new_drafts() {
    dashboard_scenario("ctrl_c_new_keep");
    dashboard_scenario("ctrl_c_new_filter");
    dashboard_scenario("ctrl_c_new_scroll");
    dashboard_scenario("ctrl_c_new_discard");
    dashboard_scenario("ctrl_c_new_image");
    dashboard_scenario("ctrl_c_new_whitespace");
}

#[test]
fn tui_dashboard_refreshes_external_commits_without_keyboard_input() {
    dashboard_scenario("live_refresh");
}

#[test]
fn tui_dashboard_external_refresh_preserves_filter_and_dirty_editor() {
    dashboard_scenario("live_refresh_filter");
}

#[test]
fn tui_dashboard_refreshes_selected_title_status_without_replacing_draft() {
    dashboard_scenario("live_title");
    dashboard_scenario("live_title_filtered");
}

#[test]
fn tui_dashboard_external_refresh_keeps_manual_list_scroll() {
    dashboard_scenario("live_refresh_scroll");
}

#[test]
fn tui_dashboard_refresh_is_not_starved_by_ignored_mouse_events() {
    dashboard_scenario("live_refresh_motion");
}

#[test]
fn tui_dashboard_hides_archived_unless_explicitly_included() {
    dashboard_scenario("archive_hidden");
    dashboard_scenario("archive_included");
}

#[test]
fn tui_dashboard_empty_exit_succeeds() {
    dashboard_scenario("empty");
    dashboard_scenario("empty_json");
}

#[test]
fn tui_dashboard_scrolls_to_selected_task() {
    dashboard_scenario("scroll");
}

#[test]
fn tui_dashboard_mouse_wheel_scrolls_both_panes() {
    dashboard_scenario("wheel");
    dashboard_scenario("wheel_error");
}

#[test]
fn tui_dashboard_click_selects_rows_and_places_caret() {
    dashboard_scenario("click");
    dashboard_scenario("click_filter");
    dashboard_scenario("click_editor_scroll");
    dashboard_scenario("click_error");
}

#[test]
fn tui_dashboard_workflow_states_and_cleanup() {
    dashboard_scenario("workflow_empty");
    dashboard_scenario("workflow_status");
}

#[test]
fn tui_dashboard_task_actions() {
    dashboard_scenario("actions_basic");
    dashboard_scenario("actions_rejected");
    dashboard_scenario("actions_narrow");
    dashboard_scenario("actions_hidden");
}

#[test]
fn tui_dashboard_complete_workflow() {
    dashboard_scenario("workflow");
}

#[test]
fn tui_dashboard_status_colors_respect_no_color() {
    dashboard_scenario("color");
    dashboard_scenario("no_color");
    dashboard_scenario("dumb");
}

#[test]
fn tui_dashboard_keeps_dirty_draft_and_survives_resize() {
    dashboard_scenario("dirty");
    dashboard_scenario("resize");
}

#[test]
fn tui_dashboard_keeps_draft_after_failed_save() {
    dashboard_scenario("save_error");
}

#[test]
fn tui_dashboard_filter_keeps_draft_and_navigates_visible_tasks() {
    dashboard_scenario("filter");
    dashboard_scenario("filter_no_color");
}

#[test]
fn tui_dashboard_plain_and_alt_slash_insert_literal_slash() {
    dashboard_scenario("slash_edit");
}

#[test]
fn tui_dashboard_ctrl_slash_accepts_legacy_and_extended_keys() {
    dashboard_scenario("filter_shortcuts");
}

#[test]
fn tui_dashboard_pasteboard_round_trip_and_plain_mode() {
    dashboard_scenario("pasteboard");
    dashboard_scenario("pasteboard_no_color");
}

#[test]
fn tui_add_history_saves_selected_task() {
    history_scenario("history_save");
}

#[test]
fn tui_add_history_shows_selected_task_status() {
    history_scenario("status_header");
    history_scenario("status_header_no_color");
}

#[test]
fn tui_add_history_skips_deleted_ids() {
    history_scenario("skip_deleted");
}

#[test]
fn tui_add_history_loads_long_task_at_end() {
    history_scenario("long_task_ends_at_bottom");
}

#[test]
fn tui_add_history_preserves_existing_and_adds_flagged_images() {
    history_scenario("existing_and_flagged_images");
}

#[test]
fn tui_pasted_image_reloads_as_editable_atom() {
    history_scenario("markdown_image_reload");
}

#[test]
fn tui_pasteboard_reloads_as_colored_atomic_item() {
    history_scenario("pasteboard_reload");
}

#[test]
fn tui_edit_command_reloads_pasteboard_and_keeps_fence_valid() {
    scenario("edit_pasteboard");
}

#[test]
fn tui_legacy_image_label_reloads_and_converts_on_save() {
    history_scenario("legacy_image_reload");
}

#[test]
fn tui_edit_command_reloads_and_saves_image_atoms() {
    scenario("edit_image");
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
fn tui_alt_arrows_move_by_word() {
    scenario("alt_words");
}
#[test]
fn tui_add_saves_multiple_tasks_without_leaving_editor() {
    scenario("continuous");
}
#[test]
fn tui_continuous_add_preserves_parent_and_uses_cli_images_once() {
    scenario("continuous_flags");
}
#[test]
fn tui_continuous_add_discards_only_unsaved_draft() {
    scenario("continuous_discard");
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
