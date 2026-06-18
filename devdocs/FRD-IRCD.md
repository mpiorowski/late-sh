# FRD: Embedded IRC Server (ircd) for late.sh Chat

## Metadata
- Status: Draft / pre-implementation
- Last updated: 2026-06-11
- Audience: LLM agents and human contributors implementing the feature
- Terminology used throughout: **ircd** = the IRC server (this feature); **ircc** = an IRC client (irssi, HexChat, mIRC, Igloo, Quassel, WeeChat, Textual, etc.)

---

## 1. Summary

Embed a minimal-yet-client-satisfying ircd inside `late-ssh` that exposes late.sh chat as an IRC network of one server. Users mint a personal IRC token in the SSH TUI (Settings → Account), paste it into their IRC client's **server password** field, and connect. Their nick is locked to their late.sh username; channels mirror late.sh chat rooms; DMs mirror IRC queries; late.sh moderation (mod/admin tiers, room/server kicks and bans) projects onto IRC ops/ircops and channel/server moderation primitives.

"Minimal yet client-satisfying" means: we implement only what we need, but enough of the protocol that major IRC clients connect, register, join, list, query, and chat without hanging, erroring, or misrendering. We do **not** aim for RFC-complete or IRCv3-complete behavior.

The protocol layer (parsing/serialization, command and numeric types, modes, CTCP framing) is provided by `irc-proto` (upstream: `~/p/gh/irc-rs/irc-proto/`), **vendored into this repo** (e.g. `vendor/irc-proto`, following the `vendor/potatis` precedent) as a path dependency so we can patch it directly whenever that is the cleanest implementation path. The "d" — listener, connection state machine, registration, channel/session bridging — is written by us inside `late-ssh`.

## 2. Goals

- G1: Any major IRC client can connect with a server password and use late.sh chat (read + write) in public rooms and DMs.
- G2: IRC presence is a *projection* of late.sh, never a second source of truth. Identity, membership, moderation state, and message history are owned by existing late.sh services and the DB.
- G3: Moderation actions round-trip where the mapping is natural (kicks, bans, ops), and late.sh-side enforcement (server ban, token revocation) takes effect on live IRC connections immediately.
- G4: The ircd is operationally boring: a config-gated listener inside the existing `late-ssh` process, no new deployable, no IRC server-to-server linking.

## 3. Non-Goals

- No server linking / federation (no S2S protocol, no netsplit semantics).
- No nick registration, NickServ/ChanServ, or services framework. Identity comes from the token; there is nothing to register.
- No IRC-originated account creation. A late.sh account (via SSH) is a prerequisite.
- No full IRCv3 capability suite. We negotiate CAP only enough to not break clients (see §7.2); individual caps may be added later (see §13 Phasing).
- No channel creation/deletion from IRC in v1. Rooms are created in the TUI.
- No exposure of non-chat surfaces (Arcade, Rooms games, Lateania, Artboard, voice, audio) beyond whatever already flows into chat rooms as messages.
- No per-channel keys (+k), limits (+l), or user-settable channel modes.

## 4. Naming Decisions & Requirement Normalization

- The originating request said **`#general`**. late.sh's built-in always-joined room is the **lounge** (`chat_rooms` kind `'lounge'`, slug `lounge`). This FRD normalizes on **`#lounge`** as the forced-join channel so IRC channel names match TUI room names 1:1. **Decided:** v1 has no `#general` alias.
- Per repo convention, "mod"/"admin" are **tiers/privileges**, not "roles" (reserve "role" for user-facing flair). This document follows that.

## 5. Identity & Token Lifecycle

### 5.1 Token model

