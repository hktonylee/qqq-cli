#[path = "../src/tui/dashboard.rs"]
mod dashboard;
#[allow(dead_code)]
#[path = "../src/tui/panel.rs"]
mod panel;
#[allow(dead_code)]
#[path = "../src/tui/render.rs"]
mod render;

use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::Position,
    style::{Color, Modifier},
};
use std::collections::HashMap;

fn line(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn click(
    size: (u16, u16),
    column: u16,
    row: u16,
    rows: &[panel::ListRow],
    list_top: usize,
    editor_top: usize,
    layout: &render::Layout,
) -> Option<dashboard::ClickTarget> {
    dashboard::click_target(
        size,
        column,
        row,
        dashboard::HitState {
            query: "",
            rows,
            list_top,
            editor_top,
            layout,
        },
    )
}

#[test]
fn action_popup_preserves_background_and_clears_overlaid_styles() {
    let rows = panel::rows("ID STATUS TASK\n1 New Selected", 70);
    let layout = render::Layout::new(&["Draft behind popup".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Editor behind popup",
        title_status_color: None,
        keys: render::KEYS,
        message: "",
    };
    let modal = [
        "Task actions #1",
        "c Complete",
        "r Retry error",
        "o Reopen",
        "a Archive",
        "p Priority",
        "d Parent",
        "Esc cancel",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, text)| {
        render::PopupRow::new(
            text,
            match index {
                0 => render::PopupKind::Heading,
                7 => render::PopupKind::Hint,
                _ => render::PopupKind::Action,
            },
        )
    })
    .collect::<Vec<_>>();
    for color in [true, false] {
        let mut terminal = Terminal::new(TestBackend::new(72, 24)).unwrap();
        let mut background = None;
        for modal_lines in [None, Some(modal.as_slice())] {
            terminal
                .draw(|frame| {
                    dashboard::draw(
                        frame,
                        &rows,
                        &HashMap::from([(1, "new")]),
                        Some(1),
                        dashboard::View {
                            query: "",
                            focused: false,
                            top: &mut 0,
                            follow_selected: true,
                            modal_lines,
                            details: None,
                        },
                        render::DashboardEditor {
                            layout: &layout,
                            cursor: 0,
                            top: &mut 0,
                            chrome: &chrome,
                            message_is_error: false,
                            follow_cursor: true,
                        },
                        color,
                    );
                })
                .unwrap();
            if modal_lines.is_none() {
                background = Some(terminal.backend().buffer().clone());
            }
        }
        let buffer = terminal.backend().buffer();
        let background = background.unwrap();
        assert_eq!(buffer[(12, 7)].symbol(), "┌");
        assert_eq!(buffer[(59, 16)].symbol(), "┘");
        assert!(line(buffer, 8).contains("│Task actions #1"));
        for (x, y, foreground, modifier) in [
            (13, 8, Color::Indexed(81), Modifier::BOLD),
            (13, 9, Color::Indexed(81), Modifier::BOLD),
            (15, 9, Color::Indexed(252), Modifier::empty()),
            (13, 15, Color::Indexed(81), Modifier::BOLD),
            (17, 15, Color::Gray, Modifier::empty()),
            (12, 7, Color::DarkGray, Modifier::empty()),
        ] {
            assert_eq!(
                buffer[(x, y)].fg,
                if color { foreground } else { Color::Reset }
            );
            assert_eq!(
                buffer[(x, y)].modifier,
                if color { modifier } else { Modifier::empty() }
            );
        }
        assert_eq!(buffer[(55, 13)].symbol(), " ");
        assert_eq!(buffer[(55, 13)].modifier, Modifier::empty());
        assert_eq!(
            buffer[(55, 13)].fg,
            if color {
                Color::Indexed(252)
            } else {
                Color::Reset
            }
        );
        assert_eq!(
            buffer[(55, 13)].bg,
            if color {
                Color::Indexed(236)
            } else {
                Color::Reset
            }
        );
        for y in 0..24 {
            for x in 0..72 {
                if !(12..60).contains(&x) || !(7..17).contains(&y) {
                    assert_eq!(buffer[(x, y)], background[(x, y)], "background {x},{y}");
                }
            }
        }
        assert_eq!(
            terminal.get_cursor_position().unwrap(),
            Position::new(13, 8)
        );
    }
}

#[test]
fn action_popup_input_errors_and_warnings_keep_roles_and_plain_styles() {
    use render::{PopupKind, PopupRow};
    let cases = [
        vec![
            PopupRow::new("Priority task #1", PopupKind::Heading),
            PopupRow::new("Enter -100..100", PopupKind::Hint),
            PopupRow::new("> -9", PopupKind::Input),
            PopupRow::new("Priority must be -100..100", PopupKind::Error),
            PopupRow::new("Enter apply  Esc cancel", PopupKind::Hint),
        ],
        vec![
            PopupRow::new("Action error", PopupKind::Error),
            PopupRow::new("> quoted error", PopupKind::Error),
            PopupRow::new("Up/Down Esc", PopupKind::Hint),
        ],
        vec![
            PopupRow::new("Archive task #1?", PopupKind::Heading),
            PopupRow::new("Lose draft?", PopupKind::Warning),
            PopupRow::new("y confirm  n/Esc cancel", PopupKind::Hint),
        ],
    ];
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Editor",
        title_status_color: None,
        keys: render::KEYS,
        message: "",
    };
    for color in [true, false] {
        for modal in &cases {
            let mut terminal = Terminal::new(TestBackend::new(72, 24)).unwrap();
            terminal
                .draw(|frame| {
                    dashboard::draw(
                        frame,
                        &[],
                        &HashMap::new(),
                        None,
                        dashboard::View {
                            query: "",
                            focused: false,
                            top: &mut 0,
                            follow_selected: true,
                            modal_lines: Some(modal),
                            details: None,
                        },
                        render::DashboardEditor {
                            layout: &layout,
                            cursor: 0,
                            top: &mut 0,
                            chrome: &chrome,
                            message_is_error: false,
                            follow_cursor: true,
                        },
                        color,
                    )
                })
                .unwrap();
            let popup =
                dashboard::popup_layout(ratatui::layout::Rect::new(0, 0, 72, 24), modal.len());
            let content = popup.content;
            let buffer = terminal.backend().buffer();
            let mut cursor = Position::new(content.x, content.y);
            for (index, row) in modal.iter().enumerate() {
                let y = content.y + index as u16;
                assert!(line(buffer, y).contains(&row.text));
                let (foreground, bold) = match row.kind {
                    PopupKind::Heading | PopupKind::Action | PopupKind::Hint => {
                        (Color::Indexed(81), true)
                    }
                    PopupKind::Input | PopupKind::Warning => (Color::Indexed(222), false),
                    PopupKind::Error => (Color::Indexed(210), index == 0),
                };
                assert_eq!(
                    buffer[(content.x, y)].fg,
                    if color { foreground } else { Color::Reset }
                );
                assert_eq!(
                    buffer[(content.x, y)].modifier,
                    if color && bold {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }
                );
                if row.kind == PopupKind::Input {
                    assert_eq!(
                        buffer[(content.x + 2, y)].fg,
                        if color {
                            Color::Indexed(252)
                        } else {
                            Color::Reset
                        }
                    );
                    cursor = Position::new(content.x + row.text.len() as u16, y);
                }
            }
            if !color {
                for y in popup.outer.y..popup.outer.bottom() {
                    for x in popup.outer.x..popup.outer.right() {
                        assert_eq!(buffer[(x, y)].fg, Color::Reset);
                        assert_eq!(buffer[(x, y)].bg, Color::Reset);
                        assert_eq!(buffer[(x, y)].modifier, Modifier::empty());
                    }
                }
            }
            assert_eq!(terminal.get_cursor_position().unwrap(), cursor);
        }
    }
}

