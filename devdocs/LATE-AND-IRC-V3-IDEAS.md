# late.sh Chat And IRCv3 Ideas

This document cross-references common IRCv3 features with the current late.sh
TUI chat feature set. It is an ideas document, not an implementation contract.

The current IRC bridge is intentionally conservative: it exposes rooms and DMs
as IRC channels/queries, advertises `UTF8ONLY` through ISUPPORT, returns an
empty `CAP LS`, NAKs requested capabilities, and projects messages as plain
`PRIVMSG`. This makes the bridge broadly compatible but leaves most structured
late.sh chat features invisible to IRC clients.

## Current late.sh Chat Inventory

The TUI chat surface already has richer semantics than plain IRC:

- Public, private, language, lounge, DM, and game-backed chat rooms.
- Room membership, room discovery, invite, leave, public/private room creation,
  and admin fill/delete room commands.
- Persistent messages with UUID ids, timestamps, edit support, hard delete, and
  optional `reply_to_message_id`.
- One reaction per `(message_id, user_id)`, stored as arbitrary icon text and
  displayed as reaction summary chips.
- Admin-only global message pins.
- Active room polls with two or three options, 10/20/30 minute durations,
  per-user votes, and automatic result posts.
- Mention notifications, unread cursors, friends, ignores, and active-user
  presence.
- `/brb` away state, shown in TUI author labels while any active session is AFK.
- Inline image URL previews and explicit upload/paste-image flows.
- Moderation mapped through shared room/server kick and ban service paths.

## Best Mapping Candidates

| IRCv3 feature                                 | late.sh feature                                   | Mapping idea                                                                                                                                                  | Fit        |
|-----------------------------------------------|---------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------|------------|
| `server-time`                                 | `chat_messages.created` / event timestamps        | Add `@time=` to outbound `PRIVMSG`, `NOTICE`, `TAGMSG`, and history replay once message tags exist.                                                           | High       |
| `msgid`                                       | `chat_messages.id` UUID                           | Use a stable opaque message id derived from the chat message UUID. This unlocks replies, reactions, redaction, and future history behavior.                   | High       |
| `message-tags`                                | structured message metadata                       | Foundation for `time`, `msgid`, account, reply, react, typing, and late.sh vendor tags.                                                                       | High       |
| `echo-message`                                | current self-echo suppression / BNC-like behavior | Let clients opt into seeing their own accepted messages with final tags and ids. Keep suppression for clients that do not request it.                         | High       |
| `+reply` client tag                           | `reply_to_message_id`                             | Map outbound replies to `@+reply=<parent msgid>` and inbound tagged replies to `reply_to_message_id` after validating the target message is in the same room. | High       |
| `+draft/react` / `+draft/unreact` client tags | chat reactions                                    | Map inbound reaction TAGMSG/PRIVMSG to the existing reaction model and outbound reaction changes to tagged TAGMSG events. See dedicated section below.        | High       |
| `chathistory` + `batch`                       | capped room tail, delta sync, DMs, unread cursors | Replay newest room tail or bounded before/after windows in IRC clients that request history. Use `batch` to group replayed messages.                          | Medium     |
| `away-notify`                                 | `/brb` AFK state                                  | Project `/brb` as away state and send AWAY updates to shared channels. Inbound IRC `AWAY` could set or clear the same per-session AFK flag.                   | Medium     |
| `account-tag`                                 | late.sh user identity                             | Tag messages with the sender's stable account/username. Useful for bots and bridges even though nicks are already locked to usernames.                        | Medium     |
| `account-notify` / `extended-join`            | login/session presence and account identity       | Because every IRC-visible user is authenticated, these mostly reinforce identity on JOIN and state changes.                                                   | Low/Medium |
| `typing` client tag                           | active composer state                             | Could show IRC users typing in the TUI and maybe TUI users typing in IRC, but this needs a new transient typing event path and privacy controls.              | Medium     |
| `labeled-response`                            | async command acks/errors                         | Useful for correlating IRC commands with numeric replies once more commands become asynchronous or tag-aware.                                                 | Low/Medium |
| `standard-replies`                            | service error messages                            | Map refusal/failure messages into structured `FAIL`/`WARN`/`NOTE` instead of ad hoc NOTICE text.                                                              | Low/Medium |
| `multi-prefix`                                | moderator/admin channel status                    | If late.sh ever exposes multiple IRC statuses beyond op, this keeps all prefixes visible. Current `+o` projection does not need it much.                      | Low        |
| `userhost-in-names` / `WHOX`                  | richer member lists                               | Could expose stable username/account data in list queries. Not very valuable while username == nick and host is synthetic.                                    | Low        |
| `MONITOR` / `extended-monitor`                | active-user presence                              | Maps cleanly to online/offline tracking, but current JOIN/QUIT projection already serves channel presence. Useful for DM-oriented clients.                    | Low/Medium |
| `invite-notify`                               | private room invites                              | Once IRC `INVITE` maps to TUI private-room membership, notify privileged members about invites.                                                               | Medium     |
| `channel-rename`                              | moderation `rename-room`                          | Map TUI room rename events to IRC channel rename if client/server support is acceptable. Until then, PART/JOIN or NOTICE is simpler.                          | Low/Medium |
| `message-redaction`                           | message delete                                    | Could project deletes as redaction instead of silently dropping them. Requires stable `msgid` and permission-aware redaction semantics.                       | Medium     |
| `read-marker`                                 | `chat_room_members.last_read_at`                  | Could sync read state across IRC and TUI clients. Useful but not necessary for basic chat.                                                                    | Low        |
| `metadata-2`                                  | profile, badges, country, bonsai, equipped badge  | Tempting, but broad and likely overkill; vendor message tags or WHOIS text are cheaper.                                                                       | Low        |
| `draft/multiline`                             | multiline composer messages                       | Current IRC bridge splits long/multiline messages into multiple `PRIVMSG`s. Multiline could preserve message atomicity, but client support is uneven.         | Low/Medium |
| `WEBIRC`                                      | browser/web IRC gateways                          | Useful only if late.sh permits third-party web gateways in front of the IRC listener.                                                                         | Low        |
| `STS`                                         | production TLS policy                             | Useful for production IRC hardening if IRC is publicly exposed over TLS. Not a chat feature mapping.                                                          | Low        |
| `SASL`                                        | IRC token auth                                    | Could replace PASS-token auth with `AUTHENTICATE PLAIN`, but current token-as-server-password flow is simpler and already client-compatible.                  | Low/Medium |

