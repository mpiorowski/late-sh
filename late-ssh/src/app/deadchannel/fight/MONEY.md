# The money pass, as built (late-ssh/src/app/deadchannel/fight)

Parent: `../CONTEXT.md` §3c (the fight: the row, the lock, the day roll,
the wire). Design and the reasons: `../GAME.md`, "The money pass". This
file is the contract of the five things that keep a runner from locking:
the picker, the step down, the lockers, the bits machine, and the ledge.
All of them are commands on the same sheet under the same row lock
(`fight/state.rs`, `fight/svc.rs`); none has a writer of its own.

## The picker

`fight/ui.rs::draw_picker`, centered over the street at the scene's
width, opened by `FightSession::step_up` (`f`, or Enter at the screen)
unless the mirror shows a fight waiting on the row.

- On top, your face and sheet: level, signal bar, attack and defense with
  the kit, rations, bits, exp toward the next level, the debt when there
  is one.
- Then the offers, each its face, level, numbers, pay, and threat word:
  `[f]` the glyph of your level (at the gate, the Old Signal in red),
  `[g]` the one a level down at half pay (absent at level 1).
- `f` and `g` step in, up/down and Enter do the same with the cursor, Esc
  closes it (`dispatch_escape`, ahead of the scene's arm).
- The threat word is `sim::odds` over the mirror: the fight played to the
  end two hundred times on fixed dice through the real machine, from the
  signal and the kit as they stand, read into `Threat` (nine in ten easy,
  six even, three risky, worse grim). It is the warning LoGD's master
  gave, before the fight instead of after. It is read when the picker
  opens and when the mirror moves, never per frame.
- Opening the picker also reloads the sheet, so the bars it shows are
  after the roll.
- With the signal down or the rations spent the offers are replaced by
  the reason; the keys still ask the row, which is the truth and answers
  with the refusal on the scene.

## The step down

Exp levels you whether the kit is ready or not, and a drop takes every
bit on hand, so a runner who walked past the armorer can stand at level
2, bare-handed and broke, facing a hiss they beat three times in a
hundred. `Pick::Lower` is the way back.

- The glyph a level below (`lower_foe_for_level`), its pay cut to
  `LOWER_PAY_PERCENT` (50; LoGD's slumming pays the lower creature in
  full, the step down here is a way out, not a farm), a line under its
  arrival saying so.
- Refused at level 1 (`Refusal::NoLowerGlyph`) without spending the
  ration.
- At the gate the fair pick is the Old Signal and the step down is the
  glyph of 14.
- `sim_test.rs` pins it: the `NEGLECTFUL` runner gains a level within a
  week of its first drop.

## The lockers

`Command::Deposit` puts everything on hand in the locker (`stash`) less
`LOCKER_FEE_PERCENT` (10, rounded up, so no deposit is free; LoGD's bank
takes nothing and pays interest, the locker pays none).
`Command::Withdraw` takes it all out for free.

- A drop never reaches it; an Old Signal mark and a step off the ledge
  empty it.
- Refusals: a glyph waiting (`FightWaiting`: the bits you carry into a
  fight are the bits you risk), nothing on hand (`NothingOnHand`), a
  deposit the cut would take whole (`DepositAllCut`: a single bit),
  nothing locked up (`LockerEmpty`).
- All or nothing on purpose: a typed amount is a form, and the city is
  not forms.
- The panel (`city/ui.rs::locker_lines`) prices the deposit net of the
  cut and spells the refusal ahead of the key; the answer is the `till`
  line; no news.

## The bits machine

`Command::Borrow` lends everything up to the cap, `LOAN_PER_LEVEL` (50)
times the level less what is owed (`loan_cap`, `loan_room`); a level-2
runner can borrow both tier-1 pieces.

- The fee is flat: `LOAN_FEE_PERCENT` (10, rounded up) of what is handed
  over goes on the debt with the loan (`loan_fee`,
  `Applied::Borrowed { amount, fee }`), once. Borrow 100, owe 110.
- The debt never grows by itself. There is no rate at the day roll, on
  purpose: a roll is any touch on the row, a connect included, so a daily
  rate bills days nobody fought and compounds past what the garnish can
  pay. The most a runner can owe is the cap they borrowed at plus its
  fee.
- The fee counts against the cap: `LoanCapped` at or past it.
- `put_down` pays `GARNISH_PERCENT` (50, rounded down) of every glyph's
  bits to the debt before the rest reaches the hand
  (`Applied::Won::garnished`, a line under the kill), so a debt is paid
  off by playing.
- `Command::Repay` feeds it what the hand holds, up to the debt
  (`NoDebt`, `NothingOnHand`).
- Nothing clears it but paying: not a drop, not a mark, not the ledge.
  Not with a glyph waiting.
- The panel is `city/ui.rs::machine_lines` (the loan priced with its
  fee); the answer is the `till` line; no news.

## The ledge

`Command::Reset`, the runner started over by choice: level 1, exp 0, bare
hands, no bits on hand or in the locker, a level-1 signal.

- What stays is what was earned or owed: the marks and their bonus, the
  peak (the tailor's rack), the kills, the look, today's rations, and
  the debt (else the ledge is loan forgiveness). No starting bits (else a
  broke runner farms them).
- Refusals: the signal down (`SignalDown`: a reset is not a way back on
  the wire before the roll), a glyph waiting (`FightWaiting`), and a
  runner the fall would take nothing from (`NothingToLose`,
  `Sheet::has_something_to_lose`: level 1, no exp, no gear, no bits on
  hand or locked up). The last one is what keeps the wire line from
  being a key to hold down: every step off costs something, so the
  rations bound how often one can be posted.
- Two presses of `r` over the ledge (`city/state.rs::arm_reset`),
  because it cannot be undone. The box bottom-right
  (`city/ledge.rs::LedgeView`) turns red while leaning out and carries
  the row's answer.
- The wire hears it (`News::SteppedOff`: "<name> stepped off the ledge.
  level 1, bare hands, starting over.").
