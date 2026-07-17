# Next door: dopewars — build prep

Status: **research + prep only, no code written.** Companion to `DOOR.md`.
Decision date 2026-06-29. Green Dragon (LotGD native port) is the door being
added now; **dopewars is the recommended one after it.**

## Why dopewars (and not Usurper / twclone)

Optimizing for: clean license + low effort + genre variety + nostalgia.

- **Genre variety.** Green Dragon already fills the LORD-style fantasy-RPG slot.
  dopewars is economy/trading ("Drug Wars") — a different genre, so it broadens
  the lineup instead of duplicating it. Usurper is GPL and easy but is *another*
  fantasy RPG, so it adds recognition, not variety.
- **Lowest real effort, and lower than NetHack.** dopewars is a clean
  **pattern 2** terminal binary, and unlike NetHack it has **no savegame and no
  save-lock**, so none of the SIGHUP-graceful-save / getlock-slot machinery that
  `late-nethack` needs applies. A dropped connection just ends the run.
- **UTF-8 native — no CP437 work.** dopewars uses `setlocale` + ncursesw + ACS
  line-drawing (terminfo-mapped), so with a UTF-8 `LANG` and a valid `$TERM` it
  renders correctly with **zero CP437→UTF-8 translation**. Usurper is DOS-origin
  ANSI/CP437 and *would* need a translation layer.
- **Right session shape.** Default `NumTurns = 31` (~10–20 min), bounded, ends
  with a high score. Fits the "drop in, play a round" door format.
- **Clean license, no caveats.** `GPL-2.0-or-later`, no Commons Clause, no
  non-commercial/trademark/SaaS clause. The chip economy and donations are fine.
  GPLv2 distribution obligations only trigger if we *distribute a modified
  dopewars binary* — hosting it untouched does not.

### Corrections to DOOR.md found during this pass
- **twclone is GPL-2, not MIT.** The README says MIT but the actual `LICENSE`/
  `COPYING` files are GPLv2 (GitHub detects GPL-2.0). Update the green-table row.
- **twclone is `v1.0.0-rc1` (Dec 15 2025), not a finished 1.0.0**, with ~175
  open issues and federation/economy/NPC systems still deferred. Still the best
  open TradeWars, but it's a later, deliberate project (run a persistent
  Postgres server + write a native JSON-protocol client), not an easy next door.
- **dopewars latest is `1.6.2` (Jun 2022)**, in Arch `extra`, Debian sid,
  Nixpkgs, Homebrew. `master` still gets commits but only GTK-deprecation fixes.
- DOOR.md's "`-r` is the AI server" is wrong: `-r` is `--pidfile`; AI players are
  `-c/--ai-player`. Single-player is `-n`; server mode is `-S` (private) / `-s`
  (public).

## Recommended integration approach

**Local PTY child inside `late-ssh` — no separate host crate.** NetHack lives in
its own `late-nethack` russh host because of save-lock isolation and graceful
saves; dopewars needs neither. Spawn the curses client on a local PTY, parse
with `vt100`, and reuse the existing blit renderer. This is lighter than NetHack
(no host crate, no shared-secret auth, no SIGHUP save dance).

- **Default mode: single-player** — `dopewars -t -n -f <per-session>.sco` per SSH
  session. Isolated, simplest, matches the per-session door feel.
- **Optional later: shared/competitive** — run one `dopewars -S` on localhost and
  spawn `dopewars -t -o 127.0.0.1 -p 7902` clients per session. Defer unless we
  want a live shared market; single-player ships first.

### Build recipe (terminal-only, no GTK/X11)
```
./autogen.sh   # git checkout only; tarball ships ./configure
./configure --disable-gui-client --disable-gui-server --enable-curses-client
make
```
Runtime deps: **`glib2` + `ncursesw`** only. Drop GTK2/3/4, gdk-pixbuf, SDL2,
ALSA, libcurl (all GUI/sound/networking-optional). Or just use the distro
package (Arch `extra` `dopewars`) if we don't need a custom build.

### Per-session launch
`dopewars -t -n -f /run/late-dopewars/<session>.sco` inside the PTY, with:
- `TERM` set to a real terminfo entry (mirror NetHack's `xterm-256color`
  fallback for unknown terms),
