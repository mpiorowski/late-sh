# Brogue Door Context

## Metadata
- Scope: the Brogue CE door as a whole: the **client** in `late-ssh/src/app/door/brogue` (proxy/identity/state/render/mod) plus its screen lifecycle wiring in `late-ssh/src/app` (state/input/render/tick) **and the standalone host crate `late-brogue/`**. There is no separate `late-brogue/CONTEXT.md`; this file is the single source for both halves.
- Domain: Brogue Community Edition (AGPL-3.0), the real upstream curses roguelike, run on a PTY inside a **dedicated `late-brogue` SSH host** and reached by late-ssh as a network-proxied door (the same model as the DCSS and NetHack doors).
- Primary audience: LLM agents changing the Brogue launcher UI, the SSH client transport, the host crate (PTY bridge / auth / TERM handling / per-player cwd), input forwarding, or its config/deploy wiring.
- Last updated: 2026-07-21 (initial build of the door)
- Status: Active
- Parent context: `../../../../../CONTEXT.md`
- Stability note: `[STABLE]` sections change rarely; `[VOLATILE]` sections change with the launcher UI, keybindings, or build/deploy wiring.

---

## 0. Context Maintenance Protocol [STABLE]

Read this after root `CONTEXT.md` whenever a task touches the Brogue launcher, launch/leave behavior, the SSH client transport, the `late-brogue` host (PTY bridge, auth, TERM resolution, player directories), input forwarding/filtering, the F1→`?` help remap, the hangup-save source patch, or Brogue config/deploy wiring.

- Keep this file aligned with the SSH transport contract, the client/host split, the spawn recipe, the config knobs, and known gotchas.
- Update root `CONTEXT.md` when routing, the top-level screen list/tab order, or global keybindings change.
- Treat tests and code as authoritative when comments drift; patch stale comments or this file before handoff.
- Do not add `pub use` re-export layers; `mod.rs` stays declaration-only.

---

## 1. Summary [STABLE]

Brogue runs the **real upstream Brogue CE curses binary on a PTY**, but **not** inside late-ssh. It lives in its own crate/pod, `late-brogue`, a minimal russh **server** that spawns one `brogue` child per SSH session. late-ssh reaches it exactly like the DCSS door reaches `late-dcss`: the door is a russh **client** that streams the remote terminal through a `vt100::Parser` and blits it into a ratatui widget below the top bar. SSH *is* the transport; there is no custom IPC.

