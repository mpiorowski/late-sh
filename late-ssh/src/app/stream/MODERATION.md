# Stream moderation

How staff and the streamer stop a broadcast or keep someone out of a stream
room. The stream's shape, lifecycle and consent rules are in `CONTEXT.md`; the
general moderation surface is in `late-ssh/src/app/moderation/`.

## Staff tools

Every path below runs through `StreamService::stop`, which is what actually
kills a broadcast: registry teardown drops the watch and publisher URLs, and
`VoiceService::remove_stream_publisher` force-disconnects the console
(identity `stream-{user_id}`, invisible to a plain participant removal by
user id). `ModerationInfra` carries the `StreamService` for all of it.

- `/mod kick stream @user` ends the current broadcast and stops there.
  Nothing persists, so `/golive` works again immediately: this is the tool
  for the wrong window shared by accident, not for a repeat offender. CLI
  voice is untouched, so the streamer can keep talking in the room.
- `/mod ban stream @user [duration] [reason]` does the same and writes a
  `stream_bans` row (`late-core/src/models/stream_ban.rs`, one active row per
  user, expiry checked at read time, no sweeper). `go_live` refuses on it, so
  unlike the runtime-only voice block the ban survives a restart.
  `/mod unban stream @user` lifts it; `/mod view bans stream` lists them, and
  `/mod view @user` reports `stream_banned`.
- `/mod kick voice @user` remains the wider hammer: it blocks future voice
  tickets (`go_live` and `stream_publish_ticket` both check it), ends the
  stream, and cuts CLI voice too. Runtime-only, lifted by
  `/mod unban voice @user`. Reach for it when the problem is the person in
  the room, not the broadcast.
- `/mod kick server` and `/mod ban server` also stop the stream. Terminating
  SSH sessions is not enough on its own: the console lives in a browser, so
  without this a banned user keeps broadcasting to anonymous link-holders.
- `/mod` room tools (ban, kick-from-room, slow mode) work on the stream
  chat room like any other room, but they do not touch the media: the
  publisher's grant comes from the per-stream token, not room membership.

## The streamer's own tools

A streamer moderates their own room without staff. `/ban @user [duration]
[reason]` and `/unban @user`, typed in the stream room, run the same
`ModerationService` room action as the mod surface: same audit log, same
voice revoke, same live-session notify. Authorization is resolved in
`resolve_room_ownership` (`moderation/service.rs`), the one place room
actions are authorized, which grants `STREAM_OWNER` caps (kick + ban +
unban, `moderation/policy.rs`) when the room is `game_kind='stream'` and
`created_by` is the actor. Two deliberate narrowings there:

- **Ban, not just kick.** A stream room is public, so a kicked viewer walks
  back in from the rail. The ban is the only thing that holds; `/kick` is
  kept for the "settle down" case.
- **`created_by`, never the derived `ChatRoom::owner_id`.** That helper
  succeeds to the earliest remaining member, which on a public room would
  hand a passing viewer the streamer's powers. The check is also scoped to
  `game_kind='stream'` so a daily match's challenger does not inherit
  powers over their own opponent.

Ownership carries no rank (`Permissions::can` still compares tiers), so a
streamer cannot touch staff, and the grant is per-action, never standing.
Staff *decisions* are equally out of reach: when the cap came from ownership
rather than rank, an active ban placed by another actor refuses both
`/unban` and a re-`/ban` (`room_action` checks the existing row's
`actor_user_id`), so a streamer cannot lift a staff ban or overwrite a
permanent one with a softer one. An expired staff ban is history and does
not block a fresh streamer ban.

A room ban takes the microphone too: `ensure_user_can_join_voice`
(`voice/svc.rs`) refuses a banned user a voice ticket. Membership alone
could not carry this, since banning drops membership but a public room is
re-enterable. `ChatRoomMember::join` refuses banned users for every join
path, so the room door needs nothing extra.

Out of reach by design: anonymous `/live/{id}` watch pages carry no identity
to ban. They also cannot speak (`canPublish=false`), so a banned viewer can
lurk on the page but not make noise. Ending the stream is the only lever
against the anonymous audience.

A minted LiveKit token stays valid for an hour; the force-disconnect plus the
refusal on the next `/golive` is what makes any of these bite immediately.

## Tests

The staff ban's tests are listed in `CONTEXT.md` §5. Stream-owner moderation: `policy_test.rs` pins the `STREAM_OWNER`
  caps and that ownership never widens a moderator's reach;
  `chat/svc_test.rs` covers the streamer banning and unbanning a regular,
  a viewer holding nothing in someone else's room, staff being out of
  reach, a private room's owner *not* gaining the ban, a non-stream game
  room having no owner-moderator, a banned user failing to rejoin, a
  streamer being refused both lifting and overwriting an active staff ban
  (`a_streamer_cannot_lift_or_replace_a_staff_ban_on_their_room`), and a
  chat `/ban` landing on the room the actor sits in rather than a
  slug-namesake topic room (chat commands carry the room id);
  `voice/svc_test.rs::a_room_banned_user_is_refused_a_voice_ticket` pins
  the microphone half; `chat/state_internal_test.rs` covers the
  `/ban @user [duration] [reason]` parse (duration slot vs reason).
