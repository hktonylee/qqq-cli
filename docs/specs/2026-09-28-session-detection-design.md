# Automatic Session Detection

Task 24: infer caller identity from environment without requiring qqq-specific
session configuration. Existing explicit `--session` and `QQQ_SESSION` override
automatic sources. Exact Herdr context (`HERDR_PANE_ID`, or `HERDR_ENV=1`) comes
next, then Codex `CODEX_THREAD_ID`, legacy `CODEX_SESSION_ID`, and existing unique
Herdr agent discovery by database cwd. Use namespaced JSON tuples for automatic
Codex identity: `["codex","id","<value>"]`.

Herdr lookup uses explicit `--pane HERDR_PANE_ID`, even without HERDR_ENV. Keep
reported session/terminal identities and active-claim retention. Invalid exact
context must fail before claim, without falling through to another client.
Native Codex ownership requires no Herdr installation/link and uses same
resolution for next, wait, complete, and release. Invalid selected env values
fail; higher-priority sources ignore lower-priority values. Aliases stay valid.

Configured dispatch must return caller's existing native Codex claim before
checking Herdr availability or spawning. Manual `herdr link` still discovers
Herdr context independently of native Codex env. Other clients supported through
Herdr's reported agent identity or explicit QQQ_SESSION; no guessed env names.

Local inspection confirmed both Codex variable names in installed binary and
current subprocess environment. CODEX_THREAD_ID matches current thread identity;
prefer it, retain CODEX_SESSION_ID compatibility. Test fixtures clear inherited
caller env so test outcomes do not depend on developer's hosting client.
