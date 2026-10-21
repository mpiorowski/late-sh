# A Dark Room: what the port contains

The scope of the port against upstream, the two endings, and the ravaged
battleship. The door's shape, the pacing model, the clock, persistence and
the gotchas are in `CONTEXT.md`.

## Scope: the whole classic game

**In:** the fire and the builder arc, the forest, traps, huts and population,
the worker/income economy, the trading post's buy menu, the workshop crafting
tier, the random event pool (village events, the thief, the delayed
Mysterious Wanderer payoffs), the compass and the path, the wasteland with its
generated map, supplies, danger and encounters, every classic setpiece, the
mines feeding the village their ore, the ravaged battleship and the fabricator
it hands over, the ship, the ascent, and both endings.

**Deliberately cut:** upstream's `prestige.js`/`scoring.js` (a per-resource
point total that resets does not fit a save that only ever grows), the `cache`
landmark, and upstream's marketing event. Nothing else.

**The three clocks.** Village time is credited, capped and slowed (`pace`).
Wall-clock cooldowns (stoke, gather, traps, embark, liftoff, delayed rewards)
are neither. Live play (world moves, fights, the ascent) runs on the raw delta
between ticks: expeditions cost no daily allowance and are never slowed,
because a trip is paid for in supplies per move, not in time.

**Expeditions park.** The in-flight trip lives in `Game::expedition` and is
saved on every move, so a dropped SSH connection or an Esc leaves the wanderer
standing where they were rather than losing the trip. A fight in progress
parks with it (`Expedition::combat`) and resumes on return, whichever way the
session ended: leaving the door must never be a way to flee one. Upstream
loses everything when the tab closes; that is a deliberate softening, and it
costs nothing because supplies burn per move. Death still discards the trip
and the pack.

**In the village, almost nothing decays.** Stores, buildings, population and
unlocked trades mostly only go up. The exceptions all arrive with the
wasteland: the event pool can burn a hut, take villagers, wreck traps, and the
thieves skim stores once the village is rich enough to rob.
Absence costs progress, never possessions. That bites early (the builder arc
stalls whenever the room is below Warm, so a dead fire freezes her; and she
refuses to build at all while the room is Cold or worse), and mostly stops
biting once she is Helping, because she stokes the fire herself and out-earns
what she burns. Past that point the brakes are the daily cap and the supply
lines an expedition needs; the village events only ever fire while somebody is
in the door, so an absent player is never robbed of anything but time.

**Deliberately dropped:** `dropbox.js` (cloud saves), `audio.js` /
`audioLibrary.js`, `notifications.js` and `Button.js` (DOM widgets), roughly
1,100 lines that the terminal replaces rather than translates.

## The two endings: the only things here that end

Winning the ascent (`Flight::Won`) is the only terminal state the door has,
and it is deliberately loud about it. Which of the two endings runs is decided
by one thing: whether the store room holds the `fleet beacon` taken off the
immortal wanderer (`state::Escape`, `Plain` or `WithBeacon`).

- **The epitaph takes the whole panel.** `state::Ending` holds the beats (the
  closing lines from `space::ENDING` or `space::BEACON_ENDING`, the run's last
  figures, the badge, the way out) and a reveal clock; `ui::draw_ending` lays
  out every beat from the first frame and leaves the unrevealed ones blank, so
  the text never crawls up the screen as it arrives. Any key skips the wait;
  the next key leaves the door. There is nothing behind the ending to go back
  to, so that is the screen's only exit (`screen::ending_took_key`, which owns
  Esc and the arrows too).
- **The account keeps the run: chips every time, a badge the first time.**
  `svc::reward_escape` is the Green Dragon `reward_dragon_kill` shape: a
  `#lounge` feed line every time (the beacon run's says so:
  `Escape::feed_detail` appends "followed the fleet beacon home"), the chip
  payout, and — first escape *of that kind* only, on the `NOT EXISTS` award
  insert — a rankless profile badge. Plain: `darkroom_escape`, migration 143,
  `ADE`, 15,000. Beacon: `darkroom_beacon_escape`, migration 145, `ADB`,
  20,000. Separate claims, so an account can earn both.
  **The chips repeat** (migration 158): the ending wipes the
  save, so a second escape is the whole arc walked again and it pays again.
  The gate is `Game.run_id`, a uuidv7 stamped on a fresh game and carried in
  the save blob (a blob written before migration 158 deserializes with one of its own rather
  than a nil id shared by every old save), passed to `reward_escape` and used
  as the `per_event` key, so a retried grant task pays once and a new run is a
  new event. `Escape::reward_line` is the one place the amounts are written
  out for the UI. Badge codes are registered in
  `late-core/src/models/profile_award.rs` and `user.rs`'s chat-label SQL; chat
  shows only the higher of the two (`BADGE_LADDERS`), the profile page still
  lists both. See `app/leaderboard/AWARDS.md`.
