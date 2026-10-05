# Release checklist

Run from repo root on intended release commit. Package is `qqq-cli`; installed
command is `qqq`. Package version and DB schema version are separate contracts.

- [ ] Review final migration/restore code against [published compatibility table](reference.md#data).
  Current schema, every automatic-upgrade source, portable format/schema range,
  recovery format/source range and legacy manual-conversion exclusions must agree.
- [ ] For schema additions, extend immutable [fixture matrix](../tests/fixtures/compatibility/README.md)
  and catalog support metadata. Review independent canonical/default/readiness
  expectations, original hashes, attachment bytes, source provenance and release
  inventory. Do not regenerate fixtures from current SQL during tests or label
  development/source-derived artifacts as released-binary captures.
- [ ] Verify all historical migrations, repeated/concurrent opens, preserved
  IDs/sequences/history/claims/images/readiness and new field defaults. Require
  SQL/image rollback, repaired retry, future/legacy/invalid-data rejection and
  unchanged read-only/dry-run sources.
- [ ] Verify portable backup/restore boundaries and separate `restore --recovery`
  mode. Check pre-upgrade snapshot directory/naming, original schema/attachments,
  durable no-clobber publication before migration, retained failed/successful
  records and snapshot failure abort. Restore into fresh directory, inspect
  original schema, run normal upgrade, then `qqq doctor`. Confirm invalid archives
  preserve destination and populated targets remain rejected.
- [ ] Update user-visible upgrade notes: schema change, new defaults, supported
  sources/formats, recovery path, manual conversion needs and worker coordination.
  State claims stay explicit: migration does not expire or transfer ownership.
- [ ] Run focused documentation/examples gate and complete compatibility gate:

  ```sh
  ./scripts/check-compatibility-docs.sh
  ./scripts/check-compatibility.sh
  ```

- [ ] Run existing quality/build checks; require CI pass on Ubuntu 24.04 and macOS 14:

  ```sh
  cargo fmt --check
  cargo clippy --locked --all-targets -- -D warnings
  cargo test --locked
  cargo build --locked --release
  ./target/release/qqq --help
  ./target/release/qqq --version
  ```

- [ ] Choose unused package version. Update `Cargo.toml` and matching `qqq-cli`
  entry in `Cargo.lock`; verify release binary reports same version. Existing
  publish workflow requires tag `v<package.version>` and package license metadata.
  Check intended commit/tag before publication; preserve these checks alongside
  compatibility gates.
- [ ] Verify crate packaging without uploading:

  ```sh
  cargo publish --locked --dry-run --registry crates-io
  ```

- [ ] Review [CI](../.github/workflows/ci.yml) and [publish workflow](../.github/workflows/publish.yml)
  results. Both require full tests, focused docs/examples gate and complete
  compatibility gate before release packaging/publication. Manual publication
  workflow defaults to `dry_run=true`; package dry run does not prove upload
  authorization. Publish only intended verified version using existing workflow
  in [publication guide](reference.md#publish-to-cratesio).

Record tested commit, commands/results, schema/default/recovery notes and any
remaining failures in release review. Published versions cannot be overwritten.
