# late.sh IRC Context

## Metadata
- Domain: embedded IRC server for late.sh chat
- Primary audience: LLM agents working in `late-ssh/src/ircd`, IRC token auth, or IRC/chat integration paths
- Last updated: 2026-06-17
- Status: Active
- Parent context: `../../../CONTEXT.md`
- Related context: `../app/chat/CONTEXT.md`

---

## 1. Scope

This file owns context for the embedded IRC daemon (`ircd`) that exposes late.sh chat to normal IRC clients.

Included here:
- IRC listener/config/TLS behavior.
- IRC registration and token auth.
- Nick/channel/DM projection between IRC protocol concepts and late.sh users/rooms.
- IRC command handling for messages, joins, parts, list/names/who/whois/whowas, modes, topic queries/refusals, ping/pong, MOTD, VERSION, TIME, LUSERS, USERHOST, ISON, AWAY, CAP compatibility, and moderation commands.
- IRC moderation mapping for kick, kill, ban, unban, and moderation event projection.
- Runtime IRC connection registry and forced-disconnect semantics.
- Performance constraints for IRC fanout over the shared chat event stream.

Out of scope:
- General chat service semantics, snapshots, message tails, room ordering, synthetic Home entries, and TUI chat UI. Those live in `late-ssh/src/app/chat/CONTEXT.md`.
- Core DB model ownership. IRC may use models from `late-core`, but model invariants belong near the model or the chat context.
- Production infra rollout policy beyond the config/env contracts documented here.

---

## 2. File Map

