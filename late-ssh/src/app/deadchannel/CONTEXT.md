# deadchannel Context (late-ssh/src/app/deadchannel)

## Metadata
- Domain: the deadchannel game (GAME.md): its onboarding, the
  first-contact haunting ladder, in the `haunt/` subdomain, and the
  start of the character layer, the runner and its look, in `runner/`
  (phase 2, build order step 1). Built for
  several replicas (root CONTEXT.md, multi-replica rule); gated behind
  the `haunt_live` fuse, unlit, so only staff (admins and moderators)
  are haunted today, and only they can finish the ladder and join.
- Last updated: 2026-09-03 (the runner: `/join #deadchannel` now creates
  a `deadchannel_runners` row wearing a random starter look (pieces and
  tints from the closed table in `runner/state.rs`), and inside
  #deadchannel every runner's portrait sits in a six-cell gutter on the
  right of their messages (hood on the separator row above the block,
  eyes on the header, coat on the first body row); looks cross replicas
  through the `deadchannel_runner_changed` notify into a process-shared
  directory, `runner/svc.rs`)
- Status: Active, staff only until `/haunt live on`
- Parent context: `../../../../CONTEXT.md`; design sources live in this
  directory: `GAME.md` (the game: thesis, first contact, the runner) and
  `DIGEST.md` (the feed budget and the welcome-back paper)

## 1. Summary

The game is never announced; it arrives. This domain will grow into the
whole character layer; what exists today is **first contact**: the
escalation ladder that onboards a person through haunting instead of a
tutorial. The chain is the spec, and the ladder never skips a rung
(counts tuned 2026-09-01): three clock bursts quiet the clock and open
stage 2, the third name hit arms the stage-3 whisper (it fires on the
next fresh connect, and once more on a later day: two doors, two
different lines), and the second delivered whisper schedules the stage-4
invitation. With the daily caps each of stages 1 and 2 spreads over two
or three days, and the two doors a day or more apart: the full ladder is
roughly a week and a half of slow burn.

Who is haunted (GAME.md, "the eligibility gate is a whisper campaign"):
**stage 1 is universal, stages 2-4 need the gate.** Stage 1 arms for
staff (admins and moderators) always and for everyone once the `haunt_live` fuse is lit (an
`app_flags` row, `/haunt live on|off`; unlit today, so nothing fires for
real users while copy and thresholds await design review). Stages 2-4
arm when the gate passes: at least `ACTIVE_MIN_HOURS` (168, seven days)
of lifetime connected time (`user_online_time.total_milliseconds`, the
online-time leaderboard's table, one primary-key read at bootstrap;
account age is not tenure, hours spent here are), at least
`TOUCHED_SETTINGS_MIN` (2) keys from the
closed `TOUCHED_SETTINGS_KEYS` list in late-core `user.rs`, and a bio of
at least `BIO_MIN_CHARS` (100) that the AI screen passed (the length is
only the floor under which no screen is spent; the screen decides
whether it reads as a person). The gate is
evaluated once at session bootstrap (`svc::bootstrap_gate`: the user row
that already loads plus the one online-time read, which fails closed to
zero hours) and is never stored: filling
your bio tonight means the static can find you tomorrow. The free legs
come first: the bio screen (a paid AI call) is only claimed once the
hours and the touched settings already pass. Eligibility
gates entering the funnel, never continuing it: any stage-2 hit on
record arms stages 2-4 whatever the bio later becomes. All three
thresholds are placeholders pending design review.

**Replica rule.** Nothing in this domain is a process-local source of
truth. The switches are rows served through one `watch` per replica
(`app/flags`). The daily and lifetime caps are enforced by conditional
claims on the user row (`User::claim_first_contact_glitch_burst`,
`claim_first_contact_name_hit`): a machine decides *when to ask*, holds
its schedule, and the beat shows on the tick the claim comes back won.
The whisper stamp and the invitation are claims. The bio screen is a
claim keyed on a hash of the bio text, so any number of sessions on any
number of replicas spend one AI call per text.

## 2. Module map

