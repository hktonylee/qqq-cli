# Markdown image references in task descriptions

## Outcome

Pasting an image in the built-in editor creates an image atom. Saving stores bytes in `.qqq/images/<task_id>/<image_id>.<ext>` and writes `![filename](.qqq/images/<task_id>/<image_id>.<ext>)` at the atom's position in the task description. Opening that task again shows the image as an atomic editor item. Its link survives unrelated edits; deleting the item removes its description link. Attachment files and metadata remain append-only, matching existing edit behavior.

CLI `--image` attachments stay metadata-only unless the user placed an image in the editor body. Ordinary Markdown, including links without matching task image metadata, remains editable text. Existing `[Image: filename]` descriptions gain editor image atoms only when the task has matching image metadata; the first save converts those tokens to Markdown links. Duplicate names match attachments in image-ID order. Literal unmatched labels remain text.

## Data flow

`Draft` keeps pasted image bytes separate from stored-image references. Its composition contains description text, new image inputs, and byte spans for new-image placeholders. Spans follow image order and cannot come from typed text. The visible editor label remains concise; the saved Markdown contains the full path. Filename text in Markdown alt labels escapes Markdown punctuation and normalizes control whitespace; metadata retains the original filename.

`Db::save_composition` uses the existing immediate add/edit transaction. After inserting image rows and writing files, it has task and image IDs. It replaces only the marked spans with Markdown references, then updates the task description before commit. Any failure rolls back task text, image rows, and newly written files together. Direct `Db::add`/`Db::edit` paths without editor spans retain their current behavior, including flagged attachments.

On task navigation and built-in `qqq edit <id>`, `Db` supplies that task's image IDs, names, and media types. `Draft` recognizes only exact references derived from those records, plus exact legacy labels matched in attachment order. Exact Markdown links reserve their image IDs before legacy labels match, so mixed descriptions do not duplicate a reference. Stored refs occupy one atom for cursor motion, deletion, and visual image styling. Dirty checks compare original loaded text, so navigation across an untouched legacy task does not prompt to discard. `finish` emits normalized Markdown for legacy refs. Built-in edit saves through the same composition path while retaining parent changes.

## Verification

Test draft image spans, exact reference recognition, duplicate legacy labels, deletion, dirty behavior, and unmatched text. Test DB save/reload with real PNG bytes, multiple pasted images and flagged attachments, filenames needing Markdown escaping, and transaction rollback. Update PTY expectations for new description syntax; add PTY navigation check for atomic image display and editing. Run focused tests, full Rust suite, fmt, Clippy, and integrated suite after local merge.
