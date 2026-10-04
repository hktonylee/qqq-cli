# Read-only next task preview

Task #118 adds `qqq next --dry-run`. Preview queued candidate only, even if caller
already owns task. Choose highest-priority ready, unarchived new task, oldest ID
on ties. Apply existing `--filter` to candidates. Return stored task status and
identity, without claiming task or overlaying assignment metadata.

Preview needs no owner identity and bypasses both local ownership discovery and
Herdr dispatch, including configured new-agent mode. Session/assignment flags
never affect preview selection. Identity overrides retain generic validation.
No changes to tasks, events, ownership, messages, images, or Herdr links. Normal
database startup/recovery remains shared with other commands. Normal `next`
continues returning existing claim; multiple active claims remain future work.

Share ready-candidate helper between preview and atomic claim. Preview uses deferred
read transaction so selection and returned task share snapshot.
Normal claims retain immediate transaction and existing mutation path. End each
preview transaction before waiting; `--wait --dry-run` waits until preview returns
task, without reserving it. Concurrent previews may return same task, and another
worker may claim it immediately afterward.

Reuse normal task output. Empty preview keeps JSON `null`; human output says
`No task available for pickup.` Add help and
README examples. Verify priority/ties, blocked/archived/
error/completed exclusion, filters, skipping owned tasks, no writes despite
overrides, dispatch bypass, waiting, and subsequent real claim. Run existing full
suite to protect atomic ownership and Herdr behavior.
