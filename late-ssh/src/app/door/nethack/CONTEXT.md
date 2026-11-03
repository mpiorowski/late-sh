# NetHack Door Context

## Metadata
- Scope: the NetHack door as a whole — the **client** in `late-ssh/src/app/door/nethack` (+ the screen lifecycle in `late-ssh/src/app`: state/input/render/tick wiring) **and the standalone host crate `late-nethack/`**. There is no separate `late-nethack/CONTEXT.md`; this file is the single source for both halves.
- Domain: NetHack, the real upstream roguelike, run on a PTY inside a **dedicated `late-nethack` SSH host** and reached by late-ssh as a network-proxied door (the *Rebels* camp).
- Primary audience: LLM agents changing the NetHack launcher UI, the SSH client transport, the host crate (PTY bridge / auth / TERM handling), input forwarding, or its config/deploy wiring.
- Status: Active
- Parent context: `../CONTEXT.md` (the door family)
- Stability note: Sections marked `[STABLE]` should change rarely. Sections marked `[VOLATILE]` are expected to change when the launcher UI, keybindings, or deploy wiring change.

---

## 0. Context Maintenance Protocol [STABLE]

Read this file after `../CONTEXT.md` whenever a task touches the NetHack launcher, launch/leave behavior, the SSH client transport, the `late-nethack` host (PTY bridge, auth, TERM resolution), input forwarding/filtering, the F1→`?` help remap, or NetHack config/deploy wiring.

- Keep this file aligned with the SSH transport contract, the client/host split, input-filter behavior, config knobs, and known gotchas.
- Update `../CONTEXT.md` when the hub roster or the door family changes, and `late-ssh/CONTEXT.md` for global keys.
- Treat tests and code as authoritative when comments drift. Patch stale comments or this file before handoff.
- Do not add `pub use` re-export layers; `mod.rs` should stay declaration-only.

---

## 1. Summary [STABLE]

NetHack runs the **real upstream NetHack binary on a PTY**, but **not** inside late-ssh. It lives in its own crate/pod, `late-nethack`, a minimal russh **server** that spawns one `nethack` child per SSH session. late-ssh reaches it exactly like the Rebels door reaches a remote SSH server: the door is a russh **client** that streams the remote terminal through a `vt100::Parser` and blits it into a ratatui widget below the top bar. SSH *is* the transport — there is no custom IPC.

(It runs in its own pod rather than as a child of `service-ssh` for a real secret boundary, independent resource limits and an isolated blast radius; a PTY cannot cross containers, so it is a network door.)

