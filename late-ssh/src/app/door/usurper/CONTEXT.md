# Usurper Door Context

## Metadata
- Scope: the Usurper door as a whole, the **client** in `late-ssh/src/app/door/usurper` (proxy/identity/state/render/mod) plus its screen lifecycle wiring in `late-ssh/src/app` (state/input/render/tick) **and the standalone host crate `late-usurper/`**. There is no separate `late-usurper/CONTEXT.md`; this file is the single source for both halves.
- Domain: Usurper, the real upstream LORD-era BBS door game (Jakob Dangarden 1993-2009, GPL-2.0-or-later; Rick Parrish's 32/64-bit Free Pascal port), run on a PTY inside a **dedicated `late-usurper` SSH host** and reached by late-ssh as a network-proxied door (the same model as the NetHack/DCSS doors).
- Primary audience: LLM agents changing the Usurper launcher UI, the SSH client transport, the host crate (PTY bridge / auth / dropfiles / node leases / CP437), input forwarding/filtering, or its config/deploy wiring.
- Last updated: 2026-07-22 (Apple Silicon Compose build support)
- Status: Active
- Parent context: `../../../../../CONTEXT.md`
- Stability note: `[STABLE]` sections change rarely; `[VOLATILE]` sections change with the launcher UI, keybindings, or build/deploy wiring.

---

## 0. Context Maintenance Protocol [STABLE]

Read this after root `CONTEXT.md` whenever a task touches the Usurper launcher, launch/leave behavior, the SSH client transport, the `late-usurper` host (PTY bridge, auth, dropfiles, node leases, CP437 transcoding, seeding), input forwarding/filtering, or Usurper config/deploy wiring.

- Keep this file aligned with the SSH transport contract, the client/host split, the dropfile/node model, config knobs, and known gotchas.
- Update root `CONTEXT.md` when routing, the top-level screen list/tab order, or global keybindings change.
- Treat tests and code as authoritative when comments drift; patch stale comments or this file before handoff.
- Do not add `pub use` re-export layers; `mod.rs` stays declaration-only.

---

## 1. Summary [STABLE]

Usurper runs the **real upstream USURPER.EXE on a PTY**, but **not** inside late-ssh. It lives in its own crate/pod, `late-usurper`, a minimal russh **server** that spawns one game child per SSH session. late-ssh reaches it exactly like the DCSS door reaches `late-dcss`: the door is a russh **client** that streams the remote terminal through a `vt100::Parser` and blits it into a ratatui widget below the top bar. SSH *is* the transport, there is no custom IPC.

Where the roguelike doors are per-player sandboxes, Usurper is **one shared persistent world**: every session runs in the same writable game tree (a PVC in prod), and the game's own data files (`DATA/USERS.DAT`, gangs, king, news) hold all state. The host therefore owns three pieces of door-era plumbing the roguelikes never needed:

- **DOOR32.SYS dropfiles (identity).** Each session gets a freshly written `DROP/<node>/door32.sys` with **comm type 0 (Local)**, the load-bearing choice: the game then talks to its controlling terminal (our PTY) with no socket/FOSSIL layer, and skips its interactive local-logon name prompt because the identity comes from the file. The account's **arcade handle** (shared claim flow with DCSS/NetHack, `door/arcade.rs`; SSH username carries it) becomes the dropfile's "real name", which the game uppercases and keys the player record on. The host re-sanitizes it (`playname::sanitize`, single token, no whitespace, a space would split into first/last name and change the identity).
- **Node leases (concurrency).** Usurper's multinode model gives each concurrent session a distinct node number (`/N<n>`, its own dropfile dir). `nodes.rs` leases 1..=`LATE_USURPER_MAX_NODES` (default 10) RAII-style; when the pool is dry the host prints "all nodes are busy" and closes, which the client treats like any exit (back to the Games hub).
- **CP437 → UTF-8 transcoding (output).** The game emits DOS CP437 bytes (box art, shades). `cp437.rs` maps bytes ≥0x80 to Unicode before they enter the SSH channel, so the client's UTF-8 `vt100` parser stays byte-compatible with the other doors. ASCII (including all ANSI escapes) passes through untouched; input is filtered to ASCII (multi-byte UTF-8 dropped) before reaching the PTY.

Core shape (mirrors DCSS unless noted):
- `Screen::Usurper` has no top-level number key; it is reached via the Games hub card (after DCSS) and `Enter`. The arcade-handle claim modal/lookup flow is shared with DCSS/NetHack verbatim.
- One per-session `UsurperProcess` (russh client, twin of `DcssProcess`) bridges remote bytes into a shared `vt100::Parser`.
- The child is spawned with `env_clear()`, cwd = the shared game dir, args `/PDROP/<node>/ /N<node>`, and **TERM pinned to `xterm`**, a deliberate divergence from nethack/dcss's pass-through-with-fallback: the output feeds late-ssh's vt100 parser, never the player's terminal, and pinning TERM makes every session emit the same dialect. There is no terminfo dependency at all (the game writes raw ANSI; FPC's Crt unit needs no curses).
- While Running, raw client bytes are forwarded to the child, minus mouse/paste noise **and all function keys F1-F12**. In DOOR32 local mode the player's keyboard IS the game's sysop console, and DDPlus binds F2 (sysop chat), F7/F8 (`time_credit`), F10 (`HosedMessage; halt`, terminate the node), letting those through would hand every player the sysop panel. There is **no F1 remap** (the game has no universal help key; its menus list their own keys).
- **The authoritative F-key filter is on the host** (`late-usurper::input_filter`), not the client. It is stream-stateful: it retains an incomplete escape-sequence prefix across SSH chunk boundaries, so a client that splits `F10` (`ESC [ 21 ~`) across two chunks cannot have the child reassemble it. This is the trust boundary, a raw SSH client straight to the host is also covered. The door's `state.rs::strip_input_noise` still strips the same keys client-side, but only as best-effort noise reduction (it is stateless and per-chunk).
- **Teardown**: on client disconnect or host SIGTERM with a live child, SIGHUP then a 3s grace then SIGKILL. Unlike crawl there is no hangup-save to protect, the game writes the world to disk as it goes, but the shape (and the `watch`-broadcast pod shutdown with `SHUTDOWN_GRACE` 8s) matches the other hosts. A hard-killed session can leave a stale online entry; the game's own kick-out ages it and the boot sweep clears it.
- **Boot-time seeding + sweeps** (`seed.rs`, run by `main.rs` before serving): copy files missing from the game dir out of the image's `/opt/usurper/seed` template (never overwriting, the live world survives image upgrades), then delete `DATA/MAINT.FLG` (the maintenance lock; left behind by a mid-maintenance crash it would wedge the whole door) and `NODE/ONLINERS.DAT` (the who-is-playing table; provably stale at boot).

