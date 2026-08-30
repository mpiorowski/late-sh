# A Dark Room door (`late-ssh/src/app/door/darkroom`)

A native, in-process door game: a terminal port of Michael Townsend's
minimalist incremental. Single-player, DB-persisted (one save per user). It
uses the **Green Dragon integration pattern** (native ratatui + a service + a
`DoorGame` impl), not the nethack/rebels PTY-proxy pattern, because upstream is
a browser game with no terminal to proxy.

This is the first game on late.sh's **incremental shelf**: not a run you die
out of (NetHack, Brogue, DCSS) and not a daily-turn RPG (Green Dragon,
Usurper), but a save that grows. See root `DOOR.md` for why that shape was
wanted.

## Upstream source of truth

Everything mechanical is transcribed from the open-source **web version**:

- **`doublespeakgames/adarkroom`**: <https://github.com/doublespeakgames/adarkroom>.
  Key files ported: `script/room.js` (fire, temperature, the builder arc,
  craftables), `script/outside.js` (gathering, traps, population, workers,
  income table), `script/state_manager.js` (`collectIncome`, the store model).

A local clone lives at **`upstream-adarkroom/`** in the repo root (gitignored;
re-fetch with `git clone --depth 1 https://github.com/doublespeakgames/adarkroom upstream-adarkroom`).
Verify against those files directly, never from memory.

**Not** the source: the iOS/Android port (Amir Rajan, RubyMotion), the Steam
release, and the prequel *The Ensign* are separate closed products.

## Licensing (read before touching any file here)

Upstream is **MPL-2.0** with the Exhibit B "Incompatible With Secondary
Licenses" notice. MPL is **file-level** copyleft with no network clause, so the
arrangement is:

| Files | License | Why |
|---|---|---|
| `data.rs`, `model.rs`, `sim.rs`, `event.rs`, `world.rs`, `world_data.rs`, `space.rs`, `scenes_village.rs`, `scenes_encounters.rs`, `scenes_setpieces.rs`, `scenes_executioner.rs` | **MPL-2.0** (header + Exhibit B on each) | Carry upstream balance tables, timing constants, rules, scene graphs and prose. Text is copied verbatim, which the MPL permits precisely because these files stay MPL. |
| `pace.rs`, `persist.rs`, `svc.rs`, `state.rs`, `ui.rs`, `ui_event.rs`, `ui_world.rs`, `screen.rs` | FSL-1.1-MIT (repo default) | Our own work: the pacing design, persistence, the TUI. |

MPL §3.3 is what lets the larger work ship under our terms. **If you move
upstream-derived logic into one of the FSL files, you have broken this**; move
the file under MPL instead, and update `NOTICE` + `LICENSING.md`.

MPL §2.3 grants **no trademark rights**. The door currently ships under
upstream's title via `data::TITLE`, which is deliberately a single constant:
renaming is a one-line change if the author would rather we did not use it.
Michael Townsend was emailed about the name before the port shipped.

## The pacing model (`pace.rs`): the one real design change

Upstream has **no offline progress at all**: every timer is wall clock while
the browser tab is open, and the whole arc is 2-4 hours. Ported faithfully that
would mean "idle in an SSH session", which is the opposite of what a clubhouse
door should be. Four rules reshape it, and they are the only intentional
deviation from upstream behavior:

1. **Credit accrues while the SSH session is connected**, not while this screen
   is focused. The village grows while the player is in the lounge; it does not
   grow while they are logged out. `State::new` is handed the session's connect
   time (from `App::started_at`), and `sim::settle` never credits back past it.
2. **The village runs at `SLOWDOWN` (5×) slower.** Applied to worker income and
   population growth **only**. Cooldowns, the fire, and room temperature stay at
   upstream speed: the opening act is a click loop, and stretching it produces
   dead air rather than a longer game.
3. **`DAILY_CREDIT_SECS` (3h) of village time per UTC day.** Without this,
   "the session must be connected" is not pacing, it just rewards whoever parks
   a terminal on a spare monitor.
4. **`DAILY_CREDIT_FLOOR_SECS` (45m) on the first settle of each UTC day, once
   the village stands.** Rules 1 and 3 alone spread the arc from ~a week (for
   whoever idles into the cap) to months (for a fifteen-minute daily visit).
   The floor compresses that spread: a short daily check-in banks at least 45
   minutes, landing as a visible welcome-back burst in the log. It counts
   against the same daily cap, so idlers gain nothing from it, and it does not
   apply before the first hut: fast-forwarding a bare room would burn allowance
   on a world where nothing moves.

