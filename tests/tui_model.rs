#[allow(dead_code)]
#[path = "../src/tui/draft.rs"]
mod draft;
#[allow(dead_code)]
#[path = "../src/images.rs"]
mod images;

use draft::Draft;
use images::{ImageInput, ImageReference};

fn pasteboard(text: &str) -> String {
    format!("```pasteboard\n{text}\n```")
}

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
        let expected = if text.chars().count() > 1000 {
            pasteboard(&text)
        } else {
            text
        };
        assert_eq!(pasted.finish().unwrap().description, expected);
    }
}

#[test]
fn new_large_paste_uses_fence_while_seeded_long_text_stays_raw() {
    let payload = "x".repeat(1001);
    let seeded = Draft::new(&payload);
    assert!(!seeded.is_dirty_against(&payload));
    assert_eq!(seeded.finish().unwrap().description, payload);
    assert_eq!(seeded.fragments().concat(), payload);
    assert!(seeded.paste_mask().iter().all(|pasted| !pasted));

    let mut draft = Draft::new("Before ");
    draft.paste(&payload);
    draft.insert(" after");
    assert_eq!(draft.fragments()[7], "[Pasted Content 1001 chars]");
    let saved = draft.finish().unwrap().description;
    assert_eq!(
        saved,
        format!("Before \n```pasteboard\n{payload}\n```\n after")
    );
    let reloaded = Draft::from_saved(&saved, 1, &[]).unwrap();
    assert_eq!(reloaded.fragments()[8], "[Pasted Content 1001 chars]");
    assert_eq!(reloaded.finish().unwrap().description, saved);
    assert!(!reloaded.is_dirty_against(&saved));
}

#[test]
fn pasteboard_fence_round_trips_crlf_trailing_newline_and_nested_ticks() {
    let payload = format!("A\r\n```\n{}\n", "z".repeat(1001));
    let mut draft = Draft::new("");
    draft.paste(&payload);
    let saved = draft.finish().unwrap().description;
    assert_eq!(saved, format!("````pasteboard\n{payload}\n````"));
    let mut reloaded = Draft::from_saved(&saved, 1, &[]).unwrap();
    assert_eq!(
        reloaded.fragments(),
        vec![format!(
            "[Pasted Content {} chars]",
            payload.chars().count()
        )]
    );
    assert_eq!(reloaded.finish().unwrap().description, saved);
    reloaded.backspace();
    assert!(reloaded.finish().is_err());
}

#[test]
fn text_inserted_after_restored_fence_keeps_closing_delimiter_valid() {
    let saved = format!("Prefix \n```pasteboard\n{}\n```", "x".repeat(1001));
    let mut draft = Draft::from_saved(&saved, 1, &[]).unwrap();
    draft.insert(" suffix");
    let updated = draft.finish().unwrap().description;
    assert_eq!(updated, format!("{saved}\n suffix"));
    let restored = Draft::from_saved(&updated, 1, &[]).unwrap();
    assert!(
        restored
            .fragments()
            .contains(&"[Pasted Content 1001 chars]".to_owned())
    );
}

#[test]
fn incomplete_or_wrong_language_fence_stays_literal_text() {
    for text in [
        "```pasteboard\nshort",
        "```text\nshort\n```",
        "prefix ```pasteboard\nshort\n```",
    ] {
        let draft = Draft::from_saved(text, 1, &[]).unwrap();
        assert_eq!(draft.fragments().concat(), text);
        assert_eq!(draft.finish().unwrap().description, text);
    }
}

#[test]
fn pasteboard_marker_inside_other_code_fence_stays_literal_text() {
    for description in [
        "```text\n```pasteboard\nx\n```\n```",
        "~~~~text\n```pasteboard\nx\n```\n~~~~",
        "  ```text\n```pasteboard\nx\n```\n```",
    ] {
        let draft = Draft::from_saved(description, 1, &[]).unwrap();
        assert_eq!(draft.fragments().concat(), description);
        assert_eq!(draft.finish().unwrap().description, description);
    }
}

#[test]
fn multiple_pasteboard_blocks_keep_image_syntax_inside_payload() {
    let image = ImageReference {
        id: 1,
        name: "same.png".into(),
        media_type: "image/png".into(),
    };
    let original = "```pasteboard\n![same.png](.qqq/images/1/1.png)\n```\n[Image: same.png]\n```pasteboard\nsecond\n```";
    let draft = Draft::from_saved(original, 1, &[image]).unwrap();
    assert_eq!(
        draft
            .fragments()
            .iter()
            .filter(|part| part.starts_with("[Pasted Content "))
            .count(),
        2
    );
    assert_eq!(draft.image_mask().iter().filter(|image| **image).count(), 1);
    let saved = draft.finish().unwrap().description;
    assert!(saved.starts_with("```pasteboard\n![same.png](.qqq/images/1/1.png)\n```"));
    assert!(saved.contains("\n![same.png](.qqq/images/1/1.png)\n```pasteboard"));
}

