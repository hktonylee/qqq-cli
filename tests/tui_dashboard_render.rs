#[path = "../src/tui/dashboard.rs"]
mod dashboard;
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
                &mut list_top,
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut editor_top,
                    chrome: &chrome,
                    message_is_error: false,
                },
                false,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(line(buffer, 0).starts_with("qqq tasks"));
    assert!(line(buffer, 1).starts_with("  ID     STATUS"));
    assert!(line(buffer, 2).starts_with("> 1      New"));
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
                &mut 0,
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
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
                &mut 0,
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
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
                &mut 0,
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
                },
                true,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(2, 2)].bg, Color::Indexed(81));
    assert_eq!(buffer[(2, 3)].fg, Color::Indexed(81));
    assert_eq!(buffer[(2, 4)].fg, Color::DarkGray);
    assert_eq!(buffer[(2, 5)].fg, Color::Red);
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
                &mut 0,
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: false,
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
                &mut 0,
                render::DashboardEditor {
                    layout: &layout,
                    cursor: 0,
                    top: &mut 0,
                    chrome: &chrome,
                    message_is_error: true,
                },
                true,
            );
        })
        .unwrap();
    assert_eq!(terminal.backend().buffer()[(0, 15)].fg, Color::Red);
}
