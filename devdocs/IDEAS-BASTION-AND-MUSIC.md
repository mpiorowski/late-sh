# Bastion and Music Session Association

> Status: idea note. This is not implementation-locked.

The bastion work gives `late.sh` a stable user SSH connection across `late-ssh`
restarts, but music pairing is still tied to process-local `late-ssh` session
tokens. If the backend restarts, the SSH channel can survive through
`late-bastion`, but the browser tab or `late-cli` websocket has no durable thing
to reattach to.

This note explores what it would take to restore that association for:

- a user listening through the browser pairing page
- a user listening through `late-cli`

## Current Shape

### Browser audio pairing

The TUI exposes a connect URL derived from `web_url + session_token`.
Pressing `P` copies and shows that URL. The browser page at `/connect/{token}`
opens:

- an audio stream from `late-web`'s `/stream`, which proxies Icecast/Liquidsoap
- a websocket to `late-ssh` at `/api/ws/pair?token=...`
- a browser-side analyzer loop that sends heartbeat and visualizer frames
- `client_state` messages with kind, muted state, and volume

On the backend, `/api/ws/pair` only accepts the websocket if
`SessionRegistry::has_session(token)` is true. That registry is in memory in the
current `late-ssh` process. The paired client registry is also in memory and is
keyed by the same token.

So today the browser association is "this browser websocket belongs to this live
in-process TUI session token." It is intentionally lightweight, but it cannot
survive a backend restart.

### `late-cli` pairing

`late-cli` starts local audio first, then starts SSH, obtains a session token,
and opens the same `/api/ws/pair?token=...` websocket. It sends visualizer frames
derived from local playback and receives terminal-originated controls like mute
and volume changes.

There are two token acquisition paths today:

- native `late-cli` opens a small SSH exec request, `late-cli-token-v1`, before
  opening the interactive shell
- subprocess mode captures a `LATE_SESSION_TOKEN=...` banner written at shell
  startup when `LATE_CLI_MODE=1` is set

Both depend on the direct `late-ssh` russh path.

### Bastion `/tunnel`

The bastion keeps a stable SSH channel and reconnects to `late-ssh` over
`/tunnel`. Its handshake includes a stable `X-Late-Session-Id`, minted once per
bastion shell channel and reused across reconnects.

That is exactly the piece music restoration wants. The catch: the current
`/tunnel` handler creates a fresh backend-local session token and passes
`session_rx: None` into `SessionConfig`. In other words, bastion sessions
currently do not participate in the public pair websocket routing at all.

## Problem Statement

After a `late-ssh` restart, there are three different continuities:

- SSH continuity: with bastion, the user's terminal connection survives.
- Audio playback continuity: browser or CLI audio may keep playing because the
  audio stream is separate from `late-ssh`.
- Association continuity: the TUI no longer knows that the still-playing browser
  tab or CLI process belongs to the newly-created TUI session.

The third one is what we need to restore.

The user-visible symptoms are:

- visualizer data stops after backend restart
- the sidebar says "No pair" until the user manually pairs again
- terminal mute/volume controls no longer reach the browser or CLI
- the reconnect-created TUI may show a new QR/link, invalidating the old browser
  pairing page URL

## Design Goal

When a bastion-backed SSH session reconnects to a new `late-ssh` pod, a browser
tab or `late-cli` process that was already paired should automatically attach to
the new TUI session, without user action.

It is acceptable for in-app TUI state to reset. The desired continuity is only:

- "this external audio client still belongs to this SSH user session"
- visualizer frames route to the new `App`
- mute and volume controls route back to the same external client
- the existing browser pairing URL remains useful during one SSH session

## Recommended Direction

Use the bastion session id as the durable pairing key.

The current ephemeral `session_token` answers "which in-process App should get
this frame?" A restart-resilient design needs a second concept:

- `session_instance_token`: short-lived, process-local route to an `App`
- `pairing_session_id`: stable for the life of the bastion SSH channel

For direct, non-bastion SSH, the existing session token can continue to be both.
For bastion `/tunnel`, `pairing_session_id` should come from
`X-Late-Session-Id`.

### Sketch

1. Bastion opens `/tunnel` with stable `X-Late-Session-Id`.
2. `late-ssh` builds the TUI with `session_token = pair_token(session_id)` or
   with both `session_token` and `pairing_session_id`.
