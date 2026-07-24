# Plan: late.sh IRCv3 Tier 1

This plan turns the "High" fit row from
`devdocs/LATE-AND-IRC-V3-IDEAS.md` into an implementation sequence for the
embedded late.sh ircd.

Tier 1 is the first structured IRCv3 chat slice:

- `message-tags`
- `server-time`
- `msgid`
- `echo-message`
- `+reply`
- `+draft/react` / `+draft/unreact`

The product goal is not "IRCv3 completeness". The goal is to expose late.sh's
existing chat semantics - timestamps, stable message identity, replies, and
reactions - to capable IRC clients while keeping plain IRC clients compatible.

## Current State

The current IRC bridge is intentionally simple:

- `CAP LS` advertises no capabilities and `CAP REQ` NAKs everything.
- Outbound chat messages are plain `PRIVMSG` lines.
- `UTF8ONLY` is advertised via ISUPPORT, but IRCv3 message tags are not.
- IRC-originated channel messages use `ChatService::send_message_task`.
- IRC-originated DMs use/create the same late.sh DM rooms as the TUI.
- TUI/IRC message creation and edits project to IRC as `PRIVMSG`.
- Edits are rendered as a fresh `PRIVMSG` prefixed with `[edit]`.
- Reactions, deletes, pins, polls, and typing events are not projected to IRC.
- Self-echo suppression is currently local and body-based: the same IRC
  connection skips one matching message body it just sent.

The late.sh chat model already has the needed data:

- `chat_messages.id` is a stable UUID.
- `chat_messages.created` is the canonical message timestamp.
- `chat_messages.reply_to_message_id` stores reply targets.
- `chat_message_reactions` stores one reaction icon per `(message_id, user_id)`.
- `ChatService::send_message_with_reply_task` already validates reply targets.
- `ChatEvent::MessageReactionsUpdated` carries reaction summaries, but not the
  actor/action delta IRC needs to project precise react/unreact TAGMSG events.

## Client Survey Summary

The surveyed clients split into two groups:

- Solid Tier 1 foundation support: Halloy, Goguma, WeeChat, Irssi, Senpai,
  glirc/irc-core, and AndroidIRCx all have useful support for some combination
  of `message-tags`, `server-time`, `msgid`, and `echo-message`.
- Rich reaction/reply UX: Halloy and Goguma support the IRCv3 `+reply` plus
  `+draft/react` / `+draft/unreact` shape directly. AndroidIRCx has reaction UI
  and TAGMSG handling, but uses a different `+draft/react=<msgid>;<emoji>`
  payload shape, so it is evidence for reaction UX but not the primary wire
  format target.

Tier 1 should therefore optimize for:

1. Better metadata in every capable client.
2. Native replies/reactions in Halloy and Goguma.
3. Harmless ignored metadata in clients that do not render replies/reactions.

## PR Shape

Tier 1 is coherent as one PR because the high-fit items form a dependency chain:

```text
message-tags -> server-time / msgid / echo-message -> +reply -> +draft/react
```

Keep the PR internally staged by the milestones below. If the diff gets large,
split after Milestone 5:

- PR A: CAP negotiation, tags, `server-time`, `msgid`, `echo-message`, replies.
- PR B: reaction delta contract plus inbound/outbound reaction TAGMSG.

Do not land reactions before the foundation. Reactions need stable `msgid`,
`+reply`, and `echo-message` to be usable in real clients.

## Normative Shape

Use the IRCv3 specs as the primary wire shape:

- `message-tags` is the only capability required for generic message tag parsing
  and `TAGMSG`.
- `server-time` is separately negotiated and uses the `time` tag.
- `msgid` is a tag, not its own capability. It requires `message-tags`.
- `echo-message` should be available so clients receive final server-assigned
  tags for their own accepted messages.
- Replies use the client-only tag `+reply=<msgid>`.
- Reactions use `+reply=<msgid>` plus exactly one of:
  - `+draft/react=<reaction>`
  - `+draft/unreact=<reaction>`
