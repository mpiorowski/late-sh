# Drain handoff

A `service-ssh` rollout runs the new pod beside the old one, and the old pod
keeps its sessions for up to 6 h (`termination_grace_period_seconds`) behind
the "Server updating!" banner. The handoff moves a quiet session whose `late`
CLI runs in native mode to the new pod on its own: the CLI redials, the
music never stops, and the session comes back on the page it was on.
Everyone else (plain `ssh`, OpenSSH and subprocess CLI modes, CLIs from
before this change) keeps the banner and the manual reconnect.

## Flow

1. **The old pod decides** (`svc::tick`, every world tick). Only a draining
   pod (`App::is_draining`, set by the shutdown signal) looks further; there
   `HandoffCheck` is ready when the account's Tweaks row "Auto restart on
   update" is on (`users.settings.auto_restart`, default on), and
   `last_active_at` is at least `HANDOFF_IDLE` (5 min) old. That clock moves only on input from a
   person, never on a mouse move or a terminal probe reply. A voice room, an
   open watch modal, or text in the chat composer each hold the session in
   place: things a person does without typing. Active sessions are checked
   again every tick, so they move once they go quiet.
2. **It asks the CLI.** A ready session sends
   `PairControlMessage::Reconnect { resume }` to the paired CLIs advertising
   `reconnect` (`PairedClientRegistry::send_control_to_reconnect_cli`). Once
   one took it, `HandoffState::sent` stops the tick for good: one handoff per
   session, and a session that stays afterwards keeps the banner.
3. **The CLI redials** (`late-cli/src/main.rs`, `run_native_session`). It
   dials and authenticates against the new pod while the old session is still
   on screen (the terminating pod has left the Service endpoints, so the dial
   cannot land on it). Only once the new connection holds a session token
   does it detach stdin, retire the old shell (its output stops reaching the
   terminal, its channel closes), and open the new shell with
   `LATE_RESUME=<resume>` set before the pty request. The old pair socket
   stays up through the dial, so a dial that fails leaves the user on the old
   session, still paired. The audio runtime belongs to the process, so
   playback runs straight through; pairing restarts on the new token with
   `PairRetryPolicy::after_handoff`, which never releases the boot mute.
4. **The new pod resumes** (`ssh.rs` `env_request`, then `App::new`).
   `ResumePoint::parse` reads the env into `SessionConfig::resume`. A resumed
   session publishes no login event (no "joined" in #lounge, no friend
   banner, nothing on the Nightcap TV), shows no splash and no paper pop, lands on the resume page
   (`ResumePage::landing`, then `svc::resume` switches pages through
   `set_screen`, as a digit on the first frame would, so the Artboard consent
   and door gates still apply) and hands chat the selected room
   (`ChatState::resume_room`). The first `sync_selection` with a room list
   takes that hint and picks it only when it is a chat-list room of this
   user. A session with the tour still to walk ignores the resume and starts
   in the Clubhouse.

## The resume point

`v1:<page>` or `v1:<page>:<room id>`, written and read only by servers; the
CLI carries it back verbatim. Anything else (another version's format, a
hand-set `LATE_RESUME`) parses to `None` and the session starts fresh. Pages
are the eight numbered pages and Zen. Every other screen resumes on the page
it was reached from: doors and the Realm on the Games hub, the Nightcap and
the undercity on the Clubhouse, daily matches, house tables and the
scratchpad on Home. What was open on the page (modals, scroll positions,
arcade runs, house seats, door connections) does not survive.

## Telemetry

`late_ssh_drain_handoffs_total{stage}`: `sent` on the old pod, `resumed` and
`unparsed` on the new one; `sent` minus `resumed` is the handoffs that never
landed. Logs: "asked an idle cli session to move to the new pod" (old pod),
"resuming a session moved off a draining pod" (new pod), and the CLI's
"drain handoff: ..." lines in its own log.

## Gaps

- The request rides the session's pair websocket on the old pod. A session
  whose socket dropped during the drain (`SCALE.md`, Pain Point 10) cannot
  hear it and stays on the banner.
- Voice, video and a composer draft block the handoff instead of moving
  with it.

## Multi-replica

Nothing shared: `is_draining` is the process's own lifecycle, `sent` is one
session's flag, and the resume point travels through the client to whichever
pod the CLI reaches.

## Tests

`state_test.rs` pins the wire format, the page mapping and the check;
`svc_test.rs` drives an app through the send, the landing and the room hint
against a disposable database. The CLI side is in `late-cli/src/ws_test.rs`.
