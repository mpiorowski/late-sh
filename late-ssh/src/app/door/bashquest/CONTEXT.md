# BashQuest Door Context

## Metadata
- Scope: the BashQuest door as a whole — the **client** in `late-ssh/src/app/door/bashquest` (proxy/identity/state/render/mod) plus its screen lifecycle wiring in `late-ssh/src/app` (state/input/render/tick) **and the standalone host crate `late-bashquest/`**. There is no separate `late-bashquest/CONTEXT.md`; this file is the single source for both halves.
- Domain: BashQuest, a native late.sh original (not a foreign upstream binary): an interactive terminal game teaching Linux/Bash, 90 levels across 18 tiers, written by Tony "Hardlygospel" Hosaroygard (GPL-3.0, `github.com/hardlygospel/bashquest`), run on a PTY inside a **dedicated `late-bashquest` SSH host** and reached by late-ssh as a network-proxied door (same transport model as dopewars/DCSS).
- Primary audience: LLM agents changing the BashQuest launcher UI, the SSH client transport, the host crate (PTY bridge / auth), input forwarding, or its config/deploy wiring.
- Last updated: 2026-08-24 (`forward_input` goes through `keys_for_game`, the shared `app/door/keys.rs` cursor-key gate wired into every vt100-backed door; a no-op for a plain bash script unless the guest ever requests application cursor mode)
- Status: Active
- Parent context: `../../../../../CONTEXT.md`
- Stability note: `[STABLE]` sections change rarely; `[VOLATILE]` sections change with the launcher UI or build/deploy wiring.

---

## 0. Context Maintenance Protocol [STABLE]

Read this after root `CONTEXT.md` whenever a task touches the BashQuest launcher, launch/leave behavior, the SSH client transport, the `late-bashquest` host (PTY bridge, auth), input forwarding, or BashQuest config/deploy wiring.

- Keep this file aligned with the SSH transport contract, the client/host split, the spawn args, config knobs, and known gotchas.
- Update root `CONTEXT.md` when routing, the top-level screen list/tab order, or global keybindings change.
- Treat tests and code as authoritative when comments drift; patch stale comments or this file before handoff.
- Do not add `pub use` re-export layers; `mod.rs` stays declaration-only.

---

## 1. Summary [STABLE]

BashQuest runs **bashquest.sh on a PTY**, but **not** inside late-ssh. It lives in its own crate/pod, `late-bashquest`, a minimal russh **server** that spawns one `bashquest.sh` child per SSH session. late-ssh reaches it exactly like the dopewars/DCSS doors reach their hosts: the door is a russh **client** that streams the remote terminal through a `vt100::Parser` and blits it into a ratatui widget below the top bar. SSH *is* the transport — there is no custom IPC.

