# NetHack Door Context

## Metadata
- Scope: `late-ssh/src/app/door/nethack` plus the NetHack screen lifecycle in `late-ssh/src/app` (state/input/render/tick wiring)
- Domain: NetHack, the real upstream roguelike run locally on a PTY inside late.sh
- Primary audience: LLM agents changing the NetHack door host, PTY bridge, launcher UI, input forwarding, or its config/deploy wiring
- Last updated: 2026-06-24
- Status: Active
- Parent context: `../../../../../CONTEXT.md`
- Stability note: Sections marked `[STABLE]` should change rarely. Sections marked `[VOLATILE]` are expected to change when the launcher UI, keybindings, or deploy wiring change.

---

## 0. Context Maintenance Protocol [STABLE]

Read this file after root `CONTEXT.md` whenever a task touches the NetHack launcher, launch/leave behavior, PTY process bridge, input forwarding/filtering, the F1→`?` help remap, or NetHack config/deploy wiring.

- Keep this file aligned with the PTY transport contract, input-filter behavior, config knobs, and known gotchas.
- Update root `CONTEXT.md` when routing, the top-level screen list/tab order, global keybindings, or deploy/config contracts change (the NetHack research/decision note lives in root `Future Work`).
- Treat tests and code as authoritative when comments drift. Patch stale comments or this file before handoff.
- Do not add `pub use` re-export layers; `mod.rs` should stay declaration-only.

---

## 1. Summary [STABLE]

NetHack is a door game that runs the **real upstream NetHack binary locally** on a PTY. Unlike Lateania (a late.sh-native MUD) and unlike Rebels (which proxies a *remote* SSH server), late.sh owns the NetHack process: it spawns the configured `nethack` binary, streams the child terminal through a `vt100` emulator, and blits it into a ratatui widget below the top bar.

Core shape:
- `Screen::Nethack` and the top-level key `7` reach the NetHack screen (tab order: `… Lateania(5) Rebels(6) NetHack(7) Pinstar(8)`).
- The launcher is a static page. `Enter` spawns the process and switches to Running mode.
- One per-session `NethackProcess` owns a background Tokio task that runs the child on an `openpty` PTY and bridges its output into a shared `vt100::Parser`. The foreground reads that screen and a `ProxyStatus` flag.
- While Running, raw client bytes are forwarded straight to the child (minus mouse/paste noise), so NetHack — not late.sh — interprets keys. `F1` is the only key late.sh keeps for itself, and it is merely **remapped to NetHack's own `?` help** (late.sh has no help UI of its own).
- Per-player saves come from launching `-u <playname>` against a shared late.sh-owned playground, so deaths naturally seed common **bones** across users.
- There is **no late.sh-side persistence**: saves/bones/dumplogs live in NetHack's own playground on disk, keyed by the `-u` name. late.sh stores nothing in its DB for this door.

The door is gated behind `LATE_NETHACK_ENABLED` (default `false`). When disabled or the binary is missing, the launcher shows "Currently unavailable" and `connect` is a no-op.

---

## 2. Module Map [STABLE]

| File | Responsibility |
|---|---|
| `mod.rs` | Module declarations and the door's framing comment. Keep declaration-only. |
| `proxy.rs` | `NethackProcess`: per-session host for the local child. Owns the PTY bridge task (`run_bridge`/`bridge_loop`), the shared `vt100::Parser`, the `ProxyStatus` flag, input/resize command channel, and `nethack_playname`. This is the local-process twin of `door::rebels::proxy::RebelsProxy`. |
| `state.rs` | Per-session `State`: launcher/running `Mode`, config (bin/data_dir/term/enabled), the optional `NethackProcess`, last viewport `Rect`, the post-exit input grace, and input interception/forwarding (`intercept_input` remaps F1→`?`, `forward_input`, `strip_input_noise`). |
| `render.rs` | Ratatui rendering: the `draw_launcher` static page (logo, blurb, hints) and `draw_running` which blits the live `vt100` screen via `rebels::render::blit_screen`. No late.sh-side help overlay — in-game help is NetHack's own `?`. |

