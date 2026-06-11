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
| 7 | Moderation mapping: ops, kicks, bans, KILL, server-ban enforce | ⬜ pending |
| 8 | TLS listener (in-process rustls) on 6697 | ✅ done |
| 9 | ircd integration tests + CONTEXT.md + splash tips | ⬜ pending |

## Current build state

`cargo check --workspace` passes as of Task #6 completion. Existing warnings remain
in `vendor/irc-proto` (`LineCodec::new` unused `label`) and `late-cli` voice stubs
on unsupported platforms.

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

## Task #7 — Moderation mapping (pending)

Spec: FRD-IRCD.md §9. Currently `conn.rs` returns **polite refusals** with TODO
markers for KICK / KILL / MODE ±b; `Session.is_moderator` is `#[allow(dead_code)]`
pending this work. Implement:

- **+o projection**: mods → channel op (+o) in every channel; admins → ircop
  (and op). Users can never be opped via ircc (op is lock-tied to mod tier —
  reject MODE +o from clients). Send the op state in NAMES/353 prefixes and a
  MODE burst on join.
- **KICK** (channel) ↔ room kick; **server KILL** by an ircop ↔ server force-quit
  (disconnect + block via registry). Room kick ↔ channel kick projection inbound.
- **MODE ±b** `nick!*@*` ↔ room ban; server ban must disconnect + block
  (registry `disconnect_user`, and auth already rejects banned users on connect —
  see `auth.rs` `AuthOutcome::Banned`). Surface banlist via 367/368.
- **Server ban / kick / token-revoke** must force-disconnect live conns
  immediately. Token-revoke path already wired (Task #6). Hook server ban/kick
  emit events to `IrcRegistry::disconnect_user`.
- `+i` (invisible) is a no-op (accept silently). INVITE is **deferred** — reply
  with a polite notice (already stubbed; confirm wording).
- **Audit logging** for moderation actions taken via ircc.
- Watch the existing chat/moderation service events: the ircd needs to subscribe
  to room ban/kick/mod-change broadcasts and project them to the right IRC verbs.
  Find where bans/kicks/mod-grants are emitted in `late-ssh`/`late-core` and add
  an ircd projection consumer (likely in `conn.rs`'s event select loop, parallel
  to `project_chat_event`).

## Task #8 — TLS listener: done

Spec: FRD-IRCD.md §5.2 / §7. Implemented as a single listener:

- Plaintext dev mode when `LATE_IRC_TLS_CERT` / `LATE_IRC_TLS_KEY` are absent
  (default port 6667).
- TLS mode when both env vars are present; certs/keys are loaded from PEM and
  accepted with `tokio_rustls::TlsAcceptor`. If `LATE_IRC_PORT` is omitted in
  TLS mode, default port is 6697.
- Config validates both-or-neither TLS env vars. Production cert requirements
  remain: publicly trusted CA, full chain, exact hostname (e.g. `irc.late.sh`).

## Task #9 — Tests + docs (pending)

- ircd integration tests under `late-ssh/tests/ircd/` using testcontainers
  (mirror existing `tests/` patterns). Cover: auth (good/bad/revoked/banned
  token), nick-lock, forced #lounge join + sticky PART refusal, PRIVMSG
  round-trip to chat + back, DM ↔ /msg query, LIST/NAMES/WHO shapes,
  multi-connection self-echo suppression, disconnect-on-revoke.
- Update repo-root `CONTEXT.md` with the ircd feature + the "agents don't run
  tests/clippy" reminder if not already there.
- Add splash/MOTD tips (FRD: motd carries the #lounge banner; there's a TODO in
  `motd.rs` to plumb the **live** lounge banner — currently static).

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
late-ssh/src/app/settings_modal/input.rs    dialog dispatch (handler fn TODO)
late-ssh/src/app/settings_modal/ui.rs       rendering (TODO)
late-ssh/tests/helpers/mod.rs               test State has irc_registry + IrcConfig::default
```

## Config / runtime notes

- `IrcConfig` defaults: `enabled = false`, `port = 6667` (or 6697 when
  `LATE_IRC_TLS_CERT` / `LATE_IRC_TLS_KEY` are configured and `LATE_IRC_PORT`
  is unset),
  `max_conns_global = 200`, `max_conns_per_user = 3`,
  `max_auth_failures_per_ip = 20`, `auth_failure_window_secs = 300`. All env-parsed,
  all optional. ircd only spawns when `config.irc.enabled`.
- Brute-force defense is **token strength** (160-bit), not rate limiting; the IP
  auth-failure limiter is a light backstop only (FRD §5).
- Registration: CAP/PASS/NICK/USER with 60s timeout; auth tarpit on failure
  (`AUTH_FAIL_DELAY = 1s`, `AUTH_FAIL_DELAY_LIMITED = 8s`).
