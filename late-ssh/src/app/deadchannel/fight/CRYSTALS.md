# The crystal pass, as built (late-ssh/src/app/deadchannel/fight)

Parent: `../CONTEXT.md` §3c (the fight: the row, the lock, the day roll,
the wire). Design and the reasons: `../GAME.md`, "The crystal pass". This
file is the contract of what keeps the daily loop from being one key held
down: crystals, the bright glyph, Dead Air, and the blade cart. How
they are measured and tuned is `BALANCE.md`. Everything here is a command
or a rule on the same sheet under the same row lock (`fight/state.rs`,
`fight/svc.rs`); nothing has a writer of its own.

## Crystals

`deadchannel_runners.crystals` (migration 221), on the sheet as
`Sheet::crystals`: the one thing bits cannot buy.

- A kill of the glyph of your level leaves one, one time in
  `CRYSTAL_DROP_ONE_IN` (12), rolled in `put_down` after the kill line.
  A step down never leaves one (the glyph's level is below the runner's).
  A bright glyph always leaves one.
- `Applied::Won { crystals, .. }` carries it and `CRYSTAL_LINE` says it
  under the kill.
- A dropped signal never takes them. An Old Signal mark and a step off
  the ledge do, and a runner holding only crystals has something to lose
  at the ledge.
- Shown on the picker's budget row, on the strip when there are any, and
  at the bar and the cart.

## The bright glyph

`Pick::Bright`, the third offer in the picker, key `b`.

- It waits behind `BRIGHT_STEPS_PER_DAY` (2) steps of every day.
  `data::bright_steps(day)` is a pure function of the UTC date: the same
  two steps for every runner (one object for the room to talk about),
  no column, no reroll. `Sheet::step` is the step the next ration buys
  (1 to 10); `Sheet::bright_waits` is whether one is behind it.
- The offer is for that step only: spending the ration on anything else
  walks past it.
- It is the glyph of the runner's level lifted (`bright_foe_for_level`):
  `BRIGHT_SIGNAL_PERCENT` (135) of the signal, `BRIGHT_EDGE_PERCENT`
  (115, rounded up) of the attack and defense, `BRIGHT_BITS_TIMES` (2)
  the bits, the plain glyph's exp. The exp stays plain on purpose: the
  climb's pace is the rations', and the bright one is for the crystal.
- `Fight::bright` is on the row (absent on fights stored before it,
  which read as plain); `Fight::name` puts `bright` ahead of the kind
  in every line.
- Off its step it is `Refusal::NoBrightGlyph` and the ration is kept.
- The wire hears a bright kill (`News::BrightDown`), after a level and a
  first kill, ahead of a near miss.
- At the gate the fair pick is the Old Signal and the bright one is
  still the glyph of 15.

## Dead Air

`Command::Drink { drink }`, a glass for `DRINK_CRYSTALS` (1), one a day.
`deadchannel_runners.drink` holds the code; the day roll clears it.

| Key | Glass | Until the roll |
|---|---|---|
| `s` | static on ice | attack + `drink_edge(level)` (1, and 1 more every 4 levels) |
| `d` | dead air, neat | defense + `drink_edge(level)` |
| `t` | test pattern | max signal + 2 a level, and the signal filled |

- The menu is the closed `Drink` enum; a code on the row that is not on
  it is `SheetError::Drink`, never a blank.
- Refusals, in the order the row checks: the signal down (`SignalDown`),
  a glyph waiting (`FightWaiting`), spent for the day (`NoRations`: no
  fight is left to use it in), a glass already poured (`GlassPoured`),
  no crystal (`ShortCrystals`).
- The panel (`city/ui.rs::bar_lines`) shows what each glass does at this
  level and spells the refusal ahead of the keys; the answer is the
  `till` line; no news.

## The blade cart

`Command::Cart { slot }`: the next tier up from what the slot carries
(`Sheet::cart_tier`), for `CART_CRYSTALS` (3) crystals and no bits.

- The armorer sells the same piece for bits; the cart is the other
  wallet. The panel prints what the wall asks beside the crystal price,
  so the trade is in front of the runner.
- One tier a visit, each slot on its own ladder. The carried piece is
  not traded in: it is replaced.
- Refusals: `PastTheWall` when the slot holds tier 15 (nothing is made
  past the top of the wall), `ShortCrystals`.
- Enter at the cart opens its panel (`city/ui.rs::cart_lines`, `w` and
  `a`); the answer is the `till` line; no news.

## Measured

The numbers (the bright glyph's odds with and without a glass, the Old
Signal with a glass, crystals a day, what the keen runner gains) are in
`BALANCE.md`, section 6, with the targets they are held to.
