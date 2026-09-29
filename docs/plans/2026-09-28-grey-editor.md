# Grey Editor Plan

Task #33 requests grey backgrounds for interactive editors, with Codex as reference.
Built-in add/edit share one alternate-screen renderer. External editors own themes.

Codex's [composer style](https://github.com/openai/codex/blob/main/codex-rs/tui/src/style.rs)
blends white into dark terminal backgrounds at 12%, or black into light backgrounds
at 4%. qqq will use a predictable neutral dark-grey surface (ANSI 236, #303030)
with light text (ANSI 252, #d0d0d0), contrast about 8.6:1. Both colors are explicit
to keep content readable across terminal defaults. No theme queries or new deps.

- [x] Reproduce missing background in existing real PTY add/edit/save/cancel/error tests.
- [x] Paint every viewport cell, including headers/footer and empty/narrow states.
  Use shared renderer, respect existing color opt-outs on terminal stderr.
- [x] Reset foreground/background before leaving alternate screen on every exit.
- [x] Add meaningful rendering/PTY checks for unused cells, opt-outs and restoration;
  verify layout, editing, JSON isolation and resizing behavior remain sound.
- [x] Run focused TUI/editor tests, fmt, Clippy and release build; read-only review.
- [x] Rebase current master, integrate, rebuild live CLI, record results, complete task.


## Verification

- Four existing PTY scenarios failed first with missing background, then passed.
- Increased repaint traffic reproduced a PTY harness deadlock during large input.
  Nonblocking writes now drain output with a deadline; large paste/image save and
  cancel finish normally. No production editor input behavior changed.
- 192 combined tests passed after rebase onto harness/orchestrator identity work.
  Colored add/edit/blank/error/save/cancel and plain NO_COLOR/TERM=dumb paths covered;
  renderer checks full unused viewport cells and narrow resize state.
- `cargo fmt --check`, all-target Clippy with warnings denied, release build and
  `git diff --check` passed. Read-only review clear.
- Integrated `37ee3bb` on local master; root CLI rebuilt before task completion.
