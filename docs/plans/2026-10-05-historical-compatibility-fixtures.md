# Historical Compatibility Fixtures Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add immutable independent historical DB/snapshot compatibility fixtures.

**Architecture:** Frozen tagged SQL plus pinned schema12 extension seed checked-in
DB/tar artifacts. Catalog and independently projected canonical JSON describe
coverage, provenance, data and readiness. Shared Rust helper copies artifacts
before current-runtime migration/restore checks.

**Tech Stack:** Rust/rusqlite/serde_json/sha2/tar, Python stdlib sqlite3/tarfile.

## Task 1: Catalog contract and frozen sources

**Files:** new `tests/compatibility.rs`, `tests/common/compatibility.rs`,
`tests/fixtures/compatibility/manifest.json`, frozen sources below fixture root.

- [ ] Write inventory regression loading catalog and requiring fixture schemas
  equal1..current init DB user_version, accepted snapshots9..current and released
  snapshot coverage9/11. Require all recorded source/artifact hashes and explicit
  untagged provenance for12. Initial run must fail because catalog absent.
- [ ] Pin source files by `git show <full-commit>:<source-path>` from design
  commits. Record SHA-256 and `git rev-parse <commit>:<path>` blob IDs. Freeze
  original paths and release metadata in provenance JSON. Never copy working
  tree `src/sql` definitions.
- [ ] Add catalog/helper types with serde Deserialize, read-only source checks
  and isolated fixture copy. Helper must only open copied DB mutable:

```rust
pub fn copy_database(entry: &DatabaseFixture) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let project = dir.path().join(".qqq");
    std::fs::create_dir(&project).unwrap();
    std::fs::copy(root().join(&entry.database), project.join("qqq.db")).unwrap();
    for image in &entry.files {
        let target = project.join(&image.path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(root().join(&image.source), target).unwrap();
    }
    dir
}
```

## Task 2: Deterministic synthetic artifacts

**Files:** new `tests/fixtures/compatibility/generate.py`, DB/image/snapshot
artifacts, canonical JSON, fixture README.

- [ ] Generator accepts fresh output directory, reads frozen sources, refuses
  overwrite. Execute historical schema1 then frozen migrations only through
  requested source version; add schema12 pinned extension for12. Seed fixed
  task IDs3,8,13,21,34,55,89,144 and timestamps. Source-specific fields/actions
  follow design. Reserve sequences tasks233/images31/messages37/events89.
- [ ] Build expected current rows independently from seed/defaults: raw task,
  message, event, image, link, dependency, sequence data; readiness and owners.
  Before schema5 derive linked identity from synthetic Herdr link, bare owner
  from claim key. Before10 revision defaults1; absent priority/archive/tags/deps
  become0/false/empty. Parent links absent before2. Error unavailable before4.
- [ ] Include PNG task3/image7 and GIF task13/image19, embedded before6, file
  afterward. Record hashes and expected migrated paths for all versions.
- [ ] Build four format1 snapshots with manifest first, DB second, images in
  fixed ID order. Manifest format definition comes from frozen release source;
  GNU headers use mode0600, uid/gid/mtime0. State source-derived construction,
  not captured real user archive. Record tagged emission distinction9/11.
- [ ] Generate into fresh temp output, validate integrity/user_version/hashes,
  then add artifacts. Regenerate into second fresh output and compare complete
  tree hashes using same recorded SQLite version.

## Task 3: Canonical, readiness and isolation checks

**Files:** `tests/compatibility.rs`, shared helper, `tests/snapshot.rs`.

- [ ] Verify raw source integrity, foreign keys, exact populated features,
  available statuses/actions, IDs and reserved sequences; hash images in blobs
  or files as appropriate.
- [ ] For every DB fixture, copy then run current `list --all --json` to migrate.
  Query canonical SQL rows with stable explicit columns/order. Compare checked-in
  JSON. Run `status`/`next --explain` after migration and compare ready IDs,
  blockers/selection/owner keys. Existing-owner `next --local --session` returns
  active task without changing canonical state.
- [ ] Verify post-upgrade allocation respects reserved sequences for task,
  image, message and event. Mutate copied project only; before/after full fixture
  tree digest remains identical and no original sidecars appear.
- [ ] Restore each immutable snapshot to TempDir; inspect DB version before
  normal open, then reuse canonical checks. Replace schema9 test's current SQL
  reconstruction with immutable source fixture and existing restore assertions.
- [ ] Inventory probes fresh current init/backup for new schema/format coverage;
  manifest retains legacy/manual-conversion and unsupported-version exclusions.
  Update `docs/reference.md` compatibility section to accurately describe12.

## Task 4: Verification and delivery

- [ ] Run `CARGO_TARGET_DIR=/app/qqq/target cargo test --locked --offline
  --test compatibility --test snapshot --test identity --test images
  --test image_storage --test priority --test archive --test reopen`.
- [ ] Run full suite, fmt/diff checks, Clippy all targets with `-D warnings`,
  release build; ensure no runtime source or dependency changes.
- [ ] Independent review of provenance, schema coverage, independent expected
  state, allocation/readiness assertions and immutable copy behavior.
- [ ] Commit fixture work, refresh/rebase/merge master, verify integrated focused
  checks. Runtime unchanged; installed binary already matches current runtime.
  Record queue evidence, complete170, clean owned worktree/branch, resume wait.

## Evidence

Inventory verified against current source and remote tags. Baseline migration/
snapshot suite running; implementation has not started.