- Reactions may be sent as tag-only `TAGMSG` events. They may also be attached
  to a `PRIVMSG`, but the first late.sh implementation should use `TAGMSG`.

Accept both `+reply` and legacy-compatible `+draft/reply` inbound, because
Halloy and Goguma both still emit or understand the draft spelling. Emit
`+reply` as the canonical tag; optionally also emit `+draft/reply` only if a
client interop test shows that it materially improves compatibility.

Do not implement AndroidIRCx's `+draft/react=<msgid>;<emoji>` shape in Tier 1
unless explicitly adding a compatibility mode. It is not the current IRCv3
draft shape and it complicates validation.

## Non-Goals

Tier 1 does not include:

- `chathistory` or history replay.
- `batch`.
- `away-notify`.
- `typing`.
- `account-tag`.
- `message-redaction`.
- `SASL`.
- `standard-replies` / `labeled-response` beyond whatever is already present.
- Edit protocol changes. Keep `[edit]` projection for now.
- Delete projection. Keep current silent-delete behavior for now.
- Polls, pins, embeds, game-room chat, or room rename mapping.

## Milestone 1: CAP And Tag Foundation

Implement per-connection capability state in `late-ssh/src/ircd/conn.rs`.

Advertise on `CAP LS`:

```text
message-tags server-time echo-message
```

Do not advertise `msgid`; it is a tag provided after `message-tags`, not a
separate IRCv3 capability.

Handle `CAP REQ` as follows:

- ACK supported caps.
- NAK unsupported caps.
- Track enabled caps per connection.
- Support `CAP LIST` after registration.
- Keep `CAP END` registration gating behavior.
- Preserve the current behavior that unknown CAP traffic never wedges
  registration.

Add a small helper for capability-gated message construction:

- Tags are serialized only for sessions that negotiated `message-tags` or the
  specific tag capability that permits them.
- `time` is sent only when `server-time` is enabled.
- `msgid`, `+reply`, and reaction tags are sent only when `message-tags` is
  enabled.
- `TAGMSG` reaction events are sent only to sessions with `message-tags`.
- Untagged `PRIVMSG` fallback remains the default for older clients.

Consider emitting `CLIENTTAGDENY` in ISUPPORT with a tight allowlist once inbound
client-only tags are accepted. If added, use an allowlist that permits the Tier 1
tags and denies everything else, for example:

```text
CLIENTTAGDENY=*,-reply,-draft/reply,-draft/react,-draft/unreact
```

If client testing shows better compatibility without the token, omit it and
enforce the allowlist server-side.

Acceptance checks:

- Existing non-CAP clients still register and chat.
- `CAP LS 302` returns the Tier 1 capabilities.
- `CAP REQ :message-tags server-time echo-message` receives ACK.
- `CAP REQ :chathistory` receives NAK and registration can continue.
- `CAP LIST` reports only acknowledged caps.

## Milestone 2: Outbound `server-time` And `msgid`

Extend IRC message projection so outbound `PRIVMSG` and `NOTICE` lines can carry
server-assigned tags.

Mapping:

- `@time=` comes from `chat_messages.created`.
- `@msgid=` comes from `chat_messages.id`.
- Use the UUID string directly unless a concrete client bug says otherwise.
  Message IDs are opaque and case-sensitive; UUIDs are stable and already fit
  the allowed character set.

For split messages:

- Prefer sending `msgid` only on the first IRC line produced from one late.sh
  message.
- Send `time` on every split line if `server-time` is enabled.
- Keep all split lines as separate `PRIVMSG`s. Do not introduce multiline in
  Tier 1.

For edits:

- Keep `[edit]`.
- Reuse the original message's `msgid` only if the edit is being represented as
  the same logical message. Otherwise omit `msgid` from edit projections until
  an edit protocol is chosen.
- Preferred Tier 1 behavior: include `time`, omit `msgid` on `[edit]` fallback
  lines to avoid implying this is the original immutable message event.

Acceptance checks:

