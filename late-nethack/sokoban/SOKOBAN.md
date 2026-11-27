# Sokoban autoplayer

A local-only tool that solves a NetHack 5.0 Sokoban level and types the
solution into the player's late.sh window. It runs on the player's desktop
and is never deployed: the crate is its own workspace, so the root
workspace, CI and prod builds never compile it, and `.dockerignore` keeps
it out of every image.

This file is the runbook. An agent pointed at it should be able to go from
"I'm on a Sokoban level" to keys flowing within a minute.

## Requirements

- Hyprland with the Lua config: keys go in through
  `hl.dsp.send_shortcut({ mods = "", key = "h", window = "address:0x..." })`.
  The old comma dispatcher syntax doesn't work there.
- `grim` with `-T` (capture one toplevel, even on a hidden special
  workspace). `hyprctl clients -j` gives its `stableId`.
- late.sh running in a Ghostty window titled `late.sh`, the game on screen
  with no menu open. A block cursor: NetHack parks the cursor on the hero
  after every move, and that is how the tool sees where the hero is.
- Cell size `13x28` pixels (Ghostty at 2560x1600). On another font or
  screen, measure it: boulders in one row sit one cell apart.

Build once: `cargo build --release --manifest-path late-nethack/sokoban/Cargo.toml`.
The binary is `late-nethack/sokoban/target/release/sokoban` (`$SOKO` below).

## Runbook

1. **Look at the screen.** Find the window and capture it:
   `hyprctl clients -j` (the entry with title `late.sh`: `stableId`), then
   `grim -T <stableId> shot.png` and crop the map. Window ids change when
   Ghostty restarts. Never send keys before the user says go.
2. **Identify the level.** Sokoban goes up: the first floor (entered from
   the Oracle side) is `soko4-*`, then `soko3-*`, `soko2-*`, and the top
   (the zoo and prize) is `soko1-*`. Each floor is one of two variants, and
   5.0 mirrors levels at random: left-right, top-bottom or both. Print the
   candidates until one matches the screen cell for cell:
   `$SOKO board levels/soko2-2.txt --flip hv` (`--flip h|v|hv`, or none).
3. **Write the board** to `tmp/sokoban-board.txt` (repo `tmp/`, ignored) in
   screen orientation, and fix it to match the screen:
   - the hero: `@`. You arrive on the down stairs `>`, which the stock
     maps hide under the `@`: put the `>` back once you step off.
   - boulders `0`, open holes `^`, a filled hole is plain floor `.`.
   - an item drawn on a hole (a `/` in the hole line) still sits on an open
     hole: keep `^`. The rolling boulder trap shows as `^` on screen: keep
     `R` where the stock map has it.
   - monsters, pets and items elsewhere are floor `.`.
   - a boulder the stock map doesn't have, on the top floor, is a giant
     mimic: `soko1-1` and `soko1-2` each hide two as boulders. Mark it
     `M`: the solver plans as if it were dead, and `play` types every key
     up to the first one that would walk into it or push a boulder onto
     it, then stops so the user can kill it. Make it `.` and play on. An
     unseen one shows up when the hero bumps into it, and `play` stops
     there too.
   - doors `+` are walls for the solver.
   Mid-level (after a fight, or a resume), copy the board from the screen
   the same way; the file `play` leaves behind is the best starting point.
4. **Solve** (prints the batches, nothing is typed):
   `$SOKO solve tmp/sokoban-board.txt`. Tell the user the key count.
5. **Check the cursor** (types nothing):
   `$SOKO locate --window late.sh --cell 13x28` must print the hero's
   pixel, not some other spot.
6. **Play, in the foreground**, after the user's go:
   `$SOKO play tmp/sokoban-board.txt --window late.sh --cell 13x28`.
   Run it as a plain foreground command with a long timeout. Never in the
   background, never in a subagent: the user watches and interrupts (Esc)
   the moment a monster shows up, and only a foreground run stops at once.
   Pacing is 5 keys, a 1 second pause, then a check that the cursor is
   where the plan put the hero. It stops on any mismatch: a monster in
   the way, a `--More--` or other prompt, a boulder that didn't move.
7. **After a stop or an interrupt**, the board file holds the last position
   verified on screen (written after every good chunk, so at most 5 keys
   behind). Capture the screen again, let the user fight whatever is
   there, fix the board's differences (hero, a boulder or two, filled
   holes) and go back to step 4. Each re-solve is fresh from the board, so
   nothing else needs resyncing.

## How it works

- `src/level.rs`: the map legend, parsing, and the 5.0 push rules, checked
  against the 5.0.0 source: a boulder dropped on a hole or pit fills it.
  A boulder pushed onto the Sokoban rolling boulder trap rolls on the same
  way into the first open hole, or stops at a wall. A boulder in its path
  takes over the roll. The trap has no launch spot and no confirmation
  prompt, so the hero may walk over it. Boulders cross and rest on stairs
  (seen in game: you arrive on the down stairs, which can sit in the
  boulders' path).
  You can't push while levitating. Autodig with a wielded mattock only
  turns a move into a wall into "too hard to dig", harmless here, and the
  plan never walks into walls.
- `src/solve.rs`: fills holes one at a time. Each fill is a best-first
  search over pushes (walking is free), guided by each boulder's push
  distance to the hole line plus the boulders in the hero's way. Ties go
  deeper first. It keeps the 3 cheapest ways to fill the next hole and
  backtracks into the next one when a branch dead-ends.
- `src/keys.rs`: pushes to keys: the walk to each boulder, then the push.
- `src/screen.rs`: finds the cursor in a screenshot as the only solid
  cell-sized rectangle: top and bottom rows lit edge to edge, dark columns
  either side. Glyphs and emoji fail the first test. Highlighted bars (the
  curses name field, menu headings) fail the second, even when whiter
  than the cursor. One sighting at start calibrates the map grid.
- `src/play.rs`: the Hyprland side: `hyprctl` keys, `grim -t ppm`
  screenshots, the chunked loop.
- `levels/`: the 8 starting maps, built by `extract.py` from the game's
  `nhdat` (`kubectl exec deploy/late-nethack -c late-nethack -- cat
  /var/games/nethack/nhdat`). Rerun it after a NetHack version bump. The
  maps are NetHack data, under the NetHack General Public License like
  the game.

## Solver reach

Release build, from each level's start:

| Map | Time | Keys |
|---|---|---|
| soko4-1 | 0.1s | 333 |
| soko4-2 | 3.7s | 381 |
| soko3-1 | 1.0s | 461 |
| soko3-2 | not solved in 120s | |
| soko2-1 | 0.3s | 485 |
| soko2-2 | 0.1s | 486 |
| soko1-1 | not solved in 120s | |
| soko1-2 | 2.4s | 778 |

soko3-2 and soko1-1 are beyond the solver from the start. Their first
fills need several boulders rearranged before any of them can reach the
hole line, and the push-distance guide doesn't see that. For those levels
the user plays the opening by hand (a published solution, mirrored to the
screen), and the solver takes over once a few holes are filled. Expect a
mid-level board to solve much faster than the start, but it is not
guaranteed. Mid-level boards of the other six solve in well under a
second.
