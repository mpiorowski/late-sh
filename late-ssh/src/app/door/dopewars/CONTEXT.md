# dopewars Door Context

## Metadata
- Scope: the dopewars door — the module in `late-ssh/src/app/door/dopewars` (proxy/state/render/mod) plus its screen lifecycle wiring in `late-ssh/src/app` (state/input/render/tick) and its config/build wiring (`config.rs`, `Dockerfile`, `Makefile`, `.env`). There is **no host crate** — dopewars runs in-process.
- Domain: dopewars, the real upstream curses "Drug Wars" trading game (GPLv2), run as a **local PTY child of late-ssh** and blitted into a ratatui widget below the top bar.
- Primary audience: LLM agents changing the dopewars launcher UI, the local-PTY proxy, input forwarding, or its config/build wiring.
- Last updated: 2026-06-30 (initial build — door playable in local/docker; prod enable deferred).
- Status: Active
- Parent context: `../../../../../CONTEXT.md`
- Stability note: `[STABLE]` sections change rarely; `[VOLATILE]` sections change with the launcher UI, keybindings, or build/deploy wiring.

---

## 0. Context Maintenance Protocol [STABLE]

Read this after root `CONTEXT.md` whenever a task touches the dopewars launcher, launch/leave behavior, the local-PTY proxy, input forwarding/filtering, or dopewars config/build wiring.

- Keep this file aligned with the proxy contract, the spawn args, config knobs, and known gotchas.
- Update root `CONTEXT.md` when routing, the top-level screen list/tab order, or global keybindings change.
- Treat tests and code as authoritative when comments drift; patch stale comments or this file before handoff.
- `mod.rs` stays declaration-only; do not add `pub use` re-export layers.

---

## 1. Summary [STABLE]

dopewars runs the **real upstream dopewars curses client on a PTY, in-process inside late-ssh**. Unlike NetHack (its own `late-nethack` SSH host) and Rebels (a remote SSH server), dopewars is the simplest door shape: a **local `openpty` child** — no host crate, no russh, no auth, no shared secret, no savegame, no save-lock. A dropped connection just ends the run. It's the local-PTY twin of the rebels/nethack *clients*: the child's terminal output is pumped into a shared `vt100::Parser` and blitted via `rebels::render::blit_screen`.

Core shape:
- `Screen::Dopewars` has no top-level number key. It is reached by selecting the dopewars card in the Games hub (page `3`, last card) and pressing `Enter`. `Enter` constructs the `State` (`enter_dopewars`) and `connect`s — spawning the child and switching to `Mode::Running` — in one step; the standalone launcher render is normally skipped.
- One per-session `DopewarsProcess` owns a background Tokio task that `openpty`s, spawns dopewars, and bridges PTY bytes into the shared `vt100::Parser`. The foreground reads that screen + a `ProxyStatus` flag.
- The child is spawned `dopewars -t -n -b -f <per-session score file>` (text client / single-player / black-and-white / per-session high-score path). See §3 for why `-b` is load-bearing.
- While Running, raw client bytes are forwarded straight to the child (minus mouse/paste noise) — dopewars, not late.sh, interprets keys. There is **no** key remap (no F1→help; dopewars is menu-driven). **Ctrl-C** ends the game: dopewars traps only `SIGWINCH`, so `^C` raises the default `SIGINT` through the PTY and the child dies, returning the session to the hub.
- **No late.sh persistence.** The per-session `-f` score file lives under `/tmp` and is removed on teardown. There are **no milestones, chips, or awards yet** — deferred second pass (scrape the final-score screen; see `DOOR_DOPEWARS_PREP.md`).

The door is gated behind `LATE_DOPEWARS_ENABLED` (default `false`); when disabled, `connect` is a no-op and the launcher shows "Currently unavailable".

---

## 2. Module Map [STABLE]

