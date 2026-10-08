# Tag Worker Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route workers and task lists using exact stored tag labels.

**Architecture:** Extend existing compiled SQL selector with bound JSON-array membership predicates. Validate labels before project preflight; retain owned-task reuse and existing atomic claim paths. Explicit tag selectors carry diagnostic metadata for `tag_excluded` reasons.

**Tech Stack:** Rust, clap, full_moon Luau AST, rusqlite bundled SQLite, CLI integration tests.

---

## Task 1: Typed Predicate

Files: modify `src/sql_filter/mod.rs`, `src/sql_filter/functions.rs`, `tests/sql_filter.rs`.

- [ ] Add source modules `tags` and `errors` to compiler test harness, add `tags TEXT` column to fixture with `["frontend","界 面"]`. Add tests:

```rust
#[test]
fn has_tag_matches_exact_normalized_literals() {
    for expression in ["has_tag('frontend')", "has_tag(' frontend ')",
        "has_tag('界 面') and priority == 3", "has_tag(('frontend'))",
        "not has_tag('Frontend')", "not has_tag('front')"] {
        assert!(evaluate(expression), "{expression}");
    }
    let filter = sql_filter::compile("has_tag(\"' OR 1=1 --\")").unwrap();
    assert!(!filter.sql().contains("OR 1=1"));
    assert_eq!(filter.params(), &[Value::Text("' OR 1=1 --".into())]);
}
#[test]
fn has_tag_rejects_nonliteral_or_invalid_labels() {
    for expression in ["has_tag()", "has_tag('a','b')", "has_tag(nil)",
        "has_tag(1)", "has_tag(description)", "has_tag(lower('UI'))",
        "has_tag('')", "has_tag(' ')", "has_tag('a,b')", "has_tag('[ui]')",
        r"has_tag('a\nb')", "has_tag('frontend') + 1"] {
        assert!(sql_filter::compile(expression).is_err(), "{expression}");
    }
}
```

- [ ] Run `CARGO_INCREMENTAL=0 cargo test --locked --offline --test sql_filter has_tag`; expect unknown-function failures for valid predicate.
- [ ] Add shared SQL helper in `src/sql_filter/functions.rs`:

```rust
pub(super) fn tag_sql(parameter: &str) -> String {
    format!("EXISTS (SELECT 1 FROM json_each(tasks.tags) AS task_tag WHERE task_tag.value COLLATE BINARY = {parameter})")
}
```

- [ ] In compiler function-call branch, compile arguments as before, route `has_tag` through method below, other names through current function allowlist:

```rust
fn has_tag(&mut self, args: Vec<Expr>) -> Result<Expr> {
    ensure!(args.len() == 1, "has_tag expects exactly one string literal");
    let arg = &args[0];
    ensure!(arg.kind == Kind::Text, "has_tag requires a string literal");
    let index = arg.sql.strip_prefix('?').and_then(|value| value.parse::<usize>().ok());
    let label = index.and_then(|index| index.checked_sub(1))
        .and_then(|index| self.params.get_mut(index));
    let Some(Value::Text(label)) = label else {
        bail!("has_tag requires a string literal");
    };
    *label = crate::tags::normalize(std::slice::from_ref(label))?.remove(0);
    Ok(Expr { sql: functions::tag_sql(&arg.sql), kind: Kind::Boolean })
}
```

- [ ] Run complete compiler tests, formatting; commit `[Feat] Add Typed Stored Tag Predicate`.

## Task 2: CLI Selectors And Diagnostics

Files: modify `src/main.rs`, `src/sql_filter/mod.rs`, `src/queue.rs`; create `tests/tag_filters.rs`.

- [ ] Add integration harness using isolated tempfile projects and binary commands with HOME redirected, ambient QQQ_SESSION/CODEX_THREAD_ID/CODEX_SESSION_ID/HERDR_ENV/HERDR_PANE_ID removed. Add initial failing test:

```rust
#[test]
fn tag_selectors_require_all_exact_labels() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "[frontend] Text only"]);
    ok(p, &["add", "Substring", "--tag", "frontend-extra"]);
    ok(p, &["add", "Case", "--tag", "Frontend"]);
    ok(p, &["add", "Match", "--tag", "frontend", "--tag", "界 面"]);
    assert_eq!(ids(&ok(p, &["list", "--tag", " frontend ", "--tag", "界 面"])), [4]);
    assert_eq!(ok(p, &["next", "--dry-run", "--tag", "frontend", "--tag", "界 面"])["id"], 4);
}
```

- [ ] Run `CARGO_INCREMENTAL=0 cargo test --locked --offline --test tag_filters`; expect unsupported `--tag`.
- [ ] Add identical fields to List and Next variants:

```rust
/// Require exact stored tag; repeat to require every label.
#[arg(long = "tag", value_name = "LABEL", allow_hyphen_values = true)]
tags: Vec<String>,
```

- [ ] Extend `CompiledFilter` with `required_tags: Vec<String>`, initialized empty by normal compiler. Add methods:

```rust
pub fn all() -> Self {
    Self { sql: "1".to_owned(), params: Vec::new(), required_tags: Vec::new() }
}
/// Labels must already pass shared normalization.
pub fn require_tags(&mut self, tags: Vec<String>) {
    for tag in &tags {
        self.params.push(Value::Text(tag.clone()));
        let predicate = functions::tag_sql(&format!("?{}", self.params.len()));
        self.sql = format!("({}) AND ({predicate})", self.sql);
    }
    self.required_tags = tags;
}
pub fn matches_tags(&self, tags: &[String]) -> bool {
    self.required_tags.iter().all(|tag| tags.contains(tag))
}
```

- [ ] At start of `run`, extract List/Next tag fields, call `tags::normalize`, map failures to `Info::invalid_argument("--tag", error.to_string())`. Compile current filter unchanged, append tags using `get_or_insert_with(CompiledFilter::all).require_tags(tags)`. Both operations precede project preflight. Existing downstream filter plumbing stays intact.
- [ ] Add `TagExcluded` variant to queue reason; replace current excluded-reason branch:

```rust
if !matches_filter {
    let reason = if filter.is_some_and(|filter| !filter.matches_tags(&task.tags)) {
        Reason::TagExcluded
    } else {
        Reason::FilterExcluded
    };
    reasons.push(reason);
}
```

- [ ] Extend CLI tests with invalid flags/predicates before project discovery and owner recovery; exact case/Unicode/SQL binding, query/status/filter composition, visible ancestors, archive/completed limits, owned reuse, dependency readiness and priority ties, explain exclusions. Verify `[]`, `null`, human no-match strings and default-agent JSON errors. For readiness tests create separate parent and prerequisite, then assert blocked task stays new until both complete. For diagnostics assert both `tag_excluded` and dependency reasons on same row, Luau-only exclusions stay `filter_excluded`.
- [ ] Run `CARGO_INCREMENTAL=0 cargo test --locked --offline --test tag_filters --test filters --test sql_filter --test cli`; run formatting; commit `[Feat] Route CLI Selection By Stored Tags`.

## Task 3: Wait, Watch, Dispatch And Aliases

Files: modify `tests/tag_filters.rs`, `tests/watch.rs`, `tests/dispatch.rs`, `tests/aliases.rs`.

- [ ] Add wait test: start tag-filtered waiter; commit untagged task and assert child remains running; commit matching tags and assert claimed ID. Add dry-run waiter with same steps and assert final task stays new. Use bounded deadline, kill/join on timeout.
- [ ] Add concurrent claims test: start three sessions against two matching tagged tasks plus unmatched task; collect IDs, assert exactly two distinct matching IDs and one null; unmatched status remains new.
- [ ] Add watch test with existing Watcher harness: initial empty array, untagged commit remains empty, edit `--set-tags "frontend, 界 面"` makes matching row appear, removal clears result, idle emits no repeats.
- [ ] Add dispatch test with existing fake Herdr harness: no matching tag -> null and zero tab creation; partial match stays new; complete match creates exactly one tab and claims matching task. Verify changed selectors reuse locally owned caller task without new tab.
- [ ] Add configured alias composition:

```rust
let p = Project::new("[alias]\nui = \"list --filter 'has_tag(\\\"frontend\\\") and status == \\\"new\\\"'\"\n");
p.ok(&["add", "UI", "--tag", "frontend", "--tag", "bug"]);
p.ok(&["add", "UI other", "--tag", "frontend"]);
assert_eq!(p.ok(&["ui", "--tag", "bug"]).as_array().unwrap().len(), 1);
```

- [ ] Run `CARGO_INCREMENTAL=0 cargo test --locked --offline --test tag_filters --test watch --test dispatch --test aliases`; formatting; commit `[Test] Verify Tag Routing Across Worker Modes`.

## Task 4: Docs And Delivery

Files: modify `README.md`, `docs/filter.md`, `docs/filter-functions.md`, `docs/reference.md`, this plan.

- [ ] Add repeatable tag examples, exact label and AND rules, spaced/Unicode labels, has_tag literal/boolean semantics, alias example, explain reasons and owned-task reuse. Examples:

```sh
qqq list --tag frontend --tag "UI review" --query layout --status new
qqq next --local --wait --tag frontend --tag bug
qqq list --filter 'has_tag("frontend") and priority >= 5'
qqq next --explain --tag "界 面"
```

- [ ] Run `cargo fmt --check`, `git diff --check`, `CARGO_INCREMENTAL=0 cargo test --locked --offline`, `CARGO_INCREMENTAL=0 cargo clippy --locked --offline --all-targets -- -D warnings`; require zero failures.
- [ ] Request independent review through requesting-code-review skill; fix verified findings with regression tests, rerun affected/full checks if code changes.
- [ ] Build release `CARGO_INCREMENTAL=0 cargo build --release --locked --offline`; run isolated CLI script covering multiple exact tags, has_tag composition, owned reuse, no-match, explain and pre-DB invalid errors. Record results.
- [ ] Mark plan completed, commit `[Docs] Document Tag Routing And Verification`. Follow finishing-a-development-branch skill: rebase onto master, full tests, fast-forward locally, full integrated verification. Install `CARGO_INCREMENTAL=0 cargo install --path . --locked --offline --force`; run isolated checks against installed binary, compare release/installed SHA256.
- [ ] Record evidence with `qqq message 182`, complete task, read back status; remove own worktree and safely delete merged branch. Start one persistent `qqq next --wait --local --json`, retain live session without status polling.
