# NetHack Door Context

## Metadata
- Scope: the NetHack door as a whole — the **client** in `late-ssh/src/app/door/nethack` (+ the screen lifecycle in `late-ssh/src/app`: state/input/render/tick wiring) **and the standalone host crate `late-nethack/`**. There is no separate `late-nethack/CONTEXT.md`; this file is the single source for both halves.
- Domain: NetHack, the real upstream roguelike, run on a PTY inside a **dedicated `late-nethack` SSH host** and reached by late-ssh as a network-proxied door (the *Rebels* camp).
- Primary audience: LLM agents changing the NetHack launcher UI, the SSH client transport, the host crate (PTY bridge / auth / TERM handling), input forwarding, or its config/deploy wiring.
- Last updated: 2026-06-25
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
- **Identity vs authorization are split.** The connection authenticates with a single Ed25519 key both ends derive from `LATE_NETHACK_SECRET` (authorization). The account-derived `-u` playname travels as the **SSH username** (identity); the host re-sanitizes it.
- While Running, raw client bytes are forwarded straight to the host→child (minus mouse/paste noise), so NetHack — not late.sh — interprets keys. `F1` is the only key late.sh keeps, and it is merely **remapped to NetHack's own `?` help**.
- Per-player saves come from `-u <playname>` against the host's shared playground (a PVC), so deaths seed common **bones** across users. **No late.sh-side persistence** — late.sh stores nothing about NetHack in its DB; saves/bones live on the host's disk/PVC.

The door is gated behind `LATE_NETHACK_ENABLED` (default `false`); when disabled, `connect` is a no-op and the launcher shows "Currently unavailable". The host pod is deployed unconditionally (the flag gates only the client).

---

## 2. Module Map [STABLE]

### Client — `late-ssh/src/app/door/nethack/`

| File | Responsibility |
|---|---|
| `mod.rs` | Module declarations + framing comment. Declaration-only. |
| `proxy.rs` | `NethackProcess`: per-session russh **client** to the host. Owns the bridge task (`run_bridge`), the shared `vt100::Parser`, the `ProxyStatus` flag, the input/resize command channel, and `nethack_playname`. Near-clone of `door::rebels::proxy`. |
| `identity.rs` | `derive_client_key(secret)`: the shared-secret → Ed25519 key derivation (blake3, domain `late.sh/nethack/v1`). Must stay byte-identical to the host's copy — a KAT pins it (§8). |
| `state.rs` | Per-session `State`: launcher/running `Mode`, connection config (host/port/secret/term/enabled), the optional `NethackProcess`, last viewport `Rect`, the post-exit input grace, and input interception/forwarding (`intercept_input` remaps F1→`?`, `forward_input`, `strip_input_noise`). |
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

Cross-module wiring (client side, outside this folder):
- `app/state.rs`: `App::nethack_state`, `enter_nethack`/`leave_nethack`, and the Running-mode input passthrough in `App::handle_input` (intercept F1, else forward raw bytes).
- `app/input.rs`: launcher `Enter` → `enter_nethack` + `connect`; `7` global screen switch; topbar hit-test columns; arrows are a no-op (Running-mode arrows are forwarded raw upstream).
- `app/render.rs`: takes `nethack_state` out (like pinstar/rebels) so the draw path can `set_viewport(content_area)` before blitting.
- `app/tick.rs`: calls `State::tick()` each app tick to detect connection close.
- `config.rs`, `state.rs` (`SessionConfig`), `ssh.rs`, `session_bootstrap.rs`, `tests/helpers/mod.rs`: thread the `nethack_enabled`/`nethack_host`/`nethack_port`/`nethack_secret` fields through.

---

## 3. Screen Lifecycle And Input Capture [STABLE]

- NetHack is no longer a top-level tab. It is launched from the Games hub (`late-ssh/src/app/door/hub`, page `3`), a selector that renders the selected door game's landing; NetHack's landing is drawn by the now-`pub` `render::draw_landing`. `Screen::Nethack` is a live-game-only screen.
- Pressing `Enter` on the focused NetHack card in the hub calls `set_screen(Screen::Nethack)` (which runs `enter_nethack`, constructing `State`) then `State::connect`, opening the SSH connection and switching to `Mode::Running` in one step — the standalone launcher (`Mode::Launcher` render) is normally skipped.
- Leaving the screen (`leave_nethack`, on navigating away) drops `nethack_state` → drops `NethackProcess`, whose `Drop` aborts the client bridge task → the SSH connection closes → the host's `channel_close` drops its `PtyHost` → `kill_on_drop` kills the child nethack.
- `State::tick` (each app tick) flips back to `Mode::Launcher` if the connection closed for any reason (clean `S` save, death, quit, crash, or network drop) — all exits are treated identically. `App::tick` then returns the session to the Games hub once the post-exit input grace (`in_exit_grace`) has elapsed.

