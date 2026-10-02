# Continuous Interactive Add

## Scope

Built-in terminal editor for `qqq add` without inline text becomes continuous. `qqq add TEXT`, `qqq add --description TEXT`, forced external editor, and nonterminal external editor stay one-shot. `qqq edit` stays one-shot.

## Save and reset

Ctrl-S validates current draft, saves one task through existing `Db::save_composition`, then clears text, pasted images, cursor, scroll, and discard state. Footer shows saved task ID while new blank draft is ready. Each save commits before reset. Blank draft or DB error shows error in footer, preserves draft and initial command-line images, and allows retry.

`--parent` applies to every saved task. Command-line `--image` files attach only to first successful save. Images pasted into editor belong only to current draft. After first save, no image carries into next draft.

## Exit and output

Esc on empty draft or Ctrl-C ends session. Esc on nonempty draft asks before discarding; rejecting prompt resumes editing. Prior saves remain committed. Ending after at least one save exits successfully. Ending before any save keeps current cancellation error and empty stdout.

Editor draws only on stderr. After editor closes, stdout prints saved tasks. JSON mode returns one array, including single-task sessions. Human mode prints existing task summaries separated by blank lines. No partial JSON reaches stdout during editing.

## Verification

PTY tests must prove two Ctrl-S saves keep same editor open, each task reaches DB before next input, fresh draft has no prior text, final JSON contains both tasks, terminal restores on exit, and cancellation before first save still saves nothing. Cover empty save, supplied images/parent, and one-shot external/inline add behavior. Run full Rust suite, formatting, Clippy, and package checks after merge.
