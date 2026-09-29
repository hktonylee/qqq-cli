# qqq

Local-first Rust CLI for project tasks and coding agent sessions. Everything lives in **`qqq.db`**, including image bytes. No server required. Commands print human-readable text by default; use `--json` for scripts and agents. Errors go to stderr with exit code 1 (argument errors: 2).

## Output

`qqq list` shows task IDs, status, and titles as a dependency tree. Children appear
below their parent; roots and siblings follow creation order. Branches replace
the parent column:

```text
ID     STATUS       TITLE
1      New          Build API
2      New          ├── Auth
4      New          │   └── Token tests
3      New          └── Client
5      New          Docs
```

`qqq list --json` keeps the flat array in creation order, including `parent_id`.

Use `qqq list --max-completed <N>` to cap completed rows while keeping every new
and in-progress task. The most recent completions are retained; completion
history determines recency, falling back to update time and ID when history is
absent. Editing an older completed task does not count as another completion.
Omit the flag to use the human display default; `0` hides completed rows. An
explicit flag applies to text and JSON, overriding config. Children whose parents
are hidden appear as roots; JSON preserves the original `parent_id`. Filtering
does not delete task data.

```sh
qqq list --max-completed 10
qqq list --max-completed 0 --json
```

`qqq show <id>` includes description, ownership, messages, images, history and Herdr
details. Empty lists print `No tasks yet.`; queues with no ready tasks print
`No ready tasks.`. In a terminal, list rows use normal terminal color for new, cyan for
in-progress, and grey for completed tasks. Aliases such as `ls = "list"` use the
same colors. Piped output and `--json` stay plain. Set `NO_COLOR=1` or `TERM=dumb`
to disable colors.

```sh
qqq list
qqq show 1
qqq --json list
qqq next --json --session agent-session-123
```

Use `qqq list --watch` to keep the list open and refresh after SQLite commits,
including changes made by another qqq process or a direct SQLite connection.
Terminal output redraws in place; piped output appends snapshots without terminal
escape codes. `qqq list --watch --json` emits compact newline-delimited JSON:
one complete task array per line. Idle databases produce no repeated snapshots.
Changes are polled every 250 ms. Ctrl-C stops watching. Aliases such as `ls` work
with `--watch` too.

`--json` works before or after subcommands, including nested commands. It preserves
the existing JSON shapes, including `[]` for empty lists and `null` when no task
is ready. Existing scripts that parse command output must add `--json`.

Limit completed tasks by default in human `list` output through
`~/.config/qqq/config.toml`:

```toml
[display]
max-completed = 10
```

The value must be a non-negative integer. `0` hides completed tasks; missing
setting means unlimited. It uses the same completion ordering as the explicit
flag. New and in-progress tasks always remain visible; children of hidden parents
appear as roots. `qqq list --all` bypasses the default. `--json` ignores the config
default; an explicit `--max-completed` still filters JSON. `show` opens any task.
The same config and flag rules apply to every `list --watch` snapshot. A configured
list with no visible tasks prints `No tasks to display.`.

## Install

Requires Rust 1.85+ and a C compiler for bundled SQLite.

```sh
cargo install --path . --locked
```

## Tasks

Run `init` in your project root. Other commands find the nearest `qqq.db` in current directory or its parents. Filename is fixed.

```sh
qqq init
qqq add "Fix login" --description "Show useful error when credentials expire"
qqq edit 1 --description "Reproduce expiry, fix retry, add regression test"
qqq image add 1 ./screenshot.png
qqq message 1 "Reproduced on fresh account"
qqq list
qqq show 1
```

Run `qqq add` without a title to compose a task in the terminal editor. First line
becomes the title; remaining lines become the description. Surrounding whitespace
is trimmed. `qqq edit <id>` opens the same editor with existing content.

