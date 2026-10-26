# Vintage Story world options

Every world option the 1.22.7 dedicated server accepts, read out of the game
itself: the create-world screen's list lives in `Mods/VSSurvivalMod.dll` of the
server tarball, with the English names in `assets/game/lang/en.json`. Also
what prod runs today. Regenerate the list on a version bump (below).

## Where the keys go

All of these sit in `serverconfig.json` on the volume, under
`WorldConfig.WorldConfiguration`, as strings (`"landcover": "0.4"`). Next to
`WorldConfiguration` in `WorldConfig`:

- `Seed`: any string; empty picks a random one at creation.
- `PlayStyle`: `surviveandbuild` (ours), `exploration`, `wildernesssurvival`,
  `homosapiens`. A play style is a preset of the keys below.
- `WorldType`: `standard` or `superflat`.
- `MapSizeY`: world height, 256 by default.

**Locked** keys are read only when the world is created; changing one later
does nothing to an existing save, so it needs a world reset (`CONTEXT.md`,
Operations). Unlocked keys change on a running world with
`/worldconfig <key> <value>` on the console (some take effect after a
restart). The deposit keys are unlocked but only shape chunks generated after
the change.

The `surviveandbuild` preset overrides four attribute defaults below:
`polarEquatorDistance 100000`, `propickNodeSearchRadius 6`,
`temporalGearRespawnUses 20`, `temporalStormSleeping 0`. A dedicated server
applies the preset to every key the file leaves out: the live save holds
those values with none of them set by hand.

## What prod runs

Read off `/data/serverconfig.json` on the pod, and out of the save itself
with `/worldconfig <key>` on the console. Everything not listed here is the
game default (the tables below), including all the roles, so a `suplayer`
gets 3 claims and 262144 blocks.

```bash
kubectl exec deploy/vintagestory -- cat /data/serverconfig.json
echo '/worldconfig landcover' | kubectl attach -i deploy/vintagestory     # the answer lands in the pod log
```

The seed is random and only the save holds it. `/seed` on the console prints
nothing; an admin types it in game.

Server keys, forced on every boot by `--withconfig` from
`local.vintagestory_config` in `infra/vintagestory.tf` (a runtime change
reverts on restart):

| Key | Value |
|-----|-------|
| `ServerName` | `late.sh` |
| `WhitelistMode` | `On` (stored as `2`) |
| `VerifyPlayerAuth` | `true` |
| `AdvertiseServer` | `false` |
| `Upnp` | `false` |
| `MaxClients` | `16` |
| `AllowPvP` | `true` |
| `AllowFireSpread` | `true` |
| `PassTimeWhenEmpty` | `false` |

World: `PlayStyle` `surviveandbuild`, `WorldType` `standard`, `Seed` `null`,
and this block in `WorldConfig.WorldConfiguration`, written by hand before the
world was generated. A reset writes the same block, so each reset rolls a new
random seed with the same settings:

```json
"WorldConfiguration": {
  "landcover": "0.4",
  "oceanscale": "0.75",
  "landformScale": "1.2",
  "polarEquatorDistance": "50000",
  "toolDurability": "1.5",
  "saplingGrowthRate": "0.75",
  "microblockChiseling": "all",
  "deathPunishment": "keep",
  "lightningFires": "true",
  "temporalStorms": "off",
  "temporalRifts": "off",
  "colorAccurateWorldmap": "true"
}
```

The first three are one player's tested set for an ocean-travel world with
real continents (Bruno Willis, "Ocean with continents on 1.21?",
vintagestory.at/forums/topic/17088): continents one or two in-game days of
riding across, split by ocean wide enough that sailing is the way over.
Below about 50% land the land stops connecting up, so separate continents are
forced. `oceanscale` reads backwards: a low value clumps the land into a few
big masses, a high one scatters it into small islands (30% / 3.0 gave islands
too small to find reeds on). The polar distance is our own addition, not part
of that set: half the preset's, so a sea crossing reaches a different climate
in half the distance and gives the boat a second reason to exist. Those four
are locked.