The door is a deliberate near-clone of the DCSS door (built per DOOR.md's "reuse the shape almost verbatim" call), with two Brogue-specific divergences:

- **Identity is a per-player working directory, not a name flag.** brogue has no `-name`: it opens every player file (saves `*.broguesave`, recordings, high scores, run history, `keymap.txt`) relative to its cwd. The host therefore runs each child with cwd `LATE_BROGUE_DATA_DIR/players/<playname>` (created on demand; `host.rs::player_dir`), the same model dgamelaunch servers use. Saves AND high scores are per player; nothing is shared between players, and there is no shared scoreboard in v1.
- **The hangup-save is ours.** Upstream's curses build dies unsaved on SIGHUP (only the SDL window-close path auto-saves). The `brogue-build` stage (`docker/doors/brogue.Dockerfile`) applies `scripts/brogue_hangup_save.patch`, which installs a SIGHUP/SIGTERM handler running upstream's own prompt-free `quitImmediately()` save path (flush recording, write the suspend `.broguesave`, `_exit`). Verified by starting a game on a PTY, SIGHUPing, and finding the save on disk. Everything downstream (the host's SIGHUP-then-SIGKILL teardown, the pod's SHUTDOWN_GRACE drain) relies on this patch; re-verify it on Brogue CE version bumps.

Core shape (all inherited from the DCSS door; its CONTEXT §1 applies):
- `Screen::Brogue` has no top-level number key; it is reached by selecting the Brogue card in the Games hub (page `3`, after DCSS) and pressing `Enter`, which constructs the `State` (`enter_brogue`) and `connect`s in one step.
- One per-session `BrogueProcess` (russh client; twin of `door::dcss::proxy::DcssProcess`) owns the bridge task, the shared `vt100::Parser`, and the `ProxyStatus` flag.
- **Identity vs authorization split:** the connection authenticates with the Ed25519 key both ends derive from `LATE_BROGUE_SECRET` (blake3 domain `late.sh/brogue/v1`); the account's **arcade handle** (shared claim-once flow in `door/arcade.rs`, same handle as NetHack/DCSS/Usurper) travels as the SSH username and, after `playname::sanitize`, becomes the player-directory name. The handle is immutable once claimed, so a rename can never orphan a save directory.
- The child is spawned with `env_clear()` + allowlist (`TERM` via the host's `effective_term` fallback, `HOME` = the player dir, `COLORTERM=truecolor`). No locale vars: the curses build renders pure ASCII (`glyphToAscii`, plain `-lncurses`). `LINES`/`COLUMNS` are deliberately NOT exported (ncurses would freeze at spawn-time geometry); the pty winsize (openpty + TIOCSWINSZ on `window_change`) is the only size source. No CLI args at all: players cannot pass flags, so wizard mode is unreachable from the command line (it remains reachable from brogue's own main-menu mode picker; see §9 on awards).
- **Truecolor out, black keyed out.** `COLORTERM=truecolor` selects term.c's 24-bit path (exact `48;2;r;g;b` per cell instead of 6x6x6 cube coercion), preserving brogue's color gradients. Brogue paints every cell an explicit color and never emits the terminal default, so `render.rs::clear_canvas_black` maps background `Rgb(0,0,0)` (and `Indexed(16)`, cube black, for a host still on the 256 path) to `Reset` after the blit; the late.sh theme background shows through empty space, matching how NetHack/DCSS default-background cells render.
- **HVP normalized to CUP before parsing.** term.c's 24-bit renderer (`buffer_render_24bit`) positions the cursor exclusively with HVP (`ESC [ r ; c f`), which the `vt100` crate does not implement: it drops the move and the whole frame smears sequentially across the grid (the 256-color path never hit this because ncurses positions via CUP, `ESC [ r ; c H`). `proxy.rs::HvpNormalizer` rewrites the `f` final byte to the semantically identical `H` before `parser.process`, carrying split-sequence tails across SSH chunks. The root cause is pinned by `proxy_test.rs::vt100_drops_hvp_without_normalizer`: when a vt100 upgrade makes that test fail, HVP is supported upstream and the normalizer can be deleted.
- While Running, raw client bytes are forwarded (minus mouse/paste noise, `strip_input_noise`); `F1` is remapped to brogue's own `?` commands/help menu. Ctrl-C arrives as a plain key (brogue runs curses `raw()` mode and the pty's ISIG never fires from the client side); `S` saves, `Q` abandons.
- **Teardown SIGHUP-saves** (via our patch): client disconnect or host SIGTERM with a live child sends SIGHUP, 5s grace, SIGKILL backstop; pod SIGTERM broadcasts a `watch` channel and `main.rs` holds the process for `SHUTDOWN_GRACE` (8s).
- **No milestones, chips, or awards in v1** (like dopewars/DCSS v1). brogue keeps a per-player machine-readable run history file in each player dir; a future award pipe should read those host-side rather than scraping vt100 (see §9).
- Brogue draws a fixed 100x34 cell UI; on smaller terminals ncurses clips the right/bottom edge (playable but cropped, no in-game warning). The launcher copy says "roomiest at 100x34". No late.sh-side size gate.

The door is gated behind `LATE_BROGUE_ENABLED` (default `false`); when disabled, `connect` is a no-op and the launcher shows "Currently unavailable". The host pod is deployed unconditionally (the flag gates only the client).

---

## 2. Module Map [STABLE]

### Client — `late-ssh/src/app/door/brogue/`

| File | Responsibility |
|---|---|
| `mod.rs` | Module declarations + framing comment. Declaration-only. |
| `proxy.rs` | `BrogueProcess`: per-session russh **client** to the host. Owns the bridge task (`run_bridge`), the shared `vt100::Parser`, the `ProxyStatus` flag, and the input/resize command channel; `ProcessConfig.playname` carries the arcade handle. Near-clone of `door::dcss::proxy`. |
| `identity.rs` | `derive_client_key(secret)`: the shared-secret → Ed25519 key derivation (blake3, domain `late.sh/brogue/v1`). Must stay byte-identical to the host's copy (KAT-pinned on both sides). |
| `state.rs` | Per-session `State` (+ `StateConfig` constructor input): launcher/running `Mode`, connection config, the optional `BrogueProcess`, last viewport `Rect`, the post-exit input grace, `connect`, `set_viewport`, `intercept_input` (F1→`?`), `forward_input`/`strip_input_noise`, and `tick`. No award/milestone scraping. |
| `render.rs` | Ratatui rendering: `draw_landing`/`draw_launcher` (BROGUE logo, blurb, dungeon strip, hints) and `draw_running` which blits the live `vt100` screen via `rebels::render::blit_screen`, then keys out brogue's canvas black (`clear_canvas_black`, see §1). In-game help is brogue's own `?`. |

### Host — `late-brogue/` crate (standalone binary)

