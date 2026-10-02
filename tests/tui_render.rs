#[allow(dead_code)]
#[path = "../src/tui/render.rs"]
mod render;

#[test]
fn editor_paints_only_body_rows_and_resets_colors_for_header_and_footer() {
    crossterm::style::force_color_output(true);
    let layout = render::Layout::new(&["Body".into()], 40);
    for message in [
        "",
        "Task description cannot be empty",
        "Discard draft? (y/N)",
    ] {
        let mut output = Vec::new();
        render::draw(&mut output, &layout, 0, &mut 0, (40, 8), message, true).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.starts_with("\x1b[0m"), "{output:?}");
        let background = output.find("\x1b[48;5;236m").unwrap();
        assert!(output.find("qqq task editor").unwrap() < background);
        assert!(output.contains("\x1b[38;5;252m"));
        let blank = " ".repeat(40);
        for row in 2..8 {
            assert!(output.contains(&format!("\x1b[{row};1H{blank}")));
        }
        for row in [1, 8] {
            assert!(!output.contains(&format!("\x1b[{row};1H{blank}")));
        }
        let reset = output.rfind("\x1b[0m").unwrap();
        assert!(output.find("Body").unwrap() < reset);
        let footer = if message.is_empty() {
            render::KEYS
        } else {
            message
        };
        let visible_footer: String = footer.chars().take(40).collect();
        assert!(reset < output.find(&visible_footer).unwrap());
        assert!(!output.contains("Whole buffer ="));
    }
}

#[test]
fn resize_hint_uses_default_colors_without_editor_body() {
    crossterm::style::force_color_output(true);
    let mut output = Vec::new();
    render::draw(
        &mut output,
        &render::Layout::new(&["Body".into()], 10),
        0,
        &mut 0,
        (10, 2),
        "",
        true,
    )
    .unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.starts_with("\x1b[0m"));
    assert!(output.contains("Resize ter"));
    assert!(!output.contains("\x1b[48;"));
    assert!(!output.contains("\x1b[38;"));
}

#[test]
fn plain_editor_does_not_emit_color_commands() {
    let mut output = Vec::new();
    render::draw(
        &mut output,
        &render::Layout::new(&["Body".into()], 40),
        0,
        &mut 0,
        (40, 8),
        "",
        false,
    )
    .unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(!output.contains("\x1b[48;"));
    assert!(!output.contains("\x1b[38;"));
    assert!(output.contains("Body"));
}

#[test]
fn editor_reclaims_hint_row_for_body_and_keeps_cursor_above_footer() {
    let layout = render::Layout::new(&["A\nB\nC\nD\nE".into()], 40);
    let mut output = Vec::new();
    let mut top = 0;
    render::draw(&mut output, &layout, 1, &mut top, (40, 5), "", false).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert_eq!(top, 2);
    for (row, text) in [(2, "C"), (3, "D"), (4, "E")] {
        assert!(
            output.contains(&format!("\x1b[{row};1H{text}")),
            "{output:?}"
        );
    }
    assert!(output.contains("\x1b[5;1HCtrl-S"));
    assert!(output.ends_with("\x1b[4;2H"));
}

#[test]
fn preserved_crlf_displays_as_line_break() {
    let layout = render::Layout::new(&["First".into(), "\r\n".into(), "Second".into()], 20);
    assert_eq!(layout.rows, ["First", "Second"]);
    assert_eq!(layout.positions[2], (1, 0));
}

#[test]
fn newline_after_exact_width_does_not_add_blank_visual_row() {
    let layout = render::Layout::new(&["abcd".into(), "\n".into(), "x".into()], 4);
    assert_eq!(layout.rows, vec!["abcd", "x"]);
    assert_eq!(layout.positions, vec![(0, 0), (1, 0), (1, 0), (1, 1)]);
    assert_eq!(layout.nearest(1, 0), 2);
}

#[test]
fn layout_wraps_unicode_and_escapes_terminal_controls() {
    let layout = render::Layout::new(
        &[
            "界".into(),
            "👩‍💻".into(),
            "\x1b[31m".into(),
            "\n".into(),
            "tail".into(),
        ],
        4,
    );
    assert_eq!(layout.positions[0], (0, 0));
    assert_eq!(layout.positions[1], (0, 2));
    assert_eq!(layout.positions[2], (1, 0));
    assert!(layout.rows.concat().contains("\\u{1b}[31m"));
    assert!(layout.rows.iter().all(|line| !line.contains('\x1b')));
    assert!(
        layout
            .rows
            .iter()
            .all(|line| unicode_width::UnicodeWidthStr::width(line.as_str()) <= 4)
    );
}
#[test]
fn narrow_layout_preserves_atomic_placeholder_cursor_boundaries() {
    let fragments = vec![
        "Title".into(),
        "\n".into(),
        "[Pasted text #1: 1001 chars]".into(),
    ];
    let layout = render::Layout::new(&fragments, 5);
    assert_eq!(layout.positions.len(), 4);
    let start = layout.positions[2];
    assert_eq!(layout.nearest(start.0, start.1), 2);
    assert!(layout.positions[3].0 > start.0);
}
