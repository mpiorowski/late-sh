# Clubhouse Context (late-ssh/src/app/clubhouse)

## Metadata
- Domain: the Late Lounge tavern, top-level screen `0`, the default landing screen (always the landing for a first-ever session)
- Status: Active

## 1. Summary

A full-bleed walkable ASCII tavern rendered over the whole content area with a
one-line #lounge composer pinned to the bottom. The crowd is real: every
logged-in human on every replica holds a seat they picked themselves,
walkers carry live positions every session renders (all through presence,
§3), and fresh #lounge messages float
over their authors' heads as speech bubbles. There is no chat panel here; the
room is the chat surface, and the full history lives in #lounge on Home.

## 2. Module map

| File | Owns |
|---|---|
| `map.rs` | The 184x50 generated floor plan (`MAP` literal, do not hand-edit; re-run `scripts/gen_clubhouse_map.py --write`), collision (`walkable`), `SEATS`/`STANDING_SPOTS`/`DOOR_STACK`, interactive zones (`BACK_DOOR` out to Nightcap among them), animation cell lists, `DOOR_SIGN`. **The generator's `RUST_TEMPLATE` has drifted behind this file** — it predates `DOOR_STACK`, `BOT_SPOT`, `DOG_HOME`/`DOG_WAYPOINTS`, `BAR_APPROACH` and the `dog` parameter on `nearest_interactive`, so a bare `--write` silently *reverts* all of them. Until it is resynced: author the art in the script, run it to validate, and splice only the `MAP` and `SEATS` blocks into this file. |
| `crowd.rs` | Pure: the room derived from presence records (`crowd`: one `Patron` per user, contested spots settled, the door stack, emotes, the last pet), `pick_spot` / `first_stand` (where a session sits), `dog_at` (the dog as a function of the wall clock). |
| `drunk.rs` | `DrunkMap`, the process's mirror of `user_drinks` (seeded by the ghost task, bumped on every local pour). |
| `state.rs` | Per-session state: this session's own stand (its part of its presence record), `settle` (follow a newer stand on another device, pick again after losing a spot or when a seat frees), the derived `Crowd`, camera target, animation clock, arrival/departure door events, the `Tutorial` state machine. |
| `input.rs` | Walking (arrows/hjkl), `i` composer, `w`/`x` emotes, `t` bartender mention, `n` out back to Nightcap (the avatar first steps onto `map::BACK_DOOR_MAT`, in its published stand too, so the room sees them leave by the door), Enter on landmarks/dog/the back door, tutorial Enter. Returns `false` for globals. |
| `ui.rs` | Renderer: camera pan, base-grid styling, animations, crowd placement, emote frames, speech bubbles, door ambience, tutorial overlays, prop popovers, composer footer, and any chat overlay that lands here (a `/summary` or reaction list requested on Home; it owns input via `screen_composes_chat`, so it must be drawn). |
| `nightcap/` | Nightcap, the small bar out back (`n` from the tavern or Enter at the back door, `map::BACK_DOOR`, past the end of the counter; Esc back): its own `Screen::Nightcap`, the stools from presence (`stools.rs`), `SharedWall`, and `CONTEXT.md`. A sub-slice, not a sibling domain. The generator script carries the door art (`back_door`), but `RUST_TEMPLATE` still lacks the `BACK_DOOR` zone and the `Interactive::BackDoor` arm, same drift as the rest. |

## 3. The crowd (multiplayer contract)

- The room is derived from presence (`app/presence`): one record per
  logged-in session on every replica, carrying its `ClubhouseStand` (the
  spot it picked or the cell it walked to, when, its last emote, its last
  pet). Every replica holding the same records draws the same room
  (`crowd::crowd`). No process-global seat map: presence is replica-clean
  (root CONTEXT.md §0, §7).
- **Everyone who logs in is in the room**, whether or not they ever open
  it: the session sits itself down at login (`State::new`, `first_stand`),
  from the records presence already has. Bots have no session, so no
  presence; they are found in `active_users` for the online flags only.
  The headcount in the frame title is the crowd's.
- **Nobody hands out seats.** A session picks its own spot
  (`crowd::pick_spot`: a random free seat, drawn per session, else the first
  free standing spot, else the door stack, `map::DOOR_STACK` slots and
  `+N at the door` past that) and publishes it. Two sessions that pick the
  same spot in the same breath are settled in `crowd`: the earlier
  `since_ms` keeps it (then the user id), the other is drawn at the door.
  `State::settle`, on every new set of records, picks again for a session
  that lost its spot, and for one at the door once a seat or standing spot
  frees, so the door drains into the room.
- One user on two devices is one patron: the latest mover's stand. A new
  session of a user already in the room joins them where they are
  (`first_stand`), and a session follows a newer stand from its other
  device (`settle`).
- The first movement key turns a parked patron into a *walker* (even into a
  wall): the seat frees for everyone on the next flush. `s` sits a walker in
  the nearest free seat within reach. Walkers keep walking until logout.