The footer always shows the remaining allowance, and says so plainly when it is
spent. **A village that has stopped growing must never look like a bug.**

### Leaving and coming back (what the floor actually means)

`settle` credits from `floor = max(session_start, last_settled)`, and leaving
the door stamps `last_settled` before dropping the state. Three consequences,
none of them obvious from the code:

- **Leaving the door while staying connected banks time.** Re-entering twenty
  minutes later credits all twenty at once. Sitting in the door and leaving it
  are worth exactly the same, which is the whole point: nobody should have to
  park on this screen.
- **Disconnecting does not.** A new session's `session_start` wins over the
  older `last_settled`, so the logged-out gap is worth nothing.
- **Connected-but-outside-the-door time is lost if you disconnect before
  returning**, because the new `session_start` jumps over it. Known and
  accepted; fixing it would mean persisting a per-session connect time.

## The clock (`sim.rs`): there are no timers

Upstream runs on live `setTimeout`/`setInterval` handles funneled through
`Engine.setTimeout`. This port has none. `sim::settle` advances the game to
`now` on demand: on load, on every action, and on leave. Elapsed time inside a
live session *is* connected time, so advancing is a subtraction, and gaps
between sessions contribute nothing without any bookkeeping.

- Stepping is **one second at a time** because the clocks interact (fire cools
  → room cools → the stranger stops progressing). Bounded by the daily cap, so
  a settle is at most 10,800 cheap iterations. Do not "optimize" this into a
  closed form without handling those interactions.
- **Two clocks, deliberately different.** Village time is credited, capped and
  slowed. Cooldowns run on plain wall clock, uncapped and unslowed, so a pacing
  rule can never leave a button stuck.
- Fractional income (a hunter earns 0.5 fur) lands in `Game::carry`, which
  holds the remainder in `[0, 1)`; whole units move into `stores`.
- Nothing here runs on the render loop.

## Module map (flat)

