# Task action menu arrow selection

Task #126: Up/Down selects action in Ctrl-G menu. Opening menu selects Complete;
arrows wrap through Complete, Retry error, Reopen, Archive/Unarchive, Priority,
Parent. Enter activates selected action through existing shortcut flow, retaining
confirmation, dirty-draft protection, input validation, status/ownership checks.
Existing letter shortcuts and Esc remain available. Moving selection changes no
task/editor/filter state or DB data.

Selected row uses existing dark accent tint plus `>` marker. Other action rows
reserve same marker width. Plain mode retains marker with default styles. Cursor
follows selected menu row. Popup keeps all six actions visible at minimum supported
12x8 size; typing fields and error/confirmation popups retain current behavior.

1. Add PTY regression through existing action fixtures before production change.
   Open menu, verify initial selection, wrap Up/Down, verify unchanged DB/draft,
   Enter opens Priority/Parent or confirmation. Cover cancellation, letter
   shortcuts, dirty-draft rejection, compact terminal, and NO_COLOR.
   Run `cargo test --locked --test tui tui_dashboard_menu_arrows` -> fails because
   menu lacks selected marker/arrow handling.
2. Add selected index to `ActionUi::Menu` and shared ordered shortcut array.
   Render selected action role and marker; Up/Down updates selection modulo six,
   Enter resolves shortcut and uses existing activation branch. Avoid let chains
   to preserve declared Rust 1.85 compatibility.
3. Add selected-action popup role/style/cursor handling in `render.rs` and
   `dashboard.rs`. Verify tint/marker, hotkey styling, plain mode, movement clearing
   previous selected row, and unchanged popup bounds/background. Update existing
   role/cursor expectations and README menu instructions.
4. Run focused `cargo test --locked --test tui_dashboard_render --test tui`, then
   `cargo fmt --check`, `cargo test --locked`, and
   `cargo clippy --locked --all-targets -- -D warnings` in private worktree target.
   Commit verified checkpoints, request read-only review, fix findings.
5. Rebase on current master; verify changes if concurrent integration moved base.
   Fast-forward master, install with `cargo install --path . --locked --offline --force`.
   Exercise installed CLI menu arrows/NO_COLOR/compact paths. Remove merged
   worktree/branch, complete #126, resume one persistent qqq queue waiter.
