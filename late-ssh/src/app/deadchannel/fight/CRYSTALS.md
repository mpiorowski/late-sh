# The crystal pass, as built (late-ssh/src/app/deadchannel/fight)

Parent: `../CONTEXT.md` §3c (the fight: the row, the lock, the day roll,
the wire). Design and the reasons: `../GAME.md`, "The crystal pass". This
file is the contract of the things bits cannot buy: crystals, the bright
glyph, Dead Air, and the blade shop. How
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
- Shown on the road's budget row, on the strip when there are any, and
  at the bar and the blade shop.

## The bright glyph

`Pick::Bright`, the call a `Node::Bright` answers, Enter with the road's
cursor on it.

- Two nodes of every day's road are bright (`road::BRIGHT_NODES`), each
  on a fight step of its own and never the first, one lane each.
  `road::road_for(day)` is a pure function of the UTC date: the same
  two nodes for every runner (one object for the room to talk about),
  no column, no reroll. The map shows them from the first step, burning
  amber, so a runner walks to one or around it.
- The node answers only the bright call: a plain fight asked of it is
  `Refusal::WrongCall` and the ration is kept, as is the bright call
  asked of a plain glyph's node.
- It is the glyph of the runner's level lifted (`bright_foe_for_level`):
  `BRIGHT_SIGNAL_PERCENT` (150) of the signal, `Rules::bright_edge`
  (`BRIGHT_EDGE_LEVEL_PERCENT` of the level added to the attack and
  `BRIGHT_GUARD_LEVEL_PERCENT` of it to the defense, each rounded up),
  `BRIGHT_BITS_TIMES` (2) the bits, the plain glyph's exp, the same
  pattern of moves. The edge is a share of the level and not of the
  glyph's own numbers on purpose: a percentage rounded up doubled the
  flicker's attack and added a sixth to the level 15 glyph's, so the
  bright one was a wall on day one and a speed bump at the top. Attack
  and defense are two knobs because a point of defense at level 4 takes
  a quarter off a strike that is only five points. The exp
  stays plain on purpose: every road has the same five fights whichever
  lanes are walked, so the climb's pace is the rations', and the bright
  one is for the crystal.
- `Fight::bright` is on the row; `Fight::name` puts `bright` ahead of
  the kind in every line, the scene is dressed in amber
  (`fight/ui.rs::bright_dress`), and the step is marked `BrightWon` on
  the run when it goes down.
- The wire hears a bright kill (`News::BrightDown`), after a level and a
  first kill, ahead of a near miss.
- At the gate a glyph's node is the Old Signal and a bright node is
  still the bright glyph of 15.

## Dead Air

`Command::Drink { drink }`, a glass for `DRINK_CRYSTALS` (1), one a day.
`deadchannel_runners.drink` holds the code; the day roll clears it.

| Key | Glass | Until the roll |
|---|---|---|
| `s` | static on ice | attack + `drink_edge(level)` (1, and 1 more every 4 levels) |
| `d` | dead air, neat | defense + `drink_edge(level)` |
| `t` | test pattern | max signal + 2 a level, and the signal filled |
| `f` | feedback | a surge in the deck |
| `u` | squelch | a burn in the deck |
| `o` | sign-off | a mute in the deck |

- The last three are pours (GAME.md, "Dead Air pours a card"): a card
  in every deck dealt today (`Drink::pour`, added by `Sheet::deck`), not
  a point on the sheet. A fight already on the row keeps the deck it was
  dealt. Every pour is a card the key plays well: the arena holds that
  no pour costs the bright fight on auto more than the dice
  (`BALANCE.md`, section 1); a wipe poured into a clean deck did (lv 9:
  48% dry, 36% poured), which is why squelch is a burn.
- The menu is the closed `Drink` enum; a code on the row that is not on
  it is `SheetError::Drink`, never a blank.
- Refusals, in the order the row checks: the signal down (`SignalDown`),
  a glyph waiting (`FightWaiting`), spent for the day (`NoRations`: no
  fight is left to use it in), a glass already poured (`GlassPoured`),
  no crystal (`ShortCrystals`).
- The panel (`city/ui.rs::bar_lines`) shows what each glass does at this
  level, names each pour's card in its neon with the card's rule, and
  spells the refusal ahead of the keys; the answer is the `till` line;
  no news.

## The blade shop

`Command::Cart { slot }`: the next tier up from what the slot carries
(`Sheet::cart_tier`), for `Sheet::cart_price` crystals and no bits:
`CART_CRYSTALS` (3) for a piece at or under the level, and
`CART_LEAD_CRYSTALS` (2) more for every tier it would put the slot above
the level (`Rules::cart_price`). Priced by the lead because at a flat
three the keen runner's two crystals a day bought four tiers over the
level by level 9 and the whole wall at 11 (`BALANCE.md`, section 6).

- The armorer sells the same piece for bits; this is the other
  wallet. The panel prints what the wall asks beside the crystal price,
  so the trade is in front of the runner.
- One tier a visit, each slot on its own ladder. The carried piece is
  not traded in: it is replaced.
- Refusals: `PastTheWall` when the slot holds tier 15 (nothing is made
  past the top of the wall), `ShortCrystals`.
- Enter inside opens its panel (`city/ui.rs::cart_lines`, `w` and
  `a`); the answer is the `till` line; no news.

## INK

`Command::Ink { level }`: the draft at `level` inked, for
`Sheet::ink_price` crystals (`INK_CRYSTALS`, 10; `Rules::ink_crystals`)
and no bits. An inked draft is both of its cards: the pick goes in over
the card the draft replaces as always, and the other option comes in
beside it, from the draft's level on (`cards::deck(drafted, inked)`,
`Draft::other`). `deadchannel_runners.inks` holds the levels in the
order they were bought (migration 238); a mark and the ledge leave it,
only the nuke clears it (GAME.md, "INK: the needle").

- The needle takes one draft for every mark the runner holds, and one
  more (`Sheet::ink_room`): a first climb inks one draft, and every
  mark opens one more. A draft not yet reached can be inked ahead; its
  cards come when its pick does.
- Refusals, in the order the row checks (`Sheet::needle_refused`): a
  glyph waiting (`FightWaiting`: the deck on the table is the one that
  was dealt), no draft at that level (`NoDraftThere`), already inked
  (`AlreadyInked`), the needle done until the next mark (`NeedleWaits`),
  short (`ShortCrystals`).
- A stored level no draft is owed at, or one stored twice, is
  `SheetError::Inks`, never a blank.
- The panel (`city/ui.rs::ink_lines`) lists every draft with its two
  cards, says which are inked, and prices the rest; under the list, in
  rows that are always there, the picked draft's two cards with their
  rules and the card the pick replaces; up/down walk the cursor over the
  drafts and `i` inks the picked one, its refusal in the
  key's row when the needle would not. Never a digit or `q`: those stay
  the globals' over every panel, and a misfired page switch must not buy
  something a mark cannot take back. The answer is the `till` line; no
  news.

## Measured

The numbers (the bright glyph's odds on the `Auto` key and read sharp,
what it leaves of the signal, the Old Signal with a glass, with a pour,
and with every draft inked, crystals a day, what the keen runner gains,
what the needle costs the inker this climb and pays the key on the
next) are in `BALANCE.md`, section 6, with the targets they are held
to.
