# CLI List Search and Status Filter Design

`qqq list --query TEXT` searches full task descriptions, including lines hidden by piped human previews. Search uses Unicode lowercase substring matching; it does not normalize accents or whitespace. `--status STATUS` repeats; statuses form an OR set, while query and status combine with AND. Accepted names match stored statuses: `new`, `in_progress`, `completed`, `error`. Invalid names fail argument parsing before database access.

Existing completed-display limit runs first. JSON keeps its current unlimited default; explicit `--max-completed` applies to JSON too. Human config limit still applies unless `--all` or explicit limit overrides it. Filters search only tasks visible after that limit. Thus a completed parent hidden by limit remains hidden even when its child matches.

A direct match includes its visible ancestor chain so human dependency tree remains intelligible. Ancestors included only for context have `context_only: true` in flat JSON; direct matches keep current task object shape. Human tree prefixes their descriptions with `[context]`. Ancestors do not need to satisfy query or status. Hidden ancestors leave matching descendants as roots, consistent with existing list behavior. Filtering is pure work over structured `Task` rows returned by `Db::list`, with ID lookup and a visited set; no SQL string construction or rendering-based search.

One-shot `list` and `list --watch` share the filter function. Empty filtered output succeeds: JSON `[]`, human `No matching tasks.` Unfiltered empty behavior stays unchanged. Watch applies filters to every refreshed snapshot. TUI and other callers of `Db::list` stay unchanged.

CLI tests cover multiline and Unicode descriptions, literal SQL-like input, repeated statuses, invalid status, ancestor context in JSON/human output, completed-limit interaction, empty results, and watch refresh. README documents matching, context-only ancestors, status names, and examples.