Core shape:
- `Screen::Nethack` has no top-level number key. It is reached by selecting the NetHack card in the Games hub (page `3`) and pressing `Enter`.
- `Enter` on the selected NetHack card opens the SSH connection to the host and switches to Running mode in one step (the standalone launcher render is normally skipped).
- One per-session `NethackProcess` (a russh client; the twin of `door::rebels::proxy::RebelsProxy`) owns a background Tokio task that connects to `late-nethack`, requests a PTY + shell, and bridges the remote bytes into a shared `vt100::Parser`. The foreground reads that screen and a `ProxyStatus` flag.
- **Identity vs authorization are split.** The connection authenticates with a single Ed25519 key both ends derive from `LATE_NETHACK_SECRET` (authorization). The account's **arcade handle** (user-chosen, immutable, claimed once via the shared one-time claim modal over the launcher (`landing::draw_name_modal`); see the DCSS door's CONTEXT §invariants and `door/arcade.rs`) travels as the **SSH username** (identity) and becomes `-u`; the host re-sanitizes it. Saves under derived `late_<hex>` playnames on the PVC are orphans (the `late`/`late_*` shapes are reserved, so no handle reaches them) and can be deleted from the playground.
- While Running, raw client bytes are forwarded straight to the host→child (minus mouse/paste noise), so NetHack — not late.sh — interprets keys. `F1` is the only key late.sh keeps, and it is merely **remapped to NetHack's own `?` help**.
- Per-player saves come from `-u <playname>` against the host's shared playground (a PVC), so deaths seed common **bones** across users. Game state has **no late.sh-side persistence**; saves/bones live on the host's disk/PVC.
- **The log pipe (boards, badges, feed events — no screen scraping).** NetHack appends two machine-readable files into `VAR_PLAYGROUND` on the host's PVC: `xlogfile` (one TAB-separated `key=value` line per finished game; XLOGFILE ships defined in 5.0.0, asserted fail-closed in the Dockerfile) and `livelog` (live mid-run achievement lines; the LIVELOG compile chain rides `NHL_SANDBOX`+`CHRONICLE`, and the sysconf `LIVELOG=0x0002` mask turns writing on — both handled in the Dockerfile). The host serves them over the reserved `late_stats` SSH username (`late-nethack/src/stats.rs`, the late-dcss twin): the client pushes per-file byte offsets in env requests (`LATE_DOOR_STATS_CURSORS`, split across several requests when large), the host streams `<file-id>\t<offset>\t<line>` frames with tail -f semantics and stays stateless. late-ssh's ingestion task (`app/door/ingest/`: `nethack.rs` pure parsers verified against the pinned 5.0.0 source, `svc.rs` orchestration spawned in main.rs behind `LATE_NETHACK_ENABLED`, cursors in `door_log_cursors`) lands one `door_runs` row per finished game (`death=ascended` → win, `quit`/`escaped` → quit/leaving, depth = `maxlvl`) and a `door_milestones` row for the Amulet pickup (livelog message "acquired The Amulet of Yendor"), idempotent via unique `(game, source_file, source_offset)`. From this pipe: the NetHack board triple on the Leaderboards page (Wins all-time, Deepest Dive, Top Score), the badge pair `NHA` (Amulet, 20k chips, granted at pickup from the livelog stream, the only Amulet source: the xlogfile `achieve` bit is not read for it) and `NHY` (ascension, 40k, granting only itself: a win line never back-grants the pickup, so a pickup the livelog missed is an ingest bug that shows as a missing badge, never a second payout) — the badge is once per account, the chips repeat once per ingested run behind a 30-day per-milestone lockout (migrations 158 and 208), and #lounge feed events for deaths (with `deathlev` + NetHack's own death text) and wins (freshness+recency gated so backfill never floods the feed). Wizard/explore games still write flagged xlogfile lines (`flags` bits 0x1/0x2) and are skipped wholesale; explore mode itself is locked off via a blanked sysconf `EXPLORERS` because livelog lines carry no such flag. There are no mid-run "descended to level N" feed lines, by owner decision. "Started a NetHack game" is connect-based in the client (`state.rs::connect`, via a plain `ActivityPublisher`). Nothing is read off the screen, and the door keeps no late.sh-side persistence of its own.
- **Live spectating.** Anyone can watch a running game read-only from the Games hub rail or the live strip, and the player opens their watchers' chat over the game with F4, where it takes every key until F4 or Esc; shared with DCSS and Brogue, see `../spectate/CONTEXT.md`. NetHack's part is its host's `watch.rs` and the NetHack screen drawing that chat beside the game, or over it where the game would lose its 80 columns.

