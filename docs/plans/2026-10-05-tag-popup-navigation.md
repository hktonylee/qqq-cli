# Tag Popup Arrow Navigation

> Execute inline with superpowers:executing-plans; user-visible regression before fix, independent read-only review before local integration.

**Task177:** Arrow keys move focus between tag rows in multiline popup. Screenshot shows compatibility/docs/ci rows with caret on last row.

**Cause:** Action input stores only String; key handler ignores arrows and appends every edit. Popup renderer always places caret at end of last input row. Tag row window always shows tail.

**Design:** Tag input tracks grapheme-safe byte caret and preferred display column. Up/Down moves between logical tag rows, Left/Right between graphemes; Home/End and Delete follow standard text behavior. Typing/paste/Backspace/Shift-Enter operate at caret. Ctrl-U clears, Enter applies, Esc/Ctrl-C cancels as before. Active line and horizontal caret stay visible, Unicode display widths honored. Add optional explicit caret to popup rows while preserving default rendering for existing non-tag inputs. Keep draft, DB, filter, tags validation and shortcut footer behavior.

## Steps

- [x] Extend existing color/no-color tag PTY coverage to move among first/middle/
  last rows, edit/paste/undo at selected row, preserve DB until Enter, navigate
  long list offscreen and back. Observe ignored-arrow failure before implementation.
- [x] Add tag caret model/rendering and integrate action input edits. Add focused
  Unicode/column/newline/viewport regressions; retain existing modal shortcuts and
  other input behavior. Update reference tag editing controls.
- [x] Run focused, full serial, compatibility, fmt/strict Clippy/release; review
  independently; locally integrate/install and verify installed tag scenarios.
  Record verification before queue completion, cleanup and next silent wait.

## Evidence

Base f97ae1b. Screenshot inspected at .qqq/images/177/9.png. Current tag input and
popup key/render paths confirm append-only behavior.

- Added tag PTY regression stalled on ignored Up before fix
  (`/tmp/qqq-task-177-red.log`).
- Tag PTY 4 Rust tests/5 scenarios passed with arrows/editing/long-list scrolling;
  caret 4 unit tests plus popup 2 units passed. Evidence:
  `/tmp/qqq-task-177-tags.log`, `/tmp/qqq-task-177-cursor.log`,
  `/tmp/qqq-task-177-popup.log`.
- Final fmt/diff checks, strict all-target Clippy and release build passed.
- Independent read-only review clear: 42 focused cursor/popup/dashboard render
  tests passed, diff checks clean.
- Full serial suite: 690 tests across 40 binaries passed. Compatibility gate:
  153 tests across 7 binaries passed. Evidence: `/tmp/qqq-task-177-full.log`,
  `/tmp/qqq-task-177-gate.log`, `/tmp/qqq-task-177-clippy-final.log`,
  `/tmp/qqq-task-177-release.log`.
- Locally integrated 8b62998 onto master. Fresh main-checkout package rebuild:
  4 tag PTY tests/5 scenarios, 4 cursor units and 153 compatibility checks passed.
  Evidence: `/tmp/qqq-task-177-integrated-tags.log`,
  `/tmp/qqq-task-177-integrated-cursor.log`,
  `/tmp/qqq-task-177-integrated-gate.log`.
- Fresh release built and installed as qqq 0.5.0. Installed binary passed all 5
  tag PTY scenarios; release and installed SHA256 both
  `231e8062fd48c3471f61444045d8d4a580470b6e119f95cdb94f9b145261ea0c`.
  Evidence: `/tmp/qqq-task-177-integrated-release.log`,
  `/tmp/qqq-task-177-install.log`, `/tmp/qqq-task-177-installed-tags.log`.
- No push or remote CI run.
