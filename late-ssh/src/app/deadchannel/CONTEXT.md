# deadchannel Context (late-ssh/src/app/deadchannel)

## Metadata
- Domain: the deadchannel game (GAME.md) - today only its onboarding, the
  first-contact haunting ladder, in the `haunt/` subdomain. Admin-scoped
  scaffolding end to end.
- Last updated: 2026-08-31 (all four haunting stages built: clock glitch,
  own-name flicker, splash whisper, invitation DM; domain restructured as
  game root + `haunt/` subdomain; root files reduced to one field / one
  routing call each)
- Status: Active, admin-only by design
- Parent context: `../../../../CONTEXT.md`; design source: `GAME.md`,
  "First contact (the haunting)"

## 1. Summary

The game is never announced; it arrives. This domain will grow into the
whole character layer; what exists today is **first contact**: the
escalation ladder that onboards a person through haunting instead of a
tutorial. The chain is the spec: stage-2 hits arm the stage-3 whisper
(it fires on the next fresh connect), and the delivered whisper schedules
the stage-4 invitation. **While this is admin-scoped scaffolding nothing
ever fires for real users** (first contact is a nonrenewable resource);
the only gate today is `is_admin`, with the real eligibility campaign
(bio, settings, tenure) due at design review before the fuse is lit.

## 2. Module map

| File | Owns |
|---|---|
| `glyphs.rs` | `GLYPH_ALPHABET`, the game's shared character vocabulary. Game-level: the haunting borrows it, stage-4-era spawns will render with it (the clock glitch is retroactive foreshadowing). Distinct from the static shades `░▒▓` (noise, not creatures). |
| `haunt/state.rs` | The pure machines and data: `HauntState` (the one `App` slot), `FirstContactMarks` (persisted marks bundle), `ClockGlitch` (stage 1), `NameFlicker` (stage 2), `WhisperState` (stage 3), the voice/invitation constants (stage 4), `HauntCommand` + `parse_haunt_command`. No I/O, no clock reads. |
| `haunt/svc.rs` | Orchestration: `arm` (session start), one `tick(app)` (splash door, glitch scheduler, name-flicker roller, invitation clock, `/haunt` drain), `note_splash_input`, `replay_whisper`. The only haunting layer touching `App`, logging, and persistence. |
| `haunt/ui.rs` | Pure render helpers: whisper frame + splash overlay + static surge, `apply_clock_glitch`, `glitched_name`, `name_flicker_for`. Deterministic per burst seed, stateless like the sidebar equalizer. |

Root integration is deliberately thin: `App.haunt` (the one field),
`haunt::svc::tick(self)` in `tick.rs` (plus the splash block consulting
`HauntState::holds_splash_door` before self-expiring), one input line
routing splash input, and three one-line draw calls in `render.rs`
(clock transform, whisper frame for `DrawContext`, splash overlay).
Chat's seams: the `/haunt` submit hook (admin-gated), the
`requested_haunt` slot, the `own_message_landed` slot set in
`push_message`, `name_flicker` threaded through the chat view structs
into the rows cache key, and
`ChatService::send_first_contact_invitation_task`.

## 3. The four stages (behavior contract)

