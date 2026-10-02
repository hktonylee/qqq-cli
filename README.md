# qqq

Local task queue for coding agents. Task metadata lives in each project's
`.qqq/qqq.db`; image bytes live in `.qqq/images/`. No server needed.

## Install

Requires Rust 1.85+ and C compiler. Install current source from repo root:

```sh
cargo install --path . --locked
```

Command name: `qqq`; crate name: `qqq-cli`. Published crates.io version 0.1.0
predates `.qqq/` layout and continuous add. After next release, install from
crates.io with:

```sh
cargo install qqq-cli --locked
```

## Quick start

Run `init` in project root. Task commands use nearest `.qqq` directory in
current directory or parents; missing `qqq.db` there causes error. Config
commands need no DB.

```sh
qqq init
qqq add "Build API"
qqq add "Build client" --parent 1
qqq list
qqq next --session worker-1
qqq message 1 "API ready" --session worker-1
qqq complete 1 --session worker-1
qqq next --session worker-1
```

`next` claims highest-priority ready task, oldest ID on ties. `complete` frees
its dependent tasks. Use IDs returned by `add` and `next`; examples above assume
fresh DB. See `qqq --help`
or `qqq <command> --help` for all flags.

## List and show

```sh
qqq list
qqq show 1
qqq show -1                         # newest task; -2 is second newest
qqq list --max-completed 10
qqq list --all                      # bypass configured completion limit
qqq list --query "parser error"     # search full descriptions, ignoring case
qqq list --status new --status error
qqq list --query "parser" --status in_progress --all
qqq list --watch
qqq list --watch --json
qqq list --include-archived
```

`list` shows status, priority, and dependency tree in stable ID order. In a
terminal with a known width, it shows full descriptions with wrapped, aligned
continuation lines; piped output keeps first-line previews. `show` adds
messages, images, ownership history and Herdr link. Negative indexes work with
`show`, `edit`, `archive`, `unarchive`, and `reopen`; `-1` selects newest task.

`--max-completed N` keeps N most recent completions plus unfinished tasks;
`0` hides completed tasks. Hidden parents display children as roots. `--all`
bypasses configured limit; it conflicts with `--max-completed`.

`--query TEXT` matches any part of a full description, including later lines,
using Unicode lowercase comparison. Repeat `--status` to match any listed
status: `new`, `in_progress`, `completed`, or `error`. Query and statuses combine
with AND. Search runs after completed-display limit; use `--all` to search all
completed tasks. Visible ancestors of matches appear as `[context]` rows even
when they do not match filters. JSON stays flat and marks those ancestors with
`context_only: true`; direct matches keep existing task fields. Empty filtered
results return `[]` in JSON or `No matching tasks.` in human output.

`--watch` prints initial list, then refreshes after DB commits. Terminal output
redraws; pipes, `TERM=dumb` and JSON append snapshots. Ctrl-C stops watching.

Default list, watch, TUI, and claim queue hide archived tasks. Use
`list --include-archived`, `list --watch --include-archived`, or
`tui --include-archived` to browse them. Human lists label archived rows
`[archived]`; JSON includes `archived: true` without changing description.
`--all` changes completed-task limit only; hidden archived completions do not
consume that limit.

Global `--json` works before or after commands. JSON lists stay flat, preserve
full descriptions and `parent_id`; watch prints one array per line. Empty lists
return `[]`; no ready task returns `null`. Errors use stderr (exit 1 for runtime,
2 for arguments). `NO_COLOR=1` or `TERM=dumb` disables color.

## Add and edit

```sh
qqq add "Fix login"
qqq add "Fix urgent login" --priority 8
qqq add --description "Fix login"   # same input, alternative flag
qqq edit 1 --description "Updated details"
qqq edit 1 --priority -5             # lower future claim order
qqq edit -1                         # edit newest task interactively
```

Descriptions preserve whitespace and newlines; blank-only text fails.
Priority defaults to `0` and accepts integers from `-100` through `100`.
Higher priority claims first among ready new tasks; equal priority uses oldest
ID. `list` stays in ID/dependency order. Editing priority on active, completed,
or error tasks keeps status and ownership metadata; an already-owned task still
returns to its owner before new claims.

## Archive and restore

```sh
qqq archive 12
qqq list --include-archived
qqq show 12
qqq unarchive 12
```

Archive keeps task status, description, priority, messages, images, ownership
history, and dependencies. `show`, `edit`, messages, and image export still work
by ID. Archive/unarchive add history events; repeated commands change nothing.
Events record supplied `--session` or `cli` when absent. In-progress tasks
cannot be archived. An unfinished parent cannot be archived while a visible
unfinished child depends on it; adding or reparenting an unfinished child, or
unarchiving one, under an archived unfinished parent also fails. Archived
completed parents still release dependent tasks.