3. The TUI's `P` link uses the stable pair token, not the backend-local token.
4. `/api/ws/pair?token=...` can accept a token even when no `App` is currently
   registered, as long as the token is a valid live/leased pairing token.
5. When the new backend pod comes up and the bastion reconnects, it registers
   the same stable pairing token to the new `SessionMessage` channel.
6. The browser or CLI websocket reconnect loop succeeds again and resumes
   sending frames to the new `App`.

## Where the State Should Live

There are three plausible homes for the durable association.

### Option A: signed stateless pair token

Encode the bastion session id into a signed token:

```text
pair_token = base64url(session_id, user_id, issued_at, exp, nonce, signature)
```

The backend validates the signature and expiration. No DB write is needed to
accept the websocket.

Pros:

- small implementation
- no new table or cleanup job
- browser URL survives a `late-ssh` restart
- works naturally with the existing browser and CLI reconnect loops

Cons:

- revocation is coarse unless we also keep deny/lease state
- `/api/ws/pair` can accept a websocket before the TUI session exists, so it
  needs a pending/reattach mode
- terminal controls still need an in-memory paired client entry after reattach

This is likely the best first version.

### Option B: DB-backed pairing lease

Create a small table keyed by `pairing_session_id`:

```text
music_pairing_leases(
  pairing_session_id text primary key,
  user_id uuid not null,
  pair_token_hash text not null,
  transport text not null,
  created_at timestamptz not null,
  last_seen_at timestamptz not null,
  expires_at timestamptz not null
)
```

`/tunnel` upserts the lease on session start/reconnect. `/api/ws/pair` validates
the token against the lease and either attaches immediately or waits for the TUI
side to register.

Pros:

- explicit revocation and cleanup
- easier observability
- future multi-bastion/multi-pod routing has a real data model

Cons:

- more moving parts
- DB dependency in a hot-ish websocket path
- needs careful cleanup when SSH sessions end uncleanly

This is the durable, grown-up version if the feature becomes important.

### Option C: bastion-owned pairing broker

Move the pair websocket routing into the bastion, or have the bastion mint and
serve pairing tokens itself.

Pros:

- the bastion already owns the stable SSH session
- no backend restart can erase the broker's association state

Cons:

- makes the bastion meaningfully smarter
- adds public HTTP/websocket surface or token-minting responsibility to a process
  whose whole point is being boring
- couples music/web behavior to the SSH frontend

This is probably too much for now. The bastion should remain the keeper of the
stable session id, not the owner of music behavior.

## Browser Path

For browser audio, restoration can be mostly transparent.

Browser page behavior already has the right shape:

- the page keeps the token in its URL
- the websocket reconnects up to a limit
- audio stream reconnect is independent of the pair websocket
- client state is resent on websocket open

Needed changes:

- make the token stable across bastion reconnects
- allow `/api/ws/pair` to accept a valid token during backend reconnect gaps
- keep the websocket open in a "waiting for terminal" state, or return a retryable
  close/status that the page treats as reconnectable
- when the TUI registers the stable token again, route frames to the new app
- reset the sidebar paired-client display from the latest `client_state`

The browser does not need to know about backend restarts. It should just keep
audio playing and keep trying to pair.

One nuance: browser autoplay rules mean we should not intentionally replace the
audio element or require a fresh click after reconnect. Treat pair reconnection
as control-plane repair only.

## `late-cli` Path

`late-cli` is trickier because the token is discovered over SSH.

For direct `late-ssh` mode, the current flow is fine but cannot survive a backend
restart because the SSH connection itself dies. That is acceptable unless we
choose to route `late-cli` through the bastion.

For bastion-routed `late-cli`, there are two reasonable approaches.

### Preferred: make bastion shell sessions expose a stable token

When `late-cli` connects through bastion, the bastion can write the existing
`LATE_SESSION_TOKEN=...` banner before it starts piping backend TUI bytes, or the
backend can write it after `/tunnel` startup using a token derived from
`X-Late-Session-Id`.

Better fit with the current minimal bastion design: backend writes the banner,
but only after deriving a stable pair token from the bastion session id. The
bastion remains an opaque byte pump.

Needed changes:

- extend `/tunnel` so it supports `session_rx: Some(...)` and registers the
  stable pair token in `SessionRegistry`
- add enough handshake metadata for the backend to know this is a CLI session,
  probably forwarded from `LATE_CLI_MODE` or a future SSH env request handled by
  the bastion
