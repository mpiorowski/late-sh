# Super Snake levels

Each `level_NN.txt` is one arena, ported from the original 1990s Turbo Pascal
Super Snake `.LEV` files. Files are embedded at compile time; add a new file
and register it in `late-ssh/src/app/lobby/house/ssnake/levels.rs` to ship a
new arena.

The arena is perpetual: clearing one of these reshuffles the table to another
random file, so a level is a lap, not a match. The original `lives`,
`lives-bonus` and `points-bonus` keys are gone — there are no lives left, and
chip payouts come from the constants in `ssnake/settings.rs` rather than from
each level.

## Format

A header of `key: value` lines, one blank line, then the arena matrix
(one character per cell, all rows the same width):

```
name: Warp Fields        arena name shown in the room UI
points-needed: 11        food on this arena; the last one clears it
tick-millis: 125         base game tick (table pace setting scales this)
initial-length: 20       starting snake length (pending growth)
growth-factor: 7         scales random growth per food eaten (original: level/3 + 3)

#####~#####
#.........#
~.........~
#####~#####
```

Matrix characters:

- `#` wall (deadly)
- `.` empty floor
- `~` warp tunnel: an open gap in the border; snakes wrap to the far side

Max arena size is 63x36 cells. The TUI renders two matrix rows per terminal
row using half blocks, so a 36-row arena needs 18 terminal rows plus chrome.

## Rules a new arena must obey

`levels_test.rs` enforces these; a file that breaks one fails the build.

- **One connected floor region, checked on a torus.** Food spawns on any
  non-wall cell and the arena only clears when the last food is eaten, so a
  sealed pocket would wedge the table forever — there is no timeout to save
  it. Movement wraps, so cells on opposite edges count as neighbours.
- **Room for a full table**: at least `5 x initial-length + 40` floor cells.
- Warp gaps should be cut in matched pairs (a `~` on the top row wants one at
  the same column on the bottom row), or the tunnel walks you into a wall.
- Avoid one-cell dents in a wall. They are reachable, so the tests pass, but
  food can land in one and a long snake that enters cannot turn around. Cut a
  tunnel straight through a block rather than notching its side.

Drawn shapes need care here: any stroke that tapers to a point ends in a cell
with one way out. `level_51`/`level_53` spell SNAKE in walls, and the letter
counters were filled in wherever they came to a point — which is why the N
reads as a solid slab rather than a clean diagonal. Legibility loses to not
stranding food.

`level_52` has no walls at all (every border cell is a tunnel), so the only
thing that can kill you there is another snake or your own tail.
