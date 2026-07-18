# DCSS Door Context

## Metadata
- Scope: the Dungeon Crawl Stone Soup door as a whole — the **client** in `late-ssh/src/app/door/dcss` (proxy/identity/state/render/mod) plus its screen lifecycle wiring in `late-ssh/src/app` (state/input/render/tick) **and the standalone host crate `late-dcss/`**. There is no separate `late-dcss/CONTEXT.md`; this file is the single source for both halves.
- Domain: Dungeon Crawl Stone Soup (crawl), the real upstream console roguelike (GPL-2.0-or-later), run on a PTY inside a **dedicated `late-dcss` SSH host** and reached by late-ssh as a network-proxied door (the same model as the NetHack door).
- Primary audience: LLM agents changing the DCSS launcher UI, the SSH client transport, the host crate (PTY bridge / auth / TERM handling), input forwarding, or its config/deploy wiring.
- Last updated: 2026-07-18 (initial build of the door)
- Status: Active
- Parent context: `../../../../../CONTEXT.md`
- Stability note: `[STABLE]` sections change rarely; `[VOLATILE]` sections change with the launcher UI, keybindings, or build/deploy wiring.

---

## 0. Context Maintenance Protocol [STABLE]

Read this after root `CONTEXT.md` whenever a task touches the DCSS launcher, launch/leave behavior, the SSH client transport, the `late-dcss` host (PTY bridge, auth, TERM resolution), input forwarding/filtering, the F1→`?` help remap, or DCSS config/deploy wiring.

- Keep this file aligned with the SSH transport contract, the client/host split, the spawn args, config knobs, and known gotchas.
- Update root `CONTEXT.md` when routing, the top-level screen list/tab order, or global keybindings change.
- Treat tests and code as authoritative when comments drift; patch stale comments or this file before handoff.
- Do not add `pub use` re-export layers; `mod.rs` stays declaration-only.

---

## 1. Summary [STABLE]

DCSS runs the **real upstream crawl console binary on a PTY**, but **not** inside late-ssh. It lives in its own crate/pod, `late-dcss`, a minimal russh **server** that spawns one `crawl` child per SSH session. late-ssh reaches it exactly like the NetHack door reaches `late-nethack`: the door is a russh **client** that streams the remote terminal through a `vt100::Parser` and blits it into a ratatui widget below the top bar. SSH *is* the transport — there is no custom IPC.

This door was born network-proxied (no local-child history): it is a deliberate near-clone of the NetHack door, built while that machinery was fresh, because crawl is the same shape of program — a curses roguelike with per-player saves and a SIGHUP hangup-save (the behavior every public dgamelaunch server relies on).

