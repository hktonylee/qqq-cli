# Multiline Tag Popup

Task #169: separate tags onto multiple popup lines.

Prefill each saved tag on its own input row. Preserve Enter apply, Esc/Ctrl-C
cancel and Ctrl-U clear. Shift+Enter appends newline. Existing append/backspace
editing stays intact; backspace at empty final row rejoins previous row.
User was offered alternate Enter/Ctrl-S behavior; absent preference, preserve
established Enter apply contract.

TUI accepts one tag per line, including CRLF paste. Ignore blank lines; reject
other controls, commas inside labels and square brackets through shared label
validation. Comma-separated input remains accepted for compatibility, including
empty-comma errors. CLI comma parser and machine-readable output stay intact.

Add TUI-specific line parser in shared tags module. Dedicated popup-row helper
renders each newline-separated value row, guidance, bounded validation errors
and one bottom shortcut line. Retain final input row and caret when height
requires hiding earlier rows. Short terminals reduce guidance before sacrificing
input or footer. Long labels keep existing horizontal tail view.

Footer shows clear, newline, apply and cancel keys together; compact aliases
fit narrow terminals. Saved tag order, deduplication, live tag color, selection,
dirty drafts, caret, parked buffers, filter and completed visibility persist.

Verify parser controls/blank lines/CRLF/comma compatibility, popup height and
cursor rows, prefill, Shift+Enter without save, multiline paste, overflow,
backspace, clear/cancel, color/NO_COLOR and installed binary. Full checks and
independent review precede completion.