Input capture contract (client side; unchanged by the extraction):
- The **launcher** behaves like a plain page: only `Enter` is consumed; every other key falls through to normal global handling. **Exception:** for a short post-exit grace window the launcher swallows *all* input — see the exit-grace gotcha in §9.
- While **Running**, `App::handle_input` intercepts bytes *before* the normal input pipeline: if `state.is_running()`, it calls `intercept_input` (F1 remap) then `forward_input` straight to the host, and returns. Number keys, `q`, `Esc`, etc. all reach NetHack.
- `F1` (`ESC O P` or `ESC [ 11 ~`) is **remapped to NetHack's own `?` help**: `intercept_input` forwards a literal `?` and swallows the F1 bytes, both giving F1 the conventional meaning and stopping the raw escape from leaking as stray commands.
- `forward_input` strips mouse reports (SGR `ESC [ < … M/m`, legacy X10 `ESC [ M b x y`) and bracketed-paste markers. late.sh keeps any-event mouse tracking (`?1003h`) on for its own UI; those motion reports' leading `ESC` would otherwise cancel every NetHack menu. Real keys and arrow escapes pass through verbatim; a sequence truncated at a chunk boundary falls through unchanged.

---

## 4. Transport Architecture [STABLE]

### Client (`proxy.rs`, in late-ssh) — the vt100 side

- `NethackProcess::spawn` creates an mpsc command channel, a shared `vt100::Parser` (sized to the viewport), a `ProxyStatus` mutex, and spawns the bridge task. On task end it forces `ProxyStatus::Closed` and wakes the render loop (so `tick()` returns to the launcher; without this the screen freezes on the last frame, e.g. right after `S` saves).
- `run_bridge` is a russh client (`AcceptAnyHostKey`): `client::connect` → `authenticate_publickey(username = nethack_playname(user_id), key = derive_client_key(secret))` → `channel_open_session` → `request_pty` → `request_shell` → status `Running`. Then a `tokio::select!` loop: command channel (`Input` → `channel.data`; `Resize` → `window_change`) and `channel.wait()` (remote `Data`/`ExtendedData` → `parser.process` + repaint; `Eof`/`Close`/`ExitStatus` → break).
- The vt100 parser lives **client-side only**. The host streams raw bytes; only late-ssh interprets them into a screen (shared with Rebels via `rebels::render::blit_screen`).

### Host (`late-nethack`) — the PTY side

- `ClientHandler` (one per SSH connection): `auth_publickey` checks the derived key and stores the sanitized playname; `pty_request` records term/cols/rows; `shell_request` resolves the effective TERM and spawns a `PtyHost`, handing it `session.handle()` + the `ChannelId`.
- `PtyHost::spawn` → `run_bridge` (unix only): `openpty`, clear `IXON/IXOFF/IXANY` on the slave termios **before exec** (§9), build the `nethack` `Command` with `env_clear()` + allowlist (`-u <playname>`, `TERM`/`HOME`/`LINES`/`COLUMNS`), wire slave→stdio, `pre_exec` `setsid` + `TIOCSCTTY`. A blocking **reader thread** pumps PTY output to an unbounded channel; the select loop forwards those chunks to `handle.data(channel, …)`, writes client `Input` to the PTY master, applies `Resize` via `TIOCSWINSZ`, and breaks on `child.wait()`.
- On child exit: send `eof`+`close` to the channel **immediately** (so the client returns to its launcher now), then kill the child and **detach** the reader — do NOT join it (the save-compressor gotcha, §9).
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
- `LATE_NETHACK_ENABLED` (default `false`): when false, `connect` is a no-op and the launcher shows "Currently unavailable".
- `LATE_NETHACK_HOST` (default `127.0.0.1`): the host service. In compose it's `service-nethack`; in prod the Service `late-nethack-sv`.
- `LATE_NETHACK_PORT` (default `2323`).
- `LATE_NETHACK_SECRET`: shared secret; **must equal the host's**. Required when enabled.

### Host config (`late-nethack` env)
- `LATE_NETHACK_SECRET` (required), `LATE_NETHACK_BIN` (default `/usr/games/nethack`), `LATE_NETHACK_DATA_DIR` (default `/var/lib/late-nethack`, the child `HOME`), `LATE_NETHACK_LISTEN_ADDR` (default `0.0.0.0`), `LATE_NETHACK_PORT` (default `2323`), `LATE_NETHACK_KEY_PATH` (host SSH key; in prod `/home/late/nethack_host_key`), `LATE_NETHACK_IDLE_TIMEOUT`.