Core shape:
- `Screen::Dcss` has no top-level number key. It is reached by selecting the DCSS card in the Games hub (page `3`, after NetHack) and pressing `Enter`. `Enter` constructs the `State` (`enter_dcss`) and `connect`s — opening the SSH connection and switching to `Mode::Running` — in one step; the standalone launcher render is normally skipped.
- One per-session `DcssProcess` (a russh client; the twin of `door::nethack::proxy::NethackProcess`) owns a background Tokio task that connects to `late-dcss`, requests a PTY + shell, and bridges the remote bytes into a shared `vt100::Parser`. The foreground reads that screen and a `ProxyStatus` flag.
- **Identity vs authorization are split, exactly like nethack.** The connection authenticates with a single Ed25519 key both ends derive from `LATE_DCSS_SECRET` (authorization; blake3 domain `late.sh/dcss/v1`). The account's **arcade handle** travels as the **SSH username** (identity); the host re-sanitizes it (`playname::sanitize`) and passes it as crawl's `-name`, which keys the per-player save and skips crawl's name prompt.
- **The arcade handle is a user-chosen, immutable, per-account public name** (`late-core` `models::arcade_handle`; migration 120): claimed once from the DCSS launcher's first-run prompt, unique case-insensitively (unique index on `lower(handle)`), shape `[A-Za-z][A-Za-z0-9_]{2,19}` (`handle_shape_valid`, a strict subset of crawl's name rules and the host sanitizer), `late`/`late_*` reserved (`handle_reserved`; those shapes are NetHack's live derived playnames). Rows outlive the account (`user_id` set NULL on user deletion) so a dead account's handle can never be re-claimed to open its saves. The launcher state machine (`HandleStatus`: Loading → Missing/Claimed/Failed, Claiming) lives in `state.rs` behind an `Arc<Mutex<..>>` written by background tasks via `ArcadeHandleService` (`door/arcade.rs`, threaded through `SessionConfig` like the other door services); `launch_pending` carries hub-Enter intent through the async lookup so returning players still launch in one keypress, and a fresh claim auto-launches. `tick.rs` holds the Dcss screen (`awaiting_handle`) while the prompt/lookup is up. The whole launcher flow lives in `door/arcade.rs` (`HandleFlow`, `HandleStatus`, `HandleKeyResult`) plus `landing::handle_launch_block` for the shared prompt UI; the NetHack door uses the same flow (its legacy `late_<hex>` saves were deliberately orphaned).
- The child is spawned `crawl -name <playname>` plus `-extra-opt-last` display defaults (`view_max_width=81`, `view_max_height=71` so the map viewport grows with the terminal instead of crawl's 33x21 default; `use_terminal_default_colours=true` so the late.sh theme background shows through instead of crawl painting every cell ANSI black), with `env_clear()` + allowlist (`TERM`, `HOME` = the shared playground, `LANG`/`LC_ALL=C.UTF-8` for crawl's Unicode glyphs). `LINES`/`COLUMNS` are deliberately NOT exported: ncurses treats them as an override of the pty size and then ignores SIGWINCH, freezing crawl at its spawn-time geometry; the pty winsize (openpty + TIOCSWINSZ on `window_change`) is the only size source. SAVEDIR is the build default `~/.crawl`, so all writable state (saves keyed by name, shared scores/logfile/milestones, morgue dumps) lands under `$HOME/.crawl` on the host's PVC.
- While Running, raw client bytes are forwarded straight to the host→child (minus mouse/paste noise), so crawl — not late.sh — interprets keys. `F1` is the only key late.sh keeps, remapped to crawl's own `?` help (same rationale and encodings as nethack's remap).
- **Teardown SIGHUP-saves.** On client disconnect or host SIGTERM with a live child, the host bridge sends SIGHUP so crawl saves-and-exits (resumable next launch), with a 5s grace then SIGKILL backstop; a pod SIGTERM broadcasts a `watch` channel and `main.rs` holds the process for `SHUTDOWN_GRACE` (8s) so saves land. Unlike nethack there is **no getlock-slot wedge to defend against** — a stale crawl lock only blocks that one player's save (crawl offers recovery), not the whole door — but the SIGHUP-save still matters so players don't lose runs to rollouts.
- **No milestones, chips, or awards in v1.** There is no screen scraping and no late.sh-side persistence. crawl's own `~/.crawl/logfile` + `milestones` files on the host are machine-readable (unlike nethack's vt100-scrape situation), so a future award pipe should read those host-side rather than scraping — deferred (see §9).

The door is gated behind `LATE_DCSS_ENABLED` (default `false`); when disabled, `connect` is a no-op and the launcher shows "Currently unavailable". The host pod is deployed unconditionally (the flag gates only the client).

---

## 2. Module Map [STABLE]

### Client — `late-ssh/src/app/door/dcss/`

| File | Responsibility |
|---|---|
| `mod.rs` | Module declarations + framing comment. Declaration-only. |
| `proxy.rs` | `DcssProcess`: per-session russh **client** to the host. Owns the bridge task (`run_bridge`), the shared `vt100::Parser`, the `ProxyStatus` flag, and the input/resize command channel; `ProcessConfig.playname` carries the arcade handle. Near-clone of `door::nethack::proxy`. |
| `identity.rs` | `derive_client_key(secret)`: the shared-secret → Ed25519 key derivation (blake3, domain `late.sh/dcss/v1`). Must stay byte-identical to the host's copy (see the cross-crate note in the source). |
| `state.rs` | Per-session `State`: launcher/running `Mode`, connection config (host/port/secret/term/enabled), the optional `DcssProcess`, last viewport `Rect`, the post-exit input grace, `connect`, `set_viewport`, `intercept_input` (F1→`?`), `forward_input`/`strip_input_noise`, and `tick` (flips back to Launcher on close). No award/milestone scraping. |
| `render.rs` | Ratatui rendering: `draw_landing`/`draw_launcher` (CRAWL logo, blurb, dungeon strip, hints) and `draw_running` which blits the live `vt100` screen via `rebels::render::blit_screen`. No late.sh help overlay — in-game help is crawl's own `?`. |

### Host — `late-dcss/` crate (standalone binary)

