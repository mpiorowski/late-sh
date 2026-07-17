# Super Snake levels

Each `level_NN.txt` is one arena, ported from the original 1990s Turbo Pascal
Super Snake `.LEV` files. Files are embedded at compile time; add a new file
and register it in `late-ssh/src/app/rooms/ssnake/levels.rs` to ship a new
arena.

## Format

A header of `key: value` lines, one blank line, then the arena matrix
(one character per cell, all rows the same width):

```
name: Warp Fields        arena name shown in the room UI
lives: 4                 lives each player starts with (per-level difficulty)
points-needed: 11        shared food count; the match ends when it hits zero
lives-bonus: 0           extra lives for whoever eats the final point
points-bonus: 200        score bonus for whoever eats the final point
tick-millis: 125         base game tick (room pace setting scales this)
initial-length: 20       starting snake length (pending growth)
growth-factor: 7         scales random growth per point eaten (original: level/3 + 3)

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
