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
| `data.rs`, `model.rs`, `sim.rs`, `event.rs`, `world.rs`, `world_data.rs`, `space.rs`, `scenes_village.rs`, `scenes_encounters.rs`, `scenes_setpieces.rs` | **MPL-2.0** (header + Exhibit B on each) | Carry upstream balance tables, timing constants, rules, scene graphs and prose. Text is copied verbatim, which the MPL permits precisely because these files stay MPL. |
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
| `svc.rs` | `DarkroomService` (cheap `Clone`, `Arc`-backed): async load via a `watch` channel, fire-and-forget save/delete over `darkroom_saves`, per-user write gate so a burst of saves cannot land out of order. No shared world, no tick loop, no published snapshot. |
| `event.rs` | **MPL.** The scene machine and the fight. Upstream's per-scene closures become a closed `Effect` enum and its `isAvailable` predicates a closed `Condition` enum; its `setInterval` fight timers become second countdowns stepped from `State::tick`. `Ctx` writes, `Look` reads (the renderer never clones a save to list rows). |
| `scenes_village.rs`, `scenes_encounters.rs`, `scenes_setpieces.rs` | **MPL.** The three event pools, transcribed scene for scene. |
| `world_data.rs`, `world.rs` | **MPL.** The wasteland: tiles, landmarks, weapons, weights and capacities; then generation, walking, supplies, danger, fights, clearing dungeons, going home and dying. Pure rules, tagged outcomes, no I/O. |
| `space.rs` | **MPL.** The sixty-second ascent as a per-tick state machine, plus the ship's costs and the ending. |
| `state.rs` | Per-session `State`: the authoritative `Game`, the `View` (Room/Outside/Path/World/Ship), the cursor over `Row`s, the capped notification log, the live event modal and the live flight. `tick()` drains the load channel, settles village time, and steps live play against the wall clock. |
| `ui_event.rs`, `ui_world.rs` | The event modal and fight panel; the masked map and the ascent. |
| `ui.rs` | Rendering only: the live page (status line, action column, stores column, log, footer with the allowance) and the Games-hub landing card (which credits upstream). |
| `screen.rs` | The `DoorGame` impl (`GAME`), launcher/active key+arrow handling, and `leave` (settle, save, return to the Games hub). |

## Persistence

`darkroom_saves` (migration `127`, model `late-core/src/models/darkroom_save.rs`)
is one JSONB blob per user, exactly like `greendragon_characters`: the save
shape evolves without new migrations. Every `Game` field carries a serde
default.

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
mines feeding the village their ore, the ship, the ascent, and the ending.

**Deliberately cut in v1/v2:** the Executioner battleship (upstream calls it
"A Ravaged Battleship"), the fabricator and everything it makes, prestige,
scoring, the `cache` landmark, and upstream's marketing event. `laser rifle`
and `energy cell` stayed, because the classic setpieces drop them regardless.

**v3, planned (see "Planned: v3" below):** the battleship and the fabricator
are being un-cut. Upstream's own `prestige.js`/`scoring.js` point-total stays
cut, it doesn't fit a save that only ever grows, but a smaller, original
"legacy" layer is going in its place: two lifetime counters (liftoffs, and
liftoffs made while holding the fleet beacon) that live outside the save
so a delete-and-replay doesn't erase them, small permanent bonuses a fresh
save reads from them, and two boards on the shared Leaderboards page. The
`cache` landmark and the marketing event stay cut, no reason found to want
either.

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
- **The thief skim is not a starved trade.** Every other income source skips
  its whole payout when an input runs short; the skim drains to zero and books
  only what was actually there into `Game::stolen` (upstream `addStolen`),
  because that is the pile "hang him" gives back.

## Planned: v3, the battleship, the fabricator, and a legacy across runs

Execution plan for the next chunk of work, written for an LLM executor and
reviewed phase by phase by another agent, same discipline v1 and v2 used.
Everything above this section is shipped and load-bearing; everything below
is not built yet. When a phase below ships, fold anything durable it
establishes up into the relevant section above and cut the phase's
description down to a line in "Deliberately cut in v1/v2" / "v3, planned"
(whichever it becomes), the way v2's plan folded into this file. Don't leave
two descriptions of the same shipped behavior around.

### Ground rules for the executor

Identical to what got v1 and v2 built:

- **Source of truth is the local clone** at `upstream-adarkroom/` (repo root,
  gitignored; `git clone --depth 1 https://github.com/doublespeakgames/adarkroom upstream-adarkroom`
  if missing). Transcribe balance tables, timings and prose from upstream
  files directly, never from memory. The clone may have moved on since v1/v2
  were written; re-verify file paths before transcribing.