Ctrl-S saves; Esc or Ctrl-C cancels. Enter inserts a newline. Arrows, Home/End
(also Ctrl-A/Ctrl-E), Backspace and Delete edit the draft. The viewport follows
the cursor and adapts to terminal resizing. Editor UI uses stderr; stdout contains
only the final command result, including JSON when `--json` is supplied.

Paste text normally through a terminal supporting bracketed paste, or use Ctrl-V
to read the desktop clipboard. Pastes over 1,000 Unicode characters show a compact
`[Pasted text #N: X chars]` placeholder. Full text expands on save; existing large
descriptions start collapsed too. Ctrl-V prefers clipboard images, encoding them
as PNG. Pasting a local image path (including a shell-quoted path with spaces)
adds an image attachment and shows `[Image #N: filename]`. Place images below the
title line. Left/Right cross placeholders as one unit; Backspace/Delete removes
the whole placeholder and its payload. Saved descriptions use `[Image: filename]`
markers; image bytes are stored in `qqq.db` and remain exportable.

Task fields and newly pasted images save in one transaction. Cancel and errors
leave task data unchanged. Existing attachments stay attached when editing.
Clipboard access requires a desktop clipboard (macOS, X11 or supported Wayland);
clipboard errors appear in the editor and keep the draft. Terminals without
bracketed paste can use Ctrl-V for collapsed text pastes.

Use `--edit` (`-e`) on `add` or `edit` to open `$EDITOR` instead, including prefilled
field flags:

```sh
export EDITOR='vim'
qqq add
qqq add "Fix login" --description "Reproduce expiry" --edit
qqq edit 1 --edit
qqq edit 1 --edit --title "Prefilled title"
```

Nonterminal interactive calls also use `$EDITOR`. `EDITOR` is required for this
external-editor input and runs through `sh`, supporting quoted
executable paths and arguments such as `EDITOR='code --wait'`. Use an editor that
waits until editing finishes. Editor output goes to stderr; stdout contains only
the command result (JSON when `--json` is supplied).
An empty first line, nonzero editor exit, or unreadable draft aborts creation.
Temporary drafts are removed on success or error. Inline `qqq add "Title"` works
without an editor.

Edit an existing task with `qqq edit <id>` (replaces `describe`). With no update
flags, the terminal editor opens with the current title on the first line and
description below. Save to update both fields. `--edit` forces the external editor.
The same draft format and error handling as `add` apply.

```sh
qqq edit 1
qqq edit 1 --title "Fix expired login"
qqq edit 1 --description "Updated details"
qqq edit 1 --title "Fix login" -d "Updated details"
qqq edit 1 --description ""  # clear description
qqq edit -1                  # edit newest created task
qqq edit -2 --title "Updated" # edit second newest created task
```

The first argument is a positive task ID or a negative creation index (`-1`
means newest, `-2` second newest). Recent indexes count existing tasks across all
statuses in descending ID order, not last-edited order. Zero, missing IDs, and
out-of-range indexes fail before opening the editor. The target is resolved once
before editing, so tasks created while the editor is open do not change it.

Field flags skip the editor and preserve omitted fields. Titles cannot be blank.
Existing multiline titles require field flags because the editor format uses one title line.
Field-only editing preserves task status and ownership. Dependencies, messages and
attachments are preserved by all edits.

Use `qqq edit <id> --set-status new` to return a claimed task to the queue
(replaces `release`). Only `new` is accepted for now. The task must be
`in_progress`, and the supplied or discovered session ID must match its recorded
`assignee`. This clears the owner and records a `release` history event.
New or completed tasks cannot use this transition.

```sh
qqq edit 1 --set-status new --session agent-session-123
qqq edit -1 --set-status new --description "Retry with updated details" --session agent-session-123
```

`--set-status` skips the editor. Combined title, description and status updates
are atomic: validation or ownership errors leave all fields and history unchanged.

Add a dependency with `--parent <task-id>`:

```sh
qqq add "Build API"                         # returns task ID, e.g. 1
qqq add "Build client" --parent 1
qqq add --parent 1                          # compose dependent task interactively
```

