# Minecraft Context

## Metadata
- Scope: the Minecraft card in the Games hub (`late-ssh/src/app/door/minecraft`) and the server it describes (`infra/minecraft.tf`, `scripts/minecraft_whitelist_add.sh`).
- Upstream: Paper (Minecraft Java Edition server) via the `itzg/minecraft-server` image, plus the GriefPrevention plugin from Modrinth.
- Status: Active. Server live since 2026-09-13.
- Last updated: 2026-09-15.
- Parent context: `../../../../../CONTEXT.md`.

## Summary

Minecraft is not a door. Nothing runs inside `late-ssh` and there is no proxy, PTY, or session: players join from their own Minecraft Java client, straight to a pod in the cluster. The only late.sh code is an information card in the Games hub that says how to connect, how to get whitelisted, and what the world rules are.

## Card

- `HubGame::Minecraft` sits in the House group right after Lateania (`hub/state.rs`). `ui.rs` is the landing: connect, get whitelisted, claim land, share a claim, griefing, world.
- Enter does nothing on this card (`launch_games_hub_selection` has an empty arm) and the hub footer drops its Enter hint for it. It is never "live": `live_screen` returns `None`.
- The landing scrolls with the rest of the hub (Ctrl+J/K, Ctrl+arrows) because it is long enough to be cut on short terminals. Claiming comes before the rules on purpose, so the most useful part stays near the top.
- `ADDRESS`, `VERSION`, `DIFFICULTY`, and `WORLD_BORDER` are constants. `ui_test.rs::quoted_settings_match_the_terraform` reads `infra/minecraft.tf` and `infra/defaults.tf` at test time and fails when the card and the server drift (version, port 25565, difficulty, online mode, whitelist, GriefPrevention, world border, and no `mob_griefing` override). The build never reads `infra/`.
- The `?` guide has a Minecraft topic (`help_modal/data.rs::minecraft_help_lines`), which also reaches @bot through `bot_app_context`. It repeats the joining, claims, and griefing facts but not the version or world border, so only the card can drift from Terraform. Keep the two in step when the rules change.
- GriefPrevention numbers on the card (claim blocks, the 9x9 chest claim, expiry days, explosion rules) are plugin defaults read off the live `config.yml`, not Terraform, so the drift test does not cover them. Re-check them if the plugin config is ever customised.

## Server

- Pod: `kubernetes_deployment_v1.minecraft`, image `itzg/minecraft-server:2026.9.1-java25` running `TYPE=PAPER`, `VERSION=26.2` (`local.minecraft_version`), 2G heap with Aikar's flags. Requests 500m / 2560Mi, limits 2 CPU / 3Gi.
- `Recreate` strategy: one world, one RWO volume, one hostPort, so two pods can never coexist. Every restart is downtime.
- 120 s termination grace: the image traps SIGTERM, runs `stop`, and waits for the world save. SIGKILL mid-save corrupts chunks.
- Startup probe allows 10 minutes (first boot downloads Paper and the plugin). Liveness and readiness use `mc-health`.
- World, plugins, and configs live on the `minecraft-data` PVC: 8Gi `local-path`, `prevent_destroy`. **There is no backup.** The PVC is a directory on `agent-1`'s disk; losing the node or deleting the claim loses the world.
- `RCON_CMDS_STARTUP` is re-applied on every boot: currently only `worldborder set 6000` (6000 wide, centred on 0,0). Anything an op changes at runtime that is also set there reverts on restart.
- RCON runs inside the pod on 25575 (not exposed). Password: random, in the `minecraft` secret.

## Placement and networking

- Runs on `agent-1` (Hetzner CX33, 4 vCPU, 8 GB, 40 GB disk) via the `role=support` node selector and toleration (`support_node_*` locals in `infra/defaults.tf`), so the JVM heap and world growth stay off `server-1`, which serves SSH sessions. See root `SCALE.md`, Immediate Next Work 3, and `infra/README.md`, Nodes.
- Port 25565 is a pod **hostPort** on `agent-1`, IPv4 only. It is deliberately not an ingress-nginx TCP passthrough: every nginx reload (cert-manager renewals included) drains workers after 240 s and would kick every player.
- DNS is managed by hand in Cloudflare, DNS-only (never proxied: raw TCP):
  - `mc.late.sh A 65.21.240.242` (agent-1). Players connect to `mc.late.sh`.
  - `_minecraft._tcp.late.sh SRV 0 5 25565 mc.late.sh.` lets the client accept plain `late.sh`. Not created yet as of 2026-09-13, so plain `late.sh` lands on `server-1` and fails.
- `late.sh` itself resolves to `server-1`, and the `*.late.sh` wildcard is Cloudflare-proxied, which is why Minecraft needs its own explicit A record.

## Access

- `ONLINE_MODE=TRUE`: Mojang/Microsoft account verification. Offline or cracked clients are refused.
- `ENABLE_WHITELIST` + `ENFORCE_WHITELIST`: whitelist always on, even with an empty seed list, and removing a player kicks them at once.
- Seeds: GitHub variables `MINECRAFT_WHITELIST` and `MINECRAFT_OPS` on the `production` environment (comma-separated Java usernames), passed as `TF_VAR_*` by `terraform.yml`. They are MERGEd into the on-disk lists at boot, so they only take effect on an infra apply plus restart, and removing a name there does not remove the player.
- Day-to-day: `scripts/minecraft_whitelist_add.sh name...` checks each name against the Mojang API, runs `whitelist add` over RCON (instant, no restart, persisted to `whitelist.json`), prints the list, and prints the `gh variable set` command for any name the seed variable is missing. Removal and ops go through RCON directly (below).
- Players request access by DMing a moderator (`/dm`) with their exact Java Edition username, as the card says.

