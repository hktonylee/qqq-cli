#[allow(dead_code)]
#[path = "../src/tui/render.rs"]
mod render;

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