#[test]
fn blank_draft_keeps_details_pane_and_editor_position() {
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Editor",
        title_status_color: None,
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 24)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &[],
                &HashMap::new(),
                None,
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                    modal_lines: None,
                    details: None,
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
    let buffer = terminal.backend().buffer();
    assert!(line(buffer, 9).starts_with("║  Select task to view details."));
    assert_eq!(line(buffer, 7), " ".repeat(72));
    assert_eq!(line(buffer, 8), format!("╔{}╗", "═".repeat(70)));
    assert_eq!(line(buffer, 12), format!("╚{}╝", "═".repeat(70)));
    for y in 9..12 {
        assert_eq!(buffer[(0, y)].symbol(), "║");
        assert_eq!(buffer[(71, y)].symbol(), "║");
        for x in [1, 2, 69, 70] {
            assert_eq!(buffer[(x, y)].symbol(), " ");
        }
    }
    assert!(line(buffer, 13).starts_with("Editor"));
    assert!(line(buffer, 14).starts_with("Draft"));
    assert_eq!(
        terminal.get_cursor_position().unwrap(),
        Position::new(0, 14)
    );
}

#[test]
fn selected_task_renders_details_between_list_and_editor() {
    let rows = panel::rows("ID STATUS TASK\n1 New Selected", 70);
    let expected = [
        (
            "Task #1 | Error | Priority 8",
            render::DetailKind::Body,
            Color::Reset,
            Modifier::empty(),
        ),
        (
            "#1 reviewer",
            render::DetailKind::MessageHeader,
            Color::Reset,
            Modifier::BOLD,
        ),
        (
            "Created: Latest message",
            render::DetailKind::Body,
            Color::Reset,
            Modifier::empty(),
        ),
        (
            "Created: now",
            render::DetailKind::Muted,
            Color::Reset,
            Modifier::DIM,
        ),
        (
            "Task #1 unavailable.",
            render::DetailKind::Warning,
            Color::Yellow,
            Modifier::empty(),
        ),
        (
            "In progress",
            render::DetailKind::InProgress,
            Color::Cyan,
            Modifier::empty(),
        ),
        (
            "Completed",
            render::DetailKind::Completed,
            Color::DarkGray,
            Modifier::empty(),
        ),
    ];
    let detail_rows: Vec<_> = expected
        .iter()
        .enumerate()
        .map(|(index, (text, kind, _, _))| {
            if index == 0 {
                render::DetailRow::styled([
                    ("Task ".into(), render::DetailKind::Body),
                    ("#1".into(), render::DetailKind::Heading),
                    (" | ".into(), render::DetailKind::Body),
                    ("Error".into(), render::DetailKind::Error),
                    (" | Priority ".into(), render::DetailKind::Muted),
                    ("8".into(), render::DetailKind::Body),
                ])
            } else {
                render::DetailRow::new(*text, *kind)
            }
        })
        .collect();
    let layout = render::Layout::new(&["Dirty draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Editor",
        title_status_color: None,
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 24)).unwrap();
    for color in [true, false] {
        for requested_top in 0..=4 {
            let mut details_top = requested_top;
            terminal
                .draw(|frame| {
                    dashboard::draw(
                        frame,
                        &rows,
                        &HashMap::from([(1, "new")]),
                        Some(1),
                        dashboard::View {
                            query: "",
                            focused: false,
                            top: &mut 0,
                            follow_selected: true,
                            modal_lines: None,
                            details: Some(dashboard::DetailsView {
                                rows: &detail_rows,
                                top: &mut details_top,
                            }),
                        },
                        render::DashboardEditor {
                            layout: &layout,
                            cursor: 0,
                            top: &mut 0,
                            chrome: &chrome,
                            message_is_error: false,
                            follow_cursor: true,
                        },
                        color,
                    )
                })
                .unwrap();
            assert_eq!(details_top, requested_top);
            let buffer = terminal.backend().buffer();
            assert!(line(buffer, 0).contains("Selected"));
            for (offset, (text, _, foreground, modifier)) in
                expected.iter().skip(details_top).take(3).enumerate()
            {
                let y = 9 + offset as u16;
                assert!(line(buffer, y).starts_with(&format!("║  {text}")));
                for x in 3..69 {
                    let (foreground, modifier) = if *text == expected[0].0 {
                        match x {
                            8..10 => (Color::Reset, Modifier::BOLD),
                            13..18 => (Color::Red, Modifier::empty()),
                            18..30 => (Color::Reset, Modifier::DIM),
                            _ => (Color::Reset, Modifier::empty()),
                        }
                    } else {
                        (*foreground, *modifier)
                    };
                    assert_eq!(
                        buffer[(x, y)].fg,
                        if color { foreground } else { Color::Reset },
                        "cell {x},{y}, top {details_top}"
                    );
                    assert_eq!(buffer[(x, y)].bg, Color::Reset);
                    assert_eq!(
                        buffer[(x, y)].modifier,
                        if color { modifier } else { Modifier::empty() },
                        "cell {x},{y}, top {details_top}"
                    );
                }
            }
            assert_eq!(line(buffer, 7), " ".repeat(72));
            assert_eq!(line(buffer, 8), format!("╔{}╗", "═".repeat(70)));
            assert_eq!(line(buffer, 12), format!("╚{}╝", "═".repeat(70)));
            for y in 9..12 {
                for x in [0, 71] {
                    assert_eq!(buffer[(x, y)].symbol(), "║");
                    assert_eq!(
                        buffer[(x, y)].fg,
                        if color { Color::DarkGray } else { Color::Reset }
                    );
                    assert_eq!(buffer[(x, y)].modifier, Modifier::empty());
                }
                for x in [1, 2, 69, 70] {
                    assert_eq!(buffer[(x, y)].symbol(), " ");
                    assert_eq!(buffer[(x, y)].fg, Color::Reset);
                    assert_eq!(buffer[(x, y)].modifier, Modifier::empty());
                }
            }
            assert!(line(buffer, 13).starts_with("Editor"));
            assert!(line(buffer, 14).starts_with("Dirty draft"));
            assert_eq!(
                terminal.get_cursor_position().unwrap(),
                Position::new(0, 14)
            );
        }
    }
}