- **Per-account `.nethackrc`.** The account's rc lives in Postgres (`door_rcs` table, `late-core/src/models/door_rc.rs`; 16KB cap, no NULs), authored via a paste box on the Games hub: `c` on the NetHack card (or its landing page, which bounces to the hub) opens the modal, a bracketed paste replaces the whole file, `x` clears back to defaults (`app/door/rc.rs` holds the `DoorRcService` + paste sanitizer, `app/door/hub/ui.rs` the modal). At every launch the client pushes the content base64-encoded as one SSH env request (`LATE_DOOR_RC_B64`, `want_reply=false`, sent before `request_pty`); the host decodes it (`late-nethack/src/rc.rs`: base64 → cap → UTF-8 → no NUL), materializes `<data_dir>/rc/<playname>.nethackrc` (`host.rs::materialize_rc`), and sets `NETHACKOPTIONS=@<path>` so it overrides the shared `$HOME/.nethackrc`. An empty push deletes the file; no push at all (an older client) leaves whatever is on disk. All filesystem errors fail soft (warn + launch with defaults). The env var name is duplicated client/host like the identity derivation; keep the copies in sync. DCSS shares the same contract (`-rc` flag instead of NETHACKOPTIONS; `macro_dir` is compiled out of its build, so a pushed rc cannot redirect macros, see the DCSS `DEPLOY.md`); Brogue is already per-player upstream and has no paste box. Why the unfiltered push is safe on this build (re-verify on version bumps) is in `DEPLOY.md`.

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
| `server.rs` | russh `Server`/`ClientHandler`: `auth_publickey` (compares the derived key — see §7), `pty_request`, `env_request` (the pushed rc, the stats cursors, the watch request), `shell_request`, `data`, `window_change_request`, `channel_eof/close`. Holds `effective_term` (TERM fallback, §4) and the `SessionHost` split: a game `PtyHost` for players, a `StatsHost` for the reserved `late_stats` username, a `WatchHost` for the reserved `late_watch` username. |
| `host.rs` | `PtyHost`: the per-session PTY bridge. `openpty` + `env_clear` + `setsid`/`TIOCSCTTY` + `IXON/IXOFF/IXANY` clear + `TIOCSWINSZ` + the **detached** reader. Output flows to the SSH channel handle; client bytes flow to the PTY master. |
| `identity.rs` | `derive_client_key(secret)` — identical to the client copy (KAT-pinned). |
| `playname.rs` | `sanitize(username)`: keep `[A-Za-z0-9_]`, cap at `PL_NSIZ`, fall back to `late`. Defense-in-depth on the `-u` arg. |
| `rc.rs` | `decode_rc(env value)`: base64 → size cap → UTF-8 → no-NUL validation of the pushed per-account rc. Constants mirror the client (`RC_ENV_VAR`, `MAX_RC_BYTES`). |
| `stats.rs` | The `late_stats` log-streaming session: cursor env parsing, complete-line framing (`frame_lines`), and the stateless tail loop over `xlogfile`/`livelog` in `var_dir`. Constants mirrored in late-ssh's `app/door/ingest/stream.rs`; the late-dcss twin. |
| `watch.rs` | Live spectating (`../spectate/CONTEXT.md`): the `LiveRegistry` of live games (host-side `vt100` mirror + pulse per game, watcher counts), the `late_watch` sessions, and `hud_status` (`Xp3 Dlvl:4` off the bottom status line). The late-dcss twin; `host.rs` feeds it every output chunk and window change. |

Cross-module wiring (client side, outside this folder):
- `app/state.rs`: `App::nethack_state`, `enter_nethack`/`leave_nethack`, and the Running-mode input passthrough in `App::handle_input` (intercept F1, else forward raw bytes).
- `app/input.rs`: launcher `Enter` → `enter_nethack` + `connect`; arrows are a no-op on the launcher (Running-mode arrows are forwarded upstream).
- `app/render.rs`: takes `nethack_state` out (like rebels) so the draw path can `set_viewport(content_area)` before blitting.
- `app/tick.rs`: calls `State::tick()` each app tick to detect connection close.
- `config.rs`, `state.rs` (`SessionConfig`), `ssh.rs`, `session_bootstrap.rs`, `src/test_helpers.rs`: thread the `nethack_enabled`/`nethack_host`/`nethack_port`/`nethack_secret` fields through.

---

## 3. Screen Lifecycle And Input Capture [STABLE]