- Tag-aware clients receive `@time` and `@msgid` on normal messages.
- Tag-unaware clients receive the same visible body as before.
- Message IDs are stable enough for replies and reactions.
- Long-message splitting does not generate duplicate `msgid` values on multiple
  visible lines.

## Milestone 3: `echo-message`

Replace body-based self-echo suppression with capability-aware echo behavior.

Behavior:

- If the IRC session negotiated `echo-message`, deliver its own accepted
  `PRIVMSG`, `NOTICE`, and `TAGMSG` events back to it with final tags.
- If it did not negotiate `echo-message`, preserve current self-echo suppression
  for messages sent by that same IRC connection.
- Other IRC connections and TUI sessions for the same user should still receive
  normal bouncer-like echoes.

Implementation note:

- The current `recent_sends` queue matches by `(room_id, body)`.
- For `echo-message`, either bypass that suppression entirely for negotiated
  sessions or track pending sends with enough request identity to avoid
  suppressing the accepted event.
- This milestone is important before replies/reactions because clients need to
  learn the `msgid` for messages they just sent.

Acceptance checks:

- A client without `echo-message` does not see duplicate local sends.
- A client with `echo-message` sees its own accepted message once, with `msgid`
  and `time` when applicable.
- A second IRC connection for the same user sees messages from the first
  connection.

## Milestone 4: Outbound Replies

Project TUI-authored replies to IRC.

Mapping:

```text
@msgid=<child>;time=<created>;+reply=<parent> :nick!nick@late.sh PRIVMSG #room :body
```

Rules:

- Only emit `+reply` to sessions that negotiated `message-tags`.
- Validate that `reply_to_message_id` belongs to the same room before emitting.
  The DB invariant and send path already enforce this for new messages; the IRC
  projection should still be defensive.
- If the parent is missing because of delete/nulling, omit `+reply`.
- Keep the visible body exactly as late.sh stores it. Do not strip TUI-visible
  quote text in Tier 1.

Acceptance checks:

- Halloy/Goguma show TUI-authored replies as replies.
- Plain IRC clients still show the message body.
- Deleted or inaccessible parent messages do not crash projection.

## Milestone 5: Inbound Replies

Parse inbound client-only reply tags on `PRIVMSG` and `NOTICE`.

Accepted forms:

```text
@+reply=<parent> PRIVMSG #room :body
@+draft/reply=<parent> PRIVMSG #room :body
```

Behavior:

- Resolve `<parent>` as a `chat_messages.id` UUID.
- Validate target room membership and ban state through the existing
  `ChatService` send path.
- Validate that the parent message is in the same room.
- Call `ChatService::send_message_with_reply_task`.
- On invalid parent ID or wrong-room parent, reject the send for IRC with a
  NOTICE or numeric consistent with current IRC error style.

For DMs:

- `PRIVMSG nick` replies should work only when the parent message belongs to the
  resolved DM room.
- Do not let a reply tag create a cross-room or cross-DM relation.

Acceptance checks:

- Halloy/Goguma outbound replies create late.sh replies.
- The TUI renders IRC-authored replies using existing reply UI.
- Wrong-room, unknown, malformed, or inaccessible `msgid` values are rejected.
- Untagged sends still use the current path.

## Milestone 6: Reaction Delta Contract

Add a precise reaction event without breaking current TUI rendering.

Current event:

```rust
ChatEvent::MessageReactionsUpdated {
    room_id,
    message_id,
    reactions,
    target_user_ids,
}
```

Tier 1 needs a delta:

```rust
ChatReactionDelta {
    room_id: Uuid,
    message_id: Uuid,
    actor_user_id: Uuid,
    icon: String,
    action: ChatReactionAction,
    previous_icon: Option<String>,
    target_user_ids: Option<Vec<Uuid>>,
}
```

Where:

```rust
enum ChatReactionAction {
    React,
    Unreact,
    Replace,
}
```

Recommended implementation:

- Change `ChatMessageReaction::toggle` or add a new model method that returns
  the action it performed.
