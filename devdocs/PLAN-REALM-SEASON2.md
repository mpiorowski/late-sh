# Plan: Realm season 2, orders resolved at the night

Status: specified, not implemented. Two runs: run 1 builds the game (section
3), run 2 builds the full-scale simulation and tunes the numbers with it
(section 4). Each run ends in a review against section 5. As the work lands,
move what shipped into `late-ssh/src/app/door/realm/RULES.md`, `CONTEXT.md`
and `UI.md` (sections 2 and 3 say which lines go where), record the measured
numbers there, and delete this file. Never commit; leave the tree for the
owner.

Read first: `late-ssh/src/app/door/realm/CONTEXT.md`, `RULES.md`, `UI.md`,
`MAPS.md`, then `rulesets.rs`, `resolver.rs`, `svc.rs::act_at` and
`svc.rs::sweep_game`, `simulation_test.rs`. The root `CONTEXT.md` and
`/home/mat/.claude/CLAUDE.md` (the guide) govern how the code is written.

## 0. Decisions already made

Do not reopen these; they are the owner's calls.

- Actions become **orders written during the day and resolved together at
  the reset** (the night). Nobody gains anything by being at the keyboard
  when the points come back.
- Fights are **rolls with steep odds**, not deterministic. A deterministic
  variant may be tested later; design the formula so that swapping the roll
  for a comparison is one function.
- The win is **domination** (a share of the world held across consecutive
  nights) inside a **season clock**; the clock runs out to the highest share.
- **20 players**, with the roster free to fill late: ten now, five more in a
  week, five never. Every rule below works at two players and at twenty.
- The changes land as a **new ruleset entry**. Season 1 is frozen on its
  snapshot and finishes under the instant rules; the house opens season 2
  on the new entry. Every second season is a drawn world (unchanged).
- Replays are **ASCII first**: a stepped playback of the night on the map.
  Animated armies are a later idea.

## 1. What season 1 showed

- Whoever spent their points at 18:00 UTC took the free land and the soft
  targets before anyone else could answer. Orders fix this outright.
- Strength was the empire's total size, raised to 1.6, so the leader became
  untouchable the moment they led. Local strength (what you have *around* a
  territory) fixes this: a big empire has a long border.
- The late game dragged: the leader at 71% of the world had to take 130 more
  countries one roll at a time. Domination ends the game when it is decided.
- The only strategy was to grab the most small countries fastest; walls were
  a losing trade by design. Walls become strength, size becomes the score.

## 2. The game

### 2.1 One ruleset entry, two engines

- `rulesets.rs`: a new entry `CAMPAIGN` (`id: "campaign"`, display
  "Campaign", tagline "write your orders by day, the night resolves them")
  beside `STANDARD`. `RULESETS = &[STANDARD, CAMPAIGN]`.
- A new ruleset field `resolution: Resolution`, a closed enum
  `{ Instant, Orders }`. `STANDARD` is `Instant`, `CAMPAIGN` is `Orders`.
  The snapshot field carries `#[serde(default)]` = `Instant`, which is what
  every frozen game was. This is the one branch the engine and service take
  on the ruleset, by exhaustive `match`; nothing else inspects the id.
- `svc::house_spec` opens on `CAMPAIGN`. `HOUSE_PACE` stays `fast`,
  `HOUSE_RESET_HOUR_UTC` stays 18.
- The instant engine (`resolver::apply_action`, `ReachIndex`, the action
  key path in `state.rs`, `svc::act_at`) stays untouched for the frozen
  season. `svc::act_at` on an `Orders` game refuses with "this realm takes
  orders, not actions". Deleting the instant engine once the last instant
  game has ended is a later task; note it in CONTEXT.md's deferred list.
- New engine file `night.rs` (pure: no db, no tokio, no clock) with
  `night_test.rs` beside it, wired `#[cfg(test)] mod night_test;`. It owns
  everything in 2.2 to 2.7 that is not a state field: order validation,
  force and defence, the night, the win check for `Orders` games.
