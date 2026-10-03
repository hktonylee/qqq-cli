# Structured JSON CLI Errors

Task 148 gives JSON callers stable failure codes without parsing human wording. Preserve successful payloads, readable human errors, ownership/state rules and command exit semantics.

## Envelope and classification

Failed `--json` commands emit one JSON error object on stderr, followed by newline: `{"code":"TASK_NOT_FOUND","message":"Task 42 not found","details":{"task_id":42,"command":"show"}}`. stdout contains no error text or UI bytes. Runtime failures keep exit 1; argument-parser failures keep Clap's failure exit code. Help/version keep their existing plain output and success status even with `--json`. An unhealthy doctor report remains its existing diagnostic JSON on stdout with exit 1; an empty queue remains successful JSON null.

Codes are explicit typed classifications, independent of message wording. Required codes: `TASK_NOT_FOUND`, `OWNERSHIP_MISMATCH`, `INVALID_TRANSITION`, `INVALID_ARGUMENT`, `INVALID_FILTER`, `DB_BUSY`, `CONTENT_CONFLICT`. Other failures use documented `PROJECT_NOT_FOUND`, `CONFIG_ERROR`, `IO_ERROR`, `EDITOR_ERROR`, `DISPATCH_ERROR`, `DATABASE_ERROR` or `COMMAND_ERROR`. Details always form an object. Include command, task ID/reference, expected/actual status, content revisions, argument names or SQLite numeric result codes when known. Never include claim keys, owner tokens, unnecessary identity values, full task descriptions or arbitrary raw editor output.

Attach typed errors at domain checks and inspect underlying SQLite/IO error types. Never classify by matching error messages. Guarded edits reuse task 146's typed `ContentConflict`; removed guarded content includes an explicit removal reason. An ownership/state check observes state under the same existing transaction and preserves rollback behavior. Human formatting retains useful original context; JSON messages use safe public wording where original context includes identity internals.

## Parsing and output

Replace terminating Clap parse with fallible parse at the CLI boundary. Detect requested JSON mode before parse/alias failures, respecting literal `--`, option values and aliases introducing `--json`. Parser help/version continue through Clap's existing output path. Successful command execution and watch output remain unchanged.

Nonterminal JSON editor failures emit only the final envelope. Preserve recovery paths and pending attachment bytes in structured details instead of printing a prose prelude. Capture/suppress nonterminal external-editor terminal output in JSON mode; keep existing human editor output. Explicit interactive terminal sessions retain live UI/prompts on stderr and emit one final JSON error object after terminal restoration if they fail; stdout remains machine output only. JSON callers requiring parseable whole stderr should use inline fields or nonterminal input.

## Verification and integration

Real CLI tests parse entire nonterminal stderr as JSON and assert empty stdout/nonzero status for parser failures, missing IDs, mismatched owners, transitions, invalid filters, DB locks, stale edits, editor/config/alias/IO errors. Validate task details and absence of private identity/claim values. Preserve successful payloads, help/version, doctor diagnostics, human error context, empty queue, alias handling, recovery files and terminal cleanup. Run focused checks, full suite, fmt, Clippy and read-only review before clean fast-forward integration; verify installed binary before explicit queue completion.