1. **Clock glitch (deniable).** The sidebar clock (pinned core block,
   Home/Arcade: the most stable, most-glanced-at element) renders one or
   two time characters from the glyph alphabet for ~200ms
   (`GLITCH_HOLD_TICKS`, spanning the sidebar's ~132ms wake cadence),
   then heals. Scheduled per session with independent dice: roughly one
   burst per 40min-3h, at most `GLITCH_DAILY_CAP` (2) per UTC day,
   deferred a few minutes whenever the clock is off screen so a burst is
   never spent unseen. Chrome, never content; timezone label untouched.
2. **Name flicker (personal).** On the landing echo of this session's own
   send (the one moment of guaranteed attention), a ~1-in-24 roll may
   corrupt that message's author label for ~300ms: name characters only,
   never the body (the escalation is targeting, not content). One hit per
   UTC day, `NAME_TOTAL_CAP` (3) ever; every hit increments the persisted
   `first_contact_name_hits` counter. The corruption rides the chat rows
   cache key, so start and heal rebuild rows exactly once.
3. **Whisper (the held door).** Arms at connect only when name hits > 0
   and `first_contact_whisper_at` is unset: the haunting follows you
   home. The splash neither skips nor expires while held; input is
   acknowledged (static surge, skip-hint dissolve) but never obeyed; the
   voiced line types itself (in answer to the first keypress, or on its
   own); a hard cap (~10s) releases whatever the phase. Delivery stamps
   `first_contact_whisper_at`; a kill-switch drop or lost session leaves
   the mark unspent.
4. **Invitation (the whole game is opt-in).** `INVITE_DELAY_DAYS` (2)
   after the delivered whisper, the game's first voice - `afterglow`
   (GAME.md reserved the name for something inside the world), a
   bartender-shaped ghost user (fixed fingerprint `afterglow-fp-000`)
   that is never auto-joined into public rooms - sends one persistent DM:
   a plea ending in the only instruction the haunting ever gives,
   `/join #deadchannel`. Self-serve: the chosen one's own session notices
   the due date; `User::claim_first_contact_invitation` (a conditional
   settings stamp) keeps racing devices to exactly one DM. Copy and name
   face design review before real users ever see them.

## 4. Persistence (`users.settings`, late-core `User`)

- `first_contact_name_hits` (int): stage-2 hits; arms stage 3, caps
  stage 2. `record_first_contact_name_hit` is a SQL increment.
- `first_contact_whisper_at` (RFC3339): stage-3 delivery; schedules
  stage 4.
- `first_contact_invited_at` (RFC3339): stage-4 claim, written only by
  `claim_first_contact_invitation` (conditional on absence).
- `reset_first_contact` wipes all three (the `/haunt reset` hook).
- Everything else is render-only and session-local: no chat rows, no IRC
  projection (the invitation DM is the deliberate exception: stage 4 is
  where the fiction goes real, and an invitation that vanishes cannot be
  followed three days later).

## 5. `/haunt` (admin composer command)

Parsed in `chat/state.rs::submit_composer` **only when `is_admin`**
(enum + parser live in `haunt/state.rs`); for everyone else the line
posts as plain text, exactly as if the command did not exist. Drained by
`haunt::svc::tick`.

- `/haunt` - status: kill switch, glitch schedule, name hits, door,
  whisper, invite.
- `/haunt on` / `/haunt off` - the process-global kill switch
  (`State.haunt_enabled`, in-memory, back on after restart, safe while
  admin-scoped). `on` also re-arms the repeatable machines for a session
  that connected while it was off; `off` drops a live whisper mid-scene.
- `/haunt glitch` - fire a clock burst now (bypasses schedule and caps).
- `/haunt name` - force the next own send to flicker.
- `/haunt replay` - re-run the splash whisper now, ignoring the marks.
- `/haunt invite` - send the invitation DM now, skipping the delay.
- `/haunt reset` - wipe all three marks; the chain starts over.

## 6. Gotchas

- Clock domains differ on purpose: the whisper runs on `splash_ticks`
  (the splash's own typing clock), the glitch and flicker on
  `marquee_tick` (wall-derived 66ms units).
- Input swallowed by the held door leaves the VT parser mid-escape; both
  the input path and the release path call `vt_input.reset()`.
- Every voiced or corrupted character obeys the screenshot test (static /
  signal / city / channel vocabulary, never Unix internals); the whisper
  pool and the invitation plea need feed-template-grade variety before
  leaving admin scope (GAME.md, Open questions).
- The invitation runs through `ChatService::send_message`, so DM
  delivery, unread badges, and IRC projection behave like any DM.
- Test apps pass `FirstContactMarks::spent_for_tests()` so no stage can
  fire in a test unless armed on purpose (`test_helpers` compiles
  unconditionally, so that helper carries no `#[cfg(test)]`).
- `right_sidebar_visible` was made `pub(crate)` for the glitch's
  visibility gate; it still lives in `tick.rs`.
