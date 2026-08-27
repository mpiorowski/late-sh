# NetHack Door Context

## Metadata
- Scope: the NetHack door as a whole — the **client** in `late-ssh/src/app/door/nethack` (+ the screen lifecycle in `late-ssh/src/app`: state/input/render/tick wiring) **and the standalone host crate `late-nethack/`**. There is no separate `late-nethack/CONTEXT.md`; this file is the single source for both halves.
- Domain: NetHack, the real upstream roguelike, run on a PTY inside a **dedicated `late-nethack` SSH host** and reached by late-ssh as a network-proxied door (the *Rebels* camp).
- Primary audience: LLM agents changing the NetHack launcher UI, the SSH client transport, the host crate (PTY bridge / auth / TERM handling), input forwarding, or its config/deploy wiring.
- Last updated: 2026-08-24 (arrow keys: the curses windowport is compiled in as a per-player rc opt-in, `door-nethack` image bumped to `5.0.0-r3`, and the client retypes cursor keys via the shared `keys::to_application_cursor` translator, mirroring brogue/dcss)
- Status: Active
- Parent context: `../../../../../CONTEXT.md`
- Stability note: Sections marked `[STABLE]` should change rarely. Sections marked `[VOLATILE]` are expected to change when the launcher UI, keybindings, or deploy wiring change.

---

## 0. Context Maintenance Protocol [STABLE]

Read this file after root `CONTEXT.md` whenever a task touches the NetHack launcher, launch/leave behavior, the SSH client transport, the `late-nethack` host (PTY bridge, auth, TERM resolution), input forwarding/filtering, the F1→`?` help remap, or NetHack config/deploy wiring.

- Keep this file aligned with the SSH transport contract, the client/host split, input-filter behavior, config knobs, and known gotchas.
- Update root `CONTEXT.md` when routing, the top-level screen list/tab order, global keybindings, or deploy/config contracts change.
- Treat tests and code as authoritative when comments drift. Patch stale comments or this file before handoff.
- Do not add `pub use` re-export layers; `mod.rs` should stay declaration-only.

---

## 1. Summary [STABLE]

NetHack runs the **real upstream NetHack binary on a PTY**, but **not** inside late-ssh. It lives in its own crate/pod, `late-nethack`, a minimal russh **server** that spawns one `nethack` child per SSH session. late-ssh reaches it exactly like the Rebels door reaches a remote SSH server: the door is a russh **client** that streams the remote terminal through a `vt100::Parser` and blits it into a ratatui widget below the top bar. SSH *is* the transport — there is no custom IPC.

(History: NetHack used to run as a local `openpty` child inside the `service-ssh` container. It was extracted to `late-nethack` on 2026-06-25 for a real secret boundary, independent resource limits, and an isolated blast radius. A PTY can't cross containers, so it became a network door.)

Core shape:
- `Screen::Nethack` has no top-level number key. It is reached by selecting the NetHack card in the Games hub (page `3`) and pressing `Enter`. The top-level tab order is now `Dashboard(1) Arcade(2) Games(3) Tables(4) Artboard(5) Directory(6)`.
- `Enter` on the selected NetHack card opens the SSH connection to the host and switches to Running mode in one step (the standalone launcher render is normally skipped).
- One per-session `NethackProcess` (a russh client; the twin of `door::rebels::proxy::RebelsProxy`) owns a background Tokio task that connects to `late-nethack`, requests a PTY + shell, and bridges the remote bytes into a shared `vt100::Parser`. The foreground reads that screen and a `ProxyStatus` flag.
- **Identity vs authorization are split.** The connection authenticates with a single Ed25519 key both ends derive from `LATE_NETHACK_SECRET` (authorization). The account's **arcade handle** (user-chosen, immutable, claimed once via the shared one-time claim modal over the launcher (`landing::draw_name_modal`); see the DCSS door's CONTEXT §invariants and `door/arcade.rs`) travels as the **SSH username** (identity) and becomes `-u`; the host re-sanitizes it. Before 2026-07-18 the playname was a derived `late_<hex>` hash; those old saves were deliberately orphaned on the PVC when handles landed (delete them from the playground at will).
- While Running, raw client bytes are forwarded straight to the host→child (minus mouse/paste noise), so NetHack — not late.sh — interprets keys. `F1` is the only key late.sh keeps, and it is merely **remapped to NetHack's own `?` help**.
- Per-player saves come from `-u <playname>` against the host's shared playground (a PVC), so deaths seed common **bones** across users. Game state has **no late.sh-side persistence**; saves/bones live on the host's disk/PVC.
- **The log pipe (boards, badges, feed events — no screen scraping).** NetHack appends two machine-readable files into `VAR_PLAYGROUND` on the host's PVC: `xlogfile` (one TAB-separated `key=value` line per finished game; XLOGFILE ships defined in 5.0.0, asserted fail-closed in the Dockerfile) and `livelog` (live mid-run achievement lines; the LIVELOG compile chain rides `NHL_SANDBOX`+`CHRONICLE`, and the sysconf `LIVELOG=0x0002` mask turns writing on — both handled in the Dockerfile). The host serves them over the reserved `late_stats` SSH username (`late-nethack/src/stats.rs`, the late-dcss twin): the client pushes per-file byte offsets in env requests (`LATE_DOOR_STATS_CURSORS`, split across several requests when large), the host streams `<file-id>\t<offset>\t<line>` frames with tail -f semantics and stays stateless. late-ssh's ingestion task (`app/door/ingest/`: `nethack.rs` pure parsers verified against the pinned 5.0.0 source, `svc.rs` orchestration spawned in main.rs behind `LATE_NETHACK_ENABLED`, cursors in `door_log_cursors`) lands one `door_runs` row per finished game (`death=ascended` → win, `quit`/`escaped` → quit/leaving, depth = `maxlvl`) and a `door_milestones` row for the Amulet pickup (livelog message "acquired The Amulet of Yendor"), idempotent via unique `(game, source_file, source_offset)`. From this pipe: the NetHack board triple on the Leaderboards page (Wins all-time, Deepest Dive, Top Score), the badge pair `NHA` (Amulet, 20k chips, granted at pickup from the livelog stream, the only Amulet source: the xlogfile `achieve` bit is not read for it) and `NHY` (ascension, 50k, granting only itself: a win line never back-grants the pickup, so a pickup the livelog missed is an ingest bug that shows as a missing badge, never a second payout) — the badge is once per account, the chips repeat once per ingested run behind a 7-day per-milestone lockout (SHOP.md Phase 6, migration 158), and #lounge feed events for deaths (with `deathlev` + NetHack's own death text) and wins (freshness+recency gated so backfill never floods the feed). Wizard/explore games still write flagged xlogfile lines (`flags` bits 0x1/0x2) and are skipped wholesale; explore mode itself is locked off via a blanked sysconf `EXPLORERS` because livelog lines carry no such flag. The mid-run "descended to level N" feed lines are gone by owner decision; livelog achievement moments replace that flavor where tracked. "Started a NetHack game" stays connect-based in the client (`state.rs::connect`, via a plain `ActivityPublisher`) — it never was a scrape. The old vt100 scrape (`milestone.rs`, `status.rs`, `award.rs`, `scan_screen`) is deleted; this door no longer has any late.sh-side persistence exception of its own.

