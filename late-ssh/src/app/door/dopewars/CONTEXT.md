# dopewars Door Context

## Metadata
- Scope: the dopewars door as a whole — the **client** in `late-ssh/src/app/door/dopewars` (proxy/identity/state/render/mod) plus its screen lifecycle wiring in `late-ssh/src/app` (state/input/render/tick) **and the standalone host crate `late-dopewars/`**. There is no separate `late-dopewars/CONTEXT.md`; this file is the single source for both halves.
- Domain: dopewars, the real upstream curses "Drug Wars" trading game (GPLv2), run on a PTY inside a **dedicated `late-dopewars` SSH host** and reached by late-ssh as a network-proxied door (the same model as the NetHack door).
- Primary audience: LLM agents changing the dopewars launcher UI, the SSH client transport, the host crate (PTY bridge / auth / TERM handling), input forwarding, or its config/deploy wiring.
- Last updated: 2026-07-01 (extracted from a local-PTY child of late-ssh to a standalone `late-dopewars` SSH host, matching nethack, for blast-radius isolation of the foreign C binary + a persistent shared high-score PVC).
- Status: Active
- Parent context: `../../../../../CONTEXT.md`
- Stability note: `[STABLE]` sections change rarely; `[VOLATILE]` sections change with the launcher UI, keybindings, or build/deploy wiring.

---

## 0. Context Maintenance Protocol [STABLE]

Read this after root `CONTEXT.md` whenever a task touches the dopewars launcher, launch/leave behavior, the SSH client transport, the `late-dopewars` host (PTY bridge, auth, TERM resolution), input forwarding/filtering, or dopewars config/deploy wiring.

- Keep this file aligned with the SSH transport contract, the client/host split, the spawn args, config knobs, and known gotchas.
- Update root `CONTEXT.md` when routing, the top-level screen list/tab order, or global keybindings change.
- Treat tests and code as authoritative when comments drift; patch stale comments or this file before handoff.
- Do not add `pub use` re-export layers; `mod.rs` stays declaration-only.

---

## 1. Summary [STABLE]

dopewars runs the **real upstream dopewars curses client on a PTY**, but **not** inside late-ssh. It lives in its own crate/pod, `late-dopewars`, a minimal russh **server** that spawns one `dopewars` child per SSH session. late-ssh reaches it exactly like the NetHack door reaches `late-nethack`: the door is a russh **client** that streams the remote terminal through a `vt100::Parser` and blits it into a ratatui widget below the top bar. SSH *is* the transport — there is no custom IPC.