- T1: A user starts with **no IRC token**. No token ⇒ no IRC access.
- T2: A user can **mint** and **revoke** their IRC token from the SSH TUI: Settings modal → **Account** tab (new `AccountRow` alongside LinkAccounts / DeleteAccount).
- T3: At most **one active token per user**. Minting while a token exists revokes the old one and issues a new one (regenerate semantics). Revoking leaves the user with no token.
- T4: Tokens are **strong**: ≥128 bits of CSPRNG entropy (e.g. base32 of 20 random bytes ⇒ 160 bits). Displayed **once at mint time** in a copy-friendly form: prefixed, fixed-length, unambiguous alphabet (e.g. `late-irc-` + 32 chars from the same `LINK_CODE_ALPHABET`-style set used by account linking). The prefix makes leaked tokens identifiable/grep-able. Token strength — not rate limiting — is the primary brute-force defense.
- T5: Storage: only a **hash** of the token is persisted (new table, e.g. `irc_tokens(user_id PK/unique, token_hash, created, last_used)`); the plaintext exists only in the mint-time UI. Lookup at connect time is by hash.
- T6: The Account tab shows token **status** (none / active since `<date>`, last used `<date>`), never the token value after mint.
- T7: Revocation and re-mint must **immediately disconnect** any live IRC connections authenticated with the old token (`ERROR :Token revoked` then close).
- T8: Server-banned users: token authentication fails while a server ban (user / fingerprint / IP as applicable) is active, and minting is unavailable/disabled for banned users.

### 5.2 Authentication at connect

