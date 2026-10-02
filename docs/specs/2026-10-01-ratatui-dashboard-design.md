# Ratatui Dashboard Design

## Goal and scope

Rebuild `qqq tui`'s two-panel dashboard with Ratatui. Keep task list above editor, current keyboard behavior, task ordering, scroll target, cursor mapping, stderr rendering, JSON stdout isolation, and terminal restoration. Keep standalone add/edit editor renderer unchanged. Add semantic color to dashboard when terminal supports it; honor `NO_COLOR` and `TERM=dumb`.

## Rendering and state

Use Ratatui 0.29 to preserve crate's Rust 1.85 floor. Keep Crossterm 0.29 for input and terminal lifecycle; Ratatui's Crossterm backend writes frames to stderr. Create one `Terminal<CrosstermBackend<Stderr>>` per dashboard session so its frame diff tracks prior output. `src/tui/render.rs` retains grapheme-aware `Layout` and standalone editor drawing. New `src/tui/dashboard.rs` receives a Ratatui frame, existing `panel::ListRow` values, status lookup by task ID, selected task ID, editor layout/cursor/scroll positions, and chrome text. Ratatui `Layout` splits available height into equal top/bottom rectangles; top renders heading, scrollable task rows, separator; bottom renders editor heading, body, footer and cursor. At widths below 12 or heights below 8, show clipped resize hint with default terminal styling.

Task rows keep existing text from `output::render(Format::Tasks, ...)`, so tree structure, wrapping, and status labels remain consistent with CLI. `panel::scroll_to` keeps selected task visible. Existing editor layout maps grapheme positions to terminal cells; dashboard painter uses its rows and image spans without recalculating cursor positions. Paint empty editor body cells with neutral background on color terminals. Footer replaces key hint with save error or discard confirmation as today. Output data remains escaped by existing layout/output formatting before Ratatui renders it.

## Color

Use restrained ANSI-indexed palette: cool accent for headings and image labels, neutral dark editor body, high-contrast light body text, muted completed rows, cyan in-progress rows, red error rows, and highlighted selected row. Apply row status to whole task row, including wrapped continuation lines. Selection takes priority over status color. Message/footer receives a distinct foreground, with red reserved for errors. With color disabled, use default styles only and retain selection marker `> `.

## Validation

Use Ratatui `TestBackend` assertions for panel geometry, selected-row and status styles, editor/image styles, footer, cursor, narrow resize, and no-color. Keep grapheme/escape layout tests. Run real PTY scenarios for editing, selection, scroll, resize, error retry, clean JSON stdout, raw-mode/alternate-screen restoration, and `NO_COLOR`/dumb terminals. Run full Rust suite, formatting, strict Clippy. No DB or input-binding changes.
