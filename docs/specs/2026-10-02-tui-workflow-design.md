# Complete task TUI workflow verification

Validate one uninterrupted `qqq tui` session: filter seeded tasks, wheel list to a result, click it, edit lower pane, save update, then create and save another task from the cleared draft. Include bracketed text paste and pasted image path in new task, proving both persisted through DB and image store. Verify stdout remains empty while TUI runs.

Add separate PTY checks for truly empty DB and mixed New, Completed, Error task labels. Keep existing focused PTY coverage for no filter matches, keyboard-only navigation, narrow resize, pasteboard round trips, failed save, and image handling in other editor modes. On normal and error exits, assert raw terminal flags restored and escape sequences disable alternate screen, bracketed paste, and mouse capture.

Update README task TUI help to state keys, filter behavior, wheel, click, confirmation, paste, and new-task continuation. Test changes should expose integration faults; alter production code only for a reproduced failure.