Core shape:
- `Screen::Bashquest` has no top-level number key. It is reached by selecting the BashQuest card in the Games hub (page `3`) and pressing `Enter`.
- **Identity is the arcade handle, exactly like DCSS/Usurper/Brogue** (`door::arcade::HandleFlow`, shared across every arcade-handle door — a player who already claimed a handle elsewhere never sees a second prompt here). The claimed handle is sent as the SSH username; the host re-sanitizes it and hands it to the child as `BASHQUEST_AUTOLOGIN`, which makes bashquest.sh skip its own login/register screen and resume (or silently create) the save matching that exact handle. See bashquest.sh's own `autologin()` function.
- One per-session `BashquestProcess` (a russh client) owns a background Tokio task that connects to `late-bashquest`, requests a PTY + shell, and bridges the remote bytes into a shared `vt100::Parser`. The foreground reads that screen and a `ProxyStatus` flag.
- Auth is shared-secret-derived (Ed25519 key from `LATE_BASHQUEST_SECRET`), same as every other door.
- While Running, raw client bytes are forwarded straight to the host→child (minus mouse/paste noise) — bashquest.sh, not late.sh, interprets keys. There is **no** key remap: bashquest.sh is menu- and prompt-driven, not modal like a roguelike, so there's no help key worth intercepting. There is also **no detach**: unlike the roguelike doors, leaving the screen always tears the session down (see §3), matching Usurper/dopewars' shape rather than nethack/DCSS/brogue's.
- **Persistence: a single shared, persistent HOME**, not per-player (unlike nethack/DCSS's per-`-u`/`-name` playground). bashquest.sh keeps `users.db` and every player's `<name>.save` under `$HOME/.bashquest`; every session gets the *same* `LATE_BASHQUEST_DATA_DIR` as `HOME`, because bashquest.sh's own in-game leaderboard (`leaderboard()`) only means anything if every late.sh player's save lands in the same place. bashquest.sh saves continuously — after nearly every state-changing action (wrong answer, hint, skip, level/tier complete, graduation), not just on explicit logout — so there is **no SIGHUP-save dance** on the host: teardown is a plain kill, same as dopewars, and the worst case loss is the single in-flight unanswered challenge.

The door is gated by the `bashquest_enabled` profile flag in `late-ssh/src/config.rs` (enabled in every current profile); when disabled, `connect` is a no-op and the launcher shows "Currently unavailable". The host pod is deployed unconditionally (the flag gates only the client).

---

## 2. Module Map [STABLE]

### Client — `late-ssh/src/app/door/bashquest/`

| File | Responsibility |
|---|---|
| `mod.rs` | Module declarations + framing comment. Declaration-only. |
| `proxy.rs` | `BashquestProcess`: per-session russh **client** to the host. Owns the bridge task (`run_bridge`), the shared `vt100::Parser`, the `ProxyStatus` flag, and the input/resize command channel; `ProcessConfig.playname` carries the arcade handle (sent as the SSH username). Near-clone of `door::dopewars::proxy`, with DCSS's playname-as-identity contract instead of dopewars' opaque session label. Also owns `extract_marker`/`take_graduation`: scans inbound bytes for the host's graduation marker before vt100 parsing (§10). |
| `identity.rs` | `derive_client_key(secret)`: the shared-secret → Ed25519 key derivation (blake3, domain `late.sh/bashquest/v1`). Must stay byte-identical to the host's copy — a KAT pins it (§8). |
| `state.rs` | Per-session `State`: launcher/running `Mode`, connection config (host/port/secret/term/enabled), the optional `BashquestProcess`, last viewport `Rect`, the post-exit input grace, the shared `HandleFlow` (lookup/claim prompt/launch intent — DCSS/Usurper pattern), the account's real `user_id` and optional `BashquestAwards`, `connect`/`tick`/`launcher_key`/`forward_input`/`strip_input_noise`. `tick()` drains any pending graduation off the proxy and hands it to `BashquestAwards::spawn_record` (§10). |
| `graduate.rs` | `BashquestAwards`: thin `Db`-backed sink that persists a verified graduation (the same fire-and-forget shape as the roguelike doors' `ingest::award` milestone sink). `None` on headless/test paths. |
| `render.rs` | Ratatui rendering: `draw_launcher` (via `landing::handle_launch_block`, the shared arcade-handle claim UI) / `draw_landing` (hub card, no live/resume state) and `draw_running` which blits the live `vt100` screen via `rebels::render::blit_screen`. |

### Host — `late-bashquest/` crate (standalone binary)

| File | Responsibility |
|---|---|
| `main.rs` | Tracing init, `Config::from_env`, load/generate the SSH host key, `create_dir_all` the data dir, run the russh server (`run_on_address`). Exits promptly on SIGTERM (no save to drain). |
| `config.rs` | `Config`: `bin`, `data_dir` (shared, persistent `HOME` for every session — DCSS's shape, not dopewars' per-session scratch dir), `secret`, listen addr/port, idle timeout. |
| `server.rs` | russh `Server`/`ClientHandler`: `auth_publickey` (compares the derived key, captures + sanitizes the playname from the SSH username — see §7), `pty_request`, `shell_request`, `data`, `window_change_request`, `channel_eof/close`. Also `effective_term`/`term_supported`: bashquest.sh clears the screen with `clear`, a terminfo consumer, so an unresolvable client TERM falls back to `xterm-256color` (§9). |
| `host.rs` | `PtyHost`: the per-session PTY bridge. `openpty` + `env_clear` + `setsid`/`TIOCSCTTY` + `IXON/IXOFF/IXANY` clear + `TIOCSWINSZ` + the **detached** reader. Spawns `bashquest.sh` directly (its own shebang, no CLI args) with `HOME` set to the *shared* `data_dir` and `BASHQUEST_AUTOLOGIN` set to the playname. Output flows to the SSH channel handle; client bytes flow to the PTY master. Teardown is a plain kill — bashquest.sh has no hangup-save need (see §1). Also `report_certificate_if_new`: after the child exits, checks for a fresh graduation certificate file and, if present, sends the marker directly over the channel (§10) — never derived from anything the child wrote. |
| `identity.rs` | `derive_client_key(secret)` — identical to the client copy (KAT-pinned). |
| `playname.rs` | `sanitize(username)`: keep `[A-Za-z0-9_]`, cap at 20 (matching the arcade handle's own `HANDLE_MAX_LEN`), fall back to `"player"`. Defense in depth on top of the already-validated arcade handle and bashquest.sh's own re-sanitizing `autologin()`. |

Cross-module wiring (client side, outside this folder — mirrors dopewars/DCSS's ~10 touchpoints):
- `app/common/primitives.rs`: `Screen::Bashquest` (+ `next`/`prev` fall back to `Games`, `draw_tabs`/page title label `"BashQuest"`).
- `app/door/arcade.rs`: shared `ArcadeHandleService`/`HandleFlow` (not BashQuest-specific; reused as-is).
- `app/door/hub/state.rs`: `HubGame::Bashquest` + `ALL` (in the `Doors` group, after Dopewars) + label + `rc_game() -> None` (no pushed config file).
- `app/door/hub/ui.rs`: `HubView.bashquest_enabled` + the landing match arm (no `bashquest_live`: this door has no detach/resume model).
- `app/state.rs`: `App::bashquest_state`/`bashquest_term`/`bashquest_enabled`/`bashquest_host`/`bashquest_port`/`bashquest_secret`, `enter_bashquest`/`leave_bashquest`, `set_screen` enter/leave arms (unconditional teardown on leave, no detach guard), and the Running-mode passthrough + exit-grace swallow in `App::handle_input`.
- `app/tick.rs`: `State::tick()` each app tick + return-to-`Games` once `!is_running() && !in_exit_grace() && !awaiting_handle()`.
- `app/render.rs`: `DrawContext.bashquest_enabled`/`bashquest_state`, take/restore `bashquest_state` so the draw path can `set_viewport(content_area)` before blitting, the dispatch arm (via `.as_deref_mut()`, not a move — the modal-check code below reads the field again), the name-claim-modal draw call, the title-bar credit + in-game `Ctrl-C quit` hint.
- `app/input.rs`: hub launch arm (`set_screen` + `connect`, banner if disabled), dedicated-screen launcher-key routing (`launcher_key_byte` + `HandleFlow`, mirroring Usurper's shape, not dopewars' plain-Enter shape), the modal-key routing function (Esc dismisses / other bytes go to `launcher_key`), and arrow/key dispatch no-ops (Running-mode bytes are forwarded raw upstream).
- `config.rs`, `state.rs` (`SessionConfig`), `ssh.rs`, `session_bootstrap.rs`, `src/test_helpers.rs`: thread `bashquest_enabled`/`bashquest_host`/`bashquest_port`/`bashquest_secret`.

---

## 3. Screen Lifecycle And Input Capture [STABLE]

- `Enter` on the selected BashQuest card in the hub calls `set_screen(Screen::Bashquest)` (which runs `enter_bashquest`, constructing `State`) then `State::connect`. If the account has no arcade handle yet, `connect` records launch intent and the launcher shows the claim prompt instead; `tick()` fires the deferred `connect()` once the handle lands (see `door::arcade::HandleFlow`).
- **No detach.** Unlike nethack/DCSS/brogue, leaving the screen **always tears the session down** (`leave_bashquest`, unconditional — same as Usurper/dopewars): bashquest.sh saves continuously, so there is nothing worth keeping a background SSH connection alive for, and no SIGHUP-save to give it time for. Dropping `bashquest_state` drops `BashquestProcess`, whose `Drop` aborts the client bridge task → the SSH connection closes → the host's `channel_close` drops its `PtyHost`, which kills the child immediately.
- `State::tick` (each app tick) flips back to `Mode::Launcher` if the connection closed for any reason (logout, quit, graduation, crash, or network drop) — all exits are treated identically. `App::tick` then returns the session to the Games hub once the post-exit input grace (`in_exit_grace`) has elapsed and the handle flow isn't mid-lookup/claim (`awaiting_handle`).

Input capture contract (client side):
- The **launcher** behaves like DCSS/Usurper's: while the claim modal is visible, its keys belong to the modal router (Esc dismisses, everything else composes/submits the handle); once claimed, only Enter is consumed to launch, every other key falls through to normal global handling. **Exception:** for a short post-exit grace window the launcher swallows *all* input — see the exit-grace gotcha in §9.
- While **Running**, `App::handle_input` intercepts bytes *before* the normal input pipeline: if `state.is_running()`, it `forward_input`s straight to the host and returns. There is **no** key remap — bashquest.sh is prompt-driven, so number/letter keys, `hint`/`skip` typed as words, Enter, etc. all reach the game verbatim.
- `forward_input` strips mouse reports (SGR `ESC [ < … M/m`, legacy X10 `ESC [ M b x y`) and bracketed-paste markers, identical to every other PTY door — late.sh keeps any-event mouse tracking on for its own UI, and those motion reports' leading `ESC` would otherwise leak into bashquest.sh as stray input.

---

## 4. Transport Architecture [STABLE]

### Client (`proxy.rs`, in late-ssh) — the vt100 side

- `BashquestProcess::spawn` creates an mpsc command channel, a shared `vt100::Parser` (sized to the viewport), a `ProxyStatus` mutex, and spawns the bridge task. On task end it forces `ProxyStatus::Closed` and wakes the render loop.
- `run_bridge` is a russh client (`AcceptAnyHostKey`): `client::connect` → `authenticate_publickey(username = cfg.playname (the arcade handle), key = derive_client_key(secret))` → `channel_open_session` → `request_pty` → `request_shell` → status `Running`. Then a `tokio::select!` loop identical to dopewars/DCSS's.
- The vt100 parser lives **client-side only**.

### Host (`late-bashquest`) — the PTY side

- `ClientHandler` (one per SSH connection): `auth_publickey` checks the derived key and captures + sanitizes the playname from the SSH username; `pty_request` records term/cols/rows; `shell_request` spawns a `PtyHost`.
- `PtyHost::spawn` → `run_bridge` (unix only): `openpty`, clear `IXON/IXOFF/IXANY` on the slave termios before exec, build the `bashquest.sh` `Command` with `env_clear()` + allowlist (`TERM`, `HOME=<shared data_dir>`, `BASHQUEST_AUTOLOGIN=<playname>`, `LANG`/`LC_ALL=C.UTF-8` for the box-drawing/emoji output, `LINES`/`COLUMNS`), wire slave→stdio, `pre_exec` `setsid` + `TIOCSCTTY`. A blocking reader thread pumps PTY output to an unbounded channel; the select loop forwards those chunks to `handle.data(channel, …)`, writes client `Input` to the PTY master, applies `Resize` via `TIOCSWINSZ`, and breaks on `child.wait()`.
- **TERM fallback (`effective_term`), same as every other door host.** bashquest.sh writes ANSI escapes directly (`printf '\033[...'`) and never calls `initscr()`/`newterm()`, so it has no ncurses abort mode, but its `clear_screen()` shells out to `clear`, which *is* a terminfo consumer: on a TERM the host cannot resolve it prints `'<term>': unknown terminal type.` and clears nothing, so every redraw stacks under the previous screen instead of replacing it. `shell_request` therefore substitutes `xterm-256color` when `/usr/share/terminfo`, `/etc/terminfo`, and `/lib/terminfo` all lack an entry for the requested name (which also blocks a path-traversal TERM), and logs the substitution.
- On child exit or client disconnect: close the SSH channel first (so the client returns to its launcher now), then kill the child. No SIGHUP dance — see §1.

### Sizing
- `State::set_viewport` (client, from the draw path) resizes the local parser and sends a `Resize` command; the client forwards a `window_change`, the host applies `TIOCSWINSZ`, and the kernel signals `SIGWINCH` — bashquest.sh doesn't read window size dynamically mid-challenge, but a fresh screen redraw (e.g. the next menu) picks up `$LINES`/`$COLUMNS` correctly since they're re-read, not cached, at each render.

### Render
- `draw_running` blits the current `vt100` screen; before `Running` it shows "Starting bashquest...". The app frame title shows a dimmed `by github.com/hardlygospel/bashquest` credit, plus `· Ctrl-C quit` while running.

---

## 5. Launcher UI [VOLATILE]

- `draw_launcher`: a BashQuest ASCII logo, a one-line blurb, a tier-progression strip (Beginner → Networking → SAN → Kernel → Ricing → Docker → TUI), stat lines (90 levels, 18 tiers, the challenge/answer loop), a flavor quote from the in-game mentor Tasmania, and `landing::handle_launch_block` for the Launch line (claim prompt / claiming / launch action / retry, depending on `HandleStatus`), an "Once Inside" hint block (`hint`, `skip`, `Ctrl-C`), and the GitHub URL.
- The app frame title shows a dimmed "by github.com/hardlygospel/bashquest" credit on this screen, plus the in-game `Ctrl-C quit` hint while running.

---

## 6. Configuration And Deploy [VOLATILE]

### Client config (`late-ssh/src/config.rs` profile → `SessionConfig` → `App`)
- Client enabled/host/port are profile literals in `late-ssh/src/config.rs` (dev `service-bashquest`, prod `late-bashquest-sv`, port 2330); `LATE_BASHQUEST_SECRET` is the only env the client reads (must equal the host's).

### Host config (`late-bashquest` env)
- `LATE_BASHQUEST_SECRET` (required), `LATE_BASHQUEST_BIN` (default `/usr/local/bin/bashquest.sh`), `LATE_BASHQUEST_DATA_DIR` (default `/var/lib/late-bashquest`, the one shared persistent HOME on the PVC), `LATE_BASHQUEST_LISTEN_ADDR` (default `0.0.0.0`), `LATE_BASHQUEST_PORT` (default `2330`), `LATE_BASHQUEST_IDLE_TIMEOUT`.

### Binary sourcing — **pinned commit, not a compiled build**
- Unlike every other door, there is nothing to compile: `bashquest.sh` is fetched by exact commit SHA and SHA-256-verified in `docker/doors/bashquest.Dockerfile` (`bashquest-build` stage), then `chmod 0755`'d. Bump `BASHQUEST_COMMIT`/`BASHQUEST_URL`/`BASHQUEST_SHA256` together when pulling in a newer upstream version, and update `NOTICE`.
- `BASHQUEST_AUTOLOGIN` is not a late.sh patch: it's an upstream feature bashquest.sh itself added specifically to support this integration (opt-in, backward-compatible — unset, the script behaves exactly as it always has for standalone `curl | bash` users). No source is forked or modified.

### Images (Dockerfile)
- `base` copies the verified script to `/usr/local/bin/bashquest.sh` (from the `bashquest-build` stage) so `dev-bashquest` (which derives from `base`) can run it; prod ships the same copy in `runtime-bashquest`, which also installs `ncurses-term` for `clear`'s terminfo lookup (§9). `late-bashquest` (the host binary) builds in its own `builder-bashquest` cargo-chef stage, same shape as every other door host.
- The committed `.env.dev` / `.env.dev2` templates carry the host-side settings compose passes to `service-bashquest` (`LATE_BASHQUEST_PORT` / `_SECRET` / `_DATA_DIR`), mirroring the dopewars/DCSS block; the client's host and port are profile literals, not env.

### Prod (Kubernetes / terraform)
- `infra/doors.tf` (`bashquest` entry, stamped out by the `infra/door` module): the `late-bashquest` Deployment (replicas **1**, `runtime-bashquest` image, `bashquest-save` PVC mounted at the shared HOME, a `bashquest-save-seed` initContainer that chowns the mount to `late`, `RUST_LOG`/`LATE_BASHQUEST_SECRET`/`LATE_BASHQUEST_DATA_DIR` env) + `late-bashquest-sv` ClusterIP Service on 2330, the RWO `bashquest-save` PVC (`local-path`, 256Mi, `prevent_destroy`), and the `bashquest-identity-secret` (random 64-char) injected into **both** service-ssh and late-bashquest so they derive the same key. **Deployed unconditionally**; kill-before-create rollout (`maxSurge=0`/`maxUnavailable=1`) so the old pod releases the RWO volume before the new one mounts it.
- `infra/service-ssh.tf` injects the client's only env, `LATE_BASHQUEST_SECRET`.
- `replicas` must stay 1 (one RWO volume holds every player's shared save data; assumes the single-node `local-path` cluster).
- CI: `-bashquest` releases route through `release.yml` to the generic `deploy_service.yml` (image-only `kubectl set image` on the existing deployment, or a `-target=module.door["bashquest"]` terraform bootstrap on first deploy). `doors.yml` build-validates `docker/doors/bashquest.Dockerfile` (fetch + checksum + the `docker/doors/smoke/bashquest.sh` smoke test) and publishes the pinned `door-bashquest` image on main pushes at the tag pinned in the root Dockerfile.

---

## 7. Critical Invariants [STABLE]

- The child process (on the host) is authoritative for game state. late.sh owns only the terminal bytes (vt100) and a status flag. The only durable state anywhere is the host's shared `$HOME/.bashquest` directory (users.db + saves + certificates) on the PVC.
- **`HOME` is shared across every session, deliberately, unlike nethack/DCSS's per-player playground.** Do not "fix" this into a per-account directory without also deciding what happens to the in-game leaderboard, which currently only works because every player's `users.db` is the same file.
- While Running, do not route bashquest.sh bytes through the normal late.sh input pipeline — forward them raw. There is no key remap.
- Keep mouse/paste stripping in client `forward_input`.
- **Auth: compare the key DATA, not the whole `PublicKey`.** Same gotcha as every other door's host (`ssh_key::PublicKey`'s `PartialEq` includes the comment field).
- **`derive_client_key` must stay byte-identical across the two crates** (same `KEY_DOMAIN` `late.sh/bashquest/v1`, same blake3 steps). Drift → client derives a different key → host rejects everything. Pinned by a KAT in both crates' `identity_test.rs` (§8).
- Keep XON/XOFF flow control **off** on the host PTY, or a stray Ctrl-S freezes output until Ctrl-Q.
- Spawn the child with `env_clear()` + an explicit allowlist (incl. a UTF-8 `LANG`/`LC_ALL`, since bashquest.sh's box-drawing characters and emoji need it).
- Treat all exits identically — logout, quit, graduation, crash, network drop all return to the hub.
- When disabled, fail soft (launcher message + no-op connect), never panic.
- `mod.rs` stays declaration-only.
- **The graduation marker must only ever originate from host Rust code, never from anything read off the child's PTY.** This is the entire anti-forgery property (§10); if a future change ever routes the marker through the same code path that forwards child output, a player could in principle spoof it.
- **`CERT_MARKER_TAG` must stay byte-identical between `late-bashquest/src/host.rs` and `late-ssh/src/app/door/bashquest/proxy.rs`**, same as `derive_client_key`'s `KEY_DOMAIN` above — drift means graduations silently stop being recorded, not a hard failure, so it wouldn't be caught by a crash.

---

## 8. Tests And Verification [STABLE]

Root policy applies: agents should not run `cargo test`/`nextest`/`clippy` as blocking verification; mention the focused command in handoff.

Inline pure tests cover:
- Client `identity.rs` / host `identity.rs`: derivation determinism + a cross-crate known-answer fingerprint test (`SHA256:9NHbIJzzfj+WQ4YoYYlgWtjvH7N+FE2m1KYAp3X/73c` for secret `late-bashquest-kat-v1`), pinning the cross-crate contract from day one (unlike dopewars, which shipped this as a TODO).
- Host `playname.rs`: sanitization (alnum+underscore only, cap at 20, empty falls back to `"player"`).
- Host `server.rs`: auth records/sanitizes the playname, a rejected auth records none, and `effective_term` falls back to `xterm-256color` for an unresolvable or hostile TERM while passing a resolvable one through.
- Client `state.rs`: `connect` no-op when disabled; `forward_input` without a proxy is a no-op; `strip_input_noise` drops mouse/paste but keeps keys/arrows; exit-grace opens on close and counts down; a disabled door's `HandleFlow` settles to `Missing` instead of hanging in `Loading`.
- Client `proxy.rs`'s `marker_test`: `extract_marker` parses a well-formed marker, finds one surrounded by other bytes and reports the correct strip span, rejects a marker whose certificate doesn't match its embedded digest (tamper/corruption), ignores plain output with no marker, and ignores a truncated one.
- `late-core/src/models/bashquest_graduate_test.rs` (DB-integration, needs `TEST_DATABASE_URL`): `record` is idempotent per account (a second report doesn't overwrite the first certificate), `list_all` returns every graduate.
- `app/door/hub/state_test.rs` + `app/door/hub/ui_test.rs`: selector ordering, screen `next`/`prev` placement, and sidebar hit-test coordinates (updated for the 11th `HubGame`/19-row sidebar).

The PTY bridge (`host.rs`) and the russh client/server loops are process/network-bound and not unit-tested; verify launch/play/logout manually against a real host.

Focused commands for human verification:

```bash
cargo test -p late-bashquest && cargo test -p late-ssh bashquest
```

(Don't fold these into one `-p late-bashquest -p late-ssh bashquest` — the `bashquest` name filter would also apply to the host crate and skip its tests.)

---

## 9. Known Gotchas [VOLATILE]

### Client-side
- **Trailing game keys can quit the whole app (exit-grace).** Same pattern as every other door: bashquest.sh's "Press Enter to continue" prompts make players mash keys right as a run ends; the guard is `EXIT_GRACE_TICKS` (~0.66s) during which the launcher swallows input.
- **No detach.** If a future change wants BashQuest to support the backtick workspace cycle like the roguelikes, that's a real design change (bashquest.sh has no hangup-save, so "keep it running in the background" would need the game to gain one, or accept that a detached game silently dies on the next teardown path).

### Host-side (`late-bashquest`)
- **Ctrl-S freeze (XON/XOFF).** Cleared on the PTY before exec, same as every other door host.
- **`clear` needs terminfo even though bashquest.sh is "plain ANSI".** This shipped broken: the door was built on the assumption that a non-curses script has no terminfo dependency, but `clear_screen() { clear; }` does, and `runtime-bashquest` only carried the ncurses-base entry set. A ghostty client (`xterm-ghostty`, terminfo installed on the player's machine, absent on the host) got `'xterm-ghostty': unknown terminal type.` twice per redraw and a screen that never cleared, so banners/menus stacked. Fixed on both sides: `effective_term` in `server.rs` (§4) and `ncurses-term` in the runtime image. Don't reintroduce the "no terminfo concerns" assumption.
- **Shared HOME is a feature, not a bug.** Every session getting the *same* `LATE_BASHQUEST_DATA_DIR` is deliberate (see §7), unlike almost every other door host in this codebase.

### Operational
- **Continuous save, not save-on-exit.** bashquest.sh calls `save_progress` after nearly every state change (see the `save_progress` call sites in `bashquest.sh`), so a pod restart or dropped connection loses at most the single in-flight unanswered challenge — never earned XP, level progress, or achievements.
- **Playground on the PVC.** `LATE_BASHQUEST_DATA_DIR` is one shared directory; `replicas` must stay 1 (RWO volume, single-node `local-path`). bashquest.sh's own account system (`register_user`/`login_user`/`autologin`) handles concurrent access to `users.db` the same way it always has for any multi-user install of the script.
- Script fetched by pinned commit (see §6); when bumping versions, update the `BASHQUEST_*` Dockerfile `ARG`s (incl. `BASHQUEST_SHA256`) and `NOTICE`.

### Possible future work
- ~~Milestones/chips/awards for graduation~~ **Done — see §10.** Graduation reporting shipped without a screen scrape: `late-bashquest` reads its own certificate file directly (it already controls that filesystem) rather than needing a machine-readable achievement file added to bashquest.sh. Tier-completion milestones (short of full graduation) are still unimplemented; the same host-side file-check approach would extend to them if bashquest.sh ever wrote a per-tier marker file.
- A public web/TUI badge for graduation (like nethack's Amulet/ascension badge on the profile modal) is not wired up. `profile_award.rs`'s category tables and `profile_modal/badges.rs`'s legend would need a `BASHQUEST_GRADUATE_AWARD_CATEGORY` entry; the `bashquest_graduates` table (§10) is intentionally separate from `profile_awards` since it holds full certificate text, not just a badge fact.

---

## 10. Certificate / Anti-Forgery Pipeline [STABLE]

Records a verified "this account graduated BashQuest" fact in the database, making sure nothing in the chain can be spoofed by a player who only ever gets PTY keystrokes into the running game, never a shell or filesystem access.

**Nothing publishes this table.** The door records the graduation and stops there. Exposing graduates anywhere, in the TUI or on the web, is its own change and its own decision: the rows hold a player's handle and their full certificate text.

**Chain of custody, host → client → database:**

1. **Host (`late-bashquest/src/host.rs`):** right before closing the channel in `run_bridge` (after the child has already exited — see `report_certificate`), check whether bashquest.sh wrote `$data_dir/.bashquest/<playname>.certificate.txt` (its own `graduation_ceremony`, only reached after completing all 90 levels). If so, send a marker **directly via `handle.data()`** — never anything read from the child's PTY output. This fires on **every** session a graduate plays, deliberately: the host cannot know whether late-ssh persisted anything (`handle.data()` returning Ok only means the bytes were queued), so a local "already reported" sentinel would turn one failed database write into a permanently lost certificate while suppressing the only path that could heal it. Re-sending is absorbed by step 5's idempotent insert.
2. **Marker format** (`CERT_MARKER_TAG = b"\x00BQCERT\x01"`, byte-identical in both `host.rs` and client `proxy.rs`): `TAG <64-hex blake3 digest of certificate> \x01 <handle> \x01 <certificate bytes> \x00`. The leading NUL is not something bashquest.sh ever legitimately prints (plain ANSI/UTF-8 only) and is sent from a call site the child process's output never flows through, so nothing the player types or pastes can reach it.
3. **Client (`late-ssh/src/app/door/bashquest/proxy.rs`, `extract_marker`):** every inbound `ChannelMsg::Data` is scanned for a complete marker before being fed to the vt100 parser. A match is stripped out (so it never corrupts the rendered screen) and verified against its own embedded digest before being trusted — a marker split across an unlucky chunk boundary is dropped rather than recorded with truncated content, not perfectly robust to adversarial fragmentation (see the doc comment on `extract_marker`; only needs to be correct for the common case, since the host always sends it as one call immediately before closing).
4. **`BashquestProcess::take_graduation`** hands the parsed `GraduationRecord` (handle + certificate bytes) up to `state.rs`'s `tick()`, which already holds the session's real, SSH-authenticated `user_id` — never anything derived from the claimed handle alone.
5. **`door::bashquest::graduate::BashquestAwards::spawn_record`** (the same fire-and-forget shape as the roguelike doors' `ingest::award` milestone sink) persists it via `late_core::models::bashquest_graduate::BashquestGraduate::record`, an `INSERT ... ON CONFLICT (user_id) DO NOTHING` into the `bashquest_graduates` table (migration `142_create_bashquest_graduates.sql`) — first record per account wins and is immutable; a re-report is a harmless no-op.
**Why each hop matters:** a player has no shell, so they cannot write the certificate file themselves (step 1's precondition). The marker cannot come from anywhere except host-originated Rust code (step 1/2). The client cross-checks the digest before trusting content (step 3). The database write is keyed off the session's real authenticated account, not the self-reported handle string (step 4/5). Breaking the guarantee requires compromising late-bashquest's host process itself, not just knowing how the format works.

**If something reads this table later:** `BashquestGraduate::list_all` already filters `user_id IS NOT NULL`, so an account deleted through the normal path drops out while its historical row survives (`user_id` is `ON DELETE SET NULL`). Keep that filter, and if the rows get mirrored anywhere, replace the copy rather than append to it, or a deletion here never reaches the mirror.
