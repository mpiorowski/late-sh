# IRCD — Session Handoff / TODO

Companion to `devdocs/FRD-IRCD.md` (the spec). This file tracks **implementation
progress** and the exact next steps. Keep it current as tasks land.

Terminology: **ircd** = the IRC server we embed in late-ssh; **ircc** = an IRC
client. We write the "d".

## Ground rules (do not violate)

- LLM agents **must not** run `cargo test`, `cargo nextest`, or `cargo clippy` —
  the human owner runs verification. `cargo check`, `cargo build`, `cargo fmt`
  are allowed.
- Commit messages must not indicate Claude / Claude Code involvement.
- Tokens are stored **hashed only** (SHA-256 hex). Plaintext exists exactly once
  at mint time and is shown to the user once; never persisted, never logged.
- Don't call admin/moderator a "role" — use **tier/privilege**. "role" is
  reserved for user-facing flair.
- Ask permission before consulting the Advisor.

## Status overview

| # | Task | State |
|---|------|-------|
| 1 | FRD: sticky-join + v1 defaults | ✅ done |
| 2 | Vendor irc-proto into `vendor/irc-proto` | ✅ done |
| 3 | `irc_tokens` migration + `IrcToken` model | ✅ done |
| 4 | ircd core: listener, registration, auth, welcome burst | ✅ done |
| 5 | Channel projection + messaging bridge | ✅ done |
| 6 | Settings → Account: IRC token mint/revoke UI | ✅ done |
| 7 | Moderation mapping: ops, kicks, bans, KILL, server-ban enforce | ✅ done |
| 8 | TLS listener (in-process rustls) on 6697 | ✅ done |
| 9 | ircd integration tests + CONTEXT.md + splash tips | 🚧 docs/tips/unit coverage done; integration tests pending |

## Current build state

`cargo check --workspace` passes as of the forced IRC lounge membership fix.
LLM agents still must not run `cargo test`, `cargo nextest`, or `cargo clippy`
for this handoff; leave the broader integration suite to the human owner.

## Task #6 — IRC token UI: done

### Done

**`late-core/src/models/irc_token.rs`** — `IrcToken` model. Methods: `mint`
(upsert, resets `last_used`/`created`), `revoke`, `find_for_user`,
`find_by_token` (hash lookup), `touch_last_used`. `TOKEN_PREFIX = "late-irc-"`,
160-bit entropy, 32-char Crockford-ish alphabet.

**`late-ssh/src/app/profile/svc.rs`**
- Added `irc_registry: Option<IrcRegistry>` field + `with_irc_registry(..)` builder.
- New `ProfileEvent` variants: `IrcTokenStatus { user_id, status: Option<IrcTokenStatus> }`,
  `IrcTokenMinted { user_id, token }`, `IrcTokenRevoked { user_id }`.
- New public struct `IrcTokenStatus { created, last_used }` (+ `From<&IrcToken>`).
- New fire-and-forget methods: `load_irc_token_status`, `mint_irc_token`,
  `revoke_irc_token` (and their `do_*` impls). Mint **and** revoke call
  `irc_registry.disconnect_user(...)` so the old token's live connections drop
  immediately (FRD §5 T7): reason `"IRC token reset"` on re-mint,
  `"IRC token revoked"` on revoke.

**`late-ssh/src/main.rs`** — `irc_registry` is now created once (just after
`session_registry`), passed to `ProfileService::with_irc_registry(..)`, **and**
reused in the `State` literal (was previously `IrcRegistry::new()` inline). The
ircd `serve::run` path uses the same registry via `State`, so disconnects from
the settings UI reach live connections. ✔ single shared instance.

**`late-ssh/src/app/settings_modal/state.rs`**
- `AccountRow` now `LinkAccounts | IrcToken | DeleteAccount` (ALL is `[_; 3]`).
- New `IrcTokenFocus { Primary, Revoke }` and `IrcTokenDialogState`
  (open/status/focus/revealed_token/confirming_revoke/pending/message) with
  getters. `status: Option<Option<IrcTokenStatus>>` — outer `None` = still
  loading; `Some(None)` = no token; `Some(Some(_))` = active token.
