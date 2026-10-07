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
fn tui_dashboard_task_id_jump_input_and_color() {
    for name in ["jump", "jump_no_color"] {
        dashboard_scenario(name);
    }
}

#[test]
fn tui_dashboard_task_id_jump_preserves_drafts_and_reveals_targets() {
    for name in ["jump_filter", "jump_drafts", "jump_archived"] {
        dashboard_scenario(name);
    }
}

#[test]
fn tui_dashboard_task_id_jump_works_in_compact_terminal() {
    dashboard_scenario("jump_narrow");
}

#[test]
fn tui_dashboard_list_selection_reclaims_one_column() {
    dashboard_scenario("list_selection");
    dashboard_scenario("list_selection_no_color");
}

#[test]
fn tui_dashboard_task_id_jump_reveals_hidden_completed_tasks() {
    dashboard_scenario("jump_completed");
}

#[test]
fn tui_dashboard_content_conflicts_preserve_local_drafts() {
    dashboard_scenario("content_conflict");
    dashboard_scenario("content_conflict_no_color");
}

#[test]
fn tui_single_task_edit_recovers_from_content_conflicts() {
    for scenario in ["overwrite", "removed", "reload_removed"] {
        let output = Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/tui_edit_conflict_pty.py"
            ))
            .arg(env!("CARGO_BIN_EXE_qqq"))
            .arg(scenario)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn tui_dashboard_actions_popup_keeps_background_and_restores_editor() {
    dashboard_scenario("actions_popup");
    dashboard_scenario("actions_popup_no_color");
}

#[test]
fn tui_dashboard_ctrl_h_opens_selected_herdr_agent_and_preserves_editor() {
    dashboard_scenario("handoff_success");
    dashboard_scenario("handoff_terminal");
    dashboard_scenario("handoff_json");
    dashboard_scenario("handoff_filter");
    dashboard_scenario("handoff_scroll");
}

#[test]
fn tui_dashboard_ctrl_h_hint_follows_selected_link_and_external_refresh() {
    dashboard_scenario("handoff_hint");
    dashboard_scenario("handoff_hint_no_color");
}

#[test]
fn tui_dashboard_ctrl_h_errors_preserve_editor() {
    for scenario in [
        "handoff_no_selection",
        "handoff_missing",
        "handoff_stale",
        "handoff_ambiguous",
        "handoff_focus_error",
        "handoff_launch_error",
        "handoff_client_error",
    ] {
        dashboard_scenario(scenario);
    }
}

#[test]
fn tui_dashboard_keeps_details_and_editor_layout_stable() {
    dashboard_scenario("details");
    dashboard_scenario("details_no_color");
    dashboard_scenario("layout_new");
}

#[test]
fn tui_dashboard_groups_assignment_sessions_and_names() {
    dashboard_scenario("details_assignment");
    dashboard_scenario("details_assignment_no_color");
}

#[test]
fn tui_dashboard_compact_layout_hides_status_and_details() {
    dashboard_scenario("compact_layout");
    dashboard_scenario("compact_layout_no_color");
}

#[test]
fn tui_dashboard_wide_layout_resizes_and_preserves_editor() {
    dashboard_scenario("wide_layout");
    dashboard_scenario("wide_layout_no_color");
}