Parent must already exist. Each task has one optional parent, fixed at creation.
Task JSON includes `parent_id` (`null` for independent tasks). Child stays new
until parent completes; `next` skips blocked children and claims the oldest ready
task. Releasing a parent keeps children blocked. Dependency chains unlock in
order. If every new task is blocked, `next` prints `No ready tasks.`
(`null` with `--json`).

Images support PNG, JPEG, GIF and WebP signatures, up to 20 MiB each. Signature checking identifies format; it does not fully decode or validate image contents. Import copies bytes into DB, so original file can be removed. Export takes **image ID**, shown by `show`, rather than task ID; destination must not exist.

```sh
qqq image export 1 ./exported.png
```

## Config

Read or edit `~/.config/qqq/config.toml` without opening a project database:

```sh
qqq config --list
qqq config --get alias.ls
qqq config alias.ls                  # same as --get
qqq config alias.ls 'list --watch'    # set
qqq config herdr.next-to-new-agent true
qqq config display.max-completed 10
qqq config --unset alias.ls
```

Keys use TOML dotted-key syntax. Quote literal dots inside key segments:
`qqq config 'alias."with.dot"' list`. Alias values always stay strings, including
`true` or `123`; `herdr.next-to-new-agent` accepts only `true` or `false`.
`display.max-completed` requires a non-negative integer up to `i64::MAX`.
Other values accept TOML literals (numbers, booleans, arrays, quoted strings),
falling back to a string when input is not a valid literal. Use `--` before
positional arguments that start with a hyphen.

`--list` shows explicitly stored values as sorted `key=value` lines. Missing
config lists no values; reads never create files. Missing keys, invalid paths,
unset/empty `HOME`, malformed files and invalid known setting types fail with
exit code 1. Actions are mutually exclusive. Updates preserve comments, unknown
settings, file permissions and existing symlinks. Concurrent qqq edits use a
persistent sidecar lock and atomic replacement; manual editors do not share this
lock.

With `--json`, list returns a flat object of dotted keys and typed values. Get
returns the value itself. Set returns `{"key":"alias.ls","value":"list"}`;
unset returns `{"key":"alias.ls","value":null}`. Unsetting a setting restores
its default behavior.

## Aliases

Define command shortcuts in `~/.config/qqq/config.toml`:

```toml
[alias]
ls = "list"
n = "next"
done = "complete"
bug = "add --description 'Needs investigation'"
img = "image add"
```

`qqq bug "Fix login"` expands to `qqq add --description 'Needs investigation'
"Fix login"`. Extra arguments keep their original boundaries. Single/double
quotes and backslash escaping group words in alias values. No shell runs: `$HOME`,
wildcards, and command substitutions stay literal; Git-style `!` aliases are
rejected. Aliases can reference other aliases; cycles, empty values, and invalid
quoting produce errors before any task changes.

Global `--session` works before or after an alias. Built-in commands (including
`help`) always take precedence; only the root command is expanded. Use
`qqq bug --help` for the expanded command's help. Unknown commands retain normal
CLI errors. Alias config is read when resolving an unknown root command; human
`list` also reads display config unless `--all` or `--max-completed` is supplied.
Top-level help and built-ins such as `show` and `list --json` remain usable with
broken config. `next` reads Herdr dispatch settings; `next --local` bypasses that
read. Missing config or unset/empty `HOME` uses defaults; unreadable or malformed config reports its
path. This fixed path uses `HOME`, not `XDG_CONFIG_HOME`.

## Agent ownership

Use a stable, unique ID for each agent session:

```sh
export QQQ_SESSION="agent-session-123"
qqq next
qqq message 1 "Implementation ready; running tests"
qqq complete 1
```

Or pass `--session agent-session-123` on individual commands. Precedence: `--session`, `QQQ_SESSION`, exact Herdr caller identity when `HERDR_ENV=1`, then unique Herdr agent at the database directory.