Cross-module wiring (outside this folder):
- `app/state.rs`: `App::nethack_state`, `enter_nethack`/`leave_nethack`, and the Running-mode input passthrough in `App::handle_input` (intercept F1, else forward raw bytes).
- `app/input.rs`: launcher `Enter` → `enter_nethack` + `connect`; `7` global screen switch; topbar hit-test columns; arrow handling is a no-op (Running-mode arrows are forwarded raw upstream).
- `app/render.rs`: takes `nethack_state` out (like pinstar/rebels) so the draw path can `set_viewport(content_area)` before blitting; restores it after draw.
- `app/tick.rs`: calls `State::tick()` each app tick to detect process exit.
- `config.rs`, `state.rs` (`SessionConfig`), `ssh.rs`, `session_bootstrap.rs`, `tests/helpers/mod.rs`: thread the three `nethack_*` config fields through.

---

## 3. Screen Lifecycle And Input Capture [STABLE]

- Top-level screen key is `7`, rendered as `NetHack`.
- Entering the screen shows the static launcher. It does **not** auto-spawn the process (`set_screen` calls `enter_nethack`, which only constructs `State`; the child is spawned by `connect`, triggered by launcher `Enter`).
- `Enter` on the launcher calls `App::enter_nethack` then `State::connect`, spawning the child and switching to `Mode::Running`.
- Leaving the screen (`leave_nethack`, on navigating away) drops `nethack_state`; dropping `State` drops `NethackProcess`, whose `Drop` aborts the bridge task, and `kill_on_drop`/`child.kill()` kills the child nethack.
- `State::tick` (each app tick) flips back to `Mode::Launcher` if the process closed for any reason (clean `S` save, death, quit, or crash) — all exits are treated identically.

Input capture contract:
- The **launcher** behaves like a plain page: only `Enter` is consumed (in `handle_dedicated_screen_input`); every other key (`Tab`/digit nav, `q`, `?`, …) falls through to normal global handling. **Exception:** for a short post-exit grace window the launcher swallows *all* input — see the exit-grace gotcha in §9.
- While **Running**, `App::handle_input` intercepts bytes *before* the normal input pipeline: if `state.is_running()`, it calls `intercept_input` (F1 remap) and otherwise `forward_input` straight to the child, then returns. So number keys, `q`, `Esc`, etc. all reach NetHack, not late.sh.
- `F1` (`ESC O P` or `ESC [ 11 ~`) is **remapped to NetHack's own `?` help**: `intercept_input` forwards a literal `?` to the child and swallows the F1 bytes. This both gives F1 the conventional "help" meaning and stops the raw `ESC O P` from leaking into the game as stray commands (`ESC` cancel, `O` options, `P` put-on). There is no late.sh help overlay; `?` works directly too.
- `forward_input` strips mouse reports (SGR `ESC [ < … M/m`, legacy X10 `ESC [ M b x y`) and bracketed-paste markers (`ESC [ 200~`/`201~`). This matters: late.sh keeps any-event mouse tracking (`?1003h`) on for its own UI, and those motion reports' leading `ESC` would otherwise cancel every NetHack menu — stripping them is what makes in-game `?` work. Real keys and arrow escapes pass through verbatim; a sequence truncated at a chunk boundary falls through unchanged rather than swallowing the next keystroke.

---

## 4. PTY Bridge Architecture [STABLE]

### Process and screen

- `NethackProcess::spawn` creates an mpsc command channel, a shared `vt100::Parser` (sized to the initial viewport), a `ProxyStatus` mutex, and spawns the bridge task. On task end it forces `ProxyStatus::Closed` and wakes the render loop.
- `run_bridge` (unix only) allocates a PTY with `openpty`, **disables XON/XOFF flow control** on the slave termios (`IXON`/`IXOFF`/`IXANY` cleared before exec — see §9), builds the `nethack` `Command` with a **cleared environment** (`env_clear`) plus an explicit allowlist (`-u <playname>`, `TERM`/`HOME`/`LINES`/`COLUMNS`) so the child never inherits late-ssh's production secrets, wires the slave to child stdio, and in `pre_exec` calls `setsid` + `TIOCSCTTY` so the child gets its own session with the PTY as controlling terminal (mirrors `late-cli/src/ssh.rs`). Then flips status to `Running`.
- A blocking **reader thread** pumps child output into the `vt100::Parser` and wakes the render `RenderSignal` on each chunk, so new frames repaint promptly.
- `bridge_loop` is a `tokio::select!` over the command channel (input bytes → write to PTY master; resize → `TIOCSWINSZ`) and `child.wait()` (exit → break).
- On exit, status is forced to `Closed` and the render loop woken **before** cleanup. This is deliberate: `tick()` watches the status to return to the launcher. The teardown then kills the child and **detaches** (does NOT join) the reader thread — a save-time compressor grandchild can hold the PTY slave open for seconds after NetHack exits, and a blocking `reader.join()` would pin a runtime worker on it and freeze the return to the launcher (see §9).
- The non-unix `run_bridge` just bails: the NetHack door requires a unix host.