#[test]
fn details_scroll_clamps_without_moving_editor() {
    let detail_rows: Vec<_> = (0..10)
        .map(|index| render::DetailRow::new(format!("Detail {index}"), render::DetailKind::Body))
        .collect();
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Editor",
        title_status_color: None,
        keys: render::KEYS,
        message: "",
    };
    let mut details_top = 99;
    let mut editor_top = 0;
    let mut terminal = Terminal::new(TestBackend::new(72, 18)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &[],
                &HashMap::new(),
                Some(1),
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                    modal_lines: None,
                    details: Some(dashboard::DetailsView {
                        rows: &detail_rows,
                        top: &mut details_top,
                    }),
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
            )
        })
        .unwrap();
    assert_eq!(details_top, 8);
    assert_eq!(editor_top, 0);
    assert!(line(terminal.backend().buffer(), 7).starts_with("║  Detail 8"));
    assert!(line(terminal.backend().buffer(), 11).starts_with("Draft"));
}

#[test]
fn selected_task_geometry_and_details_hit_test_share_rectangles() {
    use ratatui::layout::Rect;
    for height in 8..=100 {
        let panes = dashboard::panes(Rect::new(0, 0, 72, height));
        let details = panes.details;
        let content = dashboard::details_content(details);
        assert!(panes.list.height >= 4);
        assert!(details.height >= 1);
        assert!(panes.editor.height >= 3);
        assert_eq!(
            panes.list.height + details.height + panes.editor.height,
            height
        );
        for (screen, list_height, details_height) in
            [(8, 4, 1), (18, 6, 4), (24, 8, 5), (100, 35, 20)]
        {
            if height == screen {
                assert_eq!(panes.list.height, list_height);
                assert_eq!(details.height, details_height);
            }
        }
        assert_eq!(
            dashboard::wheel_area((72, height), 5, content.y, ""),
            Some(dashboard::WheelArea::Details(dashboard::details_height(
                details
            )))
        );
        assert!(content.height >= 1);
        assert_eq!(content.width, if details.height >= 3 { 66 } else { 68 });
        assert_eq!(dashboard::wheel_area((72, height), 1, content.y, ""), None);
        if details.height >= 3 {
            assert_eq!(dashboard::wheel_area((72, height), 5, details.y, ""), None);
        }
        let fragments: Vec<_> = "Editable".chars().map(|ch| ch.to_string()).collect();
        let layout = render::Layout::new(&fragments, &[], 72);
        assert_eq!(
            dashboard::click_target(
                (72, height),
                5,
                details.y,
                dashboard::HitState {
                    query: "",
                    rows: &[],
                    list_top: 0,
                    editor_top: 0,
                    layout: &layout
                }
            ),
            None
        );
        assert_eq!(
            dashboard::click_target(
                (72, height),
                2,
                panes.editor.y + 1,
                dashboard::HitState {
                    query: "",
                    rows: &[],
                    list_top: 0,
                    editor_top: 0,
                    layout: &layout
                }
            ),
            Some(dashboard::ClickTarget::Editor(2))
        );
    }
}