```text
late-ssh/src/ircd/
|-- mod.rs          # Module declarations only
|-- serve.rs        # TCP listener, optional TLS, accept loop, global socket cap, shutdown disconnect
|-- conn.rs         # Per-connection IRC registration, session state, commands, event projection
|-- auth.rs         # PASS token auth, token lookup, user/IP server-ban checks
|-- registry.rs     # Process-local user -> IRC connection control handles
|-- proj.rs         # Pure room/channel/body/CTCP projection helpers
|-- replies.rs      # IRC numeric/server reply helpers
`-- motd.rs         # MOTD response text
```

Core model touchpoints:
- `late-core/src/models/irc_token.rs` stores one hashed IRC token per user.
- `chat_room.rs`, `chat_room_member.rs`, and `chat_message.rs` back channel/DM access and message delivery.
- `room_ban.rs` and `server_ban.rs` back room/server moderation checks and ban-list projection.

App/service touchpoints:
- `late-ssh/src/main.rs` creates one `IrcRegistry`, injects it into `ChatService` and `ProfileService`, and spawns `ircd::serve::run` when `state.config.irc.enabled`.
- `late-ssh/src/config.rs` owns `LATE_IRC_*` parsing.
- `late-ssh/src/app/chat/svc.rs` is the authoritative send/moderation/event service. IRC must use it instead of duplicating write paths.
- `late-ssh/src/app/profile/svc.rs` mints/revokes IRC tokens and disconnects live IRC sessions when token/account state changes.
- `late-ssh/src/moderation/session_effects.rs` disconnects IRC sessions for server kick/ban effects.

Keep `mod.rs` declaration-only; do not add a re-export layer.

---

## 3. Architecture

IRC is a separate protocol surface, not a separate chat backend.

The IRC listener runs inside the `late-ssh` process. That is intentional for the current implementation because IRC depends on in-process app services:
- `ChatService` for sends, moderation commands, and broadcast `ChatEvent` / `ModerationEvent` subscriptions.
- `ProfileService` plus `IrcRegistry` for token reset/revoke/account-delete disconnects.
- `State.active_users` and `State.username_directory` for presence and nick projection.
- Shared process shutdown/drain state.

Do not convert IRC to direct DB writes just to make it a separate cargo package. A DB-only daemon would bypass the authoritative chat/moderation service path and would need replacement infrastructure for realtime events, presence, and forced disconnects.

A good future split would be: extract shared chat/profile/moderation service interfaces out of late-ssh, add a real event bus, then run late-ircd as its own binary/pod. Without that extraction, same process is the pragmatic choice.

If IRC is split later, the split must include a cross-process event/control design such as Postgres `LISTEN/NOTIFY`, Redis/NATS, or an internal service API. Reusing `late-core` models alone is not sufficient.

---

## 4. Config And Listener

Config:
- `IrcConfig::default()` is disabled by default.
- The root Makefile opts local dev in with `LATE_IRC_ENABLED=1` and plaintext `LATE_IRC_PORT=6667`.
- Docker Compose publishes the IRC port on `service-ssh`.
- TLS is enabled only when both `LATE_IRC_TLS_CERT` and `LATE_IRC_TLS_KEY` are set.
- When IRC is enabled with TLS cert/key and `LATE_IRC_PORT` is unset, the default port is 6697; otherwise the default is 6667.
- Partial TLS cert/key env is validated only when IRC itself is enabled, so disabled IRC must not break SSH/API startup.

Listener behavior:
- `serve.rs` binds `0.0.0.0:{port}`.
- If TLS config is present, each accepted socket is wrapped with rustls before registration.
- The accept loop enforces `max_conns_global` with a semaphore before TLS/auth/registration work. Pre-auth sockets count toward the cap.
- Draining or shutdown rejects/ends connections quickly with IRC `ERROR`; there is no graceful drain for IRC clients.
- Auth failure limiting is IP-scoped and intentionally adds delay before returning auth errors.

Production posture:
- Keep IRC disabled unless explicitly enabled.
- Prefer TLS-only in production; plaintext is for local/dev use.
- Keep connection caps conservative because each registered IRC client owns a long-lived task and event subscriptions.

---

## 5. Auth, Identity, And Tokens

IRC clients authenticate with normal IRC `PASS`, but the value is a late.sh IRC token.

Token model:
- Users start with no token, so they cannot connect over IRC until they mint one.
- A user has at most one IRC token.
- Tokens are generated as `late-irc-` plus 32 characters of a 32-symbol alphabet.
- Only the SHA-256 hash is persisted.
- Successful IRC auth updates `irc_tokens.last_used`; mint/reset clears `last_used` and updates `created`/`updated`.
- Plaintext token is shown once in Settings -> Account; it cannot be recovered later.
- Resetting/re-minting replaces the token and disconnects existing IRC sessions for that user.
- Revoking deletes the token and disconnects existing IRC sessions for that user.

Registration rules:
- Clients must send PASS, NICK, and USER before registration completes.
- The requested IRC nick is ignored except as a registration signal.
- The registered nick is locked to the late.sh username projected for IRC; `.` is displayed as `^`. IRC `NICK` changes are refused.
- Auth rejects bad tokens, deleted users, user server bans, and active IP server bans.

Do not add alternate IRC-only identities. IRC should remain another view of the same late.sh account.

---

## 6. Channels, DMs, And Messages

Room/channel projection lives in `proj.rs`.

Exposed IRC channels:
- `lounge` and `language` rooms.
- `topic` rooms with public visibility.
- `topic` rooms with private visibility, but only for members.

Not exposed as normal IRC channels:
- DMs.
- Game-room chat.
- Any room without a slug.

Behavior:
- `#lounge` is force-joined on IRC session start. If join fails because of room-level restrictions, the IRC session stays up and receives the refusal.
- Joining a public channel calls the normal room membership path.
- Joining a private channel requires existing membership; IRC presents private rooms as invite-only.
- PART detaches the IRC view for normal rooms but does not leave late.sh room membership.
- PART on `#lounge` is refused and the JOIN/NAMES burst is resent.
- `/LIST` returns IRC-visible channels plus member counts.
- `/NAMES`, `/WHO`, `/WHOIS`, `ISON`, and related compatibility commands should use shared username/presence sources and avoid per-row DB work where possible.

Message send:
- `PRIVMSG #channel ...` uses `ChatService::send_message_task`.
- `PRIVMSG nick ...` resolves the late.sh username and uses/creates the normal DM room.
- NOTICE uses the same send path as PRIVMSG but suppresses error replies. CTCP ACTION is converted to a conventional chat body like `*waves*`; late.sh chat has no separate `/me` concept. CTCP VERSION and PING get minimal NOTICE replies, and other CTCP messages are dropped.
- IRC line length limits require splitting long late.sh messages into multiple IRC `PRIVMSG` lines.
- Self-echo suppression applies only to the exact body sent by the same IRC connection; other clients/TUI sessions should still receive normal bouncer-like echoes.

Message receive:
- Registered IRC sessions subscribe to the shared chat event broadcast.
- Events for joined rooms are projected as IRC channel `PRIVMSG`.
- DM events targeted at the user are projected as direct IRC `PRIVMSG`.
- Edited messages are projected with an `[edit]` prefix.
- IRC sessions load the user's ignored-user set at registration, update it from `ChatEvent::IgnoreListUpdated`, and suppress ignored authors in channel projections; DMs are not filtered by this IRC-side ignore check.

