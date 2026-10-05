# Agent JSON Defaults

Task #165: callers recognized as coding agents receive JSON without `--json`.

## Caller Detection

Recognize nonblank, UTF-8 `CODEX_THREAD_ID` or `CODEX_SESSION_ID`, matching
qqq's supported native Codex context. Otherwise, when exact Herdr context
exists, inspect current pane. A nonblank reported agent name or agent session
name identifies an agent caller. Lookup is best effort: missing Herdr,
malformed responses, or panes without agent metadata keep human default.
Do not infer agent callers from redirected stdout, ownership tokens,
`QQQ_SESSION`, a supplied harness override, or other agents near project cwd.
Detection opens no project DB, claims no tasks, changes no owner identity.

## Output Selection

Global `--json` explicitly requests JSON. New global `--human` explicitly
requests human output. Flags conflict. With neither flag, use caller detection.
Resolve same mode before alias expansion, after expansion, and after parsing,
so alias failures, parser failures, runtime errors, success results, and watch
snapshots follow same contract. For conflicting flags, explicit `--json`
selects structured parser error. Real flags may appear before or after command;
flag-looking option values or literals after `--` remain data.

JSON mode preserves full descriptions, flat lists, no human completed limit,
existing error payloads, exit codes, and stdout/stderr separation. Help/version
remain Clap text. TUI/editor operation remains interactive; mode affects their
existing serialized success/error output, not terminal rendering.

## Validation

Test native Codex success, parser/runtime/alias errors, full completed lists,
JSON watch, explicit overrides, alias expansion, literal flag values, and
blank/missing markers. Test exact Herdr panes with agent metadata, human panes,
unavailable lookup, and absence of project-root discovery. Preserve existing
human output, explicit JSON, ownership, and PTY tests. Run full suite,
formatting, Clippy, release build, installed CLI smoke tests before completion.
