# qqq

Local-first Rust CLI for project tasks and coding agent sessions. Everything lives in **`qqq.sqlite`**, including image bytes. No server required. Commands return JSON; errors go to stderr with exit code 1 (argument errors: 2).

## Install

Requires Rust 1.85+ and a C compiler for bundled SQLite.

```sh
cargo install --path . --locked
```

## Tasks

Run `init` in your project root. Other commands find the nearest `qqq.sqlite` in current directory or its parents. Filename is fixed.

```sh
qqq init
qqq add "Fix login" --description "Show useful error when credentials expire"
qqq describe 1 "Reproduce expiry, fix retry, add regression test"
qqq image add 1 ./screenshot.png
qqq message 1 "Reproduced on fresh account"
qqq list
qqq show 1
```

Images support PNG, JPEG, GIF and WebP signatures, up to 20 MiB each. Signature checking identifies format; it does not fully decode or validate image contents. Import copies bytes into DB, so original file can be removed. Export takes **image ID**, shown by `show`, rather than task ID; destination must not exist.

```sh
qqq image export 1 ./exported.png
```

## Agent ownership

Use a stable, unique ID for each agent session:

```sh
export QQQ_SESSION="agent-session-123"
qqq next
qqq message 1 "Implementation ready; running tests"
qqq complete 1
```

Or pass `--session agent-session-123` on individual commands. Precedence: `--session`, `QQQ_SESSION`, exact Herdr caller identity when `HERDR_ENV=1`, then unique Herdr agent at the database directory.

- `next` returns oldest pending task, atomically marking it `in_progress`.
- Same session calling `next` again receives its existing task.
- One active task per session; concurrent sessions cannot claim same task.
- Empty queue returns JSON `null`, exit code 0.
- `complete <task-id>` requires current owner, marks task `completed`.
- `release <task-id>` requires current owner, returns task to `pending`.
- Claims never expire. Restarting CLI preserves locks. `show` includes claim/release/completion history.

For an abandoned session, inspect `qqq show <id>`, then explicitly release using its recorded `owner_session`:

```sh
qqq release 1 --session 'recorded-owner-session'
```

Session IDs coordinate local agents; they are not authentication credentials. Descriptions, attachments and messages can be edited/appended by local callers regardless of claim ownership. Messages record author when `--session` or `QQQ_SESSION` is supplied.

## Herdr

Outside Herdr, plain `qqq next` queries `herdr agent list` on the currently targeted server and selects the unique agent whose cwd equals the directory containing `qqq.sqlite`. No `HERDR_ENV` needed. Running from a project subdirectory still matches the DB directory. Paths resolve symlinks; `foreground_cwd` takes precedence over `cwd` when present. Zero matches, multiple matches, or missing agent-session metadata produce an error before claiming. Use `--session` to choose ownership explicitly when discovery is ambiguous.

`complete`, `release`, and `herdr link <id>` use the same discovery. Lookup requires Herdr CLI and its server to be available.

Inside Herdr, `next` can derive ownership from `HERDR_ENV=1`, `HERDR_PANE_ID`, and the exact pane's reported agent session. The claim and Herdr link persist in one transaction. Caller context takes precedence over cwd discovery; caller lookup errors do not fall through to another agent. If hooks have not reported session identity, supply an explicit session and link once identity becomes available.

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

`find` matches saved agent/session identity against `herdr agent list`, returning current `workspace_id`, `tab_id`, `pane_id`, and `agent_session`. Pane moves do not break identity matching. Missing, offline or ambiguous matches produce errors; saved link remains available in `show`. Automatic ownership uses a JSON tuple of agent, identity kind and value; keep the same identity mode through complete/release.

Explicit `--session` / `QQQ_SESSION` ownership works without Herdr and does not auto-link. `herdr link` stores association separately; it does not transfer ownership. A completed task retains its latest link. A fresh claim clears the previous link, then saves the new auto-detected link when available; explicit owners should link after claiming. Default-server links query whichever Herdr server CLI currently targets; use `--server` for a stable named-server target. No automatic agent spawning or prompt submission.

Adapter targets Herdr API protocol 20 JSON shapes: `result.agents`, `result.pane`, `agent_session.{agent,kind,value}`. Agent-session metadata may be absent depending on integration hooks.

## Data and checks

DB includes tasks, messages, image blobs, ownership events, latest Herdr link per task. SQLite foreign keys, immediate write transactions, unique active-owner index and 10-second busy timeout protect concurrent claims. Schema version is checked; newer unknown versions are rejected. Keep DB out of Git. To back up while CLI processes may run, use SQLite's backup API or `sqlite3 qqq.sqlite '.backup backup.sqlite'`; copy DB file only when all writers are stopped.

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

Tests exercise persistence, FIFO order, wrong-owner rejection, concurrent claims, attachments, ancestor lookup, cwd discovery, Herdr session matching and automatic ownership using a fake Herdr executable.