- `next` returns oldest ready new task, atomically marking it `in_progress`.
- `next --wait` waits until a task can be claimed, returning the task once available.
- Same session calling `next` (with or without `--wait`) again receives its existing task.
- One active task per session; concurrent sessions cannot claim same task.
- Without `--wait`, no ready tasks prints `No ready tasks.` (`null` with `--json`), exit code 0.
- `complete <task-id>` marks task `completed` when the supplied or discovered session ID matches its recorded `assignee`.
- `edit <task-id> --set-status new` returns a claimed task to `new` when the supplied or discovered session ID matches its recorded `assignee`.
- Claims never expire. Restarting CLI preserves locks. `show` includes claim/release/completion history.

Use `--wait` for workers that should stay ready when the queue is empty:

```sh
qqq next --wait --session worker-1
qqq next --wait --json --session worker-2
```

Waiters check every 250 ms. Each check uses the same atomic claim transaction;
only one worker session receives each task, while other waiters keep waiting.
Use a unique session ID for each worker. Existing claims return immediately for
that session. A task becomes ready when added, returned to `new`, or unblocked
by parent completion. Waiting prints no intermediate output; use Ctrl-C to stop.
Identity lookup or DB errors still exit with an error. Waiting releases the DB
write lock between checks, so other commands can add and update tasks.

For an abandoned session, inspect `qqq show <id>`, then explicitly release using its recorded `assignee`:

```sh
qqq edit 1 --set-status new --session 'recorded-owner-session'
```

Session IDs coordinate local agents; they are not authentication credentials. Descriptions, attachments and messages can be edited/appended by local callers regardless of claim ownership. Messages record author when `--session` or `QQQ_SESSION` is supplied.

## Herdr

Outside Herdr, plain `qqq next` queries `herdr agent list` on the currently targeted server and selects the unique agent whose cwd equals the directory containing `qqq.db`. No `HERDR_ENV` needed. Running from a project subdirectory still matches the DB directory. Paths resolve symlinks; `foreground_cwd` takes precedence over `cwd` when present. Zero matches, multiple matches, or missing session and terminal identity produce an error before claiming. Use `--session` to choose ownership explicitly when discovery is ambiguous.

`complete`, `edit --set-status new`, and `herdr link <id>` use the same discovery. Lookup requires Herdr CLI and its server to be available.

Inside Herdr, `next` derives ownership from `HERDR_ENV=1`, `HERDR_PANE_ID`, and
the exact pane's reported agent session. When hooks have not reported a session,
it uses the stable Herdr terminal ID plus agent kind instead. The same fallback
applies to unique-cwd discovery outside Herdr. Reported session identity takes
priority for new claims. Active auto-claims keep their saved identity until
completion when hooks appear or disappear in the same terminal, including when
switching between local and dispatch mode. Malformed metadata fails before
claiming. Terminal identity follows
the terminal lifetime, so resumed conversations in that terminal share it.
Use `--session` when a distinct conversation identity is needed.

The claim and Herdr link persist in one transaction. Caller context takes
precedence over cwd discovery; caller lookup errors do not fall through to
another agent. `complete` and `edit --set-status new` preserve active claim
identity the same way. Default `herdr link` also preserves active auto-claim
identity; explicit links associate the requested identity without transferring
ownership.

```sh
# With no QQQ_SESSION override: inside Herdr, or unique agent at DB directory:
qqq next
qqq herdr link 1
qqq herdr find 1

# Explicit link to a live agent session:
qqq herdr link 1 --agent codex --agent-session session-123
# Named Herdr server:
qqq herdr link 1 --agent codex --agent-session session-123 --server work
```

`find` matches saved agent/session identity against `herdr agent list`, returning current `workspace_id`, `tab_id`, `pane_id`, and `agent_session`. Pane moves do not break identity matching. Missing, offline or ambiguous matches produce errors; saved link remains available in `show`. Automatic ownership uses a JSON tuple of agent, identity kind and value; keep the same identity mode through completion or returning a task to new.