- Cadence: every tick `App::sync_presence` copies new records into the
  crowd (`set_records`) and publishes this session's stand when it changed;
  on the screen the crowd is redrawn every tick (`refresh_crowd`) for the
  clock-driven parts. Own moves redraw at once, laid over the records
  (`crowd::Own`), without waiting for the round trip.
- Emotes (`w` wave, `x` dance) and dog pets are stamps on the stand, played
  for their wall-clock windows (`EMOTE_MS`, `DOG_PET_MS`) by every session
  on every replica.
- **The dog** is a pure function of the wall clock (`crowd::dog_at`): cycle
  `k` trots from waypoint `k` to waypoint `k + 1` along the shortest open
  path (found once, `dog_paths`), then naps there. Nothing to sync, and it
  does not stop for walkers or pets; a pet sets its tail going.
- **Drunk glow:** `DrunkMap` (`drunk.rs`) mirrors `user_drinks` (raw
  `drunk_points` + `last_drink_at`). `Patron.drunk_level` (0 sober .. 4
  wasted, decayed at read time against wall clock via
  `late_core::models::drinks`, so a drinker sobers up while logged out)
  drives the walker's wobble and the passed-out figure here, and the printed
  `(word)` beside the name on chat author labels. `GhostService` seeds the
  map from DB every 60s on every replica (`run_drunk_glow_task`) and a pour
  bumps it at once on the replica that poured; another replica catches up on
  its next seed. The same map feeds chat author label tinting everywhere via
  `App.drunk_levels` (copied ~1/s in `App::tick`). It is not pruned by
  presence, so recent drinkers who logged out keep tinting their chat
  history until they decay.
- **Name flair:** a bought Name Glow/Gradient/Shimmer, 24h or 30-day tier (see
  `hub/CONTEXT.md`) paints the name-label foreground per character
  (`put_label_styled` in `ui.rs`, flair from `ClubhouseView.name_flair` =
  `App.name_flair`, resolved ~1/s in `App::tick` from the process-shared
  flair directory). Composes with the drunk bg tint; does not touch the
  avatar glyph or presence.
  A rented title rides the same entry and trails the name on the floor
  (`clubhouse_label`, `mira, the night clerk`). The floor is a crowded
  character grid, so the title is truncated to `LABEL_MAX` (10) the way the
  name already is: a label is at most a name plus a title, never wider.
  `put_label_styled` takes the name's character count so only that prefix
  takes the color effect; the title stays in the dim label style, as in
  chat. The click box tracks the whole drawn label.

## 4. Chat: bubbles, not a panel

- The old embedded `#lounge` chat panel is gone. `ui::draw` splits the area
  into the tavern plus the shared `ComposerBlockView` footer (same block the
  dashboard card uses; grows while typing, shows placeholder hints idle).
- `i` (or Enter in the open) composes into #lounge through the normal global
  composer pipeline; image paste works (Clubhouse is a
  `is_chat_composer_context` screen in `app::input`). Slash commands are
  off here (`ComposerCommands::Disabled`): a draft whose first word leads
  with `/` (`is_command_draft`) gets a banner, stays in the composer, and is
  neither run nor posted. A bare `/` or a `//` aside is speech and posts.
- Messages younger than ~10s render as bordered bubbles above their author's
  avatar (latest per author, up to 3 lines, width widens 28 -> 36 -> 44
  before truncating, reply-quote line stripped). Room tails are newest-first
  (`ChatState::push_message`); `fresh_bubble_messages` depends on that.
- The bartender does not bubble over his sprite: his lines pin as a
  camera-independent banner in the top-left corner (`draw_bartender_banner`),
  so they never collide with patron bubbles at the bar and are visible from
  across the room. When several patrons ask him at once his answers queue
  (`State::update_bartender_banner`, fed each on-screen tick from
  `App::tick_clubhouse`): each line holds ~6s while more wait, ~14s solo;
  lines older than 15s never enqueue and the queue caps at 8, oldest
  dropped. Graybeard bubbles normally.
  `App.clubhouse_bartender_id`/`clubhouse_graybeard_id`/`clubhouse_bot_id`
  are captured from `active_users` during roster refresh.
