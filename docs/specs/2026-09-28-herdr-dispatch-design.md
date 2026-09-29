# Herdr Task Dispatch

Config `~/.config/qqq/config.toml` supports `[herdr] next-to-new-agent = true`.
Default false preserves existing next behavior. Enabled `next` first returns caller's existing task. Only when caller has no active task does it reserve oldest
ready task under a unique generated owner.
`next --local` bypasses dispatch and returns/claims work for caller as before.
Empty/blocked queues create no Herdr resources. JSON remains Task or null.

Dispatch requires HERDR_ENV=1 and nonempty HERDR_WORKSPACE_ID. Use caller's exact
cwd in a new unfocused tab in that workspace. Start named Codex agent in returned
root pane. Pass generated QQQ_SESSION via tab environment. Read exact agent pane
with `agent get`, save its reported agent-session identity and pane location.
When hooks omit agent-session identity, associate exact terminal_id instead;
lookup also requires agent kind to match. Terminal identity tracks terminal
lifetime, not a particular resumed Codex conversation.

Reserve before spawn to prevent concurrent callers dispatching same task. Save
link before prompting. Prompt names fixed task ID, asks agent to inspect show,
implement, verify, record progress, complete with supplied session; never asks
agent to call next. Include absolute qqq executable path and shell-quoted session
and project paths. No task content is interpolated into shell command strings.

Pre-prompt failures release reservation, report task/tab details, retain created
tabs for inspection. Prompt errors retain claim/link: delivery may have occurred;
never blindly retry. Tab cleanup remains manual. DB transactions never span
Herdr calls. Owner claim and association are persisted before task work begins.

Shared config reader preserves alias behavior and lazily loads config for next.
Malformed dispatch config fails before queue changes. Config false and --local
do not spawn. Existing commands unrelated to next continue without config reads.

Tests use fake executable with recorded args and JSON responses, exercising
success, fallback/session identity, disabled/local/empty/blocked cases, startup
and prompt failure, malformed config, concurrency, and lookup after linking.
No live task is launched during verification.