## Reaction Mapping

Reactions are the most natural IRCv3-to-late.sh feature match, but they need a
small stack of prerequisite protocol work.

IRCv3 shape:

- A normal message needs a server-provided `msgid`.
- A reaction references that message with `+reply=<msgid>`.
- A reaction uses `+draft/react=<reaction>`; removal uses
  `+draft/unreact=<reaction>`.
- The event can be a tag-only `TAGMSG`, or a `PRIVMSG` that also has a visible
  body for clients that do not render reactions.

late.sh shape:

- `chat_messages.id` is already a stable UUID.
- `chat_messages.reply_to_message_id` already stores reply targets.
- `chat_message_reactions` already stores one icon text value per message/user.
- Current `ChatEvent::MessageReactionsUpdated` carries summary state, not the
  actor/action delta needed to emit precise IRC reaction events.

Recommended mapping:

1. Add `msgid` support first. Use the chat message UUID as the opaque IRC
   message id, or encode it with a stable prefix if raw UUIDs are too visually
   noisy.
2. Add `message-tags` and `TAGMSG` handling in the IRC layer, but only accept a
   tight allowlist of client-only tags.
3. Add `+reply` support for normal replies. This validates the id mapping before
   reactions depend on it.
4. Change the reaction service/event contract to expose a reaction delta:
   actor user id, room id, message id, icon, and action (`react`, `unreact`, or
   `replace`). Keep the current summary event for TUI rendering if useful.
5. Inbound IRC:
   - `@+reply=<msgid>;+draft/react=<icon> TAGMSG #room` sets/replaces the
     caller's reaction on that message.
   - `@+reply=<msgid>;+draft/unreact=<icon> TAGMSG #room` removes the caller's
     reaction only if it currently matches that icon.
   - Validate room membership, room visibility, ignored/banned state, and that
     the parent message belongs to the target room.
6. Outbound IRC:
   - Project a TUI reaction delta as
     `@+reply=<msgid>;+draft/react=<icon> :nick!nick@late.sh TAGMSG #room`.
   - Project removal as `+draft/unreact`.
   - Consider also sending a fallback `NOTICE` only to clients that did not
     request `message-tags`, but default to no fallback to avoid noisy channels.

Open design choice: late.sh currently allows exactly one reaction per user per
message. IRCv3 reaction tags can represent multiple independent reaction values.
The first implementation should preserve late.sh semantics: reacting with a new
icon replaces the old icon; unreact removes only the current matching icon.

## Replies And Message IDs

Replies are lower risk than reactions and are a good prerequisite slice.

- Outbound TUI reply: send the child message as
  `@msgid=<child>;+reply=<parent> PRIVMSG #room :body`.
- Inbound IRC reply: parse `+reply`, resolve it to a `chat_messages.id`, verify
  the target message is in the same room, and call the normal
  `send_message_with_reply_task` path.
- TUI backward compatibility currently includes a visible quote line in the
  stored body. Decide whether IRC-authored tagged replies should also store that
  visible quote line, or whether tag-aware clients should be allowed to create
  cleaner body-only replies. Keeping the quote line is less surprising for the
  TUI.

