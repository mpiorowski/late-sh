# Pet Context

## Metadata
- Scope: `late-ssh/src/app/pet`, its rows in `late-core/src/models/pet.rs`
- Purpose: the Pet Companion: a creature that grows up over the days its
  owner shows up, goes by what it ate today, and needs nothing.
- Status: live for every account. Nothing about it is sold: the
  `pet_companion` Shop item is retired and the Shop knows no pet.
- Parent context: `../../../../CONTEXT.md`. The per-user state shape it
  follows is worked in `../bonsai/CONTEXT.md`.
- What is designed and not built: `devdocs/PET-GROWTH.md`.

---

## 1. The rules, in one place

Nothing about the pet ever goes down. No hunger, no decay, no streak to
break: an owner who stays away finds it asleep and glad they are back.

- **Every account has one.** `PetCompanion::ensure` makes the row at the
  first sign in: an egg, out. The migration that added growth reset the
  pets that existed to eggs too (name and last pat kept).
- **Days together.** One per UTC day on which the owner pressed a key
  with the pet out. The count only rises. It is not calendar age.
- **Stages**: `egg`, `baby`, `young`, `adult`, `elder`, needing 2, 7, 30
  and 100 days together (the day it arrives is day 1, so the egg is ready
  the day after).
- **The hatch is the owner's pick.** Days never hatch an egg. A click on
  a ready egg opens the care box, which shows the three babies; `1` cat,
  `2` dog, `3` bird hatches it as that animal. Until then the row's
  species means nothing and an unnamed egg goes by "egg" everywhere.