- NetHack is not a top-level tab. It is launched from the Games hub (`late-ssh/src/app/door/hub`, page `3`), a selector that renders the selected door game's landing; NetHack's landing is drawn by `render::draw_landing`. `Screen::Nethack` is a live-game-only screen.
- Pressing `Enter` on the focused NetHack card in the hub calls `set_screen(Screen::Nethack)` (which runs `enter_nethack`, constructing `State`) then `State::connect`, opening the SSH connection and switching to `Mode::Running` in one step — the standalone launcher (`Mode::Launcher` render) is normally skipped.
- **Detach instead of teardown while running.** Navigating away from the screen with a live game (the ` detach below, or any screen switch) keeps `nethack_state` — and with it the SSH connection and the host child — alive: `set_screen` only drops a non-running state. The player resumes from the Games hub card (green pip + "resume your game in progress" line, Enter) or via the backtick workspace cycle, where live roguelike doors are the last stops (`app/workspace/cycle.rs`, `GameWorkspace::Door`). Teardown paths that do drop the state: the game exiting on its own (tick sees `Closed`), the **30-minute input-idle shutdown** (`IDLE_SHUTDOWN` in `state.rs`, the shared `door::game::IDLE_WINDOW`: no forwarded keystroke for 30 minutes → the proxy is dropped, so the host SIGHUP-saves; applies attached or detached, no exit grace), an off-screen state observed not running (reaped in `app/tick.rs` so the hub card stops advertising), and session end. Dropping `nethack_state` drops `NethackProcess`, whose `Drop` aborts the client bridge task → the SSH connection closes → the host's `channel_close` drops its `PtyHost`. The host bridge then **SIGHUP-saves the live child** (a recoverable save plus a getlock-slot release) before exiting, with a SIGKILL backstop for a wedged child. See §4 (Host) and the getlock wedge in `DEPLOY.md`.
- `State::tick` (each app tick) flips back to `Mode::Launcher` if the connection closed for any reason (clean `S` save, death, quit, crash, or network drop) — all exits are treated identically. `App::tick` then returns the session to the Games hub once the post-exit input grace (`in_exit_grace`) has elapsed.

Input capture contract (client side):
- The **launcher** behaves like a plain page: only `Enter` is consumed; every other key falls through to normal global handling. **Exception:** for a short post-exit grace window the launcher swallows *all* input — see the exit-grace gotcha in §9.
- While **Running**, `App::handle_input` intercepts bytes *before* the normal input pipeline: if `state.is_running()`, a whole-chunk backtick (`DOOR_DETACH_KEY`) detaches via the backtick workspace cycle (the game keeps running), else it calls `intercept_input` (F1 remap) then `forward_input` straight to the host, and returns. Number keys, `q`, `Esc`, etc. all reach NetHack; ` and F1 are the only keys late.sh keeps.
- `F1` (`ESC O P` or `ESC [ 11 ~`) is **remapped to NetHack's own `?` help**: `intercept_input` forwards a literal `?` and swallows the F1 bytes, both giving F1 the conventional meaning and stopping the raw escape from leaking as stray commands.
- `forward_input` strips mouse reports (SGR `ESC [ < … M/m`, legacy X10 `ESC [ M b x y`) and bracketed-paste markers. late.sh keeps any-event mouse tracking (`?1003h`) on for its own UI; those motion reports' leading `ESC` would otherwise cancel every NetHack menu. A sequence truncated at a chunk boundary falls through unchanged.
- After the noise strip, `keys_for_game` retypes the cursor keys into the mode the game asked for (`keys::to_application_cursor` when `Screen::application_cursor` is on), same wiring as brogue/dcss. This only matters for the curses windowport (opt-in, `DEPLOY.md`): the default tty windowport reads raw `getchar()` and decodes no escape sequences at all, so arrows there arrive as ESC + `[` + letter in either mode. That is upstream tty behavior, not a late.sh bug.

---

## 4. Transport Architecture [STABLE]

### Client (`proxy.rs`, in late-ssh) — the vt100 side

- `NethackProcess::spawn` creates an mpsc command channel, a shared `vt100::Parser` (sized to the viewport), a `ProxyStatus` mutex, and spawns the bridge task. On task end it forces `ProxyStatus::Closed` and wakes the render loop (so `tick()` returns to the launcher; without this the screen freezes on the last frame, e.g. right after `S` saves).
- `run_bridge` is a russh client (`AcceptAnyHostKey`): `client::connect` → `authenticate_publickey(username = cfg.playname (the arcade handle), key = derive_client_key(secret))` → `channel_open_session` → `request_pty` → `request_shell` → status `Running`. Then a `tokio::select!` loop: command channel (`Input` → `channel.data`; `Resize` → `window_change`) and `channel.wait()` (remote `Data`/`ExtendedData` → `parser.process` + repaint; `Eof`/`Close`/`ExitStatus` → break).
- The vt100 parser lives **client-side only**. The host streams raw bytes; only late-ssh interprets them into a screen (shared with Rebels via `rebels::render::blit_screen`).

### Host (`late-nethack`) — the PTY side

- `ClientHandler` (one per SSH connection): `auth_publickey` checks the derived key and stores the sanitized playname; `pty_request` records term/cols/rows; `shell_request` resolves the effective TERM and spawns a `PtyHost`, handing it `session.handle()` + the `ChannelId`.
- `PtyHost::spawn` → `run_bridge` (unix only): `openpty`, clear `IXON/IXOFF/IXANY` on the slave termios **before exec** (§9), build the `nethack` `Command` (`child_command`) with `env_clear()` + allowlist (`-u <playname>`, `TERM`/`HOME`/`LANG`/`LC_ALL`/`NCURSES_NO_UTF8_ACS`; the locale trio is what keeps curses line drawing out of DEC Special Graphics, `DEPLOY.md`; `LINES`/`COLUMNS` are deliberately NOT exported, since ncurses then ignores SIGWINCH and the curses windowport stays at its spawn-time size while the vt100 grid follows every resize, e.g. the watchers' chat opening), wire slave→stdio, `pre_exec` `setsid` + `TIOCSCTTY`. A blocking **reader thread** pumps PTY output to an unbounded channel; the select loop forwards those chunks to `handle.data(channel, …)`, writes client `Input` to the PTY master, applies `Resize` via `TIOCSWINSZ`, and breaks on `child.wait()`.
- On child exit: send `eof`+`close` to the channel **immediately** (so the client returns to its launcher now), then kill the child and **detach** the reader; do NOT join it (the save-compressor gotcha, §9).
- **Teardown while the game is still live** (client disconnect, e.g. a service-ssh rollout, or a host SIGTERM): the bridge classifies the stop as `StopReason::Teardown` and sends the child **SIGHUP** so NetHack runs its hangup-save (recoverable save + getlock-slot release), waits up to `HANGUP_SAVE_GRACE` (5s) for it to exit, then SIGKILLs as a backstop. `PtyHost::Drop` does not abort the bridge task; it lets `cmd_tx` close so the bridge reaches this graceful path. A pod-wide SIGTERM flips a `watch` channel (created in `main.rs`, threaded through `Server`/`Shared`) that every live bridge observes; `main.rs` holds the process up for `SHUTDOWN_GRACE` (8s) so the saves land before exit. A game that exits on its own (in-game quit/death/`S` save) is `StopReason::ChildExited` and needs no SIGHUP.
- **TERM fallback (`effective_term`).** nethack's ncurses aborts `Unknown terminal type` for any TERM the host has no terminfo entry for. `effective_term` checks the host's terminfo dirs for the client's TERM and falls back to `xterm-256color` (which every modern terminal renders) when absent — this is what makes Ghostty/kitty/wezterm clients work. `ncurses-term` in the image covers alacritty/rxvt/etc. natively. See §9.

### Sizing
- `State::set_viewport` (client, from the draw path) resizes the local parser and sends a `Resize` command; the client forwards a `window_change`, the host applies `TIOCSWINSZ`, and the kernel signals `SIGWINCH` to the child so curses redraws.

### Render
- `draw_running` blits the current `vt100` screen via `rebels::render::blit_screen`. Before the proxy reports `Running` it shows "Starting nethack...".
- The app frame title bar (`app/render.rs::app_frame_title`) shows `` · ? help · S save · ` step out · #quit `` **only while running**, outside the game grid. Quit is advertised as NetHack's own `#quit` extended command, not `Ctrl-C`: the curses windowport puts the PTY in raw mode, so `Ctrl-C` arrives as a plain key ("Unknown command '^C'"), while `#quit` works under both windowports.

---

## 5. Launcher And In-Game Help UI [VOLATILE]

- `draw_launcher`: ASCII `NETHACK` logo, a one-line blurb, `saves`/`bones`/`style` stat lines, a Launch action line (`Enter` when enabled, "Currently unavailable" in red when disabled), an "Once Inside" hint block (`? or F1`, `S`, `` ` ``, `#quit`), and the `nethack.org` URL.
- **No late.sh-authored cheat sheet.** In-game help is NetHack's own `?` (and `F1`, remapped to `?`). There is deliberately no hand-maintained keybinding card or movement hint: point at `?`; the game teaches its own controls.
- The app frame title shows a dimmed "by nethack.org" credit on this screen, plus the in-game leave/help-key hint while running.

---

## 6. Configuration And Deploy [VOLATILE]

Client knobs (profile literals in `late-ssh/src/config.rs` plus `LATE_NETHACK_SECRET`, which must equal the host's), host env, the verified 5.0.0 source build (windowports, `SAFERHANGUP`, the log-pipe knobs, the sysconf edits), the images, the Kubernetes/terraform shape (one replica, kill-before-create, the boot lock sweep) and CI are in `DEPLOY.md`.

---

## 7. Critical Invariants [STABLE]

- The child process (on the host) is authoritative for game state. late.sh owns only the terminal bytes (vt100) and a status flag; it stores no NetHack game state — no save, level, inventory, or position. Runs, milestones, badges, and death/win feed events are landed by the log pipe (`app/door/ingest/`) from the host-side xlogfile/livelog, a spoof-proof source a screen scrape could never be. The exact field strings (`death=ascended`, the livelog Amulet message, the `achieve`/`flags` bit positions) are pinned against the 5.0.0 source in `ingest/nethack.rs`; re-verify on any version bump.
- Non-scoring games must never reach boards, badges, or the feed: the parser flags wizard/explore xlogfile lines (`flags` 0x1/0x2) and the service skips them wholesale, and explore mode itself is compiled out of reach via the blanked sysconf `EXPLORERS` (livelog lines carry no mode flag, so filtering alone cannot protect the at-pickup Amulet grant).
- While Running, do not route NetHack bytes through the normal late.sh input pipeline. Only `F1` is late.sh's (it injects NetHack's `?`); everything else is forwarded raw.
- Keep mouse/paste stripping in client `forward_input`. With `?1003h` mouse tracking on, unfiltered motion reports cancel NetHack menus.
- **Cursor keys are retyped into the mode the game asked for, never forwarded verbatim** (`keys_for_game` → `app/door/keys.rs`, shared with brogue/dcss; the full invariant write-up lives in the DCSS CONTEXT §4, Critical Invariants). The curses windowport's `keypad(stdscr, TRUE)` emits `smkx`, which dies in our vt100 parser; without the rewrite the player's terminal keeps sending CSI arrows that ncurses under any modern TERM cannot decode. Read the live mode off the parser (`Screen::application_cursor`), never a mode the door remembers itself.
- **The docker build must keep all three windowport flags together** (`WANT_WIN_TTY=1 WANT_WIN_CURSES=1 WANT_DEFAULT=tty`, `DEPLOY.md`): dropping any one silently changes which ports exist or which one every player gets. The `nm` and smoke-script asserts are the fail-closed guard.
- Force `ProxyStatus::Closed` and wake the render loop the instant the connection closes, before cleanup, or the screen freezes on the last frame.
- **Auth: compare the key DATA, not the whole `PublicKey`.** `ssh_key::PublicKey`'s `PartialEq` includes the comment field; a key arriving over the wire has no comment while the host's locally-derived `authorized_key` does, so a whole-struct comparison rejects every connection. `auth_publickey` compares `key.key_data()`.
- **`derive_client_key` must stay byte-identical across the two crates** (same `KEY_DOMAIN`, same blake3 steps). Drift → client derives a different key → host rejects everything. A known-answer test in both crates pins the fingerprint (§8).
- The `-u` name is the account's arcade handle: user-chosen but **immutable once claimed** and unique case-insensitively (`late_core::models::arcade_handle`), so renames still cannot orphan a save and stripped-username collisions cannot happen. Travels as the SSH username; the host re-sanitizes (`playname::sanitize`, keeps `_`) before `-u`. `HANDLE_MAX_LEN` 20 stays within `PL_NSIZ`; `late`/`late_*` are reserved so nobody can claim the orphaned legacy hash saves.
- Spawn the child with `env_clear()` + an explicit allowlist. Even though the host is dedicated, keep its env minimal, but keep `LANG`/`LC_ALL=C.UTF-8` and `NCURSES_NO_UTF8_ACS=1` in it, or curses borders and walls render as raw DEC letters (`lqkxmj`, §4, `DEPLOY.md`). NetHack's shell/suspend escapes are compiled out in `nethack-build`.
- Keep XON/XOFF flow control **off** on the host PTY, or a stray Ctrl-S freezes output until Ctrl-Q (§9).
- On host child exit, close the channel first, then **detach** the reader thread — never join it (the save-compressor gotcha, §9).
- **On teardown while the child is still live** (client disconnect or host SIGTERM), SIGHUP the child for its hangup-save **before** any SIGKILL, so NetHack releases its getlock slot. A SIGKILL-while-live orphans the slot; once all `MAXPLAYERS` (25, set in the `nethack-build` stage (`docker/doors/nethack.Dockerfile`); 25 is NetHack's hard cap) slots are orphaned, `getlock()` fails `Too many hacks running now` and the door is dead for everyone (`DEPLOY.md`, Operations). `PtyHost::Drop` must not abort the bridge task (that path SIGKILLs), and the host SIGTERM grace (`SHUTDOWN_GRACE`) must outlast the per-child `HANGUP_SAVE_GRACE`.
- The host child must run as `late` and be able to **read** HACKDIR (esp. `sysconf`, §9) and **write** `VAR_PLAYGROUND`.
- Treat all exits identically — clean save, death, quit, crash, network drop all return to the launcher.
- When disabled, fail soft (launcher message + no-op connect), never panic.
- `mod.rs` stays declaration-only.

---

## 8. Tests And Verification [STABLE]

Agents run the focused tests through the capped runner: `make test-llm ARGS="-p late-nethack -p late-ssh -E 'package(late-nethack) or test(nethack)'"`.

Inline pure tests cover:
- Client `state.rs`: launcher inert when disabled; `late-core` `models::arcade_handle` covers the handle shape/reserved rules.
- Client `identity.rs` / host `late-nethack/src/identity.rs`: derivation determinism + a **known-answer fingerprint** (`late-nethack-kat-v1` → `SHA256:JA9AvdNoX1ZZMA43t1qMUzq73OW609Fme6rrle84UeU`) — the cross-crate drift guard.
- Client `state.rs`: `connect` no-op when disabled; `forward_input` without a proxy is a no-op; `strip_input_noise` drops mouse/paste but keeps keys/arrows; cursor keys follow the game in and out of application cursor mode (`keys_for_game` + `feed_for_test`); F1 (both encodings) consumed; exit-grace opens on close and counts down; idle shutdown closes a stale running game.
- The log pipe's tests live with the ingestion slice: `app/door/ingest/nethack_test.rs` (pure parsers against realistic 5.0.0 lines incl. cheat-mode flags, the amulet achieve bit, and hostile cases) and the NetHack cases in `ingest/svc_test.rs` (DB-backed: replay idempotency, cheat-mode skip, ascension granting both badges, livelog Amulet paying per run behind the lockout). Host `stats.rs`: cursor parsing + line framing (tab-preserving).
- `late-core` `profile_award.rs`: `NHA`/`NHY` badge codes + chips score formatting. `late-core` `user.rs`: chat author label collapses `NHA` into `NHY`.
- Host `playname.rs`: sanitize keeps alphanumerics + `_`, strips metachars, caps length, falls back when empty.
- Host `server.rs`: `effective_term` falls back for unknown/hostile TERM and passes a supported one through.
- Host `host.rs`: rc materialize/clear/keep, and `child_command` spawned on a real pty with a fake `nethack` script: a resize after spawn reaches the child's curses (`tput cols`), pinning the no-`LINES`/`COLUMNS` rule.
- `app/common/primitives.rs` + `app/input.rs`: screen `next`/`prev` ordering; NetHack is a door screen reached through the Games hub, absent from the tab cycle.

The rest of the PTY bridge (`run_bridge`) and the russh client/server loops are process/network-bound and not unit-tested; verify launch/save/quit manually against a real host.

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
- The getlock-slot wedge and its emergency unwedge, the PVC layout, concurrent same-account launches, the process-count envelope, and version bumps: `DEPLOY.md`, Operations.

### Possible future work
- Dedup `derive_client_key`/`KEY_DOMAIN` into a shared crate (currently duplicated in both, guarded only by the KAT). Deferred to avoid pulling `russh`/`blake3` into a lower-level crate for ~15 lines.
- Per-user/global concurrency cap on the host pod if needed.
- Real OTLP telemetry in `late-nethack` (today the `otel` feature is a no-op).