| File | Owns |
|---|---|
| `glyphs.rs` | `GLYPH_ALPHABET`, the game's shared character vocabulary. Game-level: the haunting borrows it, stage-4-era spawns will render with it (the clock glitch is retroactive foreshadowing). Distinct from the static shades `░▒▓` (noise, not creatures). |
| `haunt/state.rs` | The pure machines and data: `HauntState` (the one `App` slot), `FirstContactMarks` (persisted marks bundle), `FirstContactGate` + `BioStanding` + the thresholds and `bio_hash` (the eligibility gate), `ClockGlitch` (stage 1), `NameFlicker` (stage 2), `WhisperState` (stage 3), the voice/invitation constants (stage 4), `PendingClaim`/`HitStage` (claims in flight), `HauntCommand` + `parse_haunt_command`. No I/O, no clock reads. |
| `haunt/svc.rs` | Orchestration: `bootstrap_gate` (gate + bio screen claim at connect), `arm` (session start), one `tick(app)` (claim drain, splash door, glitch scheduler, name-flicker roller, invitation clock, `/haunt` drain), `note_splash_input`, `replay_whisper`, the bio screen task. The only haunting layer touching `App`, logging, metrics, and persistence. |
| `haunt/ui.rs` | Pure render helpers: whisper frame + splash overlay + static surge, `apply_clock_glitch`, `glitched_name`, `name_flicker_for`. Deterministic per burst seed, stateless like the sidebar equalizer. |
| `runner/state.rs` | The look: `PIECES` (the closed starter table, one five-cell row per piece, `Slot` hood/eyes/coat), `Tint` (the closed palette, gold deliberately absent), `Look` + `Worn` (typed, table references), `Look::random` (the join's dice), `Look::to_json` / `Look::parse` (the JSON contract on the runner row; unknown codes are a `LookError`, never a blank), `PORTRAIT_WIDTH` / `PORTRAIT_HEIGHT`. No I/O. `state_test` asserts every row is five single-width cells. |
| `runner/ui.rs` | `portrait_spans`: the look as three styled spans, one per worn piece in its tint; `tint_color` maps the palette onto the theme. Pure. |
| `runner/svc.rs` | `RunnerLookService`: the process-shared look directory (`watch<Arc<HashMap<Uuid, Look>>>`), seeded and refreshed from `deadchannel_runners` on the `deadchannel_runner_changed` LISTEN, the `app/flags` shape. A look that fails to parse is logged and skipped. `fixed_looks_rx` for test apps. |

Root integration is deliberately thin: `App.haunt` (the one field),
`haunt::svc::tick(self)` in `tick.rs` (plus the splash block consulting
`HauntState::holds_splash_door` before self-expiring), one input line
routing splash input, and three one-line draw calls in `render.rs`
(clock transform, whisper frame for `DrawContext`, splash overlay).
Chat's seams: the `/haunt` submit hook (admin-gated), the
`requested_haunt` slot, the `own_message_landed` slot set in
`push_message`, `name_flicker` threaded through the chat view structs
into the rows cache key, and
`ChatService::send_first_contact_invitation_task`. Outside the domain:
`app/flags/svc.rs` (the switches), `app/ai/screen.rs::screen_bio` (the
bio verdict), `ProfileService`'s first-contact tasks (the row claims),
and `metrics::record_first_contact_beat` / `record_first_contact_bio_screen`.

The runner's seams are as thin: `ChatService::join_deadchannel_room`
creates the row (`DeadchannelRunner::ensure_for_user`, a conditional
insert, so two devices joining at once share one face; a fresh row is
the `RunnerCreated` beat), `State.runner_looks` holds the directory
service (`main.rs` starts its listener), `App.runner_looks` is the
session's owned copy refreshed on the 1 Hz edge in `tick.rs` (bumping
`chat_ctx_epoch`, so the rows rebuild once per change), and chat's rows
builder takes `runner_looks: Option<&HashMap>`, `Some` only while the
rendered room is #deadchannel: every entry in the room wraps
`PORTRAIT_GUTTER` (6) cells short, and a block-opening message by a
runner gets `attach_portrait` (blank body rows added up to three, the
face right-aligned on the first three rows). Continuations and system
lines carry no face; every other room renders exactly as before.

## 3. The four stages (behavior contract)