- **Per-account `.nethackrc` (2026-08-05).** The account's rc lives in Postgres (`door_rcs` table, `late-core/src/models/door_rc.rs`; 16KB cap, no NULs), authored via a paste box on the Games hub: `c` on the NetHack card (or its landing page, which bounces to the hub) opens the modal, a bracketed paste replaces the whole file, `x` clears back to defaults (`app/door/rc.rs` holds the `DoorRcService` + paste sanitizer, `app/door/hub/ui.rs` the modal). At every launch the client pushes the content base64-encoded as one SSH env request (`LATE_DOOR_RC_B64`, `want_reply=false`, sent before `request_pty`); the host decodes it (`late-nethack/src/rc.rs`: base64 → cap → UTF-8 → no NUL), materializes `<data_dir>/rc/<playname>.nethackrc` (`host.rs::materialize_rc`), and sets `NETHACKOPTIONS=@<path>` so it overrides the shared `$HOME/.nethackrc`. An empty push deletes the file; no push at all (an older client) leaves whatever is on disk. All filesystem errors fail soft (warn + launch with defaults). The env var name is duplicated client/host like the identity derivation; keep the copies in sync. DCSS shares the same contract (`-rc` flag instead of NETHACKOPTIONS, plus a `macro_dir` re-force guard — see the DCSS CONTEXT); Brogue is already per-player upstream and has no paste box. **The unfiltered push is safe on this build, verified against the pinned 5.0.0 source (re-verify on version bumps):** the dir directives a user rc can carry (`HACKDIR`/`SAVEDIR`/`LEVELDIR`/`BONESDIR`/`SCOREDIR`/`LOCKDIR`/...) are parsed but compiled to no-ops on unix (`cfgfiles.c`; they need `NOCWD_ASSUMPTIONS`/`MICRO`, which only Amiga/DOS/Windows define), `SOUND`/`SOUNDDIR` need `USER_SOUNDS` which is defined nowhere, `WIZKIT` is only read `if (wizard)` and the host never passes `-D`, `NAME=` is overwritten because `-u` is processed after the config file, and the dangerous sysconf directives (`SHELLERS`, `MAXPLAYERS`, ...) are rejected from user config. `DOGNAME=`/`CATNAME=` ARE honored, which only mattered to the deleted screen-scrape matchers; the log pipe reads host-written files a player rc cannot influence. `OPTIONS=playmode:explore` in a pushed rc is refused at startup by the blanked sysconf `EXPLORERS` (options.c routes it through the same `authorize_explore_mode` gate as the `X` command).

The door is gated by the `nethack_enabled` profile flag in `late-ssh/src/config.rs` (enabled in every current profile); when disabled, `connect` is a no-op and the launcher shows "Currently unavailable". The host pod is deployed unconditionally (the flag gates only the client).

---

## 2. Module Map [STABLE]

### Client — `late-ssh/src/app/door/nethack/`

| File | Responsibility |
|---|---|
| `mod.rs` | Module declarations + framing comment. Declaration-only. |
| `proxy.rs` | `NethackProcess`: per-session russh **client** to the host. Owns the bridge task (`run_bridge`), the shared `vt100::Parser`, the `ProxyStatus` flag, and the input/resize command channel; `ProcessConfig.playname` carries the arcade handle. Near-clone of `door::rebels::proxy`. |
| `identity.rs` | `derive_client_key(secret)`: the shared-secret → Ed25519 key derivation (blake3, domain `late.sh/nethack/v1`). Must stay byte-identical to the host's copy — a KAT pins it (§8). |
| `state.rs` | Per-session `State`: launcher/running `Mode`, connection config (host/port/secret/term/enabled), the optional `NethackProcess`, last viewport `Rect`, the post-exit input grace, the idle shutdown, and input interception/forwarding (`intercept_input` remaps F1→`?`, `forward_input`, `keys_for_game`, `strip_input_noise`). Posts the connect-based "started a NetHack game" feed event via an optional `ActivityPublisher`. No scraping: badges and death/win events are the log pipe's (`app/door/ingest/`). |
| `render.rs` | Ratatui rendering: `draw_launcher` (logo, blurb, hints) and `draw_running` which blits the live `vt100` screen via `rebels::render::blit_screen`. No late.sh help overlay — in-game help is NetHack's own `?`. |