- Field wired into `SettingsModalState` + `new()` + `open_from_profile()`.
- Methods: `irc_token_dialog`, `open_irc_token_dialog` (triggers status load),
  `close_irc_token_dialog`, `move_irc_token_focus`, `dismiss_irc_token_reveal`,
  `activate_irc_token_focus` (mint/reset, or arm-then-confirm revoke).
- `drain_profile_events` handles the three new events (and the existing `Error`
  variant now also clears the IRC dialog's pending/confirming + shows message).

**`late-ssh/src/app/settings_modal/input.rs`**
- Added dispatch guard: if `irc_token_dialog().open()` →
  `handle_irc_token_dialog_input(app, event); return;`
- `AccountRow::IrcToken => open_irc_token_dialog()` in the account-tab Enter/Space arm.
- Import line updated to bring in `IrcTokenFocus`.

- `late-ssh/src/app/settings_modal/input.rs` now handles the IRC token dialog:
  pending Esc close, reveal dismissal, activation, and button focus movement.
- `late-ssh/src/app/settings_modal/ui.rs` now renders the Account row and dialog
  states: loading, no token/create, active/reset/revoke, and one-time reveal.
- Validation run: `cargo fmt -p late-ssh -p late-core`; `cargo check --workspace`.

## Task #7 — Moderation mapping: done

Spec: FRD-IRCD.md §9. Implemented through the existing moderation service path,
so IRC-originated moderation uses the same permission checks, DB writes, audit
logging, session effects, and broadcast events as `/mod` in the TUI.

- NAMES/353 already prefixes mods/admins with `@`; WHOIS shows admins as ircops.
- `KICK #chan nick [:reason]` runs the room kick path for channel ops.
- `KILL nick :reason` runs the server kick path for admins.
- `MODE #chan +b nick!*@*` / `-b nick!*@*` runs room ban/unban. Other masks are
  refused with a notice. `MODE #chan b` returns 367/368 ban list numerics.
- Room kick/ban/unban events project to IRC as KICK / MODE +b / MODE -b.
- Moderator tier changes project to all joined channels as MODE +o/-o; client
  attempts to set op modes remain refused because op is tied to late.sh tier.
- Server kick/ban session effects now also call `IrcRegistry::disconnect_user`,
  so live IRC clients receive an ERROR and close.

## Task #8 — TLS listener: done

Spec: FRD-IRCD.md §5.2 / §7. Implemented as a single listener:

- Plaintext dev mode when `LATE_IRC_TLS_CERT` / `LATE_IRC_TLS_KEY` are absent
  (default port 6667).
- TLS mode when both env vars are present; certs/keys are loaded from PEM and
  accepted with `tokio_rustls::TlsAcceptor`. If `LATE_IRC_PORT` is omitted in
  TLS mode, default port is 6697.
- Config validates both-or-neither TLS env vars. Production cert requirements
  remain: publicly trusted CA, full chain, exact hostname (e.g. `irc.late.sh`).

## Task #9 — Tests + docs (in progress)

- Done: repo-root `CONTEXT.md` now documents the embedded IRC listener,
  Settings > Account token flow, protocol bridge, moderation projection,
  disconnect semantics, and agent test-command boundary.
- Done: splash tip pools now mention IRC token setup from Settings > Account.
- Done: pure unit coverage exists for IRC `MODE +b/-b` nick-mask parsing in
  `late-ssh/src/ircd/conn.rs`.
- Done: manual Docker/tmux/`nc` smoke on 2026-06-11 using Docker Postgres and
  local `late-ssh` with `LATE_IRC_ENABLED=1`, `LATE_IRC_PORT=16667`. Covered
  good-token registration/welcome/MOTD, forced `#lounge` join, NAMES self-entry,
  LIST, sticky `PART #lounge` refusal/rejoin, bad-token `464` rejection, and
  `PRIVMSG #lounge` persistence to `chat_messages`.
- Done: first ircd integration tests under `late-ssh/tests/ircd/` using
  testcontainers and a real TCP listener. Covered: good-token registration,
  canonical nick in welcome, forced `#lounge` join, bad-token `464` refusal,
  sticky `PART #lounge` refusal/rejoin, `PRIVMSG #lounge` persistence to
  `chat_messages`, self-echo suppression on the sender connection, and
  disconnect-on-revoke through the shared `IrcRegistry`.
- Pending: broaden ircd integration coverage. Cover: banned-token auth refusal,
  nick-lock/change refusal, IRC-to-IRC channel round-trip, DM ↔ `/msg` query,
  LIST/NAMES/WHO shapes, private channel visibility, multi-connection bouncer
  behavior, moderation projection, and TLS listener smoke.
- Pending: MOTD live lounge banner plumbing (FRD: motd carries the #lounge
  banner; `motd.rs` currently uses static text).

## Key files map

```
devdocs/FRD-IRCD.md                         spec (source of truth)
vendor/irc-proto/                           vendored irc-proto (MPL-2.0); README has provenance
late-core/migrations/083_create_irc_tokens.sql
late-core/src/models/irc_token.rs           IrcToken model
late-core/src/models/user.rs                + staff_flags_by_ids(...)
late-core/src/models/chat_room.rs           + list_irc_channels(...)
late-ssh/src/config.rs                      IrcConfig (enabled=false default, ports, caps)
late-ssh/src/state.rs                       + irc_registry field
late-ssh/src/main.rs                        constructs irc_registry once, spawns serve::run
late-ssh/src/ircd/mod.rs                    module root
late-ssh/src/ircd/replies.rs                numerics, prefixes, server identity
late-ssh/src/ircd/registry.rs               IrcRegistry (live conn control handles)
late-ssh/src/ircd/proj.rs                   pure channel/message projection helpers
late-ssh/src/ircd/auth.rs                   token auth (AuthOutcome)
late-ssh/src/ircd/motd.rs                   motd_lines (TODO: live lounge banner)
late-ssh/src/ircd/conn.rs                   per-connection state machine (~700 loc)
late-ssh/src/ircd/serve.rs                  listener / accept loop / shutdown
late-ssh/src/app/profile/svc.rs             token mint/revoke/status service methods + events
late-ssh/src/app/settings_modal/state.rs    IrcTokenDialogState + AccountRow::IrcToken
late-ssh/src/app/settings_modal/input.rs    IRC token dialog dispatch/input
late-ssh/src/app/settings_modal/ui.rs       Account row + IRC token dialog rendering
late-ssh/tests/helpers/mod.rs               test State has irc_registry + IrcConfig::default
```

## Config / runtime notes

- `IrcConfig` defaults: `enabled = false`, `port = 6667` (or 6697 when
  `LATE_IRC_TLS_CERT` / `LATE_IRC_TLS_KEY` are configured and `LATE_IRC_PORT`
  is unset),
  `max_conns_global = 200`, `max_conns_per_user = 3`,
  `max_auth_failures_per_ip = 20`, `auth_failure_window_secs = 300`. All env-parsed,
  all optional. ircd only spawns when `config.irc.enabled`.
- The root `Makefile` intentionally enables plaintext ircd for generated local
  dev `.env` files with `LATE_IRC_ENABLED=1` and `LATE_IRC_PORT=6667`; optional
  TLS and tuning env vars are emitted as commented examples.
- Brute-force defense is **token strength** (160-bit), not rate limiting; the IP
  auth-failure limiter is a light backstop only (FRD §5).
- Registration: CAP/PASS/NICK/USER with 60s timeout; auth tarpit on failure
  (`AUTH_FAIL_DELAY = 1s`, `AUTH_FAIL_DELAY_LIMITED = 8s`).
