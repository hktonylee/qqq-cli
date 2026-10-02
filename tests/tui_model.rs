#[allow(dead_code)]
#[path = "../src/tui/draft.rs"]
mod draft;
#[allow(dead_code)]
#[path = "../src/images.rs"]
mod images;

use draft::Draft;
use images::{ImageInput, ImageReference};

#[test]
fn dirty_check_ignores_cursor_motion_and_reverted_edits() {
    let mut draft = Draft::new("Saved");
    assert!(!draft.is_dirty_against("Saved"));
    draft.left();
    assert!(!draft.is_dirty_against("Saved"));
    draft.insert("!");
    assert!(draft.is_dirty_against("Saved"));
    draft.backspace();
    assert!(!draft.is_dirty_against("Saved"));
}

#[test]
fn dirty_check_tracks_new_image_until_removed() {
    let mut draft = Draft::new("Saved");
    draft
        .image(ImageInput {
            name: "x.png".into(),
            data: b"\x89PNG\r\n\x1a\nbytes".to_vec(),
        })
        .unwrap();
    assert!(draft.is_dirty_against("Saved"));
    draft.backspace();
    assert!(!draft.is_dirty_against("Saved"));
}

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
fn ctrl_w_deletes_previous_word_and_spaces_without_crossing_line() {
    let mut draft = Draft::new("alpha beta   ");
    draft.delete_previous_word();
    assert_eq!(draft.finish().unwrap().description, "alpha ");
    draft.delete_previous_word();
    assert!(draft.finish().is_err());

    let mut draft = Draft::new("First\r\n  second");
    draft.delete_previous_word();
    assert_eq!(draft.finish().unwrap().description, "First\r\n  ");
    draft.delete_previous_word();
    assert_eq!(draft.finish().unwrap().description, "First\r\n");
    draft.delete_previous_word();
    assert_eq!(draft.finish().unwrap().description, "First\r\n");
}

#[test]
fn word_motion_skips_whitespace_and_moves_across_lines() {
    let mut draft = Draft::new("alpha  beta\n🦀world  ");
    draft.previous_word();
    assert_eq!(draft.cursor(), 12); // Start of 🦀world.
    draft.previous_word();
    assert_eq!(draft.cursor(), 7); // Start of beta.
    draft.previous_word();
    assert_eq!(draft.cursor(), 0);
    draft.previous_word();
    assert_eq!(draft.cursor(), 0);
    draft.next_word();
    assert_eq!(draft.cursor(), 5); // End of alpha.
    draft.next_word();
    assert_eq!(draft.cursor(), 11); // End of beta.
    draft.next_word();
    assert_eq!(draft.cursor(), 18); // End of 🦀world.
    draft.next_word();
    assert_eq!(draft.cursor(), 20); // End of trailing spaces.
    draft.next_word();
    assert_eq!(draft.cursor(), 20);
}

#[test]
fn word_motion_keeps_paste_and_image_atoms_whole() {
    let mut draft = Draft::new("One ");
    draft.paste(&"x".repeat(1001));
    draft
        .image(ImageInput {
            name: "icon.png".into(),
            data: b"\x89PNG\r\n\x1a\nimage".to_vec(),
        })
        .unwrap();
    draft.insert("suffix");
    assert_eq!(draft.cursor(), 12);
    draft.previous_word();
    assert_eq!(draft.cursor(), 6); // Before typed suffix.
    draft.previous_word();
    assert_eq!(draft.cursor(), 5); // Before image.
    draft.previous_word();
    assert_eq!(draft.cursor(), 4); // Before pasted text.
    draft.next_word();
    assert_eq!(draft.cursor(), 5); // After pasted text.
    draft.next_word();
    assert_eq!(draft.cursor(), 6); // After image.
    draft.next_word();
    assert_eq!(draft.cursor(), 12); // After typed suffix.
}

#[test]
fn ctrl_w_deletes_unicode_prefix_at_cursor() {
    let mut draft = Draft::new("école 🦀world");
    draft.set_cursor(9); // After "🦀wo".
    draft.delete_previous_word();
    assert_eq!(draft.finish().unwrap().description, "école rld");
    draft.insert("new");
    assert_eq!(draft.finish().unwrap().description, "école newrld");
}