### Host — `late-nethack/` crate (standalone binary)

| File | Responsibility |
|---|---|
| `main.rs` | Tracing init, `Config::from_env`, load/generate the SSH host key, run the russh server (`run_on_address`). |
| `config.rs` | `Config`: `bin`, `data_dir` (child `HOME`), `secret`, listen addr/port, host-key path, idle timeout. |
| `server.rs` | russh `Server`/`ClientHandler`: `auth_publickey` (compares the derived key — see §7), `pty_request`, `shell_request`, `data`, `window_change_request`, `channel_eof/close`. Holds `effective_term` (TERM fallback, §4). |
| `host.rs` | `PtyHost`: the per-session PTY bridge. `openpty` + `env_clear` + `setsid`/`TIOCSCTTY` + `IXON/IXOFF/IXANY` clear + `TIOCSWINSZ` + the **detached** reader. Output flows to the SSH channel handle; client bytes flow to the PTY master. (This is the old in-process `run_bridge`, relocated and inverted.) |
| `identity.rs` | `derive_client_key(secret)` — identical to the client copy (KAT-pinned). |
| `playname.rs` | `sanitize(username)`: keep `[A-Za-z0-9_]`, cap at `PL_NSIZ`, fall back to `late`. Defense-in-depth on the `-u` arg. |
| `rc.rs` | `decode_rc(env value)`: base64 → size cap → UTF-8 → no-NUL validation of the pushed per-account rc. Constants mirror the client (`RC_ENV_VAR`, `MAX_RC_BYTES`). |
| `stats.rs` | The `late_stats` log-streaming session: cursor env parsing, complete-line framing (`frame_lines`), and the stateless tail loop over `xlogfile`/`livelog` in `var_dir`. Constants mirrored in late-ssh's `app/door/ingest/stream.rs`; the late-dcss twin. |

Cross-module wiring (client side, outside this folder):
- `app/state.rs`: `App::nethack_state`, `enter_nethack`/`leave_nethack`, and the Running-mode input passthrough in `App::handle_input` (intercept F1, else forward raw bytes).
- `app/input.rs`: launcher `Enter` → `enter_nethack` + `connect`; `7` global screen switch; topbar hit-test columns; arrows are a no-op (Running-mode arrows are forwarded raw upstream).
- `app/render.rs`: takes `nethack_state` out (like rebels) so the draw path can `set_viewport(content_area)` before blitting.
- `app/tick.rs`: calls `State::tick()` each app tick to detect connection close.
- `config.rs`, `state.rs` (`SessionConfig`), `ssh.rs`, `session_bootstrap.rs`, `src/test_helpers.rs`: thread the `nethack_enabled`/`nethack_host`/`nethack_port`/`nethack_secret` fields through.

---

## 3. Screen Lifecycle And Input Capture [STABLE]

