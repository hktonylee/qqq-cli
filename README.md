# qqq

Local-first Rust CLI for project tasks and coding agent sessions. Everything lives in **`qqq.db`**, including image bytes. No server required. Commands print human-readable text by default; use `--json` for scripts and agents. Errors go to stderr with exit code 1 (argument errors: 2).

## Output

`qqq list` shows task IDs, status, and first description lines as a dependency tree. Children appear
below their parent; roots and siblings follow creation order. Branches replace
the parent column:

```text
ID     STATUS       TASK
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
in-progress, grey for completed tasks, and red for tasks in error. Aliases such as `ls = "list"` use the
same colors. Piped output and `--json` stay plain. Set `NO_COLOR=1` or `TERM=dumb`
to disable colors. `show` puts task ID/status and the full description first,
then groups aligned details and assignment fields. Message bodies are indented
beneath their headers; collection headings show counts. Terminal `show` uses the
same status colors, bold cyan headings, muted labels/timestamps, and colored
history actions (cyan claim, yellow release, grey complete, red error). Body text
keeps the terminal's normal color. Piped output, `NO_COLOR`, and `TERM=dumb` keep
the same layout without styling; JSON keeps the complete original data.

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

Install the `qqq` command from crates.io:

```sh
cargo install qqq-cli --locked
```

To install from a source checkout, run in the repository root:

```sh
cargo install --path . --locked
```

## Tasks

Run `init` in your project root. Other commands find the nearest `qqq.db` in current directory or its parents. Filename is fixed.

```sh
qqq init
qqq add --description "Fix login