## Protection and world rules

- GriefPrevention (`MODRINTH_PROJECTS=griefprevention`) runs on its defaults. Its config is generated on the PVC at `/data/plugins/GriefPreventionData/config.yml`, not in git.
  - Claims in the overworld only (`world: Survival`); Nether and End are unclaimable.
  - 100 starting claim blocks, 100 accrued per hour played, 80000 cap; first chest auto-claims radius 4; claims at least 5 wide and 100 area.
  - Inside claims: no building, breaking, container access, switches, animal damage, or villager trading without trust. Trust commands: `/trust`, `/containertrust`, `/accesstrust`, `/untrust`, `/trustlist`.
  - Explosions break blocks only outside claims and below sea level. Endermen cannot move blocks. Fire never spreads or burns blocks.
  - Expiry: chest-only claims after 7 days away, unused claims after 14, all claims after 60 days inactive unless the owner holds 10000+ claim blocks.
  - PvP is protected inside claims; logging out within 15 s of combat kills you. Death drops are NOT locked, because the world counts as a PvP world.
- Vanilla gamerules stay at defaults: `mob_griefing` on (villager breeding and farming and piglin bartering need it, so automation farms work), `pvp` on, `keep_inventory` off, `locator_bar` on. Difficulty `normal`, view distance 10, max 20 players, spawn protection 16.
- **Gamerule names are snake_case since 26.x.** `gamerule mobGriefing false` and `gamerule doFireTick false` fail with "Incorrect argument" and change nothing; this shipped once in `RCON_CMDS_STARTUP` unnoticed. Check the startup log after adding one.

## Operations

```bash
kubectl exec deploy/minecraft -- rcon-cli whitelist list
kubectl exec deploy/minecraft -- rcon-cli whitelist remove NAME
kubectl exec deploy/minecraft -- rcon-cli op NAME
kubectl exec deploy/minecraft -- rcon-cli gamerule mob_griefing       # read a rule
kubectl exec deploy/minecraft -- rcon-cli "help GriefPrevention 2"    # plugin commands, paged
kubectl exec deploy/minecraft -- rcon-cli save-all
kubectl logs deploy/minecraft | grep -E "Done \(|Rcon loop|Incorrect"
```

- Runtime gamerule changes apply instantly and persist in `level.dat` on the next autosave.
- Manifest changes ship only through `deploy_infra.yml` (an `-infra` or `-full` release, or a manual dispatch): a full Terraform apply. Nothing in this repo restarts the pod on an ordinary release.
- `kubectl rollout restart deploy/minecraft` restarts it; with `Recreate` the server is down until the new pod passes `mc-health`, about 20 to 30 s on a warm volume.

## Upgrading Minecraft

A version bump converts the world in place and is one-way: an older server cannot load it. With no backup, a failed upgrade loses the world.

Pending as of 2026-09-15: 26.3 (Wilderness Bound) released that day. Blocked on Paper (26.3 builds are ALPHA only) and GriefPrevention (16.18.7 lists up to 26.2).

Gates, both must pass before bumping:

```bash
curl -s https://fill.papermc.io/v3/projects/paper/versions/26.3/builds | jq -r '.[0].channel'           # STABLE
curl -s https://api.modrinth.com/v2/project/griefprevention/version | jq -r '.[0].game_versions[-1]'   # 26.3
```

- Paper: the image only takes default-channel builds and fails to boot otherwise. Never set `PAPER_CHANNEL=experimental` on this world.
- GriefPrevention: a required Modrinth project with no build for `VERSION` fails the boot. Never mark it optional (`griefprevention?`) to get past this: the server would start with no claim protection.
- Image: bump `minecraft_image` to the newest `itzg/minecraft-server:<date>-java25` tag in the same change.

Steps:

1. Back up while the old version is still running:
   ```bash
   kubectl exec deploy/minecraft -- rcon-cli save-off
   kubectl exec deploy/minecraft -- rcon-cli "save-all flush"
   kubectl exec deploy/minecraft -- tar czf - -C /data --exclude=libraries --exclude=cache --exclude=versions . > mc-<old>-backup.tgz
   kubectl exec deploy/minecraft -- rcon-cli save-on
   ```
2. Bump every copy of the version: `minecraft_version` and `minecraft_image` in `infra/defaults.tf`, `VERSION` in `ui.rs`, the `Java Edition <version>` assertion in `ui_test.rs`, and the Server section here.
3. Run the targeted tests (below); the drift test catches a missed copy.
4. Ship as an `-infra` or `-full` release. First boot downloads Paper and converts the world, so expect minutes of downtime, not seconds.
5. Verify: `kubectl logs deploy/minecraft | grep -E "Done \(|Incorrect|GriefPrevention"`, then `rcon-cli worldborder get` reports 6000.
6. Announce it: clients on the old version can no longer join. New biomes and structures only generate in chunks never loaded before.

## Critical invariants

- Keep the whitelist enforced and online mode on; the server is reachable from the whole internet on a well-known port.
- Keep 25565 a hostPort, never an ingress-nginx TCP map entry.
- Keep `Recreate` and the 120 s grace; never scale above one replica.
- Keep the card constants and `infra/` in agreement; when the drift test fails, fix whichever side is wrong, never the assertion.
- Use snake_case gamerule names in `RCON_CMDS_STARTUP`.
- `mod.rs` remains declarations only.

## Tests

```bash
make test-llm ARGS="-p late-ssh -E 'test(/door::minecraft::/) | test(/door::hub::/)'"
```