### Sizing

- `State::set_viewport` (called from the draw path with the exact content area) resizes the parser and sends a `Resize` command when the viewport changes; the kernel then signals `SIGWINCH` to the child so curses redraws at the new size.

### Render

- `draw_running` blits the current `vt100` screen with `rebels::render::blit_screen` (shared with the Rebels door — same vt100 model, different transport). Before the process reports `Running` it shows "Starting nethack...". There is no late.sh overlay on top of the game.
- The app frame title bar (chrome above the game, drawn in `app/render.rs::app_frame_title`) shows `· ? help · S save · Ctrl-C quit` **only while a game is running**. It sits outside the game grid so it never covers glyphs, and it points players who skipped the launcher at the leave/help keys instead of mashing Esc.

---

## 5. Launcher And In-Game Help UI [VOLATILE]

- `draw_launcher`: ASCII `NETHACK` logo, a one-line blurb, `saves`/`bones`/`style` stat lines, a Launch action line (`Enter` when enabled, "Currently unavailable" in red when disabled), an "Once Inside" hint block (`? or F1`, `hjkl`, `S`, `Ctrl-C`), and the `nethack.org` URL.
- **No late.sh-authored cheat sheet.** In-game help is NetHack's own `?` menu (and `F1`, which late.sh remaps to `?`). A hand-maintained keybinding card was removed deliberately: it duplicated NetHack's reference and was ours to keep in sync for no real gain. Do not reintroduce one — point at `?`.
- The app frame title shows a dimmed "by nethack.org" credit on this screen, plus the in-game leave/help-key hint while a game is running (see §4 Render).

---

## 6. Configuration And Deploy [VOLATILE]

Three config knobs (env → `Config` → `SessionConfig` → `App`):
- `LATE_NETHACK_ENABLED` (default `false`): when false the door is reachable but `connect` is a no-op and the launcher shows "Currently unavailable".
- `LATE_NETHACK_BIN` (default `/usr/games/nethack`): path to the binary.
- `LATE_NETHACK_DATA_DIR` (default `/var/lib/late-nethack`): mapped **only** to the child's `HOME`, where its `.nethackrc` lives.

Binary sourcing (current decision): **built from verified upstream source — NetHack 5.0.0.**
- The binary is compiled from the official upstream source release in the Dockerfile `nethack-build` stage (Stage 0a), NOT from the `nethack-console` distro package (which lags upstream — bookworm ships 3.6.6). The stage downloads the pinned tarball, verifies its SHA-256 against the checksum published on nethack.org (`sha256sum -c` fails the build closed on mismatch), then runs the canonical 5.0.0 unix build per `sys/unix/NewInstall.unx`: `cd sys/unix && sh setup.sh hints/linux.500` → `make fetch-Lua` → `make … all install`. `PREFIX`/`HACKDIR` are passed as make overrides (resolution confirmed via `make -pn`). Version/URL/checksum are `ARG`s (`NETHACK_VERSION`/`NETHACK_TARBALL`/`NETHACK_URL`/`NETHACK_SHA256`); bump those to change versions.
- The from-source binary installs **into** its playground at `/var/games/nethack/nethack` and self-locates via its compiled-in `-DHACKDIR`. Both the dev `base` and prod `runtime-base` stages `COPY --from=nethack-build /var/games/nethack` (read-only data + binary) **and** `/var/games/nethack-var` (the writable seed), install the runtime lib `libncursesw6`, and symlink that binary to `/usr/games/nethack` so the `LATE_NETHACK_BIN` default resolves. HACKDIR is no longer world-writable; the writable dir is owned by the `late` user (dev keeps it 0777 since it runs as root).
- **Writable state is split out of HACKDIR via `VAR_PLAYGROUND`.** The `nethack-build` stage defines `VAR_PLAYGROUND=/var/games/nethack-var` in `include/unixconf.h` (a fail-closed `grep` guards the `sed`) and passes `VARDIR=/var/games/nethack-var` to `make install`. At runtime NetHack's `unixmain.c::chdirx()` points the six writable path prefixes (save/level/bones/lock/score/trouble) at `VAR_PLAYGROUND` and still `chdir`s to HACKDIR, so read-only data files keep loading from the image while saves/bones land in the separate dir. This is the supported "static playground on a read-only filesystem" mechanism (confirmed in the 5.0.0 source), and it is what lets a PVC back the writable dir without shadowing the image's data files. NetHack never `mkdir`s its own `save/` (no `mkdir` in its source), so the writable dir must be pre-seeded with `save/`: the `make install` `VARDIR` step seeds it in the image, and the K8s `initContainer` seeds it on the PVC.
- We deliberately do **NOT** set `NETHACKDIR`: pointing it at an empty dir makes nethack fail to `chdir` to its data dir. Instead the build bakes `-DHACKDIR=/var/games/nethack` (the override passed at `make` time), so the compile-time data path equals the runtime path. `HOME` (our `LATE_NETHACK_DATA_DIR`) only carries the per-player `.nethackrc`.
- Per-player saves are keyed by the sanitized `-u <playname>`; the shared playground is what lets one player's death seed bones for others.
- Lua: `make fetch-Lua` downloads Lua 5.4.8 over the network but verifies it against the pinned checksums in `submodules/CHKSUMS` shipped inside the already-verified NetHack tarball — integrity-checked, though the build is not offline. Confirm build steps/install paths against the release's own `sys/unix/NewInstall.unx` when bumping versions.