## Edits, Deletes, And Pins

Current IRC projection sends edits as a new `PRIVMSG` prefixed with `[edit]` and
does not project deletes. IRCv3 can improve this after `msgid` exists:

- Edits: there is no single widely settled edit mechanism in the same class as
  `server-time` or `msgid`. Keep `[edit]` until a supported edit protocol is
  chosen.
- Deletes: `message-redaction` can map to late.sh hard deletes and admin
  deletes. This needs permission checks and careful behavior for clients that
  did not receive the original message.
- Pins: no obvious common IRCv3 mapping. A vendor tag such as
  `late.sh/pinned=1`, a server NOTICE, or plain omission are all plausible.
  Since pins are dashboard/global, not room-local per-user state, omit them from
  IRC until there is a clear client behavior target.

## Polls

Polls do not have a direct common IRCv3 feature. Reasonable options:

- Plain text projection: keep or improve the current result-message approach.
- Vendor tags: emit `@late.sh/poll=...` metadata for clients/bots that know how
  to render late.sh polls.
- Commands: accept plain IRC commands/messages such as `!poll` or `/msg` bot
  commands only if late.sh wants an IRC-native poll UX.

Do not try to force polls into `batch`; batches group messages but do not define
poll semantics.

## Presence, Away, And Typing

Presence:

- Current IRC presence combines TUI active users and IRC sessions, then projects
  arrivals/departures as JOIN/QUIT for shared channels.
- `MONITOR` could add nickname watch semantics for users who are not in a
  shared channel or for DM-heavy clients.

Away:

- `/brb` already marks a session AFK and renders a moon badge in TUI chat.
- IRC `AWAY` is currently acknowledged but not surfaced into late.sh state.
- A future `away-notify` slice should make `/brb` and IRC `AWAY` update the
  same session-level AFK state, then broadcast `AWAY` changes to clients that
  negotiated `away-notify`.

Typing:

- TUI does not currently have a cross-session typing event.
- IRCv3 `+typing` is useful but privacy-sensitive. If added, make it opt-in and
  transient; never persist typing state.

## Account And Profile Metadata

Because IRC nick is locked to the late.sh username, account identity is already
stronger than normal IRC. Still, IRCv3 identity features can make that explicit:

- `account-tag`: attach `account=<username>` or a stable account slug to
  messages.
- `extended-join`: include account and realname/profile display data in JOIN.
- `WHOX` / `userhost-in-names`: expose predictable user/account fields for
  clients and bots.
- `metadata-2`: probably too broad for first-pass profile/badge export.

Profile awards, bonsai glyphs, flags, store badges, and friend markers are
better treated as late.sh-specific presentation metadata. If exposed at all,
prefer vendor tags after `message-tags` exists.

## History And Read State

late.sh has DB-backed room tails and `last_read_at`, so the data model can
support history and read markers.

- `chathistory`: most direct match for room tail and before/after history
  windows. Use `batch` to group replayed messages.
- `server-time`: required for replay to feel correct.
- `read-marker`: possible mapping to `chat_room_members.last_read_at`, but it
  should wait until history replay exists and the product wants IRC clients to
  mutate TUI unread state.
- `no-implicit-names`: useful optimization later for large rooms, but current
  membership scale probably does not justify prioritizing it.

## Suggested Phasing

1. Foundation caps: `message-tags`, `server-time`, `msgid`, and `echo-message`.
2. Replies: inbound/outbound `+reply` mapped to `reply_to_message_id`.
3. Reactions: `+draft/react` / `+draft/unreact` with a reaction delta event.
4. Away: map `/brb` and IRC `AWAY` through shared AFK state, then advertise
   `away-notify`.
5. History: `chathistory` plus `batch` for bounded replay.
6. Deletes: evaluate `message-redaction` after stable `msgid` and history exist.
7. Nice-to-haves: `account-tag`, `extended-join`, `MONITOR`, `typing`, and
   vendor tags for late.sh-specific profile/chat flair.

## Source Pointers

- General IRCv3 overview in this repo: `devdocs/ABOUT-IRC-V3.md`
- Existing IRC bridge requirements: `devdocs/FRD-IRCD.md`
- IRC bridge context: `late-ssh/src/ircd/CONTEXT.md`
- Chat context: `late-ssh/src/app/chat/CONTEXT.md`
- Message model: `late-core/src/models/chat_message.rs`
- Reaction model: `late-core/src/models/chat_message_reaction.rs`
- Poll model: `late-core/src/models/chat_poll.rs`
- IRCv3 reply tag: <https://ircv3.net/specs/client-tags/reply.html>
- IRCv3 react tag: <https://ircv3.net/specs/client-tags/react.html>
- IRCv3 typing tag: <https://ircv3.net/specs/client-tags/typing.html>
