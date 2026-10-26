# Drain handoff

A `service-ssh` rollout runs the new pod beside the old one, and the old pod
keeps its sessions for up to 6 h (`termination_grace_period_seconds`) behind
the "Server updating!" banner. The handoff moves a quiet session whose `late`
CLI runs in native mode to the new pod on its own: the CLI redials, the
radio never stops, and the session comes back on the page it was on.
Everyone else (plain `ssh`, OpenSSH and subprocess CLI modes, CLIs from
before this change) keeps the banner and the manual reconnect.

## Flow

1. **The old pod decides** (`svc::tick`, every world tick). Only a draining
   pod (`App::is_draining`, set by the shutdown signal) looks further; there
   `HandoffCheck` is ready when the account's Tweaks row "Auto restart on
   update" is on (`users.settings.auto_restart`, default on), and
   `last_active_at` is at least `HANDOFF_IDLE` (5 min) old. That clock moves
   only on input from a person, never on a mouse move or a terminal probe
   reply. A voice room, an open watch modal, or unsent text in the chat or
   News composer (`ChatState::has_unsent_draft`) each hold the session in
   place: things a person does without typing. Active sessions are checked
   again every tick, so they move once they go quiet.
2. **It asks the CLI.** A ready session sends
   `PairControlMessage::Reconnect { resume }` to the paired CLIs advertising
   `reconnect` (`PairedClientRegistry::send_control_to_reconnect_cli`). Once
   one took it, `HandoffState::sent` stops the tick for good: one handoff per
   session, and a session that stays afterwards keeps the banner.
3. **The CLI redials** (`late-cli/src/main.rs`, `run_native_session`). With
   the old session still on screen and paired, it dials and authenticates
   against the new pod (the terminating pod has left the Service endpoints,
   so the dial cannot land on it) and opens the new shell there with
   `LATE_RESUME=<resume>` set before the pty request, its output held. A
   failure at any of those steps, a new pod that cannot build the session
   included, leaves the user on the old session, still paired. Only then
   does it detach stdin, retire the old shell (its output stops reaching the
   terminal, its channel closes), show the new one (the held first frame
   paints first) and attach stdin to it. The audio runtime belongs to the
   process, so the radio plays straight through; the YouTube helper belongs
   to the pairing and restarts on the new token (see Gaps). Pairing restarts
   with `PairRetryPolicy::after_handoff`, which never releases the boot
   mute.
4. **The new pod resumes** (`ssh.rs` `env_request`, then `pty_request` and
   `App::new`). `ResumePoint::parse` reads the env into
   `SessionConfig::resume`. A resumed session publishes no login event (no
   "joined" in #lounge, no friend banner, nothing on the Nightcap TV), shows
   no splash and no paper pop, lands on the resume page
   (`ResumePoint::landing`, then `svc::resume` switches pages through
   `set_screen`, as a digit on the first frame would, so the Artboard consent
   and door gates still apply) and hands chat the selected room
   (`ChatState::resume_room`). The first `sync_selection` with a room list
   takes that hint and picks it only when it is a chat-list room of this
   user. A session with the tour still to walk ignores the resume and starts
   in the Clubhouse.

## The resume point

`v1:<page>` or `v1:<page>:<room id>`, written and read only by servers; the
CLI carries it back verbatim. Anything else (another version's format, a
hand-set `LATE_RESUME`) parses to `None` and the session starts fresh. The
page is `Screen::page` of the screen the session was on
(`app/common/primitives.rs`, the same rule as the tab cycle and the lit tab):
one of the eight numbered pages or Zen, a screen off the cycle on the page it
is reached from. What was open on the page (modals, scroll positions, arcade
runs, house seats, door connections) does not survive.

Nothing signs it, so it is a hint from the client, not proof of a handoff:
any authenticated client can send a well-formed one and come in without a
login event, counted as `resumed`. That is all it buys. The room is checked
against the user's own rooms, and the page goes through `set_screen` and its
gates like a key press.

## Telemetry

`late_ssh_drain_handoffs_total{stage}`: `sent` on the old pod, `unparsed` on
the new one when the env arrives, `resumed` there once `pty_request` has
built the session; `sent` minus `resumed` is the handoffs that never landed,
those whose new session failed to start included. Logs: "asked an idle cli session to move to the new pod" (old pod),
"resuming a session moved off a draining pod" (new pod), and the CLI's
"drain handoff: ..." lines in its own log.

## Gaps

- The request rides the session's pair websocket on the old pod. A session
  whose socket dropped during the drain (`SCALE.md`, Pain Point 10) cannot
  hear it and stays on the banner.
- Voice, video and an unsent draft block the handoff instead of moving
  with it.
- YouTube does not play straight through. The helper (`late-webview`) is
  paired to the old session's token, so the move stops it and the new
  pairing starts another: a few seconds of silence, then it seeks to the
  server's position.

## Multi-replica

Nothing shared: `is_draining` is the process's own lifecycle, `sent` is one
session's flag, and the resume point travels through the client to whichever
pod the CLI reaches.

## Tests

`state_test.rs` pins the wire format and the check (the page mapping is
`common/primitives_test.rs`);
`svc_test.rs` drives an app through the send, the draft hold, the landing
and the room hint against a disposable database. The CLI side is in `late-cli/src/ws_test.rs`.
