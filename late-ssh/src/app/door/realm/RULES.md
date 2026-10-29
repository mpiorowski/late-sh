# Realm: rules and lifecycle

The game as the engines (`night.rs`, `resolver.rs`, `rulesets.rs`) and the
service's clock (`svc.rs`) play it. Every knob named here is a `RealmRuleset`
field or a `rulesets.rs` constant; the map is context in `CONTEXT.md`, the
worlds in `MAPS.md`, the screens in `UI.md`.

Two rulesets, two engines, chosen by one field (`RealmRuleset::resolution`):
**campaign** (`CAMPAIGN`, `Resolution::Orders`) is what the house opens:
orders written by day, resolved together at the night. **Standard**
(`STANDARD`, `Resolution::Instant`) is the frozen ruleset of the world that
was standing when the campaign shipped: each action resolves the instant it
is taken. A game keeps the engine it was opened with for life. Everything
under "Lifecycle" is shared; "Orders" is the campaign's moves and its win,
"Moves, reach and walls" the standard ruleset's.

## Orders (campaign)

Why it exists, from season 1 on the standard ruleset: whoever spent their
points at the reset took the free land and the soft targets before anyone
could answer, so orders resolve together and nobody gains by being at the
keyboard when the points come back. Strength was the whole empire's size,
so the leader became untouchable once they led; force here is what stands
around a territory, and a big empire has a long border. The late game was
130 more countries one roll at a time for a player holding 71% of the
world; domination ends a season once it is decided. Walls were a losing
trade by design; here they are the strength, and size is the score.

- **The day: orders.** The budget is the shared one (below): tiers by the
  involved count, `[(4, 5), (7, 4), (10, 3), (20, 2)]` on the campaign
  (the standard's `(4, 7), (7, 5), (10, 4)` made every player turn over
  nearly three territories a night at the fast pace, which ate anyone who
  missed a weekend), banking, the allowance frozen at the first order of
  the day, plus what the land pays: `land_points` (4.0, pace-scaled like
  the tiers) per hundred territories held, rounded down, banking nothing
  (`land_points`, `DayStanding::land`). At the fast pace that is one point
  per eight territories: the mild lift that lets a lead be pressed home,
  since with every budget the same size the simulation's leaders plateaued
  at twice the average share from mid-season and the clock decided every
  season. An order is
  one of two things (`resolver::Order`): `Take { target, points }` (one or
  more points; whether it is a claim or an attack is decided at the night,
  from who holds the target then) or `Fortify { target, levels }` (a point a
  level, on land you hold, up to `fort_max_level` 5). One order per target
  per player: ordering a target again replaces the order and refunds its
  points first. An order spends its points when placed (`actions_used` is
  points ordered today) and a withdrawal gives them back. The night drains
  every order, so an order standing on the row is always today's.
- **Placing is refused for free** (`OrderRejected`, `night::place_order`):
  not alive, an unknown target, a take on your own land, walls on land that
  is not yours or past the ceiling, no front on the target, more points than
  are left, protection in either direction, and the muster (a service
  rule). A day with at least one order placed is an active day.
- **Orders are secret until the night.** The board draws the viewer's own
  orders only. The terminal is server-rendered and no client reads raw
  state, so this is a presentation rule; a raw state API would have to strip
  other players' orders.
