# Grey Editor Plan

Task #33 requests grey backgrounds for interactive editors, with Codex as reference.
Built-in add/edit share one alternate-screen renderer. External editors own themes.

Codex's [composer style](https://github.com/openai/codex/blob/main/codex-rs/tui/src/style.rs)
blends white into dark terminal backgrounds at 12%, or black into light backgrounds
at 4%. qqq will use a predictable neutral dark-grey surface (ANSI 236, #303030)
with light text (ANSI 252, #d0d0d0), contrast about 8.6:1. Both colors are explicit
to keep content readable across terminal defaults. No theme queries or new deps.

- [ ] Reproduce missing background in existing real PTY add/edit/save/cancel/error tests.
- [ ] Paint every viewport cell, including headers/footer and empty/narrow states.
  Use shared renderer, respect existing color opt-outs on terminal stderr.
- [ ] Reset foreground/background before leaving alternate screen on every exit.
- [ ] Add meaningful rendering/PTY checks for unused cells, opt-outs and restoration;
  verify layout, editing, JSON isolation and resizing behavior remain sound.
- [ ] Run focused TUI/editor tests, fmt, Clippy and release build; read-only review.
- [ ] Rebase current master, integrate, rebuild live CLI, record results, complete task.
