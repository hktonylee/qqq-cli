# Grouped Task Menu

Task #166: rename Parent to Set parent; separate related Ctrl-G actions with
one empty row. Keep hotkeys and underlying actions unchanged.

Groups, in order:

1. Status: `c Complete`, `e Mark error`, `r Retry error` (error tasks only),
   `o Reopen`.
2. Settings: `p Priority`, `d Set parent`.
3. Visibility: `a Archive` or `a Unarchive`.

Blank rows use ordinary popup body style; never selectable. Arrow navigation
and Enter index only actions. Live retry visibility preserves selected key.
Full-height menus keep heading and hint. Short menus retain selection and
available group separators within a scrolling window; heading/hint use
remaining space. Hotkey access stays available for offscreen actions.

Validation: row order/labels/blanks, every selected action at multiple heights,
PTY arrow navigation/wrap/Enter, live retry changes, mark-error hotkey,
background preservation, resize, NO_COLOR, short-terminal access, full suite,
Clippy/fmt, release and installed-menu PTY checks.