### Binary sourcing — **built from verified upstream source, NetHack 5.0.0** (unchanged by the extraction)
- Compiled in the Dockerfile `nethack-build` stage (NOT the distro `nethack-console`, which lags). The stage downloads the pinned tarball, verifies SHA-256 (`sha256sum -c`, fail-closed), then runs the canonical 5.0.0 unix build per `sys/unix/NewInstall.unx`. Version/URL/checksum are `ARG`s.
- The binary installs into HACKDIR `/var/games/nethack` and self-locates via compiled-in `-DHACKDIR`. We deliberately do **NOT** set `NETHACKDIR`.
- **Writable state split via `VAR_PLAYGROUND=/var/games/nethack-var`** (defined in `include/unixconf.h`, `VARDIR` passed to `make install`). NetHack never `mkdir`s `save/`, so the writable dir must be pre-seeded.
- The `nethack-build` stage also: **removes the `SHELL`/`SUSPEND` defines** (no in-game shell/suspend escape; fail-closed grep) and **`chmod 0644` on `sysconf`** (it installs `0600 root`; the host runs as unprivileged `late` and must read it — §9).
- Lua: `make fetch-Lua` fetches over the network but verifies against `submodules/CHKSUMS` inside the already-verified tarball.

### Images (Dockerfile)
- The nethack binary/playground + `libncursesw6` + **`ncurses-term`** now live **only in the `runtime-nethack` stage** (and `dev-nethack` for compose, via the `base` stage). They were removed from `runtime-base`/`runtime-ssh` — `service-ssh` no longer ships the game, only the client.
- `runtime-nethack` `COPY`s both `/var/games/nethack` (data + binary) and `/var/games/nethack-var` (writable seed), symlinks `/usr/games/nethack`, `chown`s the writable dir to `late`, and runs as `late`. `builder` builds `late-nethack` (no `otel` feature; it has a no-op `otel` feature only so workspace-wide `--features otel` stays valid).

### Prod (Kubernetes / terraform)
- `infra/service-nethack.tf`: the `late-nethack` Deployment (replicas **1**, runtime-nethack image, `nethack-save` PVC mounted at `VAR_PLAYGROUND`, `nethack-save-seed` initContainer, `RUST_LOG`/`LATE_NETHACK_SECRET` env) + `late-nethack-sv` ClusterIP Service on 2323. **Deployed unconditionally** (the enable flag gates only the client); this keeps the image always present so the deploy workflows can read its current tag with a plain `kubectl get`.
- `infra/nethack.tf`: the RWO `nethack-save` PVC (`local-path`, 2Gi, `prevent_destroy`) + the host/port locals. The PVC + seed initContainer **moved here from `service-ssh`**; `service-ssh.tf` now only injects the client env (`LATE_NETHACK_HOST/PORT/SECRET`).
- `infra/secrets.tf`: `nethack-identity-secret` (random 64-char), injected into **both** service-ssh and late-nethack so they derive the same key.
- `replicas` must stay 1 (one RWO volume holds shared bones + per-player saves; assumes the single-node `local-path` cluster — RWO co-mount during the rolling update is fine, guarded by NetHack's own lock files).
- CI: `.github/workflows/deploy_nethack.yml` builds the `runtime-nethack` image and applies (the bootstrap path). `deploy.yml`/`deploy_web.yml`/`deploy_infra.yml` each read the live `late-nethack` image tag (plain `kubectl get`, no fallback) and pass it through `terraform.yml`'s required `nethack_image_tag`. `nethack.yml` build-validates the `nethack-build` + `runtime-nethack` stages. **First rollout is `deploy_nethack.yml`** (it builds the image); a normal deploy first would fail the image lookup. License/source obligations tracked in `NOTICE` (NGPL).

---

## 7. Critical Invariants [STABLE]

- The child process (on the host) is authoritative for game state. late.sh owns only the terminal bytes (vt100) and a status flag; it stores nothing about NetHack in its DB.
- While Running, do not route NetHack bytes through the normal late.sh input pipeline. Only `F1` is late.sh's (it injects NetHack's `?`); everything else is forwarded raw.
- Keep mouse/paste stripping in client `forward_input`. With `?1003h` mouse tracking on, unfiltered motion reports cancel NetHack menus.
- Force `ProxyStatus::Closed` and wake the render loop the instant the connection closes, before cleanup, or the screen freezes on the last frame.
- **Auth: compare the key DATA, not the whole `PublicKey`.** `ssh_key::PublicKey`'s `PartialEq` includes the comment field; a key arriving over the wire has no comment while the host's locally-derived `authorized_key` does, so a whole-struct comparison rejects every connection. `auth_publickey` compares `key.key_data()`. (This bit us once.)
- **`derive_client_key` must stay byte-identical across the two crates** (same `KEY_DOMAIN`, same blake3 steps). Drift → client derives a different key → host rejects everything. A known-answer test in both crates pins the fingerprint (§8).
- `nethack_playname` derives the `-u` name **only from the immutable `user_id`** (`late_` + UUIDv7 trailing hex), never the username — a rename would orphan the save, and usernames stripping to the same alphanumerics would collide. Travels as the SSH username; the host re-sanitizes (`playname::sanitize`, keeps `_`) before `-u`. Stable, unique, PTY-safe, within `PL_NSIZ`.
- Spawn the child with `env_clear()` + an explicit allowlist. Even though the host is dedicated, keep its env minimal. NetHack's shell/suspend escapes are compiled out in `nethack-build`.
- Keep XON/XOFF flow control **off** on the host PTY, or a stray Ctrl-S freezes output until Ctrl-Q (§9).
- On host child exit, close the channel first, then **detach** the reader thread — never join it (the save-compressor gotcha, §9).
- The host child must run as `late` and be able to **read** HACKDIR (esp. `sysconf`, §9) and **write** `VAR_PLAYGROUND`.
- Treat all exits identically — clean save, death, quit, crash, network drop all return to the launcher.
- When disabled, fail soft (launcher message + no-op connect), never panic.
- `mod.rs` stays declaration-only.

