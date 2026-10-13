# Vendored Frotz

Upstream: https://gitlab.com/DavidGriffith/frotz

Base: `042d7bcadc1e2a8090cf737c841d8b7fc14b6eff` (2.56pre).
Only tracked upstream files were copied. Frotz and these modifications remain
GPL-2.0-or-later; see COPYING. The Rust host executes this independent program
over a terminal and does not link it.

Build the door-capable curses interpreter with:

```sh
make curses SOUND_TYPE=none ZORK_DOOR=1
```

Use a clean build when changing `ZORK_DOOR` or frontend/compiler settings.
Run the adjacent real-terminal regression suite from the repository root with
`uv run --no-project vendor/frotz/src/curses/ux_door_test.py` (requires tmux).

Without `LATE_FROTZ_DOOR`, the resulting executable retains ordinary standalone
behavior. Door mode uses the current private edition directory and only writes
`autosave.lz` and `manual.lz` (plus adjacent replacement temporary files).
`LATE_FROTZ_DOOR=new|auto|manual` chooses startup behavior. `inspect-auto` and
`inspect-manual` validate the slot and print bounded JSON metadata without
initializing a terminal. Exit 20 means a deliberate return to the edition menu;
exit 21 means a selected save is invalid or unavailable. SIGHUP/SIGTERM request
normal-flow termination; they never serialize state in a signal handler.

The LZORK001 container has a big-endian header and one checksummed payload:
magic (8 bytes), format (u32), continuation kind (u32: 1 input, 2 SAVE branch),
story release (u32), checksum (u32), serial (6 bytes), zero padding (2 bytes),
Unix save time (u64), UTF-8 description length (u32), payload length (u32), and
payload FNV-1a checksum (u32). Payload contains the description (at most 128
Unicode characters / 512 UTF-8 bytes), decoded input
operands, RNG state, window properties, visible terminal cells and cursor, then
a Quetzal VM image. This is a private versioned format, not a stock Quetzal save.
Each terminal cell carries five Unicode codepoints and portable style flags;
wide-character continuation cells are explicit. Auto restoration resumes the
decoded READ directly; manual restoration follows the story's SAVE branch.
Screen data is bounded and keeps the pending prompt visible on a smaller terminal.

Local changes (2026-10-09): explicit door mode, atomic two-slot saves, coherent
input checkpoints with pending operands/RNG/screen, cosmetic manual descriptions,
RESTORE-to-menu interception, restricted interpreter file features, and graceful
shutdown. Modified files carry dated notices. The asset recipe distributes the
exact modified source and build scripts beside the executable.
