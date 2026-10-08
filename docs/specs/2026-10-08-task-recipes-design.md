# Parameterized Task Recipes

Task #183 adds reusable project-local task workflows through existing atomic
import path. JSON recipe version1 extends batch inputs with declared parameters.
No DB migration, graph loader, expression language or code execution.

## Discovery And CLI

Use `qqq add --template NAME --var NAME=VALUE` with repeatable `--var` and optional
`--dry-run`. Templates live at `<project-root>/.qqq-recipes/NAME.json`. Resolve
project root using existing nearest `.qqq` discovery, including from subdirs.
No cwd recipe shadowing, ancestor recipe fallback, global search or implicit
built-ins. Files remain ordinary committable project files; ignored `.qqq/`
continues holding runtime state. Recipe format carries required version1.

Recipe names contain ASCII letters, digits, underscores and hyphens, with at
least one character. Reject path separators, dots, absolute paths and blank names.
Users pass names without `.json`. Missing project/DB retains ProjectNotFound;
missing recipe produces actionable template error showing expected path. Read
UTF-8 files only. Never initialize project merely to find recipe.

`--template` conflicts with positional text, `--description`, `--edit`, `--stdin`,
`--parent`, `--depends-on`, explicit `--priority`, `--image` and `--tag`. Recipes
own all generated task fields. `--var` and add's `--dry-run` require `--template`.
Ordinary add/import/stdin/editor flows and output mode selection remain unchanged.

## Version1 Format

```json
{
  "version": 1,
  "parameters": [
    {"name": "component"},
    {"name": "severity", "default": "normal"}
  ],
  "tasks": [
    {
      "key": "bug",
      "description": "Fix ${component} bug\n\nAcceptance criteria\n- Regression test reproduces failure\n- Fix passes tests\nSeverity: ${severity}",
      "tags": ["bug", "${component}"],
      "priority": 5
    }
  ]
}
```

`parameters` defaults to empty array. Each declaration requires unique name;
optional `default` is string. Omitted default makes parameter required, including
declared parameters unused by recipe text. Names follow ASCII identifier syntax:
letter/underscore first, then letters/digits/underscores. Reject duplicate names,
unknown fields, duplicate JSON fields, wrong types and unsupported versions.

`tasks` uses exact existing import task shape: unique nonblank `key`, whole
`description`, integer `priority`, string-array `tags`, optional `parent` and
`depends_on` array. Acceptance criteria belong inside description. Local refs
use `{"key":"prepare"}`; explicitly existing refs use `{"id":12}`. Preserve
existing import defaults, tag normalization, forward references, duplicate/ref
validation and stable topological order. Empty task arrays retain import's no-op
semantics. Version1 recipes support no attachment fields.

## Literal Parameter Expansion

Split each CLI assignment at first `=`. Validate name, required presence,
duplicates and unknown assignments before generating tasks. Preserve entire
value after `=`, including empty strings, whitespace, further equals signs,
quotes, newlines, Unicode and shell-looking text. Defaults and supplied values
are literal strings; empty supplied string counts as provided and remains subject
to final ordinary description/tag validation.

Expand `${name}` only in descriptions and individual tag strings, after typed
JSON parsing. Priority, keys and references stay literal structural fields.
One pass only: values/defaults containing `${other}` remain literal data. `$$`
emits literal `$`; `$${name}` emits literal `${name}`. Lone dollar and other
dollar forms remain literal. Reject unknown placeholder names, empty/malformed
identifiers and unterminated `${...}`. No environment lookup, shell, evaluation,
recursive substitution or JSON-text interpolation.

Validate final expanded batch through existing import validator. Expansion can
make a description blank or tag invalid; fail whole recipe before insertion.
Any duplicate task keys, bad fields, absent references or dependency cycle also
fail before insertion.

## Shared Atomic Import

Expose typed import input and narrow `prepare(version, tasks)` entry point. File
import continues parsing its existing envelope and calls same validator. Recipe
loader parses declarations plus same typed tasks, expands string fields, then
calls prepare. Graph validation/ordering and SQL insertion exist only in import.

Prepare recipe before normal writable DB open. Real application follows existing
migration/deletion recovery, then import validates existing references and inserts
all tasks in one immediate transaction. Failure rolls back tasks, dependencies
and AUTOINCREMENT allocation. Concurrent applications receive separate complete
graphs with local refs mapped inside their own transaction. Explicit existing
task refs keep their IDs.

Dry-run uses existing current-schema read-only DB snapshot plus deferred import
transaction. No task creation, claims, migration, deletion recovery or ID
reservation. Owner preflight from #181 remains separate command-start cleanup:
confirmed dead owners can become error before either preview or real command.
Preview does not create a project. Existing references are checked again during
real transaction because state can change after preview.

## Output And Examples

JSON uses existing import report: version, dry_run, count, mapping,
creation_order and tasks. Committed mapping unambiguously associates recipe keys
with new IDs. Preview mapping is empty; new IDs remain null, while original
local/existing reference objects and creation_order describe graph without
reserving IDs. Descriptions and tags contain expanded literal values.

Recipe human output reuses import count/key/ref summary. Dry-run additionally
shows every expanded description and tag list, sanitizing controls through
existing human text helpers. Ordinary import rendering stays unchanged. Ordinary
shells use human output; native agent contexts retain automatic JSON; --human
and --json keep existing overrides and error envelope behavior.

Ship `examples/recipes/bug.json` and `release.json`, documented copying into
project's `.qqq-recipes`. Release example contains prepare, test and publish
tasks with both parent and extra prerequisite references. Document separate
existing-task reference example, name discovery from subdirs, parameter quoting,
literal escaping, preview and committed mappings. Creating publish task never
runs publication itself.

## Verification

CLI integration tests cover one/multi-task recipes, local/existing refs, stable
order, defaults, repeated invocations, Unicode/multiline/literal values, escaping,
nonrecursive expansion and parameterized tags. Check required/duplicate/unknown
params, bad assignments, malformed/duplicate fields, missing recipes, traversal,
invalid tags/descriptions/priority/refs/cycles and incompatible ordinary add flags.
Assert failed applications leave no tasks/dependencies/events/sequence changes;
inject insertion failure, use competing processes for independent graph mapping.
Dry-run asserts exact DB bytes, no IDs/claims/project creation, old-schema guard
and preserved deletion recovery state. Test human full preview, automatic JSON
and explicit output overrides. Run shipped documented examples as subprocesses.
Run focused/full suites, fmt, strict Clippy, compatibility, code review, local
integration, install and installed CLI recipe smoke before queue completion.