#[test]
fn ctrl_w_deletes_paste_and_image_placeholders_atomically() {
    let mut draft = Draft::new("Keep ");
    let pasted = "🦀".repeat(1001);
    draft.paste(&pasted);
    draft
        .image(ImageInput {
            name: "pasted.png".into(),
            data: b"\x89PNG\r\n\x1a\nimage".to_vec(),
        })
        .unwrap();
    draft.delete_previous_word();
    let after_image = draft.finish().unwrap();
    assert_eq!(after_image.description, format!("Keep {pasted}"));
    assert!(after_image.images.is_empty());
    draft.delete_previous_word();
    assert_eq!(draft.finish().unwrap().description, "Keep ");
}

#[test]
fn ctrl_w_preserves_placeholder_before_typed_word() {
    let mut draft = Draft::new("Keep ");
    draft
        .image(ImageInput {
            name: "pasted.png".into(),
            data: b"\x89PNG\r\n\x1a\nimage".to_vec(),
        })
        .unwrap();
    draft.insert("suffix");
    draft.delete_previous_word();
    let after_word = draft.finish().unwrap();
    assert_eq!(after_word.description, "Keep [Image: pasted.png]");
    assert_eq!(after_word.images.len(), 1);

    let mut draft = Draft::new("Keep ");
    let pasted = "🦀".repeat(1001);
    draft.paste(&pasted);
    draft.insert("suffix");
    draft.delete_previous_word();
    assert_eq!(
        draft.finish().unwrap().description,
        format!("Keep {pasted}")
    );
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
    assert_eq!(composition.image_spans, vec![8..27]);
    draft.backspace();
    assert!(draft.finish().unwrap().images.is_empty());
}

#[test]
fn stored_markdown_image_reloads_as_single_atom_and_deletes_as_one() {
    let image = ImageReference {
        id: 2,
        name: "pasted.png".into(),
        media_type: "image/png".into(),
    };
    let link = "![pasted.png](.qqq/images/1/2.png)";
    let mut draft = Draft::from_saved(&format!("Before {link} after"), 1, &[image]).unwrap();
    assert_eq!(draft.image_mask().iter().filter(|image| **image).count(), 1);
    assert!(
        draft
            .fragments()
            .concat()
            .contains("[Image #1: pasted.png]")
    );
    assert_eq!(
        draft.finish().unwrap().description,
        format!("Before {link} after")
    );
    assert!(!draft.is_dirty_against(&format!("Before {link} after")));
    let image_index = draft.image_mask().iter().position(|image| *image).unwrap();
    draft.set_cursor(image_index + 1);
    draft.backspace();
    assert_eq!(draft.finish().unwrap().description, "Before  after");
}

#[test]
fn legacy_labels_match_duplicate_attachments_in_order() {
    let images = [1, 2].map(|id| ImageReference {
        id,
        name: "dup.png".into(),
        media_type: "image/png".into(),
    });
    let original = "A [Image: dup.png] B [Image: dup.png]";
    let draft = Draft::from_saved(original, 4, &images).unwrap();
    assert!(!draft.is_dirty_against(original));
    assert_eq!(draft.image_mask().iter().filter(|image| **image).count(), 2);
    assert_eq!(
        draft.finish().unwrap().description,
        "A ![dup.png](.qqq/images/4/1.png) B ![dup.png](.qqq/images/4/2.png)"
    );
    let literal =
        Draft::from_saved("![dup.png](elsewhere) [Image: other.png]", 4, &images).unwrap();
    assert!(literal.image_mask().iter().all(|image| !image));
    assert_eq!(
        literal.finish().unwrap().description,
        "![dup.png](elsewhere) [Image: other.png]"
    );
}

#[test]
fn markdown_image_reference_escapes_filename_without_changing_metadata() {
    let image = ImageReference {
        id: 7,
        name: "a]b\\c.png".into(),
        media_type: "image/png".into(),
    };
    assert_eq!(
        image.markdown(3).unwrap(),
        "![a\\]b\\\\c.png](.qqq/images/3/7.png)"
    );
    assert_eq!(image.name, "a]b\\c.png");
}

#[test]
fn image_mask_marks_attachment_atom_but_not_literal_label() {
    let mut draft = Draft::new("[Image #9: literal.png] ");
    draft
        .image(ImageInput {
            name: "pasted.png".into(),
            data: b"\x89PNG\r\n\x1a\nimage".to_vec(),
        })
        .unwrap();
    let fragments = draft.fragments();
    let image_mask = draft.image_mask();
    assert_eq!(fragments.len(), image_mask.len());
    assert_eq!(image_mask.iter().filter(|image| **image).count(), 1);
    assert_eq!(
        fragments[image_mask.iter().position(|image| *image).unwrap()],
        "[Image #1: pasted.png]"
    );
    draft.backspace();
    assert!(draft.image_mask().iter().all(|image| !image));
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
