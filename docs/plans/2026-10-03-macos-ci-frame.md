# macOS CI Resize Frame Regression

Task 152: fix failed macOS job [111243003759](https://github.com/hktonylee/qqq-cli/actions/runs/37136846089/job/111243003759).

## Evidence and fix

CI failed `tui_dashboard_wide_layout_resizes_and_preserves_editor` at padding-input screen equality assertion. Ubuntu passed. Same assertion failed once during task 148 verification; isolated rerun passed.

Added before/after screen diagnostics, stressed current source with 32 independent PTY runs, eight concurrent processes, alternating color/NO_COLOR. Runs 22 and 28 reproduced failure. Both snapshots differed only in final row: blank before padding input, complete hotkey footer afterward. Cursor and pane geometry already satisfied resize predicate while frame still incomplete. Fixed 100ms settle cannot guarantee renderer completion under load.

Wait for full footer and final editor cursor sequence before recording resized screen. Keep padding equality assertion unchanged; preserve full before/after diagnostics for future failures. No TUI behavior change required.

## Checks

- [x] Fetch actual failed job logs; confirm same assertion on current source.
- [x] Isolate task worktree from clean `b17f59a`; build passed. Reuse fresh 560-test baseline, with known intermittent frame race explicitly under investigation.
- [x] Reproduce before fix; inspect precise screen difference.
- [x] Add completed-frame predicate; rerun both color modes under concurrent stress.
- [x] Run full tests, fmt, Clippy, read-only review.
- [ ] Verify fixed revision in GitHub Actions on macOS and Ubuntu.
- [ ] Rebase/merge, install, check installed PTY, explicitly complete task, cleanup, resume one blocking queue wait.

- Green stress: 64 independent PTY runs, eight concurrent processes, alternating color and NO_COLOR, 0 failures. Before fix: 2 failures in 32 runs.
- Fresh full `cargo test --locked --no-fail-fast`: 560 tests, 38 targets, 0 failures or ignored. `/private/tmp/qqq-152-full.log`. Clippy all targets with `-D warnings`, fmt and diff checks passed.
- Dedicated CI verification branch will carry only reviewed test fix on original `68e248b` source; keep unpublished local features off remote master.
- Read-only review found no issues. CI branch `ci/task-152-frame` contains only test commit `bb2d5c6`; push was blocked by automatic approval review because remote source export lacked explicit destination/branch authorization. Requested approval; no remote branch created.
- Local master advanced to `b65d344` with task 150 multiple prerequisites. Rebase completed without conflicts. Fresh rebased full suite passed: 576 tests, 38 targets, 83 TUI tests, zero failures or ignored (`/private/tmp/qqq-152-rebased-full.log`). Rebased Clippy all targets with `-D warnings`, fmt, diff checks passed. Follow-up read-only review confirmed upstream prerequisite details leave fixture geometry, footer and final cursor unchanged.