- `Resolution` is matched exhaustively in exactly three places and nowhere
  else: `svc::close_day` (what turning the day over does), the service
  entry points (`act_at` refuses on `Orders`; `place_order_at` and
  `withdraw_order_at` refuse on `Instant`), and `RealmState` (what `a`/`f`
  do and whether the history view offers the replay). Everything else is
  shared and engine-blind: the state struct with new fields serde-defaulted,
  the log with new variants added and none changed, the archive, fan-out,
  news, the results card. A fourth match site is a review finding.

### 2.2 The day: orders

- The budget is unchanged: `actions_per_day_tiers` by `involved_count`,
  pace `action_scale`, banking on the missed-day ladder, the allowance
  frozen at the first order of the day (`actions_allowance`), `actions_used`
  now meaning points ordered today. Tiers gain a row:
  `[(4, 7), (7, 5), (10, 4), (20, 3)]`.
- An order is one of two things, a closed enum `Order`:
  - `Take { target, points }`, `points >= 1`. Whether it is a claim or an
    attack is decided at the night from who holds the target then, so a
    territory that changes hands during the day never strands an order.
  - `Fortify { target, levels }`, `levels >= 1`, one point per level, on
    land the player holds, capped at `fort_max_level`.
- One order per target per player: ordering the same target again replaces
  the order (and its points). `RealmPlayer` gains `orders: Vec<Order>` and
  `orders_day: i32` (orders for another day are stale and ignored), both
  `#[serde(default)]`.
- Placing is validated on the spot and refused for free, the instant
  engine's rule (`OrderRejected`, a closed enum with a `message()`): not
  alive, unknown target, your own land for a take, not your land for a
  fortify, walls already full, no front on the target (2.4), more points
  than are left today, protection in either direction (2.6), the muster
  still running (a service rule, as today). Withdrawing an order returns
  its points.
- Orders are secret until the night. The board draws the viewer's own
  orders only. The terminal is server-rendered and no client reads raw
  state, so this is a presentation rule; if a raw state API ever appears it
  must strip other players' orders.
- A day on which a player placed at least one order is an active day
  (`active_days`, `last_action_day`), whatever happens at the night.

### 2.3 The night

`night::resolve_night(state, map, day, seed) -> NightReport`, called once
for the day that is closing, before `roll_day` opens the next. Every
quantity below is read from the board as it stands when the night begins;
nothing resolved during the night changes another fight's numbers. That is
what makes it simultaneous.

1. Gather every alive player's orders for `day`. Drop the rest.
2. **Walls go up first.** Every `Fortify` is applied, in roster order, so a
   wall ordered today holds tonight. Logged as `Fortified`.
3. **Engagement.** A territory is engaged tonight if any `Take` targets it.
   An engaged territory gives no support to its neighbours (the Diplomacy
   cut), whether or not the attack on it succeeds.
4. **Every take re-checks its legality** against the opening board: a
   target now held by a protected player fizzles (`TargetProtected`), a
   take whose front has gone (the holder withdrew during the day) fizzles
   (`NoFront`), a target the actor now holds fizzles (`AlreadyYours`).
   `LogEntry::Fizzled` and `FizzleReason` come out of retirement for this;
   the two new reasons are added to the enum.
5. **Fights, grouped by target**, targets in seeded random order (the order
   only decides which taker is named on an elimination). For each target:
   - every taker's force `A_i` and the holder's defence `D` (2.4);
   - every taker rolls once, `rng.next_f64() < p_i`, `p_i` from `A_i` and
     `D` (2.4); one roll per order however many points it carries;
   - among successful rolls the highest force takes the territory; an exact
     tie of the highest forces **bounces** and the holder keeps it; no
     success is a hold;
   - a taken territory's walls reset to 1 (as today).
   Logged as one `LogEntry::Fought` per target: the target, the holder
   before (None for neutral), the holder after, `defence`, and per taker
   `{ user_id, points, from: Option<TerritoryId>, force, probability,
   success }`, plus `bounced: bool`. `from` is the taker's strongest front
   territory (a launch point for a landing), for the replay.