- **Licensing is file-level and non-negotiable.** Any file carrying
  upstream-derived tables, rules, or prose gets the MPL-2.0 header (copy it
  verbatim from the top of `data.rs`, adjusting the "transcribed from" line).
  Files that are purely late.sh's own work (the legacy record, leaderboard
  wiring) stay FSL-1.1-MIT, no header. Update `NOTICE` and `LICENSING.md`
  whenever the MPL file list changes.
- Upstream prose is transcribed **verbatim**, including punctuation; the
  repo's no-em-dash rule applies to our own code/comments/UI chrome, not to
  upstream's sentences.
- Tests live beside the file (`foo_test.rs` + `#[cfg(test)] mod foo_test;`),
  written together with the code, run through the capped runner:
  `ARGS="darkroom" make test-llm` (env var before `make`, never as a `make`
  argument, or jemalloc's build dies).
- Never `git commit`. Leave everything in the working tree.
- No `#[allow(...)]` to silence lints; fix the structural cause.
- Closed enums with exhaustive matching everywhere. A new variant must break
  the build. No catch-all arms on enums we control.
- Keep the module flat (files directly in `darkroom/`), consistent with v1/v2.
- At the end of every phase: stop. The reviewer reads the diff before the
  next phase starts.

### Decisions

1. **Un-cut: the Ravaged Battleship and the Fabricator.** Upstream sources:
   `events/executioner.js` (the battleship landmark, its entrance turret
   fight, the three decks, the Command Deck, the Immortal Wanderer) and
   `fabricator.js` (the crafting station and its blueprint-gated recipes).
   Un-cut items: hypo, stim, kinetic armour, plasma rifle, energy blade,
   disruptor, glowstone, cargo drone, fluid recycler.
2. **Still cut: upstream's own `prestige.js`/`scoring.js`.** A per-resource
   point total that resets and carries a score forward doesn't fit a save
   that only ever grows (see "In the village, almost nothing decays" above).
   No per-resource point values are being ported. What replaces it is
   narrower and original to late.sh, decision 4 below.
3. **Still cut: the `cache` landmark and upstream's marketing event.** Purely
   cosmetic/promotional in upstream, no gameplay content worth porting.
4. **New: the legacy record.** A per-user row, `darkroom_legacy(user_id PK,
   liftoffs int, beacon_liftoffs int, updated_at)`, added by a new migration
   (next number after `133_create_door_rcs.sql` at plan time; the darkroom
   save table itself was migration 127). It lives **outside** the JSONB save
   blob in `darkroom_saves` specifically so the existing delete-and-replay
   flow (the only way to start a new run today) does not erase it. Bumped
   once, in `State::tick_flight`'s `Flight::Won` arm (`state.rs`, next to
   where `game.completed = true` is already set today): `liftoffs` on every
   win, `beacon_liftoffs` additionally when the run held the fleet beacon
   before the ascent. This is the one place v2's "no auto-reset, no
   prestige" decision gets a nuance: the *save itself* still never resets or
   carries a score, replaying is still exactly "delete and start over"; what's
   new is that the account remembers how many times that has happened.
