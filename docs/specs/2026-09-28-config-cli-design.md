# Config CLI

Use existing user file `~/.config/qqq/config.toml`, independent of project DB.
`qqq config --list` lists explicitly stored dotted keys; `--get KEY` and bare
`KEY` read a value; `KEY VALUE` sets it; `--unset KEY` removes it. Actions are
exclusive. Missing values/keys and invalid TOML paths fail with exit 1; argument
errors use exit 2. No HOME is an error for config commands. Missing config lists
empty and is only created by set. Reads never create files or directories.

Keys follow TOML dotted-key syntax, including quoted segments. Alias values are
always strings. `herdr.next-to-new-agent` accepts true/false. Other values parse
as TOML values when possible and otherwise become strings. Candidate documents
validate against existing typed config before persistence; invalid known types
do not corrupt config. Unknown settings remain supported and preserved.

Preserve comments, ordering, unknown values and existing permissions with
toml_edit. Use exclusive OS advisory lock on a persistent sidecar file before
reading/mutating, then write/sync a temporary file in target directory and
atomically replace. Symlinked config resolves to target; preserve symlink.
Concurrent CLI writes to separate keys must both survive. Config editing cannot
overwrite a non-table path segment. Unset does not remove unrelated values.

Human list shows sorted `key=value` lines; get shows value; set/unset confirms
action. JSON list is flat key/value object, get returns value, mutations return
`{"key":KEY,"value":VALUE}` (null for unset). Listing/read strings retain data;
human rendering escapes terminal controls. No DB is opened for these commands.

Tests exercise commands outside project, runtime alias/Herdr compatibility,
typed values, quoted key segments, comment/permission/symlink preservation,
failure immutability, argument conflicts, and concurrent writes.