## Reopen completed work

```sh
qqq reopen 12
qqq reopen -1 --session reviewer
```

`reopen` returns a completed task to `new` and records a reopen event. It keeps
description, priority, parent, messages, images, creation time, and prior
history. A task in any other status fails without changes; repeating `reopen`
also fails. Archived completed tasks require `unarchive` first. A completed
child under an archived unfinished parent cannot reopen until parent is
unarchived or completed. Completed descendants stay completed; new descendants
wait for reopened parent to complete again. Reopened tasks return to default
lists, including `--max-completed 0`, and become claimable when dependencies
permit.

On a terminal, `add` without text or `edit` without update flags opens built-in
editor. Ctrl-S saves, Esc exits blank draft or confirms discard of nonempty
draft, Ctrl-C exits, Ctrl-W deletes previous word on same line, Ctrl-V pastes
clipboard text or image. Option+Left/Right moves cursor by word. Clipboard paste
needs desktop clipboard support.

Built-in editors collapse pastes over 1,000 characters into one gold
`[Pasted Content N chars]` item. Saving stores full payload in Markdown:

````markdown
```pasteboard
pasted text
```
````

Shift-Up/Down and `qqq edit` restore complete fences as one editable item;
Backspace or Delete removes entire item. Payload containing backticks gets a
longer fence. `NO_COLOR=1` and `TERM=dumb` keep label plain. Existing task text
outside `pasteboard` fences remains unchanged on save.

In built-in `qqq add`, Ctrl-S creates or updates task, clears editor, then waits
for next task. Saves commit immediately; exit keeps prior saves. Shift+Up loads
newest task, then older tasks; Shift+Down moves toward newer tasks, then blank
draft. Header shows selected ID and status. JSON output returns saved tasks as
array on exit. Exit without any save returns error.

Switching from changed draft asks before discard; N, Enter, or Esc keeps it.
Navigation includes completed and active tasks; deleted IDs are skipped.
`--parent` applies to each new task. `--image` files attach only to first
successful save, including when updating existing task. Inline add, external
editor and `qqq edit` save once.

Use `--edit` (`-e`) to force `$EDITOR`; nonterminal interactive calls also need
it. Editor command must wait until editing finishes.

```sh
export EDITOR='vim'
qqq add --edit
qqq edit 1 --edit --description "Prefilled draft"
```

Built-in add/edit editor uses stderr; stdout holds result on exit. Cancelling
draft keeps earlier saves. Direct edits preserve omitted fields, ownership and
attachments.

## Task TUI

```sh
qqq tui
```

Upper panel lists all tasks; lower panel edits current task. Shift-Up/Down
selects tasks, then blank draft. Ctrl-S updates selected task or creates new
task, then clears editor for next task without leaving TUI. Esc exits blank
draft or confirms discard of nonempty draft; Ctrl-C exits. Ctrl-V or terminal
paste inserts text; pasting image file path attaches image. Saves commit
immediately. TUI needs terminal and writes no stdout, including with `--json`.

Mouse wheel scrolls task list or editor under pointer. Both panes keep separate
scroll positions; scrolling editor never edits or saves text. Shift-Up/Down
returns list to selected task. Editor keys reveal caret after manual scroll.
Left-click task row to load it in editor, including indented or wrapped rows.
Left-click editor text to place caret; dirty drafts ask before switching tasks.

Filter bar stays visible above task list. `/` focuses it; type to match any
part of full description, ignoring case. Matching tasks retain parent chain;
no matches shows `No matching tasks.`. Backspace edits query. Esc clears
nonempty query, then exits filter focus on next press. Tab or Enter returns to
editor. Alt+/ inserts literal `/` into editor. Shift-Up/Down navigates visible
tasks while filter is active. Query changes do not change selected task, unsaved
draft, or DB.

## Dependencies and images

Each task has at most one parent. Child becomes ready when parent completes.
Parent must exist; self-parenting and cycles fail. Parent changes affect future
claims without removing active ownership.

```sh
qqq add "Build client" --parent 1
qqq edit 2 --set-parent 1
qqq edit 2 --set-parent none         # clear dependency
```

Attach files through `add` or `edit`; repeat `--image` for multiple files:

```sh
qqq edit 2 --image ./before.png --image ./after.jpg
qqq show 2
qqq show 2 --export-image 1 --output ./exported.png
```