- Preserve late.sh semantics: one reaction per user per message.
- If the same icon exists, `unreact`.
- If no reaction exists, `react`.
- If a different icon exists, `replace`.
- Continue emitting `MessageReactionsUpdated` for TUI summary updates if that is
  the simplest local rendering path.
- Also emit a new delta event for IRC projection.

Acceptance checks:

- Existing TUI reaction summaries still update.
- The delta identifies actor, room, message, icon, and action.
- Replacement can be represented to IRC as unreact old icon plus react new icon,
  or as just react new icon if client testing shows that is enough. Prefer two
  events for semantic clarity.

## Milestone 7: Inbound Reactions

Parse inbound `TAGMSG` and reaction-bearing `PRIVMSG`.

Accepted canonical forms:

```text
@+reply=<msgid>;+draft/react=<icon> TAGMSG #room
@+reply=<msgid>;+draft/unreact=<icon> TAGMSG #room
@+reply=<msgid>;+draft/react=<icon> PRIVMSG #room :<fallback text>
```

Also accept `+draft/reply=<msgid>` as a compatibility alias for `+reply`.

Validation:

- Exactly one of `+draft/react` and `+draft/unreact` may appear.
- `+reply` / `+draft/reply` is required.
- Target must be a visible channel or resolved DM query target.
- Parent message must exist and belong to that target room.
- Sender must be a member and not room-banned.
- Reaction icon must satisfy the same validation as the TUI path.
- Unreact removes only if the current stored icon matches the provided icon.
- A different react replaces the current stored icon, matching late.sh's
  one-reaction-per-user model.

Do not store the fallback text body of reaction-bearing `PRIVMSG` as a normal
late.sh message in Tier 1. Treat the reaction tag as authoritative and ignore
the body after validation. Revisit only if compatibility testing shows clients
need visible fallback messages.

Acceptance checks:

- Halloy/Goguma reaction TAGMSG changes late.sh reaction state.
- The TUI updates reaction chips from IRC-authored reactions.
- Duplicate same-icon `+draft/react` toggles off to match late.sh's existing
  one-reaction-per-user behavior; `+draft/unreact` also removes the reaction
  when the stored icon matches.
- Malformed reaction TAGMSG does not create a message.

## Milestone 8: Outbound Reactions

Project late.sh reaction deltas to IRC clients that negotiated `message-tags`.

Canonical outbound forms:

```text
@+reply=<msgid>;+draft/react=<icon> :nick!nick@late.sh TAGMSG #room
@+reply=<msgid>;+draft/unreact=<icon> :nick!nick@late.sh TAGMSG #room
```

Rules:

- Send only to clients that negotiated `message-tags`.
- Respect channel join state, private-room visibility, DM targeting, and ignore
  filtering in the same spirit as message projection.
- Suppress self echo for reaction TAGMSG only when `echo-message` is not enabled
  for that connection.
- If a reaction is replaced, emit unreact-old then react-new when the old icon is
  known.
- Do not send NOTICE fallbacks to tag-unaware clients by default. Reactions are
  lightweight annotations; noisy fallback lines would be worse than omission.

Acceptance checks:

- Halloy/Goguma display TUI-authored reactions.
- Tag-unaware clients do not see reaction noise.
- The sender sees its own reaction TAGMSG only if `echo-message` is enabled.
- Reactions in DMs work only for the DM participants.

## Milestone 9: Tests And Verification

Doc-only work needs no compile/test run. Code implementation should add focused
unit and integration coverage.

Unit coverage:

- Capability set parsing and ACK/NAK grouping.
- Message tag serialization and escaping if not fully delegated to `irc-proto`.
- `msgid` formatting from UUIDs.
- Client-only tag allowlist decisions.
- Reaction delta classification: react, unreact, replace.
- Reaction TAGMSG parsing and validation helpers.

Integration coverage under `late-ssh/tests/ircd/`:

- CAP LS/REQ/LIST for Tier 1 caps.
- Outbound TUI message projects `time` and `msgid` to a tag-aware IRC client.
- Untagged fallback remains unchanged for a client that did not negotiate
  `message-tags`.