6. **Falls.** A player holding nothing after step 5 is `Eliminated`, by
   whoever took their last territory in resolution order. `exit_territories`
   as today.
7. **The end** (2.7), evaluated once, after everything above.
8. The report is appended to the closing day's log (`DayResult.entries`)
   and kept on the row as `RealmGameState.last_night: Option<DayResult>`
   (the day and its night entries only), so the dispatch is one read and
   survives a lost archive write.

The seed is `day_seed(game_id, day)` with a per-target stream derived as
`blake3(day_seed ‖ target)`, so adding a fight never reshuffles another's
roll. All rolls use `SplitMix64`.

### 2.4 Force, defence, odds

Every number here is a `CAMPAIGN` field, named below with its starting
value. Run 2 tunes them; run 1 ships them as written.

- **Garrison** of a territory: `g(T) = walls_eff(T) × (1 + garrison_weight × w_T)`
  with `w_T` the territory's weight (as today, 0 to 1),
  `walls_eff = 1 + (walls − 1) × decay(holder)` (dormancy walks walls back
  to 1), `garrison_weight` 1.0. Neutral land has walls 1 and no decay.
- **Front** of a taker on `T`: the sum of `walls_eff` over the taker's
  territories that border `T` by land (`land_links` on a landmass the taker
  stands on, the instant engine's rule), plus `sea_front × walls_eff` over
  the taker's coastal holdings within `sea_reach_km` of a coastal `T`.
  `sea_front` 0.5, `sea_reach_km` 1500. A front of zero means no order can
  be placed (2.2). The target must therefore be on the taker's border or
  within sea reach; there is no long-range strike and no march past rivals
  in this engine. `long_range_strikes` is meaningless for `Orders` and is
  not read; `landlocked_sea_attack` still decides who may launch from a
  coast.
- **Force**: `A = points × (1 + front_factor × front) × landing × encircle`,
  `front_factor` 0.5; `landing` = `landing_factor` 0.75 when the taker has no
  land front on `T` at all, else 1; `encircle` = `encircle_factor` 1.5 when
  every land neighbour of `T` is the taker's (and `T` has at least one), else
  1.
- **Defence** of a held territory:
  `D = g(T) + support_factor × Σ g(n)` over the holder's territories `n`
  bordering `T` by land that are not engaged tonight, `support_factor` 0.5.
  The holder's dormancy is already inside every `g`.
- **Defence** of neutral land: `D = g(T) × neutral_factor`, `neutral_factor`
  0.6, no support.
- **Odds**: `p = A^k / (A^k + D^k)`, `k = contest_exponent` 2.0, clamped to
  `[prob_floor, prob_ceil]` = `[0.05, 0.95]`. At 2:1 that is 80%, at even
  50%, at 1:2 20%. The deterministic variant for later is `A > D`.
- `night::projection(state, map, actor, target, points, day) ->
  Projection { kind, force, defence, probability, front, landing,
  encircled, from }` is the one function the board and the engine both
  call. The board's number and the night's number must come from the same
  code path on the same board; the night's can differ only because others'
  orders were hidden (engagement, walls ordered tonight), and the dispatch
  says so when it does.
- Sanity points for the starting values, to assert in `night_test.rs`:
  one point from a walls-1 front of two onto a medium neutral is about 83%;
  three points from the same front onto a walls-1 medium territory with two
  unengaged walls-1 neighbours is about 80%; five points from a walls-1
  front of two onto a walls-3 territory with two walls-3 neighbours is about
  55%, and the same five points from a walls-3 front are about 83%; a
  fortify ordered tonight raises `D` tonight; engaging a neighbour removes
  its support; a landing is weaker than a march; encirclement helps; a
  bigger `w` is harder; the clamps hold; odds are monotone in points.
- `fort_defence_per_level` and `fort_transit_per_level` are not read by the
  `Orders` engine. Walls enter through the garrison only.

### 2.5 Neutral land

Nothing is free. A claim is a take against `neutral_factor × g(T)`, so a
microstate is a point at good odds and a continent-sized country is a
real fight. Two players taking the same neutral land the same night resolve
like any contested target: both roll, the stronger success takes it, equal
strongest successes bounce. The map fills slower and from fronts, which is
the point.

### 2.6 Protection, joining, twenty players

- Phases are unchanged in meaning (`attack_grace_days` 1,
  `distant_attack_grace_days` 3, `newcomer_shield_max_days` 4,
  `join_max_claimed` 0.5): **new** may only take neutral land and cannot be
  targeted; **ramping up** may take from, and be taken by, land neighbours
  only (no landings either way); **involved** anything in reach. Checked at
  placement and again at the night (2.3 step 4).
- `max_players` 20 on `CAMPAIGN`. `PALETTE_SLOTS` 20. Joining, spawning,
  the muster, the doors (`joinable_of`), the short-of-rivals rule, the
  withdrawal rule, the abandon dissolve: unchanged.
- Everything sized by the roster reads one of two numbers, never a third:
  `involved_count(day)` (the budget tier, as today) or
  `start_player_count`, which already means "the most players the roster
  has held" (the domination threshold, 2.7). A roster of ten that grows to
  fifteen moves both; a roster of twenty that shrinks to three moves only
  the tier, so eliminating rivals never raises the winner's own bar.
- Drawn worlds: `GeneratedMapSpec::default` goes to 300 territories (the
  generator allows 400). Earth stays 242. Run 2 says whether either moves.

### 2.7 Winning

Evaluated at the night only (step 7), never on an order. Three ways to end,
a closed enum `WinKind { Dominated, SeasonOver, LastStanding }` carried on
`ResolveEnd::Won` and into `LogEntry::Won`, the results card and the news.

- **Share** of a player: `(pop_held / pop_world + area_held / area_world) / 2`
  over the whole map, neutral land included. One function,
  `night::share(state, map, user_id)`, used by the standings, the threshold
  and the clock.
- **Domination**: `domination_share_tiers` by `start_player_count`,
  `[(4, 0.50), (10, 0.40), (20, 0.33)]`, the tier lookup the budget tiers
  use. A player at or above their threshold at the end of a night gains one
  on `dominating_nights` (a `RealmPlayer` field, serde default 0); below it
  resets to 0. `dominating_nights >= domination_nights` (3) and `can_win`
  (unchanged: `min_players` on the roster, `min_active_days_to_win` 7
  distinct days, pace-scaled) wins `Dominated`. The three nights are the
  coalition's window, and the standings show the count.
- **The clock**: `season_days` 80 as written, pace-scaled by `day_scale`
  (28 at `fast`, 48 at `normal`, 120 at `epic`, floor 7). The night that
  closes day `start_day + season_days − 1` ends the game after its fights:
  the highest share among players who `can_win` wins `SeasonOver`; ties
  break on territory count, then the seeded jitter `final_ranking` already
  uses; nobody eligible dissolves it. The clock shows everywhere a day is
  shown ("day 12 of 28").
- **Last standing**: every rival eliminated, `can_win` met, as today.
- Payouts, ranking, `payout_earned`, `payout_eligible`, the pot and its
  cap: unchanged. A `SeasonOver` win pays like any other.

### 2.8 The log, the dispatch, the replay

- The day's log gains `Fought` (2.3) and reuses `Fortified`, `Fizzled`,
  `Eliminated`, `Won`, `Spawned`, `Left`, `Kicked`. `Claimed` and `Attacked`
  are the instant engine's and are never written by `Orders`. `log_ui.rs`
  renders `Fought` as one line per taker: "spoon → Micronesia · 3 pts · 42%
  · taken from mat", "… · bounced", "… · held".
- **The dispatch** is what a player sees first after a night: a banner on
  `DayRolled` ("The night has passed: you took 2, lost 1, 1 bounced") and a
  panel listing the viewer's fights from `last_night`, each with the odds
  the board showed at order time beside the odds rolled, and why they
  differ (walls went up, a neighbour was engaged, a landing).
- **The replay** lives in the history view (`history_ui.rs`): with a day
  selected, `p` plays that day's night from the archived log, one `Fought`
  entry per step, the target's cells drawn bright, the taker's `from`
  territory drawn bright in the taker's colour, a caption line under the
  slider ("spoon → Micronesia · 42% · taken"). `.` and `,` step, `p` again
  auto-advances every 500 ms, Esc stops. The map stays the whole world at
  one frame, the view's existing rule. The day's board drawn under the
  steps is the board the night *opened* on, so the archive now stores two
  boards per day: `realm_days.board` (closing, as today) and a new
  `board_open` column (migration, nullable; days without it skip the
  replay). Written by the same `archive_day` upsert with the same COALESCE
  rule.
- The live strip's `news` at a night is the night's biggest moment: a fall
  outranks a take, the heaviest take wins among takes
  (`news_of_night`). One ticker line a night at most, the existing
  `RealmCalls` budget; nothing new reaches #lounge.

### 2.9 What stays exactly as it is

The season loop and the muster, spawns, joining and colours, the chat room,
the live strip's shape, days as DB integers from `reset_hour_utc`, the row
CAS and revision, fan-out derived from the snapshot diff on every replica,
the archive, the pot, the lounge headlines, the results card's `final_table`
being what the payout walks, the abandon dissolve, `REALM_ABANDON_DAYS`,
the pace roster, the asset format. `simulation_test.rs` keeps running the
instant engine against `STANDARD` until that engine is deleted.

## 3. Run 1: implementation

Work in this order; each step compiles and its tests pass before the next.
Run tests scoped: `make test-llm ARGS="-p late-ssh --lib -E 'test(realm)'"`,
never a raw cargo test. Do not run `make check`; the owner does.

### Step 1: rules as data

Files: `rulesets.rs`.

- `Resolution` enum, `CAMPAIGN` entry, the new fields on `RealmRuleset` and
  `RealmRulesetSnapshot` with serde defaults equal to the `STANDARD`
  values (or inert values where `STANDARD` has no use for them):
  `resolution`, `garrison_weight`, `front_factor`, `sea_front`,
  `sea_reach_km`, `landing_factor`, `encircle_factor`, `support_factor`,
  `neutral_factor`, `contest_exponent`, `domination_share_tiers`,
  `domination_nights`, `season_days`. `at_pace` scales `season_days`
  (floor 7). `From<&RealmRuleset>` copies them. `max_players` 20 and the
  fourth tier on `CAMPAIGN` only.
- Tests (inline, as the file does today): roster lookup finds both; the
  tier table resolves 11 to 20; `season_days` scales per pace with its
  floor; snapshot serde round-trips; a snapshot without the new fields
  parses as `Instant` with `STANDARD` numbers.

### Step 2: the night engine

Files: `night.rs`, `night_test.rs`, `resolver.rs` (state fields and log
entries only).

- `resolver.rs`: `Order`, `OrderRejected`, `RealmPlayer::{orders,
  orders_day, dominating_nights}`, `RealmGameState::last_night`,
  `LogEntry::Fought`, `FizzleReason::{TargetProtected, NoFront}`,
  `WinKind` on `ResolveEnd::Won` and `LogEntry::Won` (serde default
  `LastStanding` for archived days), `points_left` reading ordered points
  on an `Orders` game. `STATE_VERSION` stays 2; every new field is
  serde-default so season 1's row keeps parsing. Add a test that the
  season 1 state shape (a fixture JSON with none of the new fields) parses.
- `night.rs`: `place_order`, `withdraw_order`, `projection`, `share`,
  `resolve_night`, `check_end_orders`. Pure. The front and the sea reach
  are computed from `land_links`, `landmasses`, `coastal` and
  `WorldMap::distance_km`; no `ReachIndex`.
- `night_test.rs`, on `testmap` (12 territories) unless a case needs Earth:
  the sanity points of 2.4; placement refusals (each `OrderRejected`
  variant once) and that a refusal changes nothing; replacing an order
  returns its points; withdrawing returns them; the allowance freezes at
  the first order and the bank survives it; walls ordered tonight hold
  tonight; engagement cuts support; two takers on one target with the
  stronger success winning; an exact tie bounces and the holder keeps it;
  a neutral contested by two; a take that becomes a claim because the
  holder left; the two fizzles; a landing with no land front; encirclement;
  a fall names the taker; `dominating_nights` counts up and resets;
  `Dominated` after three nights and not before; the clock ending on the
  right night with the highest share and the tie-breaks; `LastStanding`
  still gated by `min_active_days_to_win`; the per-target seed stream (one
  more fight elsewhere does not change this fight's roll); determinism
  end to end; walls decay back toward 1 with dormancy.

### Step 3: the service

Files: `svc.rs`, `svc_test.rs`, `late-core/src/models/realm_game.rs`, one
migration.

- `place_order_task`/`place_order_at(user, game, order, now)` and
  `withdraw_order_task`/`withdraw_order_at`: load, refuse during the
  muster, `night::place_order`, persist under the row CAS with the retry
  loop `act_at` has, send `RealmEvent::OrderPlaced { game_id, user_id,
  target, points, probability }` to the actor only, publish. `act_at`
  refuses on `Orders`; `place_order_at` and `withdraw_order_at` refuse on
  `Instant`.
- `close_day(client, game, state, map, day)`: the one function that turns a
  day over, used by `sweep_game` and by the lazy path (an order placed
  after the hour before the sweeper got there). For `Orders`: take the
  opening board, `resolve_night` on `last_day.day`, then `roll_day`,
  persist (`advance_day_cas` or `finish_cas` by the end), archive the
  closed day with both boards, `announce_day_rolled`, news. For `Instant`:
  what `sweep_game` and `act_at` do today, moved here unchanged. A night
  runs exactly once: it is inside the CAS write that advances the day, so
  a lost CAS means somebody else ran it.
- Migration in `late-core`: `realm_days.board_open JSONB NULL`;
  `archive_day` takes both boards, COALESCE on each. `RealmDay` gains the
  column.
- Telemetry, in `late-ssh/src/metrics.rs` with the no-op twin, labels from
  closed enums: `record_realm_night(RealmNightOutcome { Resolved, Won,
  Dissolved, Failed })`, `record_realm_fights(RealmFightResult { Taken,
  Held, Bounced, Fizzled }, count)`, `record_realm_order(RealmOrderOutcome
  { Placed, Withdrawn, Refused })`. One `tracing::info!` per night with
  structured counts, one `tracing::error!` per failure at the orchestration
  level, a span `realm.night` around the close in the sweeper.
- `svc_test.rs` (real `test_db`): an order lands in the row and the
  actor's event; a refused order writes nothing; the sweeper resolves the
  night at the game's hour and archives both boards; an order placed after
  the hour resolves the night first and lands on the new day; two replicas
  closing the same day run the night once (the CAS loser's state matches
  the winner's); the clock finish pays through `settle_payouts`; a
  `Dominated` finish headlines the lounge with the kind; the season loop
  opens season 2 on `CAMPAIGN`; `act_at` refuses on an `Orders` game.

### Step 4: the board

Files: `state.rs`, `input.rs`, `ui.rs`, `map_ui.rs`, `targets_ui.rs`,
`overview_ui.rs`, `history_ui.rs`, `history_ui_test.rs`, `log_ui.rs`,
`results_ui.rs`, `landing_ui.rs`, `live.rs`, new `orders_ui.rs`.

- `RealmState` learns the engine from the loaded snapshot's `resolution`
  and routes `a`/`f` accordingly; on `Orders` they open an order prompt in
  the rail (points or levels, `+`/`-` and digits, odds recomputed per
  point through `night::projection`, Enter places, Esc cancels). Pre-flight
  refusals come from `night::place_order` on the local state, the
  `act_on` pattern, so the round trip never carries an order the board
  could refuse.
- New `RealmView::Orders` in the Tab cycle after Targets: today's orders
  (target, kind as the board stands, points, odds now), points left,
  `j`/`k`, `x` withdraws the highlighted order (on every other view `x`
  keeps saying a running realm cannot be left), `g` frames; below, the
  dispatch panel (2.8). `o` jumps to this view from any other, and the
  footer leads with "o orders (N pts left)" on every view. `o`, `p`, `.`
  and `,` are unbound on the board today.
- Header: "day 12 of 28 · 4 pts unordered · night falls in 04:28 (18:00
  UTC · 19:00 local)". The rail's territory card shows walls, the
  projection, and the viewer's order on it if any. Targets view: the odds
  column is the projection at one point; the hops column becomes the front.
- Overview: standings sorted by share, columns land, share, walls,
  dominating (N of 3), days, status; the threshold and the clock spelled
  out.
- History: the replay (2.8). `history_ui_test.rs` covers a stepped night:
  which territory is bright on step N, the caption, day one first still.
- Results card and landing card: the win kind; the card's pitch says how a
  campaign is won.
- Watching: a visitor sees no orders, no prompt, the dispatch only for the
  night as a whole (every `Fought` line), the replay in full.

### Step 5: twenty colours

Files: `state.rs` (`PLAYER_PALETTES`), `map_ui.rs`, `join_ui.rs`,
`resolver.rs` (`PALETTE_SLOTS`).

- `PLAYER_PALETTES: [PlayerPalette; 20]`, each with three shades. Hues
  spaced for a dark terminal; the existing ten keep their names and their
  middle three shades so season 1 draws as before. Walls map to shades as
  1 → pale, 2 to 3 → mid, 4 to 5 → dark. At `BORDER_MAX_SCALE` or finer,
  the wall level (2 to 5) is drawn as a digit at the territory's centre
  cell in the owner's dark shade, so a fortress reads at the zoom where a
  country is more than a pixel. The colour picker lays twenty out in two
  columns. The existing test ties `PLAYER_PALETTES.len()` to
  `PALETTE_SLOTS`.

### Step 6: docs

- `RULES.md`: a new "Orders (campaign)" section holding 2.2 to 2.7 as they
  shipped, the knob names and values, and the instant engine's section
  marked as the frozen ruleset's. No history, no "before".
- `CONTEXT.md`: `night.rs` and `orders_ui.rs` in the files table, the
  shape paragraph for the night (`close_day`, the once-only rule), the new
  invariants (a night runs once, inside the CAS that advances the day; the
  board's projection and the night's force come from `night::projection`;
  orders are secret by presentation), the tests section, the deferred list
  (delete the instant engine; animated replays; the deterministic variant).
- `UI.md`: the orders view, the prompt, the dispatch, the replay, the
  palette and the wall digit.
- `AUDIENCE.md` is not touched.

## 4. Run 2: the full-scale simulation

A whole-season simulator over the real `night.rs`, the way
`simulation_test.rs` wraps the instant engine. Lives in `night_sim_test.rs`
beside `night.rs`, wired `#[cfg(test)]`, every heavy case `#[ignore]` and
run on purpose:

```
make test-llm ARGS="-p late-ssh --lib -E 'test(realm_campaign_sim)' --run-ignored ignored-only --no-capture"
```

Parameters through env with required reads that fail loudly when unset
(`REALM_SIM_SEEDS`, `REALM_SIM_OUT` for a CSV path); the quick,
un-ignored cases use fixed small numbers so the suite stays fast.

- **Policies** (each picks orders by `night::projection`, allocating its
  whole budget): `settler` (neutral land, nearest and best odds, one point
  each until odds fall under 70%, then two); `warlord` (the best-odds enemy
  territory, points until 75%); `cautious` (only takes above 80%);
  `absentee` (two days on, three off); `turtle` (walls its whole border to
  3 before taking anything); `frontier` (walls the border territories that
  touch a rival to 3, then concentrates five points per take on the
  weakest adjacent enemy); `coalition` (always targets the share leader's
  weakest front when adjacent, else behaves as `frontier`). Policies are a
  closed enum, matched exhaustively.
- **Rosters**: 2; 5; 10; 20 at the start; 10 at the start then 5 on day 5
  and 5 on day 10 (the join path through `night`'s spawn rule, with the
  newcomer shield); 10 then 5 on day 20 of a 28-day clock. Each on Earth and
  on a 300-territory drawn world, at the `fast` pace, 20 seeds each, mixed
  policies drawn by seed.
- **Invariants after every night** (assertions): the dead hold nothing;
  no budget overspent; walls within 1 to max; `dominating_nights` only
  moves by one; a bounce leaves the holder; a `Fought` entry's probability
  equals `projection` recomputed on the opening board with engagement
  applied; the end comes only from the night; determinism (same seed, same
  log).
- **Measurements**, printed as one table and written to the CSV: days to a
  result and the win kind; the day the eventual winner first led the share
  (decisiveness); how often the day-5 leader wins; how often the clock
  decides it; bounces per night; fizzles per night; wall points spent by
  winners against losers; territories lost by `frontier` against
  `warlord`; neutral land left at day 10; share of the roster alive at the
  end; what an absentee holds at the end.
- **Acceptance** (the sim fails on these; everything else is a number to
  read): at `fast` with 10 players a result lands between 14 and 28 days
  and never only by the clock in more than half the seeds; 20 players end
  inside the clock; 2 players end; the day-5 leader wins under 60% of
  seeds; two pure turtles never win; `frontier` loses fewer territories
  than `warlord` over a season; an absentee is alive on day 10; late
  joiners on day 10 hold more than their spawn when their shield ends.
- **Tuning**: move the 2.4 and 2.7 knobs until acceptance holds across the
  rosters, one knob at a time, and record the final values and the measured
  table in `RULES.md` the way the current observed numbers are recorded
  (`2p resolves in ~47 days`). Also record which knobs were moved and why,
  as a sentence beside the value, not as a changelog.

## 5. Review checklist

What the owner's review reads for, run by run:

- Nothing committed; the tree holds the change.
- Every mechanic knob is a ruleset field, none a literal in `night.rs`.
- `night.rs` imports nothing from db, tokio or the clock.
- One place turns a day over (`close_day`), and a night runs inside the CAS
  that advances the day.
- The board's odds and the night's odds share `night::projection`.
- Season 1's row parses (the fixture test), and every new field is
  serde-default.
- Closed enums everywhere a kind is a choice (`Resolution`, `Order`,
  `OrderRejected`, `WinKind`, the policies, the metric labels), matched
  exhaustively, no catch-all arms.
- Logging, metrics and spans sit in `svc.rs`, none in `night.rs` or the
  UI; the no-op twin compiles.
- Tests beside the files they test; the named sanity points assert the
  numbers in 2.4; no test weakened to pass.
- Docs current: `RULES.md`, `CONTEXT.md`, `UI.md`, and this file deleted
  once everything in it has moved.
- Prose and comments: no em dashes, errors and log strings lowercase.
- The report names where the repo's convention overrode the guide.

## 6. Out of scope

- Animated armies and arrows on the replay.
- Deleting the instant engine (after season 1 ends).
- A deterministic fight variant (an `Orders` sub-option, later).
- Splitting Earth's largest countries into regions (atlas work).
- Alliances as a mechanic. The chat is the alliance.
