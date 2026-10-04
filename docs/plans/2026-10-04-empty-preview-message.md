# Empty Next Preview Message

Task 155 screenshot: `qqq next --dry-run` printed `No ready tasks.` while unfinished tasks were already in progress. Preview correctly found no queued candidate; generic empty-task copy did not explain what the command previews.

Human empty `next` and `next --dry-run` now print `No task available for pickup.` This follows the user's clarified wording preference; both commands share the existing task formatter. Successful previews retain stored task formatting. Existing queued-only preview contract remains intact: owned tasks are skipped, JSON empty result is `null`, ordinary claims retain ownership reuse, waits emit no empty message while blocking.

## Verification

- Isolated baseline at `1c44938`: build and 579 tests across 38 targets passed.
- Updated existing output assertions before implementation; focused regression failed on old `No ready tasks.` text. After fix, 108 tests across output, CLI, filters, dispatch and wait passed.
- Full suite: 579 tests across 38 targets, zero failures or ignored (`/private/tmp/qqq-155-full.log`). Formatting, diff checks and strict all-target Clippy passed.
- Read-only review found no issues. Feature commit `44d8fa2` fast-forwarded into clean local master.
- Installed CLI passed seven smoke groups: empty human/JSON preview, ordinary empty next, owned-only preview with ownership preserved, candidate preview with stored state preserved, filtered empty preview, normal owned reuse, queue-state hint (`/private/tmp/qqq-155-installed.log`).
- Concurrent assignment-row change `4c82636` integrated during install. Task checkout rebased without conflicts; combined full suite passed the same 579 tests across 38 targets (`/private/tmp/qqq-155-integrated-full.log`). Combined strict Clippy, formatting and diff checks passed. Follow-up review confirmed assignment details do not affect preview routing or task formatting.

- Combined source installed from stable task checkout with `cargo install --path . --locked --force`. Four final installed checks passed: empty human/JSON preview, owned-only preview with unchanged ownership, stored candidate preview, filtered empty preview (`/private/tmp/qqq-155-integrated-installed.log`).
- `qqq complete 155 --json` explicitly completed task at `2026-10-04T07:59:52.219Z`.

Queue handoff after evidence integration and checkout cleanup uses one blocking `qqq next --wait --local --json` process.
