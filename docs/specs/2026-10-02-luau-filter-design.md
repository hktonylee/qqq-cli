# Luau task filters

## Contract

`qqq list --filter EXPR` and `qqq next --filter EXPR` accept one Luau expression. Parse with `full_moon`'s Luau grammar; compile an allowlisted AST to SQLite with bound literal parameters. No VM, arbitrary SQL, script statements, globals, filesystem access, or schema changes.

Expose `id`, `description`, `status`, `priority`, `archived`, `created_at`, `updated_at`, `parent_id`, `harness_name`, `harness_session`, `orchestrator_name`, `orchestrator_session`. Aliases: `task_name = description`, `created_time = created_at`, `updated_time = updated_at`. Description aliases include the whole description. Nullable fields retain nil. Internal claim keys and presentation-only context flags are unavailable.

Support literals, parentheses, comparison, `and`, `or`, `not`, numeric `+ - * /`, string concatenation `..`, byte length `#`, and Luau `if ... then ... elseif ... else ...`. Follow Luau precedence and truthiness: only false/nil are false; zero/empty strings are true. Logical and conditional value branches must have compatible types, allowing nil with any type. Equality between distinct non-nil types is false, including boolean versus number. Nullable ordering evaluates false; SQLite arithmetic/function NULL becomes nil. Reject `%`, `//`, `^`, tables, functions, member/method access, casts, interpolation, statements, unknown names, wrong arity, and wrong operand types.

Function allowlist: `like(text, pattern[, escape])`, `glob(text, pattern)`, `lower`, `upper`, `length`, `substr`, `trim`, `ltrim`, `rtrim`, `replace`, `instr`, `abs`, `round`, `coalesce`, `nullif`, scalar `min`/`max`, `date`, `time`, `datetime`, `strftime`, `julianday`, `unixepoch`. Functions follow documented SQLite behavior, including Unicode/case limitations and date modifiers. All SQL names are static. LIKE takes text first, pattern second. Booleans remain distinct from numeric arguments.

Strings support Luau quoted/long-bracket literals and escapes; reject byte strings that cannot be represented as UTF-8. Numbers support decimal floats, hexadecimal/binary integers, separators; require finite values with magnitude at most 2^53, checking source digits at rounding boundary. Hexadecimal floats are unavailable. Bound compilation: 16 KiB source, 128 non-trivia tokens, 32 AST levels, 128 AST nodes, 64 KiB generated SQL. Check token count before recursive parsing. Error text identifies `--filter` and links documentation.

## List

Base archive/completed limits apply first, preserving existing semantics. Fetch base rows and each row's SQL predicate in one SELECT. Combine SQL result with `--query` and repeated `--status` using AND, then retain ancestors already in base rows with `context_only: true`. Preserve output order, JSON shape, and empty-match messages. `--watch` compiles once, reevaluates against every committed snapshot, emits no idle redraws.

## Claims

Compile before DB open/recovery, ownership changes, or dispatch. Add predicate to new-candidate SELECT inside existing `BEGIN IMMEDIATE`, after archive/readiness constraints and before priority/ID ordering. Existing owned tasks always return regardless of predicate. Never release/reassign ownership due to filter. No match -> null; `--wait` retains existing blocking behavior and reevaluates predicate on each attempt. Herdr readiness preflight and atomic claim use same compiled filter; zero matches create no tab.

## Modules and verification

`src/sql_filter/` owns AST compilation, typed expressions, function signatures, literal decoding. DB owns SQL execution and binding. `list_filter` owns combined direct matching/ancestor context. CLI compiles once and passes references through list/watch/next/dispatch.

Tests cover parameter injection, full grammar rejection, nil/type/truthiness semantics, precedence, escapes, resource bounds, function signatures, aliases, archive/completed context, wait/watch updates, concurrent atomic claims, persistent ownership, dependencies, and Herdr dispatch filtering. Run fmt, clippy with warnings denied, full suite, integrated focused checks, release-install smoke. Public guides: `docs/filter.md`, `docs/filter-variables.md`, `docs/filter-functions.md`, linked from README and help.
