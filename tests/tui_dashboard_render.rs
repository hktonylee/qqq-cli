#[path = "../src/tui/dashboard.rs"]
mod dashboard;
#[allow(dead_code)]
#[path = "../src/tui/panel.rs"]
mod panel;
#[allow(dead_code)]
#[path = "../src/tui/render.rs"]
mod render;

use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Position, style::Color};
use std::collections::HashMap;

fn line(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

#[test]
fn wheel_hit_test_uses_list_editor_and_excludes_edges() {
    assert_eq!(
        dashboard::wheel_area((72, 16), 5, 3),
        Some(dashboard::WheelArea::List(5))
    );
    assert_eq!(
        dashboard::wheel_area((72, 16), 5, 1),
        Some(dashboard::WheelArea::List(5))
    );
    assert_eq!(dashboard::wheel_area((72, 16), 5, 7), None);
    assert_eq!(
        dashboard::wheel_area((72, 16), 5, 10),
        Some(dashboard::WheelArea::Editor(6))
    );
    assert_eq!(dashboard::wheel_area((72, 16), 5, 15), None);
    assert_eq!(dashboard::wheel_area((72, 16), 72, 3), None);
    assert_eq!(dashboard::wheel_area((10, 7), 5, 3), None);
}

#[test]
fn click_target_maps_rendered_task_rows_and_editor_caret() {
    let rows = panel::rows(
        "ID     STATUS       TASK\n1      New          Parent\n                    detail\n2      New          └── Child",
    );
    let layout = render::Layout::new(
        &["a", "b", "c", "\n", "d", "e", "f"].map(str::to_owned),
        &[],
        72,
    );
    let hit = |column, row, list_top, editor_top| {
        dashboard::click_target((72, 16), column, row, &rows, list_top, editor_top, &layout)
    };
    assert_eq!(hit(5, 3, 0, 0), Some(dashboard::ClickTarget::Task(1)));
    assert_eq!(hit(5, 4, 0, 0), Some(dashboard::ClickTarget::Task(1)));
    assert_eq!(hit(70, 5, 0, 0), Some(dashboard::ClickTarget::Task(2)));
    assert_eq!(hit(5, 2, 2, 0), Some(dashboard::ClickTarget::Task(1)));
    assert_eq!(hit(5, 3, 2, 0), Some(dashboard::ClickTarget::Task(2)));
    assert_eq!(hit(2, 9, 0, 0), Some(dashboard::ClickTarget::Editor(2)));
    assert_eq!(hit(10, 9, 0, 0), Some(dashboard::ClickTarget::Editor(3)));
    assert_eq!(hit(1, 9, 0, 1), Some(dashboard::ClickTarget::Editor(5)));
}

#[test]
fn click_target_ignores_non_content_and_out_of_bounds() {
    let rows = panel::rows("ID     STATUS       TASK\n1      New          First");
    let layout = render::Layout::new(&["abc".into()], &[], 72);
    for (column, row) in [
        (5, 0),
        (5, 1),
        (5, 2),
        (5, 4),
        (5, 7),
        (5, 8),
        (5, 10),
        (5, 15),
        (72, 3),
    ] {
        assert_eq!(
            dashboard::click_target((72, 16), column, row, &rows, 0, 0, &layout),
            None,
            "{column},{row}"
        );
    }
    assert_eq!(
        dashboard::click_target((10, 7), 5, 3, &rows, 0, 0, &layout),
        None
    );
}

#[test]
fn manual_list_scroll_does_not_snap_to_selected_task() {
    let tree = format!(
        "ID     STATUS       TASK\n{}",
        (1..=10)
            .map(|id| format!("{id}      New          Task {id}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let rows = panel::rows(&tree);
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Editor",
        keys: render::KEYS,
        message: "",
    };
    let mut top = 0;
    let mut terminal = Terminal::new(TestBackend::new(72, 16)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &rows,
                &HashMap::new(),
                Some(10),
                dashboard::ListView {
                    query: "",
                    focused: false,
                    top: &mut top,
                    follow_selected: false,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: true,
                },
                false,
            )
        })
        .unwrap();
    assert_eq!(top, 0);
    assert!(line(terminal.backend().buffer(), 2).contains("ID"));
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &rows,
                &HashMap::new(),
                Some(10),
                dashboard::ListView {
                    query: "",
                    focused: false,
                    top: &mut top,
                    follow_selected: true,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: true,
                },
                false,
            )
        })
        .unwrap();
    assert_eq!(top, 6);
    assert!(line(terminal.backend().buffer(), 6).contains("Task 10"));
}