| File | Owns |
|---|---|
| `data.rs` | **MPL.** Closed `Resource`/`Building`/`Job`/`Fire`/`Temperature` sets, build costs (with the per-unit escalation on traps and huts), the income yield table, the trap drop table, every timing constant, and the notification prose, including the `MSG_*` action lines `state.rs` prints (they live here, not in the FSL file, precisely because they are upstream's sentences). `TITLE` is the door's display name, in one place. |
| `pace.rs` | The pacing layer: `Pace` (the persisted daily credit counter and floor latch), `SLOWDOWN`, `DAILY_CREDIT_SECS`, `DAILY_CREDIT_FLOOR_SECS`, `slowed()`. The only module that knows the port is paced differently from upstream (`sim::settle` passes the floor in, zeroed until the village stands, because pace deliberately knows nothing about the game). |
| `model.rs` | **MPL.** The persistent `Game` (stores, carry, buildings, workers, population, the latched `seen_buildings`/`seen_jobs`, the room, every countdown) and the rules on it: `light_fire`/`stoke_fire` (whole-refusal on short wood, the first fire is free), `build` (whole-refusal, and refused outright while the room is Cold or worse: upstream's "builder just shivers"), `gather_wood`, trap collection, worker assignment, `refresh_build_options` (upstream's half-the-wood-and-seen-the-rest unlock rule, latched), plus the display helpers (`outside_title` hut ladder, `trap_rows` bare/baited split, `income_per_tick`). `Builder` is upstream's -1..4 level as a closed enum. |
| `sim.rs` | **MPL.** `settle()` and the per-second steps: fire cooling (with the builder's auto-stoke *before* the cool, so a tended fire holds its level), temperature drift, the builder arc, the need-wood forest unlock, income payout, arrivals. Plus `roll_traps`. |
| `persist.rs` | JSON save envelope (`schema_version` + `game`), tolerant of a missing/corrupt blob (falls back to a fresh dark room). |
| `svc.rs` | `DarkroomService` (cheap `Clone`, `Arc`-backed): async load via a `watch` channel, fire-and-forget save/delete over `darkroom_saves`, per-user write gate so a burst of saves cannot land out of order, and `reward_escape` (the ending's feed line, the per-run chip payout keyed on `Game.run_id`, and the first-time badge; the Green Dragon `reward_dragon_kill` shape). No shared world, no tick loop, no published snapshot. |
| `event.rs` | **MPL.** The scene machine and the fight, including the battleship's status layer (`Status`/`Affliction`: shield, enraged, meditation, venomous, energised, and the player's stim boost), its `Special` timers, `at_health` triggers and the death blast (`Phase::Exploding`). Upstream's per-scene closures become a closed `Effect` enum and its `isAvailable` predicates a closed `Condition` enum; its `setInterval` fight timers become second countdowns stepped from `State::tick`. `Ctx` writes, `Look` reads (the renderer never clones a save to list rows). |
| `scenes_village.rs`, `scenes_encounters.rs`, `scenes_setpieces.rs` | **MPL.** The three event pools, transcribed scene for scene. |
| `scenes_executioner.rs` | **MPL.** The ravaged battleship: the intro, the antechamber's elevators, the three decks and the command deck, from `events/executioner.js`. The one landmark with more than one way in (`world::battleship_scene`), and the only one that is never marked visited. |
| `world_data.rs`, `world.rs` | **MPL.** The wasteland: tiles, landmarks, weapons, weights and capacities; then generation, walking, supplies, danger, fights, clearing dungeons, going home and dying. Pure rules, tagged outcomes, no I/O. |
| `space.rs` | **MPL.** The sixty-second ascent as a per-tick state machine, plus the ship's costs and both endings (`ENDING`, and `BEACON_ENDING` for a ship that leaves holding the fleet beacon). |
| `state.rs` | Per-session `State`: the authoritative `Game`, the `View` (Room/Outside/Path/World/Fabricator/Ship), the cursor over `Row`s, the capped notification log, the live event modal, the live flight, and the `Ending` (the epitaph's beats and its reveal clock). `tick()` drains the load channel, settles village time, and steps live play against the wall clock. |
| `ui_event.rs`, `ui_world.rs` | The event modal and fight panel; the masked map and the ascent. |
| `ui.rs` | Rendering only: the live page (status line, action column, stores column, log, footer with the allowance), the ending screen, and the Games-hub landing card (which credits upstream). |
| `screen.rs` | The `DoorGame` impl (`GAME`), launcher/active key+arrow handling, and `leave` (settle, save, return to the Games hub). |

## Persistence

`darkroom_saves` (migration `127`, model `late-core/src/models/darkroom_save.rs`)
is one JSONB blob per user, exactly like `greendragon_characters`: the save
shape evolves without new migrations. Every `Game` field carries a serde
default.

`darkroom_veterans` (migration `145`, model `darkroom_veteran.rs`) is the one
thing that is **not** in the blob, and that is the point: winning deletes the
blob, so the fact that an account has finished has to live where the wipe
cannot reach it. One row per account that has got off the rock, existence is
the answer, no counter and no score. It is read on every load (not only on a
fresh save) so whoever earns the unlock mid-run sees it without starting over,
and a failed read is treated as "not yet" rather than as a fault: the worst it
can cost is one landmark on one map until the next visit.

## Integration points (mirror Green Dragon)

`Screen::Darkroom`, `HubGame::Darkroom`, `DoorGameId::Darkroom`,
`App::{darkroom_service, darkroom_state, enter_darkroom, leave_darkroom}`,
`SessionConfig`/server-`State` service injection
(main/ssh/session_bootstrap/test-helpers), render draw arm, input dispatch +
Esc, tick drain, and the hub launch/landing/reset.

`enter_darkroom` is the one that differs: it derives the session's connect time
from `App::started_at` and hands it to `State::new`, because that is what
bounds how much elapsed time may be credited.

## Scope: the whole classic game

**In:** the fire and the builder arc, the forest, traps, huts and population,
the worker/income economy, the trading post's buy menu, the workshop crafting
tier, the random event pool (village events, the thief, the delayed
Mysterious Wanderer payoffs), the compass and the path, the wasteland with its
generated map, supplies, danger and encounters, every classic setpiece, the
mines feeding the village their ore, the ravaged battleship and the fabricator
it hands over, the ship, the ascent, and both endings.

**Deliberately cut:** upstream's `prestige.js`/`scoring.js` (a per-resource
point total that resets does not fit a save that only ever grows), the `cache`
landmark, and upstream's marketing event. Nothing else.

**The three clocks.** Village time is credited, capped and slowed (`pace`).
Wall-clock cooldowns (stoke, gather, traps, embark, liftoff, delayed rewards)
are neither. Live play (world moves, fights, the ascent) runs on the raw delta
between ticks: expeditions cost no daily allowance and are never slowed,
because a trip is paid for in supplies per move, not in time.

**Expeditions park.** The in-flight trip lives in `Game::expedition` and is
saved on every move, so a dropped SSH connection or an Esc leaves the wanderer
standing where they were rather than losing the trip. A fight in progress
parks with it (`Expedition::combat`) and resumes on return, whichever way the
session ended: leaving the door must never be a way to flee one. Upstream
loses everything when the tab closes; that is a deliberate softening, and it
costs nothing because supplies burn per move. Death still discards the trip
and the pack.

**In the village, almost nothing decays.** Stores, buildings, population and
unlocked trades mostly only go up. The exceptions all arrive with the
wasteland: the event pool can burn a hut, take villagers, wreck traps, and the
thieves skim stores once the village is rich enough to rob.
Absence costs progress, never possessions. That bites early (the builder arc
stalls whenever the room is below Warm, so a dead fire freezes her; and she
refuses to build at all while the room is Cold or worse), and mostly stops
biting once she is Helping, because she stokes the fire herself and out-earns
what she burns. Past that point the brakes are the daily cap and the supply
lines an expedition needs; the village events only ever fire while somebody is
in the door, so an absent player is never robbed of anything but time.

**Deliberately dropped:** `dropbox.js` (cloud saves), `audio.js` /
`audioLibrary.js`, `notifications.js` and `Button.js` (DOM widgets), roughly
1,100 lines that the terminal replaces rather than translates.

## The two endings: the only things here that end

Winning the ascent (`Flight::Won`) is the only terminal state the door has,
and it is deliberately loud about it. Which of the two endings runs is decided
by one thing: whether the store room holds the `fleet beacon` taken off the
immortal wanderer (`state::Escape`, `Plain` or `WithBeacon`).

- **The epitaph takes the whole panel.** `state::Ending` holds the beats (the
  closing lines from `space::ENDING` or `space::BEACON_ENDING`, the run's last
  figures, the badge, the way out) and a reveal clock; `ui::draw_ending` lays
  out every beat from the first frame and leaves the unrevealed ones blank, so
  the text never crawls up the screen as it arrives. Any key skips the wait;
  the next key leaves the door. There is nothing behind the ending to go back
  to, so that is the screen's only exit (`screen::ending_took_key`, which owns
  Esc and the arrows too).
- **The account keeps the run: chips every time, a badge the first time.**
  `svc::reward_escape` is the Green Dragon `reward_dragon_kill` shape: a
  `#lounge` feed line every time (the beacon run's says so:
  `Escape::feed_detail` appends "followed the fleet beacon home"), the chip
  payout, and — first escape *of that kind* only, on the `NOT EXISTS` award
  insert — a rankless profile badge. Plain: `darkroom_escape`, migration 143,
  `ADE`, 15,000. Beacon: `darkroom_beacon_escape`, migration 145, `ADB`,
  20,000. Separate claims, so an account can earn both.
  **The chips repeat** (SHOP.md Phase 6, migration 158): the ending wipes the
  save, so a second escape is the whole arc walked again and it pays again.
  The gate is `Game.run_id`, a uuidv7 stamped on a fresh game and carried in
  the save blob (a pre-Phase-6 blob deserializes with one of its own rather
  than a nil id shared by every old save), passed to `reward_escape` and used
  as the `per_event` key, so a retried grant task pays once and a new run is a
  new event. `Escape::reward_line` is the one place the amounts are written
  out for the UI. Badge codes are registered in
  `late-core/src/models/profile_award.rs` and `user.rs`'s chat-label SQL; chat
  shows only the higher of the two (`BADGE_LADDERS`), the profile page still
  lists both. See `app/leaderboard/CONTEXT.md`.
- **The account also remembers that it happened.** The same task writes the
  `darkroom_veterans` row, first and on its own, because it is the one thing
  here that changes what the game *is* next time and must not be lost to a
  failure in the payout path. It lives outside the save blob precisely so the
  wipe below cannot erase it, and the only thing it buys is the battleship on
  the next run's map. Which endings an account has reached is not recorded
  there: the `ADE`/`ADB` badges already do that, permanently.
- **The save does not survive.** The win deletes `darkroom_saves` for the user,
  so the next visit is a dead fire in a dark room and the whole arc is there to
  walk again. That makes `State::save` and `save_on_leave` load-bearing: both
  return early while `ending` is up, or stepping out of the door would write
  the finished run straight back over the wipe. `tick` returns early for the
  same reason (it only advances the reveal): settling village time into a game
  nobody will ever save again is pure waste.
- **A dropped connection during the ending loses nothing.** The wipe and the
  grant both fire the moment the ship gets through, not on dismissal. The
  player misses the words, never the reward.

## The ravaged battleship: the one landmark you go back to

The 10th-anniversary content, and the only part of the game gated on anything
outside the save.

- **It is not on a first run's map.** `world::generate` takes a `veteran` flag
  and drops `Tile::Battleship` only when it is set; `Game::veteran` comes from
  `darkroom_veterans` and means "this account has flown out before, either
  way".
  A save whose map was drawn before the account earned the unlock gets the
  wreck retro-fitted on load (`world::place_battleship`), so nobody has to
  throw a run away to see it. That is the whole of what finishing the game
  buys: one landmark. It never touches stores, perks, or any pacing constant.
- **Every other landmark is played once; this one is a place.** The square is
  never marked visited. The first arrival runs `executioner-intro`, which ends
  by power-cycling the ship, fighting the turret that wakes up, and taking the
  strange device (`Effect::EnterBattleship`); every arrival after that runs
  `executioner-antechamber`, whose elevator buttons fall away as their decks
  are picked clean. The square only stops offering when the command deck falls
  and `Effect::ClearDungeon` turns the wreck into an outpost.
- **Progress is trip-scoped until you get home.** `Expedition::battleship` is
  the working copy, started from `Game::battleship` on embark and committed by
  `world::go_home`, exactly like the map. Dying in the wreck loses the decks
  cleared that trip along with the pack. That is upstream's `World.state`.
- **The fabricator is what the wreck is really for.** Getting inside once
  brings the strange device home, which opens `View::Fabricator` (its own
  panel, slotted in just before the ship's). It builds three things off alien
  alloy alone; the other six need a blueprint, and blueprints are loot the
  three decks drop, redeemed out of the pack by `Game::redeem_blueprints` on a
  safe return and never shelved as stores.
- **The fights needed a status layer the rest of the game has no use for.**
  `event::Status` is six mutually exclusive conditions (upstream keeps one
  free-form string per fighter, so they cannot stack): `Shield` turns the next
  hit into healing and breaks, `Enraged` swings on a half-second clock,
  `Meditation` banks everything thrown at it and gives the pile back in one
  swing, `Venomous` leaves a bleed, `Energised` quadruples damage, and `Boost`
  (the wanderer's, from a stim) halves attack cooldowns. They arrive from
  `Combat::specials` (timers), `Combat::at_health` (a threshold crossed
  downwards, once) and the three new fight rows. The immortal wanderer rotates
  three of them at random, never the same one twice running.
- **One enemy does not fall over.** `Combat::explosion` puts the fight into
  `Phase::Exploding` instead of `Spoils`: the medical deck's unstable automaton
  takes thirty off the wanderer three seconds after it dies. That phase is
  deliberately unparkable and unleavable, so the blast cannot be dodged by
  stepping out of the door.

## Gotchas

- **The Sleeping → Helping transition is view-gated.** Upstream fires it in
  `onArrival` when you click back to the Room tab, not on a timer. `settle`
  takes the current `View` for exactly this. Every other room clock keeps
  running while the player is outside.
- **`refresh_build_options` latches.** Once an option has been offered it stays
  offered, even after the materials are spent (upstream's `Room.buttons`).
- **The first fire is free.** Upstream lets you light it with no stores at all,
  because `stores.wood` does not exist yet; the port keys that off
  `has_seen(Wood)` rather than the balance.
- **The builder's auto-stoke runs before the cool in the same step**, so a
  tended fire holds its level instead of climbing. That is upstream's ordering,
  not an accident.
- **Do not tick this from the render loop.** `tick()` settles, but the settle is
  a subtraction against the wall clock; it is correct at any cadence and must
  stay that way.
- **Never gate the load pickup on `watch::Receiver::has_changed()`.** The loader
  drops its sender the instant it has sent, and tokio reports `Err` on a closed
  channel, so `has_changed().unwrap_or(false)` is permanently false and the
  session sits on "the dark is quiet..." forever. `tick()` reads the value while
  `game` is `None`, which is what `state_test` exercises.
- **A lone Esc never reaches the keymap.** It is held as `pending_escape` and
  dispatched by `flush_pending_escape` in `app/input.rs`, which has one explicit
  arm per screen. Wiring `handle_key` alone leaves Esc dead: the door needs its
  own arm there (next to Green Dragon's) to be leavable at all.
- **`Game::outfit` is a plan, not a claim.** The store room can shrink after
  packing (the charcutier eats meat, the thieves skim fur), so `world::embark`
  clamps every line to what the shelf still holds, and `can_embark` gates on
  the packed-and-still-held cured meat, exactly like upstream's live
  outfitting screen. Skip the clamp and a stale loadout conjures supplies.
- **Never clear `Expedition::combat` on the way out of the door.** `park()`
  snapshots the fight before dropping the modal; clearing it instead would
  turn Esc into a free flee button, while a dropped connection still resumed
  the fight. The two exits must stay equivalent.
- **The battleship square is never marked visited.** Every other setpiece ends
  with `Effect::MarkVisited`; this one must not, or the antechamber becomes
  unreachable and the decks can never be finished.
- **The quadruped's loot table has a duplicate key upstream.**
  `events/executioner.js` lists `alien alloy` twice in it, and a JavaScript
  object literal keeps the last. The port transcribes what the game does (2-4
  at one in five), not what the table reads. Do not "fix" it back.
- **The stim costs blood, so its row refuses at low health.** Upstream lets you
  kill yourself with it; `row_ready` does not, because over SSH a button that
  ends the run with no warning reads as a bug rather than a risk. `Cost::Hp`
  follows the same rule: `affordable` wants strictly more hp than the cost
  (upstream allows exactly-enough and leaves the wanderer walking at 0), so a
  button's cost can never be the killing blow.
- **A modal with nothing pressable always grows a leave row.** The burning
  junction (engineering `1-3`) is upstream-faithful: two cost-gated buttons,
  no leave. A browser player who can pay neither refreshes the page; over SSH
  the modal swallows Esc, so `Active::rows` appends `Row::Leave` whenever no
  listed row passes `row_ready`. Any future all-cost-gated scene is covered by
  the same fallback.
- **A parked fight resumes with a clean status layer.** `CombatSnapshot`
  persists only the event, the scene and `enemy_hp`; `Active::resume` rebuilds
  through `Fight::start`, which zeroes statuses, special timers, the bleed and
  the fired `at_health` thresholds. Deliberate: persisting the layer would
  grow the save schema for two fights, and the cost of the reset is bounded
  (the enemy's health is kept, and its specials simply start their clocks
  over). It does hand a parked immortal-wanderer or robot fight a mild edge;
  if that ever reads as an exploit, the fix is persisting the layer in
  `CombatSnapshot`, not blocking the park.
- **`Phase::Fighting` is boxed.** A live fight carries a stat line, three timer
  collections and two statuses; unboxed it would set the size of every `Phase`.
- **A press inside the modal keeps the cursor on the row it pressed.** Upstream
  is a page of buttons that never move: using one greys it out and leaves it
  where it was. `State::keep_cursor_on` reproduces that, tracking the row
  rather than its index, and only resets to the top when the event, the scene
  or the phase changed. The event key is part of that comparison on purpose:
  nearly every event calls its opening scene `start`, so comparing scene keys
  alone reads as "same screen" when a button has walked into a different event
  entirely.
- **The thief skim is not a starved trade.** Every other income source skips
  its whole payout when an input runs short; the skim drains to zero and books
  only what was actually there into `Game::stolen` (upstream `addStolen`),
  because that is the pile "hang him" gives back.

## Deliberately not built

- **No leaderboard boards, and no lifetime counters to feed them.** A Dark
  Room pays badges, not standings. It is a game you finish twice at most, so a
  liftoff tally would rank people on how many times they replayed a save that
  gets deleted either way. `darkroom_veterans` holds one fact for one reason;
  if a board ever seems wanted, that is a decision to reopen, not a column to
  quietly add back.
- **No legacy bonuses on a fresh save.** Finishing the game buys exactly one
  landmark. It does not pad opening stores, grant a starting perk, or touch
  `SLOWDOWN`, the daily cap, or any other pacing constant. The last of those
  is the one that matters: a bonus that moves pacing is a way to buy out of
  the pacing model, and the pacing model is the port's whole design.
- **No `prestige.js`/`scoring.js`.** A per-resource point total that resets
  does not fit a save that only ever grows (see "Scope").