- a UTF-8 `LANG`/`LC_ALL`,
- a per-instance writable score path via `-f` (sidesteps the setgid-games score
  file; note dopewars refuses a user `-f` when setgid, so don't run setgid).

### Things our wrapper must own
- **Teardown.** dopewars traps only SIGWINCH — no SIGINT/SIGHUP handler. On
  disconnect we send SIGTERM/SIGKILL ourselves (simple; no graceful save needed
  since there are no savegames).
- **Resize.** SIGWINCH does a full `endwin()`+`newterm()` rebuild — works, just
  forward window-size changes to the PTY (`TIOCSWINSZ`) like the NetHack host.
- **Score file path** — per-instance, service-user-writable (above).

## Code integration checklist

Reuse the rendering twins already in the tree (`rebels::render::blit_screen`,
the `vt100::Parser` model from `nethack/proxy.rs`). A local-PTY door is closest
to **rebels** (proxy + state + render + identity) but spawning a child instead
of dialing a remote SSH server.

New module `late-ssh/src/app/door/dopewars/`:
- `proxy.rs` — spawn dopewars on a local PTY (nix/portable-pty), pump output into
  an `Arc<Mutex<vt100::Parser>>`, accept `Input`/`Resize` commands. Local twin of
  `nethack/proxy.rs` / `rebels/proxy.rs` (no russh, no auth).
- `state.rs` — Launcher/Running mode machine; `connect()` spawns the child.
- `render.rs` — landing card + `blit_screen()` of the vt100 grid.
- `mod.rs` — declarations only.
- (optional) `milestone.rs` + `award.rs` — see below.

Registry wiring (same ~10 touchpoints every door needs; line refs current as of
this writing):
- `door/game.rs:11` — add `DoorGameId::Dopewars`.
- `door/hub/state.rs:8` — add `HubGame::Dopewars` + extend the `ALL` array.
- `door/hub/ui.rs:24` — match arm → `dopewars::render::draw_landing`.
- `app/common/primitives.rs` — add `Screen::Dopewars` (not in the tab cycle;
  falls back to `Games` hub on leave).
- `app/state.rs:449` — add `dopewars_state` field; `enter_dopewars()` /
  `leave_dopewars()` near the nethack ones (~:1175).
- `app/render.rs:1149` — render dispatch arm + title-bar case.
- `app/input.rs:2236` — hub launch arm (set screen + `enter` + `state.connect()`,
  like NetHack); `:2638` dedicated-screen key handler.
- `ssh.rs` / `main.rs` — only if we add config (enable flag, bin path); a
  local-PTY door may need just a `LATE_DOPEWARS_BIN` env, no service threading.

No DB model or migration required for single-player (no persistent character).

## Milestones / chips (optional, ship after the door works)

dopewars has no in-game persistence beyond the end-of-game high score, so awards
are best derived by scraping the final-score screen via the vt100 grid (same
technique as `nethack/milestone.rs`), e.g.:
- "started a run" feed event on connect (no reward),
- a chip payout for beating a score threshold or finishing all 31 days,
- a profile badge for a high-score finish.
Wire chips/awards through the same path as `lateania/svc.rs` /
`nethack/award.rs` (lifetime template, once-per-account). Keep this for a second
pass; the door is playable without it.

## Open decisions for the builder
1. Single-player only first (recommended), or stand up the shared `-S` server
   for a competitive market? Ship single-player, revisit shared later.
2. Distro package vs. our own `--disable-gui` build? Package is fine to start;
   own build if we want to pin/patch.
3. Theme/payments: the simulated-drug-dealing framing is a payment-processor /
   brand-fit question (not a license one) — flag for the launch/brand call, it
   does not block the technical build.

## Sources
- dopewars: [GitHub](https://github.com/benmwebb/dopewars) ·
  [site/news](https://dopewars.sourceforge.io/news.html) ·
  [server docs](https://dopewars.sourceforge.io/docs/server.html) ·
  [commandline](https://dopewars.sourceforge.io/docs/commandline.html) ·
  [Repology versions](https://repology.org/project/dopewars/versions)
- twclone license/maturity: [LICENSE](https://raw.githubusercontent.com/rdearman/twclone/master/LICENSE) ·
  [releases](https://github.com/rdearman/twclone/releases) ·
  [protocol docs](https://github.com/rdearman/twclone/tree/master/docs/PROTOCOL.v3)
- Usurper (genre-redundant alt): [rickparrish fork](https://github.com/rickparrish/Usurper) ·
  [RMDoor STDIO default](https://raw.githubusercontent.com/rickparrish/RMDoor/master/door.pas)
</content>
</invoke>