#[test]
fn image_span_after_pasteboard_block_tracks_provisional_marker() {
    let mut draft = Draft::new("Lead ");
    draft.paste(&"x".repeat(1001));
    draft
        .image(ImageInput {
            name: "after.png".into(),
            data: b"\x89PNG\r\n\x1a\n".to_vec(),
        })
        .unwrap();
    let composition = draft.finish().unwrap();
    assert_eq!(composition.image_spans.len(), 1);
    assert_eq!(
        &composition.description[composition.image_spans[0].clone()],
        "[Image: after.png]"
    );
    assert!(composition.description.contains("```pasteboard"));
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
fn existing_large_description_stays_literal_and_edits_one_character() {
    let text = "Large existing body\n".repeat(100);
    for mut draft in [Draft::new(&text), Draft::from_saved(&text, 1, &[]).unwrap()] {
        assert_eq!(draft.fragments().concat(), text);
        assert!(draft.paste_mask().iter().all(|pasted| !pasted));
        assert!(!draft.is_dirty_against(&text));
        assert_eq!(draft.finish().unwrap().description, text);
        draft.backspace();
        assert_eq!(
            draft.finish().unwrap().description,
            text.trim_end_matches('\n')
        );
        draft.insert("!");
        assert!(draft.is_dirty_against(&text));
        assert_eq!(
            draft.finish().unwrap().description,
            text.trim_end_matches('\n').to_owned() + "!"
        );
    }
}

#[test]
fn large_pastes_expand_losslessly_and_delete_atomically() {
    let mut draft = Draft::new("");
    let text = "🦀".repeat(1001);
    draft.paste(&text);
    assert!(draft.fragments().concat().contains("1001 chars"));
    assert!(!draft.fragments().concat().contains('🦀'));
    assert_eq!(draft.finish().unwrap().description, pasteboard(&text));
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
            pasteboard(&"a".repeat(1001)),
            pasteboard(&"b".repeat(1001))
        )
    );
    draft.left();
    draft.delete();
    assert!(
        draft
            .finish()
            .unwrap()
            .description
            .ends_with(&format!("{}\n", pasteboard(&"a".repeat(1001))))
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
    assert_eq!(
        after_image.description,
        format!("Keep \n{}", pasteboard(&pasted))
    );
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
        format!("Keep \n{}", pasteboard(&pasted))
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
fn legacy_label_skips_image_already_named_by_markdown_link() {
    let images = [1, 2].map(|id| ImageReference {
        id,
        name: "dup.png".into(),
        media_type: "image/png".into(),
    });
    for original in [
        "![dup.png](.qqq/images/4/1.png) [Image: dup.png]",
        "[Image: dup.png] ![dup.png](.qqq/images/4/1.png)",
    ] {
        let draft = Draft::from_saved(original, 4, &images).unwrap();
        assert!(!draft.is_dirty_against(original));
        assert_eq!(draft.image_mask().iter().filter(|image| **image).count(), 2);
        assert!(
            draft
                .finish()
                .unwrap()
                .description
                .contains("![dup.png](.qqq/images/4/2.png)")
        );
    }
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

    let emphasis = ImageReference {
        id: 8,
        name: "a*b*_`code`&copy;.png".into(),
        media_type: "image/png".into(),
    };
    assert_eq!(
        emphasis.markdown(3).unwrap(),
        "![a\\*b\\*\\_\\`code\\`\\&copy;.png](.qqq/images/3/8.png)"
    );
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
fn whitespace_only_large_paste_cannot_bypass_blank_body_validation() {
    let mut draft = Draft::new("");
    draft.paste(&" ".repeat(1001));
    assert_eq!(
        draft.finish().err().unwrap().to_string(),
        "Task description cannot be empty; task not saved"
    );

    let restored =
        Draft::from_saved(&format!("```pasteboard\n{}\n```", " ".repeat(1001)), 1, &[]).unwrap();
    assert!(restored.finish().is_err());
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

#[test]
fn long_non_pasteboard_and_incomplete_fences_stay_literal() {
    let body = "Visible ordinary text 🦀\r\n".repeat(80);
    for description in [
        body.clone(),
        format!("```text\n{body}\n```"),
        format!("```pasteboard\n{body}"),
        format!("```text\n```pasteboard\n{body}\n```\n```"),
    ] {
        let draft = Draft::from_saved(&description, 1, &[]).unwrap();
        assert_eq!(draft.fragments().concat(), description);
        assert!(draft.paste_mask().iter().all(|pasted| !pasted));
        assert_eq!(draft.finish().unwrap().description, description);
        assert!(!draft.is_dirty_against(&description));
    }
}

#[test]
fn saved_long_text_around_pasteboard_and_image_only_collapses_explicit_fence() {
    let prefix = "Visible prefix 🦀\n".repeat(80);
    let suffix = "Visible suffix 界\n".repeat(80);
    let payload = "Payload".repeat(160);
    let image = ImageReference {
        id: 1,
        name: "example.png".into(),
        media_type: "image/png".into(),
    };
    let description = format!(
        "{prefix}```pasteboard\n{payload}\n```\n![example.png](.qqq/images/1/1.png)\n{suffix}"
    );
    let draft = Draft::from_saved(&description, 1, &[image]).unwrap();
    let visible = draft.fragments().concat();
    assert!(visible.starts_with(&prefix));
    assert!(visible.ends_with(&suffix));
    assert_eq!(
        draft.paste_mask().iter().filter(|pasted| **pasted).count(),
        1
    );
    assert_eq!(draft.image_mask().iter().filter(|image| **image).count(), 1);
    assert!(visible.contains(&format!(
        "[Pasted Content {} chars]",
        payload.chars().count()
    )));
    assert!(visible.contains("[Image #1: example.png]"));
    assert_eq!(draft.finish().unwrap().description, description);
    assert!(!draft.is_dirty_against(&description));
}

#[test]
fn undo_redo_restores_unicode_edits_and_action_carets() {
    let mut draft = Draft::new("Ae");
    assert!(!draft.undo());
    draft.insert("\u{301}");
    draft.insert("👩‍💻");
    assert_eq!(draft.cursor(), 3);
    draft.home();
    assert!(draft.undo());
    assert_eq!(draft.fragments().concat(), "Ae\u{301}");
    assert_eq!(draft.cursor(), 2);
    assert!(draft.undo());
    assert_eq!(draft.fragments().concat(), "Ae");
    assert!(draft.redo());
    draft.home();
    assert!(draft.redo());
    assert_eq!(draft.fragments().concat(), "Ae\u{301}👩‍💻");
    assert_eq!(draft.cursor(), 3);
    assert!(!draft.redo());
}

#[test]
fn undo_redo_treats_word_and_forward_deletion_as_one_edit() {
    let mut draft = Draft::new("alpha beta   ");
    draft.delete_previous_word();
    assert_eq!(draft.fragments().concat(), "alpha ");
    assert!(draft.undo());
    assert_eq!(draft.fragments().concat(), "alpha beta   ");
    assert_eq!(draft.cursor(), 13);
    assert!(!draft.undo());
    assert!(draft.redo());
    draft.home();
    draft.delete();
    assert_eq!(draft.fragments().concat(), "lpha ");
    assert!(draft.undo());
    assert_eq!(draft.fragments().concat(), "alpha ");
    assert_eq!(draft.cursor(), 0);
    assert!(draft.redo());
    assert_eq!(draft.cursor(), 0);
}

#[test]
fn no_op_edits_motion_and_failed_images_preserve_redo() {
    let mut draft = Draft::new("Seed");
    draft.insert("!");
    assert!(draft.undo());
    draft.home();
    draft.backspace();
    draft.delete_previous_word();
    draft.insert("");
    draft.paste("");
    assert!(
        draft
            .image(ImageInput {
                name: "bad.png".into(),
                data: b"bad".to_vec()
            })
            .is_err()
    );
    assert!(draft.redo());
    assert_eq!(draft.fragments().concat(), "Seed!");
    assert_eq!(draft.cursor(), 5);
    assert!(draft.undo());
    draft.end();
    draft.delete();
    assert!(draft.redo());
}

#[test]
fn new_edit_after_undo_invalidates_only_redo_branch() {
    let mut draft = Draft::new("Seed");
    draft.insert(" first");
    draft.insert(" second");
    assert!(draft.undo());
    draft.insert(" replacement");
    assert!(!draft.redo());
    assert!(draft.undo());
    assert_eq!(draft.fragments().concat(), "Seed first");
    assert!(draft.undo());
    assert_eq!(draft.fragments().concat(), "Seed");
}

#[test]
fn undo_redo_preserves_paste_bytes_and_atomic_image_numbering() {
    let payload = format!("A\r\n```\n{}\n", "界".repeat(1001));
    let input = ImageInput {
        name: "x.png".into(),
        data: b"\x89PNG\r\n\x1a\nbytes".to_vec(),
    };
    let mut draft = Draft::new("Seed ");
    draft.paste(&payload);
    let pasted = draft.finish().unwrap().description;
    draft.image(input.clone()).unwrap();
    let with_image = draft.finish().unwrap();
    assert!(draft.undo());
    assert_eq!(draft.finish().unwrap().description, pasted);
    assert!(draft.undo());
    assert_eq!(draft.fragments().concat(), "Seed ");
    assert!(draft.redo());
    assert!(draft.redo());
    assert_eq!(draft.finish().unwrap().description, with_image.description);
    assert_eq!(draft.finish().unwrap().images[0].data, input.data);
    assert!(draft.undo());
    draft.image(input).unwrap();
    assert!(draft.fragments().last().unwrap().starts_with("[Image #1:"));
    assert!(!draft.redo());
    draft.backspace();
    assert!(draft.undo());
    assert_eq!(
        draft.finish().unwrap().images[0].data,
        with_image.images[0].data
    );
}

#[test]
fn fresh_saved_content_has_no_initial_undo_steps() {
    let refs = [ImageReference {
        id: 7,
        name: "x.png".into(),
        media_type: "image/png".into(),
    }];
    let body = format!(
        "Before {}\n```pasteboard\n{}\n```",
        refs[0].markdown(2).unwrap(),
        "x".repeat(1001)
    );
    let mut draft = Draft::from_saved(&body, 2, &refs).unwrap();
    assert!(!draft.undo());
    assert!(!draft.redo());
    draft.backspace();
    assert!(draft.undo());
    assert_eq!(draft.finish().unwrap().description, body);
    assert!(!draft.undo());
}

#[test]
fn undo_history_retains_latest_256_changes() {
    let mut draft = Draft::new("Seed");
    for _ in 0..300 {
        draft.insert("x");
    }
    for _ in 0..256 {
        assert!(draft.undo());
    }
    assert!(!draft.undo());
    assert_eq!(
        draft.fragments().concat(),
        format!("Seed{}", "x".repeat(44))
    );
    for _ in 0..256 {
        assert!(draft.redo());
    }
    assert!(!draft.redo());
    assert_eq!(draft.cursor(), 304);
}

#[test]
fn undo_history_bounds_large_removed_payloads_without_corrupting_redo() {
    let payload = "x".repeat(17 * 1024 * 1024);
    let mut draft = Draft::new("Seed");
    for _ in 0..5 {
        draft.paste(&payload);
    }
    for _ in 0..4 {
        assert!(draft.undo());
    }
    let mut redone = 0;
    while draft.redo() {
        redone += 1;
    }
    assert!(
        redone > 0 && redone <= 3,
        "64 MiB budget retained {redone} large payloads"
    );
    assert_eq!(
        draft
            .paste_mask()
            .iter()
            .filter(|&&is_paste| is_paste)
            .count(),
        1 + redone
    );
}

#[test]
fn save_keeps_history_and_converts_all_reachable_pending_image_states() {
    let reference = ImageReference {
        id: 7,
        name: "x.png".into(),
        media_type: "image/png".into(),
    };
    let mut draft = Draft::new("Seed ");
    draft
        .image(ImageInput {
            name: "x.png".into(),
            data: b"\x89PNG\r\n\x1a\nbytes".to_vec(),
        })
        .unwrap();
    draft.backspace();
    assert!(draft.undo());
    draft.insert("!");
    let saved = format!("Seed {}!", reference.markdown(2).unwrap());
    draft.tags = vec!["staged".into()];
    assert!(draft.adopt_saved(&saved, 2, &[reference]).unwrap());
    assert!(draft.finish().unwrap().images.is_empty());
    assert!(!draft.is_dirty_against(&saved));
    assert!(draft.undo());
    assert!(draft.undo()); // redo the prior image deletion, using stored identity
    assert_eq!(draft.fragments().concat(), "Seed ");
    assert!(draft.redo());
    assert!(draft.redo());
    assert_eq!(draft.finish().unwrap().description, saved);
    assert!(draft.finish().unwrap().images.is_empty());
    assert!(!draft.is_dirty_against(&saved));
}

#[test]
fn save_keeps_uncommitted_redo_image_and_checks_snapshot_before_mutating() {
    let mut draft = Draft::new("Seed");
    let input = ImageInput {
        name: "x.png".into(),
        data: b"\x89PNG\r\n\x1a\nbytes".to_vec(),
    };
    draft.image(input.clone()).unwrap();
    assert!(draft.undo());
    assert!(draft.adopt_saved("Seed", 2, &[]).unwrap());
    assert!(draft.redo());
    assert_eq!(draft.finish().unwrap().images[0].data, input.data);
    let ref_image = ImageReference {
        id: 7,
        name: "x.png".into(),
        media_type: "image/png".into(),
    };
    assert!(
        !draft
            .adopt_saved("External change", 2, &[ref_image])
            .unwrap()
    );
    assert_eq!(draft.finish().unwrap().images[0].data, input.data);
    assert!(draft.undo());
    assert_eq!(draft.fragments().concat(), "Seed");
}
