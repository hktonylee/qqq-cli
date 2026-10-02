# Filter functions

Exact allowlist for [Luau filters](filter.md). Names are case-sensitive. Functions require parentheses. `text` means string, `number` means numeric value; nil is accepted for either. Boolean arguments are accepted only by `coalesce`/`nullif`. SQLite NULL returns become nil, except nil LIKE/GLOB inputs return false.

| Function | Result | Behavior |
| --- | --- | --- |
| `like(text, pattern[, escape])` | boolean | `text LIKE pattern [ESCAPE escape]`; text first. `%` matches any sequence, `_` one character. Escape must be one character. |
| `glob(text, pattern)` | boolean | `text GLOB pattern`; text first. Case-sensitive, shell-style `*`, `?`, bracket classes. |
| `lower(text)` | string or nil | ASCII lowercase. |
| `upper(text)` | string or nil | ASCII uppercase. |
| `length(text)` | number or nil | Unicode code-point count before NUL. |
| `substr(text, start[, count])` | string or nil | One-based substring; negative start counts from end. |
| `trim(text[, characters])` | string or nil | Remove boundary characters; default spaces. |
| `ltrim(text[, characters])` | string or nil | Remove left boundary characters. |
| `rtrim(text[, characters])` | string or nil | Remove right boundary characters. |
| `replace(text, needle, replacement)` | string or nil | Replace matching substrings. |
| `instr(text, needle)` | number or nil | One-based first match, zero when absent. |
| `abs(number)` | number or nil | Absolute value. |
| `round(number[, digits])` | number or nil | Rounded numeric value; default zero digits. |
| `coalesce(a, b, ...)` | common type or nil | First non-nil value. At least two compatible arguments. |
| `nullif(a, b)` | type of a or nil | Nil when arguments equal, otherwise a. Compatible types required. |
| `min(a, b, ...)` | number/string or nil | Scalar minimum; at least two compatible non-boolean arguments. |
| `max(a, b, ...)` | number/string or nil | Scalar maximum; at least two compatible non-boolean arguments. |
| `date([value, modifiers...])` | string or nil | Date, `YYYY-MM-DD`. |
| `time([value, modifiers...])` | string or nil | Time, normally `HH:MM:SS`. |
| `datetime([value, modifiers...])` | string or nil | Timestamp, normally `YYYY-MM-DD HH:MM:SS`. |
| `strftime(format[, value, modifiers...])` | string or nil | Formatted date/time. |
| `julianday([value, modifiers...])` | number or nil | Julian day value. |
| `unixepoch([value, modifiers...])` | number or nil | Seconds from Unix epoch. |

Date/time `value` accepts string, number, nil. Modifiers and `strftime` format require strings. Missing value uses current time. Numeric values default to Julian days; use `"unixepoch"` modifier for Unix seconds. Invalid dates/formats can return nil. Date functions use bundled SQLite behavior; host SQLite version is irrelevant.

```sh
qqq list --filter 'like(task_name, "%auth%")'
qqq list --filter 'like(description, "%a!_b%", "!")'
qqq list --filter 'glob(task_name, "Fix*")'
qqq list --filter 'length(description) > 100 and instr(description, "TODO") > 0'
qqq list --filter 'coalesce(parent_id, 0) == 0'
qqq list --filter 'datetime(updated_time) < datetime("now", "-1 day")'
qqq list --filter 'strftime("%Y", created_time) == "2026"'
qqq list --filter 'unixepoch(created_time) >= unixepoch("now", "-7 days")'
```

LIKE's case folding is ASCII-only by default; `--query` retains Unicode lowercase search. `lower`/`upper` are also ASCII-only. LIKE/GLOB argument order here is **text, pattern**, unlike SQLite's native function-call order. `min`/`max` are scalar functions, not aggregate queries. Nil propagation, substring indexing, trim character sets, date modifiers follow SQLite semantics.

Reference: [SQLite scalar functions](https://www.sqlite.org/lang_corefunc.html), [SQLite date/time functions](https://www.sqlite.org/lang_datefunc.html). Other functions, including `load_extension`, arbitrary SQL/aggregate functions, Lua `string.*`/`math.*`, are unavailable.
