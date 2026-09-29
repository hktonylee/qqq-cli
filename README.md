# qqq

Local-first task queue for coding agents. Task descriptions, messages, images,
ownership and history live in per-project SQLite file `qqq.db`. No server needed.

## Install

Requires Rust 1.85+ and C compiler for bundled SQLite. Package name: `qqq-cli`;
installed command: `qqq`.

```sh
cargo install qqq-cli --locked
```

From source, run in repo root:

```sh
cargo install --path . --locked
```

## Quick start

Run `init` in project root. Other task commands find nearest `qqq.db` in current
folder or its parents. Config commands need no DB.

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

Tasks start `new`. `next` claims oldest ready task as `in_progress`; `complete`
marks it `completed`. In this example, task 2 becomes ready after task 1 completes.
In existing queues, use IDs returned by `add` and `next`.
Use `qqq --help` or `qqq <command> --help` for full flag reference.

## Tasks

### List and show

```sh
qqq list
qqq show 1
qqq show -1                         # newest task; -2 is second newest
qqq list --max-completed 10
qqq list --all                      # bypass configured completion limit
qqq list --watch
qqq list --watch --json
```

`list` shows IDs, statuses and first description lines as dependency tree.
`show` includes full description, messages, images, ownership, history and Herdr
link. Negative indexes work with `show` and `edit`, counting existing tasks by
creation order across all statuses.

`--max-completed N` keeps most recently completed tasks plus every unfinished
task, including errors. `0` hides completed tasks. Completion history determines
recency; editing completed text does not count as another completion. Hidden
parents make children display as roots. `--all` conflicts with an explicit limit.

`--watch` prints initial snapshot, then refreshes after DB commits, checking every
250 ms. Terminals redraw in place; piped output and `TERM=dumb` append snapshots.
Ctrl-C stops watching.

Commands print readable text by default. Global `--json` works before or after
subcommands. JSON lists stay flat, preserve full descriptions and `parent_id`;
watch mode emits one compact JSON array per line. Empty lists return `[]`; no
ready task returns `null`. Errors go to stderr: exit 1 for runtime errors, 2 for
argument errors. Terminal status colors can be disabled with `NO_COLOR=1` or
`TERM=dumb`; piped output and JSON stay plain.

### Edit descriptions

```sh
qqq add "Fix login"
qqq add --description "Fix login"   # same input, alternative flag
qqq edit 1 --description "Updated details"
qqq edit -1                         # edit newest task interactively
```

Descriptions are whole text bodies; whitespace and newlines are preserved.
Blank-only text is rejected. `add` without text and `edit` without update flags
open built-in terminal editor. Ctrl-S saves; Esc asks before discarding nonempty
draft; Ctrl-C cancels. Ctrl-V reads clipboard text or images. Large pastes
collapse into placeholders, then expand on save. Clipboard access needs desktop
clipboard support.

Use `--edit` (`-e`) to force external editor; nonterminal interactive calls also
require `$EDITOR`. Editor must wait until editing finishes.

```sh
export EDITOR='vim'
qqq add --edit
qqq edit 1 --edit --description "Prefilled draft"
```

Editor UI uses stderr; stdout contains only final result. Cancelled or failed
composition saves nothing. Inline field edits skip editor and preserve omitted
fields. Content edits preserve ownership and existing attachments.

### Dependencies and images

Each task has at most one parent. New child becomes claimable only when parent
completes. Parent must exist; self-parenting and cycles are rejected. Changing
parent affects future claims, preserving already active ownership.

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

Image IDs come from `show`; exported image must belong to selected task.
Destination must not exist. Images support PNG, JPEG, GIF and WebP signatures,
up to 20 MiB each. Signature checks do not fully validate image contents.
Bytes are copied into DB, so source files can be removed. Pasting image paths
or clipboard images into built-in editor also attaches them.

Combined description, status, parent and image updates save atomically;
validation failures leave task unchanged.

## Agents and recovery

Use stable, unique session ID for each worker:

```sh
export QQQ_SESSION='worker-1'
qqq next
qqq message 1 "Running tests"
qqq complete 1
qqq next --wait
```

`--session` overrides `QQQ_SESSION`. One active task per session; repeated `next`
returns same task. Concurrent workers cannot claim same task. Claims never expire.
`--wait` waits for ready work; without it, empty queue prints `No ready tasks.`
and exits successfully. Error tasks and children with incomplete parents are skipped.

Without explicit session ID, ownership discovery uses:

1. Exact Herdr pane from `HERDR_PANE_ID` or `HERDR_ENV=1` (requires pane ID).
2. Native Codex `CODEX_THREAD_ID`, then `CODEX_SESSION_ID` fallback.
3. Unique Herdr agent whose cwd matches DB directory.

Explicit IDs and native Codex ownership work without Herdr when dispatch is
disabled or `next --local` is used; native Codex discovery also requires being
outside exact Herdr context. Other clients can supply session ID or use Herdr
discovery. `next --local` bypasses dispatch; it still resolves caller ownership.