1. **Clock glitch (deniable).** The sidebar clock (pinned core block,
   Home/Arcade: the most stable, most-glanced-at element) renders one or
   two time characters from the glyph alphabet for ~200ms
   (`GLITCH_HOLD_TICKS`, spanning the sidebar's ~132ms wake cadence),
   then heals. Scheduled per session with independent dice: roughly one
   burst per 40min-3h, at most `GLITCH_DAILY_CAP` (2) per UTC day,
   deferred a few minutes whenever the clock is off screen so a burst is
   never spent unseen. A due burst is a `GlitchTick::Due`: the service
   claims it on the row (`claim_first_contact_glitch_burst`, both caps
   enforced in the `UPDATE ... WHERE`), the machine holds its schedule,
   and the burst starts on the tick the claim comes back won; a capped
   answer re-dices and mirrors the row's count, a failed one defers a
   few minutes. At `GLITCH_TOTAL_CAP` (3) the clock goes quiet for good
   and stage 2 opens (the quiet is part of the escalation). Chrome,
   never content; timezone label untouched. Universal: armed for every
   session the fuse allows, gate or no gate.
2. **Name flicker (personal).** Only once stage 1 has spent its share
   (glitch hits at the cap): on the landing echo of this session's own
   send (the one moment of guaranteed attention), a ~1-in-24 roll may
   corrupt two or three characters of that message's author label for
   ~800ms, then a different two or three for ~800ms more (two waves,
   `NAME_WAVES`, each with its own seed), heavier and longer than the
   clock: name characters only,
   never the body (the escalation is targeting, not content). Only a
   send that renders its own author header is a target: the landing
   hook in `chat/state.rs` skips grouped continuations (a fast
   follow-up to your own message, `MESSAGE_GROUP_WINDOW_SECS`), whose
   label never draws, so a hit is never spent invisibly. A landed roll
   is a `NameRoll::Claim`: the service claims it on the row
   (`claim_first_contact_name_hit`, `NAME_DAILY_CAP` (1) per UTC day and
   `NAME_TOTAL_CAP` (3) ever, both in the `WHERE`), no other send rolls
   while the claim is out, and the label corrupts on the tick the claim
   comes back won. The corruption rides the chat rows cache key, so
   start and heal rebuild rows exactly once. Chosen only.
3. **Whisper (the held door).** Plays `WHISPER_TOTAL_CAP` (2) times per
   person, at least `WHISPER_GAP_HOURS` (24) apart, each from its own
   line pool: the first door says the static noticed you, the second
   that something is trying to get through. Arms at connect only when
   name hits have reached `NAME_TOTAL_CAP` and
   `FirstContactMarks::whisper_due` holds (under the cap, and the last
   delivery a day or more ago): the haunting follows you home, and comes
   back. The splash neither skips nor expires while held; input is
   acknowledged (static surge, skip-hint dissolve) but never obeyed; the
   voiced line types itself (in answer to the first keypress, or on its
   own); a hard cap (~10s) releases whatever the phase. Delivery claims
   one mark (`claim_first_contact_whisper`: increments
   `first_contact_whisper_hits` and stamps `first_contact_whisper_at`,
   conditional on the cap and the gap in the row, so two devices that
   both played leave one mark and the same evening never counts twice;
   the loser is logged, the one race the claim-on-delivery shape
   accepts, because claiming at arming would burn a whisper on every
   dropped SSH session). A kill-switch drop or lost session leaves the
   mark unspent.
4. **Invitation (the whole game is opt-in).** `INVITE_DELAY_DAYS` (2)
   after the second delivered whisper, the game's first voice - `afterglow`
   (GAME.md reserved the name for something inside the world), a
   bartender-shaped ghost user (fixed fingerprint `afterglow-fp-000`)
   that is never auto-joined into public rooms - sends one persistent DM.
   The voice row is ensured at process startup
   (`ChatService::ensure_first_contact_voice_task`, main.rs), the same
   reservation move as the `system` user: the first deploy creates the
   row and the case-insensitive unique username index holds the name
   from then on; the invitation task re-runs the ensure, so a lost boot
   or a squat self-heals. The DM is
   a plea ending in the only instruction the haunting ever gives,
   `/join #deadchannel`. Self-serve: the chosen one's own session notices
   the due date; `User::claim_first_contact_invitation` (a conditional
   settings stamp) keeps racing devices to exactly one DM. **Order is
   load-bearing:** the voice user and the DM room are ensured *before*
   the claim is taken (any failure there, say a squatted `afterglow`
   username, leaves the claim untaken so a later session retries; a
   racing loser's `User::create` heals by re-finding the winner's row by
   fingerprint), the claim guards only the send, and a failed send
   releases the claim (`release_first_contact_invitation`; losing the
   release too is logged as a burned claim). The `deadchannel` room slug
   is reserved in `normalize_topic_slug` (late-core `chat_room.rs`,
   beside the `lounge` reservation, case-insensitive, refusal message
   "only static on that channel") so no user-created room can be waiting
   where the invitation points. **The invitation is the key (decided
   and built 2026-09-01):** `ChatService::open_public_room` routes the
   `deadchannel` slug (case-insensitive) into `join_deadchannel_room`,
   which requires `first_contact_invited_at`; without the stamp the
   caller gets the same static line the reserved slug gives, so from
   outside the door and the wall are indistinguishable. An open door
   would let people skip the bio/settings/tenure eligibility funnel
   that the haunting exists to drive. The room has its own
   `kind='deadchannel'` (migration 170,
   `ChatRoom::get_or_create_deadchannel_room`, seeded on the first
   invited join, never auto-joined): every room listing is a kind
   whitelist (browse lists only `topic`, IRC lists
   lounge/language/topic, and IRC JOIN filters through
   `is_irc_channel_kind`), so the channel is hidden from browse and
   IRC by construction, the same way game rooms already are, without
   inheriting the game-room join path. Once joined it does show on the
   rail: the last room in Core, under `#voice`, above Discover, never in
   Channels (`chat/state.rs::is_deadchannel_room`, read by
   `visual_order_for_rooms` and both rail builders in `chat/ui.rs`).
   Copy and name face design review before real users ever see them.

## 4. Persistence (`users.settings`, late-core `User`; `app_flags`; `deadchannel_runners`)

- `deadchannel_runners` (migration 172, model
  `late-core/src/models/deadchannel_runner.rs`): one row per user
  (`user_id` unique, cascade on delete), `look` JSONB in the shape
  `{"hood": {"piece", "tint"}, "eyes": ..., "coat": ..., "mark": {"glyph"}}`.
  Created only by the invited join; phase 2 grows it column by column.
  Insert and update fire `deadchannel_runner_changed` (payload: the user
  id, for logs only; listeners re-read every look).

- `first_contact_glitch_hits` (int) + `first_contact_glitch_day`
  (YYYY-MM-DD) + `first_contact_glitch_day_hits` (int): stage-1 bursts.
  `claim_first_contact_glitch_burst` increments all three in one
  conditional `UPDATE` (lifetime under `total`, today's under `daily`,
  the day rolling in the same statement) and returns `Won { hits }` or
  `Capped { hits }`; `record_first_contact_glitch_hit` is the uncapped
  increment for forced (`/haunt glitch`) bursts only.
- `first_contact_name_hits` + `first_contact_name_day` +
  `first_contact_name_day_hits`: stage-2 hits, same shape
  (`claim_first_contact_name_hit`; `record_first_contact_name_hit` for
  forced hits). The third hit arms stage 3.
- `first_contact_whisper_hits` (int) and `first_contact_whisper_at`
  (RFC3339): stage-3 deliveries and the last one's time, written only by
  `claim_first_contact_whisper` (conditional on the cap and the gap);
  the counter at its cap schedules stage 4 from the stamp.
- `first_contact_bio` (object `{hash, verdict, at}`): the bio screen
  cache. `claim_first_contact_bio_screen` stamps `pending` for a hash
  when no verdict exists for it, or the one on record is not `passed`
  and is older than `BIO_RESCREEN_AFTER_HOURS` (24);
  `set_first_contact_bio_verdict` lands `passed`/`failed` only while
  that hash is still on record. Not a chain mark: `/haunt reset` leaves
  it alone (rewrite the bio to re-screen).
- `app_flags` rows `haunt_enabled` (kill switch) and `haunt_live`
  (fuse), migration 171, model `late-core/src/models/app_flag.rs`,
  served by `app/flags/svc.rs`.
- `first_contact_invited_at` (RFC3339): stage-4 claim, written only by
  `claim_first_contact_invitation` (conditional on absence), taken back
  by `release_first_contact_invitation` when the send after a won claim
  fails.
- `reset_first_contact` wipes the six chain keys and both stamps (the
  `/haunt reset` hook).
- Everything else is render-only and session-local: no chat rows, no IRC
  projection (the invitation DM is the deliberate exception: stage 4 is
  where the fiction goes real, and an invitation that vanishes cannot be
  followed three days later).

## 5. `/haunt` (admin composer command)

Parsed in `chat/state.rs::submit_composer` **only when `is_admin`**
(enum + parser live in `haunt/state.rs`); for everyone else, moderators
included, the line posts as plain text, exactly as if the command did
not exist. Admin-only on purpose while the ladder runs for staff: a
moderator who could type `/haunt` would know what the glitches were.
Drained by `haunt::svc::tick`.

- `/haunt` - status: kill switch, fuse, whether stage 1 and the chosen
  stages armed for this session, the gate's three legs (active hours,
  touched settings, bio length and standing), glitch schedule, glitch
  and name hit counters against their caps, door, whisper, invite.
