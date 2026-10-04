# Dirty Description Marker (#154)

Required skill: superpowers:executing-plans.

Dirty state currently adds a column before task IDs and reduces list width globally. Move gold/plain `[*]` to the beginning of the first description preview row only. Keep ID/status columns, tree prefixes, wrapping, row count, continuation previews and clean rows unchanged when dirty state toggles. Prefix may clip first-row preview at the pane edge; full description stays available in editor/details. Dirty new/child title behavior remains.

Use saved task parent metadata to locate description start rather than interpreting literal description text. Keep selection and click targets for every preview row.

- [ ] Confirm baseline and failing render regression.
- [ ] Add first-description-row offsets to panel model; derive parent-tree depths from displayed tasks; remove dirty-dependent wrap width and global dirty column.
- [ ] Verify color/plain, compact/wide/narrow, nested trees, literal markers/tree symbols, long previews, marker clearing, retained drafts and unchanged clean-row geometry with render and PTY regressions.
- [ ] Update docs; run full locked suite, fmt, strict Clippy; read-only review.
- [ ] Integrate locally, install, verify installed behavior, complete task and resume persistent waiter.