---

## 8. Tests And Verification [STABLE]

Root policy applies: agents should not run `cargo test`/`nextest`/`clippy` as blocking verification; mention the focused command in handoff.

Inline pure tests cover:
- Client `proxy.rs`: `nethack_playname` (account-derived, PTY-safe, within `PL_NSIZ`, stable, distinct per account).
- Client `identity.rs` / host `late-nethack/identity.rs`: derivation determinism + a **known-answer fingerprint** (`late-nethack-kat-v1` → `SHA256:JA9AvdNoX1ZZMA43t1qMUzq73OW609Fme6rrle84UeU`) — the cross-crate drift guard.
- Client `state.rs`: `connect` no-op when disabled; `forward_input` without a proxy is a no-op; `strip_input_noise` drops mouse/paste but keeps keys/arrows; F1 (both encodings) consumed; exit-grace opens on close and counts down.
- Host `playname.rs`: sanitize keeps alphanumerics + `_`, strips metachars, caps length, falls back when empty.
- Host `server.rs`: `effective_term` falls back for unknown/hostile TERM and passes a supported one through.
- `app/common/primitives.rs` + `app/input.rs`: screen `next`/`prev` ordering and topbar columns place `Nethack` between `Rebels` and `Pinstar`.

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
- No late.sh persistence layer: durable state is on the host's disk/PVC. Save recovery after a dropped session depends on NetHack's own save/recover.
- HACKDIR (`/var/games/nethack`) is a read-only image layer, refreshed on rebuild; writable state (`/var/games/nethack-var`) is the `nethack-save` PVC (prod) or the image's baked seed (dev/unmounted, ephemeral).
- Multiple concurrent sessions for the same account share one `-u` save name; NetHack's own save lock makes a second concurrent launch refuse to load. Not specially handled.
- **Process-count envelope.** Each SSH connection to the host spawns at most one child; concurrent connections are bounded by late-ssh's `conn_limit`/per-IP caps (NetHack children are 1:1 with door sessions). The host pod's CPU/memory limits are the backstop. There is deliberately **no** separate per-user/global cap; add one in `connect`/the host if the envelope gets too loose.
- Binary built from verified upstream source (5.0.0); not fully hermetic (fetches Lua). When bumping versions, update the `NETHACK_*` Dockerfile `ARG`s (incl. `NETHACK_SHA256`) and `NOTICE`.

### Possible future work
- Dedup `derive_client_key`/`KEY_DOMAIN` into a shared crate (currently duplicated in both, guarded only by the KAT). Deferred to avoid pulling `russh`/`blake3` into a lower-level crate for ~15 lines.
- Per-user/global concurrency cap on the host pod if needed.
- Real OTLP telemetry in `late-nethack` (today the `otel` feature is a no-op).