The door is gated behind `LATE_USURPER_ENABLED` (default `false`); when disabled, `connect` is a no-op and the launcher shows "Currently unavailable". The host pod is deployed unconditionally (the flag gates only the client).

---

## 2. Module Map [STABLE]

### Client, `late-ssh/src/app/door/usurper/`

| File | Responsibility |
|---|---|
| `mod.rs` | Module declarations + framing comment. Declaration-only. |
| `proxy.rs` | `UsurperProcess`: per-session russh **client** to the host. Near-clone of `door::dcss::proxy`; `ProcessConfig.playname` carries the arcade handle. |
| `identity.rs` | `derive_client_key(secret)`: blake3 domain `late.sh/usurper/v1`. Byte-identical twin in the host crate; KAT-pinned both sides (`late-usurper-kat-v1` → `SHA256:EOzwdMGl+YxD1ERqkpMknze4OddMl4M6J2j8PdbZOYo`). |
| `state.rs` | Per-session `State`: launcher/running `Mode`, connection config, the optional `UsurperProcess`, viewport, post-exit input grace, shared `HandleFlow`, `forward_input`/`strip_input_noise` (mouse/paste + **F1-F12** stripping; no intercept). |
| `render.rs` | Ratatui rendering: `draw_landing`/`draw_launcher` (USURPER logo, blurb, stats, hints) and `draw_running` blitting via `rebels::render::blit_screen`. |