5. **New: permanent bonuses on a fresh save.** `Game::new()` reads the
   account's `DarkroomLegacy` once, at creation, and applies a deterministic,
   capped `LegacyBonus`, never touched again mid-run. First cut (numbers are
   placeholders to tune during implementation and playtesting, not
   contractual):
   - Every prior liftoff (either ending) grants a modest starting stash on
     the *next* fresh save, capped: e.g. `min(liftoffs, 5) * 20` wood and
     `min(liftoffs, 5) * 10` fur. Rewards replaying without trivializing the
     early game.
   - The first fleet-beacon liftoff, and only the first, unlocks one
     permanent starting `Perk` on every future fresh save (reuse the closed
     `Perk` enum from `PLAN_V2`'s data model, e.g. `Scout`, otherwise only
     earned deep into a run via The Scout's village event). Repeats past the
     first don't stack more perks: one save-file-scoped economy, avoid
     runaway power creep.
   - A fresh save with a nonzero legacy says so plainly in the opening log
     line, original prose (upstream never had this, nothing to transcribe).
   - Guardrail: a legacy bonus may only ever pad starting stores or grant one
     starting perk. It must never change `SLOWDOWN`, the daily cap, or any
     other pacing constant, a legacy bonus is not a way to skip the pacing
     model.
6. **New: two Leaderboard boards.** `late-ssh/src/app/leaderboard/state.rs`
   gets two more bespoke `Board` variants alongside `TopChips`/`ArcadeWins`:
   `DarkroomLiftoffs` and `DarkroomBeaconLiftoffs`, ranked by the lifetime
   counters in `darkroom_legacy` (fetch functions in
   `late-core/src/models/leaderboard.rs`, same shape as
   `fetch_monthly_chip_earners`/`fetch_arcade_champions`, new
   `LeaderboardData` fields alongside `monthly_chip_earners`/
   `arcade_champions`). Open wrinkle for the reviewer: existing bespoke
   boards are monthly-only (`monthly()` always returns data, `all_time()` is
   `Option`); a lifetime liftoff count is the opposite shape, all-time-only,
   a monthly reset makes no sense for it. Either invert `Board`'s contract to
   let `monthly()` return `Option` the same way `all_time()` already does, or
   park the lifetime count under `monthly()` with a title/hint that says
   plainly it isn't windowed. Pick whichever reads cleaner; don't guess,
   check in with a reviewer before locking the shape.

### Data model changes

All in `model.rs`/`data.rs` (MPL) unless noted. Every new `Game` field
carries a serde default so v1/v2 saves load unchanged; no `schema_version`
bump needed.

- `Resource` grows with the nine un-cut fabricator/executioner items above.
- New `Game::fabricator: Option<FabricatorState>` (unlocked bool + a closed
  `FabricatorBlueprint` bitset for what's been found in the three decks,
  the same latch shape as `seen_buildings`/`seen_jobs`).
- New `Game::battleship: BattleshipProgress`, a closed enum:
  `Undiscovered, EntranceCleared, DecksCleared { engineering, medical,
  martial: bool }, CommandCleared`. The battleship is multi-stage, unlike
  the single-clear dungeons in `world_data.rs`'s landmark table, so it needs
  its own progress type rather than reusing the generic dungeon-clear flag.
- New `Game::fleet_beacon: bool`, held flag, checked at the top of the space
  ascent to pick the ending.
- New landmark table entry in `world_data.rs` (rare spawn, torch required to
  enter, per upstream).
- `late-core/src/models/darkroom_legacy.rs` (new, FSL, our own model, same
  shape as other one-row-per-user tables like `tetris_high_scores`): the
  migration, the row type, and read/bump functions.

### Phases

1. **Legacy + Leaderboard plumbing.** The migration, the model, the bump call
   wired into `Flight::Won`, `LegacyBonus` computed and applied in
   `Game::new`, the two new `Board` variants and their fetch functions. No
   gameplay dependency on the battleship, ships and is reviewable on its own,
   and gives a visible result immediately (a leaderboard showing "no runs
   yet" for everyone). Tests: legacy bump on both ending types, bonus
   capping, a fresh `Game::new` reads the account's real legacy row, board
   fetch functions against seeded rows.
2. **The battleship landmark and the entrance fight.** The landmark table
   entry, a new `scenes_executioner.rs` (MPL) transcribing the entrance
   sequence: power-cycling the ship wakes a defense turret, fought like any
   other combat encounter. Clearing it sets `BattleshipProgress::
   EntranceCleared` and grants the Fabricator. Tests: landmark spawn odds
   and torch gating, entrance fight stats against upstream, state transition.
3. **The Fabricator and the three decks.** Blueprint drops from Engineering,
   Medical, and Martial (transcribed from `events/executioner.js`), the
   fabricator craft list (alien alloy + blueprint-gated, same whole-refusal
   shape as `Game::craft`), the nine new items' costs and stats from
   `fabricator.js`. Tests: each deck's blueprint drop, craft gating per
   blueprint, whole-refusal on short alloy.
4. **The Command Deck, the Immortal Wanderer, and the fleet beacon.**
   Unlocked once all three decks report cleared. The toughest fight in the
   game, transcribed faithfully (upstream calls it the strongest enemy in
   the game, no reason to soften it). Defeating it grants the fleet beacon
   (`Game::fleet_beacon = true`). The space ascent (`space.rs`) branches on
   the flag: held before liftoff swaps in the alternate ending text (the
   Wanderer's fleet, wrecked, nobody left, air runs out) instead of the
   standard one, and bumps `darkroom_legacy.beacon_liftoffs` instead of (or
   alongside, decide during review) `liftoffs`. Tests: Command Deck gating,
   Immortal Wanderer fight stats, ending branch selection, legacy bump
   distinguishes the two endings.

### Review protocol

Same as v1/v2: after each phase the executor stops with the working tree
dirty and reports what was transcribed from which upstream file, deviations
(if any, with reasons), the test list, and anything suspicious left out of
scope. The reviewer checks the diff against upstream and this section, with
special attention to: licensing placement, verbatim prose, closed-enum
discipline, the legacy record staying outside the save blob, and that the
`LegacyBonus` guardrail (stores/perk only, never pacing) holds.
