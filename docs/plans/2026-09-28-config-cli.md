# Config CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Git-style list/get/set/unset for existing user config.
**Architecture:** New config editing module using shared path helper, toml_edit and fs2 lock. CLI dispatches before opening DB; output renderer handles config-specific results.
**Tech Stack:** Rust, clap ArgGroup, toml/toml_edit, tempfile, fs2, integration tests.

- [x] Add `tests/config.rs` using real binary with temporary HOME and no DB. Basic assertion:
  ```rust
  assert_eq!(ok(&["config", "--list"]), serde_json::json!({}));
  ok(&["config", "alias.ls", "list --watch"]);
  assert_eq!(ok(&["config", "--get", "alias.ls"]), "list --watch");
  ```
  Cover invalid bool/key, existing comments/unknown data, unset, quoted keys, symlinks/modes, concurrent independent sets, and runtime alias use.
- [x] Run `cargo test --locked --test config`; verify missing-command failures.
- [x] Expose `config::path() -> Option<PathBuf>`. Add `config_cli::execute(list,get,unset,key,value) -> Result<serde_json::Value>`; read path with DocumentMut, use Key::parse, recursively get/set/remove values. Flatten table leaves with TOML-escaped path segments. Validate updated document via `toml::from_str::<config::Config>`.
- [x] Add dependencies `toml_edit = "0.22"`, `fs2 = "0.4"`. Lock target sidecar via FileExt::lock_exclusive, reread after lock, preserve mode, write/sync NamedTempFile then persist over destination. Follow existing symlink target.
- [x] Add Config command with mutually exclusive list/get/unset/key actions, optional positional value requiring key. Dispatch config before `Db::open`. Add config rendering with plain scalar reads and mutation confirmations.
- [ ] Run focused tests, full suite, fmt, clippy, diff check. Request review, address blockers. Document syntax and JSON shapes.
- [ ] Commit, rebase, merge master, verify merged checks, complete task 21 through qqq, continue `next --wait`.