#[test]
fn new_draft_hit_test_keeps_three_panes() {
    assert_eq!(
        dashboard::wheel_area((72, 18), 5, 3, ""),
        Some(dashboard::WheelArea::List(6))
    );
    assert_eq!(
        dashboard::wheel_area((72, 18), 5, 7, ""),
        Some(dashboard::WheelArea::Details(2))
    );
    assert_eq!(
        dashboard::wheel_area((72, 18), 5, 11, ""),
        Some(dashboard::WheelArea::Editor(6))
    );
}

#[test]
fn wheel_hit_test_uses_list_editor_and_excludes_edges() {
    assert_eq!(
        dashboard::wheel_area((72, 24), 5, 3, ""),
        Some(dashboard::WheelArea::List(8))
    );
    assert_eq!(
        dashboard::wheel_area((72, 24), 5, 1, ""),
        Some(dashboard::WheelArea::List(8))
    );
    assert_eq!(
        dashboard::wheel_area((72, 24), 5, 7, ""),
        Some(dashboard::WheelArea::List(8))
    );
    assert_eq!(
        dashboard::wheel_area((72, 24), 5, 10, ""),
        Some(dashboard::WheelArea::Details(3))
    );
    assert_eq!(dashboard::wheel_area((72, 24), 5, 8, ""), None);
    assert_eq!(dashboard::wheel_area((72, 24), 0, 10, ""), None);
    assert_eq!(dashboard::wheel_area((72, 24), 2, 10, ""), None);
    assert_eq!(dashboard::wheel_area((72, 24), 69, 10, ""), None);
    assert_eq!(dashboard::wheel_area((72, 24), 5, 12, ""), None);
    assert_eq!(
        dashboard::wheel_area((72, 24), 5, 14, ""),
        Some(dashboard::WheelArea::Editor(9))
    );
    assert_eq!(dashboard::wheel_area((72, 24), 5, 23, ""), None);
    assert_eq!(dashboard::wheel_area((72, 24), 72, 3, ""), None);
    assert_eq!(dashboard::wheel_area((10, 7), 5, 3, ""), None);
}