Completion, ordinary release and error marking require matching task owner.
Owner lookup prefers original session token, then uniquely matching displayed
`harness_session`. Use `--harness-name` to disambiguate public sessions.
JSON assignment fields are
`harness_name`, `harness_session`, `orchestrator_name`, `orchestrator_session`.
Override flags appear in command help.

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

`--set-pending` equals `--set-status new`. Active tasks require owner; any local
caller can retry error task. Error marking requires nonblank reason, clears
assignment and saves reason as message. Error tasks stay blocked until retried.
New and completed tasks cannot be returned to queue. Status updates skip editor;
`--set-pending` conflicts with `--set-status` and `--edit`.

For abandoned active work, inspect task before forcing return:

```sh
qqq show 1
qqq edit 1 --set-status new --force
```

`--force` skips ownership matching and Herdr discovery for active or error tasks.
It requires literal `--set-status new`; it cannot combine with `--set-pending`,
`--set-status error` or `--edit`. Return/retry preserves content, dependencies,
messages, attachments and saved Herdr link.

Session IDs coordinate local agents; they do not authenticate users. Any local
caller can edit task content or append messages regardless of ownership.

## Config and aliases

Config path: `~/.config/qqq/config.toml`, based on `HOME`.

```sh
qqq config --list
qqq config alias.ls 'list --watch'
qqq config --get alias.ls
qqq config alias.ls                 # same as --get
qqq config display.max-completed 10
qqq config --unset alias.ls
```

Keys use TOML dotted-key syntax; quote literal dots, e.g. `'alias."with.dot"'`.
Alias values stay strings. `herdr.next-to-new-agent` accepts booleans;
`display.max-completed` accepts non-negative integers up to `i64::MAX`.
Other values accept TOML literals, falling back to strings.

Reads do not create files. `--list` shows sorted, explicitly stored values;
`--get` can inspect incorrectly typed settings so they can be repaired. Writes
validate known settings, preserve comments and unknown keys, and use atomic
replacement. `--json` returns typed values. Unsetting restores default behavior.

Completion limit config affects human lists only; explicit `--max-completed`
applies to text and JSON. `--all` bypasses config. Missing limit means unlimited.

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

`qqq bug "Fix login"` expands to `qqq add "Fix login"`. Extra arguments keep
original boundaries. Alias values support quoting and other aliases; built-in
commands take precedence. No shell expansion: variables, globs and command
substitutions stay literal. Cycles, invalid quoting and `!` aliases are rejected.

## Herdr

Optional Herdr integration links tasks to live coding-agent panes:

```sh
qqq herdr link 1
qqq herdr find 1
qqq herdr link 1 --agent codex --agent-session session-123 --server work
```

Automatic discovery selects exact caller pane or unique agent at DB directory.
`foreground_cwd` takes precedence over `cwd`; symlinks are resolved. Ambiguous or
missing identity fails before claiming. When agent-session hooks are absent,
Herdr terminal ID plus agent kind supplies stable identity. Active claims retain
saved identity as hooks appear or disappear.

`link` stores association without transferring ownership. `find` locates current
pane by saved identity; moved panes remain discoverable. These commands require
Herdr CLI and running server. Explicit local `--session` claims do not auto-link.

To dispatch ready work into new Codex agent tab:

```sh
qqq config herdr.next-to-new-agent true
qqq next
```

Dispatch requires `HERDR_ENV=1` and `HERDR_WORKSPACE_ID`. It reuses caller's
existing task; otherwise reserves oldest ready task, opens unfocused sibling tab
in caller's workspace, starts agent and submits task prompt. Empty or blocked
queues create no tabs. `qqq next --local` bypasses dispatch. Default: disabled.

Startup failure returns unchanged reservation to `new`; created tabs stay open.
Prompt errors keep claim and link because delivery may have happened. Inspect
agent before retrying or forcing recovery.

## Data

Keep `qqq.db` out of Git. Back up through SQLite while workers may be running:

```sh
sqlite3 qqq.db '.backup backup.sqlite'
```

Compatible single-description DBs at schema versions 1–4 migrate automatically
to version 5. Legacy `title` or `pending` schemas need manual conversion; newer
unknown schemas are rejected. Back up before converting old data.

## Development

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

[CI](.github/workflows/ci.yml) runs checks on Ubuntu 24.04 and macOS 14, builds
and smoke-tests release binary, then uploads `qqq-<OS>-<ARCH>` archives. Download
artifacts from workflow run within 14 days.

### Publish to crates.io

[Publish workflow](.github/workflows/publish.yml) checks pushed `v<version>` tags
against `Cargo.toml`, runs formatting, Clippy, tests and package dry run, then
publishes `qqq-cli`. Setup:

1. Sign in to crates.io, verify email, create token allowed to publish `qqq-cli`.
2. Add GitHub Actions repo secret `CARGO_REGISTRY_TOKEN`.
3. Choose unused version, update `Cargo.toml` and `Cargo.lock`, commit changes.
4. Push commit and matching tag. Example after bumping to `0.1.1`:

```sh
git push origin master
git tag v0.1.1
git push origin v0.1.1
```

For manual validation, run **Publish to crates.io** with existing `tag`; leave
`dry_run` enabled (default). No token needed. Disable it to publish unpublished
version. Published versions cannot be overwritten.

License: [MIT](LICENSE).
