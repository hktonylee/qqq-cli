# Parameterized Task Recipes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apply project-local literal-parameter recipes through atomic task import.

**Architecture:** New recipe module resolves project file, parses typed parameter
declarations and existing import tasks, expands only description/tag strings, then
hands validated batch to import. CLI selects recipe path without entering editor;
import remains sole graph validator and transaction writer.

**Tech Stack:** Rust, Clap, serde JSON, rusqlite, existing subprocess tests.

---

## File Map

- `src/recipe.rs`: discovery, declarations, assignment validation, literal expansion.
- `src/import.rs`: expose typed input plus shared prepare entry point.
- `src/db.rs`: expose existing database-path discovery within crate.
- `src/main.rs`: add flags, prepare recipe, reuse dry-run/write import execution.
- `src/output.rs`, `src/output/recipe.rs`: recipe-specific full human preview.
- `tests/recipes.rs`: real CLI/file/SQLite acceptance coverage.
- `examples/recipes/bug.json`, `release.json`: reusable documented examples.
- `README.md`, `docs/reference.md`: discovery, schema, escaping and usage.

## Task 1: Recipe Happy Path And Shared Import Entry

- [ ] Add CLI test fixture invoking binary in TempDir, removing inherited agent
  context, initializing project and writing `.qqq-recipes/bug.json`.
  Test required component plus default severity; preview then commit exact Unicode
  multiline description, normalized tags, priority and key mapping.

```rust
let recipe = json!({"version":1,
    "parameters":[{"name":"component"},{"name":"severity","default":"normal"}],
    "tasks":[{"key":"bug","description":"Fix ${component}\nSeverity: ${severity}",
              "tags":["bug","${component}"],"priority":5}]});
fixture.recipe("bug", &recipe);
let preview = fixture.ok(&["add","--template","bug","--var","component=authλ","--dry-run"]);
assert_eq!(preview["mapping"], json!({}));
assert_eq!(preview["tasks"][0]["id"], Value::Null);
assert_eq!(preview["tasks"][0]["description"], "Fix authλ\nSeverity: normal");
let applied = fixture.ok(&["add","--template","bug","--var","component=authλ"]);
assert_eq!(applied["mapping"], json!({"bug":1}));
assert_eq!(fixture.ok(&["show","1"])["task"]["tags"], json!(["bug","authλ"]));
```

- [ ] Run `cargo test --locked --offline --test recipes -- --test-threads=1`.
  Expect parser failure for unknown `--template`.
- [ ] Add template/vars/dry_run fields to Add. Use Clap conflicts to reject
  ordinary task fields with template; require template for vars/dry_run.

```rust
#[arg(long, value_name = "NAME", conflicts_with_all = ["text", "description",
    "edit", "stdin", "parent", "depends_on", "priority", "images", "tags"])]
template: Option<String>,
#[arg(long = "var", value_name = "NAME=VALUE", requires = "template")]
vars: Vec<String>,
#[arg(long, requires = "template")]
dry_run: bool,
```

- [ ] Make InputTask crate-visible, with description/tags crate-visible for
  expansion. Keep its other fields and Parent type private. Expose existing
  database_path within crate. Add import entry point:

```rust
pub(crate) fn prepare(version: u32, tasks: Vec<InputTask>) -> Result<ValidatedBatch> {
    validate(Batch { version, tasks })
}
```

- [ ] Implement typed Recipe/Parameter and `recipe::read(name, assignments)`.
  Recipe version required, parameters default empty, tasks exact InputTask array.
  Require string defaults when present; custom deserializer rejects explicit null.

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    version: u32,
    #[serde(default)]
    parameters: Vec<Parameter>,
    tasks: Vec<crate::import::InputTask>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameter {
    name: String,
    #[serde(default, deserialize_with = "string_default")]
    default: Option<String>,
}
fn string_default<'de, D: serde::Deserializer<'de>>(d: D)
    -> std::result::Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