| File | Responsibility |
|---|---|
| `mod.rs` | Module declarations + framing comment. Declaration-only. |
| `proxy.rs` | `DopewarsProcess`: per-session local-PTY host. Owns the bridge task (`run_bridge`: `openpty` + `env_clear` + `setsid`/`TIOCSCTTY` + `IXON/IXOFF/IXANY` clear + a detached blocking reader that pumps PTY output into the parser), the shared `vt100::Parser`, the `ProxyStatus` flag, the input/resize command channel, the `-t -n -b -f` arg build, the TERM fallback, and `score_path`. Teardown is `kill_on_drop` SIGKILL — no graceful save. |
| `state.rs` | Per-session `State`: launcher/running `Mode`, config (bin/term/enabled), the optional `DopewarsProcess`, last viewport `Rect`, the post-exit input grace, `connect`, `set_viewport`, `forward_input`/`strip_input_noise`, and `tick` (flips back to Launcher on close). No award/milestone scraping. |
| `render.rs` | Ratatui rendering: `draw_landing`/`draw_launcher` (logo, blurb, market-ticker strip, hints) and `draw_running` which blits the live `vt100` screen via `rebels::render::blit_screen`. |

Cross-module wiring (outside this folder — the ~10 door touchpoints):
- `app/common/primitives.rs`: `Screen::Dopewars` (+ `next`/`prev` fall back to `Games`, `draw_tabs`/`page_title` label `"dopewars"`).
- `app/door/hub/state.rs`: `HubGame::Dopewars` + `ALL` (last card) + label.
- `app/door/hub/ui.rs`: `HubView.dopewars_enabled` + the landing match arm.
- `app/state.rs`: `App::dopewars_state`/`dopewars_term`/`dopewars_enabled`/`dopewars_bin`, `enter_dopewars`/`leave_dopewars`, `set_screen` enter/leave arms, and the Running-mode passthrough + exit-grace swallow in `App::handle_input`.
- `app/tick.rs`: `State::tick()` each app tick + return-to-`Games` once `!is_running() && !in_exit_grace()`.
- `app/render.rs`: `DrawContext.dopewars_enabled`/`dopewars_state`, take/restore `dopewars_state` (like rebels/nethack) so the draw path can `set_viewport(content_area)` before blitting, the dispatch arm, and the title-bar credit + in-game `Ctrl-C quit` hint.
- `app/input.rs`: hub launch arm (`set_screen` + `connect`, banner if disabled), dedicated-screen `Enter` launcher, and arrow/key dispatch no-ops (Running-mode bytes are forwarded raw upstream).
- `config.rs`, `state.rs` (`SessionConfig`), `ssh.rs`, `session_bootstrap.rs`, `tests/helpers/mod.rs`: thread `dopewars_enabled`/`dopewars_bin`.

---

## 3. Transport, Spawn Args, And Render [STABLE]

- `DopewarsProcess::spawn` creates an mpsc command channel, a shared `vt100::Parser` (sized to the viewport), a `ProxyStatus` mutex, and spawns the bridge task. On task end it forces `ProxyStatus::Closed` and wakes the render loop (so `tick()` returns to the launcher; without this the screen freezes on the last frame).
- `run_bridge` (unix): `openpty` → clear `IXON/IXOFF/IXANY` on the slave termios **before exec** (the Ctrl-S/XON freeze guard) → build the `dopewars` `Command` with `env_clear()` + allowlist (`TERM`, `LANG`/`LC_ALL=C.UTF-8`, `LINES`/`COLUMNS`) → `pre_exec` `setsid` + `TIOCSCTTY` → spawn with `kill_on_drop(true)`. A blocking **reader thread** pumps PTY output directly into the parser (+ repaint wake); the select loop writes client `Input` to the PTY master, applies `Resize` via `TIOCSWINSZ`, and breaks on `child.wait()`.
- **Spawn args: `-t -n -b -f <score>`.**
  - `-t` text (curses) client, `-n` single-player.
  - **`-b` (black-and-white) is load-bearing.** dopewars' default palette hard-codes a blue-on-blue window scheme that assumes a black terminal and renders nearly unreadable when embedded. Monochrome lets its colors map to `Color::Default → Reset`, so the game inherits the late.sh theme (same effect as the rebels/nethack doors). Selection/highlights still show via reverse-video (`A_REVERSE → Modifier::REVERSED`). Removing `-b` brings back the unreadable panels.
  - `-f <score>` points at a per-session, service-user-writable high-score file (`std::env::temp_dir()/late-dopewars-<uuid>.sco`), removed on teardown. **Do not run a setgid binary**: dopewars refuses a user `-f` under setgid (the from-source binary we ship is not setgid; see §5).