With dispatch disabled, explicit `--session` / `QQQ_SESSION` ownership works without Herdr and does not auto-link. `herdr link` stores association separately; it does not transfer ownership. A completed task retains its latest link. A fresh claim clears the previous link, then saves the new auto-detected link when available; explicit owners should link after claiming. Default-server links query whichever Herdr server CLI currently targets; use `--server` for a stable named-server target.

### Dispatch next task to a new agent

Opt in through `~/.config/qqq/config.toml`:

```toml
[herdr]
next-to-new-agent = true
```

`qqq next` first returns the caller's existing active task. Otherwise it reserves
the oldest ready task for a new Codex agent, creates an unfocused tab in the
calling Herdr workspace with the same working directory, starts the agent, saves
its Herdr association, and submits a prompt to handle that specific task.
Empty or dependency-blocked queues create no tabs; `next --wait` waits for ready
work before creating an agent. The returned task's `assignee`
is the new agent's unique session; that value is passed as `QQQ_SESSION` and
included explicitly in its completion instructions.

New-agent creation requires `HERDR_ENV=1` and `HERDR_WORKSPACE_ID`.
`qqq next --local` bypasses dispatch, even when enabled, and keeps the normal
caller claim behavior. The setting defaults to false. JSON remains a task or
`null`; inspect `qqq show <id>` / `qqq herdr find <id>` for the saved/live link.

When Herdr reports an agent-session identity, qqq saves it. When hooks omit that
identity, qqq associates the exact Herdr terminal ID and agent kind instead.
This fallback follows the terminal lifetime, not an individual resumed Codex
conversation. Caller discovery in dispatch mode supports the same fallback.

Failures before prompt submission return the task to `new`. Created tabs stay
open for inspection. A prompt error keeps the claim and link because delivery
may already have happened: inspect the agent before retrying. To recover after
confirming the agent is not working, use the recorded assignee:

```sh
qqq edit <task-id> --set-status new --session '<recorded-assignee>'
```

Adapter targets Herdr API protocol 20 JSON shapes: `result.agents`, `result.pane`, `agent_session.{agent,kind,value}`. Agent-session metadata may be absent depending on integration hooks.

## Data and checks

Initial status is `new` (displayed as `New`); use `--set-status new` to return a
claimed task to the queue. Existing databases using `pending` require a manual
schema/data update; this rename adds no automatic migration or schema version bump.

Task assignment is exposed as `assignee` in JSON and `Assignee:` in human output.
Its value is the claiming session ID, or `null` for unassigned tasks. JSON clients
should use `assignee` in place of the former `owner_session` field. Schema version
3 automatically migrates existing version 1/2 databases on open, preserving
claims, task data, history, attachments, and links. The `--session` flag and
session matching rules are unchanged.

DB includes tasks, messages, image blobs, ownership events, latest Herdr link per task. SQLite foreign keys, immediate write transactions, unique active-owner index and 10-second busy timeout protect concurrent claims. Tasks from version 1 databases have no parent after migration. Newer unknown versions are rejected. Keep DB out of Git. To back up while CLI processes may run, use SQLite's backup API or `sqlite3 qqq.db '.backup backup.sqlite'`; copy DB file only when all writers are stopped.

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

GitHub Actions runs these checks on Linux (Ubuntu 24.04) and macOS (macOS 14)
for pushes, pull requests, and manual runs. Each job builds and smoke-tests the
release binary, then uploads a `qqq-<OS>-<ARCH>` artifact containing a tar.gz
archive. Download artifacts from the workflow run within 14 days; extract the
archive to preserve the binary's executable permission. Builds use stable Rust
and the committed lockfile. This workflow builds artifacts; it does not publish
GitHub Releases.

Tests exercise persistence, FIFO order, wrong-owner rejection, concurrent claims, attachments, ancestor lookup, cwd discovery, Herdr session matching and automatic ownership using a fake Herdr executable.