- **Every later stage is the owner's pick too.** The days make a pet
  ready and enter no stage. A click on a pet with its days opens the care
  box, which shows it drawn two ways; `1` or `2` grows it into that one.
  Each stage adds one part: the top at young (the cat's ears pointed or
  straight, the dog's floppy or upright, the bird's crest a tuft or
  horns), the feet at adult (neat or big paws, tucked or out, small feet
  or long legs), the face at elder (plain or whiskered, plain or
  freckled, the bird's wings down or up). The picks stay, so three of
  them make eight elders a species. `( )` and `{ }` mark the adult and the
  elder and are never a pick. A pet with the days for two stages picks
  twice in a row.
- **A stage and its picks are one value** (`Form`, from the `stage` and
  `looks` columns): a young pet without its top cannot be stored or
  drawn.
- **One treat a day.** The first win of a UTC day in any of the house's
  games earns it, and so do ten active minutes in one door screen. Its
  kind is where it came from: `puzzle` (a daily puzzle), `arcade` (a
  score game), `match` (a daily match), `table` (a card or casino table),
  `door`. At most one waits in the pouch, and while one waits no other is
  earned; an egg can earn but not eat. Lateania's mob kills are not wins:
  its minutes earn a door treat.
- **The treat shows today.** The last treat eaten (`Fed`), if eaten this
  UTC day, is a title after the name (the bookish, the quick, the
  cunning, the lucky, the brave) and a glyph carried beside the feet
  (`# * & $ !`), and it picks the line a call brings back. Both are gone
  at the day's end. A baby shows the title only (its tail has that cell);
  an egg shows neither. It is looks, never power.
- **The bond.** The first treat given or toy fetched of a UTC day is a
  bond day. A count, shown in the care box; nothing reads it yet.
- **Tricks**: `spin`, `bow`, `hop`, `dance`. Three separate UTC days of
  practice each, one practice a day, and switching trick starts over.
- **Visits.** Anyone can pet someone else's pet from its owner's profile
  once per UTC day, and send their own hatched pet to call once per UTC
  day (one call a day, whoever it is on). A call brings back one line
  from a closed set picked by what the two pets ate today.
- **A call is a stay.** The caller's pet is its host's guest for
  `STAY_MINUTES` (60) from the call: drawn in the host's box, care box
  and profile as it looks now, named `owner's Name`. It never leaves its
  own box: its owner's tile says `visiting <host>, N min left`. A guest
  put away or reset to an egg is no guest. The stay is the `pet_visits`
  row's `created`; nothing is written when it ends.
- **Toys.** A ball and a toy mouse, both free.
- **The daily pat.** The first click of a UTC day pays 100 chips
  (`PET_CHIP_BONUS`), as it always did.
- **Slash commands** (`input::Order`, typed in chat): `/pet cat|dog|bird`
  changes a hatched pet's animal and nothing else (its picks carry over
  as that animal's drawings); `/pet away` and
  `/pet out`; `/pet reset` says what a reset takes and `/pet reset yes`
  does it (back to an egg on its first day: days, stage, picks, bond,
  tricks go;
  the name, the day's pat and a waiting treat stay). `/petname` names it.
- **Put away** (`/pet away`): the row keeps everything and takes
  nothing. No day is counted, no treat earned, no pat paid, no visitor
  let in, and every order but `/pet out` is refused.

---

## 2. File map

```text
growth.rs    the rules, pure: days, stages and their picks, the bond,
             tricks, the reset,
             GameFamily -> TreatKind, today's title, which screens are
             doors, call lines
svc.rs       the one writer: lock the row, apply one rule, store, notify
state.rs     the session's mirror, the mood reading, moments, the toy,
             the walk after the toy or the cursor
input.rs     clicks on the tile, the care box's keys, the `/pet` orders
ui.rs        the bodies drawn from picked parts, the tile's rows, the
             profile portrait
modal_ui.rs  the care box
```

Rows: `pet_companions` (one per user, the growth columns, `looks` (the
picks, in order, checked against `stage`) and `revision`; `adopted_at` is a leftover nothing reads),
`pet_treats` (the primary key `(user_id, earned_on)` is the day's claim,
a partial unique index keeps one waiting), `pet_visits`
(the primary key `(owner_id, visitor_id, day, kind)` is the claim, a
partial unique index keeps one call a day per visitor). Every data
operation is in `late-core/src/models/pet.rs`.

---

## 3. Architecture

The bonsai's shape: a pure rule over a locked row, one writer, a session
mirror that never decides.

- `PetService::act_task(user_id, PetCommand, reply)` is the orchestration
  half: a span, the work half `act`, the metrics, the activity event for
  the paid pat, and a `PetOutcome` on the session's channel. `act` ensures
  and locks the row (`SELECT ... FOR UPDATE`), refuses a pet put away,
  runs one `Growth` rule or one claim, stores, `pg_notify`s `pet_changed`
  in the same transaction, and returns the whole `PetSnapshot` (row,
  pouch, the last treat eaten, unseen visitors, the guests staying and
  where this pet is staying itself).
- **The row decides, never the session.** Every claim (the day, a treat,
  a bond day, the pat's chips, a visit) is a write on the locked row or an
  `INSERT ... ON CONFLICT DO NOTHING` in that transaction, so two
  sessions or two replicas racing settle to one.
- **Ordering.** Every write a mirror shows bumps `pet_companions.revision`
  (`store`, `touch`, `mark_visits_seen`, `set_species`). A reload and an
  answer race each other; `PetState::install` drops a snapshot older than
  the one it holds.
- **Fan-out.** `pet_changed` carries the owner's id through
  `pg_listener` (`Channel::PetChanged`) into a process-local broadcast;
  a session whose user it names reloads. That is how a second session
  and an owner on another replica hear of a visit. A call tells the
  caller's sessions too, so their tiles learn where the pet went.
- **A stay ends on the clock.** A mirror holds the guests and its own
  stay with the time each began, and the tick drops them when
  `growth::stay_left` says the hour is up (the UTC clock is `PetTick`'s
  day and day age). No write, no notice, no read.
- **The species.** `PetCommand::Hatch(species)` and
  `PetCommand::Species(species)` write it under the same lock
  (`PetCompanion::set_species`, which bumps `revision`); the second is
  refused on an egg (`Refusal::Egg`). `PetCommand::Grow(pick)` stores
  `Growth::grow`, refused without the days (`Refusal::NotReady`).
  `PetCommand::Reset` stores `Growth::reset()`.
- `PetService::visit_task(visitor, owner, kind, reply)` is the surface
  other domains call (the profile modal, through `PetState::visit`). The
  host's row is locked first, the caller's own row is only read.
  `PetCompanion::guests_since` is the read a profile makes for the pets
  staying with the one it shows.
- The session asks sparingly: the day once per session and UTC day (again
  after a failure, or after the pet is brought back out), a treat once
  a day and not while one waits, and only one owner command
  is out at a time (`busy`), so a held key queues nothing.
- **The mood is still read, not stored state**: `mood_at` from the
  session's signals (a win, a loss, a message, music, a pat, quiet). The
  row keeps the last one for the profile.

Glue outside this folder, each a delegation: `app/tick.rs` hands the tick
its facts (`PetTick`: the last key, whether the pet is out, the UTC day)
and tells the pet of wins (`note_win(now, family)`) and of active seconds
on a screen (`note_active_seconds`); `app/input.rs` routes clicks to
`input::handle_click`/`handle_right_click` and the open care box to
`input::handle_modal_input`; `app/render.rs` draws `modal_ui::draw` from the pet's
`PetView` and hands the Arcade `portrait()` while the pet is out;
`App::sync_presence` hands the same to the Clubhouse's `set_pet` for the
record it publishes; `chat` parses `/pet` with `input::parse_order` and
hands the order to `input::handle_order`.

---

## 4. Surfaces

- **The Home tile** (`ui::draw_tile`, a `zen` tile): the box the pet
  roams, a status row above it and a pouch row under it when the tile is
  tall enough (five rows for the first, six for both). The stock Home
  gives it three rows: the box alone.
- **Guests in the box** (`ui::draw_pet_box`, so the tile and the care
  box's pen alike): the newest two stand in a strip on the right,
  fourteen cells each, never more than half the box's width. The pet's
  zone is the rest, so its walk, its toys and its clicks never reach
  them. A box with a row to spare names each guest on its bottom row and
  everyone stands on the row over it; a box of three rows draws the
  bodies only. The status row counts them when there is more than one.
  A guest stands still, idle, and blinks: the tick reports those edges
  (`ui::guest_moved`) while a box is on screen.
- **Clicks**, in order, on the pet: one with its days (a ready egg, or
  a hatched pet ready to grow) opens the care box for the pick; the
  day's pat; give the waiting treat; otherwise purr
  and perform the next known trick. On the pouch's
  treat: give it. On a row: the care box. On the floor: throw a
  ball. A right click anywhere on the tile: the care box.
- **The care box** (`show_pet_modal`): a pen with the same box, then
  rows for the treat, growing (a picker while a pick waits: the three
  babies for a ready egg, the pet drawn two ways for one ready to grow),
  play, tricks, who came by, who is staying now (`here now`, all of
  them by name), and for a hatched pet the slash commands that change
  it. Keys: `f` give, `1` `2` `3` hatch a ready egg as a cat,
  a dog or a bird, `1` `2` grow into the first or second drawing, `b`
  ball, `m` toy mouse, `t` next trick, `p` practise,
  `r` perform, `?` guide, `q`/Esc close.
  Opening it marks the visitors seen. Also opened by Enter on the pet
  tile in arrange mode (`zen`'s `TileKey::Care`).
- **The profile** (`profile_modal`, through `ui::Portrait` and
  `portrait_lines`): the body in its form, the title, `stage · day N`,
  the mood, and a known trick every twenty seconds. A visitor gets `p`
  (pet it) and `v` (send yours). Today's treat rides `ProfilePet.fed_today`.
  The pets staying with it (`ProfilePet.guests`, as of the load) stand
  beside it (`ui::portrait_with_guests`) over a `with owner's Name` row.
- **The Arcade corner** (`arcade/ui.rs`, `draw_beside_pet`): while a
  game is open and the pet is out and hatched, its portrait sits in the
  top right corner of the game's area when the board leaves that corner
  spare, on the wall tick. No room or an egg: not drawn. Decoration only.
- **The tavern heel** (`clubhouse`): a hatched pet that is out walks at
  its owner's heel in the Clubhouse, one letter (`c` cat, `d` dog, `b`
  bird) on free floor beside them, seen by every session on every
  replica. It rides the owner's presence record
  (`ClubhouseStand.pet_at_heel`, its species), never a local registry.
- Both read `PetState::portrait()`: species, form, mood, today's treat
  as `carries`, the tricks it knows. The glue
  builds it only while `App::pet_out`; each surface leaves an egg out.
- **Moments** are session-only and last seconds: eating, the welcome
  back (longer the more days away), a trick, the hatch.

---

## 5. Invariants and limits

- A new stage adds a variant to `Form` and an arm to `form_rows` and
  `days_for`; a new animal adds an arm to each part (`top_row`,
  `feet_row`, `elder_face`, `pick_name`, `mouth`): all exhaustive, no
  catch-all.
- The bird's beak is the middle cell of its eyes (`face_cells`), so every
  eye string is three cells with the nose in the middle.
- Every body is seven cells plus one column (the tail, and on the floor
  row the carried glyph): eight, the box's `PET_WIDTH`.
- The egg and a sleeping pet are fully static; `ui::frame_changed`
  compares the same `PetArt` the draw uses, so a frame is only paid when
  the box changes.
- A match win earns the day's treat only in a connected session: a daily
  match that ends while its owner is away earns nothing.
- The name and the put-away switch reach the owner's other sessions at
  their next connect (they are not on `pet_changed`); the species is.
- The put-away switch is written from a task of its own: `/pet away` and
  `/pet out` typed back to back are two writes that can land in either
  order.

---

## 6. Telemetry

`late-ssh/src/metrics.rs`, labels from closed enums:
`late_ssh_pet_actions_total{action,result}` (every command; `result` is
`stored`, `refused` or `failed`), `late_ssh_pet_days_total`,
`late_ssh_pet_stage_ups_total{stage}`,
`late_ssh_pet_treats_total{kind,beat}`,
`late_ssh_pet_visits_total{beat}`. Failures are logged once, in
`act_task` and `visit_task`.

---

## 7. Tests

- `growth_test.rs`: the rules, whole-state from egg to elder by days and
  picks, the reset, today's title, the hour of a stay.
- `svc_test.rs`: every claim against a real database, two sessions
  racing, a pet put away, the hatch pick, the grow pick, a change of
  species, a reset, visits and calls, the guest a call leaves.
- `state_test.rs`: the mood reading, the walk, the first key of a day,
  the click chain, the door clock, snapshot ordering, the portrait, a
  stay from the call to the end of its hour.
- `ui_test.rs`: both drawings of every part of every species, whole;
  the eight elders a species; the eight columns; what the care box
  offers; guests in the box and beside a portrait; poses and the frame
  gate.
- `input_test.rs`: the care box through a rendered app, the hatch pick,
  the grow pick, a guest in the care box and on the tile, the `/pet`
  words and what each order does.
- `late-core/src/models/pet_test.rs`: the rows and their claims.