- **TERM fallback.** Empty/unknown client TERM → `xterm-256color` (renders on every modern terminal). A real terminfo entry is required for ncursesw ACS line-drawing; `ncurses-term` in the image covers alacritty/st/rxvt natively.
- **Sizing.** `State::set_viewport` (from the draw path) resizes the local parser and sends a `Resize`; the bridge applies `TIOCSWINSZ`; the kernel signals `SIGWINCH` and dopewars does a full `endwin()`+`newterm()` rebuild.
- **Render.** `draw_running` blits the current `vt100` screen; before `Running` it shows "Starting dopewars...". The app frame title shows a dimmed `by dopewars.sourceforge.io` credit, plus `· Ctrl-C quit` while running.

---

## 4. Configuration And Build [VOLATILE]

### Config (env → `Config` → `SessionConfig` → `App`)
- `LATE_DOPEWARS_ENABLED` (default `false`): when false, `connect` is a no-op and the launcher shows "Currently unavailable".
- `LATE_DOPEWARS_BIN` (default `dopewars`, resolved via `PATH`): the dopewars binary path. In images it's `/usr/games/dopewars`.
- No host/port/secret — there is no network transport.

### Binary sourcing — **built from verified upstream source, dopewars 1.6.2**
- Compiled in the Dockerfile `dopewars-build` stage (terminal-only: `--disable-gui-client --disable-gui-server --enable-curses-client`). The stage downloads the pinned 1.6.2 SourceForge tarball, verifies SHA-256 (`sha256sum -c`, fail-closed), builds, and copies the binary to `/dopewars`. Version/URL/checksum are `ARG`s.
- **Build quirk (`make LIBS="-lncursesw"`):** dopewars' release Makefile drops `$(CURSES_LIBS)` from `dopewars_LDADD` when the GTK client is disabled, so the link fails with undefined `initscr`/`newterm`/… The curses lib is injected via the trailing `$(LIBS)` on the link line. Keep this on any version bump.
- The binary is **self-contained**: drug/location data is compiled in (no data dir), and it is **not setgid** (so the per-session `-f` is honored). Runtime deps: `libglib2.0-0` + `libncursesw6` (+ `libcurl4`, pulled in by the optional metaserver client).