### Host, `late-usurper/` crate (standalone binary)

| File | Responsibility |
|---|---|
| `main.rs` | Tracing init, `Config::from_env`, boot seeding/sweeps, ephemeral host key, russh server, SIGTERM `watch` + `SHUTDOWN_GRACE`. |
| `config.rs` | `Config`: `bin`, `game_dir` (the writable shared world, children's cwd), `seed_dir`, `secret`, listen addr/port, idle timeout, `max_nodes`. |
| `server.rs` | russh `Server`/`ClientHandler`: `auth_publickey` (key DATA comparison), node lease + dropfile write + "all nodes busy" refusal in `shell_request`, data/resize/close plumbing. |
| `host.rs` | `PtyHost`: the per-session PTY bridge, openpty + env_clear + setsid/TIOCSCTTY + XON/XOFF clear + detached reader, CP437→UTF-8 on output, `input_filter` sanitize on input, `StopReason` teardown (SIGHUP grace then SIGKILL). |
| `input_filter.rs` | `InputFilter`: stream-stateful input sanitizer (the authoritative trust boundary). Drops F1-F12 (all encodings) + mouse/paste + high bytes, retaining an incomplete escape-sequence prefix across chunk boundaries so a split F-key can't reassemble in the child. |
| `identity.rs` | `derive_client_key`, identical to the client copy (KAT-pinned). |
| `playname.rs` | `sanitize(username)`: `[A-Za-z0-9_]`, cap 20 (the handle cap), fall back to `late`. Guards the dropfile's line-oriented format. |
| `cp437.rs` | The 0x80-0xFF CP437→Unicode table + `to_utf8`. Byte-wise, so chunk boundaries are safe. |
| `nodes.rs` | `Nodes`/`NodeLease`: RAII node-number pool. |
| `dropfile.rs` | `write_door32(game_dir, node, playname)`: the per-session DOOR32.SYS (comm 0/local, ANSI, generous session minutes, the game's own daily turn limits bound play). |
| `seed.rs` | `prepare_game_dir`: fill-in-missing copy from the seed template + the MAINT.FLG / ONLINERS.DAT boot sweeps. |

Cross-module wiring (client side) mirrors dcss exactly: `app/state.rs` (`usurper_state`/`enter_usurper`/`leave_usurper`, Running passthrough without intercept), `app/input.rs` (hub launch, launcher/claim-modal keys, arrow no-op), `app/render.rs` (state taken out for `set_viewport`; `by usurper.info` chrome + in-game hint), `app/tick.rs` (tick + return-to-hub with `awaiting_handle` hold), `config.rs`/`state.rs` (`SessionConfig`)/`ssh.rs`/`session_bootstrap.rs`/`src/test_helpers.rs` (the `usurper_*` fields), hub `state.rs`/`ui.rs` (the `HubGame::Usurper` card), `common/primitives.rs` (Screen), `help_modal/data.rs` + `clubhouse/ui.rs` (copy).

---

## 3. Config And Deploy [VOLATILE]

### Client (env → `Config` → `SessionConfig` → `App`)
- `LATE_USURPER_ENABLED` (default `false`), `LATE_USURPER_HOST` (default `127.0.0.1`; compose `service-usurper`, prod `late-usurper-sv`), `LATE_USURPER_PORT` (default `2326`), `LATE_USURPER_SECRET` (must equal the host's; required when enabled).

### Host (`late-usurper` env)
- `LATE_USURPER_SECRET` (required), `LATE_USURPER_BIN` (default `/opt/usurper/bin/USURPER.EXE`), `LATE_USURPER_GAME_DIR` (default `/var/lib/late-usurper`; the PVC in prod), `LATE_USURPER_SEED_DIR` (default `/opt/usurper/seed`), `LATE_USURPER_LISTEN_ADDR`, `LATE_USURPER_PORT` (default `2326`), `LATE_USURPER_IDLE_TIMEOUT`, `LATE_USURPER_MAX_NODES` (default `10`).

### Binary sourcing, built from verified upstream source (v0.25 development line)
- Compiled in the Dockerfile `usurper-build` stage from a **pinned commit tarball** of `rickparrish/Usurper` (`USURPER_COMMIT`/`USURPER_SHA256` `ARG`s, `sha256sum -c` fail-closed). Upstream CI cross-compiles from Windows with a patched fpcupdeluxe; we build for Linux x86-64 with **Debian's stock `fpc` 3.2.2** using upstream's own `build.ps1` flags (`-Mtp -Scgi -CX -O3 -Xs -XX`), verified against the official release binary. Upstream contains Intel assembly and is not ARM-native; Compose therefore pins only `service-usurper` to `linux/amd64`, which OrbStack/Docker Desktop can run under emulation on Apple Silicon.
- **The world data files are generated at build time.** Upstream ships no `DATA/`; the vital files (`MONSTER.DAT`, `NPCS.DAT`, `GUARDS.DAT`, `LEVELS.DAT`, `OBJDAT*.DAT`, ...) are created only by the EDITOR's "Reset Game" TUI button. `scripts/usurper_seed_data.py` drives the FreeVision UI on a PTY (`r`, `y`, `y`, wait for quiet), and the stage asserts the vital files exist fail-closed. NPC generation is randomized, so the seed is not bit-reproducible; the world it defines is the stock one.
- The seed tree also carries the RELEASE `TEXT/` + `DOCS/` assets, the sample `USURPER.CFG` (lines 1-2 rewritten to `Late Sysop` / `late.sh`, display-only; registration is bypassed in the port), and a minimal `USURP.CTL` naming the sysop `Late Sysop` (handles can't contain spaces and `late`/`late_*` are reserved, so no player can match the sysop identity).
- The binaries are statically linked; the runtime stage needs no ncurses/terminfo.

### Images / infra / CI
- `runtime-usurper` stage: `/opt/usurper` (bins + seed) + `/var/lib/late-usurper` chowned to `late`. `dev-usurper` copies the same tree directly; other dev targets do not depend on the Usurper build stage. Compose gives the emulated amd64 service its own `cargo-target-amd64` volume so its native Rust artifacts cannot collide with the ARM services' shared `cargo-target` volume.
- `infra/usurper.tf`: the RWO `usurper-save` PVC (1Gi, `prevent_destroy`) + locals. `infra/service-usurper.tf`: the `late-usurper` Deployment (replicas **1**, kill-before-create, `terminationGracePeriodSeconds=30` > `SHUTDOWN_GRACE`, chown-only initContainer, the host seeds itself) + the `late-usurper-sv` ClusterIP Service on 2326. `infra/secrets.tf`: `usurper-identity-secret` injected into both service-ssh and late-usurper.
- CI: `.github/workflows/deploy_usurper.yml` (the `-usurper` release-tag suffix; builds + pushes `runtime-usurper`, bootstrap path), `usurper.yml` (PR/weekly build-validate, manual verify_deployed). Every other deploy workflow reads the live `late-usurper` image tag and passes it through `terraform.yml`'s required `usurper_image_tag`. **First rollout must be `deploy_usurper.yml`** (it builds the image); a normal deploy first would fail the image lookup. License obligations tracked in `NOTICE` (GPL-2.0-or-later).

---

## 4. Critical Invariants [STABLE]

Most of the NetHack/DCSS door invariants apply verbatim, auth compares key DATA, `derive_client_key` byte-identical across crates (KAT), `env_clear` + allowlist, XON/XOFF off, close-channel-then-detach-reader, force `ProxyStatus::Closed` + wake render on close, all exits treated identically, fail-soft when disabled, `mod.rs` declaration-only, arcade handle immutable once claimed. Usurper-specific:

- **The dropfile playname must stay a single whitespace-free token.** DOOR32.SYS is line-oriented and the game splits the real-name field on spaces into first/last name; `playname::sanitize` guarantees this. The game uppercases the name and keys `DATA/USERS.DAT` records on it, so (like the other handle doors) a claimed handle row must never be deleted or reassigned.
- **Comm type in the dropfile must stay `0` (Local).** Any other value makes the game try serial/socket I/O that doesn't exist here.
- **Strip F1-F12 at the host** (`input_filter`, stateful across chunks), with a best-effort client strip too. In local mode the player's keys are the sysop console keys (F2 chat, F7/F8 time, F10 terminate). Re-check the strip if DDPlus keybindings change on a version bump.
- **CP437 transcoding lives host-side, on output only,** mapping only bytes ≥0x80 (byte-wise, chunk-safe; ANSI escapes are ASCII and untouched). Client input is filtered to ASCII before the PTY.
- **Node leases are RAII and must be held by the bridge task** (via `HostConfig.node`), so any bridge exit path frees the node.
- **The boot sweeps are safe only at boot** (single host process, no sessions yet). Never sweep `DATA/MAINT.FLG` or `NODE/ONLINERS.DAT` while serving.
- **Seeding never overwrites.** `copy_missing` is strictly fill-in-the-blanks; the shared world on the PVC outlives image upgrades.
- replicas MUST stay 1 (one RWO volume, one shared world, file locking assumes one machine).

---

## 5. Tests And Verification [STABLE]

Root policy applies: agents run targeted tests (`make test-llm ARGS=...`); humans run the suite.

Sibling `_test.rs` files cover: host `cp437` (ASCII/ANSI passthrough, box art, full-range validity), `dropfile` (comm 0, identity fields, rewrite), `nodes` (lowest-free lease, exhaustion, RAII recycle), `seed` (fill-in-missing, never-overwrite, boot sweeps), `playname` (sanitize incl. whitespace), `identity` (determinism + KAT); client `identity` (same KAT) and `state` (disabled no-op, F-key/mouse/paste stripping vs arrows/nav keys, exit grace, claim-prompt basics). Hub/screen ordering tests updated in place.

```bash
make test-llm ARGS="-p late-usurper"
make test-llm ARGS="-p late-ssh -E 'test(usurper)'"
```

The PTY bridge and russh loops are process/network-bound and not unit-tested; verify launch/play/quit manually against a real host (compose `service-usurper`).

---

## 9. Known Gotchas / Deferred [VOLATILE]

- **First login of a game day runs maintenance** (news, NPC activity; ~30s). That player watches a progress readout before the login screen, classic door behavior, left as-is. If the host dies mid-maintenance the boot sweep clears the `MAINT.FLG` lock.
- **The game is a fixed 80x25 screen.** It does not resize; smaller viewports clip/wrap (the landing warns "keep the window roomy"). No late.sh-side size gate, matching the DCSS posture.
- **FPC's Crt emits a cursor-position query (`ESC[6n`) at startup** and tolerates no reply (verified live); the host does not answer it. If a future port version blocks on the reply, answer `ESC[1;1R]` host-side in the bridge.
- **Same-account concurrent sessions**: the game's ONLINERS table refuses a name already playing; not specially handled (matches nethack's posture).
- **A hard-killed session loses unsaved turn progress** (the game saves at its own checkpoints) and leaves an online ghost until the game's kick-out or the next host boot. Accepted; the SIGHUP grace makes it rare.
- **The seed world is generated, not pinned**: rebuilding the image regenerates NPCS.DAT etc. with different random rolls. Irrelevant in prod (the PVC world is seeded once and never overwritten), but two fresh dev environments won't have byte-identical worlds.
- Deferred: awards/chips for in-game milestones (becoming king, completing the game, `DATA/FAME.DAT` and the news files are machine-readable host-side, so a future award pipe should read those, not scrape vt100); surfacing the shared news/hall-of-fame on the landing; per-user concurrency caps beyond the node pool.
