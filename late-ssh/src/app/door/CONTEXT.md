# Doors: the Games hub and the door family

`late-ssh/src/app/door` holds every immersive game reached from the Games hub
(page `3`), the hub itself, and the pieces the doors share. Each door has its
own `CONTEXT.md`; this file is the map of the family. Research on candidate
doors and licensing is in `devdocs/DOOR-CANDIDATES.md`.

## 1. The hub (`hub/`)

A grouped rail of game cards with the selected card's full landing rendered
beside it (a selector, not a scroll). `HubGame` is the closed roster and
`HubGroup` its display groups, in order:

- **the house:** Night City (runners only: `HubGame::roster(App::is_runner())`;
  Enter takes the same descent as `0` on the Clubhouse,
  `app/deadchannel/CONTEXT.md`), Lateania, the Realm.
- **roguelikes:** DCSS, NetHack, Brogue.
- **remakes:** A Dark Room, Zork Trilogy.
- **servers:** Minecraft, Vintage Story (info-only cards, Enter does nothing).
- **doors:** Green Dragon, Usurper, dopewars, BashQuest, Rebels, CodeKeep.

Under the cards the rail pins a `live` section: one row per game someone is
playing right now on a watchable door (DCSS, NetHack, Brogue, Zork). Sitting on
a live row previews it in the landing's place; Enter opens the watch with its
chat (`spectate/CONTEXT.md`).

The hub's keys live in `hub/input.rs` (`handle_event`, the one arm the
root dispatcher keeps for the Games screen; `open_rc_modal` for the door
screens' `c`). Every launch is counted there. `launch_selection` runs one
gate (`unavailable`: the banner a switched-off host shows instead of
launching, every host's flag in that one match, Zork's included) and then,
before the card's own way in, writes a `door_openings` row through
`hub/svc.rs` (`GamesHubService::record_task`, fire-and-forget, its failure
logged as `door_opening_failed` and counted in
`late_ssh_door_openings_total{card,result}`). A row is one Enter that got
past the gate: a host that then refuses still had its door opened, a
member who quits and comes back opens it again, and the two server cards
write nothing. The Realm between seasons is the one banner that still
counts, a sweeper tick at most. `HubGame::card` maps the roster onto the
stored key (`late_core::models::door_opening::DoorCard`, a closed roster of
its own; the roguelikes' keys are `leaderboard::DoorGame`'s). The paper's
YESTERDAY column reads the day's openings of every card through
`leaderboard::day_tallies` (`DayGame::Door`, `app/paper/CONTEXT.md`), the
runs won for the three roguelikes beside them.

Door screens are never top-level tabs: they are entered only from the hub
(the Realm also from the #lounge live strip) and return to it on exit (`Esc`
in Lateania; quitting the game in the PTY doors, e.g. `Ctrl-C`, `Q` at
Usurper's menus, `S` save in the roguelikes).

| Key | Action |
|-----|--------|
| arrows, `h`/`j`/`k`/`l` | Step along the rail (cards, then live rows), wrapping |
| `Enter` | Launch the card, written down as a door opened (below); on a live row, open the watch |
| `Ctrl+J`/`Ctrl+K`, `Ctrl+Down`/`Ctrl+Up` | Scroll the selected landing (`Enter` matches CR only, since `Ctrl+J` is a bare LF) |
| `d` | Reset the Green Dragon or A Dark Room save, after confirmation (Lateania resets from its own landing) |
| `t` | On a watchable door's card: show or hide your watchers' chat beside your own game |
| `c` | On NetHack or DCSS: the per-account config paste box (`.nethackrc` / `init.txt`) |
| `Esc` | From a live row back to the cards |

## 2. Three ways a door plugs in

1. **Native Rust, in process.** Lateania, Green Dragon, A Dark Room, the Realm.
   Saves are our own rows; `game.rs` (`DoorGameId`, `DoorGameEvent`) is the
   shared event shape for the three native single-player doors.
2. **Upstream binary on a PTY in its own host crate**, reached over an internal
   SSH hop (russh client in late-ssh, `late-<door>` host with its own
   pod): NetHack, DCSS, Brogue, Usurper, dopewars, BashQuest, CodeKeep, Zork.
   Each host owns auth with a shared secret, identity, save storage and
   teardown; the client side owns input forwarding and the vt100 render.
3. **Remote SSH door proxy:** Rebels in the Sky (`rebels/`, a third-party
   server).

Any native port must be license-compatible with the repo's FSL; GPL sources
can only run out of process (pattern 2). Each door's context records its
license boundary.

## 3. Shared pieces

- `keys.rs`: key translation for every vt100-backed door, including DECCKM
  application cursor mode, which the vt100 parser swallows and ncurses games
  need for their arrow keys.
- `landing.rs`: shared line builders for landing pages, so their styling
  cannot drift.
- `arcade.rs`: the arcade handle, one immutable public name per account, used
  by doors whose upstream keys saves and score files by player name.
- `rc.rs`: per-account rc files (`.nethackrc`, DCSS `init.txt`). The DB row is
  the truth; every launch pushes it (empty included, which is what deletes a
  cleared file) to the host as one SSH env request.
- `ingest/`: the door log pipe that turns host log lines into leaderboard
  rows and milestone payouts. The contract (transport, cursors, idempotency,
  award sink) lives in `ingest/CONTEXT.md`.
- `spectate/`: watching someone else's live game.
- Session state for the doors still sits in `app/state.rs`; moving it into
  this domain is listed in `SCALE.md`, Small Debts, "Root-file thinning".
