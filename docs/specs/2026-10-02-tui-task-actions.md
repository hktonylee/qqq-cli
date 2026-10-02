# TUI Task Actions

Dashboard footer advertises `Ctrl-G actions`. On selected task, `Ctrl-G`
opens full-screen keyboard menu: `c` complete owned task, `r` retry error,
`o` reopen completed, `a` archive/unarchive, `p` change priority, `d` set or
clear parent. Menu remains usable at 12x8; `Esc` closes. Priority prompt
accepts signed integer in -100..100; parent accepts positive ID or `none`.
Input errors keep prompt/value visible. Menu and prompts never edit draft.

State actions confirm before running; priority/parent confirm when draft dirty.
Confirmation says draft will be discarded on success. Cancel keeps draft.
DB/API failure closes modal, shows exact error, keeps dirty draft and current
selection. Success reloads task, clears stale draft, refreshes list/filter;
archiving hidden task selects blank draft while keeping query. Confirmation
and action UI do not write stdout. Ctrl-C exits with existing terminal cleanup.

TUI delegates operations to callback supplied by CLI. Callback invokes
`session::owner` plus `Db::complete` for owned completion;
`Db::edit_with_priority` with `RetryError`, priority, or `ParentChange`;
`Db::reopen`; and `Db::set_archived`. All DB guards and ownership rules stay
in existing methods. Actor for non-owned transitions follows CLI convention.

PTY tests exercise success and rejection for each action family, dirty draft
cancel/failure, filter/list selection after archive, narrow 12x8 menu, stdout
silence, and terminal restoration. README lists keys and prompts.
