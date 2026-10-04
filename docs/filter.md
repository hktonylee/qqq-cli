# Luau filters

`qqq list --filter EXPR` and `qqq next --filter EXPR` compile one Luau expression to SQLite. Quote expression so shell preserves spaces, operators, string literals.

```sh
qqq list --filter 'status == "new" and priority >= 5'
qqq list --filter 'like(task_name, "%auth%")'
qqq list --filter 'parent_id ~= nil' --query migration --status new
qqq list --watch --json --filter 'priority > 0'
qqq next --local --session worker-1 --filter 'priority >= 5'
qqq next --local --wait --session worker-1 --filter 'like(description, "%Rust%")'
qqq list --filter 'datetime(created_time) >= datetime("now", "-7 days")'
qqq list --filter '(if parent_id == nil then priority else priority + 1) >= 5'
```

See [all variables](filter-variables.md), [all functions](filter-functions.md).

## List behavior

`--filter`, `--query`, and `--status` combine with AND. Repeated statuses still combine with OR. Archive visibility and completed-task limits apply before matching. Use `--all` to bypass configured completed limit, `--include-archived` to include archived rows.

Visible ancestors of direct matches remain as context rows, even when predicate is false for parent. Human output marks `[context]`; JSON marks `context_only: true`. Ancestors excluded by archive/completed limits remain excluded. Task order, complete descriptions, JSON fields stay unchanged. No matches -> `[]` in JSON, `No matching tasks.` in human output.

`--watch` compiles once, evaluates predicate for every committed DB snapshot, retains existing idle behavior. Date functions using `"now"` reevaluate on those snapshots; watch does not refresh solely because wall-clock time passed.

## Claim behavior

`next` applies predicate to **new candidates** inside atomic claim transaction. Candidates must also be unarchived, ready, unclaimed. Completed parent dependency remains required. Highest priority wins, oldest ID breaks ties.

Without `--dry-run`, existing owned task always returns, even when predicate no longer matches. Changing filter never releases ownership. Complete or explicitly release current task before choosing another.

`next --dry-run --filter EXPR` previews matching queued candidate using same readiness/priority rules. Existing claims are skipped. Preview uses read-only snapshot; `--wait` waits for matching candidate without claiming or dispatching it. No session or Herdr identity required.

No match -> JSON `null` or human `No task available for pickup.`. `--wait` blocks until matching ready candidate exists, reevaluating predicate on each claim attempt. Herdr dispatch uses same filter for readiness and atomic claim; no matching candidate creates no tab.

## Supported expression language

Syntax follows [Luau expression grammar](https://luau.org/syntax/). This is a typed query subset: no Luau VM or arbitrary script execution.

| Form | Example |
| --- | --- |
| Task variables | `priority`, `task_name`, `created_time` |
| Boolean/nil literals | `true`, `false`, `nil` |
| String literals | `"auth"`, `'auth'`, `[=[auth]=]` |
| Numeric literals | `5`, `1.5`, `1e-2`, `0xFF`, `0b1_010`, `1_000` |
| Parentheses | `(priority + 1) > 5` |
| Equality | `status == "new"`, `parent_id ~= nil` |
| Ordering | `priority < 5`, `priority <= 5`, `created_at > "2026-01-01"`, `id >= 10` |
| Logic | `not archived`, `priority > 0 and status == "new"`, `parent_id == nil or priority > 5` |
| Numeric arithmetic | `priority + 1`, `priority - 1`, `priority * 2`, `priority / 2` |
| String concatenation | `"Fix " .. "auth"` |
| UTF-8 byte length | `#description` |
| Conditional expression | `if archived then 0 elseif priority > 5 then 2 else 1` |
| Allowlisted functions | `like(description, "%auth%")`, `coalesce(parent_id, 0)` |

Luau precedence applies. Only `false` and `nil` are false; `0` and `""` are true. Final result is converted using that truthiness. For numeric selection use `priority > 0`; `--filter 'priority'` also matches zero priorities.

`and`/`or` return selected operand, preserving short-circuit behavior. Operand types must agree, with nil accepted alongside any type. Conditional branches follow same type rule. `(parent_id or 0) == 0` is valid; `true or 3` fails type checking. `not` and conditional conditions accept every supported type.

Equality handles nil: `parent_id == nil` tests absence; two nil values compare equal. Distinct non-nil types compare unequal, including `false == 0`. Ordering requires compatible numeric/text operands; nil ordering returns false. Strings use SQLite binary ordering. Arithmetic requires numeric operands, concatenation requires text; nil propagates through arithmetic, concatenation, byte length. No implicit string/number/boolean conversions. SQLite arithmetic is used, with `/` forced to real division; division by zero yields nil. SQLite function NULL becomes nil. Guard nullable operands or use `coalesce` when needed.

`length(text)` counts Unicode code points before NUL; `#text` counts UTF-8 bytes. Quoted strings support `\a \b \f \n \r \t \v \\ \' \"`, decimal byte escapes (up to three digits), `\xHH`, `\u{HEX}`, `\z` whitespace skipping, escaped newlines. Other escaped characters retain character without backslash, following Luau. Long-bracket strings normalize CRLF to LF, omit initial CRLF/LF, preserve standalone CR. Resulting string must be valid UTF-8. Numeric literals must be finite, magnitude at most 2^53; decimal/exponent/hex spellings cannot bypass bound through rounding.

Unsupported forms fail explicitly: assignments, statements, loops, tables, anonymous functions, arbitrary globals, member/method calls, casts, interpolated strings, hexadecimal floats, `%`, `//`, `^`. Functions require parentheses and documented signatures.

## Compilation and errors

Literal values become bound SQL parameters. Task columns, operators, function names come from compiler allowlist. SQL text in a string stays a string. Invalid syntax/names/types fail with `Invalid --filter` before DB open, migration, recovery, claim, or dispatch. SQLite runtime errors abort query; claim transaction rolls back.

Limits: 16 KiB source, 128 non-trivia tokens, 32 AST levels, 128 AST nodes, 64 KiB compiled SQL. Comments and whitespace do not count toward token limit; each string literal counts once. Large expressions can reach generated-SQL bound earlier because value-returning logic repeats operand SQL. Split oversized expressions into smaller predicates. No messages, image contents, aggregates, subqueries, SQL extension loading, or user-defined functions are exposed.
