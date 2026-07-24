# About IRCv3

IRCv3 is a collection of backwards-compatible extensions to IRC. Most IRCv3
features are negotiated with `CAP`, so old clients can still connect to newer
servers and newer clients can opt in only to features they understand.

This document is a general reference for IRCv3 features that are widely useful
and commonly supported by modern IRC clients, servers, bouncers, bots, or
libraries. It is not a statement about what this repository currently supports.

## Core negotiation and framing

- `CAP` / `CAP LS 302`: lets clients discover, request, enable, and disable
  protocol extensions during registration. This is the foundation most IRCv3
  features build on.
- `cap-notify`: tells clients when capabilities become available or disappear
  after registration, such as when an authentication service reconnects.
- `message-tags`: adds structured key/value tags to IRC messages. Many other
  IRCv3 features use tags, even when `message-tags` is not directly visible to
  users.

## Authentication and account tracking

- `sasl`: standard account authentication during connection registration. It
  avoids service-bot flows such as `NickServ IDENTIFY` and lets clients join
  restricted channels immediately after connecting.
- `account-notify`: sends `ACCOUNT` updates when users in shared channels log
  in, log out, or change account state.
- `account-tag`: attaches the sender's account name to messages, making account
  identity available to clients and bots without extra lookups.
- `extended-join`: adds account and realname information to `JOIN`, improving
  initial channel state after joining.

## Presence and user state

- `away-notify`: sends immediate `AWAY` updates for users in shared channels,
  replacing polling-based away checks.
- `chghost`: sends `CHGHOST` when a user's username or hostname changes,
  avoiding fake leave/rejoin events for host changes.
- `setname`: lets users update their realname/gecos after connecting and lets
  other clients observe those changes.
- `BOT` mode: lets automated clients identify themselves as bots, with matching
  display behavior in clients that understand the mode.

## Message delivery and metadata

- `server-time`: adds a `time` tag so clients can display when the server
  received or replayed a message. This is especially useful with bouncers and
  history playback.
- `echo-message`: echoes a client's own `PRIVMSG` and `NOTICE` messages back
  after the server accepts them. This helps clients confirm delivery and helps
  bouncer users see messages sent from another attached session.
- `msgid`: adds stable message identifiers, giving clients and later
  extensions a way to refer to specific messages.
- `labeled-response`: lets clients tag commands with labels and correlate
  returned numerics or responses with the command that caused them.
- `standard-replies`: standardizes structured `FAIL`, `WARN`, and `NOTE`
  responses so servers can report errors and warnings without inventing
  conflicting numerics.

## Channel state and user lists

- `multi-prefix`: includes all of a user's channel status prefixes in `NAMES`
  and `WHO` output, not only the highest-ranking prefix.
- `userhost-in-names`: includes full `nick!user@host` masks in `NAMES` output,
  reducing follow-up lookups after joining a channel.
- `WHOX`: extends `WHO` so clients can request richer, structured user data.
- `MONITOR`: provides standardized online/offline notifications for watched
  nicknames, replacing polling with `ISON` or network-specific `WATCH`.
- `extended-monitor`: extends `MONITOR` style tracking to more events.
- `no-implicit-names`: lets clients opt out of automatic `NAMES` replies after
  joining channels when they do not need that data.

## Channel and network events

- `invite-notify`: notifies privileged channel users when someone is invited to
  a channel.
- `batch`: groups related messages together, such as netsplit/netjoin bursts or
  replayed history, so clients can display or process them as one event.

## Transport and connection policy

- `sts`: Strict Transport Security for IRC. It lets clients automatically use
  TLS and helps prevent downgrade attacks.
- `UTF8ONLY`: an `ISUPPORT` token that lets servers advertise that traffic is
  UTF-8 only, allowing clients to choose encoding behavior without guessing.

## Web gateway support

- `WEBIRC`: lets web IRC gateways tell the IRC server the real client IP
  address and hostname instead of only the gateway's address. This is common
  for browser-based IRC clients and hosted web gateways.

## Common but less uniformly deployed features

These are useful IRCv3 features, but support is more uneven or the feature is
still draft/work-in-progress in the IRCv3 ecosystem:

- `chathistory`: lets clients request stored message history from servers or
  bouncers.
- `draft/multiline`: sends messages that exceed the traditional IRC line limit
  or contain line breaks.
- `read-marker`: synchronizes read markers across multiple clients for the same
  user.
- `metadata-2`: associates retrievable metadata with users.
- `channel-rename`: renames a channel without replacing it with a new channel.
- Client-only tags such as `+typing`, `+reply`, and `+draft/react`: carry
  direct client-to-client interaction hints when both clients understand them.

## Sources

- IRCv3 specifications index: <https://ircv3.net/irc/>
- IRCv3 capability negotiation: <https://ircv3.net/specs/extensions/capability-negotiation>
- IRCv3 client support table: <https://ircv3.net/software/clients>
- IRCv3 server support table: <https://ircv3.net/software/servers>
- IRCv3 library support table: <https://ircv3.net/software/libraries>
- IRC capability registry: <https://defs.ircdocs.horse/defs/clientcaps.html>
