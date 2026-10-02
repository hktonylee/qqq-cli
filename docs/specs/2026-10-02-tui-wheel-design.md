# Mouse wheel scrolling in task TUI

`qqq tui` enables Xterm mouse capture inside alternate screen; terminal guard disables capture before leaving screen on normal exit or error. Other built-in editors keep existing mouse behavior. Wheel events identify upper list versus lower editor from same Ratatui split geometry used for drawing. Events on separator, footer, outside terminal bounds, or undersized resize hint do nothing.

Each wheel notch moves viewport by up to three rendered rows, clamped to first/last possible top row. List and editor own independent top offsets. Wheel-scrolled list stops auto-following selected task; keyboard task navigation resumes selection follow. Wheel-scrolled editor stops auto-following caret; cursor hides when outside viewport. Typing, paste, or cursor movement resumes caret follow and reveals current edit position. Resize clamps both manual offsets to new row limits without changing draft or selection.

Pure model tests cover clamped wheel movement and pane hit-testing at boundaries. Ratatui render tests prove manual top persists despite offscreen selection/caret and keyboard-follow mode reveals them. PTY tests send SGR wheel input to both panes, verify first/last row limits, long descriptions, independent offsets, resize, no DB mutation, mouse disable and terminal restoration on success and induced error.
