# CodeKeep Door Context

## Metadata
- Scope: the CodeKeep client under `late-ssh/src/app/door/codekeep`, its app-screen wiring, and the standalone `late-codekeep` host crate.
- Upstream: CodeKeep: The Pale 1.0.9, https://github.com/tooyipjee/codekeep
- License: MIT; attribution is in `NOTICE`.
- Status: Active.
- Parent context: `../../../../../CONTEXT.md`.

## Summary

CodeKeep is an upstream Bun/Ink terminal game embedded as a network door. It never runs inside `late-ssh`. `late-ssh` connects to the dedicated `late-codekeep` russh server, requests a PTY, parses the returned ANSI stream with `vt100`, and blits that screen below late.sh's top bar.

The game is the exact npm package `codekeep@1.0.9`. `docker/codekeep/package.json` and `bun.lock` pin the package and all transitive dependencies with npm integrity hashes. Docker builds use `bun install --frozen-lockfile`; runtime never runs `bunx` or reaches npm.

## User flow

- CodeKeep is the final card in the Games hub (`3`). `Enter` switches to `Screen::Codekeep`, constructs the per-session client state, and connects immediately.
- While running, all ordinary terminal bytes go to CodeKeep. late.sh strips its own any-event mouse reports and bracketed-paste markers first.
- Upstream owns its controls: arrows select, `Enter` confirms, `q`/`Esc` backs out, and `q` on the main menu exits. `Ctrl-C` invokes upstream's graceful save-and-exit handler.
- When the remote channel closes, `State::tick` returns to `Screen::Games` on the same app tick. CodeKeep has no post-exit prompts, so it does not use the roguelike doors' trailing-input grace.
- Upstream enforces a minimum 108x24 game viewport. Because late.sh keeps a top frame row, the user's terminal needs additional vertical room. A smaller terminal shows upstream's own size warning; late.sh does not alter or crop game logic.

## Identity and persistence

Authorization and save identity are separate:

- Both client and host derive one Ed25519 authorization key from `LATE_CODEKEEP_SECRET`, domain-separated with `late.sh/codekeep/v1`. The two `identity.rs` copies must stay byte-identical.
- The SSH username is `codekeep_session_label(user_id)`: `late_` plus the trailing 24 lowercase hex digits of the immutable account UUID. It is opaque, stable across username changes, and not user-editable.
- The host rejects any username outside that exact shape. The accepted label becomes a directory name below `LATE_CODEKEEP_DATA_DIR`.
- Every child gets `HOME=/var/lib/late-codekeep/<account>` and `cwd` equal to that HOME. Upstream therefore stores its autosave at `<HOME>/.config/codekeep/game.json`.
- Starting from the per-account HOME also prevents CodeKeep's optional git-bonus scanner from seeing the late.sh source repository. The runtime image does not ship `git` or `gh`.
- The host allows one live child per account. `SessionLease` remains owned by the detached bridge through graceful teardown, preventing two processes from racing on upstream's `game.json.tmp` atomic-save path.

## Transport and teardown

Client files:
- `proxy.rs`: russh client, shared `vt100::Parser`, status, input/resize channel, account label.
- `state.rs`: launcher/running lifecycle, viewport, noise filtering, exit grace.
- `render.rs`: Games landing and vt100 blit.
- `identity.rs`: shared-secret key derivation.

Host files:
- `server.rs`: key auth, account-label validation, one-session lease, PTY request and channel routing.
- `host.rs`: `openpty`, clear XON/XOFF, `setsid` + `TIOCSCTTY`, minimal environment, stable HOME, bidirectional bridge, resize, and graceful teardown.
- `config.rs`: binary/data root/listener/secret settings.
- `account.rs`: fail-closed account-label validation.
- `main.rs`: listener and pod-wide shutdown broadcast.

On a normal upstream exit, the host closes the SSH channel. On client disconnect or host shutdown while the child is live, it sends SIGHUP. CodeKeep 1.0.9 handles SIGINT, SIGTERM, and SIGHUP by best-effort saving and unmounting Ink. The host allows five seconds, then uses SIGKILL as a backstop. Pod shutdown broadcasts to every child and holds the process for eight seconds.

## Configuration

Client (`late-ssh`):
- `LATE_CODEKEEP_ENABLED` (default false)
- `LATE_CODEKEEP_HOST` (default `127.0.0.1`; Compose `service-codekeep`; prod `late-codekeep-sv`)
- `LATE_CODEKEEP_PORT` (default 2328)
- `LATE_CODEKEEP_SECRET` (required when enabled)

Host (`late-codekeep`):
- `LATE_CODEKEEP_BIN` (default `/usr/local/bin/codekeep`)
- `LATE_CODEKEEP_DATA_DIR` (default `/var/lib/late-codekeep`)
- `LATE_CODEKEEP_SECRET` (required)
- `LATE_CODEKEEP_LISTEN_ADDR` (default `0.0.0.0`)
- `LATE_CODEKEEP_PORT` (default 2328)
- `LATE_CODEKEEP_IDLE_TIMEOUT` (default 3600 seconds)

## Images and production

- `runtime-codekeep` contains Bun 1.3.10, the lockfile-installed CodeKeep package, the tiny absolute-path wrapper, and `late-codekeep`.
- Compose uses `dev-codekeep` plus the `codekeep-data` named volume.
- `infra/codekeep.tf` owns the 1Gi RWO `codekeep-save` PVC.
- `infra/service-codekeep.tf` owns the one-replica, kill-before-create Deployment and internal Service on 2328. The init container chowns the mounted root to `late`.
- `.github/workflows/deploy_codekeep.yml` builds and rolls out CodeKeep, and only CodeKeep, for `-codekeep` releases. Every other deploy workflow (including the standard one) reads the live tag off the `late-codekeep` deployment and passes it through the required Terraform input, so an ordinary release never rebuilds or restarts the door. Same rule as nethack, dcss, brogue, dopewars, and usurper.
- Pod resources come from the shared `door_*` locals in `infra/defaults.tf`, one spec for every door host. Do not set per-door values.

## Critical invariants

- Never run `bunx` at runtime; package resolution must remain build-time, exact, locked, and integrity-checked.
- Never run the upstream child in `late-ssh`; the dedicated host is the resource and fault boundary.
- Keep account HOME based on immutable UUID label, not mutable username or arcade handle.
- Keep one live child per account until graceful teardown completes.
- Keep `cwd` at the per-account HOME and keep `git`/`gh` out of the image so upstream integration cannot inspect the late.sh repo or submit host-side bug reports.
- Keep raw input passthrough ahead of normal late.sh routing while running, with mouse/paste noise filtering.
- Keep SIGHUP save, five-second child grace, eight-second host shutdown grace, and the 30-second pod termination grace ordered as documented.
- Compare SSH public-key data, not `PublicKey` comments.
- `mod.rs` remains declarations only.

## Tests and verification

Pure tests cover key derivation, account-label validation, session-label stability, input filtering, disabled launch, immediate close-to-launcher transition, selector ordering, and Screen fallback. The PTY/network path is verified with the built runtime image and a real terminal.

Focused checks:

```bash
make test-llm ARGS="-p late-codekeep"
make test-llm ARGS="-p late-ssh -E 'test(codekeep) | test(door_games_are_outside) | test(selection_clamps) | test(all_games_are_listed)'"
```