| File | Responsibility |
|---|---|
| `main.rs` | Tracing init, `Config::from_env`, generate the ephemeral SSH host key, run the russh server. Broadcasts a shutdown `watch` on SIGTERM/SIGINT and holds the process for `SHUTDOWN_GRACE` so live games hangup-save. |
| `config.rs` | `Config`: `bin` (default `/usr/games/crawl`), `data_dir` (the child `HOME`, default `/var/lib/late-dcss`), `secret`, listen addr/port, idle timeout. |
| `server.rs` | russh `Server`/`ClientHandler`: `auth_publickey` (compares key DATA — see the nethack CONTEXT §7 comment-field gotcha), `pty_request`, `shell_request`, `data`, `window_change_request`, `channel_eof/close`. Holds `effective_term` (TERM fallback). |
| `host.rs` | `PtyHost`: the per-session PTY bridge — `openpty` + `env_clear` + `setsid`/`TIOCSCTTY` + `IXON/IXOFF/IXANY` clear + `TIOCSWINSZ` + the detached reader, plus the `StopReason` teardown (SIGHUP-save on `Teardown`, plain close on `ChildExited`). Same shape as late-nethack's `host.rs`. |
| `identity.rs` | `derive_client_key(secret)` — identical to the client copy. |
| `playname.rs` | `sanitize(username)`: keep `[A-Za-z0-9_]`, cap at crawl's `MAX_NAME_LENGTH` (30), fall back to `late`. Defense-in-depth on the `-name` arg. |

Cross-module wiring (client side, outside this folder) mirrors nethack exactly: `app/state.rs` (`dcss_state`/`enter_dcss`/`leave_dcss`, Running-mode passthrough with F1 intercept + exit grace), `app/input.rs` (hub launch, launcher Enter, arrow no-op), `app/render.rs` (state taken out for `set_viewport` before blitting; `by crawl.develz.org` chrome + in-game key hints), `app/tick.rs` (tick + return-to-hub), `config.rs`/`state.rs` (`SessionConfig`)/`ssh.rs`/`session_bootstrap.rs`/`tests/helpers` (the `dcss_enabled`/`dcss_host`/`dcss_port`/`dcss_secret` fields), hub `state.rs`/`ui.rs` (the `HubGame::Dcss` card).

---

## 3. Config And Deploy [VOLATILE]

