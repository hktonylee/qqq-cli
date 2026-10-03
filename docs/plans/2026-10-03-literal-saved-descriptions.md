# Literal Saved Descriptions

> **For agent:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to execute this plan task-by-task.

Task #151: long ordinary descriptions must remain editable text. Only complete
`pasteboard` fences collapse when loading saved descriptions. Existing terminal
pastes over 1,000 characters still collapse and save as `pasteboard` fences.

Cause: `Draft::new` and ordinary segments in `Draft::from_saved` call `seed`,
which sends text through size-based paste collapsing. Tasks #146–150 contain
1,362–1,675 characters and no pasteboard fences; attached screenshot matches this
incorrect display. Stored bytes are intact.

- [ ] Verify current model baseline. Update seeded-long-text tests to require literal
  fragments and per-character edits. Add saved long ordinary/non-pasteboard/incomplete
  fence cases, plus long text around explicit pasteboard block and image reference.
  Assert ordinary spans are not paste atoms, baseline clean and save bytes unchanged.
  Observe expected failures before implementation.
- [ ] Seed ordinary text through `insert`. Remove obsolete `SeededPlain` source and
  shared seeded-paste helper; keep newly pasted text threshold and explicit saved
  fence parsing. Update documentation to distinguish saved text from new paste.
- [ ] Add real dashboard PTY check for long ordinary and long `text` code block:
  cursor/scroll reach end, home/edit/save changes one character, no Pasted Content
  label; contrast same body inside `pasteboard` fence. Keep existing pasteboard,
  image, dirty buffer, single editor and history checks.
- [ ] Run model/focused PTY, full locked suite, fmt, Clippy with warnings denied and
  diff whitespace checks. Request read-only review through existing reviewer.
- [ ] Commit, rebase onto current master, verify integration if code changes,
  fast-forward, install locked/offline release, run installed ordinary/plain/fence
  scenarios, clean merged worktree/branch, complete #151, resume persistent waiter.