- have the `/tunnel` backend path emit `LATE_SESSION_TOKEN=...` for CLI-mode
  sessions before entering the normal alt-screen/TUI output
- keep `late-cli`'s existing websocket reconnect loop; it can reuse the same
  stable token after backend restarts

Open issue: the current bastion only supports `pty-req`, `shell`, data, and
window-change. It does not support the native CLI's `late-cli-token-v1` exec
handshake. That means native `late-cli` would need either the banner path or a
bastion-supported token exec path.

### Alternative: teach bastion the CLI token exec

The bastion could answer `late-cli-token-v1` itself with a token derived from the
same session id it will use for `/tunnel`.

Pros:

- native `late-cli` keeps its clean pre-shell token fetch
- token is available before the backend is even connected

Cons:

- bastion now has token minting knowledge
- token format/signing must be shared with backend anyway
- more protocol surface in the bastion

This is feasible, but it bends the bastion design more than the banner path.

## Backend Routing Changes

The core backend change is to separate "token validity" from "live app route."

Today:

```text
/api/ws/pair accepts only if SessionRegistry has token
SessionRegistry token -> mpsc Sender<SessionMessage>
PairedClientRegistry token -> control tx + latest ClientAudioState
```

Proposed:

```text
PairTokenAuthority validates stable token
SessionRegistry token -> optional live app sender
PairedClientRegistry token -> optional live external client sender + latest state
PendingPairRegistry token -> websocket(s) waiting for app registration
```

When a pair websocket is alive but no app is registered, heartbeat/client state
can be accepted and remembered, while visualizer frames can be dropped. Once the
app registers, new visualizer frames route normally.

This matters during the restart gap: we do not need to buffer audio visualizer
history. We just need to avoid losing the association.

## Security Notes

- Do not expose raw `X-Late-Session-Id` as the public token. Sign it or map it to
  a random public token.
- Tokens should expire. A reasonable initial lifetime is "bastion session life +
  short grace window"; with stateless tokens, use a wall-clock TTL like 12 hours.
- `/api/ws/pair` should remain rate limited.
- Pair tokens authorize visualizer/control association only, not account access.
- If using signed stateless tokens, rotate signing keys with overlap.
- If using DB leases, store token hashes rather than raw tokens.

## Suggested MVP

1. Add a stable pair token derivation for `/tunnel` sessions:
   `pair_token = sign(user_id, bastion_session_id, issued_at, exp)`.
2. Register `/tunnel` sessions in `SessionRegistry` using that stable token and
   pass `session_rx: Some(...)` into `SessionConfig`.
3. Make `/api/ws/pair` validate signed pair tokens even when no app is currently
   registered; keep the websocket alive in a pending state.
4. Make the browser reconnect loop treat pending/retry as normal and continue
   audio playback.
5. Add CLI-mode forwarding for bastion sessions and emit the stable
   `LATE_SESSION_TOKEN=...` banner from the backend tunnel path.
6. Keep direct `late-ssh` behavior unchanged until `late-cli` routing through
   the bastion is decided.

This gets the important behavior without turning the bastion into a music-aware
service.

## Non-goals

- Perfect visualizer continuity across the restart gap. Dropping frames is fine.
- Restoring in-app TUI state. The bastion MVP explicitly accepts app reset.
- Keeping audio playback sample position synchronized with the TUI. The audio
  stream is live radio; the association is for controls and visualization.
- Moving Icecast/Liquidsoap stream ownership into `late-ssh`.

## Open Questions

- Should the stable pair token be deterministic from `X-Late-Session-Id`, or
  should `/tunnel` upsert a random token lease in the DB?
- How long should a token remain valid after the SSH channel disappears?
- Should `/api/ws/pair` allow multiple external clients for one SSH session, or
  keep the current replacement behavior?
- Should terminal mute/volume controls target the latest client only, or all
  paired clients?
- Should browser pairing get an explicit "terminal reconnecting" state, or is
  silent retry enough?
- Do we want native `late-cli` to keep the exec token handshake through bastion,
  or standardize on the shell banner path?

## Bottom Line

Yes, the bastion laid the missing foundation: a stable SSH-session identity that
survives `late-ssh` restarts. Music restoration should build on that by making
the pair token stable for the life of the bastion session and by allowing pair
websockets to wait through backend reconnect gaps.

The clean first version keeps the bastion boring: it continues to assert
identity and session id, while `late-ssh` and the public API own token validation,
pair routing, browser state, CLI state, and audio-client controls.