(History: dopewars used to run as a local `openpty` child inside the `service-ssh` container. It was extracted to `late-dopewars` on 2026-07-01 to get the foreign GPLv2 C binary out of the SSH gateway container — the same **blast-radius isolation** that motivated the nethack extraction — and to give it a **persistent, shared high-score table** on a PVC. A PTY can't cross containers, so it became a network door.)

Core shape:
- `Screen::Dopewars` has no top-level number key. It is reached by selecting the dopewars card in the Games hub (page `3`, last card) and pressing `Enter`. `Enter` constructs the `State` (`enter_dopewars`) and `connect`s — opening the SSH connection and switching to `Mode::Running` — in one step; the standalone launcher render is normally skipped.
- One per-session `DopewarsProcess` (a russh client; the twin of `door::nethack::proxy::NethackProcess`) owns a background Tokio task that connects to `late-dopewars`, requests a PTY + shell, and bridges the remote bytes into a shared `vt100::Parser`. The foreground reads that screen and a `ProxyStatus` flag.
- **Auth is shared-secret-derived; there is no per-player identity.** The connection authenticates with a single Ed25519 key both ends derive from `LATE_DOPEWARS_SECRET`. Unlike nethack, dopewars single-player takes **no `-u` playname**: the SSH username carries only an opaque account-correlation label for host logs (`dopewars_session_label`), which the host does not act on. The player types their handle in-game and it lands in the shared high-score table.
- The child is spawned `dopewars -t -n -b -f <shared score file>` (text client / single-player / black-and-white / shared high-score path). See §4 for why `-b` is load-bearing.
- While Running, raw client bytes are forwarded straight to the host→child (minus mouse/paste noise) — dopewars, not late.sh, interprets keys. There is **no** key remap (no F1→help; dopewars is menu-driven). **Ctrl-C** ends the game: dopewars traps only `SIGWINCH`, so `^C` raises the default `SIGINT` through the PTY and the child dies, returning the session to the hub.
- **Persistence: the shared high-score table only.** dopewars has **no mid-game save** (upstream single-player has no savegame format), so a dropped connection or a pod restart *ends* any in-progress game — no architecture changes that. What *does* persist is the leaderboard: every session's `-f` points at **one shared `dopewars.sco` file on the host's PVC**, so scores survive restarts and are global across players (the dopewars analog of nethack's shared bones playground). There are **no milestones, chips, or awards** — deferred (see §9).

The door is gated behind `LATE_DOPEWARS_ENABLED` (default `false`); when disabled, `connect` is a no-op and the launcher shows "Currently unavailable". The host pod is deployed unconditionally (the flag gates only the client).

---

## 2. Module Map [STABLE]

### Client — `late-ssh/src/app/door/dopewars/`

| File | Responsibility |
|---|---|
| `mod.rs` | Module declarations + framing comment. Declaration-only. |
| `proxy.rs` | `DopewarsProcess`: per-session russh **client** to the host. Owns the bridge task (`run_bridge`), the shared `vt100::Parser`, the `ProxyStatus` flag, the input/resize command channel, and `dopewars_session_label`. Near-clone of `door::nethack::proxy`. |
| `identity.rs` | `derive_client_key(secret)`: the shared-secret → Ed25519 key derivation (blake3, domain `late.sh/dopewars/v1`). Must stay byte-identical to the host's copy (see the cross-crate note in the source). |
| `state.rs` | Per-session `State`: launcher/running `Mode`, connection config (host/port/secret/term/enabled), the optional `DopewarsProcess`, last viewport `Rect`, the post-exit input grace, `connect`, `set_viewport`, `forward_input`/`strip_input_noise`, and `tick` (flips back to Launcher on close). No award/milestone scraping. |
| `render.rs` | Ratatui rendering: `draw_landing`/`draw_launcher` (logo, blurb, market-ticker strip, hints) and `draw_running` which blits the live `vt100` screen via `rebels::render::blit_screen`. |

### Host — `late-dopewars/` crate (standalone binary)

| File | Responsibility |
|---|---|
| `main.rs` | Tracing init, `Config::from_env`, load/generate the SSH host key, run the russh server (`run_on_address`). Exits promptly on SIGTERM (no save to drain). |
| `config.rs` | `Config`: `bin`, `score_file` (the one shared `.sco`), `secret`, listen addr/port, idle timeout. |
| `server.rs` | russh `Server`/`ClientHandler`: `auth_publickey` (compares the derived key — see §7), `pty_request`, `shell_request`, `data`, `window_change_request`, `channel_eof/close`. Holds `effective_term` (TERM fallback, §4). No playname (dopewars uses no `-u`). |
| `host.rs` | `PtyHost`: the per-session PTY bridge. `openpty` + `env_clear` + `setsid`/`TIOCSCTTY` + `IXON/IXOFF/IXANY` clear + `TIOCSWINSZ` + the **detached** reader + a private per-session scratch `HOME`. Output flows to the SSH channel handle; client bytes flow to the PTY master. Teardown is a plain kill — dopewars has no hangup-save. |
| `identity.rs` | `derive_client_key(secret)` — identical to the client copy. |

Cross-module wiring (client side, outside this folder — the ~10 door touchpoints):
- `app/common/primitives.rs`: `Screen::Dopewars` (+ `next`/`prev` fall back to `Games`, `draw_tabs`/`page_title` label `"dopewars"`).
- `app/door/hub/state.rs`: `HubGame::Dopewars` + `ALL` (last card) + label.
- `app/door/hub/ui.rs`: `HubView.dopewars_enabled` + the landing match arm.
- `app/state.rs`: `App::dopewars_state`/`dopewars_term`/`dopewars_enabled`/`dopewars_host`/`dopewars_port`/`dopewars_secret`, `enter_dopewars`/`leave_dopewars`, `set_screen` enter/leave arms, and the Running-mode passthrough + exit-grace swallow in `App::handle_input`.
- `app/tick.rs`: `State::tick()` each app tick + return-to-`Games` once `!is_running() && !in_exit_grace()`.
- `app/render.rs`: `DrawContext.dopewars_enabled`/`dopewars_state`, take/restore `dopewars_state` (like rebels/nethack) so the draw path can `set_viewport(content_area)` before blitting, the dispatch arm, and the title-bar credit + in-game `Ctrl-C quit` hint.
- `app/input.rs`: hub launch arm (`set_screen` + `connect`, banner if disabled), dedicated-screen `Enter` launcher, and arrow/key dispatch no-ops (Running-mode bytes are forwarded raw upstream).
- `config.rs`, `state.rs` (`SessionConfig`), `ssh.rs`, `session_bootstrap.rs`, `src/test_helpers.rs`: thread `dopewars_enabled`/`dopewars_host`/`dopewars_port`/`dopewars_secret`.

---

## 3. Screen Lifecycle And Input Capture [STABLE]

- `Enter` on the selected dopewars card in the hub calls `set_screen(Screen::Dopewars)` (which runs `enter_dopewars`, constructing `State`) then `State::connect`, opening the SSH connection and switching to `Mode::Running` in one step — the standalone launcher (`Mode::Launcher` render) is normally skipped.
- Leaving the screen (`leave_dopewars`, on navigating away) drops `dopewars_state` → drops `DopewarsProcess`, whose `Drop` aborts the client bridge task → the SSH connection closes → the host's `channel_close` drops its `PtyHost`, which kills the child. There is no save to run.
- `State::tick` (each app tick) flips back to `Mode::Launcher` if the connection closed for any reason (quit, death, end-of-game, crash, or network drop) — all exits are treated identically. `App::tick` then returns the session to the Games hub once the post-exit input grace (`in_exit_grace`) has elapsed.

Input capture contract (client side):
- The **launcher** behaves like a plain page: only `Enter` is consumed; every other key falls through to normal global handling. **Exception:** for a short post-exit grace window the launcher swallows *all* input — see the exit-grace gotcha in §9.
- While **Running**, `App::handle_input` intercepts bytes *before* the normal input pipeline: if `state.is_running()`, it `forward_input`s straight to the host and returns. There is **no** key remap — dopewars is menu-driven, so number/letter keys, `q`, `Esc`, etc. all reach the game verbatim.
- `forward_input` strips mouse reports (SGR `ESC [ < … M/m`, legacy X10 `ESC [ M b x y`) and bracketed-paste markers. late.sh keeps any-event mouse tracking (`?1003h`) on for its own UI; those motion reports' leading `ESC` would otherwise leak into the curses game as stray commands. Real keys and arrow escapes pass through verbatim; a sequence truncated at a chunk boundary falls through unchanged.

---

## 4. Transport Architecture [STABLE]

### Client (`proxy.rs`, in late-ssh) — the vt100 side

- `DopewarsProcess::spawn` creates an mpsc command channel, a shared `vt100::Parser` (sized to the viewport), a `ProxyStatus` mutex, and spawns the bridge task. On task end it forces `ProxyStatus::Closed` and wakes the render loop (so `tick()` returns to the launcher; without this the screen freezes on the last frame).
- `run_bridge` is a russh client (`AcceptAnyHostKey`): `client::connect` → `authenticate_publickey(username = dopewars_session_label(user_id), key = derive_client_key(secret))` → `channel_open_session` → `request_pty` → `request_shell` → status `Running`. Then a `tokio::select!` loop: command channel (`Input` → `channel.data`; `Resize` → `window_change`) and `channel.wait()` (remote `Data`/`ExtendedData` → `parser.process` + repaint; `Eof`/`Close`/`ExitStatus` → break).
- The vt100 parser lives **client-side only**. The host streams raw bytes; only late-ssh interprets them into a screen (shared with rebels/nethack via `rebels::render::blit_screen`).

### Host (`late-dopewars`) — the PTY side

- `ClientHandler` (one per SSH connection): `auth_publickey` checks the derived key and marks the session authorized; `pty_request` records term/cols/rows; `shell_request` resolves the effective TERM and spawns a `PtyHost`, handing it `session.handle()` + the `ChannelId`.
- `PtyHost::spawn` → `run_bridge` (unix only): `openpty`, clear `IXON/IXOFF/IXANY` on the slave termios **before exec** (§9), build the `dopewars` `Command` with `env_clear()` + allowlist (`-t -n -b -f <score>`, `TERM`, a private scratch `HOME`, `LANG`/`LC_ALL=C.UTF-8`, `LINES`/`COLUMNS`), wire slave→stdio, `pre_exec` `setsid` + `TIOCSCTTY`. A blocking **reader thread** pumps PTY output to an unbounded channel; the select loop forwards those chunks to `handle.data(channel, …)`, writes client `Input` to the PTY master, applies `Resize` via `TIOCSWINSZ`, and breaks on `child.wait()`.
- **Spawn args: `-t -n -b -f <score>`.**
  - `-t` text (curses) client, `-n` single-player.
  - **`-b` (black-and-white) is load-bearing.** dopewars' default palette hard-codes a blue-on-blue window scheme that assumes a black terminal and renders nearly unreadable when embedded. Monochrome lets its colors map to `Color::Default → Reset`, so the game inherits the late.sh theme (same effect as the rebels/nethack doors). Removing `-b` brings back the unreadable panels.
  - `-f <score>` points at the **one shared high-score file** on the PVC (`LATE_DOPEWARS_SCORE_FILE`, default `/var/lib/late-dopewars/dopewars.sco`). dopewars creates it on the first write and locks it during updates, so concurrent sessions writing the same file are safe. **Do not run a setgid binary**: dopewars refuses a user `-f` under setgid (the from-source binary we ship is not setgid; see §6).
- On child exit **or** client disconnect: close the SSH channel (`eof` + `close`) first so the late-ssh client returns to its launcher now, then kill the child. There is **no** SIGHUP/hangup-save dance (dopewars has no mid-game save, so nothing to persist beyond scores already written on the game's own exit path). The per-session scratch `HOME` is removed on teardown.
- **TERM fallback (`effective_term`).** dopewars' ncursesw aborts `Unknown terminal type` for a TERM the host has no terminfo for; `effective_term` checks the host's terminfo dirs for the client's TERM and falls back to `xterm-256color` (which every modern terminal renders) when absent — this is what makes Ghostty/kitty/wezterm clients work. `ncurses-term` in the image covers alacritty/rxvt/etc. natively.

### Sizing
- `State::set_viewport` (client, from the draw path) resizes the local parser and sends a `Resize` command; the client forwards a `window_change`, the host applies `TIOCSWINSZ`, and the kernel signals `SIGWINCH` so dopewars does a full `endwin()`+`newterm()` rebuild.

### Render
- `draw_running` blits the current `vt100` screen; before `Running` it shows "Starting dopewars...". The app frame title shows a dimmed `by dopewars.sourceforge.io` credit, plus `· Ctrl-C quit` while running.

---

## 5. Launcher UI [VOLATILE]

- `draw_launcher`: ASCII `DOPEWARS` logo, a one-line blurb, a market-ticker strip (prices swinging), stat lines, a Launch action line (`Enter` when enabled, "Currently unavailable" in red when disabled), an "Once Inside" hint block (letters/`J`/`Ctrl-C`), and the `dopewars.sourceforge.io` URL.
- The app frame title shows a dimmed "by dopewars.sourceforge.io" credit on this screen, plus the in-game `Ctrl-C quit` hint while running.

---

## 6. Configuration And Deploy [VOLATILE]

### Client config (env → `Config` → `SessionConfig` → `App`)
- `LATE_DOPEWARS_ENABLED` (default `false`): when false, `connect` is a no-op and the launcher shows "Currently unavailable".
- `LATE_DOPEWARS_HOST` (default `127.0.0.1`): the host service. In compose it's `service-dopewars`; in prod the Service `late-dopewars-sv`.
- `LATE_DOPEWARS_PORT` (default `2324`).
- `LATE_DOPEWARS_SECRET`: shared secret; **must equal the host's**. Required when enabled.

### Host config (`late-dopewars` env)
- `LATE_DOPEWARS_SECRET` (required), `LATE_DOPEWARS_BIN` (default `/usr/games/dopewars`), `LATE_DOPEWARS_SCORE_FILE` (default `/var/lib/late-dopewars/dopewars.sco`, the one shared high-score file on the PVC), `LATE_DOPEWARS_LISTEN_ADDR` (default `0.0.0.0`), `LATE_DOPEWARS_PORT` (default `2324`), `LATE_DOPEWARS_IDLE_TIMEOUT`.

### Binary sourcing — **built from verified upstream source, dopewars 1.6.2**
- Compiled in the Dockerfile `dopewars-build` stage (terminal-only: `--disable-gui-client --disable-gui-server --enable-curses-client`). The stage downloads the pinned 1.6.2 SourceForge tarball, verifies SHA-256 (`sha256sum -c`, fail-closed), builds, and copies the binary to `/dopewars`. Version/URL/checksum are `ARG`s.
- **Build quirk (`make LIBS="-lncursesw"`):** dopewars' release Makefile drops `$(CURSES_LIBS)` from `dopewars_LDADD` when the GTK client is disabled, so the link fails with undefined `initscr`/`newterm`/… The curses lib is injected via the trailing `$(LIBS)` on the link line. Keep this on any version bump.
- The binary is **self-contained** (drug/location data compiled in) and **not setgid** (so the `-f` is honored). Runtime deps: `libglib2.0-0` + `libncursesw6` (+ `libcurl4`, pulled in by the optional metaserver client).

### Images (Dockerfile)
- dopewars now ships in its **own `runtime-dopewars` stage** (the late-dopewars host), copied from `dopewars-build`, alongside its runtime libs + `ncurses-term`. It was **removed from `runtime-ssh`** — `service-ssh` ships only the client. `builder` builds `late-dopewars` (no `otel` feature; it has a no-op `otel` feature only so workspace-wide `--features otel` stays valid). The from-source binary still lives in `base` for `dev-dopewars` (via `dev-base`).
- `Makefile` + `.env` thread `LATE_DOPEWARS_ENABLED=1` / `LATE_DOPEWARS_HOST=service-dopewars` / `_PORT=2324` / `_SECRET` / `_SCORE_FILE` (mirroring the nethack block).

### Prod (Kubernetes / terraform)
- `infra/service-dopewars.tf`: the `late-dopewars` Deployment (replicas **1**, runtime-dopewars image, `dopewars-save` PVC mounted at the score dir, `dopewars-score-seed` initContainer that chowns the mount to `late`, `RUST_LOG`/`LATE_DOPEWARS_SECRET`/`LATE_DOPEWARS_SCORE_FILE` env) + `late-dopewars-sv` ClusterIP Service on 2324. **Deployed unconditionally** (the enable flag gates only the client); the rollout is **kill-before-create** (`maxSurge=0`/`maxUnavailable=1`) so the old pod releases the RWO volume before the new one mounts it.
- `infra/dopewars.tf`: the RWO `dopewars-save` PVC (`local-path`, 256Mi, `prevent_destroy`) + the host/port/score-file locals.
- `infra/secrets.tf`: `dopewars-identity-secret` (random 64-char), injected into **both** service-ssh and late-dopewars so they derive the same key.
- `infra/service-ssh.tf` now only injects the client env (`LATE_DOPEWARS_HOST/PORT/SECRET`), not the binary path.
- `replicas` must stay 1 (one RWO volume holds the shared score file; assumes the single-node `local-path` cluster).
- CI: `.github/workflows/deploy_dopewars.yml` builds the `runtime-dopewars` image and applies (the bootstrap path). `deploy.yml`/`deploy_web.yml`/`deploy_infra.yml` each read the live `late-dopewars` image tag (plain `kubectl get`, no fallback) and pass it through `terraform.yml`'s required `dopewars_image_tag`. `dopewars.yml` build-validates the `dopewars-build` + `runtime-dopewars` stages. **First rollout is `deploy_dopewars.yml`** (it builds the image); a normal deploy first would fail the image lookup. License/source obligations tracked in `NOTICE` (GPLv2).

---

## 7. Critical Invariants [STABLE]

- The child process (on the host) is authoritative for game state. late.sh owns only the terminal bytes (vt100) and a status flag. It persists **nothing** on the late.sh side; the only durable state anywhere is the host's shared `.sco` high-score file on the PVC.
- While Running, do not route dopewars bytes through the normal late.sh input pipeline — forward them raw. There is no key remap; `Ctrl-C` ending the game is the intended leave path (dopewars catches no `SIGINT`).
- Keep mouse/paste stripping in client `forward_input`. With `?1003h` mouse tracking on, unfiltered motion reports' leading `ESC` would leak into the curses game as stray commands.
- Keep `-b` (monochrome) in the spawn args, or the blue-on-blue panels become unreadable embedded (§4).
- Do **not** run a setgid dopewars binary — it refuses the `-f` under setgid. The from-source binary is non-setgid; if `LATE_DOPEWARS_BIN` ever points at a distro package, `chmod g-s` it.
- Keep XON/XOFF flow control **off** on the host PTY, or a stray Ctrl-S freezes output until Ctrl-Q.
- **Auth: compare the key DATA, not the whole `PublicKey`.** `ssh_key::PublicKey`'s `PartialEq` includes the comment field; a key arriving over the wire has no comment while the host's locally-derived `authorized_key` does, so a whole-struct comparison rejects every connection. `auth_publickey` compares `key.key_data()`. (Same gotcha as the nethack host.)
- **`derive_client_key` must stay byte-identical across the two crates** (same `KEY_DOMAIN` `late.sh/dopewars/v1`, same blake3 steps). Drift → client derives a different key → host rejects everything. (The nethack crates pin this with a known-answer fingerprint test; add the same here when test-running is back on.)
- Force `ProxyStatus::Closed` and wake the render loop the instant the connection closes, before cleanup, or the screen freezes on the last frame.
- On the host, close the channel first, then **detach** the reader thread — never join it (it could pin a runtime worker if a grandchild lingers on the PTY).
- Spawn the child with `env_clear()` + an explicit allowlist (incl. a UTF-8 `LANG`/`LC_ALL` for ncursesw and a private writable `HOME`).
- Treat all exits identically — quit, end-of-game, crash, network drop all return to the hub.
- When disabled, fail soft (launcher message + no-op connect), never panic.
- `mod.rs` stays declaration-only.

---

## 8. Tests And Verification [STABLE]

Root policy applies: agents should not run `cargo test`/`nextest`/`clippy` as blocking verification; mention the focused command in handoff.

Inline pure tests cover:
- Client `proxy.rs`: `dopewars_session_label` (account-derived, safe, stable, distinct per account).
- Client `identity.rs` / host `late-dopewars/identity.rs`: derivation determinism. (A cross-crate known-answer fingerprint test — like nethack's — is TODO once test-running is re-enabled.)
- Client `state.rs`: `connect` no-op when disabled; `forward_input` without a proxy is a no-op; `strip_input_noise` drops mouse/paste but keeps keys/arrows; exit-grace opens on close and counts down.
- Host `server.rs`: `effective_term` falls back for unknown/hostile TERM and passes a supported one through.
- `app/door/hub/state.rs` + `app/common/primitives.rs`: selector ordering and screen `next`/`prev` place `Dopewars` correctly.

The PTY bridge (`host.rs`) and the russh client/server loops are process/network-bound and not unit-tested; verify launch/play/quit manually against a real host.

Focused commands for human verification:

```bash
cargo test -p late-dopewars && cargo test -p late-ssh dopewars
```

(Don't fold these into one `-p late-dopewars -p late-ssh dopewars` — the `dopewars` name filter would also apply to the host crate and skip its tests.)

---

## 9. Known Gotchas [VOLATILE]

### Client-side
- **Trailing game keys can quit the whole app (exit-grace).** dopewars' end-of-game high-score screen makes players mash keys; the game exits mid-burst and the remaining keys land on the launcher, where `q` is the **global** app-quit (drops the SSH session). Guard: on close, `State::tick` opens `EXIT_GRACE_TICKS` (~0.66s); while `in_exit_grace()`, `App::handle_input` swallows launcher input. `connect` resets it. (Same pattern as the nethack door.)

### Host-side (`late-dopewars`)
- **Curses link bug (`make LIBS="-lncursesw"`).** See §6 — the release Makefile omits `$(CURSES_LIBS)` from `dopewars_LDADD` with the GUI disabled; the build fails on undefined curses symbols without the override.
- **Unreadable colors without `-b`.** See §4/§7 — dopewars' default palette is blue-on-blue and assumes a black terminal.
- **setgid + `-f`.** A setgid dopewars binary refuses a user `-f`. Ship a non-setgid binary (the from-source build is fine).
- **Ctrl-S freeze (XON/XOFF).** Cleared on the PTY before exec, same as the nethack host.
- **TERM / terminfo.** dopewars' ncursesw aborts `Unknown terminal type` for a TERM with no terminfo on the host; `effective_term` falls back to `xterm-256color`, and `ncurses-term` covers the rest. Symptom if reintroduced: a specific client terminal blinks "Starting dopewars..." then returns to the launcher while others work.

### Operational
- **No mid-game save.** dopewars single-player has no savegame format; a pod restart or dropped connection ends any in-progress game. This is an upstream-game property, not fixable by this architecture (nethack survives restarts only because *it* has SIGHUP-save/recover; dopewars has no equivalent). Only the **high-score table** persists (the PVC).
- **Score file on the PVC.** `LATE_DOPEWARS_SCORE_FILE` is one shared file; `replicas` must stay 1 (RWO volume, single-node `local-path`). dopewars locks it during updates, so concurrent sessions are safe. Kill-before-create rollout means no two pods co-mount it.
- Binary built from verified upstream source (1.6.2); when bumping versions, update the `DOPEWARS_*` Dockerfile `ARG`s (incl. `DOPEWARS_SHA256`) and `NOTICE`.

### Possible future work
- Milestones/chips/awards by scraping the final-score screen (deferred; mirror `nethack/milestone.rs` + `award.rs` + the `scan_screen` scrape in `nethack/state.rs`).
- ~~Optional shared/competitive market: one `dopewars -S` server with `-o`/`-p` clients per session.~~ **Evaluated and rejected 2026-07-18**: concurrency is far too low (~1 player/week) for two people to ever share the market, and one shared server makes every deploy wipe all in-progress runs. Cheap to build (supervised `-S` on localhost, spawn `-t -b -o 127.0.0.1 -p 7902 -P <handle>`, thread the handle through the SSH username), so revisit only if concurrent players become a thing.
- A cross-crate known-answer fingerprint test pinning `derive_client_key` (mirror nethack's KAT), once test-running is re-enabled.