- NetHack is no longer a top-level tab. It is launched from the Games hub (`late-ssh/src/app/door/hub`, page `3`), a selector that renders the selected door game's landing; NetHack's landing is drawn by the now-`pub` `render::draw_landing`. `Screen::Nethack` is a live-game-only screen.
- Pressing `Enter` on the focused NetHack card in the hub calls `set_screen(Screen::Nethack)` (which runs `enter_nethack`, constructing `State`) then `State::connect`, opening the SSH connection and switching to `Mode::Running` in one step — the standalone launcher (`Mode::Launcher` render) is normally skipped.
- **Detach instead of teardown while running.** Navigating away from the screen with a live game (the ` detach below, or any screen switch) keeps `nethack_state` — and with it the SSH connection and the host child — alive: `set_screen` only drops a non-running state. The player resumes from the Games hub card (green pip + "resume your game in progress" line, Enter) or via the backtick workspace cycle, where live roguelike doors are the last stops (`lobby/workspace.rs`, `GameWorkspace::Door`). Teardown paths that do drop the state: the game exiting on its own (tick sees `Closed`), the **20-minute input-idle shutdown** (`IDLE_SHUTDOWN` in `state.rs`: no forwarded keystroke for 20 minutes → the proxy is dropped, so the host SIGHUP-saves; applies attached or detached, no exit grace), an off-screen state observed not running (reaped in `app/tick.rs` so the hub card stops advertising), and session end. Dropping `nethack_state` drops `NethackProcess`, whose `Drop` aborts the client bridge task → the SSH connection closes → the host's `channel_close` drops its `PtyHost`. The host bridge then **SIGHUP-saves the live child** (a recoverable save plus a getlock-slot release) before exiting, with a SIGKILL backstop for a wedged child. See §4 (Host) and the wedge note in §9.
- `State::tick` (each app tick) flips back to `Mode::Launcher` if the connection closed for any reason (clean `S` save, death, quit, crash, or network drop) — all exits are treated identically. `App::tick` then returns the session to the Games hub once the post-exit input grace (`in_exit_grace`) has elapsed.

Input capture contract (client side; unchanged by the extraction):
- The **launcher** behaves like a plain page: only `Enter` is consumed; every other key falls through to normal global handling. **Exception:** for a short post-exit grace window the launcher swallows *all* input — see the exit-grace gotcha in §9.
- While **Running**, `App::handle_input` intercepts bytes *before* the normal input pipeline: if `state.is_running()`, a whole-chunk backtick (`DOOR_DETACH_KEY`) detaches via the backtick workspace cycle (the game keeps running), else it calls `intercept_input` (F1 remap) then `forward_input` straight to the host, and returns. Number keys, `q`, `Esc`, etc. all reach NetHack; ` and F1 are the only keys late.sh keeps.
- `F1` (`ESC O P` or `ESC [ 11 ~`) is **remapped to NetHack's own `?` help**: `intercept_input` forwards a literal `?` and swallows the F1 bytes, both giving F1 the conventional meaning and stopping the raw escape from leaking as stray commands.
- `forward_input` strips mouse reports (SGR `ESC [ < … M/m`, legacy X10 `ESC [ M b x y`) and bracketed-paste markers. late.sh keeps any-event mouse tracking (`?1003h`) on for its own UI; those motion reports' leading `ESC` would otherwise cancel every NetHack menu. A sequence truncated at a chunk boundary falls through unchanged.
- After the noise strip, `keys_for_game` retypes the cursor keys into the mode the game asked for (`keys::to_application_cursor` when `Screen::application_cursor` is on), same wiring as brogue/dcss. This only matters for the curses windowport (opt-in, §6): the default tty windowport reads raw `getchar()` and decodes no escape sequences at all, so arrows there arrive as ESC + `[` + letter in either mode. That is upstream tty behavior, not a late.sh bug.

---

## 4. Transport Architecture [STABLE]

### Client (`proxy.rs`, in late-ssh) — the vt100 side

- `NethackProcess::spawn` creates an mpsc command channel, a shared `vt100::Parser` (sized to the viewport), a `ProxyStatus` mutex, and spawns the bridge task. On task end it forces `ProxyStatus::Closed` and wakes the render loop (so `tick()` returns to the launcher; without this the screen freezes on the last frame, e.g. right after `S` saves).
- `run_bridge` is a russh client (`AcceptAnyHostKey`): `client::connect` → `authenticate_publickey(username = cfg.playname (the arcade handle), key = derive_client_key(secret))` → `channel_open_session` → `request_pty` → `request_shell` → status `Running`. Then a `tokio::select!` loop: command channel (`Input` → `channel.data`; `Resize` → `window_change`) and `channel.wait()` (remote `Data`/`ExtendedData` → `parser.process` + repaint; `Eof`/`Close`/`ExitStatus` → break).
- The vt100 parser lives **client-side only**. The host streams raw bytes; only late-ssh interprets them into a screen (shared with Rebels via `rebels::render::blit_screen`).

### Host (`late-nethack`) — the PTY side

- `ClientHandler` (one per SSH connection): `auth_publickey` checks the derived key and stores the sanitized playname; `pty_request` records term/cols/rows; `shell_request` resolves the effective TERM and spawns a `PtyHost`, handing it `session.handle()` + the `ChannelId`.
- `PtyHost::spawn` → `run_bridge` (unix only): `openpty`, clear `IXON/IXOFF/IXANY` on the slave termios **before exec** (§9), build the `nethack` `Command` with `env_clear()` + allowlist (`-u <playname>`, `TERM`/`HOME`/`LINES`/`COLUMNS`), wire slave→stdio, `pre_exec` `setsid` + `TIOCSCTTY`. A blocking **reader thread** pumps PTY output to an unbounded channel; the select loop forwards those chunks to `handle.data(channel, …)`, writes client `Input` to the PTY master, applies `Resize` via `TIOCSWINSZ`, and breaks on `child.wait()`.
- On child exit: send `eof`+`close` to the channel **immediately** (so the client returns to its launcher now), then kill the child and **detach** the reader; do NOT join it (the save-compressor gotcha, §9).
- **Teardown while the game is still live** (client disconnect, e.g. a service-ssh rollout, or a host SIGTERM): the bridge classifies the stop as `StopReason::Teardown` and sends the child **SIGHUP** so NetHack runs its hangup-save (recoverable save + getlock-slot release), waits up to `HANGUP_SAVE_GRACE` (5s) for it to exit, then SIGKILLs as a backstop. `PtyHost::Drop` no longer aborts the bridge task; it lets `cmd_tx` close so the bridge reaches this graceful path. A pod-wide SIGTERM flips a `watch` channel (created in `main.rs`, threaded through `Server`/`Shared`) that every live bridge observes; `main.rs` holds the process up for `SHUTDOWN_GRACE` (8s) so the saves land before exit. A game that exits on its own (in-game quit/death/`S` save) is `StopReason::ChildExited` and needs no SIGHUP.
- **TERM fallback (`effective_term`).** nethack's ncurses aborts `Unknown terminal type` for any TERM the host has no terminfo entry for. `effective_term` checks the host's terminfo dirs for the client's TERM and falls back to `xterm-256color` (which every modern terminal renders) when absent — this is what makes Ghostty/kitty/wezterm clients work. `ncurses-term` in the image covers alacritty/rxvt/etc. natively. See §9.

### Sizing
- `State::set_viewport` (client, from the draw path) resizes the local parser and sends a `Resize` command; the client forwards a `window_change`, the host applies `TIOCSWINSZ`, and the kernel signals `SIGWINCH` to the child so curses redraws.

### Render
- `draw_running` blits the current `vt100` screen via `rebels::render::blit_screen`. Before the proxy reports `Running` it shows "Starting nethack...".
- The app frame title bar (`app/render.rs::app_frame_title`) shows `· ? help · S save · Ctrl-C quit` **only while running**, outside the game grid.

---

## 5. Launcher And In-Game Help UI [VOLATILE]

- `draw_launcher`: ASCII `NETHACK` logo, a one-line blurb, `saves`/`bones`/`style` stat lines, a Launch action line (`Enter` when enabled, "Currently unavailable" in red when disabled), an "Once Inside" hint block (`? or F1`, `S`, `Ctrl-C`), and the `nethack.org` URL.
- **No late.sh-authored cheat sheet.** In-game help is NetHack's own `?` (and `F1`, remapped to `?`). A hand-maintained keybinding card was removed deliberately; do not reintroduce one — point at `?`. (The `hjkl` movement hint was likewise dropped — the game teaches its own controls.)
- The app frame title shows a dimmed "by nethack.org" credit on this screen, plus the in-game leave/help-key hint while running.

---

## 6. Configuration And Deploy [VOLATILE]

### Client config (env → `Config` → `SessionConfig` → `App`)
- Client enabled/host/port are profile literals in `late-ssh/src/config.rs` (dev `service-nethack`, prod `late-nethack-sv`, port 2323).
- `LATE_NETHACK_SECRET`: the only env the client reads; shared secret, **must equal the host's**.

### Host config (`late-nethack` env)
- `LATE_NETHACK_SECRET` (required), `LATE_NETHACK_BIN` (default `/usr/games/nethack`), `LATE_NETHACK_DATA_DIR` (default `/var/lib/late-nethack`, the child `HOME`), `LATE_NETHACK_VAR_DIR` (default `/var/games/nethack-var`; the compiled-in VAR_PLAYGROUND the stats session tails `xlogfile`/`livelog` from — must match the Dockerfile path and the PVC mount), `LATE_NETHACK_LISTEN_ADDR` (default `0.0.0.0`), `LATE_NETHACK_PORT` (default `2323`), `LATE_NETHACK_IDLE_TIMEOUT`. (The SSH host key is generated fresh each start, like late-dcss; there is no key-path knob.)

### Binary sourcing — **built from verified upstream source, NetHack 5.0.0** (unchanged by the extraction)
- Compiled in the `nethack-build` stage (`docker/doors/nethack.Dockerfile`, published as the ghcr `door-nethack` image the root Dockerfile pins) (NOT the distro `nethack-console`, which lags). The stage downloads the pinned tarball, verifies SHA-256 (`sha256sum -c`, fail-closed), then runs the canonical 5.0.0 unix build per `sys/unix/NewInstall.unx`. Version/URL/checksum are `ARG`s.
- The binary installs into HACKDIR `/var/games/nethack` and self-locates via compiled-in `-DHACKDIR`. We deliberately do **NOT** set `NETHACKDIR`.
- **Writable state split via `VAR_PLAYGROUND=/var/games/nethack-var`** (defined in `include/unixconf.h`, `VARDIR` passed to `make install`). NetHack never `mkdir`s `save/`, so the writable dir must be pre-seeded.
- The `nethack-build` stage also: **removes the `SHELL`/`SUSPEND` defines** (no in-game shell/suspend escape; fail-closed grep) and **`chmod 0644` on `sysconf`** (it installs `0600 root`; the host runs as unprivileged `late` and must read it, §9).
- **Keep NetHack's SIGHUP hangup-save intact.** The graceful teardown (§3/§4) depends on it: `SAFERHANGUP` must stay defined in `unixconf.h` (it defers the hangup to a safe point rather than saving from inside the signal handler). The `nethack-build` stage asserts this **fail-closed** (a sed re-enables a single-line-commented form, then `grep -qE '^#define SAFERHANGUP\b'`), so a version bump that flips the default breaks the build instead of silently regressing the lock-release. `#define SAFERHANGUP` is on by default in 5.0.0 (verified against the pinned tarball).
- Lua: `make fetch-Lua` fetches over the network but verifies against `submodules/CHKSUMS` inside the already-verified tarball.
- **Log-pipe build knobs (Phase 2).** `XLOGFILE` ships defined in 5.0.0's config.h and `LIVELOG` rides the `NHL_SANDBOX`+`CHRONICLE` chain (config.h force-defines it for `--loglua`); all three are asserted fail-closed so a version bump that drops any of them breaks the build instead of silently starving the boards. Runtime: sysconf gets `LIVELOG=0x0002` appended (LL_ACHIEVE only — `sysopt.livelog` defaults to LL_NONE, so without the mask the compiled-in livelog never writes) and `EXPLORERS=` blanked (shipped `EXPLORERS=*` would let any player enter non-scoring explore mode, whose livelog lines are indistinguishable from real ones; xlogfile lines are flagged and filtered, livelog has no flag). Both sysconf edits are grep-asserted. This change bumped the `door-nethack` image tag to `5.0.0-r2` (root Dockerfile + `nethack.yml`).
- **Windowports: tty (default) + curses (per-player opt-in), tag `5.0.0-r3`.** The build passes `WANT_WIN_TTY=1 WANT_WIN_CURSES=1 WANT_DEFAULT=tty` to both `make` invocations. All three flags are required together: `WANT_WIN_CURSES=1` alone would stop the hints fallback (`multiw-2.500`) from defining `WANT_WIN_TTY`, silently dropping the tty port and flipping everyone's default to curses. `WANT_DEFAULT` bakes in as `DEFAULT_WINDOW_SYS` (linux.500). Asserted fail-closed three ways: `nm` on the built binary must show both `tty_procs` and `curses_procs` (build stage); the smoke script `strings -n 3`-checks the exact windowport name literals in the installed binary (`-n 3` matters: default `strings` never emits the 3-char `tty`); and the smoke script proves the default launch lands on tty by comparing a no-options run's failure output against forced `windowtype:tty` (must match) and forced `windowtype:curses` (must differ), which is the only assert covering `WANT_DEFAULT`. The probes run as an unprivileged uid with a tmpfs playground and leave `NETHACKOPTIONS` unset for the default run: an unwritable `record` or an empty `NETHACKOPTIONS` makes NetHack warn and block on `Hit return to continue:` before any windowport is up, so every probe would time out with identical output and the assert would go red (this is what kept `5.0.0-r3` from publishing on 2026-08-24). Why: the tty windowport (`tgetch` = `getchar`, `unixconf.h`) decodes no escape sequences, so arrow keys cannot work in it, on any server, in any cursor mode; the curses windowport goes through `keypad()`/terminfo and maps arrows to `hjkl`/numpad. Players opt in by putting `OPTIONS=windowtype:curses` (first non-comment line, per Guidebook: it cannot be set with `O`) in the rc paste box (§1). Curses rendering through our vt100 blit is lightly proven; if it misbehaves, the opt-in nature means tty players are untouched.

### Images (Dockerfile)
- The nethack binary/playground + `libncursesw6` + **`ncurses-term`** now live **only in the `runtime-nethack` stage** (and `dev-nethack` for compose, via the `base` stage). They were removed from `runtime-base`/`runtime-ssh` — `service-ssh` no longer ships the game, only the client.
- `runtime-nethack` `COPY`s both `/var/games/nethack` (data + binary) and `/var/games/nethack-var` (writable seed), symlinks `/usr/games/nethack`, `chown`s the writable dir to `late`, and runs as `late`. `builder` builds `late-nethack` (no `otel` feature; it has a no-op `otel` feature only so workspace-wide `--features otel` stays valid).

### Prod (Kubernetes / terraform)
- `infra/doors.tf` (`nethack` entry, stamped out by the `infra/door` module): the `late-nethack` Deployment (replicas **1**, runtime-nethack image, `nethack-save` PVC mounted at `VAR_PLAYGROUND`, `nethack-save-seed` initContainer, `RUST_LOG`/`LATE_NETHACK_SECRET` env) + `late-nethack-sv` ClusterIP Service on 2323. **Deployed unconditionally** (the enable flag gates only the client).
- The rollout is **kill-before-create** (`maxSurge=0`/`maxUnavailable=1`) with `terminationGracePeriodSeconds=30`, so the old pod SIGHUP-saves its live games and exits before the new pod starts. The `nethack-save-seed` initContainer also **sweeps orphaned `?lock.*` files at boot** (`rm -f $VAR_PLAYGROUND/?lock.*`), mopping up slots leaked by hard SIGKILLs (OOM, node loss, the save backstop). The sweep is safe **only** because kill-before-create guarantees no second pod co-mounts the RWO volume, so every lock present at boot is provably stale; it never touches `save/*.gz`.
- The RWO `nethack-save` PVC (`local-path`, 2Gi, `prevent_destroy`) lives in the same doors.tf entry. The PVC + seed initContainer **moved here from `service-ssh`**; `service-ssh.tf` now only injects `LATE_NETHACK_SECRET` (host/port live in the config.rs prod profile).
- `nethack-identity-secret` (random 64-char, from the door module) is injected into **both** service-ssh and late-nethack so they derive the same key.
- `replicas` must stay 1 (one RWO volume holds shared bones + per-player saves; assumes the single-node `local-path` cluster). The rollout is kill-before-create (see the strategy note above), so the old pod terminates before the new one mounts the volume; there is no RWO co-mount during a redeploy, which is also what makes the boot lock-sweep safe.
- CI: `-nethack` releases route through `release.yml` to the generic `deploy_service.yml`: `ci` + `build`, then `kubectl set image deployment/late-nethack '*=<image>'` (covers the `nethack-save-seed` init container and the main container) and a rollout wait. No terraform runs on ordinary releases, so nothing else in the cluster is touched; when the `late-nethack` deployment is missing the run auto-bootstraps (a terraform apply `-target=module.door["nethack"]`). Other manifest changes go through `deploy_infra.yml`; terraform never reads or rewrites live image tags (deployments carry `ignore_changes` on images), so an ordinary release never rebuilds or restarts the door. Same rule as every other door. `doors.yml` build-validates `docker/doors/nethack.Dockerfile` via `docker/doors/smoke/nethack.sh` and publishes the pinned `door-nethack` image on main pushes at the tag pinned in the root Dockerfile. License/source obligations tracked in `NOTICE` (NGPL).

---

## 7. Critical Invariants [STABLE]

- The child process (on the host) is authoritative for game state. late.sh owns only the terminal bytes (vt100) and a status flag; it stores no NetHack game state — no save, level, inventory, or position. Runs, milestones, badges, and death/win feed events are landed by the log pipe (`app/door/ingest/`) from the host-side xlogfile/livelog, the spoof-proof source the old screen scrape could never be. The exact field strings (`death=ascended`, the livelog Amulet message, the `achieve`/`flags` bit positions) are pinned against the 5.0.0 source in `ingest/nethack.rs`; re-verify on any version bump.
- Non-scoring games must never reach boards, badges, or the feed: the parser flags wizard/explore xlogfile lines (`flags` 0x1/0x2) and the service skips them wholesale, and explore mode itself is compiled out of reach via the blanked sysconf `EXPLORERS` (livelog lines carry no mode flag, so filtering alone cannot protect the at-pickup Amulet grant).
- While Running, do not route NetHack bytes through the normal late.sh input pipeline. Only `F1` is late.sh's (it injects NetHack's `?`); everything else is forwarded raw.
- Keep mouse/paste stripping in client `forward_input`. With `?1003h` mouse tracking on, unfiltered motion reports cancel NetHack menus.
- **Cursor keys are retyped into the mode the game asked for, never forwarded verbatim** (`keys_for_game` → `app/door/keys.rs`, shared with brogue/dcss; the full invariant write-up lives in the DCSS CONTEXT §4). The curses windowport's `keypad(stdscr, TRUE)` emits `smkx`, which dies in our vt100 parser; without the rewrite the player's terminal keeps sending CSI arrows that ncurses under any modern TERM cannot decode. Read the live mode off the parser (`Screen::application_cursor`), never a mode the door remembers itself.
- **The docker build must keep all three windowport flags together** (`WANT_WIN_TTY=1 WANT_WIN_CURSES=1 WANT_DEFAULT=tty`, §6): dropping any one silently changes which ports exist or which one every player gets. The `nm` and smoke-script asserts are the fail-closed guard.
- Force `ProxyStatus::Closed` and wake the render loop the instant the connection closes, before cleanup, or the screen freezes on the last frame.
- **Auth: compare the key DATA, not the whole `PublicKey`.** `ssh_key::PublicKey`'s `PartialEq` includes the comment field; a key arriving over the wire has no comment while the host's locally-derived `authorized_key` does, so a whole-struct comparison rejects every connection. `auth_publickey` compares `key.key_data()`. (This bit us once.)
- **`derive_client_key` must stay byte-identical across the two crates** (same `KEY_DOMAIN`, same blake3 steps). Drift → client derives a different key → host rejects everything. A known-answer test in both crates pins the fingerprint (§8).
- The `-u` name is the account's arcade handle: user-chosen but **immutable once claimed** and unique case-insensitively (`late_core::models::arcade_handle`), so renames still cannot orphan a save and stripped-username collisions cannot happen. Travels as the SSH username; the host re-sanitizes (`playname::sanitize`, keeps `_`) before `-u`. `HANDLE_MAX_LEN` 20 stays within `PL_NSIZ`; `late`/`late_*` are reserved so nobody can claim the orphaned legacy hash saves.
- Spawn the child with `env_clear()` + an explicit allowlist. Even though the host is dedicated, keep its env minimal. NetHack's shell/suspend escapes are compiled out in `nethack-build`.
- Keep XON/XOFF flow control **off** on the host PTY, or a stray Ctrl-S freezes output until Ctrl-Q (§9).
- On host child exit, close the channel first, then **detach** the reader thread — never join it (the save-compressor gotcha, §9).
- **On teardown while the child is still live** (client disconnect or host SIGTERM), SIGHUP the child for its hangup-save **before** any SIGKILL, so NetHack releases its getlock slot. A SIGKILL-while-live orphans the slot; once all `MAXPLAYERS` (25, set in the `nethack-build` stage (`docker/doors/nethack.Dockerfile`); 25 is NetHack's hard cap) slots are orphaned, `getlock()` fails `Too many hacks running now` and the door is dead for everyone (§9). `PtyHost::Drop` must not abort the bridge task (that path SIGKILLs), and the host SIGTERM grace (`SHUTDOWN_GRACE`) must outlast the per-child `HANGUP_SAVE_GRACE`.
- The host child must run as `late` and be able to **read** HACKDIR (esp. `sysconf`, §9) and **write** `VAR_PLAYGROUND`.
- Treat all exits identically — clean save, death, quit, crash, network drop all return to the launcher.
- When disabled, fail soft (launcher message + no-op connect), never panic.
- `mod.rs` stays declaration-only.

---

## 8. Tests And Verification [STABLE]

Root policy applies: agents should not run `cargo test`/`nextest`/`clippy` as blocking verification; mention the focused command in handoff.

Inline pure tests cover:
- Client `state.rs`: launcher inert when disabled; `late-core` `models::arcade_handle` covers the handle shape/reserved rules.
- Client `identity.rs` / host `late-nethack/identity.rs`: derivation determinism + a **known-answer fingerprint** (`late-nethack-kat-v1` → `SHA256:JA9AvdNoX1ZZMA43t1qMUzq73OW609Fme6rrle84UeU`) — the cross-crate drift guard.
- Client `state.rs`: `connect` no-op when disabled; `forward_input` without a proxy is a no-op; `strip_input_noise` drops mouse/paste but keeps keys/arrows; cursor keys follow the game in and out of application cursor mode (`keys_for_game` + `feed_for_test`); F1 (both encodings) consumed; exit-grace opens on close and counts down; idle shutdown closes a stale running game.
- The log pipe's tests live with the ingestion slice: `app/door/ingest/nethack_test.rs` (pure parsers against realistic 5.0.0 lines incl. cheat-mode flags, the amulet achieve bit, and hostile cases) and the NetHack cases in `ingest/svc_test.rs` (DB-backed: replay idempotency, cheat-mode skip, ascension granting both badges, livelog Amulet paying per run behind the lockout). Host `stats.rs`: cursor parsing + line framing (tab-preserving).
- `late-core` `profile_award.rs`: `NHA`/`NHY` badge codes + chips score formatting. `late-core` `user.rs`: chat author label collapses `NHA` into `NHY`.
- Host `playname.rs`: sanitize keeps alphanumerics + `_`, strips metachars, caps length, falls back when empty.
- Host `server.rs`: `effective_term` falls back for unknown/hostile TERM and passes a supported one through.
- `app/common/primitives.rs` + `app/input.rs`: screen `next`/`prev` ordering; NetHack is a door screen reached through the Games hub, absent from the tab cycle.

The PTY bridge (`host.rs`) and the russh client/server loops are process/network-bound and not unit-tested; verify launch/save/quit manually against a real host.

Focused commands for human verification:

```bash
cargo test -p late-nethack && cargo test -p late-ssh nethack
```

(Don't fold these into one `-p late-nethack -p late-ssh nethack` — the `nethack` name filter would also apply to the host crate and skip its tests.)

---

## 9. Known Gotchas [VOLATILE]

### Client-side
- **Trailing game keys can quit the whole app (exit-grace).** NetHack's end-of-game disclosure (`--More--`, `[ynq]`, …) makes players mash `q`/space; the game exits mid-burst and the remaining keys land on the launcher, where `q` is the **global** app-quit (drops the SSH session and any paired CLI). Guard: on close, `State::tick` opens `EXIT_GRACE_TICKS` (~0.66s at the 66ms world tick); while `in_exit_grace()`, `App::handle_input` swallows launcher input. `connect` resets it. Re-check if you change the launcher's global-key fall-through or the tick rate.

### Host-side (`late-nethack`)
- **A save-time compressor holds the PTY open after NetHack exits.** On `S`+`y` nethack exits in ~10ms but hands the save to an external compressor that *inherits the PTY slave* and can run for seconds (worse on slow storage). The PTY doesn't hit EOF until that grandchild dies. Guard: the teardown **detaches** the reader (no join) — the channel is already closed, so the session ends now; a blocking `reader.join()` would pin a runtime worker and stall. Do not "tidy up" by joining it.
- **Ctrl-S freezes the game (XON/XOFF).** A stray Ctrl-S is XOFF: the line discipline pauses child output until XON (Ctrl-Q). Guard: `run_bridge` clears `IXON`/`IXOFF`/`IXANY` on the slave termios **before exec**.
- **`sysconf` perms (works-in-dev, fails-in-prod #1).** `make install` writes HACKDIR `sysconf` as `0600 root`. Dev (`dev-nethack`) runs as **root** and reads it fine; the prod host runs as **`late`** → `EACCES` → nethack aborts `Unable to open SYSCF_FILE.` the instant it starts (looks like "starts then drops back to launcher"). Guard: `nethack-build` `chmod 0644 sysconf` with a `stat`-mode build assertion.
- **TERM / terminfo (works-in-dev, fails-in-prod #2).** nethack's ncurses aborts `Unknown terminal type` for a TERM with no terminfo on the host. The slim image lacks alacritty/kitty/wezterm/**ghostty** (`xterm-ghostty`); terminals that ship their own terminfo are never in `ncurses-term`. Guard: `effective_term` falls back unknown TERM → `xterm-256color` (renders on every modern terminal), and `ncurses-term` is installed for native coverage of the rest. Symptom if reintroduced: a specific client terminal blinks "Starting nethack..." then returns to the launcher while others work.
- **`NETHACKDIR` must stay unset**; overriding it to an empty dir breaks the child's chdir.

### Operational
- **getlock-slot wedge (root-caused + fixed 2026-06-28).** NetHack's `getlock()` walks `MAXPLAYERS` (25, NetHack's hard cap) slots, each a single-character-prefixed lock file `<c>lock.*` in `VAR_PLAYGROUND` (the prefix runs `a`, `b`, `c`, … one per slot — it was `alock.*`..`jlock.*` back when `MAXPLAYERS` was the stock 10); if every slot already holds a (stale) lock it bails `Too many hacks running now` and exits instantly, so **no one** can start a game. The boot sweep and the emergency command below both glob `?lock.*` (any single-char prefix) **on purpose, not a fixed `[a-j]lock.*` range**, so they stay correct as `MAXPLAYERS` changes. Original cause: the host SIGKILLed the child on every client disconnect, and a service-ssh rollout disconnects all sessions at once, so each rollout with a live game leaked one slot. After enough rollouts (the door's `MAXPLAYERS` was 10 at the time) it was wedged prod-wide. Fixed by (a) the SIGHUP-save teardown so a disconnect releases the slot (§3/§4/§7), and (b) the initContainer boot sweep for slots leaked by genuine hard kills (§6). **Emergency unwedge** (if it ever recurs): `kubectl exec deploy/late-nethack -- sh -c 'rm -f $VAR_PLAYGROUND/?lock.*'` (these are dead games; real saves are `save/*.gz` and are untouched). No pod roll needed; `getlock()` re-walks the freed slots on the next launch.
- No late.sh persistence layer: durable state is on the host's disk/PVC. Save recovery after a dropped session depends on NetHack's own save/recover.
- HACKDIR (`/var/games/nethack`) is a read-only image layer, refreshed on rebuild; writable state (`/var/games/nethack-var`) is the `nethack-save` PVC (prod) or the image's baked seed (dev/unmounted, ephemeral).
- Multiple concurrent sessions for the same account share one `-u` save name; NetHack's own save lock makes a second concurrent launch refuse to load. Not specially handled.
- **Process-count envelope.** Each SSH connection to the host spawns at most one child; concurrent connections are bounded by late-ssh's `conn_limit`/per-IP caps (NetHack children are 1:1 with door sessions). The host pod's CPU/memory limits are the backstop. There is deliberately **no** separate per-user/global cap; add one in `connect`/the host if the envelope gets too loose.
- Binary built from verified upstream source (5.0.0); not fully hermetic (fetches Lua). When bumping versions, update the `NETHACK_*` Dockerfile `ARG`s (incl. `NETHACK_SHA256`) and `NOTICE`.

### Possible future work
- Dedup `derive_client_key`/`KEY_DOMAIN` into a shared crate (currently duplicated in both, guarded only by the KAT). Deferred to avoid pulling `russh`/`blake3` into a lower-level crate for ~15 lines.
- Per-user/global concurrency cap on the host pod if needed.
- Real OTLP telemetry in `late-nethack` (today the `otel` feature is a no-op).
