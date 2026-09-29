#[allow(dead_code)]
#[path = "../src/tui/clipboard.rs"]
mod clipboard;
#[allow(dead_code)]
#[path = "../src/images.rs"]
mod images;

use std::borrow::Cow;
#[test]
fn clipboard_rgba_encodes_losslessly_as_png() {
    let input = arboard::ImageData {
        width: 1,
        height: 1,
        bytes: Cow::Borrowed(&[20, 40, 60, 80]),
    };
    let image = clipboard::encode(input).unwrap();
    assert_eq!(image.media_type().unwrap(), "image/png");
    let decoded = image::load_from_memory(&image.data).unwrap().into_rgba8();
    assert_eq!(decoded.into_raw(), vec![20, 40, 60, 80]);
}
#[test]
fn invalid_clipboard_buffers_report_errors() {
    for (width, height, bytes) in [(0, 1, vec![]), (1, 1, vec![0; 3]), (usize::MAX, 1, vec![])] {
        assert!(
            clipboard::encode(arboard::ImageData {
                width,
                height,
                bytes: Cow::Owned(bytes)
            })
            .is_err()
        );
    }
}
#[test]
fn quoted_image_path_is_captured_but_other_text_stays_text() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("space image.png");
    std::fs::write(&path, b"\x89PNG\r\n\x1a\n").unwrap();
    let input = clipboard::path_image(&format!("'{}'", path.display()))
        .unwrap()
        .unwrap();
    assert_eq!(input.name, "space image.png");
    assert_eq!(input.data, b"\x89PNG\r\n\x1a\n");
    std::fs::write(&path, b"plain text").unwrap();
    assert!(
        clipboard::path_image(&format!("'{}'", path.display()))
            .unwrap()
            .is_none()
    );
    assert!(clipboard::path_image("Words to paste").unwrap().is_none());
}