- **The account also remembers that it happened.** The same task writes the
  `darkroom_veterans` row, first and on its own, because it is the one thing
  here that changes what the game *is* next time and must not be lost to a
  failure in the payout path. It lives outside the save blob precisely so the
  wipe below cannot erase it, and the only thing it buys is the battleship on
  the next run's map. Which endings an account has reached is not recorded
  there: the `ADE`/`ADB` badges already do that, permanently.
- **The save does not survive.** The win deletes `darkroom_saves` for the user,
  so the next visit is a dead fire in a dark room and the whole arc is there to
  walk again. That makes `State::save` and `save_on_leave` load-bearing: both
  return early while `ending` is up, or stepping out of the door would write
  the finished run straight back over the wipe. `tick` returns early for the
  same reason (it only advances the reveal): settling village time into a game
  nobody will ever save again is pure waste.
- **A dropped connection during the ending loses nothing.** The wipe and the
  grant both fire the moment the ship gets through, not on dismissal. The
  player misses the words, never the reward.

## The ravaged battleship: the one landmark you go back to

The 10th-anniversary content, and the only part of the game gated on anything
outside the save.

- **It is not on a first run's map.** `world::generate` takes a `veteran` flag
  and drops `Tile::Battleship` only when it is set; `Game::veteran` comes from
  `darkroom_veterans` and means "this account has flown out before, either
  way".
  A save whose map was drawn before the account earned the unlock gets the
  wreck retro-fitted on load (`world::place_battleship`), so nobody has to
  throw a run away to see it. That is the whole of what finishing the game
  buys: one landmark. It never touches stores, perks, or any pacing constant.
- **Every other landmark is played once; this one is a place.** The square is
  never marked visited. The first arrival runs `executioner-intro`, which ends
  by power-cycling the ship, fighting the turret that wakes up, and taking the
  strange device (`Effect::EnterBattleship`); every arrival after that runs
  `executioner-antechamber`, whose elevator buttons fall away as their decks
  are picked clean. The square only stops offering when the command deck falls
  and `Effect::ClearDungeon` turns the wreck into an outpost.
- **Progress is trip-scoped until you get home.** `Expedition::battleship` is
  the working copy, started from `Game::battleship` on embark and committed by
  `world::go_home`, exactly like the map. Dying in the wreck loses the decks
  cleared that trip along with the pack. That is upstream's `World.state`.
- **The fabricator is what the wreck is really for.** Getting inside once
  brings the strange device home, which opens `View::Fabricator` (its own
  panel, slotted in just before the ship's). It builds three things off alien
  alloy alone; the other six need a blueprint, and blueprints are loot the
  three decks drop, redeemed out of the pack by `Game::redeem_blueprints` on a
  safe return and never shelved as stores.
- **The fights need a status layer the rest of the game has no use for.**
  `event::Status` is six mutually exclusive conditions (upstream keeps one
  free-form string per fighter, so they cannot stack): `Shield` turns the next
  hit into healing and breaks, `Enraged` swings on a half-second clock,
  `Meditation` banks everything thrown at it and gives the pile back in one
  swing, `Venomous` leaves a bleed, `Energised` quadruples damage, and `Boost`
  (the wanderer's, from a stim) halves attack cooldowns. They arrive from
  `Combat::specials` (timers), `Combat::at_health` (a threshold crossed
  downwards, once) and the three fight rows the wreck adds. The immortal wanderer rotates
  three of them at random, never the same one twice running.
- **One enemy does not fall over.** `Combat::explosion` puts the fight into
  `Phase::Exploding` instead of `Spoils`: the medical deck's unstable automaton
  takes thirty off the wanderer three seconds after it dies. That phase is
  deliberately unparkable and unleavable, so the blast cannot be dodged by
  stepping out of the door.