---

## 7. Moderation

IRC moderation commands are adapters over existing late.sh moderation, not independent IRC state.

Mappings:
- IRC `KICK #room nick :reason` maps to the normal room kick command path.
- IRC `KILL nick :reason` maps to server kick and requires admin privilege.
- IRC mode `+b nick!*@*` maps to room ban.
- IRC mode `-b nick!*@*` maps to room unban.
- Ban-list queries read active room bans and render IRC ban-list numerics.

Invariants:
- Re-check permissions before executing moderation commands. Do not trust permissions cached when the IRC session registered.
- Use `ChatService::run_mod_command` so audit logs, notifications, session effects, voice revocation, and DB state stay shared with TUI moderation.
- Server kick/ban and account deletion must disconnect live IRC sessions through `IrcRegistry`.
- Room moderation events should be projected to IRC clients in affected joined channels.
- Moderator grant/revoke events are projected as IRC channel `+o`/`-o` across joined channels, and the target session updates its cached staff flags.

---

## 8. Registry And Presence

`IrcRegistry` is process-local runtime state:
- Tracks registered IRC connections by `user_id`.
- Enforces `max_conns_per_user`.
- Sends `IrcControl::Disconnect` to live sessions for token revoke/reset, account deletion, server kick/ban, and shutdown.
- Reports IRC online users for presence projection.

Presence projection:
- IRC presence combines SSH/TUI active users and registered IRC users.
- Each IRC session keeps a previous online set and periodically diffs it.
- Departures render as IRC `QUIT`.
- Arrivals render as `JOIN` only for channels shared with the current IRC session.
- Avoid per-arrival/per-room DB loops. Batch membership lookups for arrivals against joined rooms.

`IrcRegistry` is not durable and does not need to be. On process restart, IRC clients reconnect and re-register with their token.

---

## 9. Performance Notes

The hot path is fanout: every connected IRC client receives the shared chat/moderation broadcasts and decides whether to project the event.

Expected cost:
- Message event handling is `O(number of IRC connections)`.
- Each connection should do cheap in-memory filtering first.
- DB work inside fanout must be avoided or batched because one chat message can wake every IRC session.

Current performance guardrails:
- Global socket cap before TLS/auth/registration.
- Per-user registered connection cap.
- IP auth-failure limiter.
- Post-registration expensive commands are rate-limited per IRC connection over a 10-second window; NOTICE is rate-limited but does not generate rate-limit error replies.
- `/LIST` uses one query for IRC-visible rooms plus member counts.
- Presence arrivals batch `chat_room_members` lookups for all new online users against the session's joined rooms.
- Targeted non-DM private-room events are negative-cached per session to avoid repeated room-kind lookups.
- Pure projection helpers live in `proj.rs` and should stay allocation/DB-light.

When adding features, ask whether the work happens once per user action or once per IRC connection. Per-connection work needs much stricter DB and allocation discipline.

---

## 10. Testing And Verification

Follow the root test policy.

Unit tests:
- Pure helpers in `proj.rs`, `registry.rs`, and reply formatting can use inline `#[cfg(test)]` tests.
- Unit tests must not touch DB, services, sockets, or async process orchestration.

Integration tests:
- Registration/auth, DB-backed channel membership, message delivery through `ChatService`, moderation mapping, and listener behavior belong under `late-ssh/tests/`.
- Use shared DB helpers for any DB-backed test.

Agent command policy:
- LLM agents must not run `cargo test`, `cargo nextest`, or `cargo clippy`.
- For doc-only changes, no compile/test verification is required.
- If code changes touch IRC, `cargo check -p late-core -p late-ssh` is the lightweight compile check; leave full test gates to the human owner unless explicitly asked.

---

## 11. Gotchas

- IRC clients vary heavily in CAP negotiation behavior. Unknown/unsupported CAP requests should fail cleanly without blocking registration forever.
- Many clients support self-signed TLS bypass, but production should still prefer valid cert chains where possible.
- Do not expose game-room chat as IRC channels unless product semantics are revisited; game rooms have separate active-room/runtime behavior.
- Do not make IRC nick changes mutate late.sh usernames.
- Do not let token reset/revoke close the Settings dialog while the one-time plaintext token is still pending display.
- Do not add hidden IRC-only moderation state. Everything should map to existing room/server moderation rows and commands.
- Keep shutdown behavior fast-disconnect; IRC clients are expected to reconnect.
