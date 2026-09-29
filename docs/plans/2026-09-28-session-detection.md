# Session Detection Plan

Use superpowers:executing-plans inline. Rust CLI; existing Herdr identity adapter.

- [ ] Add real-binary tests for Codex thread/session fallback, overrides,
  wrong owner, wait/release/complete, no Herdr invocation, exact pane without
  HERDR_ENV, invalid context, dispatch's existing native claim, manual link.
- [ ] Clear inherited Codex/Herdr context in existing fixtures; observe new
  feature tests fail before implementing.
- [ ] Add native Codex env helper; use it in local owner and dispatch caller.
  Detect exact Herdr context from pane env; pass pane explicitly to Herdr.
  Extract automatic Herdr link lookup so native identity does not block linking.
- [ ] Document precedence and limitations. Run full suite, fmt, clippy, review.
- [ ] Commit, rebase, ff merge, verify merged checkout, mark task complete,
  continue next --wait.
