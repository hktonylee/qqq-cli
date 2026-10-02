# TTY Multiline Task List Design

`qqq list` currently shows only first description line. On stdout TTY with a usable column count, show every description line and wrap long lines to terminal width. Keep JSON output and redirected human output unchanged.

The formatter receives optional terminal columns. CLI and `list --watch` supply columns only when stdout is a TTY and size detection succeeds; Unix size detection queries stdout itself because its width may differ from the controlling TTY. Watch refreshes width on each redraw. Width at or below the task text start falls back to current one-line preview. `NO_COLOR` and `TERM=dumb` affect color, not whether a TTY can show multiline text. Existing status colors apply to each physical row, after width calculation.

Each task's first row retains `ID`, `STATUS`, and tree branch. Continuation rows use spaces matching actual ID/status column width, then ancestor tree guides and either `│   ` for a sibling branch or four spaces for a final branch. Text starts at the same display column on every row of that task. Root continuations start at the TASK column. Explicit blank description lines produce blank continuation rows. Wrap at Unicode grapheme boundaries by display-cell width, without dropping spaces or splitting a grapheme; one grapheme wider than remaining width stays intact. Escape controls before wrapping; no task text becomes terminal control bytes.

Verify explicit newlines, soft wraps, nested branch padding, Unicode width, color, narrow fallback, pipe/JSON stability, and real PTY width detection. Existing non-TTY list tests remain unchanged.