- A1: Clients authenticate by sending the token as the **server password** (`PASS <token>`) before completing registration. This is the field every major client exposes as "server password".
- A2: A connection that completes `NICK`+`USER` without a valid `PASS` is rejected with `464 ERR_PASSWDMISMATCH` followed by `ERROR` and a close. No anonymous or wrong-token access, ever.
- A3: SASL PLAIN may be added later as an alternative carrier for the same token (some clients/networks prefer it); not required for v1 (§13).
- A4: Because tokens carry ≥128 bits of entropy (T4), online brute force is computationally infeasible; auth rate limiting exists to curb abuse/log-spam, **not** as a security boundary, and must not burden legitimate users. Limits are per-IP on *failed* attempts only (reuse `late_core::rate_limit` sliding-window limiter), generous enough that client auto-reconnect loops and post-restart reconnect storms (§10 L3) never lock out a user with a valid token; sustained failure floods get a tarpit/disconnect.
- A5: TLS: tokens must not transit plaintext in production. Production exposes **TLS only** (standard port 6697, terminated **in-process via rustls** — v1 default; IRC is raw TCP so ingress termination would need TCP passthrough anyway, and in-process keeps chain/SNI under our control; cert/key mounted from the cluster's cert-manager secret). Local dev may use plaintext 6667.
- A6: Certificate requirements — major clients (WeeChat, irssi 1.2+, HexChat, Textual, Halloy, Igloo) verify the chain against the OS trust store **and enforce hostname matching** by default; mIRC and Quassel interrupt with an accept/pin dialog on untrusted certs. Therefore production must serve: a publicly trusted CA cert (Let's Encrypt is fine), the **full chain including intermediates** (IRC clients do not fetch missing intermediates via AIA), a subject covering the exact advertised connect hostname (e.g. `irc.late.sh`), and SNI. Met, every target client connects with zero prompts; self-signed certs are not acceptable even short-term.

### 5.3 Nick locking

- N1: The nick is **locked to the late.sh username** of the token's owner. The client's preferred `NICK` at registration is ignored; the server's `001 RPL_WELCOME` (and all subsequent prefixes) use the canonical username, which all major clients adopt as their nick.
- N2: Post-registration `NICK` commands are rejected (recommended: `435`/`447`-style "nick change not permitted" numeric + notice; exact numeric chosen at implementation for best client behavior). The nick never changes mid-session.
- N3: late.sh username changes while an IRC session is live: v1 policy is to disconnect affected IRC sessions with `ERROR :Username changed, reconnect` rather than emit a server-originated NICK (simpler, rare event).
- N4: Hostmask: synthesize stable, non-PII prefixes, e.g. `<username>!<username>@late.sh` (or `@irc.late.sh`). Never expose client IPs in prefixes, WHOIS, or WHO.

### 5.4 Multiple connections per user

- M1: The same user may have an SSH TUI session and IRC session(s) concurrently; they are one principal, one nick.
- M2: Multiple **concurrent IRC connections** with the same token are allowed bouncer-style (all share the one nick; each connection receives all traffic; outbound messages from any connection attribute to the user once). Cap concurrent IRC connections per user at a small number (suggest 3); excess connections are refused with `ERROR`.
- M3: A user's own messages sent from the TUI (or another IRC connection) are delivered to their IRC connections as normal `PRIVMSG` from their own nick — this is standard bouncer behavior and all major clients render it acceptably. (`echo-message` cap can refine this later.)

## 6. Channel & Room Mapping

### 6.1 Mapping table

| late.sh concept | IRC concept |
|---|---|
| Built-in lounge room (`kind='lounge'`) | `#lounge` — forced join, cannot leave |
| Language rooms (`kind='language'`) | `#<slug>` channels, public |
| Public topic rooms (`kind='topic'`, `visibility='public'`) | `#<slug>` channels, public, listed |
| Private topic rooms (`kind='topic'`, `visibility='private'`) | `#<slug>` channels, member-only, unlisted to non-members |
| Game-room chat (`kind='game'`) | **Not exposed** in v1 (§13) |
| DM rooms (`kind='dm'`) | IRC query: `PRIVMSG <nick>` (no channel) |
| Room topic/banner | Channel `TOPIC` (332); lounge banner also in MOTD |
| Mod tier (`users.is_moderator`) | Channel operator (`+o`) in **all** channels |
| Admin tier (`users.is_admin`) | IRC operator (ircop; WHOIS 313), plus everything mods get |
| Room ban | Channel ban `+b <nick>!*@*` |
| Server ban | Auth refusal + immediate disconnect of live connections |
| Room kick | Channel `KICK` |
| Server kick | Server-side force-quit (`ERROR` + close) / ircop `KILL` |

### 6.2 Channel naming

- C1: Channel name = `#` + room slug. Room slugs must therefore be valid IRC channel names (no spaces, commas, `^G`; sensible length). If any existing slug violates this, define a deterministic sanitization and collision rule at implementation time.
- C2: `CASEMAPPING=ascii` (declared in ISUPPORT); channel and nick matching is ASCII case-insensitive.

### 6.3 Membership & join/part semantics

- J1: On successful registration every IRC session is **auto-joined to `#lounge`** (server-sent `JOIN`, then `332` topic and `353`/`366` names). Clients accept server-initiated JOINs.
- J2: `PART #lounge` is refused with **sticky join**: numeric error + notice ("you cannot leave the lounge"), no PART echo, **always followed by a server-initiated JOIN burst** (`JOIN` + `332` + `353`/`366`). Rationale: clients diverge on leave gestures — typed `/part` waits for the server echo, but window/tab close sends PART and destroys the UI immediately — so the unconditional JOIN burst converges every client back to a visible, joined lounge regardless of gesture (redundant JOINs are handled gracefully by all target clients).
- J3: `JOIN` on a public/built-in channel: allowed for any authenticated user; effects mirror TUI room join semantics (the user becomes/remains a member of the late.sh room).
- J4: `JOIN` on a private channel: allowed **only if the user is already a member** of the private room (membership managed in the TUI). Non-members get `473 ERR_INVITEONLYCHAN` (private channels present as `+i`). Non-members also cannot see the channel in LIST/NAMES/WHO (it behaves `+s` secret to outsiders).
- J5: `JOIN` on a nonexistent channel: `403 ERR_NOSUCHCHANNEL`. IRC cannot create rooms in v1.
- J6: `PART` of a non-lounge channel **detaches the IRC session's view only** — late.sh room membership is never changed by PART (v1 default). This makes an accidental tab-close non-destructive; membership changes happen in the TUI.
- J7: `INVITE`: deferred past v1 (reserved for future private-room membership interaction). v1 answers `INVITE` with a polite "not supported yet" notice.

### 6.4 Presence & NAMES

- P1: A channel's IRC-visible member list = room members **currently online** (active SSH TUI session or IRC connection). This keeps JOIN/QUIT churn meaningful to clients.
- P2: TUI users coming online/offline generate `JOIN`/`QUIT` (not PART-per-channel) toward IRC viewers; an online TUI user appears in all channels mapped from rooms they are a member of.
- P3: `353 RPL_NAMREPLY` shows `@` for mods/admins (they hold +o everywhere), no voice (`+v`) tier in v1.
- P4: An active IRC connection counts as late.sh presence (user shows online in TUI presence surfaces).

## 7. Protocol Surface

### 7.1 Baseline client-compatibility contract

This is the bar for "client-satisfying": the following must work with irssi, WeeChat, HexChat, mIRC, Quassel (client+core), Igloo, Textual, and Halloy without hangs or protocol errors:

1. **Registration:** `PASS`/`NICK`/`USER` in any standard order; `CAP LS [302]` answered (even with an empty/minimal cap list) and `CAP END` honored — clients that open with CAP must not hang.
2. **Welcome burst:** `001 002 003 004` then `005 RPL_ISUPPORT` (see 7.3), optional minimal LUSERS (`251`), then MOTD (`375`, `372`×n, `376`).
3. **Keepalive:** answer client `PING` with `PONG`; send server `PING` and reap connections that miss PONG (suggest 90–180 s window).
4. **Join burst:** `JOIN` echo, `332`/`333` topic, `353`/`366` names.
5. **Queries:** `LIST` (321/322/323), `NAMES`, `TOPIC` (read; write per §9.4), `WHO` (352/315), `WHOIS` (311, 313 for ircops, 317 idle optional, 318), `MODE` query for channels (324, 329) and self (221), `USERHOST`, `ISON`, `MOTD`, `VERSION`, `TIME`, `LUSERS` — all answered, even minimally.
6. **Messaging:** `PRIVMSG`/`NOTICE` to channels and nicks; CTCP `ACTION` (`/me`); CTCP `VERSION`/`PING`/`TIME` answered with NOTICE; CTCP `DCC` refused/ignored.
7. **Errors:** correct use of standard numerics (401 no-such-nick, 403 no-such-channel, 404 cannot-send, 442 not-on-channel, 461 need-more-params, 464 passwd-mismatch, 473 invite-only, 482 chanop-needed) — clients render these natively.
8. **Line discipline:** 512-byte lines (incl. CRLF), UTF-8 in/out (advertise `UTF8ONLY`), server-side splitting of long outbound messages into multiple PRIVMSGs, tag-free messages in v1.

### 7.2 Command disposition table

| Disposition | Commands |
|---|---|
| **Implemented (full semantics)** | PASS, NICK (registration only), USER, CAP (LS/REQ→ACK-or-NAK/END), PING, PONG, JOIN, PART, PRIVMSG, NOTICE, TOPIC, NAMES, LIST, WHO, WHOIS, MODE (subset per §9), KICK, QUIT, MOTD, VERSION, TIME, LUSERS, USERHOST, ISON, ADMIN, INFO |
| **Implemented, privileged** | KILL (ircop ⇒ late.sh server kick), MODE ±b/±o per §9 |
| **Accepted as no-op (silent or polite numeric, never an error spam)** | MODE +i/-i and other user self-modes (per requirement: user `+i` does nothing), AWAY (ack with 305/306, not surfaced in v1), WHOWAS (406) |
| **Rejected with proper numeric** | NICK after registration, OPER (491 — ircop comes from admin tier, not OPER), PART #lounge, MODE grants we don't allow (482), INVITE (v1, polite notice) |
| **Unknown/unsupported** | 421 ERR_UNKNOWNCOMMAND (catch-all; never close the connection for an unknown command) |

### 7.3 ISUPPORT (005) tokens (initial set)

`NETWORK=late.sh`, `CASEMAPPING=ascii`, `CHANTYPES=#`, `PREFIX=(o)@`, `CHANMODES=b,,,imnst` (advertise conservatively; only what MODE query can return), `NICKLEN`/`USERLEN` per late.sh username limits, `CHANNELLEN`, `TOPICLEN`, `UTF8ONLY`, `ELIST=` (none) or omit, `MODES=1`, `STATUSMSG=@` omitted in v1.

### 7.4 MOTD

- MOTD content: late.sh welcome blurb + **lounge banner info** (the same banner content the TUI lounge top boxes show) + pointer to docs ("nick is your late.sh username; manage your token via ssh late.sh → Settings → Account").
- MOTD regenerated per connection (banner may change); no caching requirement.

## 8. Messaging Semantics

- S1: Inbound `PRIVMSG #chan :text` ⇒ post via `ChatService` to the mapped room, attributed to the token's user, subject to the same permission/ban/rate checks as a TUI send. `404 ERR_CANNOTSENDTOCHAN` when refused (banned, not a member of private room, etc.).
- S2: Inbound `PRIVMSG <nick> :text` ⇒ late.sh DM (find-or-create the DM room, same as TUI DM initiation). `401 ERR_NOSUCHNICK` for unknown users. DMs to offline users still persist (late.sh DMs are persistent); the IRC sender just gets no presence feedback in v1.
- S3: Inbound CTCP `ACTION` maps to late.sh's emote/`/me` representation if one exists, else is posted as `* <nick> text`. Outbound late.sh emotes map to CTCP ACTION.
- S4: Outbound (room → IRC): new chat messages in mapped rooms are delivered as `PRIVMSG #chan` from the author's nick. System/bot/AI authors (graybeard etc.) use their usernames as nicks; pure system lines may use a server NOTICE.
- S5: Long/multiline late.sh messages are split into multiple ≤512-byte PRIVMSGs, hard-wrapping on UTF-8 boundaries.
- S6: **Edits**: re-sent to IRC as a new PRIVMSG prefixed `[edit]` (v1 policy; IRCv3 caps later). **Deletes**: silently not projected (v1 default); never crash the projection.
- S7: **Reactions, pins, polls, rich embeds**: not delivered to IRC in v1, except polls may render as plain text lines if cheap. Inbound IRC text cannot react/pin/reply-thread; replies arrive as ordinary messages.
- S8: **No history replay on join** in v1 (clients don't expect it without `chathistory`). IRC connections see messages from connect-time forward.
- S9: Inbound message rate limiting mirrors TUI chat limits; flood beyond limits gets messages dropped with a NOTICE warning, then disconnect for sustained abuse.
- S10: mIRC color/formatting codes in inbound text: strip or pass through to TUI rendering — TBD at implementation (irc-proto's `colors`/`FormattedStringExt` can strip). Outbound TUI styling is not translated to IRC formatting in v1.

## 9. Moderation Mapping

### 9.1 Privilege projection

- O1: `is_moderator` ⇒ `+o` (channel op) in **every** channel. Op status is **lock-tied to tier**: it cannot be granted or removed via IRC `MODE ±o`; such attempts get `482 ERR_CHANOPRIVSNEEDED` (non-ops) or a polite refusal notice (ops trying to op others).
- O2: `is_admin` ⇒ ircop (WHOIS 313, access to KILL), in addition to channel op everywhere. `OPER` is refused (491); ircop status is intrinsic.
- O3: Tier changes while connected take effect promptly (re-project: send MODE +o/-o for that user to affected channels, or disconnect-and-ask-to-reconnect if simpler — implementation choice, but the lazy option must still revoke *enforcement* immediately even if display lags).

### 9.2 Kicks

- K1: IRC `KICK #chan nick` by a channel op ⇒ late.sh **room kick** (same service path as TUI `/mod` room kick, same audit logging via `moderation_audit_log`).
- K2: late.sh room kick (from TUI) ⇒ IRC viewers of that channel see `KICK`; the kicked user's IRC clients are removed from the channel (and may rejoin per room-kick semantics, matching TUI behavior).
- K3: late.sh **server kick** ⇒ all of the user's IRC connections receive `ERROR :Kicked from server` and are closed. Their TUI sessions are handled by existing moderation paths.
- K4: ircop `KILL nick :reason` ⇒ late.sh server kick of that user (audit-logged with the admin as actor), which closes IRC connections (K3) and terminates SSH sessions via the existing server-kick path.

### 9.3 Bans

- B1: late.sh **room ban** ⇔ channel ban. Active room bans are visible as `+b <username>!*@*` in the channel banlist (`367`/`368`). Setting `MODE #chan +b nick!*@*` as an op creates a late.sh room ban (audit-logged); `-b` removes it. Ban masks other than the `nick!*@*` shape are refused with a notice (we ban identities, not masks).
- B2: A room-banned user is kicked from the channel (if present) and refused on JOIN (`474 ERR_BANNEDFROMCHAN`) and on send (`404`).
- B3: late.sh **server ban** ⇒ token auth refusal (§5.1 T8) **and immediate disconnect** of all live IRC connections belonging to the banned user (and, where the ban is IP/fingerprint-scoped, refusal of matching IPs at accept time using existing `ServerBan` IP lookups).
- B4: There is no IRC-side command to create a *server* ban in v1 (KILL kicks; it does not ban). Server bans are created in the TUI/Control Center.

### 9.4 Topics

- TP1: `TOPIC #chan :new topic` allowed for ops only (mods/admins), and only where late.sh has a corresponding editable room topic/banner concept; otherwise `482`. Reads (332) always work, sourced from room metadata.

## 10. Lifecycle, Shutdown & Upgrades

- L1: The ircd is a tokio listener task inside the `late-ssh` process, started from `main.rs` alongside SSH (2222) and API (4000), gated by config (enabled flag, bind addr, port, TLS settings).
- L2: **No linking ⇒ no netsplits.** The only multi-server-ish event is a pod replacement. Policy: **fast shutdown, no drain** — on SIGTERM the ircd sends each connection `ERROR :Server restarting` and closes promptly so clients' auto-reconnect logic (universal in major clients) re-attaches to the new pod. Do not hold the old pod open for IRC.
- L3: Reconnect storms after restart are absorbed by the auth rate limiter (per-IP) plus a global accept limiter consistent with existing SSH connection-limit philosophy.
- L4: Connection caps: global max IRC connections and per-IP max (mirroring SSH limits) to keep the ircd from becoming a cheap DoS lever on the shared process.

## 11. Architecture Fit (informative)

- New domain: `late-ssh/src/ircd/` (peer of `ssh.rs`/`api.rs`, not under `app/` — it is a network frontend, not a TUI screen): `mod.rs`, `listener.rs`, `conn.rs` (per-connection state machine: registration → registered), `proj.rs` (room/channel projection + fan-out), `auth.rs`, `motd.rs`, plus `token` model in `late-core/src/models/` and a migration for `irc_tokens`.
- Framing/parsing: `irc-proto`'s `IrcCodec` (tokio feature) over the TCP/TLS stream; `Command`/`Response`/`Mode`/`Prefix` types for all message construction. We write zero protocol parsing by hand.
- Vendoring: `irc-proto` is copied into `vendor/irc-proto` and consumed as a workspace path dependency (same pattern as `vendor/potatis`), so server-side needs (missing numerics, mode quirks, codec tweaks) are patched in-tree rather than worked around. Keep upstream provenance (source URL + commit) in the vendored crate's README; like Potatis, vendored code is excluded from `make check`'s first-party fmt scope.
- Bridging: each registered connection subscribes to the same service channels the TUI uses (`ChatService` snapshots/events, presence/activity as needed) and translates events → IRC lines; inbound commands call the same service methods the TUI input layer calls. The ircd must **never** write chat/moderation state through a side door.
- The settings-modal token UI follows the existing Account-tab dialog pattern (cf. LinkAccounts flow) and calls a small token service.
- Tests per repo policy: pure protocol/translation logic (channel-name mapping, line splitting, command disposition) as inline unit tests; anything touching DB/services (auth, message round-trip, ban enforcement) as `late-ssh/tests/ircd/` integration tests with testcontainers. Agents do not run `cargo test`/`clippy` here; note expected commands in handoff.
- Telemetry: connection open/close/auth-fail counters and per-command counters consistent with existing operation/event naming.

## 12. Resolved Questions (v1 defaults) & Remaining Items

Decided 2026-06-11:

1. **`#general` alias** — none; `#lounge` only.
2. **TLS termination** — in-process rustls (§5.2 A5); cert requirements per §5.2 A6.
3. **`INVITE`** — deferred past v1; v1 answers with a polite "not supported yet" notice (§6.3 J7).
4. **PART semantics** — IRC-view detach only; never changes room membership (§6.3 J6).
5. **Concurrent IRC connection cap** — 3 per user (§5.4 M2).
6. **Delete projection** — silent; deletes are not projected (§8 S6).
7. **Sticky join** — `PART #lounge` refusal always re-sends the JOIN burst (§6.3 J2).
8. **Game-room chat channels** — out of v1 (§6.1); revisit later behind a distinct namespace if wanted.
9. **IRC presence scope** — chat presence only; IRC connections do not feed quest/leaderboard/activity systems in v1.

Remaining (non-blocking, handle during implementation/testing):

- **Quassel core / Igloo compatibility pass** — both are confirmed targets; budget a verification pass against each (Quassel is the pickiest about the welcome burst and WHO).

## 13. Phasing

**v1 (this FRD's requirement set):** token mint/revoke UI, PASS auth over TLS, nick lock, forced `#lounge`, public/built-in/private channel projection, DMs, full §7.1 compatibility contract, moderation round-trip (ops, kicks, ±b, KILL, server-ban enforcement), MOTD with lounge banner, fast-shutdown lifecycle.

**Later candidates (explicitly out of v1):** SASL PLAIN; IRCv3 caps (`server-time`, `echo-message`, `message-tags`, `chathistory` for backlog, `away-notify`); reactions/replies projection via tags; game-room channels; `INVITE`-driven private-room membership; AWAY surfaced to TUI presence; WEBIRC/ident niceties.
