# BashQuest door: build, config and deploy

How the `late-bashquest` host is configured, sourced and shipped. The door's
shape, transport and invariants are in `CONTEXT.md`; the generic door module
and release flow are in `infra/` and the root `CONTEXT.md`.

## Client config (`late-ssh/src/config.rs` profile → `SessionConfig` → `App`)
- Client enabled/host/port are profile literals in `late-ssh/src/config.rs` (dev `service-bashquest`, prod `late-bashquest-sv`, port 2330); `LATE_BASHQUEST_SECRET` is the only env the client reads (must equal the host's).

## Host config (`late-bashquest` env)
- `LATE_BASHQUEST_SECRET` (required), `LATE_BASHQUEST_BIN` (default `/usr/local/bin/bashquest.sh`), `LATE_BASHQUEST_DATA_DIR` (default `/var/lib/late-bashquest`, the one shared persistent HOME on the PVC), `LATE_BASHQUEST_LISTEN_ADDR` (default `0.0.0.0`), `LATE_BASHQUEST_PORT` (default `2330`), `LATE_BASHQUEST_IDLE_TIMEOUT`.

## Binary sourcing — **pinned commit, not a compiled build**
- Unlike every other door, there is nothing to compile: `bashquest.sh` is fetched by exact commit SHA and SHA-256-verified in `docker/doors/bashquest.Dockerfile` (`bashquest-build` stage), then `chmod 0755`'d. Bump `BASHQUEST_COMMIT`/`BASHQUEST_URL`/`BASHQUEST_SHA256` together when pulling in a newer upstream version, and update `NOTICE`.
- `BASHQUEST_AUTOLOGIN` is not a late.sh patch: it's an upstream feature bashquest.sh itself added specifically to support this integration (opt-in and backward-compatible: unset, the script behaves as it does for standalone `curl | bash` users). No source is forked or modified.

## Images (Dockerfile)
- `base` copies the verified script to `/usr/local/bin/bashquest.sh` (from the `bashquest-build` stage) so `dev-bashquest` (which derives from `base`) can run it; prod ships the same copy in `runtime-bashquest`, which also installs `ncurses-term` for `clear`'s terminfo lookup (`CONTEXT.md` §9). `late-bashquest` (the host binary) builds in its own `builder-bashquest` cargo-chef stage, same shape as every other door host.
- The committed `.env.dev` / `.env.dev2` templates carry the host-side settings compose passes to `service-bashquest` (`LATE_BASHQUEST_PORT` / `_SECRET` / `_DATA_DIR`), mirroring the dopewars/DCSS block; the client's host and port are profile literals, not env.

## Prod (Kubernetes / terraform)
- `infra/doors.tf` (`bashquest` entry, stamped out by the `infra/door` module): the `late-bashquest` Deployment (replicas **1**, `runtime-bashquest` image, `bashquest-save` PVC mounted at the shared HOME, a `bashquest-save-seed` initContainer that chowns the mount to `late`, `RUST_LOG`/`LATE_BASHQUEST_SECRET`/`LATE_BASHQUEST_DATA_DIR` env) + `late-bashquest-sv` ClusterIP Service on 2330, the RWO `bashquest-save` PVC (`local-path`, 256Mi, `prevent_destroy`), and the `bashquest-identity-secret` (random 64-char) injected into **both** service-ssh and late-bashquest so they derive the same key. **Deployed unconditionally**; kill-before-create rollout (`maxSurge=0`/`maxUnavailable=1`) so the old pod releases the RWO volume before the new one mounts it.
- `infra/service-ssh.tf` injects the client's only env, `LATE_BASHQUEST_SECRET`.
- `replicas` must stay 1 (one RWO volume holds every player's shared save data; assumes the single-node `local-path` cluster).
- CI: `-bashquest` releases route through `release.yml` to the generic `deploy_service.yml` (image-only `kubectl set image` on the existing deployment, or a `-target=module.door["bashquest"]` terraform bootstrap on first deploy). `doors.yml` build-validates `docker/doors/bashquest.Dockerfile` (fetch + checksum + the `docker/doors/smoke/bashquest.sh` smoke test) and publishes the pinned `door-bashquest` image on main pushes at the tag pinned in the root Dockerfile.