- `echo-message` negotiated client receives its own accepted send with `msgid`.
- Non-`echo-message` client keeps current self-echo suppression.
- IRC `+reply` send stores `reply_to_message_id`.
- TUI reply projects as `+reply`.
- IRC reaction TAGMSG updates late.sh reaction state.
- TUI reaction projects as `+draft/react` / `+draft/unreact`.
- Wrong-room and unknown-msgid reply/reaction cases are rejected.

Per local context, agents may use:

```bash
cargo check -p late-core -p late-ssh
```

Leave full `cargo test`, `cargo nextest`, and `cargo clippy` runs to the human
owner unless explicitly asked.

## Likely Code Touchpoints

- `vendor/irc-proto/`: confirm `TAGMSG` and message tag round-tripping are
  available; patch vendored protocol support only if needed.
- `late-ssh/src/ircd/conn.rs`: CAP state, inbound tag parsing, inbound replies,
  inbound reactions, echo-message behavior, projection gates.
- `late-ssh/src/ircd/replies.rs`: message constructors that can carry tags.
- `late-ssh/src/ircd/proj.rs`: pure helpers for tag formatting, msgid mapping,
  target resolution, and reaction parsing.
- `late-ssh/src/app/chat/svc.rs`: reaction delta event and possibly explicit
  react/unreact service entrypoints.
- `late-core/src/models/chat_message_reaction.rs`: reaction mutation method that
  reports action and previous icon.
- `late-core/src/models/chat_message.rs`: any helper needed to look up messages
  by IRC `msgid`/UUID and room.
- `late-ssh/tests/ircd/`: real TCP smoke coverage for negotiated behavior.

## Open Decisions

- Should outbound replies include only `+reply`, or both `+reply` and
  `+draft/reply` for compatibility? Start with `+reply`; add dual tags only if
  client testing justifies it.
- Should `CLIENTTAGDENY` be advertised as an explicit allowlist? This is cleaner
  protocol posture but should be checked against Halloy and Goguma before
  committing.
- Should reaction replacements emit two TAGMSG events (`unreact old`, `react
  new`) or only `react new`? Two events better preserve semantics.
- Should reaction-bearing `PRIVMSG` fallback bodies ever become visible late.sh
  messages? Tier 1 says no.
- Should edit fallback lines carry the original `msgid`? Tier 1 says no until an
  edit protocol is chosen.

## Done Definition

Tier 1 is done when:

- Tag-aware clients can negotiate `message-tags`, `server-time`, and
  `echo-message`.
- Normal outbound late.sh messages include `time` and `msgid` where negotiated.
- `echo-message` clients receive their own accepted messages with final tags.
- TUI-authored replies appear as IRC replies in Halloy/Goguma.
- IRC-authored replies create late.sh replies.
- TUI-authored reactions appear as IRC reactions in Halloy/Goguma.
- IRC-authored reactions update late.sh reaction chips.
- Untagged/plain clients still behave as they do today.
- The implementation does not introduce direct DB side doors around
  `ChatService` for sends, replies, membership checks, or moderation checks.

## Sources

- `devdocs/ABOUT-IRC-V3.md`
- `devdocs/LATE-AND-IRC-V3-IDEAS.md`
- `devdocs/FRD-IRCD.md`
- `devdocs/TODO-IRCD.md`
- `late-ssh/src/ircd/CONTEXT.md`
- `late-ssh/src/app/chat/CONTEXT.md`
- Local client survey paths: `~/p/gh/irssi`, `~/p/gh/halloy`,
  `~/p/gh/weechat`, `~/p/gh/goguma`, `~/p/gh/irc-core`,
  `~/p/gh/senpai`, `~/p/gh/AndroidIRCx`
- IRCv3 message tags: <https://ircv3.net/specs/extensions/message-tags.html>
- IRCv3 message IDs: <https://ircv3.net/specs/extensions/message-ids>
- IRCv3 `reply` client tag: <https://ircv3.net/specs/client-tags/reply.html>
- IRCv3 `react` client tag: <https://ircv3.net/specs/client-tags/react.html>