#[test]
fn tui_dashboard_prerequisites_refresh_while_retaining_dirty_description() {
    dashboard_scenario("prerequisites");
    dashboard_scenario("prerequisites_no_color");
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
fn tui_dashboard_buffers_exit_confirmation_preserves_every_draft_on_cancel() {
    for name in [
        "buffers_exit_n",
        "buffers_exit_enter",
        "buffers_exit_escape",
        "buffers_exit_active_new",
        "buffers_exit_deleted",
        "buffers_exit_mouse",
        "buffers_exit_filter",
        "buffers_exit_no_color",
        "buffers_exit_resize",
    ] {
        dashboard_scenario(name);
    }
}

#[test]
fn tui_dashboard_buffers_preserve_manual_scroll_and_pending_atoms() {
    dashboard_scenario("buffers_scroll");
    dashboard_scenario("buffers_atoms");
}

#[test]
fn tui_dashboard_dirty_marker_preserves_list_geometry() {
    dashboard_scenario("dirty_marker");
    dashboard_scenario("dirty_marker_no_color");
}

#[test]
fn tui_dashboard_buffers_retain_navigation_and_independent_saves() {
    for name in [
        "buffers_navigation",
        "buffers_navigation_no_color",
        "buffers_navigation_compact",
    ] {
        dashboard_scenario(name);
    }
    dashboard_scenario("buffers_mouse_revert");
    dashboard_scenario("buffers_new_child");
    dashboard_scenario("buffers_child_exit");
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
fn tui_dashboard_ctrl_c_empty_filter_returns_to_editor() {
    for name in [
        "filter_ctrl_c_empty",
        "filter_ctrl_c_empty_dirty",
        "filter_ctrl_c_empty_selected",
        "filter_ctrl_c_empty_selected_dirty",
        "filter_ctrl_c_empty_child",
        "filter_ctrl_c_empty_no_color",
        "filter_ctrl_c_empty_cleared",
        "filter_ctrl_c_empty_backspace",
    ] {
        dashboard_scenario(name);
    }
}

#[test]
fn tui_dashboard_escape_empty_filter_returns_to_editor() {
    for name in [
        "filter_escape_empty",
        "filter_escape_empty_dirty",
        "filter_escape_empty_selected",
        "filter_escape_empty_selected_dirty",
        "filter_escape_empty_child",
        "filter_escape_empty_no_color",
        "filter_escape_empty_cleared",
        "filter_escape_empty_backspace",
    ] {
        dashboard_scenario(name);
    }
}

#[test]
fn tui_dashboard_escape_clears_filter_editor_then_exits() {
    for name in [
        "escape_selected_filter_editor",
        "escape_selected_filter_focused",
        "escape_dirty_selected_filter_editor",
        "escape_dirty_selected_filter_focused",
        "escape_selected_filter_menu",
        "escape_dirty_selected_filter_confirmation",
        "escape_staged_new",
        "escape_staged_new_empty_filter",
        "escape_staged_new_image",
        "escape_staged_new_whitespace",
        "escape_staged_empty",
        "escape_staged_empty_filter",
        "escape_staged_child",
    ] {
        dashboard_scenario(name);
    }
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
fn tui_dashboard_menu_arrows() {
    dashboard_scenario("menu_arrows");
    dashboard_scenario("menu_arrows_no_color");
    dashboard_scenario("menu_arrows_narrow");
}

#[test]
fn tui_dashboard_retry_menu_action_requires_error_status() {
    for scenario in [
        "menu_retry_new",
        "menu_retry_in_progress",
        "menu_retry_completed",
        "menu_retry_error",
        "menu_retry_live",
        "menu_retry_new_no_color",
        "menu_retry_error_no_color",
    ] {
        dashboard_scenario(scenario);
    }
}

#[test]
fn tui_dashboard_force_completion_requires_explicit_confirmation() {
    dashboard_scenario("force_complete");
    dashboard_scenario("force_complete_no_color");
}

#[test]
fn tui_dashboard_force_completion_handles_session_discovery_and_claim_changes() {
    for name in [
        "force_complete_sessionless",
        "force_complete_native",
        "force_complete_race",
        "force_complete_owner_db_error",
    ] {
        dashboard_scenario(name);
    }
}

#[test]
fn tui_dashboard_force_completion_recovers_from_herdr_confirmation_failure() {
    for name in [
        "force_complete_herdr_confirm_failure",
        "force_complete_herdr_confirm_failure_no_color",
    ] {
        dashboard_scenario(name);
    }
}

#[test]
fn tui_dashboard_force_completion_checkbox_handles_initial_herdr_failure_and_mouse() {
    for name in [
        "force_complete_herdr_initial_failure",
        "force_complete_herdr_initial_failure_narrow",
        "force_complete_herdr_initial_failure_compact",
    ] {
        dashboard_scenario(name);
    }
}

#[test]
fn tui_dashboard_task_actions() {
    dashboard_scenario("actions_basic");
    dashboard_scenario("actions_rejected");
    dashboard_scenario("actions_narrow");
    dashboard_scenario("actions_hidden");
}

#[test]
fn tui_dashboard_reopen_recovers_absent_herdr_owner_and_checks_current_liveness() {
    for scenario in [
        "orphan_reopen",
        "orphan_reopen_no_color",
        "orphan_reopen_live",
    ] {
        dashboard_scenario(scenario);
    }
}

#[test]
fn tui_dashboard_mark_error_requires_reason_and_confirmation() {
    for name in ["menu_error_success", "menu_error_success_no_color"] {
        dashboard_scenario(name);
    }
}

#[test]
fn tui_dashboard_mark_error_keeps_dirty_draft_until_confirmed() {
    dashboard_scenario("menu_error_dirty");
}

#[test]
fn tui_dashboard_mark_error_enforces_current_owner_and_state() {
    for name in ["menu_error_rejected", "menu_error_live", "menu_error_new"] {
        dashboard_scenario(name);
    }
}

#[test]
fn tui_dashboard_mark_error_keeps_full_reason_in_narrow_terminal() {
    dashboard_scenario("menu_error_narrow");
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
fn tui_dashboard_completed_toggle_preserves_drafts_and_filters_live_tasks() {
    dashboard_scenario("completed_toggle");
    dashboard_scenario("completed_toggle_no_color");
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
fn tui_dashboard_only_explicit_pasteboard_fences_collapse_loaded_descriptions() {
    for name in [
        "long_description_plain",
        "long_description_no_color",
        "long_description_text_fence",
        "long_description_pasteboard",
    ] {
        dashboard_scenario(name);
    }
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

#[test]
fn tui_dashboard_tags_edit_clear_cancel_color_and_paste() {
    for name in ["tags", "tags_no_color"] {
        dashboard_scenario(name);
    }
}

#[test]
fn tui_dashboard_tags_preserve_dirty_drafts_caret_and_buffers() {
    dashboard_scenario("tags_dirty");
}

#[test]
fn tui_dashboard_tags_preserve_filter_focus_and_completed_toggle() {
    dashboard_scenario("tags_filter");
}

#[test]
fn tui_dashboard_tags_stage_new_task_before_atomic_creation() {
    for name in [
        "tags_new",
        "tags_new_no_color",
        "tags_new_wait",
        "tags_new_open_new",
    ] {
        dashboard_scenario(name);
    }
}

#[test]
fn tui_dashboard_tags_preserve_general_and_child_drafts() {
    dashboard_scenario("tags_new_child");
}

#[test]
fn tui_dashboard_tags_only_draft_can_cancel_clear_and_confirm_discard() {
    dashboard_scenario("tags_new_empty");
}
