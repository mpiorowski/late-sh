# deadchannel Context (late-ssh/src/app/deadchannel)

## Metadata
- Domain: the deadchannel game (GAME.md): its onboarding, the
  first-contact haunting ladder, in the `haunt/` subdomain, and the
  start of the character layer, the runner and its look, in `runner/`
  (phase 2, build order step 1), and the night city street in `city/`
  (the wallet, GAME.md "The three surfaces"; art and walkable street
  first, under the clubhouse on a second `0`, runners only, no
  transactions yet). Built for
  several replicas (root CONTEXT.md, multi-replica rule); gated behind
  the `haunt_live` fuse, unlit, so only staff (admins and moderators)
  are haunted today, and only they can finish the ladder and join.
- Last updated: 2026-09-19 (the city moved under the clubhouse: `0` on the clubhouse goes down, runners only; the tile register won and the drawn one left the live script; the street went long, with walkers and running; the renderer lights the street from every lamp and sign, fades it with distance and shadows the walls, in the city's own fixed palette; §3b). Before that, 2026-09-18 (the night city exists: the generated street, the runner as its mark, landmarks with popovers, shop panels showing catalogs without tills). Before that, 2026-09-14 (the breakthrough plays only on a send from
  its own session and swallows keys ahead of door games; the static rolls
  slower, scene lengths unchanged). Before that, 2026-09-13 (stage 3
  plays on its own clock: the static
  pulses and the line types from the first frames, and every key, Esc
  included, is swallowed; the invitation no longer arrives cold: once due,
  the next own send plays the breakthrough, a full-screen static tear with
  a line naming afterglow, and the DM lands as the line finishes). Before
  that, 2026-09-09 (pacing retuned so the whole ladder fits a
  week of daily connects, with every haunt kept: the first clock burst
  of a session comes 5-20 min in and later ones 20-60 min apart, the
  name roll is 1-in-3 (the daily cap does the spacing), the whisper gap
  and the invitation delay are both 20 hours so an evening-to-evening
  connect never slips a day, and the tenure leg of the gate is 8 hours
  instead of 168. Before that, 2026-09-05: stage 2 is no longer private: a name hit now
  travels to every session in the room, on every replica, over the
  `deadchannel_name_hit` notify on chat's message listener, and each
  witness replays it once the message is on their screen. Before that,
  2026-09-04: every deadchannel log line now carries
  `username` beside `user_id`, and Grafana has a "deadchannel" row over
  the haunt logs and the three first-contact counters; the runner:
  `/join #deadchannel` now creates a `deadchannel_runners` row wearing a
  random starter look (pieces and
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
(counts tuned 2026-09-01, pacing 2026-09-09): three clock bursts quiet
the clock and open stage 2, the third name hit arms the stage-3 whisper
(it fires on the next fresh connect, and once more on a later day: two
doors, two different lines), and 20 hours after the second delivered
whisper the next own send plays the stage-4 breakthrough, which carries
the invitation DM in. The daily caps, not the dice, do the pacing, and
the ladder is built for a person who connects once a day: day 1 two
bursts, day 2 the third burst and the first name hit, days 3 and 4 the
other two, day 5 the first door, day 6 the second, day 7 the breakthrough
and the DM. The
day-scale gaps (whisper gap, invitation delay) are 20 hours rather than
24 so evening-to-evening connects at different times never slip a day.

Who is haunted (GAME.md, "the eligibility gate is a whisper campaign"):
**stage 1 is universal, stages 2-4 need the gate.** Stage 1 arms for
staff (admins and moderators) always and for everyone once the `haunt_live` fuse is lit (an
`app_flags` row, `/haunt live on|off`; unlit today, so nothing fires for
real users while copy and thresholds await design review). Stages 2-4
arm when the gate passes: at least `ACTIVE_MIN_HOURS` (8) of lifetime
connected time (`user_online_time.total_milliseconds`, the
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
| `haunt/state.rs` | The pure machines and data: `HauntState` (the one `App` slot), `FirstContactMarks` (persisted marks bundle), `FirstContactGate` + `BioStanding` + the thresholds and `bio_hash` (the eligibility gate), `ClockGlitch` (stage 1), `NameFlicker` (stage 2, the person being haunted), `ActiveHit` (one hit's playback, holding the wave seed: the roller's own hit and the `witness` slot share it, so every screen corrupts identically), `WhisperState` (stage 3), `Breakthrough` + `InvitationClaim` (stage 4's full-screen beat and the claim answer), the voice/invitation/breakthrough-line constants (stage 4), `PendingClaim`/`HitStage` (claims in flight), `HauntCommand` + `parse_haunt_command`. No I/O, no clock reads. |
| `haunt/svc.rs` | Orchestration: `bootstrap_gate` (gate + bio screen claim at connect), `arm` (session start), one `tick(app)` (claim drain, splash door, glitch scheduler, name-flicker roller, witness replay, breakthrough (claim answer, scene, due send), `/haunt` drain), `swallows_splash_input`, `replay_whisper`, the bio screen task, and `publish_name_hit` (a won or forced hit goes on the wire through `ChatService::publish_name_hit`). The only haunting layer touching `App`, logging, metrics, and persistence. |
| `haunt/ui.rs` | Pure render helpers: whisper frame + splash overlay + static surge, breakthrough frame + full-screen draw, `apply_clock_glitch`, `glitched_name`, `name_flicker_for`. Deterministic per burst seed, stateless like the sidebar equalizer. |
| `runner/state.rs` | The look: `PIECES` (the closed starter table, one five-cell row per piece, `Slot` hood/eyes/coat), `Tint` (the closed palette, gold deliberately absent), `Look` + `Worn` (typed, table references), `Look::random` (the join's dice), `Look::to_json` / `Look::parse` (the JSON contract on the runner row; unknown codes are a `LookError`, never a blank), `PORTRAIT_WIDTH` / `PORTRAIT_HEIGHT`. No I/O. `state_test` asserts every row is five single-width cells. |
| `runner/ui.rs` | `portrait_spans`: the look as three styled spans, one per worn piece in its tint; `tint_color` maps the palette onto the theme. Pure. |
| `runner/svc.rs` | `RunnerLookService`: the process-shared look directory (`watch<Arc<HashMap<Uuid, Look>>>`), seeded and refreshed from `deadchannel_runners` on the `deadchannel_runner_changed` LISTEN, the `app/flags` shape. A look that fails to parse is logged and skipped. `fixed_looks_rx` for test apps. |
| `city/map.rs` | **Generated** by `scripts/gen_city_map.py --write` (never hand-edited): the 232x52 `MAP` literal, the `SOLID` collision bitmap, `SPAWN`, every zone (`SIGNS`, `BANNERS`, `CART_SIGNS`, `AWNINGS`, `WINDOWS`, `VENTS`, `PUDDLES`, `LAMPS`, `DROP_LIGHTS`, `SCREEN_FACE`, `WIRE`, ...), the closed `Neon` palette, `Landmark` + `nearest_landmark` (reach zones), `walkable`, `grid`/`char_at`. |
| `city/state.rs` | Per-session view state: the runner's cell, the animation clock, the open panel, the pinned street line. `walk`, `nearby`, `Landmark::on_enter` (`Enter::Panel` for shops, `Enter::Line` for carts and the screen, `Enter::Leave` for the wire). Pure. |
| `city/data.rs` | The city's copy and catalogs: the gear ladder (`COST_LADDER`, `WEAPONS`, `ARMOR`: LoGD numbers, GAME.md names), `BANDS` with draft move names, `NOTICES`, `DRINKS`, `TAILOR_PRICES`, the per-landmark `lines` pools, `title` and `pitch`. |
| `city/input.rs` | Arrows/hjkl walk; Enter at a landmark; Esc or Enter closes a panel (walk keys are swallowed while one is open). Returns `false` for globals. |
| `city/ui.rs` | Renderer: base styling by zone, the ambience pass (rain, puddles reflecting the nearest sign, neon shorts and dropped letters, window flicker, the screen's static and test pattern with rare glyph frames, steam, lamps, the drop's lights, the blimp, the mast, the bits machine, the wire's pulse), the runner as its mark, the popover, the street line, the shop panels. |

Root integration is deliberately thin: `App.haunt` (the one field),
`haunt::svc::tick(self)` in `tick.rs` (plus the splash block consulting
`HauntState::holds_splash_door` before self-expiring), two input lines
(splash input to the held door, and a swallow while the breakthrough
plays, first thing in `App::handle_input` so no door passthrough sees
the keys), `HauntState::breakthrough_playing` keeping `wake_hint` hot, and
the draw hooks in `render.rs` (clock transform, whisper and breakthrough
frames for `DrawContext`, splash overlay, the breakthrough painted last
over every modal).
Chat's seams: the `/haunt` submit hook (admin-gated), the
`requested_haunt` slot, the `own_send_succeeded` flag (a `SendSucceeded`
for a request this session submitted: the breakthrough's trigger), the
`own_message_landed` slot set in
`push_message` (the message id *and* its room, since the won hit is put
on the wire for that room a tick later), the stage-2 wire itself
(`ChatService::publish_name_hit` does the `pg_notify`; chat's message
listener, the one that already carries gild markers, turns the notify
into `ChatEvent::NameHit`; `note_name_hit` hands the beat over through
the `witnessed_hit_landed` slot as soon as the message is on screen,
holding it in `pending_name_hits` until `push_message` lands the message
if a replica's delta is behind), `name_flicker` threaded through the chat
view structs into the rows cache key (unchanged: the row builder corrupts
whichever message id it is handed, so witnessing cost the chat renderer
nothing), and `ChatService::send_first_contact_invitation_task` (answers
the claim on a oneshot, then sends the DM after a delay). Outside the domain:
`app/flags/svc.rs` (the switches), `app/ai/screen.rs::screen_bio` (the
bio verdict), `ProfileService`'s first-contact tasks (the row claims),
`late-core`'s `models/deadchannel_name_hit.rs` (the wire's channel,
payload, and parse), and `metrics::record_first_contact_beat` /
`record_first_contact_bio_screen`.

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
   then heals. Scheduled per session with independent dice: the first
   burst 5-20 min after connect (so an hour-long evening sees one), the
   next 20-60 min later, at most `GLITCH_DAILY_CAP` (2) per UTC day,
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
2. **Name flicker (personal, witnessed).** Only once stage 1 has spent its share
   (glitch hits at the cap): on the landing echo of this session's own
   send (the one moment of guaranteed attention), a 1-in-3 roll may
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
   **The room watches (2026-09-05).** The hit is no longer private to the
   session that rolled it: a won (or forced) hit goes onto the
   `deadchannel_name_hit` wire and every session in that room replays it
   on the same name. Rolling, capping, and claiming are unchanged and
   still belong to one session; a witness decides nothing, it replays.
   Three properties carry it: the *seed* rides the wire, so every screen
   swaps the same characters in the same two waves (`ActiveHit` is the
   one playback both sides use; the witness's copy sits in
   `HauntState.witness`); the beat *waits for its message*, which only
   matters across replicas (on the sender's own replica the chat
   broadcast lands the message before the claim is even out), so chat
   holds a beat whose message the room delta has not brought yet and
   hands it over as the message lands; and past `NAME_HIT_WAIT` (30s,
   chat's constant) it is **dropped rather than played late**, so
   somebody opening the room a minute afterwards sees a clean name. The
   person being haunted declines their own copy off the wire by
   recognising their live hit (a second device of theirs holds no live
   hit and witnesses it normally), and the audience is exactly stage 1's:
   a beat is only painted while the kill switch is on, and while the fuse
   is unlit only staff are in the audience, so nothing of the haunting
   reaches a real user before `/haunt live on`.
3. **Whisper (the held door).** Plays `WHISPER_TOTAL_CAP` (2) times per
   person, at least `WHISPER_GAP_HOURS` (20) apart, each from its own
   line pool: the first door says the static noticed you, the second
   that something is trying to get through. Arms at connect only when
   name hits have reached `NAME_TOTAL_CAP` and
   `FirstContactMarks::whisper_due` holds (under the cap, and the last
   delivery a day or more ago): the haunting follows you home, and comes
   back. The splash neither skips nor expires while held,
   and since 2026-09-13 the scene waits on nobody: the static pulses on its
   own rhythm (~1.3s, each noise pattern held ~130ms) from the first
   frame, the voiced line types itself once
   the base splash line is done (`VOICE_TICK`), the skip hint dissolves as
   it starts, and every key, Esc included, is swallowed and does nothing
   (without a keypress the old scene was a quiet line under the cup, and
   people were missing the door). A hard cap (~10s) releases whatever the
   phase. Delivery claims
   one mark (`claim_first_contact_whisper`: increments
   `first_contact_whisper_hits` and stamps `first_contact_whisper_at`,
   conditional on the cap and the gap in the row, so two devices that
   both played leave one mark and the same evening never counts twice;
   the loser is logged, the one race the claim-on-delivery shape
   accepts, because claiming at arming would burn a whisper on every
   dropped SSH session). A kill-switch drop or lost session leaves the
   mark unspent.
4. **Breakthrough, then the invitation (the whole game is opt-in).**
   `INVITE_DELAY_HOURS` (20) after the second delivered whisper the
   breakthrough comes due (`FirstContactMarks::breakthrough_due`), and the
   next send this session submits plays it (added 2026-09-13: the DM
   alone, met cold, was taken for spam). Only a `SendSucceeded` for a
   request this session submitted counts, never the same person's send
   from another device: every session of a user hears every send, and an
   idle one would play the scene to nobody. The send asks
   `ChatService::send_first_contact_invitation_task` for the once-ever
   claim; the task answers `Won`, `Taken`, or `Failed` on a oneshot before
   it sends anything, the scene plays only on `Won` (a `Taken` stamps the
   marks; a `Failed` is logged by the task under
   `first_contact_invitation_failed`, and this session stops asking until
   `/haunt invite`), and the task sends the DM after
   `Breakthrough::dm_delay`, the moment the line has typed, whether or not
   the session is still there. The scene is private and full screen: the
   door's static, heavier and pulsing quicker, over the whole frame and
   every modal, `BREAKTHROUGH_LINE` typing in a gap torn out of the middle,
   about seven seconds, every key swallowed ahead of door passthrough and
   the parser, the kill switch cuts it. Known gap to close before the fuse
   is lit: the claim is stamped `dm_delay` before the DM sends, so a
   replica stopping in that window leaves a stamp with no DM, and nothing
   repairs it but `/haunt reset` by hand. The DM comes from the game's first voice - `afterglow`
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

## 3b. The night city (the undercity under `0`, the wallet; art first)

GAME.md, "The three surfaces": the city is a full-screen destination
where nothing happens that you could miss; transactions only. What exists
is the street and its doors, none of the tills: the design pass that fixes
the art before a single purchase is wired.

- **Where it is reached.** Under the clubhouse: `0` lands on the
  clubhouse, `0` again on the clubhouse goes down to the undercity, `0`
  on the undercity comes back up. Runners only (`city::input::allowed`:
  a look in `App.runner_looks` for this user, so a `deadchannel_runners`
  row); anyone else stays on the clubhouse. Not in the Tab cycle
  (`Screen::City.next()`/`prev()` return the clubhouse), no tab of its
  own, the clubhouse tab stays lit under it, title "Undercity". Enter at
  the wire goes back up to the clubhouse. The wiring is thin on purpose
  (`Screen::City`, `App.city`, one dispatch line each in `input.rs`,
  `render.rs`, `tick.rs`).
- **The register (decided 2026-09-19): tiles.** Top-down, one tile per
  thing, the Dwarf Fortress register. `#` walls, `+` doors, `╬` windows
  that flicker, `=` counters, `)` blades, `[` plate, `"` marks, `!`
  bottles, `%` bowls, `∩` lockers, `▬` beds, `▪` crates, `▓` shutters and
  cabinets, `≈` water, `@` people, `c` cats, `r` rats, `>` stairs, `*`
  lamps, `°` lanterns, `≡` grates that steam. The first pass drew the
  street front-on with multi-cell facades; it read as a plaza and lives
  only in `scripts/city_versions/v2_tiles_gen_city_map.py` (`--style
  drawn` there). A 440-column street in three legs with a dogleg between
  each (the camera scrolls), four tiles wide, alleys one to three wide
  running off it (dead ends, a hidden court with a shrine and a plant, a
  back lane behind the second leg reached from its two end alleys and the
  arcade's back door), rooms you walk into, stalls of five tiles against
  the walls, a canal under the first two legs (a walkway, two bridges,
  warehouses on the far bank), the ledge with the railing and the wire
  stairs (`>` in the gap, the spawn) along the third leg, a small yard
  and the screen as three tiles of static closing the street. The shops
  are the landmarks; everything else is there to be there: tenements
  (`tenement()` lays out corridor, rooms, beds and a sleeper from a
  seed), a lockup, a shuttered pawn shop, a clinic, the baths, a motel,
  the arcade, a shrine, a market hall, a garage, a dock, a chop shop, a
  video store, an aerial lot. **Walkers** (`map::WALKERS`,
  `ui::walkers`): people, cats and rats pacing a stretch of floor as a
  pure function of the tick, no state; the generator and `map_test`
  prove every path is open floor.
- **Light (2026-09-19).** Blade Runner, not cyberpunk: the street is
  dark and every color has a source. `map::LIGHTS` is every light on the
  street, found by the generator scanning the finished grid (a `*` is a
  lamp, a `°` a lantern, `$` `♪` `?` machines, `_` candles, `>` stairs)
  plus the signs, the shops' doorways, the windows and the screen, each
  with a kind, a `Neon` and a radius. Each frame `ui::Scene` spreads
  every light over the floor it reaches (Dial's buckets: a column costs
  one, a row two, light crosses open ground and doorways, lands on walls
  and stops) at the level `light_level` gives it this tick (lamps
  flicker, signs short out and their light with them, windows go dark
  for a while, the screen pulses with its static), and adds the
  spinner's searchlight passing over. A **visibility map** fades
  everything with distance from the runner (`SEE_FULL` columns in full,
  black-ish by `SEE_END`) and keeps a room at `INSIDE_DARK` until the
  runner is at its door. Every cell is a `Surface`: lit (its color times
  ambient plus the light on it, wet things more, halved in the shadow
  under a wall) or emissive (neon, lamps, windows: they burn on their
  own and only fade with distance). Height is faked: the top wall of a
  building against the dark draws as `▀`, the floor south of any wall is
  in shadow. Rain shows only where there is light to see it by, in that
  light's color. The runner carries a light (`CARRY_RADIUS`): where you
  stand is always the best lit place on the street. Light crosses flat
  water, so the canal and the puddles take the bank's lanterns. Every
  sign smears its color a few rows into the wet ground in front of it
  (`reflections`, shimmering). The fixed lights' footprints are computed
  once (`footprints`), as is every cell's base surface (`base_map`) and
  which room it is in (`inside_map`): a debug frame is ~30ms, not 130.
- **Traffic, billboards, splashes.** All pure in the tick. A car runs the
  length of the street (`car_route`: the three legs through both
  doglegs, `ui_test` proves every cell is open) as two cells, tail red,
  head white, with the pool of its headlights running ahead of it in the
  light map; it draws only on open floor, so it passes behind whatever
  is in the road. The monorail crosses the sky row (`TRACK_Y`, a dashed
  track across the top of the map), eastbound one run and westbound the
  next, windows amber and cyan, flickering. `map::BILLBOARDS` are nine
  two-row zones the generator leaves blank (five over the rooftops in
  the sky rows, four on the towers below the ledge); the renderer writes
  the city's script on them, one text per `BILLBOARD_CYCLE`, dark for a
  moment between, a letter flickering; each is a `LightKind::Billboard`
  in the light map. Raindrops landing on a puddle throw an `o`.
- **The ledge (`Landmark::Ledge`, `city/ledge.rs`).** Standing at the
  railing anywhere along it (one row north of `RAIL_Y`, not on the wire
  stairs) the popover says "look over"; Enter swaps the street for the
  lower city, all the way down: a perspective picture at half-block
  resolution (two colors per cell, `▀` with fg and bg), towers in four
  depths from the far hazed ones on the horizon to the near black ones
  standing below the frame, windows lit at random and flickering, the
  city's script in neon bands across the near towers, antenna lights
  blinking, the spinner's beam crossing, rain in the sky. Pure in the
  area's size and the tick; the same size always draws the same city.
  Enter or Esc steps back. `State::at_ledge` gates the walk keys like a
  panel does.
- **Own palette, not the theme.** The city does not follow the person's
  theme at all: one look, tuned once (`ui::NIGHT` painted under every
  cell and overlay, `ui::neon_rgb`, the surface constants, the `INK_*`
  greys of the overlay text, all fixed RGB). A hundred palettes cannot
  all be lit well, and a light canvas showing through the street breaks
  the night. Nothing in `city/ui.rs` reads the theme module; the mirror
  in the tailor's panel recolors the tints through `ui::tint_rgb`.
- **The runner** is its mark (GAME.md, "The look"; `@` for a session
  without a runner row), name label above. Single-width glyphs
  only; the generator refuses wide and combining characters and
  `map_test` asserts it again.
- **Landmarks.** Enter at a shop (armorer, tailor, lockers, bands, bar,
  patch, board, bits machine) opens a centered panel with its catalog and
  a line saying the till is not open: the armorer lists all fifteen
  tiers with bits prices, the tailor shows your portrait in the mirror
  and the whole starter rack (every piece row, the ten marks, the five
  tints) plus the placeholder chip prices, bands shows the three bands
  with draft move names. Enter at a cart, the screen, or the stairs pins
  a line from that landmark's pool top-left for ~8s. Enter at the wire
  leaves (to Home today; to #deadchannel once the page moves).
- **Animation** rides the clubhouse's `anim_half` edge (~7.5fps,
  `tick.rs`), the wake tier is `ANIM_HALF_TICK` on this screen, and every
  effect is a pure function of `marquee_tick` and the cell, so nothing
  accumulates. The ambience never paints over a prop (`put_if_floor`, and
  `ui_test` checks the signs and the board survive a frame).
- **The camera looks north** (`LOOK_NORTH`, 8 rows): it centers above the
  runner so a 24-row terminal at the spawn shows shopfronts and street,
  not the drop.
- **Nothing is persisted, nothing is shared.** No lobby, no crowd, no DB:
  the city is one runner on one street per session by design (transactions
  only; presence would make standing here beat standing in chat).

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
- The stage-2 wire has **no table on purpose**
  (`late-core/src/models/deadchannel_name_hit.rs`): a name hit is a second
  and a half of theater whose mark is already a conditional claim on the
  user row, so there is no truth to store. The channel carries a
  self-contained payload
  (`<message>:<room>:<user>:<seed>`), the publisher's pooled
  connection is not the listener's (so Postgres hands the beat back to the
  publishing replica too, and every session hears it exactly one way), and
  a replica that boots mid-beat misses it like a person who was not
  looking. Nothing to sweep, nothing to migrate.
- Everything else is render-only and session-local: no chat rows, no IRC
  projection (the invitation DM is the deliberate exception: stage 4 is
  where the fiction goes real, and an invitation that vanishes cannot be
  followed three days later). A witnessed beat is render-only too: the
  message body, the row, and the IRC projection are all untouched, so the
  corruption exists only on the screens that were looking.

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
  and name hit counters against their caps, the witness (whether a beat
  of somebody else's is on this screen), door, whisper, and the
  breakthrough or invite.
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
- `/haunt name` - force the next own send to flicker. Skips the row's
  caps, not the wire: the forced beat travels like a real one, which is
  how the public half of stage 2 is watched from a second session.
- `/haunt replay` - re-run the splash whisper now, ignoring the marks.
- `/haunt invite` - the next own send breaks through, skipping the delay;
  the DM follows exactly as for a real one.
- `/haunt reset` - wipe every mark; the chain starts over.

## 6. Gotchas

- The flags `watch` carries `None` until the listener's first load, and
  `None` reads as off everywhere: a session connecting in that window
  arms nothing, and an armed whisper would drop unspent. Fail closed on
  purpose; test apps get a pre-seeded receiver (`test_app_flags_rx`).
- A hit shows one tick after the claim wins, not on the tick the dice
  landed (one DB round trip). For the flicker that is still on the
  landing echo's ~800ms hold; the glitch never had a moment to miss.
- On this replica the room sees a hit within a tick of the person being
  haunted: the chat broadcast (`ChatEvent::MessageCreated`) lands the
  message in every local session before the sender's claim is even out.
  Only a session on *another* replica learns of the message from the chat
  snapshot refresh (`CHAT_REFRESH_INTERVAL`, 10s), so there the beat is
  heard first and the name corrupts as the message *arrives*, then
  heals. That is why chat holds the beat rather than the haunting
  painting on receipt, and why `NAME_HIT_WAIT` (30s) has to stay
  comfortably above that cadence. Do not reach for a shared clock to make
  the waves simultaneous; it would cost persistence and buy nothing the
  fiction wants.
- A witnessed beat is spent when it starts, and "started" only means the
  message is in this session's copy of the room, whether or not that room
  is on screen. A witness who is off in the arcade spends the beat
  without seeing it. That is the same bargain the own-flicker makes (it
  fires on the landing echo and trusts the eye is there) and cheaper than
  teaching the haunting what is on screen; if it turns out to matter, the
  fix is a visibility gate like the clock glitch's, not a queue.
- If stage 2 looks dead from a second session, check that the session
  holds the room (a beat for a room it is not in is dropped on receipt),
  that chat's message listener is running (tests and headless paths do
  not start it, so there stage 2 stays private to the session that rolled
  it), and that the message landed inside `NAME_HIT_WAIT`.
- The wire is a delivery path, never a source of truth, so it is one of
  the few pieces of deadchannel state that is process-local by design.
  The caps it could be tempted to enforce already live in the row claim.
- `screen_bio` fails closed at every step: AI off means `BioStanding::AiOff`
  (no claim, no pass, unless a pass is already on record, which is
  final); a broken call leaves the pending claim to expire rather than
  releasing it, so a flapping API costs at most one call per bio text
  per day.
- Clock domains differ on purpose: the whisper runs on `splash_ticks`
  (the splash's own typing clock), the glitch and flicker on
  `marquee_tick` (wall-derived 66ms units).
- Input swallowed by the held door leaves the VT parser mid-escape; both
  the input path and the release path call `vt_input.reset()`. Input
  swallowed by the breakthrough is dropped before the feed, so it needs no
  reset.
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
  keyed by `user_id` and `username`, the two fields every deadchannel line
  carries): `first contact gate evaluated` at every connect
  once the fuse is lit (for staff, always) with each leg's number, the
  bio standing, and the `GateVerdict`; `first contact gate shut` when
  haunting is off (info for staff, debug for everyone else); `first
  contact armed` for every session that can fire stage 1, with `chosen`
  and `whisper_armed`; then one line per hit, whisper, breakthrough, invitation, bio
  screen, and runner. How many the gate turns away, and on which leg, is
  `late_ssh_first_contact_gate_total{verdict, audience}` (one count per
  connect, not per person); bio screens by outcome are
  `late_ssh_first_contact_bio_screens_total`; delivered beats are
  `late_ssh_first_contact_beats_total`. The name is a log field only, never
  a metric label: the three counters stay keyed on closed enums so the
  series count cannot grow with the player base. Grafana's "deadchannel"
  row (`monitoring/dashboards/observability.json`) reads both: the beat and
  gate counters as reset-safe `max_over_time` sums, and from the log
  lines a Runners table (one row per person: ladder hits, invited and
  joined times, last seen, latest gate verdict and legs), a Recent Beats
  table, and the Haunt Log, all three filtered by the `$runner` regex
  variable. The log tables use the `instant` query type and extract
  fields, because a `stats` query splits every group into its own series
  and cannot carry strings; logs keep 7 days, so older rungs show empty
  there and `users.settings` stays the truth. Counters live on the pod, so a
  deploy zeroes the live value; every panel there sums per-instance
  high-water marks instead. The row ships to prod with the dashboard
  ConfigMap, on a `-infra` release (`infra/monitoring.tf`), not on merge.
- The city map is generated: hand edits to `city/map.rs` are clobbered by
  the next `scripts/gen_city_map.py --write`. Move a prop in the script
  and its zone, reach, and animation cells move with it. The literal
  arrays carry `#[rustfmt::skip]`, so `cargo fmt` and `--write` agree.
- The city's copy (`city/data.rs`: shop lines, cart lines, draft move
  names, notices) is placeholder content at feed-template quality
  standards and faces design review with the rest of the phase 2 copy.
- Piece rows are five cells with no wide glyph; the state test guards
  that and nothing more. The rows are block, box-drawing, and shape
  glyphs (`◈ ◌ ●` and their kin), which are East Asian ambiguous width
  like the rest of the TUI's frames, so portraits assume the same
  ambiguous-narrow terminal the whole app does. Any new piece with a
  shape glyph should still be checked in the terminals people here use
  before it ships.