- **The night** (`night::resolve_night`, once per day, for the open day,
  with that day's seed, before the next day opens). Everything is read from
  the board as the night finds it; nothing resolved tonight changes another
  fight's numbers.
  1. Every alive player's orders are gathered. A take is re-checked against
     the opening board (`check_take`): a target now its own fizzles
     (`AlreadyYours`), one with no front left fizzles (`NoFront`), one a
     phase now protects fizzles (`TargetProtected`). What its own board
     showed for it is kept beside it (`TakerRoll::shown`).
  2. Walls go up, in roster order: a wall ordered today holds tonight.
  3. Every surviving take engages its target: an engaged territory lends its
     neighbours no support, whether or not the take on it lands.
  4. Every take is priced and rolled, one roll per order however many points
     it carries, before any lands. Per target: among the successful rolls
     the highest force takes it, an exact tie of the highest **bounces** and
     the holder keeps it, no success holds. A taken territory's walls drop
     to 1. One `LogEntry::Fought` per target: the holder before, who took
     it, the defence, and per taker `{ points, from, force, probability,
     shown, success }`, `from` being their strongest front territory.
  5. Falls: a player holding nothing is `Eliminated`, by whoever took their
     last territory.
  6. The end (below), checked once.
  The night's entries go on the day's log and, on their own, onto
  `RealmGameState::last_night`, so the dispatch is one read. Rolls use
  `SplitMix64` on a per-target stream, `blake3(day_seed ‖ target)`, so one
  more fight elsewhere never changes this one's roll.
- **Force, defence, odds** (`night::garrison`, `force`, `odds`, `project`).
  `w` is the territory's size weight (0 to 1).
  - Garrison: `g(T) = walls_eff × (1 + garrison_weight × w)`,
    `walls_eff = 1 + (walls − 1) × garrison_per_wall × decay(holder)`; free
    land is walls 1 with no decay. `garrison_weight` 1.0, `garrison_per_wall`
    0.5 (walls 3 hold twice a bare territory, walls 5 three times).
  - Front: the `walls_eff` of the taker's land bordering `T`, plus
    `sea_front` (0.5) × the `walls_eff` of their coastal holdings within
    `sea_reach_km` (3000) of a coastal `T`. Each holding counts once, by land
    when it borders. No front, no order: there is no long-range strike and
    no march past rivals in this engine (`long_range_strikes` is not read),
    while `landlocked_sea_attack` still decides who may launch from an
    inland holding.
  - Force: `A = points × (1 + front_factor × front) × landing × encircle`,
    `front_factor` 0.25 (0.5 as written made a walled front double a take
    as well as hold, and the walled won every stalemate), `landing_factor`
    0.75 when nothing of the taker's
    borders `T` by land, `encircle_factor` 1.5 when every land neighbour of
    `T` is theirs.
  - Defence of held land: `D = g(T) + support_factor × Σ g(n)` over the
    holder's land neighbours of `T` that are not engaged tonight,
    `support_factor` 0.5. Of free land: `g(T) × neutral_factor` (0.6), no
    support. Nothing is free: a microstate is one point at good odds, a
    continent-sized country is a real fight, and two players taking the same
    free land the same night resolve like any contested target.
  - Odds: `A^k / (A^k + D^k)`, `contest_exponent` 2.0, clamped to
    `[prob_floor, prob_ceil]` (0.05, 0.95): 80% at 2:1, even at 1:1, 20% at
    1:2. `fort_defence_per_level` and `fort_transit_per_level` are not read;
    walls enter through the garrison only.
  - `night::projection(state, map, actor, target, points, day)` is the one
    function the board and the night both price with: the board's tonight is
    the viewer's own orders (their walls up, their targets engaged), the
    night's is everyone's. The two differ only by what the board could not
    see, and the dispatch names it.
- **Protection** reads as on the standard ruleset (`attack_grace_days`,
  `distant_attack_grace_days`, `newcomer_shield_max_days`, below), with the
  campaign's own lengths: 6, 9 and 8 days as written, so 2, 3 and 3 at the
  fast pace (the standard's 1, 3 and 4 scale to a single day each, and the
  land rush then carved up anyone away on day three). **New** takes free
  land only and cannot be taken from; **ramping up** takes from, and is
  taken by, land neighbours only, so no landings either way; **involved**
  takes anything with a front. The doors stay open to `join_max_claimed`
  0.9 (0.5 shut them on day two). A win needs `min_active_days_to_win` 40
  as written, 14 at the fast pace: half the season, so the land rush alone
  never ends a twenty-player world (it did, on day 7). Checked at placement
  and again
  at the night.
- **Twenty players.** `max_players` 20, `PALETTE_SLOTS` 20. Everything sized
  by the roster reads one of two numbers: `involved_count(day)` (the budget
  tier: the living, past their opening days) or `war_roster(day)` (the
  domination bar: everyone who has been past their opening days, the fallen
  included). Knocking rivals out never raises the winner's own bar, and a
  late arrival lowers it only once they are out of protection and able to
  fight the leader: measured by `start_player_count` (the most the roster
  ever held) five arrivals on day 5 handed a leader standing just over the
  lower bar the world on day 9.
  Drawn worlds have 300 territories (`GeneratedMapSpec::default`); Earth has
  242.
- **Winning**, checked at the night only, never on an order
  (`night::check_end_orders`, `WinKind` on `ResolveEnd::Won`, `LogEntry::Won`,
  the news and the results card). A player's **share** is the land they
  hold over the whole map, free land included, each territory counting
  `1 + weight` (`night::share`, also what the standings sort by): a giant is
  worth twice a speck, about what it costs to take, and no more. Raw
  population or area made the four biggest countries the whole game: in the
  simulation a twenty-player season was dominated on day 4 by whoever
  claimed Russia, Canada, the USA and China first. In order:
  - **Last standing**: one player alive, and `can_win`.
  - **Dominated**: holding the bar for `domination_nights` (3) nights running
    (`RealmPlayer::dominating_nights`, +1 a night at or over the bar, back to
    0 under it), and `can_win`. The bar is `domination_share_tiers` by
    `war_roster(day)`: `[(4, 0.50), (6, 0.36), (8, 0.27), (10, 0.21),
    (13, 0.165), (16, 0.135), (20, 0.11)]`, each about twice the average
    player's share. The three nights are the coalition's window.
  - **Season over**: the night that closes `start_day + season_days` (day
    `start_day + 1` is the first played) ends it after its fights: the
    biggest share among those who `can_win` takes it, a tie going to the
    final ranking; nobody eligible dissolves it. `season_days` is 80 as
    written, pace-scaled with a floor of 7: slow 80, normal 48, fast 28,
    epic 120, blitz 16.
  Payouts, ranking and the pot are the shared ones; a win on the clock pays
  like any other.
- **The knobs above are what the simulation settled on.**
  `night_sim_test.rs` plays whole seasons at the house's rules and pace
  over the real night: seven policies (settler, warlord, cautious, absentee,
  turtle, frontier, coalition, each pricing with `projection`; a turtle
  walls its whole border and settles free land but never strikes a player,
  and everyone but a turtle presses whoever stood at the domination bar
  last night, by land or by sea, the way the standings' `dom 1/3` asks the
  field to) on rosters of 2, 5, 10 and 20, ten joined by five on day 5 and
  five on day 10, ten joined by five on day 20, and five with two pure
  turtles, on Earth and on a drawn world. After every night it checks that
  the dead hold nothing, no budget is overspent, walls stay in bounds, a
  run of dominating nights moves by one or starts over, a bounce leaves the
  holder, every fight's odds are `projection` on the board the night opened
  on with its engagements, and nothing only a night may do happens by day;
  and the same seed plays the same season. The ignored full-scale run
  reports every missed bar together: ten players and twenty each reach a
  result between days 14 and 28, the clock deciding at most half; a duel
  ends; the day-5 leader wins under 60% of seasons; a pure turtle never
  takes the world and is handed it at the bell in fewer than one of its
  seasons in five; a frontier loses fewer territories a season than a
  warlord; three in four absentees who were there for the opening are
  alive on day 7; most day-10 arrivals hold more than their spawn when their
  shield ends. Measured at 8 seeds a roster (days min/mean/max; results
  dominated/clock/last standing/dissolved; the biggest share after days 7,
  14 and 21; takes a night):

  | roster | world | days | dom/clk/ls/x | top 7/14/21 | takes |
  |---|---|---|---|---|---|
  | 2p | earth | 14/20/28 | 6/2/0/0 | 24/44/35 | 11.0 |
  | 5p | earth | 24/28/28 | 2/6/0/0 | 18/27/33 | 18.1 |
  | 10p | earth | 14/23/28 | 4/4/0/0 | 15/20/19 | 22.7 |
  | 20p | earth | 14/18/28 | 7/1/0/0 | 11/14/13 | 26.6 |
  | 10p+5@5+5@10 | earth | 14/16/24 | 8/0/0/0 | 15/16/15 | 25.6 |
  | 10p+5@20 | earth | 14/23/28 | 5/3/0/0 | 15/20/19 | 22.7 |
  | 5p+2turtles | earth | 16/26/28 | 3/5/0/0 | 19/32/34 | 16.2 |
  | 2p | drawn | 14/21/28 | 5/2/0/1 | 21/42/42 | 12.5 |
  | 5p | drawn | 17/25/28 | 3/5/0/0 | 18/31/36 | 17.3 |
  | 10p | drawn | 14/22/28 | 6/2/0/0 | 14/21/19 | 22.1 |
  | 20p | drawn | 14/20/28 | 5/3/0/0 | 11/13/11 | 26.0 |
  | 10p+5@5+5@10 | drawn | 14/20/28 | 6/2/0/0 | 14/16/15 | 24.4 |
  | 10p+5@20 | drawn | 14/22/28 | 6/2/0/0 | 14/21/19 | 22.2 |
  | 5p+2turtles | drawn | 17/25/28 | 4/4/0/0 | 16/29/32 | 14.2 |

  Share of the world held at the end, per player of each policy over every
  season: cautious 11.5%, coalition 11.5%, frontier 10.8%, warlord 10.8%,
  settler 10.1%, turtle 7.7%, absentee 2.9%. What moved from the values as
  first written, and why, is beside each knob above: the share by weight
  (raw population and area made four countries the whole game), the tiers
  of the domination bar (every budget being the same size caps a lead near
  twice the average share), `war_roster` under the bar (five arrivals on
  day 5 handed a leader the world on day 9), `land_points` (without it the
  clock decided every ten-player season), the budget tiers (three
  territories a night turning over per player ate the part-timers),
  `garrison_per_wall` and `front_factor` (a border dug to 3 then won by
  never being attacked), `sea_reach_km` 3000 (1500 left whole continents
  nobody could reach, and a settler alone on one held a quarter of the
  world), `join_max_claimed`, the protection days and `min_active_days_to_win`
  (above). Rerun it after moving anything:
  `REALM_SIM_SEEDS=8 REALM_SIM_OUT=/tmp/realm-sim.csv make test-llm
  ARGS="-p late-ssh --lib -E 'test(realm_campaign_sim)' --run-ignored
  ignored-only --no-capture"`.

## Moves, reach and walls (standard: the instant engine)

How the standard ruleset's actions are priced and resolved, on the spot.
None of it is read by the campaign's night except the phases and the decay.

- **Attacking is the hard move.** Claiming free land is cheap; taking held land is not, and it gets harder the bigger the holder is: the strength ratio is raised to `attack_strength_exponent` (1.6), so a sprawling empire is a genuinely bad target and a cornered player is a soft one. On top of that an **opening truce** protects the early game — `attack_grace_days` (1) forbids every attack, `distant_attack_grace_days` (3) forbids attacks past your own border — both counted from `state.start_day`, both ruleset data. Claims are never blocked: the truce is there so players develop before anyone can be knocked out. `RealmGameState::attack_window(day)` is the single answer to "what is allowed right now", used by the engine, the rail, and the target table alike.
- **Routes are per landmass** (`Territory.landmasses` / `land_links`, built by `map::compute_landmasses` from the grid). A border only helps you if you are standing on the same landmass as the border: France borders both Brazil and Spain (French Guiana is France), and without the landmass split the atlas would be a dry road from South America to Europe, showing Europe as "on your border" from Brazil at full odds and letting a landlocked player walk off their continent. `ReachIndex` keys its Dijkstra on `(territory, landmass)`; holding a multi-part country means standing on each of its landmasses, which is right — you hold ground on both. Only landmasses a territory has a *border* on are kept (Earth is tens of thousands of islets; carrying them all would blow up the node set for nothing, and anything reached from a lone islet is reached by sea anyway).
- **No route is not a cheap route.** `RouteMode::None` / `Reach::UNREACHABLE` / `ActionRejected::NoRoute`: another landmass with nothing to sail from or nothing to land on is refused, not priced. Pricing it as `Reach::ADJACENT` (cost 1.0, "on your border", full odds) would hand out free crossings; that fallback covers only a player holding nothing at all.
- **The newcomer shield scales with the world.** `RealmPlayer.joined_claimed` (percent of the map already taken, frozen at join) plus `newcomer_shield_max_days` (4 on standard, pace-scaled) stretch the `New` phase: the plain `attack_grace_days` for somebody who was here at the start, up to that plus the max at the join door (`join_max_claimed`, 50%), pro rata between. `state.shield_days(user)` is the one answer; the rampup window keeps its own length and moves along behind it. The field is serde-default 0, so a stored game without it keeps the grace it was set up with. In the sims a latecomer comes out of protection holding ~20-28 territories instead of the 1 they spawned on — what happens to them afterwards is endgame pressure, not arrival handicap.
- **Spawns prefer a land border, then free ground next to it** (`svc::spawn_player`, seeded from the game and the revision at the join, so two arrivals never land by the same draw). A lone island is a fine place to play from and a hopeless place to start from once sea crossings are real, and a spawn walled in by other people's borders has nothing to spend its shield on; both preferences fall back when nothing else is free.
- **Reach is what a route costs, not how many steps it has** (`ReachIndex`). Two ways to arrive, cheaper one wins:
  - *Overland*: Dijkstra from everything you hold, charging for each territory the path passes **through** — your own land is free, empty land costs `free_transit_cost` (1), and a rival's costs `enemy_transit_cost` (3). Marching past someone is the expensive way round, which is what makes a border worth holding. The target itself is never charged; you are taking it, not crossing it.
  - *By sea*: from a coastal holding, `km / sea_km_per_cost` (2000km per point). A strait is nearly free, an ocean is not. The target must have a coast — an inland country cannot be taken off a boat — and whether an inland *player* may sail at all is the `landlocked_sea_attack` option.
  - Odds pay `distance_decay^(cost−1)`, continuous, so a border move (cost 1) pays nothing and the sea's fractional costs work properly. An island player has sea costs like everyone else; there is no stranded bonus.
- **Fortifications** (`OPTION_FORTIFY`, on by default). `f` — and only `f` — spends a point on land you hold to raise it a level; the attack key refuses your own ground with a nudge rather than quietly digging, because a point spent on walls is a point not spent taking ground and nobody should spend one by reaching for the wrong key. It raises it a level, to `fort_max_level` (5); level one is simply holding the place and is never stored. Each level past the first multiplies an attacker's odds by `fort_defence_per_level` (0.88) and adds `fort_transit_per_level` (0.4) to the cost of marching past — so a fortified pass is worth going *around*, which the routing will do by itself. Taking a territory levels its walls, so offence is rewarded with a weak position and a frontier never sets. Walls fade with their holder's dormancy (the decay multiplier walks the bonus back towards nothing), because an abandoned fortress must not stall a realm for everyone else.
  - **The economy is deliberately unfavourable**: four points of wall buy fewer than four points of attack (asserted in `resolver_test`), because a realm is won by taking ground. Digging in is for the pass you mean to keep or a day with nothing better. `simulation_test::digging_in_is_not_a_way_to_win` plays two pure turtles in a five-player game and requires that they never win and that the realm still ends — that test is the balance, not the arithmetic above it.
  - Shown everywhere, to everyone, on both engines: the map draws walls as a step on that player's colour ladder (`state::PLAYER_PALETTES`, three shades: 1 pale, 2-3 mid, 4-5 dark), so solid-looking means solid and it is the only signal that survives a one-pixel country; at border zoom the level itself is drawn as a digit on the territory's centre. The target table has a `walls` bar column and the rail card draws the same bars (`map_ui::fort_bar`, shared so the two cannot drift), the standings total each player's walls so a turtle is visible, and the log records each dig.
- **Options** (`rulesets::OPTIONS`, frozen per game at their roster defaults, stored in the snapshot): `long_range_strikes` (on by default) and `landlocked_sea_attack` (off). They are data, so adding one is an array entry plus a place that reads it; a game frozen before an option existed takes the roster default rather than a rule nothing enforces. With long-range **off**, a move must stay inside `short_reach_cost` (2) — and that cap **lifts entirely for a player with nothing in range** (`anything_within`), so a short-reach game can never box someone into having no move.

## Lifecycle (both rulesets)

- **A world musters for `REALM_MUSTER` (a day) before anyone can act.** Opening one announces it — a banner to every session (`SeasonOpened`, derived from the snapshot, so on every replica) and a `system` headline in #lounge (`ActivityKind::RealmSeasonOpened`, carrying `muster_label()` as `opens_in` so the deadline is Realm's own; `filter.rs::lounge_headline`: "A new realm rises: season N ... the first point is spent in a day. Press 3, then Realm, to take your spot") — and for that day the world is visible and joinable but frozen: map, spawns and roster all readable, every action refused with the countdown. A day, because the headline is for the whole house and every time zone should get to read it and take a spot before the first point is spent. The freeze is the **one rule the engine does not hold**: `resolver.rs` has no clock at all (it is day-quantised by design), so threading a wall-clock instant through `apply_action` and every simulation playout to enforce something that expires in minutes would be the wrong trade. It lives in `svc::act_at` and, for the pre-flight refusal, `state::act_on`/`fortify_on`. `svc::muster_left` is the single answer; there is no new column, since `realm_games.created` already says when the clock started. `act_at(now)` exists so tests need not wait the muster out (`sweep(now)` precedent); tests that are not about the muster wind `created` back two days (`svc_test::open_the_doors`).
- **There is no starting gun.** Opening a world makes it live, with nobody on it. Everyone arrives through the gates (`j` on the board's map while they are open, the colour picker, `join_game`), spawning on arrival into a war already in progress — no waiting room, no roster to assemble, no synchronised start. `join_game` accepts while the roster has room and the map is under `join_max_claimed` (50%); past that a newcomer would only be food, so the doors close (`RealmGameState::joinable`, surfaced as `RealmGameItem::joinable`/`claimed`). A realm short of a rival (`short_of_rivals`: fewer than `min_players` on the roster) keeps them open however much is taken, because it cannot be won until somebody arrives; closing them stranded a lone player in a war with no end but the dissolve.
- **Protection is per player, not per game** (`PlayerPhase`, counted from each player's own `joined_day`, which is the day their clocks start: the day they arrived, or for an arrival during the muster the day the gates open, `svc::first_playable_day`, so the frozen day never eats a shield or counts as quiet): **new** for `attack_grace_days` (1) — free land only, and untouchable; **ramping up** until `distant_attack_grace_days` (3) — neighbours only, and only neighbours may come for them; then **involved**, where anything goes both ways. `attack_allowed(actor, defender, hops, day)` is the single check, and it refuses in both directions, so arriving on day forty buys the same footing the founders had.
- **The daily budget follows the involved**, not the roster (`daily_base` → `involved_count`): a realm whose players are all still finding their feet pays the most generous tier, which is right when one or two people are alone on a whole world.
- **The day turning over is announced twice.** Every player on the roster gets `RealmEvent::DayRolled` — derived by each replica from `last_resolved_day` advancing in its snapshot, and carrying the game's *name*, because the session raises it as a banner wherever they are in the app ("The Realm calls you! (name)"), not only with the board open. And the lounge ticker gets one line (`svc::announce_day_rolled`, from the write that won the CAS), `ActivityKind::RealmCalls` → "realm <name> calls its players", attributed to no user because the summons is the game's. That line is gated on more than one living player: a realm somebody made and sat in alone would otherwise post about nothing, daily, for as long as the row lives. The repeat key is `realm-calls:<game>:<day>`, so a rollover announced twice posts once.
- **Reset hour** (`realm_games.reset_hour_utc`, 0-23, held by the column's CHECK): when this game's points come back, every day. The house world refills at `HOUSE_RESET_HOUR_UTC` (18), an evening in Europe and an afternoon in the Americas; the column stays per row so a world's hour is its own data. Every surface writes it in both clocks (`svc::reset_hour_label`, `RealmState::viewer_tz`, mirrored from `App` like the chat pane's zone).
- Actions/day follow the involved count (above), by tier: 2-4 → 7, 5-7 → 5, 8-10 → 4 on the standard ruleset, and 2-4 → 5, 5-7 → 4, 8-10 → 3, 11-20 → 2 on the campaign, plus what its land pays (`actions_per_day_tiers`, as written; the pace's `action_scale` multiplies them). A point is spent on the attempt (an order, on the campaign), won or lost; a *refused* one costs nothing.
- Probability on the standard ruleset (pure fn `action_probability`; the campaign's odds are under "Orders"): territory weight `w = (ln pop + ln area normalized)/2`; strength `S = 1 + Σ held weights` (the defender's after decay); claim `0.90 − 0.25w`; attack `(0.45·(2S_att/(S_att+S_def))^1.6 − 0.15w) × fortification`; then `× distance_decay^(cost−1)`; clamp to [0.05, 0.95] on your border, [0.01, 0.95] beyond it. The UI's projected % is the same function on the same state — it is the real number, it can just go stale the moment someone else acts.
- Exits: a voluntary leave frees the territories (the last player leaving cancels the world, and the next sweep opens the next one); nobody is kicked for going quiet, a dormant empire just decays (`decay_multiplier`) until someone takes it; losing your last territory = **Eliminated**, stays spectator. On the standard ruleset one alive player left (every rival conquered) finishes the game, as does someone owning every territory — the same finish reached from either end; the campaign adds domination and the clock (under "Orders"). Free land left on the map does not hold it open. A world nobody has marched into yet stands until somebody does: an empty roster is an invitation, not an abandoned game, so the first reset never dissolves it mid-muster. A roster with nobody alive dissolves it (no winner, no payouts), as does a realm every living player has been quiet in for `REALM_ABANDON_DAYS`.
- **Pace** (`rulesets::PACES`, `HOUSE_PACE` for the house world, frozen into the snapshot by `RealmRulesetSnapshot::at_pace`). Orthogonal to the ruleset: the rules say how a conquest works, the pace says how long it takes. `action_scale` multiplies the daily points, `day_scale` multiplies every window measured in days (opening protection, the week a win takes, dormancy, banking), and `pot_scale` multiplies the prize. Floors keep a blitz playable — a win you could reach in a day, or an opening with no protection, would be a different game rather than a faster one. Measured on the standard ruleset with five mixed policies: **epic 158 days, slow 68, normal 39, fast 17, blitz 12** (`simulation_test.rs`). `slow` is the rules exactly as written, and `normal` is the reference tempo whose pot is the plain per-player rate.
- **A finished world stays listed for `REALM_RESULTS_HOURS` (24)**, so everyone gets to read the result on the hub card and the strip while the next world musters, then drops out of the listing. The row is never deleted: the history and the archived day logs outlive the listing.
- **Ranked payouts** (migration 194, rescaled by 197). The pot is computed per game: `REALM_POT_PER_PLAYER` (3000) × `paying_players`, everyone who played as long as a win takes (`min_active_days_to_win`), floored at two. Counting the roster would let a colluder walk alts in without taking a turn, and counting the podium bar (`min_payout_active_days`) let nine alts clicking a few days each bank a whole 60/25/15 split; at the win bar every padded body costs as many days as the conquest. It is a mitigation, not a cure: the pot still scales with headcount, which is what alts fake. The cost accepted: an honest player knocked out before the win bar does not grow the pot. So a duel is worth 6k and a ten-player war 30k — placing in a crowded game is the harder feat and pays like it. Multiplied by the pace's `pot_scale` (epic 3.0 → blitz 0.25) and capped at `REALM_POT_MAX` (60k, because ten players at the epic tempo would otherwise compute a prize bigger than anything else on the platform pays). Split by tier (`payout_plan`): ≤3 winner takes all; 4-6 → 70/30; 7+ → 60/25/15, rounded to the nearest 50. That puts a realm win in the door-game milestone class (Green Dragon 10k, A Dark Room 15-20k) instead of below a daily Asterion escape (4000). The six `reward_templates` rows still own the claim policy (`per_event`, one payment per rank per game) and the game/payout_kind identity; `reward_chips` on them is the floor of each tier, and `credit_per_event_reward_template_amount` passes the computed figure. Ledger reason is `ChipMove::RealmConquest` (`realm_conquest`), which counts as earnings on the Top Chips board.
- **The pot is settled after the finish, and recorded** (`svc::settle_payouts`, `realm_games.payout`, migration 242). `finish_cas` makes the finish durable; the credits run after it, from the row as stored, and `payout` is written only once every place in the plan has been credited (`paid`), or the game turned out not to pay (`unearned`: a winner who did not clear the week or the territory floor; `dissolved`: no winner). A failed credit leaves `payout` NULL and the sweeper settles it on its next tick (`list_finished_unpaid`), the per-event claims making the retry pay nothing twice. Every place lands on the dashboard by outcome (`record_realm_payout`: paid, already paid, unearned, dissolved, failed). The results card reads the column: until it is set, the pot line says the chips are on their way rather than paid.
- **You can withdraw on your first day, and not after.** While you are `New` nothing has happened to you and nothing has happened because of you, so leaving is free; past that you are committed, because a late quit would hand the game to whoever was left. The last player in a realm may always pack the whole thing up (`RealmGame::cancel`), since there is nobody to hand it to. Stopping otherwise is something you do by not playing: your land stays yours and stays takeable.
- **Winning takes a week of play, against real rivals.** `can_win` wants `min_players` on the roster (a realm sat in alone is not a conquest) and `min_active_days_to_win` (7 on the standard ruleset, 40 on the campaign: half its season) *distinct days the player acted*, tracked as `RealmPlayer::active_days` — elapsed days would be farmable by waiting. Eliminating everyone sooner does not end the game (`check_end` returns `Ongoing`); you hold the field until the days are in. Every way to win, on either ruleset, goes through the same bar, which never bites on Earth (a conquest there is weeks of work) but stops a 12-territory map being swept in an afternoon with an obliging friend.
- **Missed days bank, on a ladder** (`standing`, `bank_rate`), one rung per missed day: the first banks 75%, each further one to `bank_missed_cutoff` (3) banks 50%, and each past that banks nothing, without taking back what the earlier rungs banked. The bank is capped at `bank_cap_days` (3) days' worth, which is what stops a fortnight away from becoming one flattening blitz. The day's allowance is **frozen at the first action of the day** (`RealmPlayer::actions_allowance`) — the gap it is computed from closes the moment you act, so without freezing it the bank would evaporate on the first spend.
- **An abandoned empire crumbles** (`decay_multiplier`): after `dormancy_grace_days` (3) quiet days a player's strength falls `decay_per_day` (10%) per further day, floored at `decay_floor` (0.40). Their land defends with less than its size suggests, so quiet players get carved up rather than freezing the game — and nobody is removed, which is what keeps "go quiet" from laundering into an exit. On the standard ruleset `effective_strength` is what the odds use (`strength` stays the raw figure); on the campaign the decay walks each wall back towards 1 (`walls_eff`).
- **The sweeper never kicks.** It rolls days over, and dissolves a game only when *every* living player has been quiet for `resolver::REALM_ABANDON_DAYS` (5 days, flat — not scaled by ruleset or pace, because what it measures is whether anybody still comes back). A player who never acted counts from their join day, so a realm nobody ever played clears itself too. That path pays nothing, so it cannot be farmed, and the next sweep opens the next world.
- **A game has to have been played to pay** (`payout_earned`: the winner cleared the week and holds at least `min_payout_territories`). Per player, `payout_eligible` also wants `min_payout_active_days` (3) of their own — the podium means "played and placed", not "was in the roster".
- Ranking (`final_ranking`): winner, then later exit day > exit territory count > seeded jitter; **leavers are payout-ineligible** (and `Kicked`, which only the abandoned-realm dissolve produces, pays nothing anyway) and the next eligible player moves up.
- Activity: two `system` headlines per world, attributed to nobody (`realm`): the rise (`RealmSeasonOpened`, with the season and the day's deadline) and the fall (`RealmSeasonEnded`, naming the winner as an `@mention` with how they took it, `RealmWinner::took_it` from `WinKind::lounge_phrase`: "dominated the realm", "won the realm on the clock", "conquered the realm", with the players and the pot, or that nobody held it; both say the next world rises within the day), each also a ticker line and keyed once per world in the repeat gate. Plus the conqueror's own #lounge chat line (`ActivityGame::Realm` → `GameWon`) and the ticker-only daily call (`RealmCalls`). Joins, takes and falls are not lounge lines: they are the strip's (`news`).