- `/haunt on` / `/haunt off` - the kill switch, an `app_flags` row: the
  flip lands on every replica through the `app_flag_changed` notify and
  survives a restart. `on` also forces this session chosen and arms the
  repeatable machines, so the flip (and the gate) is testable without
  reconnecting or a passing bio; `off` drops a live whisper mid-scene.
- `/haunt live on` / `/haunt live off` - the fuse (`haunt_live`): lit,
  stage 1 arms for every connecting user, not only staff, and the gate
  decides who goes further. Takes effect from each user's next connect.
- `/haunt glitch` - fire a clock burst on a ~7s fuse (the banner covers
  the clock for ~5s), bypassing schedule and caps.
- `/haunt name` - force the next own send to flicker.
- `/haunt replay` - re-run the splash whisper now, ignoring the marks.
- `/haunt invite` - send the invitation DM now, skipping the delay.
- `/haunt reset` - wipe every mark; the chain starts over.

## 6. Gotchas

- The flags `watch` carries `None` until the listener's first load, and
  `None` reads as off everywhere: a session connecting in that window
  arms nothing, and an armed whisper would drop unspent. Fail closed on
  purpose; test apps get a pre-seeded receiver (`test_app_flags_rx`).
- A hit shows one tick after the claim wins, not on the tick the dice
  landed (one DB round trip). For the flicker that is still on the
  landing echo's ~800ms hold; the glitch never had a moment to miss.