| File | Responsibility |
|---|---|
| `main.rs` | Tracing init, `Config::from_env`, ephemeral SSH host key, run the russh server. Broadcasts a shutdown `watch` on SIGTERM/SIGINT and holds the process for `SHUTDOWN_GRACE` so live games hangup-save. |
| `config.rs` | `Config`: `bin` (default `/usr/games/brogue`), `data_dir` (playground root, default `/var/lib/late-brogue`), `secret`, listen addr/port (default 2327), idle timeout. |
| `server.rs` | russh `Server`/`ClientHandler`: `auth_publickey` (compares key DATA; see the nethack CONTEXT §7 comment-field gotcha), `pty_request`, `shell_request`, `data`, `window_change_request`, `channel_eof/close`. Holds `effective_term` (TERM fallback). |
| `host.rs` | `PtyHost`: the per-session PTY bridge (openpty + `env_clear` + `setsid`/`TIOCSCTTY` + XON/XOFF clear + TIOCSWINSZ + detached reader) plus the `StopReason` teardown (SIGHUP-save on `Teardown`, plain close on `ChildExited`) and `player_dir` (the per-player cwd). Same shape as late-dcss's `host.rs`. |
| `identity.rs` | `derive_client_key(secret)`, identical to the client copy. |
| `playname.rs` | `sanitize(username)`: keep `[A-Za-z0-9_]`, cap at 30, fall back to `late`. Because the playname becomes a **path component**, this is the traversal defense: `/` and `.` can never survive. |

Cross-module wiring (client side, outside this folder) mirrors DCSS exactly: `app/state.rs` (`brogue_state`/`enter_brogue`/`leave_brogue`, Running-mode passthrough with F1 intercept + exit grace), `app/input.rs` (hub launch, launcher keys, claim-modal bytes, arrow no-op), `app/render.rs` (viewport + blit, `by tmewett/BrogueCE` chrome + in-game key hints, name modal), `app/tick.rs` (tick + return-to-hub), `config.rs`/`app/state.rs` (`SessionConfig`)/`ssh.rs`/`session_bootstrap.rs`/`src/test_helpers.rs` (the `brogue_enabled`/`brogue_host`/`brogue_port`/`brogue_secret` fields), hub `state.rs`/`ui.rs` (the `HubGame::Brogue` card).

---

## 3. Config And Deploy [VOLATILE]

