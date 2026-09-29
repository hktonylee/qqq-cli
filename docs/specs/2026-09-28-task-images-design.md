# Images Through Task Commands

Task #23 removes standalone `image`. User selected repeated `--image PATH` on
`add` and `edit`, TUI paste support, export through `show`.

## Interface

`qqq add TITLE --image a.png --image b.jpg` creates task and copies attachment
bytes into SQLite. `qqq edit ID --image a.png` appends attachments without opening
editor; omitted fields, existing attachments, ownership and dependency remain.
Negative edit indexes resolve before reading attachments. Flags work alongside
title, description and `--set-status new`. Add composition may combine flagged
attachments with pasted TUI images. Cancelled or failed composition saves nothing.

`qqq show TASK --export-image IMAGE --output PATH` displays task and exports one
attachment belonging to that task. Both flags require each other. Destination
must not exist. JSON includes normal task detail plus an `export` object with
image ID, path and bytes; ordinary show retains its existing shape. Human output
appends export confirmation. No standalone image command remains.

## Storage

Shared `ImageInput` reader checks regular files, reads at most 20 MiB plus one
byte, validates PNG/JPEG/GIF/WebP signatures. No schema change or migration.
Task creation, field/status updates, release event and all image inserts share
one immediate transaction. Validation precedes writes. Any validation, ownership
or insert failure leaves task, events and attachments unchanged. Image-only edit
updates task timestamp, ensuring list watchers see commit. Export queries both
task ID and image ID before creating destination; no path derived from image name.

## TUI Integration

Task #22 supplies `ImageInput`, `Composition` and paste UI. Reuse its validator
when integrated. Its composition persistence delegates to attachment-aware add
and edit, preserving atomic saves and existing attachments. CLI paths and pasted
images share validation and persistence; $EDITOR remains available.

## Verification

CLI tests cover repeated attachments, image-only and negative-index edit, byte
round-trip after removing source, scoped export, no overwrite, flag pairing,
unsupported/oversized/nonregular/missing files, atomic release/field/image failure
and database-trigger rejection. Update legacy image and alias tests to selected
interface. Run focused tests first, full Rust suite, formatting, Clippy and release
build after TUI integration. Review isolated diff before local merge.