The rest are unlocked, so `/worldconfig` can still move them on the live
world: tools last 150%, saplings grow in 75% of the time
(`saplingGrowthRate` is a growth time, so lower is faster), the chisel works
on most cubic blocks instead of only stone and wood, a death keeps the
inventory, lightning starts fires (they spread because `AllowFireSpread` is
on in Terraform), no temporal storms or rifts for now (the stability meter
itself stays on), and the world map shows the real terrain colors.

Every other world key is the `surviveandbuild` preset or the attribute
default.

## Finding a seed

Generation is deterministic for the same game version, seed and locked
keys, so seeds can be scouted in a singleplayer world on the same client
version with the same settings (creative mode and the world map make it
quick). Reset the server once with the winner.

## Is there a new version?

```bash
curl -s https://api.vintagestory.at/stable.json | jq -r 'to_entries[] | select(.value.linuxserver.latest == 1) | .key'
grep VS_VERSION= docker/vintagestory/Dockerfile                    # what we run
gh release list -R anegostudios/VintagestoryServerArm64 -L 3        # a new minor line (1.23) also needs its arm64 build
```

A patch release (1.22.x) needs only the first line to move; a new minor line
waits for Anego's arm64 release too. The upgrade itself is `CONTEXT.md`,
Upgrading Vintage Story.

## Regenerating this list

New versions can add keys or values. Download the server tarball (the URL and
md5 come from `stable.json`, as in the Dockerfile) into an empty directory,
unpack it, and dump the embedded JSON:

```bash
python3 -I - Mods/VSSurvivalMod.dll <<'PY'
import sys, json
b = open(sys.argv[1], 'rb').read()
i = b.find(b'"code": "landcover"')
s = e = i
ok = lambda c: c >= 0x20 or c in (9, 10, 13)
while s > 0 and ok(b[s - 1]): s -= 1
while e < len(b) and ok(b[e]): e += 1
t = b[s:e].decode('utf-8', 'replace'); t = t[t.find('{'):]
print(json.dumps(json.loads(t), indent=2))
PY
```