Persistence (prod): saves/bones survive redeploys. `infra/nethack.tf` provisions an RWO `nethack-save` PVC (`local-path`, 2Gi, `prevent_destroy`); `infra/service-ssh.tf` mounts it at `/var/games/nethack-var` (= `VAR_PLAYGROUND`), sets `LATE_NETHACK_ENABLED` from the `NETHACK_ENABLED` var (defaults on), and runs a `nethack-save-seed` initContainer that creates `save/` and `chown`s the tree to `late` before late-ssh starts (the empty PVC shadows the image's baked seed, and NetHack won't create `save/` itself). HACKDIR stays a read-only image layer and is never mounted, so image rebuilds keep shipping fresh data files without clobbering saves. `replicas` must stay 1 (one RWO volume holds shared bones + per-player saves; assumes the single-node `local-path` cluster). Dedicated CI/CD is `.github/workflows/nethack.yml` (build-validate the from-source stage + a manual deploy/seed verify); full infra applies still run via `deploy_infra.yml` → `terraform.yml`. License/source-availability obligations for the from-source binary are tracked in `NOTICE` (NGPL).

---

## 7. Critical Invariants [STABLE]

- The child process is authoritative for game state. late.sh owns only the terminal bytes and a status flag; it stores nothing about NetHack in its own DB.
- While Running, do not route NetHack bytes through the normal late.sh input pipeline. Only `F1` is late.sh's (and it just injects NetHack's `?`); everything else is forwarded raw. Adding global-shortcut handling here would steal keys from the game.
- Keep mouse/paste stripping in `forward_input`. With late.sh's `?1003h` mouse tracking on, unfiltered motion reports cancel NetHack menus.
- Force `ProxyStatus::Closed` and wake the render loop the instant the child exits, before any cleanup, or the screen freezes on the last frame. Do **not** reintroduce a blocking `reader.join()` in the teardown — detach the reader; a lingering save compressor can hold the PTY open and freeze the return to the launcher (§9).
- Keep XON/XOFF flow control **off** on the PTY (`IXON`/`IXOFF`/`IXANY`), or a stray Ctrl-S freezes the game's output until Ctrl-Q (§9).
- Spawn the child with `env_clear()` + an explicit allowlist (`TERM`/`HOME`/`LINES`/`COLUMNS`). late-ssh's own environment carries production secrets (DB/S3/AI/LiveKit/tunnel/rebels); the door child must never inherit them. NetHack's shell (`!`) and suspend (`^Z`) escapes are **compiled out** in the Dockerfile `nethack-build` stage (the `SHELL`/`SUSPEND` defines are removed from `unixconf.h`, with a fail-closed grep), so there's no in-game path to a shell as the service user; the env clear is additional defense in depth.
- `nethack_playname` derives the `-u` name **only from the immutable `user_id`** (`late` + the UUIDv7 random trailing hex), never the username. The name keys the player's save/bones, so any dependence on the mutable username would orphan a character on rename, and usernames that strip to the same alphanumerics (`bob_1` vs `bob1`) would collide. It must stay unique per account, stable forever, PTY-safe, and within NetHack's `PL_NSIZ` (32 → 31 usable). Trade-off: bones/ghost names are opaque (`late9f3c…`), not the username.
- Treat all child exits identically — clean save, death, quit, crash all return to the launcher.
- When disabled or the binary is missing, fail soft (launcher message + no-op connect), never panic.
- `mod.rs` stays declaration-only; the `door` folder is a grouping folder — keep NetHack-specific behavior in this context, not a `door/CONTEXT.md`.

