# The pet grows (what is left)

Status: built, except what this file lists. The pet is a creature every
account has from an egg: it grows up, goes by what it ate today, is
played with and is seen by other people, with nothing it needs, nothing
to lose and nothing for sale. What is built, and
how, is `late-ssh/src/app/pet/CONTEXT.md`. Delete this file when the
last item below is gone or dropped.

## The lesson of the first pet

The first pet had care (feed, water, play, treat, a streak): keys to
press with nothing behind them, removed in migration 181. The root
`AUDIENCE.md` reads every care loop the same way: busy at the level of
actions, and "day 40 is day 4". So nothing here is upkeep.

## The rules anything added has to keep

- **The pet only goes up.** Nothing decays, dies or is lost by staying
  away. An owner who was gone finds it asleep, and glad.
- **Play, never upkeep.** Everything the owner can do gives something;
  skipping all of it costs nothing.
- **Play is clicks and keys on the tile and in the care box.** What an
  owner rarely changes (the name, the animal, away and out, the reset)
  is a slash command, so no key in the care box can undo a pet.
- **Growth is play, capped at a day's worth.** Treats grow a hatched
  pet, one a UTC day at most, and only the first treat or play of a UTC
  day counts toward the bond, so there is nothing to grind.
- **Nothing about the pet is sold.** No unlock, no species, no looks, no
  toys: the Shop knows no pet.

## Unbuilt

### Hide and seek

The pet hiding in another Home tile for its owner to find. A `zen` task:
the tile tree has to offer a hiding place and take the click, and the
pet's box is the only place it is drawn today.

### The treat for a match that ended while away

A match win asks for the day's treat when the session hears its own win
(`daily_tick.own_win` in `app/tick.rs`). A daily match that ends while
its owner is disconnected earns nothing. The fix reads the result row at
the next connect instead of the event; it belongs to the lobby's daily
state, which owns that row.

### The table treat has no source

`TreatKind::Table` (the lucky) is earned by a win in `GameFamily::Table`
(Blackjack, Poker, Asterion, Tron, Ssnake), and no house table raises a
win: they tell the registry who sat down (`RoomGameEvent::SeatJoined`)
and nothing else. The wiring is a second `RoomGameEvent` from each
table's settlement, forwarded to the winner's session as a private
activity event. With treats growing the pet, it is the one family whose
play grows nothing but by door minutes. What counts as a win at each
table is undecided (a paid
Blackjack hand, a Poker pot, a round of Tron or Ssnake).

### The bond has no reader

`bond_days` is counted (the first treat given or toy fetched of a UTC
day) and shown in the care box. Nothing is unlocked by it. A candidate:
more call lines, or a trick only a bonded pet learns.

### Small debts

- The name and the put-away switch reach the owner's other sessions at
  their next connect: they are not on `pet_changed`. The switch is
  written from a task of its own, so `/pet away` and `/pet out` typed
  back to back can land in either order.
- The `pet_companions.adopted_at` column is left from the retired
  unlock: the model no longer reads it. Dropping it is a migration for
  a later release, since a build still running during the deploy selects
  it.
- One call a UTC day and an hour's stay: a call just before midnight and
  one just after put the same pet in two boxes for up to an hour.

## Decisions taken while building

- **An egg for everyone.** The row is made at the first sign in. The
  `pet_companion` unlock is retired, with no refund for those who bought
  it.
- **Treats grow it, not days.** Ready to hatch at 2 days together (the
  day after it arrives: the first day is day 1), so every account gets a
  creature. Then young, adult and elder at 5, 20 and 60 treats eaten: a
  push into playing anything, at the cost of a pet that stays a baby for
  an owner who only chats. Days together stay the count on the tile.
- **Every stage is a pick.** The owner picks cat, dog or bird in the
  care box when the egg is ready, and at young, adult and elder one of
  two drawings of the part that stage adds. The picks stay: eight elders
  a species. The feet come before the face so an adult's wide head never
  sits on a young animal's small feet, and the plain elder face changes
  only its brackets.

  ```text
          young: top row        adult: feet row        elder: face row
  cat     /\_/\  or  |\_/|      > . <  or  d . b       { o.o } or {=o.o=}
  dog     \,_,/  or  /|_|\      \_._/  or  w_._w       { o.o } or {:o.o:}
  bird     \|/   or  ^   ^       " "   or   J L        /{ovo}\ or \{ovo}/
  ```
- **The bird faces front**, its beak between its eyes (`ovo`), like the
  cat and the dog, in place of the first side profile. Its head stays
  small at every stage.
- **A call is a stay.** A pet sent calling shows in its host's box, care
  box and profile for an hour, under its owner's name, and stays in its
  own box too: sending it never empties the sender's tile. Still one
  call a UTC day, whoever it is on. At most two guests are drawn; the
  care box names them all.
- **Pets that existed before.** Reset: every pet starts as an egg, picks
  its animal again, and keeps its name and its last pat.
- **One treat a day**, from the first win in any game or ten minutes in
  a door, and one waiting at most: a treat left in the pouch refuses the
  next day's until it is given.
- **The treat is today's title.** Puzzle "the bookish", Arcade "the
  quick", Match "the cunning", Table "the lucky", Door "the brave", with
  its glyph carried, until the UTC day ends. Nothing is kept from it.
- **No hats, no collars.** Nothing is worn.
- **The reset is two steps**: `/pet reset` says what goes, `/pet reset
  yes` does it. It keeps the name, the day's pat and a waiting treat.
- **Both toys are free.**
- **A waiting treat is not on Home's attention list**: the pouch on the
  tile is the nudge, and the list is for what is owed.
- **The art.** Drawn from parts and pinned whole in `pet/ui_test.rs`:
  the egg, three babies, and both drawings of every part.
- **The daily pat** stays as it was, 100 chips.
- **No global key.** The care box opens from the tile: a right click, a
  click on its status row, or Enter on the tile in arrange mode.

## What would say it worked

- Owners who give a treat or play in a week, against owners who pat
  today (`pet_companions.last_petted`, `pet_treats.given_on`,
  `late_ssh_pet_actions_total`).
- Days with a win per owner per week, before and after treats
  (`pet_treats.earned_on`), and which families the day's first win comes
  from (`pet_treats.kind`): the test of "a push to play anything".
- Visitor pats and calls per day (`pet_visits`), and the share of
  visitors outside the 20 busiest chat authors.
- Two-day return of the cohorts that started with an egg against the
  cohorts before it. The `Board` tour's cohort reads share those weeks,
  so one cannot be told from the other where both landed together.
- Which animal is picked at the hatch, and how many change it after
  (`late_ssh_pet_actions_total{action="hatch"|"grow"|"species"|"reset"}`).

## Tensions

- **The deadchannel runner** is the prestige character with its own
  look. The pet is everyone's creature and stays off the chat name line.
- **Reach.** Every account has one, so the read is who touches it:
  `SELECT count(*) FILTER (WHERE enabled), count(*) FILTER (WHERE
  last_petted >= current_date - 7) FROM pet_companions`.