### Client (env → `Config` → `SessionConfig` → `App`)
- `LATE_BROGUE_ENABLED` (default `false`), `LATE_BROGUE_HOST` (default `127.0.0.1`; compose `service-brogue`, prod `late-brogue-sv`), `LATE_BROGUE_PORT` (default `2327`), `LATE_BROGUE_SECRET` (must equal the host's; required when enabled).

### Host (`late-brogue` env)
- `LATE_BROGUE_SECRET` (required), `LATE_BROGUE_BIN` (default `/usr/games/brogue`), `LATE_BROGUE_DATA_DIR` (default `/var/lib/late-brogue`; the playground root = the PVC in prod), `LATE_BROGUE_LISTEN_ADDR`, `LATE_BROGUE_PORT` (default `2327`), `LATE_BROGUE_IDLE_TIMEOUT`.

### Binary sourcing: built from verified upstream source, Brogue CE 1.15.1
- Compiled in the `brogue-build` stage (`docker/doors/brogue.Dockerfile`, published as the ghcr `door-brogue` image the root Dockerfile pins) from the pinned GitHub tag tarball, SHA-256 verified fail-closed (`BROGUE_*` ARGs). Build: `make bin/brogue TERMINAL=YES GRAPHICS=NO RELEASE=YES` (curses-only, no SDL; RELEASE drops the `-dev` version suffix that brogue bakes into save names). One source patch, `scripts/brogue_hangup_save.patch` (see §1), applied with a fail-closed grep; the stage also asserts `--version` prints exactly `Brogue version: CE <version>`.
- The terminal build reads no data files (DATADIR only matters for graphics assets), so there is no read-only data tree: `/opt/brogue/brogue` is the whole install, symlinked to `/usr/games/brogue`.

### Images / infra / CI
- `runtime-brogue` stage: `libncurses6` (the build links plain `-lncurses`, not ncursesw) + `ncurses-term`, `/opt/brogue`, the `/usr/games/brogue` symlink, `/var/lib/late-brogue` chowned to `late`. `base` gets the same binary for `dev-brogue` (compose `service-brogue`).
- `infra/brogue.tf`: the RWO `brogue-save` PVC (2Gi, `prevent_destroy`) + locals (enable flag, host/port, `brogue_var_path`). `infra/service-brogue.tf`: the `late-brogue` Deployment (replicas **1**, kill-before-create, `terminationGracePeriodSeconds=30` > `SHUTDOWN_GRACE`, `brogue-save-seed` initContainer that only chowns the mount; the host mkdirs the player dirs itself) + the `late-brogue-sv` ClusterIP Service on 2327. `infra/secrets.tf`: `brogue-identity-secret` injected into both service-ssh and late-brogue.
- CI: `.github/workflows/deploy_brogue.yml` (the `-brogue` release-tag suffix; builds + pushes `runtime-brogue`), `brogue.yml` (PR/weekly build-validate of `docker/doors/brogue.Dockerfile`, publishes the pinned `door-brogue` image on main pushes, manual verify_deployed). Every other deploy workflow reads the live `late-brogue` image tag and passes it through `terraform.yml`'s required `brogue_image_tag`. **First rollout must be `deploy_brogue.yml`** (it builds the image); a normal deploy first would fail the image lookup. License obligations tracked in `NOTICE`: **AGPL-3.0, and because we run a patched build, section 13 requires the corresponding source (pinned tarball + our patch + build recipe) to stay publicly available while the door is hosted**; this repo is that offer.

---

## 4. Critical Invariants [STABLE]

Most of the DCSS door's invariants (§4 of its CONTEXT) apply verbatim: auth compares key DATA not the struct, `derive_client_key` byte-identical across crates (KAT-pinned), `env_clear` + allowlist, XON/XOFF off, close-channel-then-detach-reader, force `ProxyStatus::Closed` + wake render on close, all exits treated identically, fail-soft when disabled, `mod.rs` declaration-only, playname = the claim-once arcade handle. Brogue-specific:

- The playname keys the save **directory**, so `playname::sanitize` is a path-traversal boundary, not cosmetics: it must never let `/`, `.`, or any non-`[A-Za-z0-9_]` byte through, and a claimed handle row must never be deleted or reassigned (a re-claimed handle would open the dead account's saves).
- The hangup-save exists only because of `scripts/brogue_hangup_save.patch`. If a Brogue CE bump changes `curses-platform.c` or `quitImmediately`, re-verify the patch applies AND that a SIGHUPed mid-game child actually leaves a `.broguesave` behind; without it every rollout silently eats live runs.
- On teardown with a live child, SIGHUP before any SIGKILL so the run is saved; `PtyHost::Drop` must not abort the bridge task.
- Keep the child's argv empty. brogue's wizard/easy modes and seed control are runtime flags, and an empty argv is what keeps them unreachable for players.
- Don't add a late.sh-side terminal-size gate: brogue clips on small terminals without complaint, and the landing copy ("roomiest at 100x34") is the only messaging by design.

---

## 5. Tests And Verification [STABLE]

Root policy applies: agents run targeted tests via `make test-llm`; humans run the suite.

Inline `_test.rs` files cover: client+host `identity` (derivation determinism + distinctness + the cross-crate known-answer fingerprint pinning `late.sh/brogue/v1`), client `state` (disabled connect no-op, forward without proxy no-op, mouse/paste stripping, F1 both encodings, exit grace, claim-prompt byte handling/caps/validation errors), host `playname` (sanitize incl. traversal), host `server` (`effective_term` fallback), host `host` (`player_dir` distinctness), hub `state` (card order incl. Brogue), `primitives` (door screens outside the tab cycle).

```bash
make test-llm ARGS="-E 'package(late-brogue) or (package(late-ssh) and test(brogue))'"
```

The PTY bridge and russh loops are process/network-bound and not unit-tested; verify launch/save/quit manually against a real host (compose `service-brogue`). The hangup-save path was verified by hand at build time (PTY + SIGHUP + save file present); re-verify on CE bumps.

---

## 9. Deferred / Future Work [VOLATILE]

- **Awards from the per-player run history, not the screen.** brogue appends a machine-readable line per game to each player dir's run history file (seed, time, result, killer, score, gold, lumenstones, deepest level, turns). An award pipe (chips/badges for an escape, a lumenstone, a deep dive) should read those host-side and signal late-ssh rather than scraping vt100. Needs the same cross-crate signal path DCSS §9 defers.
- **Awards must account for the in-game mode picker.** Wizard and Easy mode are selectable from brogue's own main menu (`chooseGameMode` in MainMenu.c), so an empty argv does not gate them. Upstream mostly self-polices: the run history line is skipped for Easy/Wizard deaths, and 1.15.1's victory path has the condition inverted (`mode != EASY && != NORMAL`), logging only Wizard victories, an upstream quirk to re-check on CE bumps before trusting run-history lines for awards.
- No shared scoreboard: high scores are per player because identity is the cwd. A late.sh-side leaderboard would aggregate the run-history files host-side; surface it on the landing if built.
- Brogue CE ships two official variants in the same binary (Rapid Brogue, Bullet Brogue) selectable from the main menu; they share the player dir and work today. If we ever want them as separate cards, it is a `--variant` argv decision, which collides with the keep-argv-empty invariant; decide deliberately.
- Per-user/global concurrency cap on the host pod if the envelope gets too loose (same posture as dcss: bounded 1:1 by late-ssh's conn caps today).