---

## 8. Tests And Verification [STABLE]

Root policy applies: agents should not run `cargo test`/`cargo nextest`/`cargo clippy`; leave blocking verification to the human owner. If a change needs verification, mention the focused command in handoff.

Inline pure tests currently cover:
- `proxy.rs`: `nethack_playname` (account-derived + PTY-safe + within `PL_NSIZ`, stable per account, distinct names for distinct accounts).
- `state.rs`: `connect` is a no-op when disabled; `forward_input` without a proxy is a no-op; `strip_input_noise` drops mouse/paste but keeps keys and arrows; F1 (both encodings) is consumed while other keys pass through (`f1_is_consumed_and_other_keys_pass_through`); `is_f1` matches both encodings; the exit-grace opens on process close and counts down to clear (`exit_grace_opens_on_close_and_counts_down`).
- `app/common/primitives.rs` and `app/input.rs`: screen `next`/`prev` ordering and topbar hit-test columns include `Nethack` between `Rebels` and `Pinstar`.

The PTY bridge (`run_bridge`/`bridge_loop`) is unix-process-bound and not unit-tested; the `repaint` field is `None` on headless/test paths. Verify launch/save/quit behavior manually against a real binary.

Expected focused command for human verification after NetHack door changes:

```bash
cargo test -p late-ssh nethack
```

---

## 9. Known Gotchas And Future Work [VOLATILE]

These three bit us on the exit path and are easy to reintroduce; the guards are intentional.

- **Trailing game keys can quit the whole app (exit-grace).** NetHack's end-of-game disclosure (`--More--`, `identify? [ynq]`, …) makes players mash `q`/space. The game exits partway through that burst, and the *remaining* queued keys land on the launcher, where `q` is wired to the **global** app-quit (`input.rs` → `trigger_global_quit` → `app.running = false`) — dropping the whole SSH session (and any paired CLI). Guard: when a game exits, `State::tick` opens a short input-grace (`EXIT_GRACE_TICKS`, ~0.66s at the 66ms world tick); while `State::in_exit_grace()` is true, `App::handle_input` swallows launcher input instead of letting it fall through. `connect` resets the grace. If you change the launcher's global-key fall-through or the world-tick rate, re-check this window.
- **A save-time compressor holds the PTY open after NetHack exits.** On `S`+`y` the NetHack process exits almost instantly (the `nethack child exited` debug log fires ~10ms after the keypress), but it hands the save file to an external compressor that *inherits the PTY slave* and can run for many seconds (worse on slow container storage). The PTY does not hit EOF until that grandchild dies, so the reader thread's blocking `read()` lingers. A blocking `reader.join()` in the teardown then pins a Tokio worker on it; on a CPU-limited host that starves the render loop, so world ticks stop, `tick()` never flips back to the launcher, and the screen freezes for the compressor's whole runtime (~10s observed). Guard: the teardown **detaches** the reader (no join). Status is already `Closed`, so the launcher returns immediately; the detached reader exits on its own at EOF. Do not "tidy up" by joining it.
- **Ctrl-S freezes the game (XON/XOFF flow control).** A stray Ctrl-S from the client is XOFF: the PTY line discipline pauses the child's output until an XON (Ctrl-Q) arrives, so the game looks hung and glyphs garble when output finally resumes (Ctrl-C jolts it loose). NetHack has no use for XON/XOFF. Guard: `run_bridge` clears `IXON`/`IXOFF`/`IXANY` on the slave termios **before exec** so Ctrl-S passes through as an ordinary (ignored) key; cbreak-mode curses like NetHack's tty window-port don't turn it back on.

