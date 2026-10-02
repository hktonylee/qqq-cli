# Project-local `.qqq` directory

## Storage and discovery

`qqq init` creates `.qqq/qqq.db` in current working directory. Repeating `init` opens same database without clearing tasks. Other commands search current directory, then each parent through filesystem root for nearest `.qqq` directory. Once found, they open its `qqq.db`; a missing or invalid database in that nearest directory is an error, not permission to use an outer project. A child project can create its own `.qqq` without changing its parent.

Database schema, image blobs, task operations, and SQLite transaction behavior stay unchanged. `.qqq/images` remains available for future file storage; this task does not change image storage. The project root used for Herdr cwd matching remains the parent of `.qqq`.

## Existing databases

Existing root-level `qqq.db` files stay untouched. The new CLI does not discover them. README gives a one-time SQLite backup procedure to move existing data into `.qqq/qqq.db` while writers are stopped. Legacy root-level DB files remain ignored by Git. The entire `.qqq/` directory is ignored by Git.

## Verification

Integration tests cover initial and repeat `init`, nested cwd lookup, nearest-project precedence, missing inner DB, and root-level legacy DB non-discovery. Existing tests that inspect SQLite use `.qqq/qqq.db`. Herdr tests confirm cwd matching still uses project root. Full Rust test suite, formatting, and Clippy must pass.