```

- [ ] Define `identifier` and `recipe_name` using ASCII byte predicates. Read
  `<database_path(false).parent().parent()>/.qqq-recipes/NAME.json`; reject invalid
  names before read. Parse UTF-8 with typed serde, attach template error context
  and JSON line/column. Enforce version1. Build BTreeMap of declarations, reject
  duplicate names. `values(parameters, assignments)` splits first equals, rejects
  malformed/invalid/duplicate/unknown names, then fills defaults and requires
  every remaining declared parameter. Preserve each value byte-for-byte.

```rust
fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn recipe_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
```

- [ ] Define expansion as one-pass scan; never rescan inserted values. Use
  structured InvalidArgument errors for unclosed/invalid/unknown placeholders.

```rust
fn expand(text: &str, values: &BTreeMap<String, String>) -> Result<String> {
    let mut result = String::new();
    let mut rest = text;
    while let Some(index) = rest.find('$') {
        result.push_str(&rest[..index]);
        rest = &rest[index..];
        if let Some(tail) = rest.strip_prefix("$$") {
            result.push('$'); rest = tail;
        } else if let Some(tail) = rest.strip_prefix("${") {
            let end = tail.find('}').context(Info::invalid_argument("template",
                "Unterminated recipe parameter placeholder"))?;
            let name = &tail[..end];
            ensure!(identifier(name), Info::invalid_argument("template",
                "Invalid recipe parameter placeholder"));
            result.push_str(values.get(name).with_context(||
                Info::invalid_argument("template", "Unknown recipe parameter placeholder")
                    .detail("parameter", name))?);
            rest = &tail[end + 1..];
        } else {
            result.push('$'); rest = &rest[1..];
        }
    }
    result.push_str(rest);
    Ok(result)
}
```

- [ ] Expand every description/tag, then `import::prepare(recipe.version,
  recipe.tasks)`. Prepare alongside ordinary import before writable DB open:

```rust
Commands::Add { template: Some(name), vars, .. } => Some(recipe::read(name, vars)?),
```

  Route template dry-run through current read-only import branch; route real
  template Add arm to `import::run(..., false)`. Keep ordinary Add arm beneath
  template arm, ignoring prepared-only fields with `..`. Add recipe module.
- [ ] Run happy-path recipe tests plus CLI baseline; expect both green. Commit
  `[Feat] Apply Parameterized Recipes Through Atomic Import`.

## Task 2: Validation, Graphs And Atomicity

- [ ] Add fixtures for two/three-task forward references, extra prerequisites
  and explicit existing IDs. Verify mapping, creation_order and actual saved IDs.
  Repeat application; ensure new independent graph and original existing refs.
- [ ] Add literal assignments containing quotes, equals, CRLF, Unicode, `${other}`,
  shell-looking text; assert exact saved descriptions and absent execution marker.
  Cover `$$`, `$${name}`, default literal tokens and malformed placeholders.
- [ ] Add rejection matrix: required/duplicate/unknown vars, missing equals,
  invalid identifiers, missing recipes/traversal, duplicate declarations/JSON
  fields/task keys, invalid types/version/tags/descriptions/priority, missing refs,
  overlap/cycles, every conflicting Add flag, vars/dry_run without template.
  Capture DB bytes and logical task/dependency/event/sequence state before errors.
- [ ] Run focused tests, diagnose expected failures, adjust parser/CLI validation
  only where evidence requires. Keep import graph implementation unchanged.
- [ ] Add read-only preview assertions for bytes, sqlite_sequence, statuses,
  claims and delete-staging marker. Older schema requires ordinary migration;
  failed recipe validation happens before normal writable migration. Missing
  project never creates `.qqq`. Subdir discovery and nested-project isolation.
- [ ] Inject second task insertion failure and verify transaction rollback:

```rust
conn.execute_batch("CREATE TRIGGER fail_second BEFORE INSERT ON tasks
 WHEN NEW.description='Fail second' BEGIN SELECT RAISE(ABORT,'injected'); END;")?;
```

  Apply two-task recipe; assert no new tasks/dependencies/events/sequence consumed.
  Launch two CLI processes applying same graph concurrently; verify disjoint
  complete key/ID mappings and local edges inside each graph.
- [ ] Run `cargo test --locked --offline --test recipes --test cli --test
  dependencies -- --test-threads=1`; commit verified acceptance checkpoint.

## Task 3: Human Preview And Documented Examples

- [ ] Add human preview test asserting full multiline description, tags, local
  and existing refs; add agent-default JSON and explicit --human/--json tests.
  Ordinary add/import output tests remain unchanged.
- [ ] Add Format::Recipe selected only by template Add; recipe renderer delegates
  summary to import renderer, appends full expanded descriptions/tags for dry-run
  using existing clean helper. Render every line without terminal controls.
- [ ] Create version1 bug/release examples. Release graph uses prepare as parent,
  tests as extra prerequisite for publish, param version in descriptions/tags.
  Document copying examples, exact discovery, schema/escaping, quoted assignments,
  previews, no reserved IDs, committed mapping and existing task ref variant.
- [ ] Exercise both copied examples in TempDir through documented CLI commands,
  from project root and subdir. Verify preview/commit descriptions and graph IDs.
- [ ] Run recipes, output, CLI/default-output suites; commit docs/output/examples.

## Task 4: Final Verification And Delivery

- [ ] Run fmt, strict all-targets Clippy, full serial suite and release:
  `cargo fmt --all -- --check`; `cargo clippy --locked --offline --all-targets
  -- -D warnings`; `cargo test --locked --offline -- --test-threads=1`;
  `cargo build --locked --offline --release`.
- [ ] Request focused code review per skill; fix important findings with
  regression tests and rerun affected checks. Confirm DB schema stays13 and
  historical artifact hashes unchanged through compatibility suite.
- [ ] Rebase onto current master, preserve concurrent work, fast-forward locally.
  Rebuild root package paths as needed, offline install, verify installed recipe
  examples, literal inputs, preview and ordinary add/import plus binary hash.
- [ ] Record verified evidence, update plan, complete #183, remove merged
  worktree/branch, resume one persistent queue waiter.

## Progress

Base d4f4443: prior full suite730/42 binaries, no code changes since verification.
Fresh worktree baseline CLI68 plus dependencies7 pass. Design self-review clear:
one importer, fixed structural types, no schema migration, no unresolved choices.
