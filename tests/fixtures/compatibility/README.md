# Historical compatibility fixtures

Immutable synthetic artifacts for qqq upgrades. Normal test runs copy these
files to temporary projects; they never rebuild golden DBs from current SQL.
`manifest.json` lists artifact hashes, versions, image paths and independent
canonical expectations. `provenance.json` records original source paths, full
commits, Git blob IDs and SHA-256 hashes for frozen source bytes.

## Coverage and provenance

| Artifact | Coverage | Source |
| --- | --- | --- |
| Normal-open DBs | `user_version` 1–9 | SQL definitions frozen from v0.1.1 |
| Normal-open DBs | 10–11 | SQL definitions frozen from v0.4.0 |
| Normal-open DB | 12 | v0.4.0 base plus pinned schema-12 introducing SQL |
| Portable snapshots | Format 1, DB schemas 9–12 | Frozen format definitions, deterministic synthetic GNU tar |

Verified release inventory:

| Tag | Commit | DB schema | Snapshot format / DB schema |
| --- | --- | --- | --- |
| v0.1.1 | `0ea59d1f13cd5df84d08f6ee62d88cfc996e2e9a` | 9 | 1 / 9 |
| v0.2.0 | `47b1b59438a5c34504e690b0bd03acbe87362bff` | 9 | 1 / 9 |
| v0.3.0 | `77701f5b9feee40ba56d0dd61d7bfc63a310a512` | 9 | 1 / 9 |
| v0.4.0 | `5c97d0c0dda62603bdd0e2be15e0f4296157220a` | 11 | 1 / 11 |

Schema 10 is an accepted intermediate definition. Tagged releases emitted
schemas 9 and 11. Schema 12 has no release tag in this inventory: its extension
comes from commit `6c67d0ddac81faffceb94104d4486ceaf57cd78d`, package version
0.4.0. Catalog explicitly marks this development provenance. Snapshots are
source-derived synthetic archives, not recordings of released binaries.

Schema 0 is initialization-only. Legacy `title` / `pending` layouts need manual
conversion and are outside normal-open compatibility; schema-1 fixtures use
the compatible description/new definition shipped in v0.1.1. Future unknown
schemas and snapshot formats remain unsupported.

## Synthetic data

Every DB has multiline Unicode descriptions; task IDs 3, 8, 13, 21, 34, 55,
89, 144; image IDs 7 and 19; messages and events; two active claims; synthetic
linked/bare identities. Fields appear when their historical schema supports
them: parents (2), error (4), structured identity (5), file images (6), priority
(7), archive (8), reopen events (9), revisions (10), extra prerequisites (11),
tags (12). Earlier DBs contain embedded PNG/GIF bytes. Later DBs carry matching
image files. All timestamps and session/project values are synthetic.

SQLite sequences reserve tasks=233, images=31, messages=37, events=89, beyond
existing IDs. Tests verify these values survive migration and govern new
allocations. Canonical expectations project explicit seed data and migration
defaults independently; they are not captured from current qqq output.

## Reproduction

Generator uses Python stdlib and SQLite **3.53.1**. That engine version is
required for byte-for-byte reproduction; normal Rust tests do not need it.
Frozen SQL is the only schema input. Generator refuses existing output paths
and checks source hashes before generation.

```sh
python3 tests/fixtures/compatibility/generate.py /tmp/qqq-compatibility-a
python3 tests/fixtures/compatibility/generate.py /tmp/qqq-compatibility-b
diff -r /tmp/qqq-compatibility-a /tmp/qqq-compatibility-b
```

Output includes `assets/`, `databases/`, `expected/`, `snapshots/` and
`manifest.json`. Review new output before replacing checked-in artifacts;
retain frozen sources and provenance. Tar entries are regular files, manifest
first, then DB and images in fixed ID order, with uid/gid/mtime=0 and mode 0600.

```sh
./scripts/check-compatibility.sh
cargo test --locked --test compatibility --test snapshot
```

Inventory test probes current initialization and backup output. New schema or
format support without matching fixtures fails. Other checks verify hashes,
raw integrity and foreign keys, canonical migration/restore state, queue
readiness, ownership, sequence allocation, image bytes and original-file
immutability, including absence of new SQLite sidecars.

CI and publication require same compatibility script. Extended matrix checks
reopen/concurrent migration idempotence, SQL/image rollback and retry, future/
legacy/invalid DB and snapshot rejection, diagnostics/import read-only behavior,
read-only next preview semantics and destination preservation for failed restores.

Automatic upgrade recovery matrix uses unchanged historical DB fixtures to compare
original schema, table rows, sequences, claims and attachments before/after explicit
`restore --recovery`. It tests all old schemas, unique retained archives, concurrent
openers/writers, WAL snapshots, SQL/image/commit rollback and repaired retry,
snapshot write/verification failures and tampered metadata/hash/claim rejection.
Recovery format2 is distinct from frozen portable format1; portable lower bound
remains schema9. Initialization/current-schema/read-only commands save no archives.