- `screen_bio` fails closed at every step: AI off means `BioStanding::AiOff`
  (no claim, no pass, unless a pass is already on record, which is
  final); a broken call leaves the pending claim to expire rather than
  releasing it, so a flapping API costs at most one call per bio text
  per day.
- Clock domains differ on purpose: the whisper runs on `splash_ticks`
  (the splash's own typing clock), the glitch and flicker on
  `marquee_tick` (wall-derived 66ms units).
- Input swallowed by the held door leaves the VT parser mid-escape; both
  the input path and the release path call `vt_input.reset()`.
- Every voiced or corrupted character obeys the screenshot test (static /
  signal / city / channel vocabulary, never Unix internals); the whisper
  pool and the invitation plea need feed-template-grade variety before
  leaving staff scope (GAME.md, Open questions).
- The invitation runs through `ChatService::send_message`, so DM
  delivery, unread badges, and IRC projection behave like any DM.
- Test apps pass `FirstContactMarks::spent_for_tests()` and
  `FirstContactGate::closed_for_tests()` so no stage can fire in a test
  unless armed on purpose (`test_helpers` compiles unconditionally, so
  those helpers carry no `#[cfg(test)]`).
- `right_sidebar_visible` was made `pub(crate)` for the glitch's
  visibility gate; it still lives in `tick.rs`.
- The look directory starts empty and fills on the listener's first
  load, so a portrait can be absent for the first seconds after a
  replica boots; a session copies the directory on its next 1 Hz tick.
  The `mark` is stored from birth but not yet painted anywhere: the
  chat badge stack and the clubhouse floor glyph are the next slice.
- Where to look when the ladder seems dead (per person, in the logs, all
  keyed by `user_id`): `first contact gate evaluated` at every connect
  once the fuse is lit (for staff, always) with each leg's number, the
  bio standing, and the `GateVerdict`; `first contact gate shut` when
  haunting is off (info for staff, debug for everyone else); `first
  contact armed` for every session that can fire stage 1, with `chosen`
  and `whisper_armed`; then one line per hit, whisper, invitation, bio
  screen, and runner. How many the gate turns away, and on which leg, is
  `late_ssh_first_contact_gate_total{verdict, audience}` (one count per
  connect, not per person); bio screens by outcome are
  `late_ssh_first_contact_bio_screens_total`; delivered beats are
  `late_ssh_first_contact_beats_total`.
- Piece rows are five cells with no wide glyph; the state test guards
  that and nothing more. The rows are block, box-drawing, and shape
  glyphs (`◈ ◌ ●` and their kin), which are East Asian ambiguous width
  like the rest of the TUI's frames, so portraits assume the same
  ambiguous-narrow terminal the whole app does. Any new piece with a
  shape glyph should still be checked in the terminals people here use
  before it ships.