- No late.sh persistence layer: everything durable is in NetHack's own playground on disk. Save recovery after a dropped SSH session depends on NetHack's own save/recover, not late.sh.
- Read-only data files + the binary are baked into the image at HACKDIR (`/var/games/nethack`) and refreshed on every rebuild; the **writable** state (saves/bones/locks/record) lives at `VAR_PLAYGROUND` (`/var/games/nethack-var`), backed in prod by the `nethack-save` PVC so it survives redeploys (see §6). Dev/unmounted runs fall back to the image's baked writable seed (ephemeral).
- `NETHACKDIR` must stay unset; overriding it to an empty dir breaks the child's chdir.
- Multiple concurrent sessions for the same user would share the same `-u` save name; NetHack itself guards a save with a lock, so a second concurrent launch may refuse to load. Not specially handled here.
- **Process-count envelope (no NetHack-specific cap).** Each session spawns at most one NetHack child (`connect` is a no-op when `proxy.is_some()`), and concurrent SSH sessions are bounded by the server's global `conn_limit` semaphore (`max_conns_global`) plus the per-IP cap (`state.rs`). So NetHack children are bounded 1:1 by SSH concurrency, not unbounded; the pod CPU/memory limits are the backstop (an overload OOM-kills and self-heals the single replica). There is deliberately **no** separate per-user/global NetHack process cap — the SSH + pod limits are the accepted envelope for the default-on door. If that envelope becomes too loose (e.g. `max_conns_global` × per-child memory approaches the pod limit), add a dedicated cap in `connect` backed by a shared counter (decremented on `NethackProcess` drop) rather than relying on OOM.
- Binary is built from verified upstream source (NetHack 5.0.0) in the Dockerfile `nethack-build` stage; the build is not fully hermetic because it fetches Lua over the network (see §6). When bumping versions, update the `NETHACK_*` Dockerfile `ARG`s (incl. the verified `NETHACK_SHA256`) and `NOTICE`.

### Future work: run NetHack in its own pod (planned)

Today NetHack runs as a **local child** inside the `service-ssh` container: `proxy.rs` does `openpty()` + `Command::spawn()`, so the game shares the SSH service's container, env (mitigated by `env_clear`), resource limits, and blast radius. The planned next step is to move it to a **dedicated pod**, which is the real (vs. mitigated) fix for the secret-boundary concern and also gives independent resource limits, an isolated blast radius (a NetHack OOM/crash no longer drops SSH sessions), and natural per-game concurrency caps.

Why it's a rearchitecture, not a flag: a PTY/child can't cross containers, so NetHack stops being a local child and becomes a **network-proxied door** — i.e. it moves into the *Rebels* camp (`door::rebels`, remote SSH proxy) instead of the local-process camp this file describes. Reuse that machinery rather than inventing a new transport.

Migration outline (do after this PR merges):
1. **NetHack server pod** — own Deployment + Service running `dgamelaunch` (the alt.org/nethack approach) or a minimal `sshd` + NetHack wrapper. Build on the existing `nethack-build` artifact.
2. **Identity/auth bridge** — the `-u <playname>` is currently a trusted CLI arg to a local child; remotely, late-ssh must tell the server "this session is account X, use playname Y" over a trusted channel. Model it on the Rebels door's shared-secret identity (`LATE_REBELS_SECRET`). Keep `nethack_playname` (account-id-derived) as the identity.
3. **Transport swap** — retire the `openpty`/`Command` path in `proxy.rs`; reuse the Rebels SSH-proxy transport to bridge the remote vt100 stream.
4. **Move the infra** — the `nethack-save` PVC + the `nethack-save-seed` initContainer move from `service-ssh` to the new pod (`infra/nethack.tf`); `service-ssh` only needs network reach. The PVC RWO/replicas-1 and seeding rules (§6) carry over unchanged; concurrency caps become the new pod's own limits (supersedes the envelope note above).
5. **Docs** — once moved, NetHack is a remote door; update §1/§2/§4 (no longer local-process) and `NOTICE` if the build/runtime split changes.

Deferred deliberately: the in-process model is adequately safe for the current launch (escapes compiled out, env cleared, envelope documented) and far simpler to operate. Pick this up when true multi-tenant isolation or independent NetHack scaling is wanted.
