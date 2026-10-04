# Dirty Description Marker (#154)

Required skill: superpowers:executing-plans.

Dirty state currently adds a column before task IDs and reduces list width globally. Move gold/plain `[*]` to the beginning of the first description preview row only. Keep ID/status columns, tree prefixes, wrapping, row count, continuation previews and clean rows unchanged when dirty state toggles. Prefix may clip first-row preview at the pane edge; full description stays available in editor/details. Dirty new/child title behavior remains.

Use saved task parent metadata to locate description start rather than interpreting literal description text. Keep selection and click targets for every preview row.

- [x] Confirm baseline and failing render regression.
- [x] Add first-description-row offsets to panel model; derive parent-tree depths from displayed tasks; remove dirty-dependent wrap width and global dirty column.
- [x] Verify color/plain, compact/wide/narrow, nested trees, literal markers/tree symbols, long previews, marker clearing, retained drafts and unchanged clean-row geometry with render and PTY regressions.
- [x] Update docs; run full locked suite, fmt, strict Clippy; read-only review.
- [ ] Integrate locally, install, verify installed behavior, complete task and resume persistent waiter.

## Validation before integration

- Original render regression failed because marker occupied metadata prefix. Revised render check passes across 12/50/72/150 widths, color/plain and selected/unselected states.
- Model check covers tree depth, missing displayed parent, literal tree symbols, Unicode text, long IDs, blank descriptions and clearing dirty flags.
- Color/plain PTY toggles marker at 72/50/150/72 widths. Other task rows and continuation rows remain identical; revert restores original list. Test waits for completed cursor/frame state.
- Full locked suite: 578 passed. Strict all-target Clippy, fmt and diff checks passed. Independent review approved.
- Master advanced with task #138 assignment detail changes; rebase and verify combined tree before local integration.
