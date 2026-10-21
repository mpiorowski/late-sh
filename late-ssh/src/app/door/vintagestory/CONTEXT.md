# Vintage Story Context

## Metadata
- Scope: the Vintage Story card in the Games hub (`late-ssh/src/app/door/vintagestory`), the server it describes (`infra/vintagestory.tf`), and its image (`docker/vintagestory/`, `.github/workflows/vintagestory.yml`).
- Upstream: the official Vintage Story dedicated server by Anego Studios, plus Anego's arm64 launcher build (github.com/anegostudios/VintagestoryServerArm64).
- Status: Active.
- Parent context: `../CONTEXT.md` (the door family).

## Summary

Vintage Story is not a door. Nothing runs inside `late-ssh` and there is no proxy, PTY, or session: players join from their own Vintage Story client, straight to a pod on the game node. The only late.sh code is an information card in the Games hub that says how to connect, how to get whitelisted, how to claim land, and what the world rules are.

## Card

- `HubGame::VintageStory` sits in the `Servers` group after Minecraft (`hub/state.rs`). Enter does nothing on it (`launch_games_hub_selection` shares Minecraft's empty arm), the hub footer drops its Enter hint, and it is never live.
- The landing scrolls with the rest of the hub (Ctrl+J/K). Claiming comes before the rules so the most useful part stays near the top.
- `ADDRESS` and `VERSION` are constants. `ui_test.rs::quoted_settings_match_the_terraform_and_the_image` reads `infra/vintagestory.tf`, `infra/defaults.tf`, and `docker/vintagestory/Dockerfile` at test time and fails when the card and the server drift: the Dockerfile's `VS_VERSION`, the game version in the `vintagestory_image` tag, port 42420, the address, and the `--withconfig` keys the card quotes (whitelist, account verification, PvP, fire spread, time passing when empty). The build never reads those files.
- The claim allowance on the card (3 claims, 262144 blocks) is the game's default for the `suplayer` role, read off a generated `serverconfig.json`, not Terraform, so the drift test does not cover it. Re-check it if the roles are ever customised.
- The `?` guide has a Vintage Story topic (`help_modal/data.rs::vintagestory_help_lines`), which also reaches @bot through `bot_app_context`. It repeats the joining, claims, and world facts but not the version, so only the card can drift from Terraform. Keep the two in step.

## Image

- `docker/vintagestory/Dockerfile` is Anego's own ARM recipe, pinned: download the linux-x64 server tarball for `VS_VERSION` and check its md5 (from `https://api.vintagestory.at/stable.json`, `linuxserver`), download Anego's arm64 release for the minor line and check its sha256, delete the x64 launcher (`VintagestoryServer*`) and `Lib/`, and unpack the arm64 build over it. The game code (VintagestoryLib, the API, assets, built-in mods) is IL and runs unchanged. The x64 `Lib/` files the arm64 build lacks (JsonDiffPatch, Gtk/Cairo) are not referenced by the server or its built-in mods.
- Runtime base is `mcr.microsoft.com/dotnet/runtime:10.0` (1.22 needs .NET 10), running as its `app` user (uid 1654). The fetch stage runs on the build platform and the runtime stage has no `RUN`, so it builds for arm64 on an x86 machine with no emulation.
- The server traps SIGTERM itself: it saves the world, shuts its threads down, and exits 0 within a second or two on a small world. No wrapper, no `screen`, no Anego `server.sh`.
- `.github/workflows/vintagestory.yml` runs on a native `ubuntu-24.04-arm` runner for changes under `docker/vintagestory/`: build, `docker/vintagestory/smoke.sh` (boot a throwaway world, run a console command over stdin, require `World saved!` and exit 0 on stop), and on main push the tag `vintagestory_image` names. A published tag is never overwritten: a recipe change under an existing tag fails the push and asks for a revision bump.

## Server

- Pod: `kubernetes_deployment_v1.vintagestory`, image `local.vintagestory_image` (`ghcr.io/mpiorowski/late-sh/vintagestory:<game version>-<recipe revision>`). Requests 500m / 1536Mi, limits 3 CPU / 3Gi. Measured on agent-2: about 1.2 GiB and 40 s to first boot with nobody on.
- `Recreate` strategy and 120 s termination grace, for the same reasons as Minecraft: one world, one RWO volume, one hostPort.
- Probes are TCP checks on 42420; the startup probe allows 10 minutes for world generation or a version conversion. The port opens while the spawn area is still loading, so the pod reports Ready up to about 20 s before `Dedicated Server now running`; a player joining in that window waits or retries.
- Settings live in `local.vintagestory_config` and reach the server as `--withconfig` on every boot, which also writes them into `serverconfig.json`: `ServerName`, `WhitelistMode = "On"`, `VerifyPlayerAuth`, `AdvertiseServer = false`, `Upnp = false`, `MaxClients`, `AllowPvP`, `AllowFireSpread`, `PassTimeWhenEmpty = false`. A runtime `/serverconfig` change to one of those reverts on restart; every other key keeps the game default or a runtime change.
- The world is standard survival (`surviveandbuild`), default map size (1024000 x 1024000), no world border, generated with world-creation keys set by hand in `WorldConfig.WorldConfiguration` of `serverconfig.json` on the volume before first boot: `landcover 0.4`, `oceanscale 0.75`, `landformScale 1.2`, `upheavelCommonness 0.3`, `polarEquatorDistance 50000`, `storyStructuresDistScaling 1`. The aim is a few big continents split by real oceans: everyday play stays on foot or elk, a long trip means a sailboat (the elk rides along, roped to a fence post in a cargo slot). These keys are locked into the save at creation; `/worldconfig <key>` on the console prints what the save holds. Everything else about the world is the game default. Disk growth is bounded only by exploration.
- Data: `/data` on the `vintagestory-data` PVC, 10Gi `local-path`, `prevent_destroy`, reclaim policy Delete. It holds `Saves/default.vcdbs` (the world), `Playerdata/` (including `playerswhitelisted.json`), `serverconfig.json`, `Logs/`, `Backups/`. **There is no off-node backup.** The PVC is a directory on `agent-2`'s disk; losing the node or deleting the claim loses the world.

## Placement and networking

- Runs on `agent-2` (Hetzner CAX21, arm64, 4 vCPU, 8 GB, 40 GB disk) via the `role=games` node selector and toleration (`games_node_*` locals in `infra/defaults.tf`) plus `kubernetes.io/arch=arm64`. Nothing else runs there but Canal and kube-proxy. See `infra/README.md`, Nodes.
- Port 42420 is a pod **hostPort** on `agent-2`, TCP and UDP (the game uses both since 1.20), IPv4 only. Not an ingress-nginx TCP entry, for the same reload reason as Minecraft.
- DNS by hand in Cloudflare, DNS-only (never proxied): `vs.late.sh A 37.27.12.76` (agent-2). Players connect to `vs.late.sh`.
- Any Hetzner Cloud Firewall on agent-2 must allow TCP and UDP 42420 from anywhere.

## Access

- `VerifyPlayerAuth`: the server checks every login against Anego's auth server, so only real accounts join.
- `WhitelistMode = "On"`: only whitelisted accounts join. The whitelist is `Playerdata/playerswhitelisted.json` on the volume, managed from the console. There is no seed list in Terraform or GitHub variables.
- Players request access by DMing a moderator (`/dm`) with their exact Vintage Story username, as the card says.
- Land claims are built into the game and run on the role defaults: a `suplayer` (the default role) gets up to 3 claims and 262144 blocks in all, at least 5x5x5. Claims are 3D boxes. Inside one, only the owner and those granted (`traverse`, `use`, `all`) can act.

## Operations

The server console reads stdin and the pod keeps stdin open (`stdin`, not `stdin_once`), so commands go in through `kubectl attach`, not `exec`, one attach per command, as many in a row as needed. Commands sent while the world is still loading queue up and run once the console starts. The answer lands in the pod log.

```bash
echo '/player NAME whitelist on' | kubectl attach -i deploy/vintagestory     # the server resolves NAME with Anego's auth server
echo '/player NAME whitelist off' | kubectl attach -i deploy/vintagestory
echo '/op NAME' | kubectl attach -i deploy/vintagestory                      # admin role
echo '/stats' | kubectl attach -i deploy/vintagestory
kubectl logs deploy/vintagestory --tail=20
kubectl logs deploy/vintagestory | grep -E "now running|SIGTERM|World saved|xception"
```

- `kubectl rollout restart deploy/vintagestory` restarts it; with `Recreate` the server is down until the TCP probe passes, about a minute.
- Manifest changes ship only through `deploy_infra.yml` (an `-infra` or `-full` release, or a manual dispatch). Nothing restarts the pod on an ordinary release.
- Resetting the world (nothing survives but `Playerdata/`, so the whitelist and roles stay): `kubectl scale deploy/vintagestory --replicas=0`, wait for the pod to go, run a throwaway busybox pod on `role=games` (node selector plus toleration) with the `vintagestory-data` claim mounted at `/data`, edit `WorldConfig.WorldConfiguration` in `/data/serverconfig.json` (values are strings; `kubectl cp` out and back, then `chown 1654:1654`), `rm -rf /data/Saves/* /data/Backups/* /data/BackupSaves/*`, delete the helper pod, scale back to 1. First boot generates the new world in about a minute.
- A backup while it runs: `echo '/genbackup' | kubectl attach -i deploy/vintagestory` writes a consistent copy of the world into `/data/Backups/`, then `kubectl exec deploy/vintagestory -- tar czf - -C /data Backups > vs-backup.tgz` takes it off the node.

## Upgrading Vintage Story

A version bump converts the world in place and every player needs the new client. With no off-node backup, take one first (Operations).

Gates, both must pass:

```bash
curl -s https://api.vintagestory.at/stable.json | jq -r '.["<version>"].linuxserver | .filename, .md5'
gh release list -R anegostudios/VintagestoryServerArm64 -L 3      # a new minor line (1.23) needs its arm64 release first
```

Steps:

1. Back up (Operations).
2. In `docker/vintagestory/Dockerfile` set `VS_VERSION` and `VS_SERVER_MD5`; for a new minor line also `VS_ARM64_RELEASE` and `VS_ARM64_SHA256` (`sha256sum` of the downloaded release).
3. In `infra/defaults.tf` set `vintagestory_image` to `<version>-1`, and `VERSION` in `ui.rs` plus the `Vintage Story <version>` assertion in `ui_test.rs`.
4. Run the targeted tests (below); the drift test catches a missed copy.
5. Merge: `vintagestory.yml` builds, smoke-tests, and publishes the tag on main. Then ship an `-infra` or `-full` release so the pod takes it.
6. Verify: `kubectl logs deploy/vintagestory | grep -E "Game Version|now running"`, and announce it.

## Critical invariants

- Keep `WhitelistMode = "On"` and `VerifyPlayerAuth = true`; the server is reachable from the whole internet.
- Keep 42420 a hostPort, TCP and UDP, never an ingress-nginx TCP map entry.
- Keep `Recreate` and the 120 s grace; never scale above one replica.
- Keep the image arm64 and the pod pinned to `role=games` plus `kubernetes.io/arch=arm64`.
- Never overwrite a published image tag; bump the revision.
- Keep the card constants, the Dockerfile, and `infra/` in agreement; when the drift test fails, fix whichever side is wrong, never the assertion.
- `mod.rs` remains declarations only.

## Tests

```bash
make test-llm ARGS="-p late-ssh -E 'test(door::vintagestory)'"
make test-llm ARGS="-p late-ssh -E 'test(door::hub) or test(help_modal)'"
bash docker/vintagestory/smoke.sh <arm64 image>    # on an arm64 host
```