Show useful error when credentials expire"
qqq edit 1 --description "Reproduce expiry, fix retry, add regression test"
qqq edit 1 --image ./screenshot.png
qqq message 1 "Reproduced on fresh account"
qqq list
qqq show 1
```

Run `qqq add` without text to compose a task in the terminal editor. The whole
buffer becomes one description: no separate title, splitting or trimming.
`qqq edit <id>` opens the same editor with the complete existing description.
Inline `qqq add TEXT` and `qqq add --description TEXT` are equivalent; use one
form at a time. Blank first lines are allowed; the whole body must contain text.
Leading/trailing whitespace and final newlines are preserved. Human list/tree/
watch output previews only the first line; `show` displays the whole body. JSON
keeps the full `description` and has no `title` field.

Ctrl-S saves. Esc asks to discard when buffer contains text or images: Y confirms;
N, Enter or Esc keeps editing. Empty-buffer Esc exits directly. Ctrl-C cancels
immediately. Enter inserts a newline. Arrows, Home/End
(also Ctrl-A/Ctrl-E), Backspace and Delete edit the draft. The viewport follows
the cursor and adapts to terminal resizing. Editor UI uses stderr; stdout contains
only the final command result, including JSON when `--json` is supplied.

Built-in add/edit editors use a dark-grey background with light text within
the editing area, including blank rows. Header, footer, errors and resize hints
use default terminal colors. Terminal colors reset on save, cancel
or error. `NO_COLOR=1` or `TERM=dumb` keeps the editor plain. External `$EDITOR`
commands use their own theme.

Paste text normally through a terminal supporting bracketed paste, or use Ctrl-V
to read the desktop clipboard. Pastes over 1,000 Unicode characters show a compact
`[Pasted text #N: X chars]` placeholder. Full text expands on save; existing large
descriptions start collapsed too. Ctrl-V prefers clipboard images, encoding them
as PNG. Pasting a local image path (including a shell-quoted path with spaces)
adds an image attachment and shows `[Image #N: filename]`. Images can appear
anywhere in the buffer, including the first line. Left/Right cross placeholders as one unit; Backspace/Delete removes
the whole placeholder and its payload. Saved descriptions use `[Image: filename]`
markers; image bytes are stored in `qqq.db` and remain exportable.

Task fields and newly pasted images save in one transaction. Cancel and errors
leave task data unchanged. Existing attachments stay attached when editing.
Clipboard access requires a desktop clipboard (macOS, X11 or supported Wayland);
clipboard errors appear in the editor and keep the draft. Terminals without
bracketed paste can use Ctrl-V for collapsed text pastes.

Use `--edit` (`-e`) on `add` or `edit` to open `$EDITOR` instead, including prefilled
description flags:

```sh
export EDITOR='vim'
qqq add
qqq add --description "Fix login

Reproduce expiry" --edit
qqq edit 1 --edit
qqq edit 1 --edit --description "Prefilled description"
```

Nonterminal interactive calls also use `$EDITOR`. `EDITOR` is required for this
external-editor input and runs through `sh`, supporting quoted
executable paths and arguments such as `EDITOR='code --wait'`. Use an editor that
waits until editing finishes. Editor output goes to stderr; stdout contains only
the command result (JSON when `--json` is supplied).
A blank whole buffer, nonzero editor exit, or unreadable draft aborts creation.
Temporary drafts are removed on success or error. Inline `qqq add "Task description"` works
without an editor.

Edit an existing task with `qqq edit <id>` (replaces `describe`). With no update
flags, the terminal editor opens with the complete description. Save to replace
that body. `--edit` forces the external editor.
The same draft format and error handling as `add` apply.

```sh
qqq edit 1
qqq edit 1 --description "Updated details"
qqq edit -1                  # edit newest created task
qqq edit -2 --description "Updated" # edit second newest created task
```

`qqq show -1` also opens the newest created task; `qqq show -2` opens the second newest.
Show supports the same indexes with image export, resolving the selected task once.

The first argument to `edit` or `show` is a positive task ID or a negative creation
index (`-1` means newest, `-2` second newest). Recent indexes count existing tasks across all
statuses in descending ID order, not last-edited order. Zero, missing IDs, and
out-of-range indexes fail before opening the editor. The target is resolved once
before editing, so tasks created while the editor is open do not change it.

Description flags skip editor, replace complete body. Omitted body stays unchanged
during image/status/parent edits. Blank-only descriptions are rejected. Editing
content or attachments preserves status and ownership unless a status flag is
supplied. Messages and existing attachments remain; dependencies change only
through `--set-parent`.

Use `qqq edit <id> --set-status new` to return a claimed task to the queue
(replaces `release`). For an active task, it must be
`in_progress`, and the supplied or discovered session ID must match its recorded
`harness_session`. This clears the owner and records a `release` history event.
New or completed tasks cannot use this transition.

```sh
qqq edit 1 --set-status new --session agent-session-123
qqq edit -1 --set-status new --description "Retry with updated details" --session agent-session-123
```

For manual recovery, inspect the task and return it without discovering or
matching its current owner:

```sh
qqq show 1
qqq edit 1 --set-status new --force
qqq edit 1 --set-status new --force --session recovery-operator --description "Ready to retry"
```

`--force` skips Herdr lookup, including when neither `--session` nor
`QQQ_SESSION` is set. A supplied session records the release author without
enforcing ownership; otherwise history uses `manual`. `--harness-session` also
supplies the author when no `--session`/`QQQ_SESSION` is set. It clears the claim
and all harness/orchestrator identity fields,
records a `release` event, and preserves omitted content, dependencies,
messages, attachments and the saved Herdr link. Combined field, parent and
image updates commit atomically with the release. It accepts active or error
tasks; new/completed tasks reject the transition. The flag is valid only with
literal `--set-status new`, not `--set-pending`, `--set-status error`, field-only
edits or `--edit`. Without `--force`, active tasks still require their owner;
error retries remain session-free.

`--set-pending` is a shortcut for `--set-status new`, returning work to the
execution queue using the existing `new` status:

```sh
qqq edit 1 --set-pending --session agent-session-123
qqq edit -1 --set-pending --description "Retry with updated details" --session agent-session-123
```

It skips the editor, clears active ownership, records a `release` history event,
and preserves omitted content, dependencies, messages and attachments. Combined
field, parent and image updates are atomic; wrong owner or invalid input leaves
everything unchanged. It follows the same rules as `--set-status new`: owner is
required for active tasks; any local caller can retry an error task without a
session. New/completed tasks reject the transition. `next` can claim returned
work once its current dependency is satisfied. `--set-pending` conflicts with
`--set-status` and `--edit`.

`--set-status` skips the editor. Combined description and status updates
are atomic: validation or ownership errors leave all fields and history unchanged.

Mark failed work that needs manual handling with `error` and a required reason:

```sh
qqq edit 1 --set-status error --reason "Missing API credentials" --session agent-session-123
qqq show 1
# After handling failure, explicitly return it to the queue:
qqq edit 1 --set-status new
```

Only the owner of an `in_progress` task can mark it `error`. This clears its
assignment, saves the reason as a message, and records an `error` history event.
Blank reasons fail; `--reason` is valid only with `--set-status error`. Optional
description edits commit together with the failure. Error tasks stay
visible in `list`, even with `--max-completed 0`; `show` includes the reason.
`next` and `next --wait` skip them, and dependent tasks stay blocked. The worker
can claim another ready task.

Any local caller can retry an error task through `--set-status new` or
`--set-pending`, without a
session or Herdr connection. An explicit session records the retry author;
otherwise history uses `manual`. Retry preserves content, dependencies,
messages, attachments and saved Herdr link. Generic CLI errors do not mark
tasks error automatically. Dispatch startup failures still return tasks to new;
ambiguous prompt-delivery failures retain their active claim for inspection.

Add a dependency with `--parent <task-id>`:

```sh
qqq add "Build API"                         # returns task ID, e.g. 1
qqq add "Build client" --parent 1
qqq add --parent 1                          # compose dependent task interactively
```

Parent must already exist. Each task has one optional parent.
Task JSON includes `parent_id` (`null` for independent tasks). Child stays new
until parent completes; `next` skips blocked children and claims the oldest ready
task. Releasing a parent keeps children blocked. Dependency chains unlock in
order. If every new task is blocked, `next` prints `No ready tasks.`
(`null` with `--json`).

Change or clear a dependency after creation:

```sh
qqq edit 2 --set-parent 1
qqq edit 2 --set-parent none
qqq edit -1 --set-parent 1 --description "Updated dependency details"
qqq edit 2 --edit --set-parent 1  # compose content while changing dependency
```

`--set-parent` accepts a positive existing task ID or `none`; omitted flag keeps
the current parent. Parent-only edits skip the editor. Self-parenting and
dependency cycles fail without saving changes. Parent validation and updates
share the same transaction as content, attachments and optional status updates,
including forced editor input. Concurrent edits cannot create a cycle.

Changing a parent preserves task status, assignment metadata, messages, history, attachments
and Herdr link. Already claimed work remains assigned. Fresh `next` claims,
including `--wait`, use the updated dependency: incomplete parents block queued
tasks, completed parents unblock them, and `none` makes queued tasks independent.

Attach images through `add` or `edit`; repeat `--image PATH` for multiple files.
Image-only edits skip the editor and preserve existing fields and attachments.
During interactive add, flagged files save alongside pasted TUI images. Use
`--edit` to combine flagged attachments with a prefilled `$EDITOR` draft.
Images support PNG, JPEG, GIF and WebP signatures, up to 20 MiB each. Signature
checking identifies format; it does not fully decode or validate image contents.
Files must be regular files. Bytes are copied into DB, so source can be removed.
Fields, status, history and all new attachments save together; any failure leaves
task unchanged. Cancelled or failed composition saves nothing.

Export through `show TASK --export-image IMAGE --output PATH`. Image ID comes
from `show`; it must belong to selected task. Destination must not exist. Normal
task details still print; JSON additionally includes `export` metadata (image ID,
path, bytes). Ordinary `show` JSON stays unchanged.

```sh
qqq add "Investigate screenshot" --image ./before.png --image ./after.jpg
qqq edit -1 --image ./extra.png
qqq show 1 --export-image 1 --output ./exported.png
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
bug = "add"
img = "edit -1 --image"
```

`qqq bug "Fix login"` expands to `qqq add "Fix login"`. Extra arguments keep
their original boundaries. Single/double
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

Or pass `--session agent-session-123` on individual commands. Automatic ownership uses this precedence:

1. Explicit `--session`, then `QQQ_SESSION`.
2. Exact Herdr pane when `HERDR_PANE_ID` is present, or `HERDR_ENV=1`.
3. Native Codex `CODEX_THREAD_ID`, then `CODEX_SESSION_ID` fallback.
4. Unique Herdr agent at the database directory.

Outside exact Herdr context, Codex can run `qqq next --local`, `qqq complete`,
`qqq edit --set-pending` and `qqq edit --set-status error` without setting
`QQQ_SESSION` or installing Herdr. Native claims expose `harness_name=codex` and
the raw ID as `harness_session`; orchestrator fields remain null unless overridden.
The private owner key is a JSON tuple of client, identity kind and ID. Both Codex
variables with the same ID resolve to the same claim; when both are set, thread ID
wins. Selected values must be UTF-8 and nonblank; invalid selected values fail
before claiming. Higher-priority sources ignore lower-priority values.

Other AI clients use Herdr's reported agent/session identity or explicit
`--session` / `QQQ_SESSION`; qqq does not guess undocumented client env variables.
`--harness-name` / `--harness-session` and orchestrator overrides remain available.

- `next` returns oldest ready new task, atomically marking it `in_progress`.
- `next --wait` waits until a task can be claimed, returning the task once available.
- Same session calling `next` (with or without `--wait`) again receives its existing task.
- One active task per session; concurrent sessions cannot claim same task.
- Without `--wait`, no ready tasks prints `No ready tasks.` (`null` with `--json`), exit code 0.
- `complete <task-id>` marks task `completed` when the supplied or discovered session ID matches its recorded `harness_session`.
- `edit <task-id> --set-status new` returns a claimed task to `new` when the supplied or discovered session ID matches its recorded `harness_session`.
- `edit <task-id> --set-status new --force` returns active/error work without owner discovery or matching, recording the supplied session or `manual` as release author.
- `edit <task-id> --set-pending` is the same return-to-queue transition, using existing `new` status and ownership/history rules.
- `edit <task-id> --set-status error --reason "Details"` marks owned active work as failed, clearing its claim until a user explicitly retries with `--set-status new`.
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

For an abandoned session, inspect `qqq show <id>`, then explicitly return it to the queue:

```sh
qqq edit 1 --set-status new --force
```

Session IDs coordinate local agents; they are not authentication credentials. Descriptions, attachments and messages can be edited/appended by local callers regardless of claim ownership. Messages record author when `--session` or `QQQ_SESSION` is supplied.

## Herdr

When no explicit, exact-pane or native Codex identity is available, plain `qqq next` queries `herdr agent list` on the currently targeted server and selects the unique agent whose cwd equals the directory containing `qqq.db`. No `HERDR_ENV` needed. Running from a project subdirectory still matches the DB directory. Paths resolve symlinks; `foreground_cwd` takes precedence over `cwd` when present. Zero matches, multiple matches, or missing session and terminal identity produce an error before claiming. Use `--session` to choose ownership explicitly when discovery is ambiguous.

`complete` and ordinary active-task `edit --set-status new` use the same caller resolution as `next`. Native Codex ownership works without Herdr. `herdr link <id>` always uses Herdr discovery, requiring its CLI and server. `edit --set-status new --force` and error retries skip discovery.

With `HERDR_PANE_ID` present (even without `HERDR_ENV=1`), `next` queries
`herdr pane current --pane <HERDR_PANE_ID>` and derives ownership from that
pane's reported agent session. `HERDR_ENV=1` also selects exact Herdr context
and requires a valid pane ID. When hooks have not reported a session,
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
another agent. `complete` and ordinary `edit --set-status new` preserve active claim
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

`find` matches saved agent/session identity against `herdr agent list`, returning current `workspace_id`, `tab_id`, `pane_id`, and `agent_session`. Pane moves do not break identity matching. Missing, offline or ambiguous matches produce errors; saved link remains available in `show`. Automatic Herdr ownership uses a JSON tuple of agent, identity kind and value; keep the same identity mode through completion or returning a task to new.

With dispatch disabled (or `next --local`), explicit `--session` / `QQQ_SESSION` and native Codex ownership work without Herdr and do not auto-link. Manual `herdr link` still discovers Herdr independently of native Codex env. `herdr link` stores association separately; it does not transfer ownership. A completed task retains its latest link. A fresh claim clears the previous link, then saves the new auto-detected link when available; explicit owners should link after claiming. Auto-discovered links retain named Herdr server when available; explicit links support `--server` for a stable target.

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
work before creating an agent. The returned task's `harness_name` and `harness_session` identify the new Codex
agent. A separate generated claim token is passed as `QQQ_SESSION` and included
in completion instructions. Both that token and the displayed harness session
can recover the claim.

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
confirming the agent is not working, use the recorded harness session:

```sh
qqq edit <task-id> --set-status new --session '<recorded-harness-session>'
```

Adapter targets Herdr API protocol 20 JSON shapes: `result.agents`, `result.pane`, `agent_session.{agent,kind,value}`. Agent-session metadata may be absent depending on integration hooks.

## Data and checks

Initial status is `new` (displayed as `New`); use `--set-status new` to return a
claimed task to the queue. Existing databases using `pending` require a manual
schema/data update; this rename adds no automatic migration or schema version bump.

Task assignment exposes four nullable JSON fields: `harness_name`,
`harness_session`, `orchestrator_name`, `orchestrator_session`. Human task details
show each field; human `add` output omits these empty assignment fields. JSON
always includes all four fields. `assignee` and `owner_session` are absent. Herdr auto-fill uses
agent kind/session (terminal ID fallback), orchestrator `herdr`, named Herdr
server session. Native Codex auto-fill uses `CODEX_THREAD_ID` / `CODEX_SESSION_ID` for harness fields, leaving orchestrator fields null. Optional server discovery failure leaves its session null.

Override fields when claiming or linking:

```sh
qqq next --local --session stable-key --harness-name codex \
  --harness-session agent-session --orchestrator-name herdr \
  --orchestrator-session default
```

`--session`/`QQQ_SESSION` remain stable ownership inputs and bypass automatic
automatic ownership discovery. Without those inputs, `--harness-session` first
recovers a matching active claim; otherwise it overrides auto-discovered session
metadata, with local explicit ownership fallback when discovery fails. Other
flags override auto-filled metadata. Metadata overrides do not change saved
Herdr lookup identity. Repeated `next` applies explicit overrides to existing
claim. Content/attachment edits preserve metadata; completion/release/error clear
current assignment fields and retain history/link.

Ownership commands accept original token or displayed harness session. Exact
legacy token takes priority; ambiguous public sessions require `--harness-name`
or original token. For example:

```sh
qqq complete 1 --harness-session agent-session --harness-name codex
```

Schema version 5 automatically migrates compatible single-description version
1/2/3/4 databases, preserving
claims, error tasks, dependencies, timestamps, history, images, links and
sequence high-water marks. Internal `claim_key` retains existing ownership;
linked active claims populate public fields from saved agent identity/server.
Unknown legacy names/server sessions remain null. Legacy title schemas still require manual text/schema conversion before opening.
JSON clients should replace `assignee` with harness fields.

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

### Publish to crates.io

`.github/workflows/publish.yml` publishes when a `v<version>` tag is pushed.
It checks that the tag matches `Cargo.toml`, runs formatting, Clippy and tests,
then builds the packaged crate with `cargo publish --locked --dry-run` before
uploading. Publishing uses the committed lockfile and targets crates.io.

Before the first publish:

1. Sign in to crates.io, verify your email, and create a token with permission
   to publish `qqq-cli`, including permission to create it for the first release.
2. Add the token as the GitHub repository Actions secret
   `CARGO_REGISTRY_TOKEN`. The token is available only to the upload step.

The package is named `qqq-cli` because `qqq` is already registered on crates.io.
It ships the `qqq` binary under the MIT license.

Commit the intended version and updated lockfile, then push the commit and
matching tag. For the current `0.1.0` version:

```sh
git push origin master
git tag v0.1.0
git push origin v0.1.0
```

To validate an existing tag without uploading, run **Publish to crates.io** from
GitHub Actions with `tag` set to that tag and leave `dry_run` enabled (the default).
Disable `dry_run` to publish or retry an unpublished version. Each crates.io
version can be uploaded only once; bump the version and update the lockfile
before creating the next tag. No secret is needed for a dry run.

Tests exercise persistence, FIFO order, wrong-owner rejection, concurrent claims, attachments, ancestor lookup, cwd discovery, Herdr session matching and automatic ownership using a fake Herdr executable.


Databases containing the old `title` column require a manual SQLite update; no
new migration was added for single-description storage. Back up first, combine
old title and body with a blank line, then remove the old column while preserving
task IDs, ownership, dependencies and related tables. qqq rejects the old schema
before writing rather than silently discarding content.
