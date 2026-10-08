# Tag Worker Routing Design

Task #182 routes specialist workers using stored task tags.

## Selectors

`list` and `next` accept repeatable `--tag LABEL`. Every supplied label must
match. Selectors combine with existing Luau, text, status, archive and completed
visibility rules. Lists retain visible ancestor context.

Shared tag normalization trims boundary whitespace, removes exact duplicates,
preserves case and Unicode, rejects empty labels, controls, commas and square
brackets. Selector validation precedes project preflight, DB open, migration,
owner recovery and dispatch. Invalid flags produce `INVALID_ARGUMENT` with
`details.argument = "--tag"`; invalid predicates retain `INVALID_FILTER`.

`has_tag("LABEL")` returns boolean in Luau filters. Exactly one string literal
is required; parenthesized literals work. Literal decoding and shared tag
normalization run during compilation. Dynamic text expressions and nil are
rejected because label validation must precede DB access. Boolean composition,
status, priority and configured aliases work through existing filter machinery.

## SQL And Ownership

Compile membership as `EXISTS (SELECT 1 FROM json_each(tasks.tags) AS task_tag
WHERE task_tag.value COLLATE BINARY = ?N)`. Labels remain bound parameters.
Membership uses whole stored labels, never descriptions, substrings, case
folding or Unicode normalization. No schema change or new DB index.

Append normalized `--tag` predicates to existing `CompiledFilter`, preserving
parameter numbering and keeping required labels as diagnostic metadata. One
compiled selector flows through list/watch, next/wait, dry-run, explain and
Herdr dispatch. Claim selection stays inside existing atomic transaction.
Readiness, dependencies, archive exclusion, priority and oldest-ID ties retain
existing behavior. Existing owned tasks return before selector evaluation;
changing selectors never releases or transfers ownership. No matching ready
candidate creates no Herdr tab.

## Explain

Keep existing JSON fields and combined `matches_filter` meaning. Add
`tag_excluded` reason when explicit `--tag` selectors fail, using exact stored
labels from same report snapshot. Keep dependency reasons alongside exclusion.
If tags match but Luau expression fails, retain `filter_excluded`.
`has_tag` inside arbitrary Luau boolean expressions uses `filter_excluded`:
an OR/NOT expression cannot attribute failure to individual positive tags.
Human reason rendering already derives labels from reason names.

## Verification

Observe failing compiler and CLI tests before implementation. Cover literal
typing/validation, SQL binding, exact versus substring/description matches,
case, Unicode, spaced labels, repeated-tag AND, other-filter composition,
ancestor context, archive/completed limits, owned reuse, parent/prerequisite
readiness, priority/ties, no-match, wait, watch, concurrent claims and dispatch.
Invalid selectors must precede both project discovery and active-owner
preflight mutations. Verify agent-default JSON and human success/error output.

Run focused tests, full locked offline Rust suite, formatting, strict Clippy,
independent code review, release and installed CLI checks. Rebase onto current
master, fast-forward locally, verify integrated tree, install binary, record
queue evidence, complete #182, return to one persistent queue waiter.
