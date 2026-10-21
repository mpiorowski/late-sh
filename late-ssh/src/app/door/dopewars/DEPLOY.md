# dopewars door: build, config and deploy

How the `late-dopewars` host is built, configured and shipped. The door's
shape, transport and invariants are in `CONTEXT.md`; the generic door module
and release flow are in `infra/` and the root `CONTEXT.md`.

## Client config (env → `Config` → `SessionConfig` → `App`)
- Client enabled/host/port are profile literals in `late-ssh/src/config.rs` (dev `service-dopewars`, prod `late-dopewars-sv`, port 2324).
- `LATE_DOPEWARS_SECRET`: the only env the client reads; shared secret, **must equal the host's**.

## Host config (`late-dopewars` env)
- `LATE_DOPEWARS_SECRET` (required), `LATE_DOPEWARS_BIN` (default `/usr/games/dopewars`), `LATE_DOPEWARS_SCORE_FILE` (default `/var/lib/late-dopewars/dopewars.sco`, the one shared high-score file on the PVC), `LATE_DOPEWARS_LISTEN_ADDR` (default `0.0.0.0`), `LATE_DOPEWARS_PORT` (default `2324`), `LATE_DOPEWARS_IDLE_TIMEOUT`.

## Binary sourcing — **built from verified upstream source, dopewars 1.6.2**
- Compiled in the `dopewars-build` stage (`docker/doors/dopewars.Dockerfile`, published as the ghcr `door-dopewars` image the root Dockerfile pins) (terminal-only: `--disable-gui-client --disable-gui-server --enable-curses-client`). The stage downloads the pinned 1.6.2 SourceForge tarball, verifies SHA-256 (`sha256sum -c`, fail-closed), builds, and copies the binary to `/dopewars`. Version/URL/checksum are `ARG`s.
- **Build quirk (`make LIBS="-lncursesw"`):** dopewars' release Makefile drops `$(CURSES_LIBS)` from `dopewars_LDADD` when the GTK client is disabled, so the link fails with undefined `initscr`/`newterm`/… The curses lib is injected via the trailing `$(LIBS)` on the link line. Keep this on any version bump.
- The binary is **self-contained** (drug/location data compiled in) and **not setgid** (so the `-f` is honored). Runtime deps: `libglib2.0-0` + `libncursesw6` (+ `libcurl4`, pulled in by the optional metaserver client).

## Images (Dockerfile)
- dopewars ships in its **own `runtime-dopewars` stage** (the late-dopewars host), copied from `dopewars-build`, alongside its runtime libs + `ncurses-term`; `service-ssh` ships only the client. `builder` builds `late-dopewars` (no `otel` feature; it has a no-op `otel` feature only so workspace-wide `--features otel` stays valid). The from-source binary still lives in `base` for `dev-dopewars` (via `dev-base`).
- `Makefile` + `.env` thread `LATE_DOPEWARS_PORT` / `_SECRET` / `_SCORE_FILE` for the host container (client host/port/enabled live in config.rs).

## Prod (Kubernetes / terraform)
- `infra/doors.tf` (`dopewars` entry, stamped out by the `infra/door` module): the `late-dopewars` Deployment (replicas **1**, runtime-dopewars image, `dopewars-save` PVC mounted at the score dir, `dopewars-score-seed` initContainer that chowns the mount to `late`, `RUST_LOG`/`LATE_DOPEWARS_SECRET`/`LATE_DOPEWARS_SCORE_FILE` env) + `late-dopewars-sv` ClusterIP Service on 2324. **Deployed unconditionally** (the enable flag gates only the client); the rollout is **kill-before-create** (`maxSurge=0`/`maxUnavailable=1`) so the old pod releases the RWO volume before the new one mounts it.
- The RWO `dopewars-save` PVC (`local-path`, 256Mi, `prevent_destroy`) lives in the same doors.tf entry.
- `dopewars-identity-secret` (random 64-char, from the door module) is injected into **both** service-ssh and late-dopewars so they derive the same key.
- `infra/service-ssh.tf` injects only `LATE_DOPEWARS_SECRET`; host/port live in the config.rs prod profile.
- `replicas` must stay 1 (one RWO volume holds the shared score file; assumes the single-node `local-path` cluster).
- CI: `-dopewars` releases route through `release.yml` to the generic `deploy_service.yml`: `ci` + `build`, then `kubectl set image deployment/late-dopewars '*=<image>'` (covers the `dopewars-score-seed` init container and the main container) and a rollout wait. No terraform runs on ordinary releases, so nothing else in the cluster is touched; when the `late-dopewars` deployment is missing the run auto-bootstraps (a terraform apply `-target=module.door["dopewars"]`). Other manifest changes go through `deploy_infra.yml`; terraform never reads or rewrites live image tags (deployments carry `ignore_changes` on images), so an ordinary release never rebuilds or restarts the door. Same rule as every other door. `doors.yml` build-validates `docker/doors/dopewars.Dockerfile` via `docker/doors/smoke/dopewars.sh` and publishes the pinned `door-dopewars` image on main pushes at the tag pinned in the root Dockerfile. License/source obligations tracked in `NOTICE` (GPLv2).
