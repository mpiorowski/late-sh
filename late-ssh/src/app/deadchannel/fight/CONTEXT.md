# The road and the round (the static at the end of the row)

Part of Dead Channel (`../CONTEXT.md`; design in `../GAME.md`). The round and the screens, and every file in depth, are in `MACHINE.md`; the money, crossing, crystal and balance detail sits beside it in `MONEY.md`, `CROSSING.md`, `CRYSTALS.md` and `BALANCE.md`.

## Files

| File | Owns |
|---|---|
| `fight/data.rs` | The numbers: LoGD's ladders transcribed, plus the ones the climb sets (`Rules`, `BALANCE.md`) |
| `fight/cards.rs` | The deck and the hand: `Card`, `DRAFTS`, `Card::effect` (what each card does), the piles |
| `fight/road.rs` | `road_for(day)`: the day's road, a pure function of the UTC date, and the run on it |
| `fight/policy.rs` | How a turn is played without a person choosing: `Table`, `auto` (the `Auto` key), the sharp read |
| `fight/share.rs` | `road_card(sheet)`: the day's road as a share card once it is over |
| `fight/state.rs` | The pure machine: `Sheet`, `Fight`, `Command`, `Applied`, `Refusal`, `settle`, `news` |
| `fight/svc.rs` | `FightService`, the one writer: lock the row, settle, apply, store, commit; metrics, logs, the wire's news |
| `fight/arena.rs` | Test-only balance arena (`BALANCE.md`): matchups, the targets as one contract |
| `fight/sim.rs` | The balance harness: simulated runners walking real roads through the real machine |
| `fight/session.rs` | `FightSession`, the session's side: the sheet mirror, the lookout, the `Picker`, the `Scene` |
| `fight/input.rs` | Keys while the road or a scene is open |
| `fight/ui.rs` | `draw_picker` (the road) and `draw_scene` (a fight, the Old Signal's torn frame) |

Every file with logic has a `*_test.rs` sibling. Detail per file: `MACHINE.md`, "Files in depth". Run the slice scoped: `make test-llm ARGS="-p late-ssh --lib -E 'test(deadchannel::fight)'"`.

../GAME.md, "The road pass": the screen closing the street is where the
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
  pay; `MONEY.md`), a bright node `Fight(Bright)`
  (`CRYSTALS.md`), a rest `Clear` (every static card out of the
  deck, the signal untouched; taken with a clean deck too, the ration
  spent, so a road of rests never blocks a step), a cache `Take` (`CACHE_PERCENT` of what the glyph of the level
  pays, the bits machine's garnish first). A lane out of reach is
  `Refusal::NotThatWay` and a call the node does not answer is
  `Refusal::WrongCall`; neither spends anything. At level 15 with the exp
  to leave it, a glyph's node meets the Old Signal instead.
- **The round, the draft, the cards, static, and the row.** A fight is a hand of ten cards, five drawn a turn, three energy, against a glyph whose next move is shown a turn ahead; four drafts (levels 3, 6, 9, 12) swap a strike or a block for a drafted card; a hit through the block leaves static in the deck for the day; the fight lives on the row (`fight` JSONB) so a dropped session or a second device finds the same hand. All of it in `MACHINE.md`, with the two screens (the road panel and the scene, the Old Signal's torn frame).
- **The day rolls lazily.** `Sheet::settle(today)` on every locked touch
  (an action or the descent's reload): the first touch after midnight UTC
  refills signal to `level * 10` and rations to ten, clears the day's
  glass, drops a hanging fight, and wipes the road (the steps taken and
  the static in the deck). Nothing regenerates in between, and nothing
  accrues: the debt is not touched by the roll.
- **The balance is a test.** `fight/sim.rs` plays a runner by a rule
  through the real machine, a road a day, and `sim_test.rs` asserts the
  climb to the first mark lands in ../GAME.md's window: about two weeks for
  `CAREFUL` (reads the hand, spends the day's patch once two fifths of
  the signal are gone), about three for `AMBIENT` (the same runner on
  the `Auto` key), later for `RECKLESS` (the key, the patch under a
  third, spends it all), each never
  faster than the one before; and the lock can not come back:
  `NEGLECTFUL` never visits the armorer or heeds the road's warning until
  its first drop, then plays carefully and steps down from a grim fight,
  and gains a level within a week of that drop. `fight/arena.rs` holds
  the rest of the targets as one contract (the kit tracking the level by
  price and never by a gate, where the bits go, what a fair fight costs
  on the key and read well, the bright glyph, the Old Signal, what
  crystals are worth), and `BALANCE.md` is the manual: the
  targets, the knobs (`Rules`), how to run a pass, and what the last one
  measured. Retune a number until it passes; a failing message names
  every band missed. A target moves only with a design decision, and a
  balance problem is never fixed with a gate.
- **Levels climb on exp, for now.** ../GAME.md gives that to the operators;
  until they exist, crossing `EXP_TO_ADVANCE[level - 1]` on a win levels
  you in the fight (signal +10 with the new max) and the wire says so.
  The operators replace this; it is the one stated deviation, and the
  step down is what keeps it from locking a runner in.
- **The wire sees the news, never the play-by-play.** `Sheet::news`
  (pure) decides what one command was worth, `FightService::post_news`
  words it, and `ChatService::post_wire_line_task` posts it in
  #deadchannel from the `afterglow` voice (ensures the voice, joins it to
  the room, sends; a failure is logged there). Every runner a line names
  is an `@` mention, so the line lands in their mentions (a trailing full
  stop is the sentence's: `common/mentions.rs`). The lines: `Lost` posts
  "@<name>'s signal dropped at step S of the road. the street took N
  bits." and nothing else; a crossing posts who lost what to whom
  (`CROSSING.md`); a win posts at most one story, the biggest of
  a level gained ("@<name> is level N." with the portrait's three rows
  under it), the first kill ever ("@<name> put down their first <foe>.",
  exact through the row's `kills` count), a bright glyph ("@<name> put
  down a bright <foe>. it left a crystal."), or a near miss ("... with S
  signal left", signal at or under `NEAR_MISS_SIGNAL` (3)); and whatever
  the road's last step was (a fight won or run from, a rest, a cache),
  finishing it adds the day's card ("@<name> walked the road. K glyphs
  down, R runs, signal S/M.", from `kills_today` and `runs_today`).
  Ordinary kills, cards, turns, runs, rests, caches, and purchases post
  nothing. Outside the fight, the join and the leave post too: the
  welcome on a created runner ("@<name>. you got through. ..."),
  "@<name> is back on the wire." on a `Returned` one, and "<name> went
  dark." on the leave that stamps `left_at`, said and not mentioned
  because the leaver asked the wire to go quiet (each once by the same
  conditional statement that decides the beat).
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
  `TRADE_IN_PERCENT` of the carried piece's ladder price, rounded down;
  nothing for bare hands), red when you are short, "you carry that, or better"
  when it is not above your tier. The wall's price is `COST_LADDER` at
  `PRICE_PERCENT`, and `PRICE_LEAD_PERCENT` more for every tier the piece
  would sit above the level (`Rules::price`, `wall_price` for the panel;
  the trade-in is read off the ladder, never the lead), tuned so
  a level pays for about its own tier (`BALANCE.md`); there is no
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
- **Patch.** The heal, same lock, one command (`Command::Patch`), once
  a day: the signal back to full for `Sheet::patch_price`, a bit a
  point times the level (`(max - signal) * level` at `PATCH_PERCENT`,
  rounded up), paid in full or nothing moves (`Refusal::Short`). The
  day's patch is `RoadRun::patched` on the row's `road`, wiped by the
  roll like the rest of the day; a second one is `Refusal::PatchedToday`
  (../GAME.md, "One patch a day": the signal is a budget of two bars a
  day, and the siphon in the deck is the only heal between them). A
  dropped signal is the roll's, not patch's
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
- **The crossing.** ../GAME.md, "The crossing pass": one quiet step of
  every road holds a `Node::Crossing` on one lane beside a rest and a
  cache, and who waits there is another runner, asleep. Enter there
  (`FightService::cross_task`) fights them for what they carry, the
  answer they are owed is `r` on the road (`answer_task`), and the two
  rows settle in one transaction over `deadchannel_ambushes` (migration
  240). The contract (who is met and in what order, the caps as
  indexes, the settle, the day roll dropping a hanging fight, the
  lookout the session reads, the wire lines) is `CROSSING.md`.
- **Not yet:** bands and charge (the surge and the wipe are the
  bandless deck's two moves; a band's replace them), events and card
  drops on the road, the operators (levels climb on exp until they
  exist; the step down is what keeps that from locking a runner in), the
  board's standing orders, and the tailor's bought rack (chips, seasonal
  stock).