### Images (Dockerfile)
- dopewars is a **local PTY child of late-ssh**, so the binary + runtime libs ship in the late-ssh images, **not** a separate `service-` container: copied into `base` (dev, for `dev-ssh`'s cargo-watch) and `runtime-ssh` (prod) at `/usr/games/dopewars`. `base` also gains `libglib2.0-0`/`libcurl4` (it already had `libncursesw6`); `runtime-ssh` installs `libglib2.0-0`/`libncursesw6`/`libcurl4`/`ncurses-term`.
- `Makefile` + `.env` thread `LATE_DOPEWARS_ENABLED=1` / `LATE_DOPEWARS_BIN=/usr/games/dopewars` (mirroring the nethack block).

### Prod (deferred)
- **Not yet wired into prod infra.** `infra/service-ssh.tf` does not inject `LATE_DOPEWARS_ENABLED`, so on prod the door defaults off and shows "Currently unavailable" (graceful, not a crash). The binary already ships in `runtime-ssh`. To go live later, set on the prod ssh service: `LATE_DOPEWARS_ENABLED=1` and `LATE_DOPEWARS_BIN=/usr/games/dopewars`. Intentional per the build owner ("no prod infra for now").

---

## 5. Critical Invariants [STABLE]

- The child process is authoritative for game state; late.sh owns only the terminal bytes (vt100) and a status flag. No save, score, or position is persisted (the `-f` file is per-session scratch, deleted on teardown).
- While Running, do not route dopewars bytes through the normal late.sh input pipeline — forward them raw. There is no key remap; `Ctrl-C` ending the game is the intended leave path (dopewars catches no `SIGINT`).
- Keep mouse/paste stripping in `forward_input`. With `?1003h` mouse tracking on, unfiltered motion reports' leading `ESC` would leak into the curses game as stray commands.
- Keep `-b` (monochrome) in the spawn args, or the blue-on-blue panels become unreadable embedded (§3).
- Do **not** run a setgid dopewars binary — it refuses the per-session `-f` under setgid. The from-source binary is non-setgid; if `LATE_DOPEWARS_BIN` ever points at a distro package, `chmod g-s` it.
- Keep XON/XOFF flow control **off** on the PTY, or a stray Ctrl-S freezes output until Ctrl-Q.
- Force `ProxyStatus::Closed` and wake the render loop the instant the child exits, before cleanup, or the screen freezes on the last frame.
- Spawn the child with `env_clear()` + an explicit allowlist (incl. a UTF-8 `LANG`/`LC_ALL` for ncursesw).
- Treat all exits identically — quit, end-of-game, crash all return to the hub.
- When disabled, fail soft (launcher message + no-op connect), never panic.
- `mod.rs` stays declaration-only.

---

## 6. Tests And Verification [STABLE]

Root policy applies: agents should not run `cargo test`/`nextest`/`clippy` as blocking verification; mention the focused command in handoff.

Inline pure tests cover:
- `proxy.rs`: `score_path` is account-scoped and ends `.sco`.
- `state.rs`: `connect` no-op when disabled; `forward_input` without a proxy is a no-op; `strip_input_noise` drops mouse/paste but keeps keys/arrows; exit-grace opens on close and counts down.
- `app/door/hub/state.rs` + `app/common/primitives.rs`: selector ordering and screen `next`/`prev` place `Dopewars` correctly.

The PTY bridge (`run_bridge`) is process-bound and not unit-tested; verify launch/play/quit manually against a real `dopewars` binary.

Focused command for human verification:

```bash
cargo test -p late-ssh dopewars
```

---

## 7. Known Gotchas [VOLATILE]

- **Trailing game keys can quit the whole app (exit-grace).** dopewars' end-of-game high-score screen makes players mash keys; the game exits mid-burst and the remaining keys land on the launcher, where `q` is the **global** app-quit (drops the SSH session). Guard: on close, `State::tick` opens `EXIT_GRACE_TICKS` (~0.66s); while `in_exit_grace()`, `App::handle_input` swallows launcher input. `connect` resets it. (Same pattern as the nethack door.)
- **Curses link bug (`make LIBS="-lncursesw"`).** See §4 — the release Makefile omits `$(CURSES_LIBS)` from `dopewars_LDADD` with the GUI disabled; the build fails on undefined curses symbols without the override.
- **Unreadable colors without `-b`.** See §3/§5 — dopewars' default palette is blue-on-blue and assumes a black terminal.
- **setgid + `-f`.** A setgid dopewars binary refuses a user `-f` and would error/ignore the per-session score path. Ship a non-setgid binary (the from-source build is fine).
- **Ctrl-S freeze (XON/XOFF).** Cleared on the PTY before exec, same as the nethack host.

### Possible future work
- Milestones/chips/awards by scraping the final-score screen (the deferred second pass; mirror `nethack/milestone.rs` + `award.rs`).
- Optional shared/competitive market: one `dopewars -S` server with `-o`/`-p` clients per session (single-player ships first; see `DOOR_DOPEWARS_PREP.md`).
- Wire the enable flag into `infra/service-ssh.tf` to turn the door on in prod (§4).