The output has `playstyles` (each preset's `worldConfig`) and
`worldConfigAttributes` (`code`, `category`, `default`, `values`, `names`,
`onlyDuringWorldCreate`), which the tables below are built from.

## Options

### World generation

| Key | Name | Default | Values | Locked |
|-----|------|---------|--------|--------|
| `worldClimate` | Climate distribution | `realistic` | `realistic` Realistic, `patchy` Patchy | yes |
| `landcover` | Landcover | `0.975` | `0` ~0%, `0.1` 10%, `0.2` 20%, `0.3` 30%, `0.4` 40%, `0.5` 50%, `0.6` 60%, `0.7` 70%, `0.8` 80%, `0.9` 90%, `0.95` 95%, `0.975` 97.5%, `1` 100% | yes |
| `oceanscale` | Landcover scale | `5` | `0.1` 10%, `0.25` 25%, `0.5` 50%, `0.75` 75%, `1` 100%, `1.25` 125%, `1.5` 150%, `1.75` 175%, `2` 200%, `3` 300%, `4` 400%, `5` 500% | yes |
| `upheavelCommonness` | Upheaval rate | `0.3` | `0` 0%, `0.1` 10%, `0.2` 20%, `0.3` 30%, `0.4` 40%, `0.5` 50%, `0.6` 60%, `0.7` 70%, `0.8` 80%, `0.9` 90%, `1` 100% | yes |
| `geologicActivity` | Geologic Activity | `0.05` | `0` None, `0.05` Rare, `0.1` Uncommon, `0.2` Common, `0.4` Very Common | yes |
| `landformScale` | Landform scale | `1.0` | `0.2` 20%, `0.4` 40%, `0.6` 60%, `0.8` 80%, `1.0` 100%, `1.2` 120%, `1.4` 140%, `1.6` 160%, `1.8` 180%, `2` 200%, `3` 300% | yes |
| `worldWidth` | World width | `1024000` | `8192000` 8 mil blocks, `4096000` 4 mil blocks, `2048000` 2 mil blocks, `1024000` 1 mil blocks, `600000` 600k blocks, `512000` 512k blocks, `384000` 384k blocks, `256000` 256k blocks, `102400` 102k blocks, `51200` 51k blocks, `25600` 25k blocks, `10240` 10k blocks, `5120` 5120 blocks, `1024` 1024 blocks, `512` 512 blocks, `384` 384 blocks, `256` 256 blocks, `128` 128 blocks, `64` 64 blocks, `32` 32 blocks | yes |
| `worldLength` | World length | `1024000` | `8192000` 8 mil blocks, `4096000` 4 mil blocks, `2048000` 2 mil blocks, `1024000` 1 mil blocks, `600000` 600k blocks, `512000` 512k blocks, `384000` 384k blocks, `256000` 256k blocks, `102400` 102k blocks, `51200` 51k blocks, `25600` 25k blocks, `10240` 10k blocks, `5120` 5120 blocks, `1024` 1024 blocks, `512` 512 blocks, `384` 384 blocks, `256` 256 blocks, `128` 128 blocks, `64` 64 blocks, `32` 32 blocks | yes |
| `worldEdge` | World edge | `traversable` | `blocked` Blocked, `traversable` Traversable (Can fall down) |  |
| `polarEquatorDistance` | Polar-Equator distance | `50000` | `800000` 800k blocks, `400000` 400k blocks, `200000` 200k blocks, `100000` 100k blocks, `50000` 50k blocks, `25000` 25k blocks, `15000` 15k blocks, `10000` 10k blocks, `5000` 5000 blocks | yes |
| `storyStructuresDistScaling` | Story structures distance scaling | `1` | `0.15` 15%, `0.25` 25%, `0.5` 50%, `0.75` 75%, `1` 100%, `1.5` 150%, `2` 200%, `3` 300% | yes |
| `globalTemperature` | Global temperature | `1` | `4` Scorching hot, `2` Very hot, `1.5` Hot, `1` Normal, `0.75` Cold, `0.5` Very Cold, `0.25` Snowball earth | yes |
| `globalPrecipitation` | Global precipitation | `1` | `4` Super humid, `2` Very humid, `1.5` Humid, `1` Normal, `0.5` Semi-Arid, `0.25` Arid, `0.1` Hyperarid | yes |
| `globalForestation` | Forestation & shrubs | `0` | `1` Forest World (+100%), `0.9` Extremely forested (+90%), `0.75` Very highly forested (+75%), `0.5` Highly forested (+50%), `0.25` Somewhat more forest (+25%), `0` Normal, `-0.25` Somewhat less forest (-25%), `-0.5` Significantly less forested (-50%), `-0.75` Much less forested (-75%), `-0.9` Near Tree-less (-90%), `-1` Tree-less World (-100%) | yes |
| `globalDepositSpawnRate` | Global deposit spawn rate | `1` | `3` 300%, `2` 200%, `1.8` 180%, `1.6` 160%, `1.4` 140%, `1.2` 120%, `1` 100%, `0.8` 80%, `0.6` 60%, `0.4` 40%, `0.2` 20% |  |
| `surfaceCopperDeposits` | Surface copper deposit frequency | `0.12` | `1` Very common, `0.5` Common, `0.2` Uncommon, `0.12` Rare, `0.05` Very Rare, `0.015` Extremly rare, `0` Never |  |
| `surfaceTinDeposits` | Surface tin deposit frequency | `0.007` | `0.5` Very common, `0.25` Common, `0.12` Uncommon, `0.03` Rare, `0.014` Very Rare, `0.007` Extremly rare, `0` Never |  |
| `snowAccum` | Snow and ice | `true` | `true` Enabled, `false` Disabled |  |

### Spawn and death

| Key | Name | Default | Values | Locked |
|-----|------|---------|--------|--------|
| `gameMode` | Game mode | `survival` | `survival` Survival, `creative` Creative |  |
| `playerlives` | Player lives | `-1` | `1`, `2`, `3`, `4`, `5`, `10`, `20`, `-1` infinite |  |
| `startingClimate` | Starting climate | `temperate` | `hot` Hot (28-32°C), `warm` Warm (19-23 °C), `temperate` Temperate (6-14 °C), `cool` Cool (-5 to 1 °C), `icy` Icy (-15 to -10°C) | yes |
| `spawnRadius` | Random respawn radius | `50` | `10000` 10000 blocks, `5000` 5000 blocks, `2500` 2500 blocks, `1000` 1000 blocks, `500` 500 blocks, `250` 250 blocks, `100` 100 blocks, `50` 50 blocks, `25` 25 blocks, `0` 0 blocks |  |
| `graceTimer` | Grace timer | `0` | `10` 10 days before monsters appear, `5` 5 days before monsters appear, `4` 4 days before monsters appear, `3` 3 days before monsters appear, `2` 2 days before monsters appear, `1` 1 day before monsters appear, `0` No timer. Monsters spawn right away. | yes |
| `deathPunishment` | Death punishment | `drop` | `drop` Drop inventory contents, `keep` Keep inventory contents |  |
| `droppedItemsTimer` | Timer for items dropped on death | `600` | `300` 5 minutes, `600` 10 minutes, `1200` 20 minutes, `1800` 30 minutes, `3600` 1 hour |  |

### Survival rules

| Key | Name | Default | Values | Locked |
|-----|------|---------|--------|--------|
| `seasons` | Seasons | `enabled` | `enabled` Enabled, `spring` Off, always spring, `summer` Off, always summer, `fall` Off, always fall, `winter` Off, always winter |  |
| `daysPerMonth` | Days per month | `9` | `30` 30 days (24 real life hours), `20` 20 days (16 real life hours), `12` 12 days (9.6 real life hours), `9` 9 days (7.2 real life hours), `6` 6 days (4.8 real life hours), `3` 3 days (2.4 real life hours) |  |
| `harshWinters` | True winters | `true` | `true` Enabled, `false` Disabled |  |
| `blockGravity` | Block gravity | `sandgravel` | `sandgravel` Sand and gravel, `sandgravelsoil` Sand, gravel and soil with sideways instability |  |
| `caveIns` | Cave-ins | `off` | `off` Disabled, `on` Enabled |  |
| `allowFallingBlocks` | Allow falling blocks | `true` | `true`, `false` |  |
| `allowFireSpread` | Allow fire spread | `true` | `true`, `false` |  |
| `lightningFires` | Fire from lightning | `false` | `true`, `false` |  |
| `allowUndergroundFarming` | Allow underground farming | `false` | `true`, `false` |  |
| `noLiquidSourceTransport` | Prevent liquid source transport by buckets | `false` | `true`, `false` |  |
| `playerHealthPoints` | Player health points | `15` | `5` 5 hp, `10` 10 hp, `15` 15 hp, `20` 20 hp, `25` 25 hp, `30` 30 hp, `35` 35 hp |  |
| `playerHealthRegenSpeed` | Health regeneration rate | `1` | `2` Very fast (200%), `1.5` Fast (150%), `1.25` Slightly faster (125%), `1` Normal (100%), `0.75` Slightly slower (75%), `0.5` Slower (50%), `0.25` Much slower (25%) |  |
| `playerHungerSpeed` | Hunger rate | `1` | `2` Very fast (200%), `1.5` Fast (150%), `1.25` Slightly faster (125%), `1` Normal (100%), `0.75` Slightly slower (75%), `0.5` Slower (50%), `0.25` Much slower (25%) |  |
| `lungCapacity` | Lung capacity | `40000` | `10000` 10 seconds, `20000` 20 seconds, `30000` 30 seconds, `40000` 40 seconds, `60000` 60 seconds, `120000` 2 minutes, `3600000` 60 minutes |  |
| `bodyTemperatureResistance` | Body temperature hardiness | `0` | `-40`, `-30`, `-25`, `-20`, `-15`, `-10`, `-5`, `0`, `5`, `10`, `15`, `20` |  |
| `playerMoveSpeed` | Walk speed | `1.5` | `2` Fast, `1.75` Slightly faster, `1.5` Normal, `1.25` Slightly slower, `1` Slower, `0.75` Much slower |  |
| `creatureHostility` | Creature hostility | `aggressive` | `aggressive` Aggressive, `passive` Passive, `off` Never hostile |  |
| `creatureStrength` | Creature strength | `1` | `4` Deadly (400%), `2` Very Strong (200%), `1.5` Strong (150%), `1` Normal (100%), `0.5` Weak (50%), `0.25` Very weak (25%) |  |
| `creatureSwimSpeed` | Creature swim speed | `2` | `0.5` 50%, `0.75` 75%, `1` 100%, `1.25` 125%, `1.5` 150%, `1.75` 175%, `2` 200%, `3` 300% |  |
| `foodSpoilSpeed` | Food spoilage rate | `1` | `4` 400%, `3` 300%, `2` 200%, `1.5` 150%, `1.25` 125%, `1` 100%, `0.75` 75%, `0.5` 50%, `0.25` 25% |  |
| `saplingGrowthRate` | Tree sapling growth time | `1` | `16` Extremely slow (16x), `8` Much slower (8x), `4` Slower (4x), `2` Somewhat slower (2x), `1.5` Slightly slower (1.5x), `1` Normal (1x), `0.75` Slightly faster (0.75x), `0.5` Faster (0.5x), `0.25` Much faster (0.25x) |  |
| `toolDurability` | Tool durability | `1` | `4` 400%, `3` 300%, `2` 200%, `1.5` 150%, `1.25` 125%, `1` 100%, `0.75` 75%, `0.5` 50% |  |
| `toolMiningSpeed` | Tool mining speed | `1` | `3` 300%, `2` 200%, `1.5` 150%, `1.25` 125%, `1` 100%, `0.75` 75%, `0.5` 50%, `0.25` 25% |  |
| `propickNodeSearchRadius` | Prospecting Pick node search radius | `0` | `0` Disabled, `2` 2 blocks, `4` 4 blocks, `6` 6 blocks, `8` 8 blocks |  |
| `microblockChiseling` | Microblock chiseling | `stonewood` | `off` Off, `stonewood` Stone and Wood, `all` Most cubic blocks |  |
| `allowCoordinateHud` | Coordinate overlay | `true` | `true`, `false` |  |
| `allowMap` | World map | `true` | `true`, `false` |  |
| `colorAccurateWorldmap` | Color-accurate World map | `false` | `true`, `false` |  |
| `loreContent` | Lore content | `true` | `true`, `false` |  |
| `clutterObtainable` | Clutter obtainable | `ifrepaired` | `ifrepaired`, `yes`, `no` |  |

### Temporal stability

| Key | Name | Default | Values | Locked |
|-----|------|---------|--------|--------|
| `temporalStability` | Temporal stability | `true` | `true`, `false` |  |
| `temporalStorms` | Temporal storms | `sometimes` | `off` Off, `veryrare` Every 30-40 days, increase strength/frequency by 2.5% each time, capped at +25%, `rare` Approx. every 20-30 days, increase strength/frequency by 5% each time, capped at +50%, `sometimes` Approx. every 10-20 days, increase strength/frequency by +10% each time, capped at 100%, `often` Approx. every 5-10 days, increase strength/frequency by 15% each time, capped at +150%, `veryoften` Approx. every 3-6 days, increase strength/frequency by 20% each time, capped at +200% |  |
| `tempstormDurationMul` | Temporal storm length | `1` | `2` Much longer (200%), `1.5` Longer (150%), `1.25` Slightly longer (125%), `1` Normal (100%), `0.75` Slightly shorter (75%), `0.5` Shorter (50%), `0.25` Much Shorter (25%) |  |
| `temporalRifts` | Temporal Rifts | `visible` | `off` Off, `invisible` Invisible, `visible` Visible |  |
| `temporalGearRespawnUses` | Temporal gear respawn uses | `1` | `-1` Infinite, `20` 20 times, `10` 10 times, `5` 5 times, `4` 4 times, `3` 3 times, `2` 2 times, `1` One time |  |
| `temporalStormSleeping` | Sleeping during temporal storms | `1` | `0` Disallowed, `1` Allowed |  |

### Multiplayer

| Key | Name | Default | Values | Locked |
|-----|------|---------|--------|--------|
| `allowLandClaiming` | Land claiming | `true` | `true`, `false` |  |
| `classExclusiveRecipes` | Class exclusive recipes | `true` | `true`, `false` |  |
| `auctionHouse` | Auction house | `true` | `true`, `false` |  |