#[test]
fn manual_editor_scroll_keeps_viewport_then_keyboard_reveals_caret() {
    let layout = render::Layout::new(&["A\nB\nC\nD\nE\nF\nG\nH\nI\nJ".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Editor",
        keys: render::KEYS,
        message: "",
    };
    let cursor = layout.positions.len() - 1;
    let mut top = 0;
    let mut terminal = Terminal::new(TestBackend::new(72, 16)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &[],
                &HashMap::new(),
                None,
                dashboard::ListView {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor,
                    top: &mut top,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: false,
                },
                false,
            )
        })
        .unwrap();
    assert_eq!(top, 0);
    assert!(line(terminal.backend().buffer(), 9).starts_with("A"));
    assert_eq!(terminal.get_cursor_position().unwrap(), Position::new(0, 0));
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &[],
                &HashMap::new(),
                None,
                dashboard::ListView {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor,
                    top: &mut top,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: true,
                },
                false,
            )
        })
        .unwrap();
    assert_eq!(top, 4);
    assert!(line(terminal.backend().buffer(), 9).starts_with("E"));
    assert_eq!(
        terminal.get_cursor_position().unwrap(),
        Position::new(1, 14)
    );
}

#[test]
fn manual_offsets_clamp_after_resize() {
    let rows = panel::rows(&format!(
        "ID     STATUS       TASK\n{}",
        (1..=10)
            .map(|id| format!("{id}      New          Task {id}"))
            .collect::<Vec<_>>()
            .join("\n")
    ));
    let layout = render::Layout::new(&["A\nB\nC\nD\nE\nF\nG\nH\nI\nJ".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Editor",
        keys: render::KEYS,
        message: "",
    };
    let mut list_top = 6;
    let mut editor_top = 4;
    let mut terminal = Terminal::new(TestBackend::new(72, 24)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &rows,
                &HashMap::new(),
                Some(10),
                dashboard::ListView {
                    query: "",
                    focused: false,
                    top: &mut list_top,
                    follow_selected: false,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut editor_top,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: false,
                },
                false,
            )
        })
        .unwrap();
    assert_eq!(list_top, 2);
    assert_eq!(editor_top, 0);
    assert!(line(terminal.backend().buffer(), 2).contains("Task 2"));
    assert!(line(terminal.backend().buffer(), 13).starts_with("A"));
}

#[test]
fn filter_bar_shows_empty_result_and_takes_cursor_only_while_focused() {
    let layout = render::Layout::new(&["Unsaved".into()], &[], 72);
    let chrome = render::Chrome {
        title: "qqq task editor - new task",
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 16)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &[],
                &HashMap::new(),
                Some(99),
                dashboard::ListView {
                    query: "absent",
                    focused: true,
                    top: &mut 0,
                    follow_selected: true,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: true,
                },
                true,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(line(buffer, 1).starts_with("Filter: absent"));
    assert!(line(buffer, 2).starts_with("No matching tasks."));
    assert!(line(buffer, 8).starts_with("qqq task editor"));
    assert_eq!(
        terminal.get_cursor_position().unwrap(),
        Position::new(14, 1)
    );
}

#[test]
fn small_dashboard_keeps_filter_row_and_plain_style() {
    let layout = render::Layout::new(&["Draft".into()], &[], 12);
    let chrome = render::Chrome {
        title: "Editor",
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(12, 8)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &[],
                &HashMap::new(),
                None,
                dashboard::ListView {
                    query: "abcdefghijklmnop",
                    focused: true,
                    top: &mut 0,
                    follow_selected: true,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: true,
                },
                false,
            );
        })
        .unwrap();
    assert_eq!(terminal.get_cursor_position().unwrap().y, 1);
    let buffer = terminal.backend().buffer();
    assert!(line(buffer, 1).starts_with("Filter: "));
    for y in 0..8 {
        for x in 0..12 {
            assert_eq!(buffer[(x, y)].fg, Color::Reset);
            assert_eq!(buffer[(x, y)].bg, Color::Reset);
        }
    }
}

#[test]
fn minimum_dashboard_height_still_shows_selected_task() {
    let rows = panel::rows("ID     STATUS       TASK\n1      New          First");
    let layout = render::Layout::new(&["Draft".into()], &[], 12);
    let chrome = render::Chrome {
        title: "Editor",
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(12, 8)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &rows,
                &HashMap::from([(1, "new")]),
                Some(1),
                dashboard::ListView {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: true,
                },
                false,
            );
        })
        .unwrap();
    assert!(line(terminal.backend().buffer(), 2).starts_with("> 1"));
}