Image IDs come from `show`. Export needs selected task's image ID and new output
path. PNG, JPEG, GIF and WebP signatures supported, up to 20 MiB each; signature
check does not fully validate file. Bytes copy into
`.qqq/images/<task_id>/<image_id>.<ext>`. Built-in editor also accepts pasted
image paths or clipboard images. Pasted images become Markdown links in the
description, such as `![before.png](.qqq/images/2/4.png)`. Reloading a task
shows each linked image as one editable item; deleting that item removes its
description link while keeping the attachment available through `show`.

Description, status, parent and image updates save atomically.

## Agents and recovery

Use stable, unique session ID per worker:

```sh
export QQQ_SESSION='worker-1'
qqq next
qqq message 1 "Running tests"
qqq complete 1
qqq next --wait
```

`--session` overrides `QQQ_SESSION`. Each owner gets one active task; repeated
`next` returns it. Claims never expire. `--wait` blocks until work is ready;
without it, empty queue prints `No ready tasks.` and exits successfully. Error
tasks and blocked children stay out of queue.

Without `--session` or `QQQ_SESSION`, owner discovery uses:

1. Exact Herdr pane from `HERDR_PANE_ID` or `HERDR_ENV=1` (requires pane ID).
2. Native Codex `CODEX_THREAD_ID`, then `CODEX_SESSION_ID` fallback.
3. Unique Herdr agent at project root (`foreground_cwd` before `cwd`).

Explicit session IDs and native Codex ownership work without Herdr when dispatch is
disabled or `next --local` is used. Exact Herdr context takes precedence.
`next --local` skips dispatch, still resolves owner.

Completion, release and error marking require owner. Use original session token
or uniquely matching displayed `harness_session`; add `--harness-name` when
public sessions overlap. JSON assignment fields: `harness_name`,
`harness_session`, `orchestrator_name`, `orchestrator_session`.

### Return or retry work

Return active task owned by `worker-1`:

```sh
qqq edit 1 --set-pending --session worker-1
```

Alternatively, mark active task failed, then retry:

```sh
qqq edit 1 --set-status error --reason "Missing credentials" --session worker-1
qqq edit 1 --set-status new          # retry error task; no session required
```

`--set-pending` equals `--set-status new`. Active tasks require owner; error
tasks can be retried without one. Error marking requires nonblank reason,
clears assignment, records reason as message. Status updates skip editor.

For abandoned active work, inspect task before forcing return:

```sh
qqq show 1
qqq edit 1 --set-status new --force
```

`--force` skips ownership matching and Herdr discovery. It requires literal
`--set-status new`; cannot combine with `--set-pending` or `--edit`. Return/retry
preserves content, dependencies, messages, images and Herdr link.

Session IDs coordinate local agents; they do not authenticate users. Any local
caller can edit task content or append messages regardless of ownership.

## Config and aliases

Config: `~/.config/qqq/config.toml` (under `HOME`).

```sh
qqq config --list
qqq config alias.ls 'list --watch'
qqq config --get alias.ls
qqq config alias.ls                 # same as --get
qqq config display.max-completed 10
qqq config --unset alias.ls
```

Keys use TOML dotted syntax; quote literal dots: `'alias."with.dot"'`.
Alias values are strings; `herdr.next-to-new-agent` is boolean;
`display.max-completed` is non-negative integer. Other values accept TOML
literals, falling back to strings.

Reads do not create files. `--list` shows stored values; `--get` can inspect
invalid settings. Writes validate known settings, preserve comments and unknown
keys. `--json` returns typed values; unset restores default.

Completion limit config affects human lists only; explicit `--max-completed`
affects text and JSON. Missing limit means unlimited.

Example config:

```toml
[display]
max-completed = 10

[alias]
ls = "list"
n = "next"
done = "complete"
bug = "add"
```

`qqq bug "Fix login"` expands to `qqq add "Fix login"`. Aliases support quoting
and chains; built-in commands take precedence. No shell expansion. Cycles,
invalid quoting and `!` aliases fail.

## Herdr

Optional Herdr integration links tasks to live agent panes:

```sh
qqq herdr link 1
qqq herdr find 1
qqq herdr link 1 --agent codex --agent-session session-123 --server work
```

Discovery selects exact caller pane or unique agent at project root. Ambiguous
identity fails before claim. Without agent-session hooks, terminal ID plus agent
kind supplies identity. Active claims retain identity as hooks change.

`link` stores association without changing owner; `find` searches live panes for
saved link identity. Both require Herdr CLI and server. Explicit `--session`
claims do not auto-link.

To dispatch ready work into new Codex agent tab:

```sh
qqq config herdr.next-to-new-agent true
qqq next
```