- `@bot` stands at `map::BOT_SPOT`, a fixed aisle cell between the arcade
  cabinet and the poker table (not a `Seat`: no furniture glyph there, he
  just stands, same rendering as the bartender's stick figure). He bubbles
  normally like graybeard rather than using the pinned banner, so a bubble
  only appears when he is the author of a fresh #lounge message; replies he
  posts to other rooms (any room he is mentioned in, per `ai/ghost.rs`) do
  not surface here since `lounge_messages` is #lounge-only.
- **Drinks cost chips:** `@bartender` mentions (from anywhere, but usually
  `t` at the bar) run an ungrounded, schema-enforced JSON decision in `ai/ghost.rs`
  (`pour`/`offer`/`chat`): the prompt carries the patron's live balance and
  spendable amount (balance minus the 100-chip floor), the model prices the
  drink 100-1000 chips, and the server refuses any out-of-range or unaffordable
  price (served uncharged, so the debit always matches the quoted line),
  floor-guards, and debits via
  `ChipService::buy_drink` (atomic with the `user_drinks` buzz upsert;
  ledger reason `drink_purchase`, source_ref = drink name). Unaffordable or
  chatty mentions charge nothing. The exact phrase `@bartender buy @user a drink`
  instead leaves one 200-chip credit on that person's tab, even while they
  are offline, without pouring the buyer. The recipient claims it on their
  own order; see `app/chat/CONTEXT.md` §9d. The tutorial greeting stays free.
- Message selection/reactions/scroll do not exist on this screen; Home owns
  them. The lounge is still pinned as the visible chat room for read cursors
  (`sync_visible_chat_room`).

## 5. First-visit tour

- Armed by `!extract_clubhouse_tutorial_done(user.settings)`
  (`users.settings.clubhouse_tutorial_done`, late-core). Fires once on the
  first clubhouse entry: a centered box at the door pitches what late.sh is
  (`Tutorial::Welcome`), then the tour walks every top-level page in number
  order with two Enter interludes for the features that have no page of
  their own: `VisitChat` (1) -> `VisitMusic` (Enter, still on Home: the
  sources, the Music Booth, and the two ways to actually hear sound:
  late.sh/listen or the `late` CLI) -> `VisitArcade` (2) -> `VisitLobby`
  (Enter, still on The Arcade: the Ctrl+G daily duels and live tables) ->
  `VisitGames` (3) -> `VisitArtboard` (4) -> `VisitDirectory` (5) ->
  `VisitLeaderboard` (6) -> `VisitZen` (`Ctrl+F`: Zen has no digit, so the
  stop teaches the chord and the main tile keys) -> `Homecoming` (0, back in
  the tavern). Each stop
  draws a centered pitch box over the real page (`ui::draw_tour_overlay`,
  called from `render.rs`) ending in the next key; `Homecoming`'s Enter
  finishes the tour in place and frees input: the player stays in the
  tavern. Hold the line at these two interludes and the Zen stop: pages are
  self-evidencing, and every extra forced stop taxes all future newcomers.
- **The tour is forced.** While `State::tutorial_forced_step` is `Some`,
  `handle_tour_gate` in `app/input.rs` (sitting above the reserved chords,
  below the quit-confirm modal) swallows every input, mouse and chords
  included, except the named digit (which runs `set_screen`; the stage
  advances in `State::tutorial_screen_entered`, hooked there), `Ctrl+F` at
  the Zen stop (`TourStep::Zen`, which runs the real `toggle_zen_globally`
  so the page opens exactly as it does everywhere; Enter runs the same
  toggle there, because the gate also blocks the `/zen` fallback and a
  terminal that swallows the chord would otherwise trap the newcomer), Enter where
  the box names it (the two interludes advance via `tutorial_advance`
  without persisting; only the homecoming Enter finishes and persists),
  and `q` (quitting always works; Esc's lone-byte path can still arm the
  quit confirm). There is no skip. Completion persists once via
  `ProfileService::set_clubhouse_tutorial_done` (fire-and-forget, failure
  only logged: worst case the tour runs again next session).
- **The hidden treasure:** the bartender is deliberately absent from the
  route. His scripted welcome (`ghost::bartender_tutorial_greeting`, local
  banner only, never posted to #lounge) plus the comped welcome pour fire
  the first time the newcomer walks up to the counter
  (`State::welcome_pour_due`); since walking is gated until the homecoming
  Enter, in practice that is after the send-off. The homecoming box ends
  with a whispered pointer at it, and the bar sign pulses until the pour is
  claimed (`State::bar_glow`). The once-ever guarantee is
  `UserDrinks::record_welcome_pour`, an insert-only comp that returns
  `None` for anyone who has ever drunk, so tour reruns after a mid-tour
  disconnect can't double-comp.
- The Ctrl+O profile nudge lives here on purpose: the old
  "open settings on connect" behavior was removed in favor of this beat.

## 6. Gotchas

- Single-width glyphs only in the art and effects (no emoji-class chars).
- `MAP` is generated; hand-edits get clobbered by `gen_clubhouse_map.py`.
  New furniture/zones go into the generator, then re-sync the hand-written
  constants (`SEATS`, zones, `DOOR_STACK`, test probes) from its output.
- Every presence stamp is wall-clock unix ms (`presence::svc::now_ms`), so
  stamps from different replicas compare; the pure `crowd.rs` and `state.rs`
  take `now_ms` as an argument, and tests pass fixed times and records.
- `walkable` allows standing ON the counter but never behind it; the flood
  fill tests in `map.rs` guard the bartender alley seal and seat
  reachability. `DOOR_STACK` slots must stay walkable.
- The tavern draws no widget chrome; headcount and key hints live in
  `app_frame_title` (`render.rs`). Update that line when keys change.