#[test]
fn click_target_maps_rendered_task_rows_and_editor_caret() {
    let rows = panel::rows(
        "ID     STATUS       TASK\n1      New          Parent\n                    detail\n2      New          └── Child",
        70,
    );
    let layout = render::Layout::new(
        &["a", "b", "c", "\n", "d", "e", "f"].map(str::to_owned),
        &[],
        72,
    );
    let hit = |column, row, list_top, editor_top| {
        click((72, 24), column, row, &rows, list_top, editor_top, &layout)
    };
    assert_eq!(hit(5, 0, 0, 0), Some(dashboard::ClickTarget::Task(1)));
    assert_eq!(hit(5, 1, 0, 0), Some(dashboard::ClickTarget::Task(1)));
    assert_eq!(hit(70, 2, 0, 0), Some(dashboard::ClickTarget::Task(2)));
    assert_eq!(hit(5, 0, 1, 0), Some(dashboard::ClickTarget::Task(1)));
    assert_eq!(hit(5, 1, 1, 0), Some(dashboard::ClickTarget::Task(2)));
    assert_eq!(hit(2, 14, 0, 0), Some(dashboard::ClickTarget::Editor(2)));
    assert_eq!(hit(10, 14, 0, 0), Some(dashboard::ClickTarget::Editor(3)));
    assert_eq!(hit(1, 14, 0, 1), Some(dashboard::ClickTarget::Editor(5)));
}

#[test]
fn click_target_ignores_non_content_and_out_of_bounds() {
    let rows = panel::rows("ID     STATUS       TASK\n1      New          First", 70);
    let layout = render::Layout::new(&["abc".into()], &[], 72);
    for (column, row) in [
        (5, 1),
        (5, 2),
        (5, 3),
        (5, 4),
        (5, 7),
        (5, 8),
        (5, 10),
        (5, 15),
        (72, 3),
    ] {
        assert_eq!(
            click((72, 24), column, row, &rows, 0, 0, &layout),
            None,
            "{column},{row}"
        );
    }
    assert_eq!(click((10, 7), 5, 3, &rows, 0, 0, &layout), None);
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
    let rows = panel::rows(&tree, 70);
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Editor",
        title_status_color: None,
        keys: render::KEYS,
        message: "",
    };
    let mut top = 0;
    let mut terminal = Terminal::new(TestBackend::new(72, 24)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &rows,
                &HashMap::new(),
                Some(10),
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut top,
                    follow_selected: false,
                    modal_lines: None,
                    details: None,
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
    assert!(line(terminal.backend().buffer(), 0).contains("Task 1"));
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &rows,
                &HashMap::new(),
                Some(10),
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut top,
                    follow_selected: true,
                    modal_lines: None,
                    details: None,
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
    assert_eq!(top, 2);
    assert!(line(terminal.backend().buffer(), 7).contains("Task 10"));
    assert_eq!(
        click((72, 24), 5, 7, &rows, top, 0, &layout),
        Some(dashboard::ClickTarget::Task(10))
    );
    assert_eq!(
        dashboard::wheel_area((72, 24), 5, 7, ""),
        Some(dashboard::WheelArea::List(8))
    );
    assert_eq!(
        line(terminal.backend().buffer(), 8),
        format!("╔{}╗", "═".repeat(70))
    );
    assert!(line(terminal.backend().buffer(), 13).starts_with("Editor"));
}