Dispatch needs `HERDR_ENV=1` and `HERDR_WORKSPACE_ID`. It reuses caller's active
task; otherwise reserves ready task, opens sibling tab, starts agent and submits
prompt. Empty or blocked queues create no tab. `next --local` skips dispatch.
Default: disabled.

Startup failure releases dispatch reservation; created tabs stay open. Prompt
errors keep claim and link because delivery may have happened. Inspect agent
before retry or forced release.

## Data

Keep `.qqq/` out of Git. Create portable snapshot while other local qqq
writers run:

```sh
qqq backup ../project-snapshot.tar
mkdir ../restored-project
cd ../restored-project
qqq restore ../project-snapshot.tar
qqq list
```

Backup contains consistent SQLite data plus every stored image. Tar archive
starts with `manifest.json`, then `qqq.db`, then `images/<task-id>/<image-id>.<ext>`.
Manifest records SHA-256 hashes and sizes; restore checks those, image rows,
SQLite integrity, and foreign keys before installing `.qqq`. Backup refuses to
overwrite destination. Restore works only from new project directory or one
with empty `.qqq`; existing data and nested projects are rejected. Invalid
archives leave project unchanged. Relative snapshot paths resolve from current
directory.

Preview permanent deletion before confirming it:

```sh
qqq backup ../before-delete.tar
qqq delete 42
qqq delete 42 --yes
```

Only archived tasks without children can be deleted. Use positive task ID;
relative creation indexes can change before confirmation. Preview lists messages,
events, Herdr link, stored images, and paths without changing project data;
`--json` returns same counts with `deleted:false`. `--yes` removes task and
dependent data permanently, including stored image files, then returns
`deleted:true`. Release or complete an active task before archiving it; delete
or reparent every child before deleting parent. IDs remain reserved after
deletion. Preview refuses legacy DB versions and pending recovery without
changing files; run `qqq list` to migrate or recover before previewing again.
If deletion stops while images are staged, next DB-backed qqq command recovers
them according to committed DB state. Keep backup until recovered project
passes `qqq doctor`.

Check project health without changing DB or attachments:

```sh
qqq doctor
qqq --json doctor
```

Doctor checks SQLite integrity, foreign keys, schema, image paths, byte counts,
signatures, and orphan files/directories. Healthy project exits 0. Issues print recovery
actions and exit 1; JSON includes `ok`, counts, and `issues` with code, path,
message, and action. Doctor never migrates DB. If SQLite journal/WAL sidecars
exist, stop writers and recover or checkpoint SQLite before rerunning doctor.
For damaged DB or images, restore verified snapshot into new directory first;
inspect recovered data before replacing damaged project files. Keep damaged
copy until recovery is verified. `DELETE_RECOVERY_PENDING` means staged deletion
needs a normal qqq command to recover, followed by another doctor check.

To move old root-level `qqq.db`, stop DB writers. Before `qqq init`, check that
`.qqq/qqq.db` does not exist. From project root:

```sh
mkdir -p .qqq
sqlite3 qqq.db ".backup '.qqq/qqq.db'"
qqq list # run updated CLI
```

Check task data before removing old DB. New CLI does not discover root-level
`qqq.db`; `qqq init` without migration creates separate empty DB.

Compatible DBs at schema versions 1–8 migrate to version 9. Version 6 moves
existing image blobs to `.qqq/images/` before SQLite drops its `data` column.
Migration tries `VACUUM` to reclaim old blob pages. If compaction warns, stop
writers and run `sqlite3 .qqq/qqq.db 'VACUUM;'` later. Upgrade other qqq workers
before migration; older binaries cannot open version 9. Legacy `title` or
`pending` schemas need manual conversion; newer unknown schemas fail. Back up
before conversion.

## Development

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

[CI](.github/workflows/ci.yml) checks Ubuntu 24.04 and macOS 14, builds release
binary, uploads `qqq-<OS>-<ARCH>` archives (14-day retention).

### Publish to crates.io

[Publish workflow](.github/workflows/publish.yml) checks `v<version>` tag against
`Cargo.toml`, runs checks and package dry run, then publishes `qqq-cli`. Setup:

1. Sign in to crates.io, verify email, create token allowed to publish `qqq-cli`.
2. Add GitHub Actions repo secret `CARGO_REGISTRY_TOKEN`.
3. Choose unused version, update `Cargo.toml` and `Cargo.lock`, commit changes.
4. Push commit and matching tag. Example after bumping to `0.1.1`:

```sh
git push origin master
git tag v0.1.1
git push origin v0.1.1
```

For manual validation, run **Publish to crates.io** with existing `tag` and
default `dry_run=true`. Set `dry_run=false` to publish new version. Published
versions cannot be overwritten.

License: [MIT](LICENSE).
