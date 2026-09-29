#[allow(dead_code)]
#[path = "../src/tui/draft.rs"]
mod draft;
#[allow(dead_code)]
#[path = "../src/images.rs"]
mod images;

use draft::Draft;
use images::ImageInput;

#[test]
fn whole_body_seed_and_paste_preserve_short_and_large_line_endings() {
    for text in [
        "First\r\nSecond\rThird\n".to_owned(),
        "Line\r\nNext\r".repeat(200),
    ] {
        let seeded = Draft::new(&text);
        assert_eq!(seeded.finish().unwrap().description, text);
        let mut pasted = Draft::new("");
        pasted.paste(&text);
        assert_eq!(pasted.finish().unwrap().description, text);
    }
}

#[test]
fn home_and_end_respect_preserved_crlf_line_boundary() {
    let mut draft = Draft::new("First\r\nSecond");
    draft.home();
    draft.insert("Start ");
    assert_eq!(draft.finish().unwrap().description, "First\r\nStart Second");
    draft.end();
    draft.insert(" end");
    assert_eq!(
        draft.finish().unwrap().description,
        "First\r\nStart Second end"
    );
}

#[test]
fn existing_large_description_starts_collapsed_and_saves_unchanged() {
    let text = "Large existing body\n".repeat(100);
    let draft = Draft::new(&text);
    assert!(draft.fragments().concat().contains("[Pasted text #1:"));
    assert_eq!(draft.finish().unwrap().description, text);
}

#[test]
fn large_pastes_expand_losslessly_and_delete_atomically() {
    let mut draft = Draft::new("");
    let text = "🦀".repeat(1001);
    draft.paste(&text);
    assert!(draft.fragments().concat().contains("1001 chars"));
    assert!(!draft.fragments().concat().contains('🦀'));
    assert_eq!(draft.finish().unwrap().description, text);
    draft.backspace();
    assert!(draft.finish().is_err());
}

#[test]
fn repeated_pastes_and_literal_labels_keep_distinct_payloads() {
    let mut draft = Draft::new("");
    draft.insert("[Pasted text #1: 1001 chars]\n");
    draft.paste(&"a".repeat(1001));
    draft.insert("\n");
    draft.paste(&"b".repeat(1001));
    assert_eq!(
        draft.finish().unwrap().description,
        format!(
            "[Pasted text #1: 1001 chars]\n{}\n{}",
            "a".repeat(1001),
            "b".repeat(1001)
        )
    );
    draft.left();
    draft.delete();
    assert!(
        draft
            .finish()
            .unwrap()
            .description
            .ends_with(&format!("{}\n", "a".repeat(1001)))
    );
}

#[test]
fn short_paste_and_grapheme_editing_preserve_unicode() {
    let mut draft = Draft::new("");
    draft.paste("e\u{301}界👩‍💻");
    draft.backspace();
    assert_eq!(draft.finish().unwrap().description, "e\u{301}界");
    draft.left();
    draft.delete();
    assert_eq!(draft.finish().unwrap().description, "e\u{301}");
    draft.backspace();
    assert!(draft.finish().is_err());
    draft.insert("e");
    draft.insert("\u{301}");
    draft.backspace();
    assert!(draft.finish().is_err());
}

#[test]
fn image_placeholders_attach_only_while_present() {
    let mut draft = Draft::new("Details ");
    draft
        .image(ImageInput {
            name: "pasted.png".into(),
            data: b"\x89PNG\r\n\x1a\nimage".to_vec(),
        })
        .unwrap();
    assert!(
        draft
            .fragments()
            .concat()
            .contains("[Image #1: pasted.png]")
    );
    let composition = draft.finish().unwrap();
    assert_eq!(composition.images.len(), 1);
    assert_eq!(composition.description, "Details [Image: pasted.png]");
    draft.backspace();
    assert!(draft.finish().unwrap().images.is_empty());
}

#[test]
fn draft_rejects_blank_body_but_allows_image_on_first_line() {
    let mut draft = Draft::new("");
    assert!(draft.finish().is_err());
    draft
        .image(ImageInput {
            name: "x.png".into(),
            data: b"\x89PNG\r\n\x1a\n".to_vec(),
        })
        .unwrap();
    let result = draft.finish().unwrap();
    assert_eq!(result.description, "[Image: x.png]");
    assert_eq!(result.images.len(), 1);
}

#[test]
fn image_inputs_enforce_existing_signature_and_size_limits() {
    for (data, media) in [
        (b"\x89PNG\r\n\x1a\n".as_slice(), "image/png"),
        (b"\xff\xd8\xff".as_slice(), "image/jpeg"),
        (b"GIF89a".as_slice(), "image/gif"),
        (b"RIFF1234WEBP".as_slice(), "image/webp"),
    ] {
        let image = ImageInput {
            name: "file".into(),
            data: data.to_vec(),
        };
        assert_eq!(image.media_type().unwrap(), media);
    }
    assert!(
        ImageInput {
            name: "bad".into(),
            data: vec![0]
        }
        .media_type()
        .is_err()
    );
    assert!(
        ImageInput {
            name: "big.png".into(),
            data: vec![0; 20 * 1024 * 1024 + 1]
        }
        .media_type()
        .is_err()
    );
}
