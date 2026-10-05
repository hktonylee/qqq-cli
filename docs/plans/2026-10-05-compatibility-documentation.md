# Compatibility Documentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Keep published upgrade/support facts tied to immutable fixtures, runtime schema/archive output and release checks.

**Architecture:** Add a marked, human-readable support table to `docs/reference.md`. A focused Rust integration test compares only that table with fixture support metadata and actual initialized/upgraded DB/archive versions. Existing compatibility tests continue verifying every historical schema and failure boundary; golden artifacts remain unchanged. A small shell entry point runs the documentation contract independently, required by both CI OSes and publication. Recovery examples are executed directly from marked documentation blocks in temporary fixture projects.

**Tech Stack:** Rust integration tests, existing SQLite/tar fixture helpers, POSIX shell, Markdown, GitHub Actions.

---

## Task 1: Add focused documentation gate

**Files:**
- Create: `tests/compatibility/documentation.rs`
- Modify: `tests/compatibility.rs`
- Create: `scripts/check-compatibility-docs.sh`
- Modify: `.github/workflows/ci.yml`, `.github/workflows/publish.yml`

- [x] Verify unchanged compatibility baseline with `CARGO_TARGET_DIR=/app/qqq/target cargo test --locked --offline --test compatibility` (32 tests).
- [x] Add a failing `compatibility_documentation_matches_fixture_matrix_and_runtime` test reading the `compatibility-support:start/end` block. Initialize a temporary project, read `PRAGMA user_version`, verify catalog DB schemas are exactly `1..=current`, portable fixture schemas exactly `9..=current`, emit portable and upgrade archives, compare runtime format versions with table rows and fixture metadata. Check legacy exclusions against catalog. Fail for missing or duplicate contract blocks.
- [x] Run `cargo test --locked --offline --test compatibility documentation::compatibility_documentation` and observe missing support table failure.
- [x] Add the table with rows: current=12, normal-open=1–12, automatic upgrade=1–11, portable format=1/schema=9–12, recovery format=2/source=1–11, initialization-only=0, manual conversion=`title`, `pending`.
- [x] Create executable focused gate:

```sh
#!/bin/sh
set -eu
compatibility_docs_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
cd "$compatibility_docs_dir/.."
exec cargo test --locked --test compatibility documentation::compatibility_documentation "$@"
```

- [x] Add unconditional `Check compatibility documentation` workflow step before compatibility/package checks, using `RUSTUP_TOOLCHAIN: stable`. Retain all existing tag/package-version checks.
- [x] Prove temporary changes to published schema, fixture support metadata and current schema constant fail. Restore exact bytes after each experiment; append unrelated prose and prove gate still passes.

## Task 2: Publish verified upgrade and recovery guidance

**Files:**
- Modify: `docs/reference.md`, `tests/fixtures/compatibility/README.md`
- Extend: `tests/compatibility/documentation.rs`

- [x] Explain new field defaults (`parent_id=NULL`, identity derivation, priority=0, archived=false, revision=1, prerequisites empty, tags empty), preserved IDs/sequences/descriptions/history/claims/readiness and embedded→file image migration against final SQL/code.
- [x] Document `.qqq-upgrades/schema-<source>-to-<target>-.upgrade-<unique>.tar`, format2 metadata/hash checks, coordinated writer snapshot, no-clobber durable publication before migration, retained failed/successful records, snapshot failure abort and explicit recovery preserving original schema.
- [x] Replace placeholder recovery filename with runnable interactive selection of printed absolute archive path. Use `mktemp -d`, `restore --recovery`, SQLite inspection, then `qqq list` and `qqq doctor` in fresh directory. Clarify inspection happens before normal CLI open upgrades restored DB.
- [x] Mark portable/recovery shell examples, execute their exact commands through `/bin/sh -eu` in temporary historical fixture projects; use selected archive path as stdin, isolate `TMPDIR` and qqq binary on `PATH`. Verify resulting schema, images and claims with existing canonical expectations.
- [x] Explain stop/upgrade/restart workers together, older binaries rejecting newer schemas, explicit completion/release ownership with no timeout or migration reassignment.
- [x] Cross-link focused docs gate and fixture inventory; preserve historical provenance distinctions.

## Task 3: Add release checklist, verify and integrate

**Files:**
- Create: `docs/release-checklist.md`
- Modify: `docs/reference.md`

- [x] Add checklist for schema/matrix/golden metadata, both restore modes, upgrade snapshot recovery/failure paths, defaults/claim notes, docs gate, fmt/Clippy/tests/release build, package/tag/lock/binary version checks and publish dry run. Link current workflows; no new distribution or MSRV proposal.
- [ ] Run focused docs gate and complete compatibility leaf gate; parse workflows and verify unconditional ordering. Run `cargo fmt --check`, strict all-targets Clippy, full locked/offline suite and release build as appropriate.
- [ ] Obtain independent read-only review through requesting-code-review skill; fix concrete findings, rerun affected checks.
- [ ] Record actual checks and stale-metadata experiments here and in qqq task message. Commit verified changes, rebase onto master, fast-forward locally, verify integrated gate, remove owned worktree/branch, complete task, resume single queue waiter.

## Validation evidence

- Baseline: 32 compatibility tests passed, `/tmp/qqq-task-173-baseline.log`.
- Focused support-table red: missing marker failed one test; table made it pass, `/tmp/qqq-task-173-docs-{red,green}.log`.
- Both executable-example tests failed missing markers before documentation changes, `/tmp/qqq-task-173-examples-red.log`.
- Final focused gate: 3 tests passed; exact portable commands on schemas 9–12, exact recovery/inspection/upgrade commands on all source schemas 1–11, `/tmp/qqq-task-173-docs-final.log`.
- Temporary stale published schema, catalog support schema, missing schema-12 fixture and `SCHEMA_VERSION=13` mutations each failed one focused test with exit101. Unrelated prose passed. Original files restored exactly; runtime SQL/source and golden catalog/artifacts have no final diff. Logs: `/tmp/qqq-task-173-drift-*.log`.
- Complete compatibility leaf gate: 152 tests / 7 binaries passed, `/tmp/qqq-task-173-gate.log`.
- Workflow YAML parsed; both docs gates unconditional, stable toolchain, before compatibility/package steps. Existing release metadata/tag checks unchanged. Shell syntax, formatting and diff checks passed.
- Strict all-targets Clippy passed, `/tmp/qqq-task-173-clippy.log`.
- Full suite, release build, independent review and integrated verification still running/pending.
