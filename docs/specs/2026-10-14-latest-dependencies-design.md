# Latest Dependencies Design

Task #231 requests the latest dependencies. Interpret this as the latest stable
Rust crates in `Cargo.toml`, including target-specific dependencies and major
version upgrades. Verify releases using crates.io's `max_stable_version`, since
the most recently published version can be a backport.

Update the manifest constraints that exclude current stable releases, retaining
the existing exact-pin style and dependency features. Refresh `Cargo.lock` to
the newest releases allowed by the resulting dependency graph. Include target
dependencies and transitive packages; retain immutable historical compatibility
fixtures.

Latest direct dependencies require Rust 1.88: image 0.25.10, Ratatui 0.30.2 and
SHA-2 0.11.1. Set the declared minimum to the highest Rust requirement in the
resolved graph and update the installation requirements in `README.md` and
`docs/reference.md`. Verify a build with that minimum toolchain.

The manifest changes are arboard 3.6.1, image 0.25.10, Ratatui 0.30.2,
rusqlite 0.40, SHA-2 0.11, TOML 1.1, toml_edit 0.25 and Unicode segmentation
1.13.3. Other direct constraints already admit their latest releases. Preserve
the Linux Crossterm `use-dev-tty` feature and the requested Wayland, image,
Luau, serde and bundled SQLite features.

Adapt only API calls required by the upgrades. Existing tests remain the
behavior contract for queue ownership, filters, configuration edits, snapshots,
clipboard encoding, Unicode handling and terminal interactions. Investigate
failures before changing code or expectations.

Verification requires a fresh baseline, all-target compilation, full tests,
formatting, strict Clippy, release build and minimum-toolchain compilation.
Use a temporary directory outside the repository because `/tmp` is full and
queue discovery intentionally searches ancestor directories. Independently
review the completed diff, integrate locally, install the verified release,
smoke-test the installed binary, then complete #231 and resume one blocking
queue waiter without repeated model status checks.
