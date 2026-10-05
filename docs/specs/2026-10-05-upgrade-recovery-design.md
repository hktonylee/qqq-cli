# Recoverable automatic upgrades

## Contract

Normal opens of supported schemas1..11 save a verified durable pre-upgrade
archive before any migration SQL or image/staging changes. Existing-schema12
opens, schema0 initialization, read-only diagnostics and every dry run create
none. Unsupported/manual layouts fail before attempted upgrades. Existing JSON
command result shapes remain unchanged; human stderr reports saved archive path
before migration, including when migration later fails.

## Lock and copy

Use existing BEGIN IMMEDIATE migration transaction as one coordination boundary.
Re-read schema after acquiring it; a contender seeing current schema saves nothing.
While lock excludes qqq writers and attachment staging, open separate read-only
SQLite connection and use VACUUM INTO to create consistent original-schema DB.
Do not copy live DB bytes or invoke Db::open/ordinary backup on source. Check
integrity, foreign keys, supported schema, description layout, status/owner
relationships and dependency graph where available. Copy required external image
bytes for schemas6+; schemas1..5 retain embedded BLOBs and need no external files.
Pending deletion images may be read from validated task staging without changing
live staging; recovery places required bytes at canonical live image paths.

## Archive publication

Store archives in real sibling directory <project>/.qqq-upgrades/, outside .qqq.
Reject symlink/unsafe directory destinations. Manifest2 carries upgrade metadata:
canonical project root, original schema, destination schema and creation timestamp,
plus DB/file sizes and SHA256 hashes. Ordinary portable manifest1 stays unchanged.
Names include source/target schemas and random suffix; atomic no-clobber publication,
file and parent-directory fsync precede migration. Verify staged DB, attachments,
manifest and final archive before returning. Snapshot failure rolls back untouched
source; valid archive persists on migration failure, success or later retry. Retries
may create another uniquely named record; concurrent successful contenders produce
one archive for actual upgrade. No garbage collection/removal policy added.

## Recovery

qqq restore --recovery ARCHIVE explicitly accepts upgrade manifest2 and schemas
1..current; ordinary restore still accepts portable manifest1/schemas9..current.
Shared strict extraction rejects unsafe paths, duplicates, unsupported format,
hash/size mismatch and invalid schema/data. Metadata source schema must match DB;
source<target<=current. Embedded schema images remain in DB, manifest external
image list must be empty. External schema images must match DB rows exactly.
Install unchanged source schema atomically in new/empty project; never invoke normal
DB open during recovery, never replace populated target. Next normal open creates
new recovery archive then retries upgrade. Original IDs, sequences, claims, content,
links, relationships and image bytes survive recovery and subsequent upgrade.

## Preview behavior

Move next --dry-run to Db::open_read_only and return peek_next_filtered before
normal open/deletion recovery. Old schemas report existing migration_required error;
current schemas retain queued-candidate semantics with no session discovery/claim.
This intentionally tightens prior preview behavior which allowed schema migration.

## Verification

Frozen historical fixtures drive original-schema row/schema/sequence equivalence
and byte hashes; all supported old schemas get automatic capture and recovery.
Exercise concurrent migration contenders, committed concurrent writer + attachments,
WAL source, valid claim ownership recovery, late SQL/image migration faults, snapshot
path/missing attachment/corruption rejection, commit busy rollback, no-clobber retries,
original store byte preservation, read-only commands and initialized/current no-op.
Recovery rejects metadata/hash/schema/claim errors before destination install.
Required compatibility gate includes recovery tests. Run focused/gate/full locked
suite, fmt, strict all-target Clippy, release/install tests and independent review.
