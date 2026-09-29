# Forced Return To Queue

Task #31 adds `qqq edit <id> --set-status new --force` for explicit manual claim
recovery. Modifier is valid only with literal `--set-status new`, not error,
`--set-pending`, field-only edits, or forced editor mode.

Force mode skips Herdr lookup and assignee matching, including with absent or
mismatched explicit/environment session. Supplied session records history author;
otherwise use `manual`. Existing nonblank session validation stays. New typed
`ForceNew` transition updates only in-progress/error tasks to new and clears
assignee, recording release event in same immediate edit transaction.

Content, dependencies, messages, attachments and saved Herdr link remain intact
unless explicitly edited. Parent/content/image edits commit with release or all
roll back on failure. New/completed tasks reject force release. Standard active
release still requires owner; session-free error retry retains error-only race
guard when force absent. Force intentionally authorizes overriding current claim.
Queue eligibility and subsequent fresh claim behavior remain unchanged.

Tests cover absent/mismatched explicit/environment sessions, unavailable Herdr,
no discovery calls, default wrong-owner rejection, retained task/link/details,
atomic field/image/parent writes and failed insert, history, restricted modifier,
manual error retry, invalid statuses, and fresh claim after force release.
No schema change required.