#[test]
fn split_dashboard_keeps_list_above_editor() {
    let rows = panel::rows("ID     STATUS       TASK\n1      New          First");
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "qqq task editor - task #1 (New)",
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 16)).unwrap();
    let mut list_top = 0;
    let mut editor_top = 0;
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &rows,
                &HashMap::from([(1, "new")]),
                Some(1),
                dashboard::ListView {
                    query: "",
                    focused: false,
                    top: &mut list_top,
                    follow_selected: true,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut editor_top,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: true,
                },
                false,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(line(buffer, 0).starts_with("qqq tasks"));
    assert!(line(buffer, 1).starts_with("Filter: "));
    assert!(line(buffer, 2).starts_with("  ID     STATUS"));
    assert!(line(buffer, 3).starts_with("> 1      New"));
    assert!(line(buffer, 8).starts_with("qqq task editor"));
    assert!(line(buffer, 9).starts_with("Draft"));
    assert!(line(buffer, 15).starts_with("Ctrl-S"));
    assert_eq!(terminal.get_cursor_position().unwrap(), Position::new(0, 9));
}

#[test]
fn narrow_dashboard_shows_plain_resize_hint() {
    let layout = render::Layout::new(&["Draft".into()], &[], 10);
    let chrome = render::Chrome {
        title: "qqq task editor",
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(10, 7)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &[],
                &HashMap::new(),
                None,
                dashboard::ListView {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: true,
                },
                true,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(line(buffer, 0).starts_with("Resize ter"));
    assert_eq!(buffer[(0, 0)].fg, Color::Reset);
    assert_eq!(buffer[(0, 0)].bg, Color::Reset);
}

#[test]
fn no_color_dashboard_keeps_default_cell_styles() {
    let rows = panel::rows("ID     STATUS       TASK\n1      Error        Failed");
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "qqq task editor",
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 16)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &rows,
                &HashMap::from([(1, "error")]),
                None,
                dashboard::ListView {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: true,
                },
                false,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    for y in 0..16 {
        for x in 0..72 {
            assert_eq!(buffer[(x, y)].fg, Color::Reset, "cell {x},{y}");
            assert_eq!(buffer[(x, y)].bg, Color::Reset, "cell {x},{y}");
        }
    }
}

#[test]
fn status_selection_and_editor_images_use_distinct_colors() {
    let rows = panel::rows(
        "ID     STATUS       TASK\n1      New          First\n2      In progress  Working\n3      Completed    Done\n4      Error        Failed",
    );
    let layout = render::Layout::new(
        &["A".into(), "[Image #1: sample.png]".into(), " tail".into()],
        &[false, true, false],
        72,
    );
    let chrome = render::Chrome {
        title: "qqq task editor - task #1 (New)",
        keys: render::KEYS,
        message: "Saved #1. New task",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 16)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &rows,
                &HashMap::from([
                    (1, "new"),
                    (2, "in_progress"),
                    (3, "completed"),
                    (4, "error"),
                ]),
                Some(1),
                dashboard::ListView {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: true,
                },
                true,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(2, 3)].bg, Color::Indexed(81));
    assert_eq!(buffer[(2, 4)].fg, Color::Indexed(81));
    assert_eq!(buffer[(2, 5)].fg, Color::DarkGray);
    assert_eq!(buffer[(2, 6)].fg, Color::Red);
    assert_eq!(buffer[(0, 9)].fg, Color::Indexed(252));
    assert_eq!(buffer[(0, 9)].bg, Color::Indexed(236));
    assert_eq!(buffer[(1, 9)].fg, Color::Indexed(81));
    assert_eq!(buffer[(0, 10)].bg, Color::Indexed(236));
    assert_eq!(buffer[(0, 15)].fg, Color::Yellow);
}

#[test]
fn dashboard_paste_label_uses_gold_while_body_stays_neutral() {
    let layout = render::Layout::with_paste(
        &[
            "A".into(),
            "[Pasted Content 1001 chars]".into(),
            " tail".into(),
        ],
        &[false, false, false],
        &[false, true, false],
        72,
    );
    let chrome = render::Chrome {
        title: "qqq task editor",
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 16)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &[],
                &HashMap::new(),
                None,
                dashboard::ListView {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
                    follow_cursor: true,
                },
                true,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 9)].fg, Color::Indexed(252));
    assert_eq!(buffer[(1, 9)].fg, Color::Indexed(222));
    assert_eq!(buffer[(28, 9)].fg, Color::Indexed(252));
}

#[test]
fn failed_save_footer_uses_error_color() {
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "qqq task editor",
        keys: render::KEYS,
        message: "Task description cannot be empty",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 16)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &[],
                &HashMap::new(),
                None,
                dashboard::ListView {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                },
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: true,
                    follow_cursor: true,
                },
                true,
            );
        })
        .unwrap();
    assert_eq!(terminal.backend().buffer()[(0, 15)].fg, Color::Red);
}