#[test]
fn manual_editor_scroll_keeps_viewport_then_keyboard_reveals_caret() {
    let layout = render::Layout::new(&["A\nB\nC\nD\nE\nF\nG\nH\nI\nJ".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Editor",
        title_status_color: None,
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
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                    modal_lines: None,
                    details: None,
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
    assert!(line(terminal.backend().buffer(), 10).starts_with("A"));
    assert_eq!(terminal.get_cursor_position().unwrap(), Position::new(0, 0));
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &[],
                &HashMap::new(),
                None,
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                    modal_lines: None,
                    details: None,
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
    assert_eq!(top, 5);
    assert!(line(terminal.backend().buffer(), 10).starts_with("F"));
    assert_eq!(
        terminal.get_cursor_position().unwrap(),
        Position::new(1, 14)
    );
}

#[test]
fn manual_offsets_clamp_after_resize() {
    let rows = panel::rows(
        &format!(
            "ID     STATUS       TASK\n{}",
            (1..=10)
                .map(|id| format!("{id}      New          Task {id}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        70,
    );
    let layout = render::Layout::new(&["A\nB\nC\nD\nE\nF\nG\nH\nI\nJ".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Editor",
        title_status_color: None,
        keys: render::KEYS,
        message: "",
    };
    let mut list_top = 6;
    let mut editor_top = 4;
    let mut terminal = Terminal::new(TestBackend::new(72, 36)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &rows,
                &HashMap::new(),
                Some(10),
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut list_top,
                    follow_selected: false,
                    modal_lines: None,
                    details: None,
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
    assert_eq!(list_top, 0);
    assert_eq!(editor_top, 0);
    assert!(line(terminal.backend().buffer(), 0).contains("Task 1"));
    assert!(line(terminal.backend().buffer(), 21).starts_with("A"));
}

#[test]
fn list_mouse_geometry_tracks_filter_visibility() {
    let rows = panel::rows("1 New First\n2 New Second", 70);
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    for query in ["", "first"] {
        let hit = |row| {
            dashboard::click_target(
                (72, 24),
                5,
                row,
                dashboard::HitState {
                    rows: &rows,
                    query,
                    list_top: 0,
                    editor_top: 0,
                    layout: &layout,
                },
            )
        };
        assert_eq!(
            hit(0),
            query.is_empty().then_some(dashboard::ClickTarget::Task(1))
        );
        assert_eq!(
            hit(1),
            Some(dashboard::ClickTarget::Task(if query.is_empty() {
                2
            } else {
                1
            }))
        );
        assert_eq!(
            dashboard::wheel_area((72, 24), 5, 0, query),
            Some(dashboard::WheelArea::List(if query.is_empty() {
                8
            } else {
                7
            }))
        );
    }
}

#[test]
fn empty_filter_reclaims_list_row_regardless_of_focus() {
    let rows = panel::rows(
        &(1..=8)
            .map(|id| format!("{id} New Task {id}"))
            .collect::<Vec<_>>()
            .join("\n"),
        70,
    );
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Editor",
        title_status_color: None,
        keys: render::KEYS,
        message: "",
    };
    for focused in [false, true] {
        let mut terminal = Terminal::new(TestBackend::new(72, 24)).unwrap();
        let mut top = 0;
        terminal
            .draw(|frame| {
                dashboard::draw(
                    frame,
                    &rows,
                    &HashMap::new(),
                    Some(8),
                    dashboard::View {
                        query: "",
                        focused,
                        top: &mut top,
                        follow_selected: true,
                        modal_lines: None,
                        details: None,
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
        assert_eq!(top, 0);
        assert!(line(terminal.backend().buffer(), 0).contains("Task 1"));
        assert!(line(terminal.backend().buffer(), 7).contains("Task 8"));
    }
}

#[test]
fn filter_bar_shows_empty_result_and_takes_cursor_only_while_focused() {
    let layout = render::Layout::new(&["Unsaved".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Task Editor - new task",
        title_status_color: None,
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 24)).unwrap();
    for color in [true, false] {
        for focused in [true, false] {
            for query in ["", "absent", ""] {
                terminal
                    .draw(|frame| {
                        dashboard::draw(
                            frame,
                            &[],
                            &HashMap::new(),
                            Some(99),
                            dashboard::View {
                                query,
                                focused,
                                top: &mut 0,
                                follow_selected: true,
                                modal_lines: None,
                                details: None,
                            },
                            render::DashboardEditor {
                                layout: &layout,
                                cursor: 0,
                                top: &mut 0,
                                chrome: &chrome,
                                message_is_error: false,
                                follow_cursor: true,
                            },
                            color,
                        );
                    })
                    .unwrap();
                let buffer = terminal.backend().buffer();
                let editable_start = if query.is_empty() {
                    assert!(line(buffer, 0).starts_with("No matching tasks."));
                    72
                } else {
                    assert!(line(buffer, 0).starts_with("Filter: absent"));
                    for x in 0..8 {
                        assert_eq!(buffer[(x, 0)].bg, Color::Reset);
                        assert_eq!(
                            buffer[(x, 0)].fg,
                            if !color {
                                Color::Reset
                            } else if focused {
                                Color::Indexed(81)
                            } else {
                                Color::Gray
                            }
                        );
                        assert_eq!(
                            buffer[(x, 0)].modifier,
                            if color && focused {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }
                        );
                    }
                    8
                };
                for x in editable_start..72 {
                    assert_eq!(
                        buffer[(x, 0)].bg,
                        buffer[(x, 14)].bg,
                        "query {query:?}, focused {focused}, color {color}, x {x}"
                    );
                    assert_eq!(
                        buffer[(x, 0)].bg,
                        if color {
                            Color::Indexed(236)
                        } else {
                            Color::Reset
                        }
                    );
                    assert_eq!(
                        buffer[(x, 0)].fg,
                        if color {
                            Color::Indexed(252)
                        } else {
                            Color::Reset
                        }
                    );
                    assert_eq!(buffer[(x, 0)].modifier, Modifier::empty());
                }
                assert!(
                    line(buffer, u16::from(!query.is_empty())).starts_with("No matching tasks.")
                );
                assert!(line(buffer, 13).starts_with("Task Editor"));
                if !focused || !query.is_empty() {
                    assert_eq!(
                        terminal.get_cursor_position().unwrap(),
                        if focused {
                            Position::new(14, 0)
                        } else {
                            Position::new(0, 14)
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn small_dashboard_keeps_filter_row_and_plain_style() {
    let layout = render::Layout::new(&["Draft".into()], &[], 12);
    let chrome = render::Chrome {
        title: "Editor",
        title_status_color: None,
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
                dashboard::View {
                    query: "abcdefghijklmnop",
                    focused: true,
                    top: &mut 0,
                    follow_selected: true,
                    modal_lines: None,
                    details: None,
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
    assert_eq!(terminal.get_cursor_position().unwrap().y, 0);
    let buffer = terminal.backend().buffer();
    assert!(line(buffer, 0).starts_with("Filter: "));
    for y in 0..8 {
        for x in 0..12 {
            assert_eq!(buffer[(x, y)].fg, Color::Reset);
            assert_eq!(buffer[(x, y)].bg, Color::Reset);
        }
    }
}

#[test]
fn minimum_dashboard_height_still_shows_selected_task() {
    let rows = panel::rows("ID     STATUS       TASK\n1      New          First", 70);
    let layout = render::Layout::new(&["Draft".into()], &[], 12);
    let chrome = render::Chrome {
        title: "Editor",
        title_status_color: None,
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
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                    modal_lines: None,
                    details: None,
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
    assert!(line(terminal.backend().buffer(), 0).starts_with("> 1"));
}

#[test]
fn split_dashboard_keeps_list_above_editor() {
    let rows = panel::rows(
        "ID     STATUS       TASK\n1      New          First\n2      New          Second\n3      New          Third\n4      New          Fourth\n5      New          Fifth",
        70,
    );
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Task Editor - task #1 (New)",
        title_status_color: None,
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 24)).unwrap();
    let mut list_top = 0;
    let mut editor_top = 0;
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &rows,
                &HashMap::from([(1, "new")]),
                Some(1),
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut list_top,
                    follow_selected: true,
                    modal_lines: None,
                    details: None,
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
    assert!(line(buffer, 0).starts_with("> 1      New"));
    assert!(line(buffer, 1).contains("Second"));
    assert!(!line(buffer, 0).contains("qqq tasks"));
    assert!(line(buffer, 4).contains("Fifth"));
    assert!(line(buffer, 5).trim().is_empty());
    assert!(!(0..8).any(|y| line(buffer, y).contains("ID     STATUS")));
    assert!(line(buffer, 13).starts_with("Task Editor"));
    assert!(line(buffer, 14).starts_with("Draft"));
    assert!(line(buffer, 23).starts_with("Ctrl-S"));
    assert_eq!(
        terminal.get_cursor_position().unwrap(),
        Position::new(0, 14)
    );
}

#[test]
fn narrow_dashboard_shows_plain_resize_hint() {
    let layout = render::Layout::new(&["Draft".into()], &[], 10);
    let chrome = render::Chrome {
        title: "Task Editor",
        title_status_color: None,
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
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                    modal_lines: None,
                    details: None,
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
    let rows = panel::rows("ID     STATUS       TASK\n1      Error        Failed", 70);
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Task Editor",
        title_status_color: None,
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 24)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &rows,
                &HashMap::from([(1, "error")]),
                Some(1),
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                    modal_lines: None,
                    details: None,
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
    for y in 0..24 {
        for x in 0..72 {
            assert_eq!(buffer[(x, y)].fg, Color::Reset, "cell {x},{y}");
            assert_eq!(buffer[(x, y)].bg, Color::Reset, "cell {x},{y}");
            assert_eq!(buffer[(x, y)].modifier, Modifier::empty(), "cell {x},{y}");
        }
    }
}

#[test]
fn status_selection_and_editor_images_use_distinct_colors() {
    let rows = panel::rows(
        "ID     STATUS       TASK\n1      New          First\n                    Second line\n                    Third line\n2      In progress  Working\n3      Completed    Done\n4      Error        Failed",
        70,
    );
    let layout = render::Layout::new(
        &["A".into(), "[Image #1: sample.png]".into(), " tail".into()],
        &[false, true, false],
        72,
    );
    let chrome = render::Chrome {
        title: "Task Editor - task #1 (New)",
        title_status_color: None,
        keys: render::KEYS,
        message: "Saved #1. New task",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 30)).unwrap();
    for selected in [Some(1), Some(2), Some(3), Some(4), None] {
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
                    selected,
                    dashboard::View {
                        query: "",
                        focused: false,
                        top: &mut 0,
                        follow_selected: true,
                        modal_lines: None,
                        details: None,
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
        for (y, id, foreground) in [
            (0, 1, Color::Reset),
            (1, 1, Color::Reset),
            (2, 1, Color::Reset),
            (3, 2, Color::Indexed(81)),
            (4, 3, Color::DarkGray),
            (5, 4, Color::Red),
        ] {
            let is_selected = selected == Some(id);
            for x in 0..72 {
                assert_eq!(
                    buffer[(x, y)].bg,
                    if is_selected {
                        Color::Rgb(15, 51, 62)
                    } else {
                        Color::Reset
                    },
                    "selection {selected:?}, cell {x},{y}"
                );
                assert_eq!(buffer[(x, y)].modifier, Modifier::empty());
            }
            assert_eq!(
                buffer[(2, y)].fg,
                if is_selected {
                    Color::Indexed(252)
                } else {
                    foreground
                }
            );
        }
        for y in [6, 9] {
            assert_eq!(buffer[(71, y)].bg, Color::Reset);
        }
        let body_y = 18;
        assert_eq!(buffer[(0, body_y)].fg, Color::Indexed(252));
        assert_eq!(buffer[(0, body_y)].bg, Color::Indexed(236));
        assert_eq!(buffer[(1, body_y)].fg, Color::Indexed(81));
        assert_eq!(buffer[(0, body_y + 1)].bg, Color::Indexed(236));
        assert_eq!(buffer[(0, 29)].fg, Color::Yellow);
    }
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
        title: "Task Editor",
        title_status_color: None,
        keys: render::KEYS,
        message: "",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 24)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &[],
                &HashMap::new(),
                None,
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                    modal_lines: None,
                    details: None,
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
    assert_eq!(buffer[(0, 14)].fg, Color::Indexed(252));
    assert_eq!(buffer[(1, 14)].fg, Color::Indexed(222));
    assert_eq!(buffer[(28, 14)].fg, Color::Indexed(252));
}

#[test]
fn hotkey_footer_colors_shortcuts_and_clears_styles_for_messages() {
    for width in [12, 17, 72, 79, 96] {
        let layout = render::Layout::new(&["Draft".into()], &[], width.into());
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        for color in [true, false] {
            for (keys, message, error) in [
                (render::DASHBOARD_KEYS, "", false),
                (render::DASHBOARD_KEYS, "Ctrl-S save failed", true),
                (render::DASHBOARD_KEYS, "Saved #1. New task", false),
                (render::FILTER_KEYS, "", false),
                (render::KEYS, "", false),
                (render::NAV_KEYS, "", false),
                (render::ADD_KEYS, "", false),
            ] {
                let chrome = render::Chrome {
                    title: "Task Editor",
                    title_status_color: None,
                    keys,
                    message,
                };
                terminal
                    .draw(|frame| {
                        dashboard::draw(
                            frame,
                            &[],
                            &HashMap::new(),
                            None,
                            dashboard::View {
                                query: "",
                                focused: false,
                                top: &mut 0,
                                follow_selected: true,
                                modal_lines: None,
                                details: None,
                            },
                            render::DashboardEditor {
                                layout: &layout,
                                cursor: 0,
                                top: &mut 0,
                                chrome: &chrome,
                                message_is_error: error,
                                follow_cursor: true,
                            },
                            color,
                        );
                    })
                    .unwrap();
                let buffer = terminal.backend().buffer();
                let text = if message.is_empty() { keys } else { message };
                let clipped = render::clipped(text, width.into());
                assert_eq!(line(buffer, 23).trim_end(), clipped.trim_end());
                for (x, _) in clipped.chars().enumerate() {
                    let shortcut = message.is_empty()
                        && [
                            "Ctrl-S",
                            "Ctrl-P",
                            "Ctrl-G",
                            "Shift-Up/Dn",
                            "Ctrl+/",
                            "Backspace",
                            "Esc",
                            "Esc/Ctrl-C",
                            "Tab/Enter",
                            "Ctrl-V",
                        ]
                        .iter()
                        .any(|key| {
                            keys.find(key)
                                .is_some_and(|start| (start..start + key.len()).contains(&x))
                        });
                    let foreground = if !color {
                        Color::Reset
                    } else if !message.is_empty() {
                        if error { Color::Red } else { Color::Yellow }
                    } else if shortcut {
                        Color::Indexed(81)
                    } else {
                        Color::Gray
                    };
                    let cell = &buffer[(x as u16, 23)];
                    assert_eq!(cell.fg, foreground, "{text:?} column {x} width {width}");
                    assert_eq!(cell.bg, Color::Reset);
                    assert_eq!(
                        cell.modifier,
                        if color && shortcut {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        },
                        "{text:?} column {x} width {width}",
                    );
                }
            }
        }
    }
}

#[test]
fn failed_save_footer_uses_error_color() {
    let layout = render::Layout::new(&["Draft".into()], &[], 72);
    let chrome = render::Chrome {
        title: "Task Editor",
        title_status_color: None,
        keys: render::KEYS,
        message: "Task description cannot be empty",
    };
    let mut terminal = Terminal::new(TestBackend::new(72, 24)).unwrap();
    terminal
        .draw(|frame| {
            dashboard::draw(
                frame,
                &[],
                &HashMap::new(),
                None,
                dashboard::View {
                    query: "",
                    focused: false,
                    top: &mut 0,
                    follow_selected: true,
                    modal_lines: None,
                    details: None,
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
    assert_eq!(terminal.backend().buffer()[(0, 23)].fg, Color::Red);
}