### Client (env → `Config` → `SessionConfig` → `App`)
- `LATE_DCSS_ENABLED` (default `false`), `LATE_DCSS_HOST` (default `127.0.0.1`; compose `service-dcss`, prod `late-dcss-sv`), `LATE_DCSS_PORT` (default `2325`), `LATE_DCSS_SECRET` (must equal the host's; required when enabled).

### Host (`late-dcss` env)
- `LATE_DCSS_SECRET` (required), `LATE_DCSS_BIN` (default `/usr/games/crawl`), `LATE_DCSS_DATA_DIR` (default `/var/lib/late-dcss`; the child `HOME` = the PVC in prod), `LATE_DCSS_LISTEN_ADDR`, `LATE_DCSS_PORT` (default `2325`), `LATE_DCSS_IDLE_TIMEOUT`.

### Binary sourcing — built from verified upstream source, DCSS 0.34.1
- Compiled in the Dockerfile `dcss-build` stage (NOT the distro `crawl` package, which lags: bookworm ships 0.29). The stage downloads the pinned release tarball from GitHub, verifies SHA-256 (`sha256sum -c`, fail-closed), then runs the release's own documented install (`make install prefix=/opt/dcss NOWIZARD=y`, console build — tiles needs an explicit `TILES=y` we never pass; `NOWIZARD=y` compiles out wizard/cheat mode for untrusted players, asserted fail-closed via the binary's `-version` CFLAGS line). Version/URL/checksum are `ARG`s (`DCSS_*`).
- The install bakes `DATADIR=/opt/dcss/data` into the binary and leaves `SAVEDIR` at the default `~/.crawl`, so the data tree is a read-only image layer while all writable state follows the child's `HOME`.
- No source is patched (contrast nethack's unixconf.h edits). crawl has no in-game shell escape to compile out; the one hardening knob is `NOWIZARD=y` above.

### Images / infra / CI
- `runtime-dcss` stage: crawl runtime libs (`libncursesw6`, `liblua5.4-0`, `libsqlite3-0`) + `ncurses-term`, the `/opt/dcss` tree, `/usr/games/crawl` symlink, `/var/lib/late-dcss` chowned to `late`. `base` gets the same tree for `dev-dcss` (compose `service-dcss`).
- `infra/dcss.tf`: the RWO `dcss-save` PVC (2Gi, `prevent_destroy`) + locals (enable flag, host/port, `dcss_var_path`). `infra/service-dcss.tf`: the `late-dcss` Deployment (replicas **1**, kill-before-create, `terminationGracePeriodSeconds=30` > `SHUTDOWN_GRACE`, `dcss-save-seed` initContainer that only chowns the mount — crawl mkdirs its own `~/.crawl`) + the `late-dcss-sv` ClusterIP Service on 2325. `infra/secrets.tf`: `dcss-identity-secret` injected into both service-ssh and late-dcss.
- CI: `.github/workflows/deploy_dcss.yml` (the `-dcss` release-tag suffix; builds + pushes `runtime-dcss`, bootstrap path), `dcss.yml` (PR/weekly build-validate of the `dcss-build` + `runtime-dcss` stages, manual verify_deployed). `deploy.yml`/`deploy_web.yml`/`deploy_infra.yml`/`deploy_nethack.yml`/`deploy_dopewars.yml` each read the live `late-dcss` image tag and pass it through `terraform.yml`'s required `dcss_image_tag`. **First rollout must be `deploy_dcss.yml`** (it builds the image); a normal deploy first would fail the image lookup. License obligations tracked in `NOTICE` (GPL-2.0-or-later).

---

## 4. Critical Invariants [STABLE]

Most of the NetHack door's invariants (§7 of its CONTEXT) apply verbatim — auth compares key DATA not the struct, `derive_client_key` byte-identical across crates, `env_clear` + allowlist, XON/XOFF off, close-channel-then-detach-reader, force `ProxyStatus::Closed` + wake render on close, all exits treated identically, fail-soft when disabled, `mod.rs` declaration-only. One deliberate divergence: the playname is the user-chosen arcade handle, not a derived hash (immutability moves from the derivation to the claim-once rule). DCSS-specific:

- The `-name` playname keys the save; the arcade handle is therefore immutable once claimed (no rename path exists on purpose), and a claimed handle row must never be deleted or reassigned. It must stay ≤30 chars (crawl rejects longer names at the prompt — and `-name` bypasses the prompt, so an over-long name would misbehave, not error cleanly); `HANDLE_MAX_LEN` 20 keeps margin.
- Keep `LANG`/`LC_ALL=C.UTF-8` in the child env: crawl's map uses Unicode glyphs through ncursesw, and a POSIX locale degrades the whole dungeon to mojibake.
- On teardown with a live child, SIGHUP before any SIGKILL so the run is saved; `PtyHost::Drop` must not abort the bridge task. (Stakes are lower than nethack — no door-wide wedge — but a SIGKILLed run is a lost game and crawl's next launch shows a crash-recovery prompt.)
- crawl wants ≥80x24; below that it draws a "terminal too small" notice itself. Don't add a late.sh-side size gate — the game's own messaging is the contract.

---

## 5. Tests And Verification [STABLE]

Root policy applies: agents run `cargo check --tests`; humans run the suite.

Inline pure tests cover: `late-core` `models::arcade_handle` (shape + reserved rules), client+host `identity.rs` (derivation determinism + distinctness; add a cross-crate KAT fingerprint from the first real test run, mirroring nethack's), client `state.rs` (disabled connect no-op, forward without proxy no-op, mouse/paste stripping, F1 both encodings, exit grace, claim-prompt byte handling/caps/validation errors), host `playname.rs` (sanitize), host `server.rs` (`effective_term` fallback), hub `state.rs` (card order incl. DCSS).

```bash
cargo test -p late-dcss && cargo test -p late-ssh dcss
```

The PTY bridge and russh loops are process/network-bound and not unit-tested; verify launch/save/quit manually against a real host (compose `service-dcss`).

---

## 9. Deferred / Future Work [VOLATILE]

- **Milestone awards from the host's files, not the screen.** crawl appends machine-readable lines to `~/.crawl/logfile` (game ends: wins with `ktyp=winning`) and `~/.crawl/milestones` (rune pickups, Zot entry, orb pickup) — a spoof-proof source nethack never had. An award pipe (chips/badges for a rune, the Orb, a win) should read those host-side and signal late-ssh, rather than scraping vt100. Needs a cross-crate signal path that deliberately doesn't exist yet.
- Shared ghosts/scores already work (one playground), but nothing surfaces the shared scoreboard in the landing. crawl's `-scores` flag could feed it.
- A KAT fingerprint test pinning the `late.sh/dcss/v1` derivation across the two crates (currently guarded by the weaker determinism tests + comments).
- Per-user/global concurrency cap on the host pod if the envelope gets too loose (same posture as nethack: bounded 1:1 by late-ssh's conn caps today).
