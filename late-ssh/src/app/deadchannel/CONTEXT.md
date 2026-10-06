# deadchannel Context (late-ssh/src/app/deadchannel)

## Metadata
- Domain: the deadchannel game (GAME.md): its onboarding, the
  first-contact haunting ladder, in the `haunt/` subdomain, and the
  start of the character layer, the runner and its look, in `runner/`
  (phase 2, build order step 1), the night city street in `city/`
  (the wallet, GAME.md "The three surfaces"; art and walkable street
  first, under the clubhouse on a second `0`, runners only), the shared
  street in `street/` (every runner on it, across replicas, over
  `app/presence`; §3b), and the fight in `fight/` (the runner's sheet on
  the row, the lazy day roll, the day's road down from the screen, the
  round of cards against a glyph, the wire's news lines, the armorer's
  till, the lockers, the bits machine, the step off the ledge, crystals
  and the bright glyph, the bar's glass, the blade shop; §3c) and the
  tailor in `tailor/` (the mirror as the look's editor, the look's writer
  after the join; §3b). Built for
  several replicas (root CONTEXT.md, multi-replica rule); staff only
  (admins and moderators) by decision in code (`haunt::svc::arm` and
  `bootstrap_gate`), so only staff are haunted, and only they can finish
  the ladder and join.
- Status: Active, staff only
- Parent context: `../../../../CONTEXT.md`; design sources live in this
  directory: `GAME.md` (the game: thesis, first contact, the runner) and
  `DIGEST.md` (the feed budget and the welcome-back paper)

## 1. Summary

The game is never announced; it arrives. This domain will grow into the
whole character layer; what exists today is **first contact**: the
escalation ladder that onboards a person through haunting instead of a
tutorial. The chain is the spec, and the ladder never skips a rung:
three clock bursts quiet
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
staff (admins and moderators) and nobody else, a rule in code rather than
a switch, so nothing fires for real users while copy and thresholds await
design review; opening it up is a code change. Stages 2-4
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
truth. The daily and lifetime caps are enforced by conditional
claims on the user row (`User::claim_first_contact_glitch_burst`,
`claim_first_contact_name_hit`): a machine decides *when to ask*, holds
its schedule, and the beat shows on the tick the claim comes back won.
The whisper stamp and the invitation are claims. The bio screen is a
claim keyed on a hash of the bio text, so any number of sessions on any
number of replicas spend one AI call per text.

## 2. Module map

| File | Owns |
|---|---|
| `glyphs.rs` | `GLYPH_ALPHABET`, the game's shared character vocabulary. Game-level: the haunting borrows it, stage-4-era spawns will render with it (the clock glitch is retroactive foreshadowing). Distinct from the static shades `░▒▓` (noise, not creatures). `MARK_ALPHABET` is what a runner can wear as a mark: the alphabet less the Signal's `╬`, which is the paragon count in the badge and nobody's for free (migration 206 moved the marks that wore it to `┼`). |
| `haunt/state.rs` | The pure machines and data: `HauntState` (the one `App` slot), `FirstContactMarks` (persisted marks bundle), `FirstContactGate` + `BioStanding` + the thresholds and `bio_hash` (the eligibility gate), `ClockGlitch` (stage 1), `NameFlicker` (stage 2, the person being haunted), `ActiveHit` (one hit's playback, holding the wave seed: the roller's own hit and the `witness` slot share it, so every screen corrupts identically), `WhisperState` (stage 3), `Breakthrough` + `InvitationClaim` (stage 4's full-screen beat and the claim answer), the voice/invitation/breakthrough-line constants (stage 4), `PendingClaim`/`HitStage` (claims in flight), `HauntCommand` + `parse_haunt_command`. No I/O, no clock reads. |
| `haunt/svc.rs` | Orchestration: `bootstrap_gate` (gate + bio screen claim at connect), `arm` (session start), one `tick(app)` (claim drain, splash door, glitch scheduler, name-flicker roller, witness replay, breakthrough (claim answer, scene, due send), `/haunt` drain), `swallows_splash_input`, `replay_whisper`, the bio screen task, and `publish_name_hit` (a won or forced hit goes on the wire through `ChatService::publish_name_hit`). The only haunting layer touching `App`, logging, metrics, and persistence. |
| `haunt/ui.rs` | Pure render helpers: whisper frame + splash overlay + static surge, breakthrough frame + full-screen draw, `apply_clock_glitch`, `glitched_name`, `name_flicker_for`. Deterministic per burst seed, stateless like the sidebar equalizer. |
| `runner/state.rs` | The look: `PIECES` (the closed table, one five-cell row per piece, `Slot` hood/eyes/coat, each with the `level` that unlocks it: three per slot at 1, 4, 7, 10, 13; no earned or legendary pieces, every piece is reachable by level), `Tint` (the closed palette of seven, each with its unlock `level`, white alone at 15; gold deliberately absent), `UNLOCK_LEVELS`, `unlocked_pieces` / `unlocked_tints` / `next_unlock` (the level gate), `Look` + `Worn` (typed, table references), `Look::random(level, rng)` (the join's dice at 1, the tailor's shuffle at the runner's level), `Look::to_json` / `Look::parse` (the JSON contract on the runner row; unknown codes are a `LookError`, never a blank), `PORTRAIT_WIDTH` / `PORTRAIT_HEIGHT`. No I/O. `state_test` asserts every row is five single-width cells and pins the unlock ladder whole. |
| `runner/ui.rs` | `portrait_spans`: the look as three styled spans, one per worn piece in its tint; `badge_text` (the mark and the level, `▚7`, with the marks behind the Signal's glyph once there are any, `▚3╬2`) and `level_color` (the newest tint the level unlocked: static to 3, phosphor to 6, cyan to 9, magenta to 12, red to 14, white at 15) for the wire's author header and the profile; `tint_color` maps the palette onto the theme (cyan and magenta fixed, the theme has neither). Pure. |
| `runner/data.rs` | `welcome`: the voice's welcome for a runner whose row was just created, one message, one paragraph per line (the runner mentioned by name, the story so far, `0` twice as the way down, `/leave` and `/join #deadchannel`). Names no key on the street: those are the guide's. Placeholder copy at feed-template standards. Pure. |
| `runner/svc.rs` | `RunnerLookService`: the process-shared runner directory (`watch<Arc<HashMap<Uuid, RunnerEntry>>>`, an entry being the look, level, peak level, and marks of a standing runner), seeded and refreshed from `deadchannel_runners` (`list_standing`) on the `deadchannel_runner_changed` LISTEN, re-read whole on any change. A look that fails to parse is logged and skipped. `fixed_looks_rx` for test apps. |
| `street/state.rs` | Pure: `street_view`, the street derived from presence records (one `StreetRunner` per user: the latest mover's cell, present if any session is looking), and `StreetPresence`, the `App` slot: `descend` (on the street until the session ends), `sync` (the stand this session publishes; the move stamp moves only on a move), `leave`, and the derived `view` the renderer reads (`set_records` says whether it moved). Stands off this map are dropped here. The wire is `app/presence`. |
| `city/map.rs` | **Generated** by `scripts/gen_city_map.py --write` (never hand-edited): the 440x44 `MAP` literal, the `SOLID` collision bitmap, `SPAWN`, every zone (`SIGNS`, `BANNERS`, `CART_SIGNS`, `AWNINGS`, `WINDOWS`, `VENTS`, `PUDDLES`, `LAMPS`, `DROP_LIGHTS`, `SCREEN_FACE`, `WIRE`, ...), the closed `Neon` palette, `Landmark` + `nearest_landmark` (reach zones), `walkable`, `grid`/`char_at`. |
| `city/state.rs` | Per-session view state: the runner's cell, the animation clock, the open panel, the cursor on the armorer's wall (`picked_tier`, `pick_up` / `pick_down`), the ledge and the lean out over it (`arm_reset` / `disarm_reset` / `reset_armed`, only while looking over, cleared by every way back), the pinned street line. `walk`, `run`, `nearby`, `Landmark::on_enter` (`Enter::Panel` for shops and the blade shop, `Enter::Line` for the other carts, `Enter::Fight` at the screen, `Enter::Leave` for the wire). Pure. |
| `city/landing.rs` | Night City's card in the Games hub (`door/hub`, runners only): an animated skyline in the city's palette (far and near towers, windows that flip, signs that short out and reflect in the wet street, a blinking antenna, rain, the monorail), NIGHT CITY in a five-row block font (one line when the pane is wide enough, stacked when not, each letter shorting out on its own clock), a billboard ticker, then the sheet (level, signal, pockets, today's road, glyphs, the glass, a waiting fight), the other runners on the street, and the keys down. Pure render over `LandingView` (sheet mirror, street view, `marquee_tick`); the hub repaints it on the ambience edge while it is selected (`hub::state::animates`). Also the sidebar row's neon styles. |
| `city/data.rs` | The city's copy and catalogs: the gear ladder (`COST_LADDER`, `WEAPONS`, `ARMOR`: LoGD numbers, GAME.md names), `BANDS` with draft move names, `NOTICES`, `TAILOR_PRICES`, the per-landmark `lines` pools, `title` and `pitch`. |
| `city/input.rs` | `descend`: the way down (closes whatever was open, reloads the sheet, claims the first-descent guide, puts the runner on the street, `Screen::City`), called by `0` on the clubhouse and by Enter on the hub card; callers hold the runner gate. While the guide is open every key goes to `guide/input.rs`, and `?` anywhere on the page opens it. Arrows/hjkl walk; Enter at a landmark; `f` walks up to the static (the road, `FightSession::step_up`) and `p` opens patch from anywhere (the armorer, the tailor, the lockers, and the bits machine stay a walk away on purpose); Enter closes a panel or the ledge view, Esc too through the root's `dispatch_escape`, which on the bare street goes up to Home with #lounge selected (while one is open every typed key but the digits and `q` is swallowed: the walk keys, and the letters a global would spend). While the road or the scene is open every key goes to `fight/input.rs`. In the armorer's panel up/down walk the wall and `w` / `a` send `Command::Outfit` to `App.fight`; in patch's panel `p` sends `Command::Patch`; at the lockers `d` / `w` send `Deposit` / `Withdraw`; at the bits machine `b` / `r` send `Borrow` / `Repay`; at Dead Air `s` / `d` / `t` send `Command::Drink`; at the blade shop `w` / `a` send `Command::Cart`; over the ledge `r` leans out and a second `r` sends `Reset`, any other key leaning back in first. Returns `false` for globals. |
| `city/ui.rs` | Renderer: base styling by zone, the ambience pass (rain, puddles reflecting the nearest sign, neon shorts and dropped letters, window flicker, the screen's static and test pattern with rare glyph frames, steam, lamps, the drop's lights, the blimp, the mast, the bits machine, the wire's pulse), the runner as its mark, the popover, the street line, the shop panels (`armorer_lines` is the live till: the wall with the cursor, what you carry lit, the tiers under it dimmed, the two keys priced net of the trade-in, the armorer's last word; `bar_lines`: the crystals, the glass in you, each glass with what it does at this level, the refusal ahead of the keys; `cart_lines`: the next tier up from what each slot carries, priced in crystals, with what the wall asks in bits beside it; `patch_lines`: the signal, the price of the gap, the refusal spelled out ahead of the key; `locker_lines`: on hand and locked up, the deposit priced net of the cut; `machine_lines`: owed against the cap, the loan and the repayment priced, the terms); hands the sheet strip, the road, and the fight scene to `fight/ui.rs`, and the ledge (with the lean out and the till line) to `city/ledge.rs`. Its palette helpers (`ink`, `lit`, `glow`, `dim`, `scale`, `neon_rgb`, `tint_rgb`, the `INK_*` greys) are `pub(crate)` for that. |
| `fight/data.rs` | The numbers: LoGD's, transcribed (`RATIONS_PER_DAY`, `SIGNAL_PER_LEVEL`, `START_BITS`, `EXP_TO_ADVANCE` with LoGD's per-mark scaling in `exp_to_advance(level, marks)` and `exp_to_seek(marks)`, `FOE_TIERS`, `TRADE_IN_PERCENT`, `MARK_BONUS_CAP`); the ones the climb sets, `fight/BALANCE.md` (`PAY_BITS_PERCENT` and `PAY_EXP_PERCENT`, what a glyph pays over the table, `PRICE_PERCENT`, the wall over `COST_LADDER`, `PATCH_PERCENT`, `EXP_KEEP_ON_DEATH`); the round's (`FOE_SIGNAL_PERCENT`, `HIT_PERCENT`, `BLOCK_PERCENT`, `NOISE_CARDS`, and the formulas `Rules::strike`, `surge`, `block`, `hit`); the drafted cards' (`JAB_PERCENT`, `SIPHON_MEND_PERCENT`, `BULWARK_PERCENT`, `BURN_ENERGY`, `GROUND_PERCENT`, `SEVER_PERCENT`, each a share of a strike or a block through `Rules::share`); the road's (`REST_MEND_PERCENT`, `CACHE_PERCENT`, `Rules::mend`, `cache`); ours, harder than LoGD's (`LOWER_PAY_PERCENT`, `lower_foe_for_level`, `LOCKER_FEE_PERCENT`, `LOAN_PER_LEVEL`, `LOAN_FEE_PERCENT`, `GARNISH_PERCENT`, `percent_up`); and the crystal pass's, `fight/CRYSTALS.md` (`CRYSTAL_DROP_ONE_IN`, `bright_foe_for_level`, `DRINK_CRYSTALS`, `drink_edge`, `CART_CRYSTALS`). All of the tunable ones are gathered into `Rules`, the value the machine runs under (`RULES` is the live game; `Rules::foe`, `price`, `trade_in`). And the fauna: `FOES`, fifteen glyphs, one per level, each with a name, a five-by-three portrait in the runner's format, an arrival line, and a `pattern`, the cycle of `Intent`s (`Hit`, `Charge`, `Heavy`, `Noise`) that is the whole of its mind; `OLD_SIGNAL` and `OLD_SIGNAL_TIER` (290 / 39 / 22, the reason beside it); `title(marks)`; the kill, drop, run, rest, cache, heard, and slain lines. |
| `fight/cards.rs` | The deck and the hand: `Card` (strike, block, surge, wipe; the eight drafted cards: jab, siphon, riposte, bulwark, burn, ground, sever, mute; static) with `cost`, `name`, `rule` (its one-line text), and `effect(&Powers, &Board) -> Effect` (the one place a card's numbers are worked out: damage, block, mend, energy back, whether it clears the hand's static, whether it mutes the glyph's move), `DECK` (the ten every runner starts with), `DRAFTS` (four `Draft`s: a level, the card replaced, two options), `draft_owed(level, drafted)`, `deck(drafted)` (the ten with each pick over a card its draft replaces), `HAND` (5), `ENERGY` (3), `STATIC_CAP` (5), and `Piles` (draw, hand, discard: the JSON inside the fight on the row): `deal` (a deck plus the day's static, shuffled, a hand drawn), `draw_hand` (the hand discarded, five drawn, the discard pile shuffled back under when the draw pile runs dry), `play` (a played card leaves a hole in its slot; static leaves the deck), `wipe_hand`, `static_cards`, `add_static` (stops at the cap). Pure; the shuffle's dice are handed in. |
| `fight/road.rs` | The road and the run on it. `road_for(day)`: a pure function of the UTC date, ten steps by three lanes of `Node` (`Glyph`, `Bright`, `Rest`, `Cache`), `FIGHT_STEPS` (5) of them a fight in every lane (the first and the last always, never three fights or three quiet steps in a row), `BRIGHT_NODES` (2) bright glyphs on fight steps of their own after the first, every quiet step holding both a rest and a cache. `RoadRun` is one runner's day on it (the `road` column): `path`, a `Trace` (lane and `Mark`: fighting, won, bright won, ran, fell, mended, cleared, cached) per step taken, and `static_cards`; `lane`, `reaches` / `open_lanes` (the lane stood in and the ones beside it, every lane before the first step), `settle_fight`. `road_test` pins one day's road whole and asserts the shape over 800 days. |
| `fight/policy.rs` | How a turn is played without a person choosing: `Table` (one read of the hand, the energy, the block, both signals and their maxes, the `Powers`, and the glyph's move now and next), a private `Turn` that plays cards out through `Card::effect` the way the machine would, `auto` (the game's `Auto` key: a burn first, the kill if the hand holds it, a mute or at most two guards on the turn a heavy lands, everything else hits by the best damage for the energy, leftover energy goes on a guard against whatever is landing and then shakes static out) and `sharp` (every order of every affordable set of cards played out and weighed, signal against signal; for the sim and the arena only, the game plays it for nobody). Pure. |
| `fight/share.rs` | `road_card(sheet)`: the day's road as an `arcade/share.rs` card once the road is over (`Sheet::road_over`) and at least one step was taken: three lanes by ten steps, the lane walked lit by each step's `Mark`, a stat line under it, the header `late.sh the road #N · walked` or `· fell at S`. Pure; copying is `fight/input.rs`. |
| `fight/state.rs` | The pure machine: `Sheet` (the row's stats and tally, typed, with `peak_level`, `marks`, `mark_bonus`, `signal_hears`, and `road`, the day's `RoadRun`; `from_row` rejects an unreadable fight, drink, or road loudly, except a fight with no `piles`, the exchange loop's shape from a pre-round binary still draining its sessions, which reads as no fight with the day handed back), `Fight` (the `Quarry`, a glyph or the Old Signal, its numbers, the last six lines, and the round: `turn`, `energy`, `block`, `piles`; `intent` / `intent_in` read its pattern), `Powers` (what a strike, a surge, a block, and the glyph's hit are worth in this fight: `Sheet::powers`), the step in `step_to` (`Command::Step { lane, call }`: the lane must be in reach, and the node there answers only its own `Call`: `Fight(Pick::Fair \| Pick::Lower)` on a glyph, `Fight(Pick::Bright)` on a bright one, `Mend` or `Clear` at a rest, `Take` at a cache), `engage` (the fight on the row with a fresh deal, no ration: what the sim's odds and the arena call), `Command::Resume`, the round (`Play { slot }`, `EndTurn`, `Auto`, `Run`), `shut` (why a step would start nothing, the closed `Shut`), `road_over`, the reset in `slay` (GAME.md, "Marks: the reset"), `settle(today)` (the lazy day roll, which wipes the road), `apply(Command, rng)`, plus the armorer's till (`Command::Outfit`, `Slot`, `gear_name`, `outfit_price`, `MAX_TIER`; no level gate, the price keeps the kit with the level), the bar (`Command::Drink`, the closed `Drink` menu, `glass_refused` the one order and wording of the bartender's refusals), the blade shop (`Command::Cart`, `cart_tier`), patch (`Command::Patch`, `patch_price`), the lockers (`Deposit`, `Withdraw`, `deposit_fee`), the bits machine (`Borrow`, `Repay`, `loan_cap`, `loan_room`, `loan_fee`, the garnish in `put_down` and on a cache), and the ledge (`Reset`, `has_something_to_lose`), returning an `Outcome` (`Applied` plus the lines), and `news(&Applied)` (the closed `News` list the wire prints for it; §3c). No I/O, no clock. |
| `fight/svc.rs` | `FightService`, the one writer: lock the standing row, settle, apply, store, commit; the metric, the log line per outcome, and the wire's news (`post_news`: what `Sheet::news` decided, worded; a dropped signal with the step it fell on, a level gained with the face, the Old Signal put down with the face, a step off the ledge, the first kill, a bright glyph put down, a near miss, the day's road walked) through `ChatService::post_wire_line_task`; the mark's chips after an Old Signal kill's commit (`pay_mark`: `ChipService::credit_run_cooldown_reward_template` on the `deadchannel_old_signal_slain` template, 40,000, keyed `<runner row id>:<generation>:<mark>` and at most once every 30 days per account; the answer's last line says whether the house paid, paid already this month, or failed; each outcome is counted (`late_ssh_deadchannel_old_signal_payouts_total`) and the failure logged as an error. The debt is durable: the kill writes the mark to `unpaid_mark` in its own transaction (migration 210), a paid or refused grant settles it (`DeadchannelRunner::settle_mark`), and a grant that erred leaves it standing, so the next command or reload on the row calls `pay_mark` again with the same event key; the unique gate makes the repeat safe) and the `SIG` milestone badge (`grant_unique_milestone_award`, once per account). `act_task` and `reload_task` answer on a session's `mpsc`. |
| `fight/arena.rs` | Test-only (`#[cfg(test)] mod arena`): the balance arena, `fight/BALANCE.md`. `matchup(rules, &Recipe, Pick, Hand)` (a level, a `Kit` as a lead over the level, a glass, marks, the cards drafted, against one pick, the cards played by the `Auto` key or read sharp, 400 fights to the end: odds, turns, signal left, static left, patch, pay), `old_signal`, `level_economy`, `climbs`, `BuildReading` (one of the sixteen builds read on its own) and `draft_spreads` (each draft's two options side by side), `Reading` (every number the targets are stated in, for one `Rules`, every build beside the named players, the batches on threads through `fan`) with `Reading::misses` (the bands it is out of), the report's tables (the day-by-day curve, a day at a time per player, the drafts, every build among them), `sweep` (one reading per candidate), and `run` and `run_words` (one journaled climb printed whole, every shop visit, step, and turn). `arena_test.rs` holds the contract (`the_live_rules_miss_no_target`), the `SWEEP` list, and three `#[ignore]` prints: `make deadchannel-arena` (`late-ssh/target/deadchannel-arena.md`), `make deadchannel-sweep` (`deadchannel-sweep.md`), and `make deadchannel-run` (`deadchannel-run.md`). |
| `fight/sim.rs` | The balance harness: a `Player` (who plays the cards, `Hand::Auto` or `Hand::Sharp`; the card it takes at each draft, its `Build`; a run line, a patch line, a step-down line, when it starts heeding them, `Heeds`; whether it routes to the bright glyph, drinks, when it buys at the blade shop, `Carts`, and whether it borrows; how it holds its `Purse`; `CAREFUL`, `AMBIENT`, `RECKLESS`, `NEGLECTFUL`, `KEEN`, `HOARDER`, and `BORROWER` named, all on `HOUSE_BUILD`; `builds()` is all sixteen) played through the real `Sheet` from a fresh row with seeded dice, a road a day: before every step the runner shops (the card owed, patch, the loan, the armorer, the blade shop, the bar), `route` picks the lane (a few steps read ahead: a rest is worth what the signal and the deck are missing, a bright glyph a detour or a berth), `play_turn` plays one turn by the hand a card at a time, and `climb` returns a `Climb` (the day of every level and the kit it was fought in, every `Day` played with where the runner stood at dusk and what the day held, the gate, the mark, the drops, the first drop and the level that ended it, the boss tries, the turns and the fights, the `Ledger` of bits and crystals; under a `Bench`: the `Rules` in play and a memo of threat reads shared across a batch) and, with `Journal::On`, writes every `Event` of it in order (dawns and dusks with a `Snapshot` of the sheet, shop visits, steps, and every turn with the glyph's move, the hand, and the cards played), `median`, `summary`. `sim_test.rs` pins GAME.md's target (about two weeks to the first mark read sharp, about three on the `Auto` key, later reckless, a level within a week of the first drop for the runner who skipped the armorer) and one seeded climb whole. Also `odds(sheet, pick, fights)` (and `odds_under` a `Rules`, `odds_played` by a `Hand`), one fight played to the end on the `Auto` key `ODDS_FIGHTS` times on fixed dice through the same machine, and `Threat` (easy, even, risky, grim) over it: the road's word is the sim's number, and the floor. Pure. |
| `fight/session.rs` | `FightSession`, the session's side: the sheet mirror, the `Picker` (the road open over the street: the cursor's `lane`, and the threat of each pick read from the mirror by `sim::odds` when it opens and when the mirror moves, never per frame, and only for the nodes on the lanes the next step reaches), the `Scene` over the street (lines, `over`, `waiting`, `failed` while the last answer was the service not answering, `old_signal` set from the row's quarry when a fight starts or resumes and kept until the scene closes), the `till` line (an answer that lands with no scene open: a counter's word, or the road's last step), one action in flight, `step_up` (the road, or a waiting fight straight back in through `Command::Resume`) / `call(Call)` (a step key: ignored when the node under the cursor does not answer it, closes the road when the day's road is over, opens the scene for a fight, steps in place for a rest or a cache) / `enter` (the plain call for the node; at a rest the mend, or the clear when `Sheet::rest_clears`) / `pick_up` / `pick_down` (the cursor over `open_lanes`) / `leave_scene` (a finished scene back to the road) / `close` / `clear_till` / `request` / `reload` / `drop_sheet` / `tick`. A card the turn cannot pay for (`Refusal::NoEnergy`, `NoCard`) is a line on the scene and the fight goes on; any other refusal there ends it. Decides nothing. |
| `fight/input.rs` | Keys while the road is open (`handle_picker`: up/down the lanes, Enter the plain call, `f` the glyph, `g` the one below, `b` the bright one, `h` mend and `c` clear at a rest, `t` take a cache, `s` copies the day's card once the road is over, through `fight/share.rs`, the banner, and `metrics::record_share_card(ShareCardKind::Road)`) and while the scene is open (`handle_event`: `1` to `5` play that slot and are swallowed for the length of the fight so a card key is never a page switch, `e` / space / Enter end the turn, `a` the `Auto` turn, `r` run, Enter on a finished scene goes back to the road); the other digits, Tab, `q` stay global (`?` is the guide's, taken in `city/input.rs` first), everything else is swallowed. |
| `fight/ui.rs` | `draw_picker`, the road (the sheet on top; `step_numbers` and `road_map`, ten steps by three lanes on faint rails, the lane walked joined like a transit line and lit by each step's `Mark`, the step in front lit with the cursor's node in brackets and the way to it dashed, the road past that fading with distance except the bright nodes, which burn and flicker with the tick; `legend_rule`; then `node_lines`, three rows on what waits under the cursor: a glyph's face, signal, hit, moves in order, pay, and threat word, or a rest's two calls priced, or a cache; the last step's word; the keys. Lanes sit a row apart when the terminal has the rows. Once the road is over the same panel is the day's card in words), `draw_scene` (two portraits facing, each losing cells to static in proportion to its missing signal, `corrupt`, which the profile's runner column shares; the bars; the glyph's next move in its column, `intent_spans`, a heavy pulsing with the tick; `stance_row`, the kit, the block standing, and the move after; the exchange; the hand, `hand_readout` and `hand_cards`, five cards in fixed slots with the slot's key, the name, what it does at this kit, and its cost, a card the turn cannot pay for dark, a played slot only its corners, or `hand_chips`, one row, on a terminal too short for cards; the keys; dressed a glyph's way, a bright one's in amber, or the Old Signal's, `Dress`: the whole area in red, `noise_line` on the empty rows, `tear` on the border, the box leaning a column with the tick while it broadcasts) and `draw_strip` (level, signal, rations, bits, the debt, the crystals, and the day's glass when there are any, the weapon and the armor by name, top-right on the street); `weapon_name` / `armor_name` (`bare hands`, `street clothes` at tier 0) for every readout that names the kit. Pure. |
| `tailor/state.rs` | `Draft`, the mirror's editor over one `Look` at the runner's `peak_level`: four `Row`s (hood, eyes, coat, mark), `up` / `down`, `next` / `prev` around the row's unlocked rack (wrapping), `tint` around the unlocked tints (nothing on the mark row: a colored mark is earned), `shuffle` (the join's dice at the peak); `new` snaps a piece or tint the peak has not unlocked onto the rack's first entry, so the walks never meet one they cannot place. Pure. |
| `tailor/svc.rs` | `TailorService`, the look's writer after the join: `wear_task` runs `DeadchannelRunner::store_look` (one statement, standing runner only, last write wins) and answers `TailorOutcome::{Worn, NoRunner, Failed}` on the session's `mpsc`; the metric, the log line per outcome. The change trigger carries the look to every replica's directory. |
| `tailor/session.rs` | `TailorSession`: the `draft` while the panel is open, `worn` (what the row wears as far as this session knows), the tailor's `word`, one write in flight (`saving`); `open(runner)` / `close` / `changed` / `wear` / `tick`. Decides nothing. |
| `tailor/input.rs` | Keys while the tailor's panel is open: up/down (`k`/`j`) row, left/right (`h`/`l`) pick, `t` tint, `r` shuffle, `s` wear, Enter leaves; digits, Tab, `q` stay global (`?` is the guide's, taken in `city/input.rs` first), everything else is swallowed. |
| `guide/data.rs` | The undercity guide's copy: `SECTIONS`, a heading, its neon, and its `Block`s each (`Loop`, `Prize`, `Keys`, `Figures`, `Rule`), the short version first and built for the runner who reads nothing else (the loop as a chain, the prize boxed: 40,000 chips once every 30 days and the mark, the keys, the day's numbers, six bullets), then every key and rule of the street, the sheet, the road (the map, the step down, the bright glyph, the rest, the cache), the hand (the cards, the glyph's moves, static, the run, the `a` key), the Old Signal, the armorer, patch, the lockers, the bits machine, dead air, the blade shop, the ledge, the tailor, the rest of the row, and the wire. Prose marks `` `key` `` and `*strong*`; nothing else uses a backtick or an asterisk. Kept out of `app/help_modal` on purpose (that one is fed to the bot). **Always current**: see §3b. Pure. |
| `guide/state.rs` | `State`: open, scroll, and the page the renderer last measured (`record_page`, a `Cell`), so `scroll_by` holds at the end. Pure. |
| `guide/svc.rs` | `GuideService`: `claim_first_descent_task` runs `DeadchannelRunner::mark_guide_seen` (one conditional update) and answers `GuideOutcome::{FirstDescent, SeenBefore}` on the session's `mpsc`; a failed claim answers nothing and logs. |
| `guide/session.rs` | `GuideSession`: the `state`, `descend` (the claim), `tick` (opens the guide on `FirstDescent`). Decides nothing. |
| `guide/input.rs` | Keys while the guide is open: `j`/`k` and the arrows scroll a line, PageUp/PageDown a screen, Enter, `q` and `?` close it; digits and Tab stay global, everything else is swallowed. `opens` names the key (`?`). |
| `guide/ui.rs` | `draw`: the box over the street in the city's palette, each section under a neon `▚ heading ───` rule, each block drawn its own way (the loop joined by arrows, the prize in a rounded amber box, keys as amber chips in as many columns as fit and one wrapped column when narrow, figures bright beside their meaning, rules as bullets with a hanging indent, the marks read into chips and bright spans), wrapped here through `common/markdown::wrap_spans` so the row count is the page the scroll holds against; the keys under; `body_lines`. Pure. |
| `tailor/ui.rs` | `mirror_lines` for the city's panel: the draft as a portrait with the mark under it, four rack rows beside it (the cursor, the label, the tint's name, a window of up to five pieces around the worn one, bracketed, the whole rack once while it is shorter; the mark alphabet on the mark row), the keys (`[s] wear it` lit only when the draft differs from what is worn), what the next unlock level opens, the tailor's word. Pure. |

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
`app/ai/screen.rs::screen_bio` (the
bio verdict), `ProfileService`'s first-contact tasks (the row claims),
`late-core`'s `models/deadchannel_name_hit.rs` (the wire's channel,
payload, and parse), and `metrics::record_first_contact_beat` /
`record_first_contact_bio_screen`.

The runner's seams are as thin: `ChatService::join_deadchannel_room`
creates the row (`DeadchannelRunner::ensure_for_user`, a conditional
insert, so two devices joining at once share one face; a fresh row is
the `RunnerCreated` beat and posts the voice's welcome on the wire
(`runner/data.rs::welcome` through `post_wire_line_task`, a mention of
the runner, once per person because only the winning insert lands there;
a second device or a return finds it in the room's history), a return
the `RunnerDoor::Returned` one),
`ChatService::leave_room` stamps the leave for the `deadchannel` kind
(`mark_left`, the `RunnerDoor::Left` beat), `State.runner_looks` holds the directory
service (`main.rs` starts its listener), `App.runner_looks` is the
session's owned copy refreshed on the 1 Hz edge in `tick.rs` (bumping
`chat_ctx_epoch`, so the rows rebuild once per change; the same edge
reloads `App.fight`'s sheet for a runner and drops it for a leaver), and
chat's rows builder takes `runner_looks: Option<&HashMap<Uuid, RunnerEntry>>`,
`Some` only while the rendered room is #deadchannel: a runner's author
header opens its badge stack with the level badge (`runner/ui.rs::badge_text`,
`▚7`, painted in the level's band through `AuthorTint.runner`; the
first badge, so it sits right after the name, crown, and title),
every entry in the room wraps `PORTRAIT_GUTTER` (6) cells short, and a block-opening message by a
runner gets `attach_portrait` (the face right-aligned on the entry's
first rows, the hood level with the header, wearing what the entry has
rows for: a one-liner, header plus one body row, shows hood and eyes
and hands the coat to the first row of a continuation right under it
(none, or a divider between them, and it goes bare), anything taller
wears the coat itself, so no message grows a row for its face). The blank separator above a block stays blank, so two faces
stacked down the wire never touch. The profile modal grows a runner
column beside the late.fetch grid for a standing runner (`ProfileSnapshot.runner`,
loaded by `ProfileService::do_find_profile` from the row; no frame, its own
heading carrying the level badge, the face wounded by the missing signal beside
one key column: the signal and exp bars, the bits, the kit, the glyphs down), shown only when
the viewer is a runner too (`profile_modal::ui::draw`'s
`viewer_is_runner`, one argument to drop at the public flip;
`late-ssh/src/app/profile_modal/CONTEXT.md`). Continuations and system lines carry
no face; every other room renders exactly as before.

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
   never content; timezone label untouched. Armed for every staff
   session, gate or no gate.
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
   **The room watches.** The hit is not private to the
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
   only staff are in it, so nothing of the haunting reaches a real user.
3. **Whisper (the held door).** Plays `WHISPER_TOTAL_CAP` (2) times per
   person, at least `WHISPER_GAP_HOURS` (20) apart, each from its own
   line pool: the first door says the static noticed you, the second
   that something is trying to get through. Arms at connect only when
   name hits have reached `NAME_TOTAL_CAP` and
   `FirstContactMarks::whisper_due` holds (under the cap, and the last
   delivery a day or more ago): the haunting follows you home, and comes
   back. The splash neither skips nor expires while held,
   and the scene waits on nobody: the static pulses on its
   own rhythm (~1.3s, each noise pattern held ~130ms) from the first
   frame, the voiced line types itself once
   the base splash line is done (`VOICE_TICK`), the skip hint dissolves as
   it starts, and every key, Esc included, is swallowed and does nothing
   (a scene that waits on a keypress reads as a quiet line under the cup,
   and people miss the door). A hard cap (~10s) releases whatever the
   phase. Delivery claims
   one mark (`claim_first_contact_whisper`: increments
   `first_contact_whisper_hits` and stamps `first_contact_whisper_at`,
   conditional on the cap and the gap in the row, so two devices that
   both played leave one mark and the same evening never counts twice;
   the loser is logged, the one race the claim-on-delivery shape
   accepts, because claiming at arming would burn a whisper on every
   dropped SSH session). A hard-cap drop or lost session leaves the
   mark unspent.
4. **Breakthrough, then the invitation (the whole game is opt-in).**
   `INVITE_DELAY_HOURS` (20) after the second delivered whisper the
   breakthrough comes due (`FirstContactMarks::breakthrough_due`), and the
   next send this session submits plays it (the DM
   alone, met cold, reads as spam). Only a `SendSucceeded` for a
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
   the parser. Known gap to close before the haunting leaves staff: the
   claim is stamped `dm_delay` before the DM sends, so a
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
   where the invitation points. **The invitation is the key:** `ChatService::open_public_room` routes the
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
is the street and its doors, the screen (the fight, §3c), and the open
counters (the armorer, patch, the lockers, the bits machine, the bar, the
blade shop, the tailor); bands and the board are catalogs with the till
shut.

- **Where it is reached.** Under the clubhouse: `0` lands on the
  clubhouse, `0` again on the clubhouse goes down to the undercity, `0`
  on the undercity comes back up. The Games hub (page `3`) has a front
  door too: a Night City card first under "the house", above Lateania,
  whose Enter takes the same descent (`city/input.rs::descend`, the one
  function both doors call; the card is on the roster only for runners,
  `HubGame::roster`, so a non-runner can neither see nor select it).
  Runners only (`App::is_runner`: an
  entry in `App.runner_looks` for this user, so a `deadchannel_runners` row
  without a leave stamp; the app-wide gate for everything under the
  clubhouse, per user); anyone else stays on the clubhouse. The gate guards the descent,
  so the standing there is guarded on the 1 Hz edge in `tick.rs`: when the
  directory changes and this user is no longer in it, a session on
  `Screen::City` is walked back up to the clubhouse. That is the only
  place in the process that can notice a leave taken on another session or
  another replica. Not in the Tab cycle
  (`Screen::City.next()`/`prev()` return the clubhouse), no tab of its
  own, the clubhouse tab stays lit under it, title "Undercity" with
  `· f fight · p patch · ? guide` beside it in the chrome. Enter at the wire goes
  back up to the clubhouse; Esc on the bare street (after it has closed
  whatever was open: the guide, a panel, the ledge, the scene) goes up
  to Home with #lounge selected instead, the chat rather than the
  tavern. The wiring is thin on purpose (`Screen::City`,
  `App.city`, `App.guide`, one dispatch line each in `input.rs`,
  `render.rs`, `tick.rs`).
- **The guide (`guide/`).** The street explains itself: a box over the
  city that opens on the short version (the whole game in a screen, for
  the runner who reads nothing else) and goes on to every key and every
  rule (the street, the sheet, the static, the armorer, patch, the
  tailor, the rest of the row, the wire). `?`
  opens it anywhere on the page, the fight scene and the tailor's panel
  included (the site guide's key, taken over down here; the site guide
  is a page away). Esc, Enter, `q`, `?` close it; `j`/`k`, the arrows,
  PageUp/PageDown scroll. It opens by itself on a runner's first descent,
  once per runner on any device or replica: the descent fires
  `GuideSession::descend`, the service's conditional update on
  `deadchannel_runners.guide_seen_at` (migration 203,
  `mark_guide_seen`) is the claim, and `FirstDescent` opens it a tick
  later over the street. Not in `app/help_modal`: that guide is fed to
  the bot, and this one is the runners' own. **Invariant: the guide is
  always current.** Every change to a key, a price, a rule, a counter,
  or a wire beat in this domain updates `guide/data.rs` in the same
  change, and `guide/ui_test.rs` renders every line, so a stale guide is
  a failed review, not a follow-up.
- **The register: tiles.** Top-down, one tile per
  thing, the Dwarf Fortress register. `#` walls, `+` doors, `╬` windows
  that flicker, `=` counters, `)` blades, `[` plate, `"` marks, `!`
  bottles, `%` bowls, `∩` lockers, `▬` beds, `▪` crates, `▓` shutters and
  cabinets, `≈` water, `@` people, `c` cats, `r` rats, `>` stairs, `*`
  lamps, `°` lanterns, `≡` grates that steam. Not front-on with
  multi-cell facades: that reads as a plaza. A 440-column street in three legs with a dogleg between
  each (the camera scrolls), four tiles wide, alleys one to three wide
  running off it (dead ends, a hidden court with a shrine and a plant, a
  back lane behind the second leg reached from its two end alleys and the
  arcade's back door), rooms you walk into, stalls of five tiles against
  the walls, a canal under the first two legs (a walkway, two bridges,
  warehouses on the far bank), the ledge with the railing and the wire
  stairs (`>` in the gap, the spawn) along the third leg, a small yard
  and the screen as three tiles of static closing the street. **Shops
  are where the game is played; the street is flavor.** Everything that
  spends or earns happens inside a building with a lit sign or at the
  screen; stalls, carts, and the reader only talk. The open shops are
  laid out by how often a runner needs them: the armorer and the blade
  shop side by side in the middle of the street with patch and Dead Air
  just west of them, the lockers beside the wire stairs where a runner
  arrives, loans (the bits machine in its corner) further east toward
  the screen, the tailor at the west end. A shop that does nothing yet
  is `closed='Name'` in the generator: its sign is in `map::DARK_SIGNS`
  (its own color at `DARK_SIGN`, throwing no light), its walls and door
  take no neon. Today that is MARKET, SHRINE, PAWN, INK, BATHS, BANDS
  (its panel still opens as a preview), SLEEP, COIN, VIDS. Every one
  is already a `Landmark` (the flag is its name): its reach is its
  front doors, its popover says `data::CLOSED_PITCH`, and Enter answers
  `data::CLOSED_LINES`, the same for all. Opening one is three moves:
  drop the flag and set a reach in the generator, move the variant to
  `Enter::Panel` in `city/state.rs`, and give it lines in
  `city/ui.rs`; the exhaustive matches name every other place to
  touch. The rest is there to be there: tenements
  (`tenement()` lays out corridor, rooms, beds and a sleeper from a
  seed), a lockup, a garage, a dock, a chop shop. **Walkers** (`map::WALKERS`,
  `ui::walkers`): people, cats and rats pacing a stretch of floor as a
  pure function of the tick, no state; the generator and `map_test`
  prove every path is open floor. Rule: every shop with a sign has a
  door somewhere in its walls (the generator refuses a signed shop
  without one); what happens inside can be shuffled or removed later.
- **Light.** Blade Runner, not cyberpunk: the street is
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
  in shadow. Rain takes the color of the light it falls through, in that
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
  one-row strips the generator leaves blank (five over the rooftops in
  the sky row, four on the towers below the ledge); the renderer edges
  each in dark steel and scrolls a line of street copy across it in the
  board's neon (`BILLBOARD_LINES`, one line per `BILLBOARD_CYCLE`, dark
  for a moment between, a letter flickering); each is a
  `LightKind::Billboard` in the light map. Not the glyph script: two rows
  of it read as floating blocks of noise. Raindrops
  landing on a puddle throw an `o`. Rain falls in two columns of three
  on every bare cell under the sky, the street, the drop and the void
  between the blocks alike, dim where the light is dim, and never inside
  a room (`inside_map`) or on a prop.
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
  panel does. The ledge is also the reset (`fight/MONEY.md`): `r` leans
  out, the box bottom-right turns red and says what the fall takes and
  keeps, a second `r` steps off, any other key leans back in.
- **Own palette, not the theme.** The city does not follow the person's
  theme at all: one look, tuned once (`ui::NIGHT` painted under every
  cell and overlay, `ui::neon_rgb`, the surface constants, the `INK_*`
  greys of the overlay text, all fixed RGB). A hundred palettes cannot
  all be lit well, and a light canvas showing through the street breaks
  the night. Nothing in `city/ui.rs` reads the theme module; the mirror
  in the tailor's panel recolors the tints through `ui::tint_rgb`.
- **Running** (`State::run`, Shift+arrow or `HJKL`) goes up to
  `RUN_STEPS`, stops at anything solid and at the first landmark that
  comes within reach that is not the one you set off from: the railing
  runs the length of the ledge, so a run along it must not stop every
  step.
- **The runner** is its mark (GAME.md, "The look"; `@` for a session
  without a runner row), name label above. Single-width glyphs
  only; the generator refuses wide and combining characters and
  `map_test` asserts it again.
- **Landmarks.** Enter at a shop (armorer, tailor, lockers, bands, bar,
  patch, board, bits machine) or at the blade shop opens a centered
  panel. The armorer's till, patch, the lockers, the bits machine
  (§3c, `fight/MONEY.md`), the bar, and the blade shop
  (`fight/CRYSTALS.md`) are open and the tailor's mirror edits (below);
  the others say their till is not: bands shows the three bands with
  draft move names. Enter at another cart, the reader, or the stairs pins
  a line from that landmark's pool top-left for ~8s. Enter at the wire
  leaves, back up to the Clubhouse; Esc with nothing open goes up to
  Home with #lounge selected.
- **The tailor.** Pick, not draw (GAME.md, "The look"): Enter at the
  tailor opens the panel with the runner's look and level from the
  directory (`App.runner_looks`; with no entry yet the mirror says nothing
  looks back) as a `Draft` in `App.tailor`. The city's panel is the frame, every
  key goes to `tailor/input.rs`: the cursor walks the four rows, left
  and right walk the row's rack (a window of up to five around the worn
  piece, wrapping, the whole rack while it is shorter; the ten marks all
  at once), `t` cycles the piece's tint, `r`
  rolls the join's dice again, `s` wears it. Wearing is one write
  (`TailorService::wear_task`, `store_look`: the standing runner only, no
  lock, last write wins, since a look is one value), and the row's change
  trigger carries the new face to every replica's look directory, so the
  gutter portraits, the mark on the street, and the fight scene all
  change on the next directory refresh; the panel's own mirror shows the
  draft at once. Enter or Esc drops a draft not worn. The rack is gated
  by the peak level and free once unlocked: the draft only walks pieces
  and tints at or under the runner's `peak_level` (from the directory
  entry), and the panel names what the next unlock level opens. The
  draft is the gate, with no second check at `wear`: the peak only climbs
  (an Old Signal reset takes the level, never the peak), migration 204
  re-rolled every slot that sat above the runner's level into the level-1
  rack when the gate shipped, and a look that still misses the rack (one
  written by an older replica mid-deploy) is snapped onto it by
  `Draft::new` at open, so the draft's walks never meet a piece they
  cannot place. The ownership check for bought and earned pieces lands
  with them.
- **Animation** rides the clubhouse's `anim_half` edge (~7.5fps,
  `tick.rs`), the wake tier is `ANIM_HALF_TICK` on this screen, and every
  effect is a pure function of `marquee_tick` and the cell, so nothing
  accumulates. The ambience never paints over a prop (`put_if_floor`, and
  `ui_test` checks the signs and the board survive a frame).
- **The camera looks north** (`LOOK_NORTH`, 8 rows): it centers above the
  runner so a 24-row terminal at the spawn shows shopfronts and street,
  not the drop.
- **The street is shared (`street/`).** Every runner who has gone down
  this session stands on it, on every replica, until the session ends:
  the first descent puts the runner on the street (`StreetPresence::descend`)
  and it stays there, lit while the session is on the page and dim while
  it is on another (`present`), standing where it was left. A logout
  (presence drops the session's record) or losing the runner (the directory edge in `tick.rs`) takes
  it off. One runner per user: two sessions show the latest mover's cell.
  Nothing is persisted: a fresh login starts at the stairs. Another
  runner who is looking carries a light like yours (`CARRIERS_MAX`
  nearest are spread per frame); one who is away has none and shows only
  under the street's own light, its name barely there. Runners do not
  collide, and a panel, the ledge or the fight are the session's own:
  others see you standing there. A stand off this map (a replica on
  another map, mid-deploy) is dropped in `street_view`, the boundary, so
  the renderer never indexes a cell it does not have.
  - **The wire** is presence (`app/presence`, root CONTEXT.md §7): the
    street stand rides this session's presence record next to its tavern
    seat and its Nightcap stool, batched per replica per flush, heartbeat
    and timeout included. `App::sync_presence` syncs the stand and
    publishes it on every tick (a send only on a change) and derives the
    view from new records; a city frame is bought only when the street
    view itself changed, never for a step in the tavern. Another replica
    sees a step within about a flush and a frame; your own runner is
    drawn from `city::State` and never waits on the wire.

## 3c. The road and the round (the static at the end of the row)

GAME.md, "The road pass": the screen closing the street is where the
day's road starts. Enter there (`Enter::Fight`) walks up to it: the road
opens over the street (`FightSession::step_up`), or, when the mirror
shows a fight waiting on the row, the scene opens straight onto it (a
dropped session never finds a map over a live fight). `f` does the same
from anywhere on the street (`city/input.rs`), and the frame title says
so (`Undercity · f road · p patch · ? guide`,
`render.rs::app_frame_title`).

- **One road a day, the same for everyone.** `road::road_for(day)` is a
  pure function of the UTC date: ten steps (one per ration) by three
  lanes, stored nowhere. Five steps are a fight in every lane, the first
  and the last always, so every path through every road has the same
  five fights and the pace of the climb belongs to the rations, never to
  the route. The other five are a rest or a cache, both on offer on each.
  Two bright glyphs sit on fight steps of their own, one lane each, to
  walk to or around. A step goes to the lane stood in or the one beside
  it (every lane before the first step).
- **A step is one command.** `Command::Step { lane, call }` spends a
  ration, records the lane and how it went on the run
  (`Sheet::road`, a `RoadRun`), and does `call` with the node there: a
  glyph answers `Fight(Fair)` or `Fight(Lower)` (the step down, half
  pay; `fight/MONEY.md`), a bright node `Fight(Bright)`
  (`fight/CRYSTALS.md`), a rest `Mend` (35% of the signal's max, never
  past it, allowed with nothing to mend so a whole runner can still walk
  past) or `Clear` (every static card out of the deck; refused with none,
  the ration kept), a cache `Take` (half of what the glyph of the level
  pays, the bits machine's garnish first). A lane out of reach is
  `Refusal::NotThatWay` and a call the node does not answer is
  `Refusal::WrongCall`; neither spends anything. At level 15 with the exp
  to leave it, a glyph's node meets the Old Signal instead.
- **The round.** A fight is a hand of cards (`fight/cards.rs`): ten for
  every runner (five strikes, three blocks, a surge, a wipe to start
  with; "The draft" below changes four of them), five drawn a turn,
  three energy. A strike lands for the attack less
  half the glyph's defense; a surge is two and a half strikes for two
  energy; a block holds 60% of the defense and stays up until a hit eats
  it; a wipe is a block that also throws every static card out of the
  hand (`Rules::strike`, `surge`, `block`). The glyph's move is shown a
  turn ahead and comes from its `pattern`, a fixed cycle
  (`Fight::intent`): it hits (185% of its attack less a quarter of the
  runner's defense, `Rules::hit`), gathers (nothing this turn, a heavy
  for double on the next), or throws noise (two static cards, no damage,
  no block stops it). `Play { slot }` plays one card from its slot (the
  slots hold still for the turn: a played card leaves a hole),
  `EndTurn` lets the glyph act and draws a fresh hand, `Auto` is the
  obvious turn played and ended in one command (`policy::auto`), `Run`
  is out of the fight through whatever the glyph meant to do this turn.
  There are no dice in the round but the shuffle.
- **The draft.** `cards::DRAFTS` is four drafts, at levels 3, 6, 9, and
  12: each offers two cards, the same two to every runner, and the one
  taken replaces a strike or a block, so the deck stays ten
  (`cards::deck`). The picks are `Sheet::cards` (the `cards` column, a
  JSON list of card names, in draft order; `Sheet::from_row` rejects a
  card its draft never offered). `Sheet::draft()` is the draft owed
  (`draft_owed(level, cards.len())`); while one is owed `Step` is
  refused (`Refusal::CardWaiting`) and the road panel shows the two
  cards in place of the node under the cursor (`ui.rs::draft_lines`),
  `1` and `2` sending `Command::Draft { card }` (refused with a glyph
  waiting, with nothing owed, or for a card not on offer). A mark and
  the ledge clear the picks with the level. The cards' numbers are
  shares of the strike and the block (`Rules::jab_percent` and its
  neighbours, carried on `Powers`), worked out in one place
  (`Card::effect`) that the machine applies (`Sheet::play`), the
  policies weigh, and the hand prints. A mute sets `Fight::muted` for
  the turn: the glyph's hit, heavy, or noise does nothing at the end of
  it (and on the way out of a run), a gathering is not a move to mute,
  and the flag falls with the turn.
- **Static.** A hit that gets through the block puts one static card in
  the discard pile; it costs one energy to play and does nothing but
  leave the deck. It rides the deck for the rest of the day
  (`RoadRun::static_cards`, dealt into the next fight), capped at five,
  cleared by a rest, by an Old Signal kill, and by the day roll. Patch
  does not touch it.
- **The road is a panel, and so is the scene.** `fight/ui.rs::draw_picker`
  and `draw_scene`, centered over the street in the city's palette at a
  **fixed width** (`INNER`); a short terminal closes the map's lanes up,
  gives up log rows, and last turns the hand's cards into one row of
  chips, nothing else. The road shows the sheet, the map, and what waits
  on the lane under the cursor with its threat word; the scene shows the
  two faces losing cells to static (`░▒▓`) in proportion to missing
  signal, seeded per fight and per day so a wound holds still, the
  glyph's next move, the exchange newest at the bottom with a `▸` marker
  on the latest answer, and the hand. Once the day's road is over (every
  step taken, or the signal down) the road panel is the day's card, and
  `s` copies it (`fight/share.rs`). No look wears a plain `@`.
- **The Old Signal gets the screen.** GAME.md's "diegetic spectacle": a
  boss whose presence tears the frame. When the row's quarry is the Old
  Signal (`Scene::old_signal`, set on `Started` and `Resumed`), the same
  scene is drawn the whole city area wide and tall in red, titled `the
  bottom of the city`, the empty exchange rows sparse static instead of
  blank, one border cell in six gone to static and a different six every
  tick, the box one column narrower than the area and leaning left or
  right with the tick: the shudder. It runs on `anim_tick` (`SceneView::
  tick`); on a glyph's scene only a heavy about to land moves with it.
  Once the fight is over, whichever way, the tear stops, the lean stops,
  the static goes still and grey, the red burns down: "every screen in
  the city goes quiet". The runner's own side keeps its colors
  throughout, and running is still a key. The border, the noise, and the
  shudder are this session's screen only; the wire carries the news as
  before.
- **The row is the fight.** A step onto a glyph puts it on the row
  (`fight` JSONB: the quarry, its signal, attack, defense, bits, exp, the
  last six lines, `bright`, and the round: `turn`, `energy`, `block`, and
  the three `piles`; the pay is stored as the pick made it, and the
  glyph's level is its kind, `Fight::foe_level`), so a dropped session or
  a second device finds the same fight with the same hand, and a
  reconnect never deals a better one. A second `Step` with a fight
  waiting resumes it, whatever it asked, and spends nothing; a session
  whose mirror shows a fight sends `Command::Resume`, which is refused
  when nothing waits, so a stale mirror never turns into a step. There is
  no stepping out: Esc over a live scene is `Run`
  (`fight/input.rs::handle_escape`), the root's `dispatch_escape` hands it
  there before the panel arm; only a finished scene closes on Esc, or one
  whose last answer was the service failing (`Scene::failed`: an outage
  is not a fight to be trapped in), and otherwise only leaving the page
  (`0`, a leave) closes a live one, the fight staying on the row. `Won`
  pays the glyph's bits and exp (the machine's garnish first,
  `fight/MONEY.md`) and clears the fight; `Lost` (signal at zero) takes
  every bit on hand and 35% of the exp, never the locker, clears the
  fight, and leaves the signal at zero until the day rolls
  (`Sheet::is_down`: a step is refused, the strip says `signal down`,
  the road is over for the day). `Refused` changes nothing and is not
  stored.
- **The day rolls lazily.** `Sheet::settle(today)` on every locked touch
  (an action or the descent's reload): the first touch after midnight UTC
  refills signal to `level * 10` and rations to ten, clears the day's
  glass, drops a hanging fight, and wipes the road (the steps taken and
  the static in the deck). Nothing regenerates in between, and nothing
  accrues: the debt is not touched by the roll.
- **The balance is a test.** `fight/sim.rs` plays a runner by a rule
  through the real machine, a road a day, and `sim_test.rs` asserts the
  climb to the first mark lands in GAME.md's window: about two weeks for
  `CAREFUL` (reads the hand, patches under three quarters), about three
  for `AMBIENT` (the same runner on the `Auto` key), later for
  `RECKLESS` (the key, patches under a third, spends it all), each never
  faster than the one before; and the lock can not come back:
  `NEGLECTFUL` never visits the armorer or heeds the road's warning until
  its first drop, then plays carefully and steps down from a grim fight,
  and gains a level within a week of that drop. `fight/arena.rs` holds
  the rest of the targets as one contract (the kit tracking the level by
  price and never by a gate, where the bits go, what a fair fight costs
  on the key and read well, the bright glyph, the Old Signal, what
  crystals are worth), and `fight/BALANCE.md` is the manual: the
  targets, the knobs (`Rules`), how to run a pass, and what the last one
  measured. Retune a number until it passes; a failing message names
  every band missed. A target moves only with a design decision, and a
  balance problem is never fixed with a gate.
- **Levels climb on exp, for now.** GAME.md gives that to the operators;
  until they exist, crossing `EXP_TO_ADVANCE[level - 1]` on a win levels
  you in the fight (signal +10 with the new max) and the wire says so.
  The operators replace this; it is the one stated deviation, and the
  step down is what keeps it from locking a runner in.
- **The wire sees the news, never the play-by-play.** `Sheet::news`
  (pure) decides what one command was worth, `FightService::post_news`
  words it, and `ChatService::post_wire_line_task` posts it in
  #deadchannel from the `afterglow` voice (ensures the voice, joins it to
  the room, sends; a failure is logged there). The lines: `Lost` posts
  "<name>'s signal dropped at step S of the road. the street took N
  bits." and nothing else; a win posts at most one story, the biggest of
  a level gained ("<name> is level N." with the portrait's three rows
  under it), the first kill ever ("<name> put down their first <foe>.",
  exact through the row's `kills` count), a bright glyph ("<name> put
  down a bright <foe>. it left a crystal."), or a near miss ("... with S
  signal left", signal at or under `NEAR_MISS_SIGNAL` (3)); and whatever
  the road's last step was (a fight won or run from, a rest, a cache),
  finishing it adds the day's card ("<name> walked the road. K glyphs
  down, R runs, signal S/M.", from `kills_today` and `runs_today`).
  Ordinary kills, cards, turns, runs, rests, caches, and purchases post
  nothing. Outside the fight, the join and the leave post too: the
  welcome on a created runner, "<name> is back on the wire." on a
  `Returned` one, and "<name> went dark." on the leave that stamps
  `left_at` (each once by the same conditional statement that decides the
  beat).
- **Where the mirror comes from.** `App.fight` (`FightSession`) reloads
  at connect for a standing runner (`App::new`), on every descent (`0`
  on the clubhouse, or Enter on the hub card), on every walk up to the static, and on every
  directory edge in `tick.rs` (a leaver drops the mirror there instead),
  so the strip never shows yesterday's bars and the road is today's;
  each action answers with the stored sheet. Another device's action
  shows up on the next descent, action, or level change (the migration
  202 trigger notifies on `level`), the acceptable case (root
  CONTEXT.md, "Replica-ready, not over-engineered"): no per-card notify.
  `tick.rs` drains the session every tick; the city's `anim_half` edge
  paints the answer. The street's strip is the only readout of the
  bars; the frame's status HUD outside the city does not carry them.
  `/haunt status` prints the whole sheet as this session holds it. The
  mirror is not rolled at midnight by itself: an idle session shows
  yesterday's bars until the next reload (a descent, an action, or a
  directory edge), the accepted case. The profile's runner column reads
  the row directly and settles the parsed copy for the view
  (`ProfileService::do_find_profile`, nothing written), so it never
  shows a dead signal after the roll.
- **The armorer.** The same sheet, the same lock, one more command
  (`Command::Outfit { slot, tier }`), because the sheet has one writer.
  The panel (`city/ui.rs::armorer_lines`) is the wall: fifteen rows, a
  `▸` cursor (`State::picked_tier`, up/down or `j`/`k`, kept across
  visits), what you carry lit and the tiers under it dimmed, per slot;
  the keys line prices the picked row for `[w]` and `[a]` net of the
  trade-in (`Sheet::outfit_price`: the price on the wall less
  `TRADE_IN_PERCENT` of the carried piece's price, rounded down; nothing
  for bare hands), red when you are short, "you carry that, or better"
  when it is not above your tier. The wall's price is `COST_LADDER` at
  `PRICE_PERCENT` (`Rules::price`, `wall_price` for the panel), tuned so
  a level pays for about its own tier (`fight/BALANCE.md`); there is no
  level gate. The rule is the same in `Sheet::outfit`:
  above what you carry and on the wall (`Refusal::NotAnUpgrade`, the
  armorer does not sell down), paid in full (`Refusal::Short { by }`), or
  nothing moves. `Applied::Outfitted` sets the tier and takes the bits.
  The answer lands as the session's `till` line (the panel shows it in
  cyan; stepping in again clears it) because no scene is open; the wire
  hears nothing, the piece shows up on the cards (a strike hits for
  more, a block holds more), in the hit line ("your tire iron hits the
  howler for 9."), on the strip, under the scene's header, and on the
  sheet. Bits only, no credit, no chips.
- **Patch.** The heal, same lock, one command (`Command::Patch`): the
  signal back to full for `Sheet::patch_price`, a bit a point times
  the level (`(max - signal) * level` at `PATCH_PERCENT`, rounded up),
  paid in full or nothing moves
  (`Refusal::Short`). A dropped signal is the roll's, not patch's
  (`Refusal::SignalDown`: the day is the day), a fight waiting on the
  row is finished first (`Refusal::FightWaiting`: leave the page, patch,
  step back in would be a free heal mid-fight), a runner spent for the day is
  sold nothing (`Refusal::NoRations`: no fight can spend the signal
  before the roll refills it for free), and a full signal buys nothing
  (`Refusal::NothingToPatch`). It mends the signal only: the static in
  the deck is a rest's to clear. `Applied::Patched { restored, paid }`,
  the `patched` beat, no news. `p` on the street opens the panel from
  anywhere (`city/input.rs`), so the heal is one key from any step; the
  armorer and the tailor are not, on purpose, so the street still gets
  walked. The panel (`city/ui.rs::patch_lines`) shows
  the signal and the bits, spells the refusal that would come ahead of
  the `[p]` key, and prices the key red when short; the answer is the
  `till` line like the armorer's.
- **Not yet:** bands and charge (the surge and the wipe are the
  bandless deck's two moves; a band's replace them), events and card
  drops on the road, the operators (levels climb on exp until they
  exist; the step down is what keeps that from locking a runner in), the
  board's standing orders, and the tailor's bought rack (chips, seasonal
  stock).

## 4. Persistence (`users.settings`, late-core `User`; `deadchannel_runners`)

- `deadchannel_runners` (migration 172, model
  `late-core/src/models/deadchannel_runner.rs`): one row per user
  (`user_id` unique, cascade on delete), `look` JSONB in the shape
  `{"hood": {"piece", "tint"}, "eyes": ..., "coat": ..., "mark": {"glyph"}}`,
  `left_at` (migration 187), the leave stamp, `guide_seen_at`
  (migration 203, the first descent's claim: `mark_guide_seen`, a
  conditional update on the standing row, fires no trigger), and the sheet (migration
  199): `level`, `exp`, `signal`, `weapon_tier`, `armor_tier`, `bits`,
  `rations_left`, `day` (the UTC date of the last roll), `fight`
  (JSONB, the fight in progress or null: the glyph, the round, and the
  three piles of the deck), and the tally (migration 201):
  `kills` (ever), `kills_today`, `runs_today` (zeroed by the day roll,
  the last-ration line's numbers), migration 221: `crystals`
  (`CHECK >= 0`) and `drink` (the day's glass, a code from the closed
  `Drink` menu, cleared by the day roll), and migration 205: `peak_level` (the
  highest level ever reached, the tailor's gate) and `marks` (Old Signal
  kills); 205 also rewrote stored fights from `{"kind": n}` to
  `{"quarry": {"glyph": n}}`; migration 210: `unpaid_mark`; migration
  214: `stash` (the locker) and `debt` (the bits machine), whole bits,
  `CHECK >= 0`; migration 225: `road` (JSONB, the day's `RoadRun`:
  `{"path": [{"lane", "mark"}], "static_cards"}`, null before the first
  step and after the roll), which also dropped every fight stored before
  the round (they had no deck) and handed the day's rations back whole;
  migration 226: `cards` (JSONB, the drafted cards in draft order,
  `["siphon", "bulwark"]`, null with none taken), which also handed the
  day back to any runner caught mid-fight (a fight stored before it has
  no `muted` flag) and redefined the nuke to clear the column.
  Migration 222 defines `deadchannel_nuke_runners()`,
  which puts every sheet back to a fresh one for a balance test (the
  row, the look, and the leave and guide stamps stay) and calls it; to
  nuke again ship a migration holding only `SELECT
  deadchannel_nuke_runners();`, and a migration that adds a sheet column
  adds it to that function. Migration 223: `reset_generation`, bumped by
  every nuke and by nothing else; the Old Signal's payout key is
  `<runner row id>:<generation>:<mark>`, so a mark earned again after a
  nuke is paid, and the nuke leaves `unpaid_mark` standing (chips the
  house still owes). Created only by the invited
  join with the column defaults (level 1, signal 10, 50 bits, ten
  rations, today). The look is written again only by the tailor's
  mirror (`store_look`, standing runner only) and by migration 204,
  which re-rolled the slots that sat above the runner's level into the
  level-1 rack. An insert, or an update
  of `look`, `left_at`, or `level` (migration 202; `peak_level` and
  `marks` only move with the level), fires
  `deadchannel_runner_changed` (payload: the user id, for logs only;
  listeners re-read every standing row); a sheet write that leaves the
  level alone fires nothing, so ten fights a day per runner wake no
  directory, and a level gained reaches every badge and HUD.
  - The sheet is written whole by `store_sheet` under `lock_standing`
    (`SELECT ... FOR UPDATE` on the standing row: `None` for a runner who
    left, so the door being shut also shuts the fight). The service is
    the only caller of both.
  - `/leave #deadchannel` stamps `left_at` (`mark_left`, conditional on
    the stamp being absent, so leaving twice writes and notifies once) and
    never deletes: the character keeps its row, its id, and its face, and
    an invited rejoin clears the stamp and gets that face back
    (`ensure_for_user`, whose `RunnerOrigin` is `Created`, `Returned`, or
    `Existing`, one statement per outcome). A stamp is also what keeps the
    directory honest, because the trigger fires on insert and update only:
    a delete would notify nobody and leave the undercity open on every
    replica that missed it.
  - `list_standing` serves only rows with `left_at IS NULL`, so one write
    closes the gate and drops the portrait everywhere. A runner who left
    disappears from the #deadchannel gutter retroactively, old messages
    included: going dark takes the face with it.

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

- `/haunt` - status: whether stage 1 and the chosen
  stages armed for this session, the gate's three legs (active hours,
  touched settings, bio length and standing), glitch schedule, glitch
  and name hit counters against their caps, the witness (whether a beat
  of somebody else's is on this screen), door, whisper, and the
  breakthrough or invite.
- `/haunt arm` - forces this session chosen and arms the repeatable
  machines, so a beat (and the gate) is testable without reconnecting or
  a passing bio. Session only; nothing is written.
- `/haunt glitch` - fire a clock burst on a ~7s fuse (the banner covers
  the clock for ~5s), bypassing schedule and caps.
- `/haunt name` - force the next own send to flicker. Skips the row's
  caps, not the wire: the forced beat travels like a real one, which is
  how the public half of stage 2 is watched from a second session.
- `/haunt replay` - re-run the splash whisper now, ignoring the marks.
- `/haunt invite` - the next own send breaks through, skipping the delay;
  the DM follows exactly as for a real one.
- `/haunt reset` - wipe every mark; the chain starts over.
- `/haunt welcome` - post the runner's welcome on the wire for this user
  now, exactly what a fresh invited join posts; nothing else changes.

## 6. Gotchas

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
  The `mark` is painted as the level badge in the #deadchannel author
  header and the heading of the profile's runner column; the clubhouse floor glyph
  is the next slice.
- Where to look when the ladder seems dead (per person, in the logs, all
  keyed by `user_id` and `username`, the two fields every deadchannel line
  carries): `first contact gate evaluated` at every staff connect
  with each leg's number, the bio standing, and the `GateVerdict`
  (nobody else gets a line: the gate is shut before any read); `first
  contact armed` for every session that can fire stage 1, with `chosen`
  and `whisper_armed`; then one line per hit, whisper, breakthrough, invitation, bio
  screen, and runner. How many the gate turns away, and on which leg, is
  `late_ssh_first_contact_gate_total{verdict}` (one count per
  connect, not per person); bio screens by outcome are
  `late_ssh_first_contact_bio_screens_total`; delivered beats are
  `late_ssh_first_contact_beats_total`. The street's wire is
  presence's, `late_ssh_presence_total{beat}` (root CONTEXT.md §7). The
  game's own counters sit under the same row: fights by beat (the per-
  exchange `round` beat left out), the tailor and the door, Old Signal
  payouts. The name is a log field only, never
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
- A fight action holds a pooled connection while it waits for the row
  lock, so `FightSession::request` drops a press while one is out (the
  bonsai rule); the scene shows `the static is deciding.` meanwhile.
- The fight's dice are `rand::thread_rng` in the service; the state
  takes any `Rng`, so `state_test` drives a fight to the end with a
  seeded `StdRng` and asserts the whole sheet.
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
