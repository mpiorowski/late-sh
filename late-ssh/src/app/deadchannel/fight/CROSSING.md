# The crossing, as built (late-ssh/src/app/deadchannel/fight)

Parent: `../CONTEXT.md` §3c (the fight: the row, the lock, the day roll,
the wire). Design and the reasons: `../GAME.md`, "The crossing pass".
One quiet step of every road holds a `Node::Crossing` on one lane beside
a rest and a cache, and who waits there is another runner, asleep. The
one thing in the fight that touches two rows.

## The step

The session's Enter there is `FightService::cross_task`, not a
`Command`: the private `Ask::Cross { lane, username }` beside
`Ask::Command`. Under the attacker's lock the service:

- lists the standing runners within `CROSSING_LEVEL_BAND` (2) levels
  (`candidates`), drops the ones the day's crossings already count
  (`deadchannel_ambushes`, migration 240: a sleeper is crossed once a
  day, a pair crosses once a day, both unique partial indexes; the
  filter mirrors them, the indexes decide), drops the ones whose row as
  it stands unlocked is not on the wire or has no answer in it
  (`settled`, `can_be_crossed`: signal up after the roll, no fight, in
  the band, and `sim::can_answer_back`: the sleeper's own fight against
  the attacker's numbers, read clean at full signal with no static on
  the `Auto` key over `ODDS_FIGHTS`, reads risky or better; a sleeper
  the read calls grim is walked past, so nobody is crossed by a runner
  they could not beat back, and the gate follows the cards and the kit
  wherever a balance pass moves them: below level 6 that is the same
  level or upward, from about level 8 two levels either way), and
  orders the rest by `crossing_order` (seeded by the day and the
  attacker, so the road previews the name the step meets);
- locks the first whose row still stands (`find_prey`), rolls their day
  under the lock (`settle_day`: a rival fight the roll wipes closes its
  ambush row as dropped) and stores it if it turned, claims the ambush
  row (`stake`, `Sheet::crossing_stake`: `CROSSING_TAKE_PERCENT`, half,
  of their bits on hand, rounded down; a cap the preview missed is the
  insert's `ON CONFLICT DO NOTHING`, and the next in order is tried),
  and hands the sheet a `Prey` (`Sheet::cross(lane, Option<Prey>, rng)`:
  the ration, the step marked, `Quarry::Runner(Rival)` on the row with
  the sleeper's full signal, attack, and defense as the glyph's numbers,
  `data::RUNNER_PATTERN` as its mind; their cards do not play).

Nobody left is `Refusal::NobodyThere`, nothing spent. The fight is the
normal round.

## The settle

A win takes half of what the sleeper carries, not all of it, so a
night carrying is a bet and the locker (`MONEY.md`, a tenth on the way
in) a choice: it is the cheaper one only for a runner crossed more often
than one night in five. The road and the scene say both numbers
(`data::carrying_line`: "carrying 340 bits, 170 to take").

`Applied::Ambushed { rival, stake }` moves nothing on the sheet: the same
transaction locks the sleeper's row (`settle_won`), takes up to the
stake off their hand (`Sheet::robbed(stake, Reach::Hand)`; what they
carry now, so a deposit mid-fight shrinks it; an answer reaches
`Reach::HandAndLocker`, below), puts it on the attacker's with the garnish
(`Sheet::earn`), closes the row as won with `bits_moved`, and the scene
gets the took line. `Applied::Downed { rival, bits_lost }` is the
attacker's full fall (signal down, every bit on hand, the exp cut) with
the bits onto the sleeper's row (`settle_lost`) and the row closed as
lost. A run closes it as ran. A rival fight the day roll drops is closed
as dropped by `settle_day`, on whichever touch rolls that row: the
runner's own command or reload, or another runner finding them asleep or
answering them. No exp, no kill counted. The sleeper's signal, exp, and
day are untouched.

Two rows lock in one transaction, attacker first, so two runners
crossing each other at the same instant can deadlock; Postgres breaks
it and one answers `ActionFailed`, which the scene shows as the static
not answering.

## The answer

`answer_task` (`r` on the road), `Ask::Answer { username }`: the latest
crossing won against this runner inside `ANSWER_HOURS` (48, two days,
so a runner who connects once a day and finds the crosser mid-fight the
first evening still has the second) and not yet answered
(`Ambush::answerable`), the one who did it locked and claimed as an
answer row (`answer_to`, one per crossing), on the row as a rival
through `Sheet::answer(Answer, rng)` with no ration and no step. The
stake is the answer's (`Prey::of` with `Meeting::Answer { owed }`,
`Sheet::answer_stake`): what the crossing took (the crossing row's
`bits_moved`) or half of what the crosser holds on hand, whichever is
more, never more than hand and locker together, and a win takes it out
of the locker too (`settle_won`, `Reach::HandAndLocker`), so banking the
loot is no escape; the road and the scene show what they hold in all.
The rival carries the name the crosser goes by now
(`current_username`), not the one the crossing row kept: the wire
mentions it, and a mention finds its runner by the current handle; the
lookout's token (`Token::crosser`, the label and key under the road)
reads the same name. Refused with nothing owed (`NoAnswerOwed`), with
them gone dark, down, or in a fight (`NotOnTheWire`: the answer keeps),
down, or with a fight waiting.

## The lookout

After every answer and reload the task reads a `Lookout` for the session
(`lookout_for`, off the rows as they stand, outside the transaction):
`crossing` (the preview, `Prey` with a nil ambush id), `token` (the
crossing owed an answer, the crosser's current name, and the crosser as
prey when on the wire, at the answer's stake), `news` (the latest fight on this runner in the
window, worded with the names the row kept that night), all `None` while
a fight is on the row. A lookout that fails to read is logged and
counted (`lookout_failed`) and the session gets the empty one: the
answer that landed is never reported failed for it. The road draws the
lookout under the legend and the threat words come from
`sim::odds_rival`.

## The wire and the beats

Every runner on a line is an `@` mention, so it lands in their mentions
too, and every line says who lost what to whom (a crossing moves bits
on hand only; a fall also drops the signal). A fight can sit on the row
until the next roll, so the rival is named as they go by when the line
is posted (`wire_name`), not as the fight kept them; the scene's own
lines keep the fight's name. The wording:
"@<a> crossed @<d> at step S and won. @<d> never woke and lost N bits to
@<a>.", "@<a> crossed @<d> at step S and went down. @<d> never woke. @<a>
lost their signal and N bits to @<d>.", "@<a> answered @<d> and won. @<d>
lost N bits to @<a>.", "@<a> answered @<d> and went down again. @<a> lost
their signal and N bits to @<d>."; with nothing taken, the loser "lost
nothing" (a win) or "lost their signal, with no bits on hand for @<d>"
(a fall). Beats on the fight counter: `crossed`,
`answered`, `ambushed`, `downed`.
