# Green Dragon 1=1 parity checklist

Goal: full parity with **stock LoGD 1.1.2 (DragonPrime Edition)** — every
mechanic, formula, odds table, and cost transcribed exactly; **all prose and
names original to late.sh** (upstream text is CC BY-NC-SA and off-limits).

## Target / provenance

- **Source of truth: `jimlunsford/lotgd`** (github mirror of DragonPrime
  1.1.2, the final content-complete classic release; project ceased Sept 2019).
- **Local reference clone: `upstream-lotgd/` at the repo root** (gitignored).
  Always verify formulas against these files directly — never from memory or
  ad-hoc web fetches. If missing, re-fetch with
  `git clone --depth 1 https://github.com/jimlunsford/lotgd upstream-lotgd`.
  CC BY-NC-SA source: consult it, never copy prose/names or commit it.
- Newer lineages checked (2026-07): **NB-Core/lotgd** ("+nb", v2.0.5, Apr 2024)
  and **stephenKise/Legend-of-the-Green-Dragon** are PHP-8/MySQL-8/security
  modernizations of the *same game* — explicitly no new content or mechanics.
  So 1.1.2 stays the mechanics target. NB-Core is the tie-breaker when 1.1.2
  has an outright bug (their 2.0.1/2.0.2 fixed mount + mercenary-heal bugs).
- Defaults rule: upstream reads admin settings via `getsetting(key, default)`;
  **the shipped default is the number we port.** Notably `suicide` searching
  defaults **off**, `villagechance`/`gardenchance` default **0%** — stock
  installs don't have them, so neither do we.
- `e_rand(a,b)` = inclusive uniform int. PHP `round()` = half-away-from-zero,
  `(int)` = truncate toward zero.

## Already 1=1 (verified against source)

- Combat resolver (`lib/battle-skills.php` `rolldamage`): bell_rand
  inverse-normal roll, signed damage (glance heals), 1-in-20 triple crit,
  dmgmod stages, power moves >1.5/2/3/4×, reroll-until-progress, invulnerable.
  (`bell_rand` is a continuous inverse-normal approximation of upstream's
  441-row percentile table; z deltas ≲0.002 never move truncated integer
  damage.)
- Specialties (3 × 4 skills), use economy `floor(skill/3)+1`, gem advancement.
- Buff + companion engines; forest death (gold→0, exp×0.9); master fights
  (non-lethal loss, +5 soulpoints on win); shop ladder + 75% trade-in +
  level gating; healer full-heal cost `round(ln(level)·(missing+10))`;
  8 stock forest events at 15% (`forestchance`); exp curve + DK scaling;
  new-day spirits `e_rand(-1,1)+e_rand(-1,1)` (the −6 turn dock belongs to
  the *paid* resurrection only — see phase 1);
  interest gating (>4 unused turns or ≥100k ⇒ none).

## Phase 0 — core fidelity fixes (this pass)

- [x] **Forest victory payout** (`lib/forestoutcomes.php::forestvictory`):
  per-enemy gold roll `e_rand(0, creaturegold)` (the `dropmingold` branch is
  non-default); total gold re-rolled `e_rand(avg, avg·round((n+1)·1.2^(n-1)))`
  (single kill ⇒ `e_rand(g, 2g)`); per-enemy exp bonus
  `round(exp·(1+.25·(clvl−plvl)) − exp)`, `+dragonkills·level` when n>1,
  averaged over n, floored at `−exp+1`, positive bonus scaled `·1.05^(n-1)`;
  exp awarded = `round(Σexp/n) + bonus`.
- [x] **Gem drop**: on forest victory, if `level < 15`, `e_rand(1,25)==1` ⇒ +1
  gem (`forestgemchance` 25).
- [x] **Flawless turn refund**: no enemy did damage ⇒ if
  `level ≤ max(clvl)+0.5·(n−1)` refund the turn (`turns++`); otherwise
  message only. (`denyflawless` has no stock setters in our scope.)
- [x] **Mushroom save**: player at 0 HP on a *victory* clamps to 1.
- [x] **`buffbadguy` creature scaling**: base points
  `at+de dragon points + (maxhp − level·10)/5`, then
  `dk = round(points · (0.25 + 0.05·dragonkills/100))`; per creature:
  exp flux `±round(exp/10)`; `atk += e_rand(0,dk)`,
  `def += e_rand(0, dk−atkflux)`, `hp += 5·remainder`; gold/exp compensation
  `·(1 + .03·(atkflux+defflux) + .001·hpflux)` (`disablebonuses` default 1 =
  compensation ON).
- [x] **Forest level jitter, exact**: `if e_rand(0,2)==1 { plev = (e_rand(1,5)==1);
  nlev = (e_rand(1,3)==1) }`; slum `nlev++`, thrill `plev++`;
  `target = level + plev − nlev`. Thrill ×1.1 gold/exp applied **after**
  buffbadguy.
- [x] **Multi-fights** (`multifightdk` 10, `multichance` 25): at ≥10 dragon
  kills, 25% of searches spawn `e_rand(2,3)` enemies; slum
  `−e_rand(0,1)` and min level −1/−2 (coin flip); thrill `+e_rand(1,2)`,
  coin flip also target+1, min = target−1; `multi = clamp(multi, 1, level)`;
  overflow past the level cap converts to +1 enemy each.
  **Pack of monsters**: when multi>1, `e_rand(0,5)==0` ⇒ one creature cloned
  `multi` times, each at `e_rand(min,target)`. Non-pack: independent creatures
  at levels within `[min, target]`. Multi-kill gold multiplier + per-enemy exp
  bonuses via forestvictory above. Extra foes each strike the player every
  round; the player strikes the first living foe.
- [x] **Flee is a 1/3 roll** (`e_rand()%3==0`); failure = the foes still get
  their round.
- [x] **Dragon-kill gold reset**: on-hand gold is *not* retained —
  `gold = min(50 + 50·kills, 300)`; overflow gems `clamp(kills−7, 0, 10)`;
  flawless +150 gold +1 gem (unchanged); companions wiped (upstream resets
  the field). Extended by the 2026-07 new-day audit: `dragon.php`'s field
  loop reverts *every* column outside its preserve list, so the **bank
  balance (debt included), favor, any haunt mark, the race, and the
  specialty** reset too, and the wiped `lasthit` forces a **full new day as
  soon as the post-kill gates close** (`dawn_owed` → `Character::dawn`).
- [x] **Dragon points are chosen, not auto-applied**: each kill grants 1
  point; a forced spend gate (upstream: newday blocks until
  `count(dragonpoints) == dragonkills`) offers `hp` (+5 max HP), `ff`
  (+1 daily forest fight, permanent), `at` (+1 attack), `de` (+1 defense).
  `ff` spent today also adds +1 to today's pool (upstream spends before turn
  assembly). Migration: legacy saves (auto-boon era, 3 boons/kill + implicit
  ff≤10) keep their boons and get `ff = min(kills,10)`; grandfathered as
  over-granted, noted here so it stops surprising.
- [x] **Healer partial heals**: rows 100%,90..10; price `round(cost·pct/100)`
  off the rounded full cost; heal `round(missing·pct/100)`; free forced
  normalize down to max when over-healed.
- [x] **Bank loans/debt**: borrow up to `level·20` (`borrowperlevel`);
  balance goes negative; interest applies to debt every day regardless of
  turns used (the "work for it" gate only skips *positive* balances).
- [x] **Creature roster variety**: multiple original-named creatures per
  level (upstream ships ~250 forest rows; same-level rows share the band
  stats, so names-only variety is 1=1).
- [x] **`seendragon` is a daily flag** (`newday.php` clears it every dawn):
  fleeing or dying to the dragon no longer locks the seek out for the rest of
  the run. Found and fixed during the phase-1 source audit.
- [x] **`seenmaster` daily gate** (`train.php`): one master challenge per day
  — `seen_master_today` set when the challenge starts (persisted immediately),
  cleared on a win (`multimaster` default 1) and at every dawn (`newday.php`
  clears it unconditionally, paid resurrections included). Ported 2026-07
  with phase 4's first slice.

### Known deliberate deviations (single-player shape, documented)

- Creature table caps at level 16 (upstream has 17–18 easter-egg rows);
  multi-fight overflow clamps at 16 instead of 17.
- Doppleganger fallback (empty creature query) is unreachable with a static
  table — omitted.
- Companion incoming-damage model: foe rolls against a random companion each
  round rather than LoGD's single-target redistribution (pre-existing,
  see CONTEXT.md).
- `suicide` searching: stock default **off** — correctly absent.

## Phase 1 — the dead realm (`graveyard.php`, `shades.php`, `lib/graveyard/case_*.php`) — DONE

Implemented 2026-07, re-verified line-by-line against the local
`upstream-lotgd/` clone. The audit corrected two claims this section
originally shipped with:

1. **The passive wait-for-dawn revival is a plain new day.** `checkday()`
   redirects a dead player to bare `newday.php` (no `resurrection=true`), so
   turns are the full base + spirits + ff, and soulpoints/gravefights DO
   refresh. The −6 `resurrectionturns` dock and the skipped
   `playerfights`/`soulpoints`/`gravefights` resets apply **only to the paid
   resurrection**. (Our port used to dock −6 on the passive path — fixed.)
2. **A "graveyard-only roster" doesn't exist upstream**: the installer flags
   the *entire* forest table `graveyard=1` and `case_battle_search.php`
   overrides every stat anyway. The pool is pure flavor; we use a dedicated
   10-entry original-name cast (`data::GRAVEYARD_CREATURES`).

- [x] **New `Character` fields**: `favor` (upstream `deathpower`) and
  `grave_fights` (upstream `gravefights`), serde-defaulted. Both refresh on a
  normal new day (`grave_fights = 10` via `gravefightsperday`, soulpoints
  `= 50 + 5·level`) but not via the paid resurrection.
- [x] **While dead**: the graveyard replaces the village as the hub (Esc
  leaves the game; the village is unreachable until revival). Combat buffs
  can't follow (encounter-scoped) and specialty skills are hidden — upstream
  strips buffs on entry and calls `fightnav(false, ...)` (no specials).
  **Soulpoints are the HP pool**: fight setup swaps `hitpoints = soulpoints`,
  dead attack and defense are both `10 + round((level−1)·1.5)` (gear/boons
  irrelevant), and the remaining pool is written back after the fight —
  damage persists between torments. Max soulpoints is always computed
  `level·5 + 50` (`Character::max_soulpoints`), never stored.
- [x] **Torment fights** (`case_battle_search.php`): gated on
  `grave_fights > 0`, one spent per search (persisted at fight start, like
  forest turns). Foe stats override the flavor roster entirely:
  `shift = -1 if level < 5 else 0`; `atk = 9 + shift + int((level−1)·1.5)`;
  `def = atk · 0.7`; `hp = level·5 + 50`; its "exp" slot carries the **favor
  payout** `e_rand(10+round(level/3), 20+round(level/3))`. Victory: `favor +=
  payout`. Defeat: `grave_fights = 0`, soul pool written back at 0, no other
  penalty. Flee: 1-in-3 escape costing `min(favor, 5 + e_rand(0, level))`
  favor; failure gives the shade its round.
- [x] **Mausoleum** (`case_restore.php`): restore soulpoints to max for
  `round(10 · (max − soulpoints) / max)` favor (0..10 with depletion);
  enabled only when below max and affordable.
- [x] **Favor tiers** (`case_question.php`): tier messaging at <25 / ≥25 /
  ≥100 favor renders in the graveyard panel. The 25-favor haunt itself
  landed with phase 4's bounties+haunt slice (see that section).
- [x] **Paid resurrection** (`case_resurrection.php` + `newday.php`
  `resurrection=true`): 100 favor (deducted at the moment of resurrection),
  an immediate extra new day — bank interest settles, specialty uses refresh,
  full heal, `seendragon` clears, turns = `base + ff − 6` (floored at 0);
  soulpoints/grave fights are NOT refilled and `last_day` is untouched, so
  the real next dawn still rolls a full day.
- [x] **Death overlord NPC**: original name (`data::DEATH_OVERLORD`,
  "Morvane"); upstream's "Ramius" is theirs. All graveyard prose original.
- [x] **Death news hook**: graveyard defeats and resurrections write daily
  news (landed with phase 3's news system).

### Phase 1 deliberate deviations

- Shade defense is upstream's PHP float `(int)(9+shift+(level−1)·1.5) · 0.7`
  fed straight to combat; our integer combatant rounds it (±0.5).
- Torment foes draw from a 10-entry original dead-realm cast instead of the
  whole forest roster (upstream's pool is names-only there anyway).
- Searching with an empty soul pool isn't blocked (upstream doesn't gate on
  soulpoints either): the fight opens at 0 and the first blow ends it.

## Phase 2 — races + titles — DONE

Sources: `modules/race{human,elf,dwarf,troll}.php`, `lib/newday/setrace.php`,
`lib/titles.php`, `titleedit.php`. Implemented 2026-07, verified line-by-line
against the local `upstream-lotgd/` clone. Source-audit corrections to what
this section originally claimed:

1. **The cave-in death roll is strict**: `e_rand(1,100) < $vals['chance']`
   (`goldmine.php`), not `<=` — 90 ⇒ 89% death, 5 ⇒ 4%. Ported as `<`.
2. **A survived cave-in zeroes the day's turns** ("your close call scared
   you so badly that you cannot face any more opponents today"), it isn't a
   free walk-away. `percentgoldloss`/`percentgemloss` default 0, so a mine
   death costs no gold/gems (unlike a forest death).
3. **The race `newday` hook fires on the paid resurrection too**
   (`newday.php` runs `modulehook("newday")` regardless of the flag), so the
   human-analog's bonus fights soften the −6 dock: `10 + 2 − 6 = 6` turns.

- [x] **Gate order** (upstream `newday.php:100-104`): dragon points → race →
  specialty. `Mode::ChooseRace` is a forced choice, armed on load
  when `race` is unset and chained after the dragon-point gate; Esc leaves
  the door and the gate re-arms. The village specialty chooser stays as-is.
  `Character.race` (enum, serde default `None`); phase 3's transmutation
  potion resets it so the gate re-arms. **Per-run, not one-time** (corrected
  by the 2026-07 new-day audit): every dragon kill resets `race` — upstream's
  field loop — so the gate re-arms after each kill, and the day itself rolls
  only once the gates close (`settle_dawn`, matching `newday.php`'s
  gate-then-day flow).
- [x] **Race effects** (numbers exact; race names original — Plainsborn /
  Wealdkin / Deepfolk / Cragborn for the human/elf/dwarf/troll analogs):
  - *Plainsborn*: +2 forest fights per day (`bonus` default **2**), in
    `roll_new_day` and `resurrect` (correction 3).
  - *Wealdkin*: +`1 + floor(level/5)` defense, a flat add in
    `Character::defense()` (numerically identical to upstream's recomputed
    `defmod` buff). No effect while dead (`dead_combatant` ignores it, as
    upstream strips buffs at the graveyard).
  - *Cragborn*: same formula on attack, in `Character::attack()`.
  - *Deepfolk*: forest creature gold ×1.2 rounded, applied after `buff_foe`
    and before thrill ×1.1 (verified: the `creatureencounter` hook fires at
    the tail of `buffbadguy()`, `lib/forestoutcomes.php:200`; thrill applies
    after in `forest.php`).
  - **Goldmine cave-in** (`raceminedeath`): on the 19–20 roll,
    `e_rand(1,100) < chance` (90 default / 5 Deepfolk) kills; otherwise the
    lucky escape zeroes the day's turns (corrections 1–2).
  - Elf/troll `pvpadjust` (same bonus defending in PvP) — landed free with
    phase 4's PvP: the defender's stats come from `attack()`/`defense()`,
    which already fold the race add in. The attacking side matches too:
    upstream's `racialbenefit` buff is `allowinpvp=1`/`allowintrain=1`
    (the one stock buff so flagged), and ours rides the same stat fold.
  - The dwarf-analog's exclusive mercenary (bear companion: atk 1 +2/lvl,
    def 5 +2/lvl, hp 25 +25/lvl, ability defend, 4 gems + 600 gold) joins
    the phase-3 mercenary camp as a race-gated listing.
- [x] **DK titles** (`titles` table + `lib/titles.php` `get_dk_title`):
  `data::TITLES` holds `(threshold, first-style, second-style)` rows —
  **one rung per kill 0..=31, upstream's 32-row seed** (fixed 2026-07 from
  a compressed 10-tier ladder, phase-5 finding 1) — **all title strings
  original** (upstream's Farmboy→God ladder is theirs). Selection: highest
  `threshold <= dragon_kills`, random among rows sharing it; re-rolled on
  every dragon kill (`dragon.php`) with the news/log fired every kill,
  changed or not; stamped onto never-titled saves at load; shown before
  the name in the stat rail. `Character.title: String` (serde default
  empty = never titled).
- [x] **Address style**: `Character.style` (enum `First`/`Second`, serde
  default `First`) picks the title column where upstream reads `sex`. The
  actual one-time chooser is phase 3's (with the romance/bard hooks); until
  then everyone renders first-style titles.
- [x] **Title news hook**: "has earned the title X" writes to the daily news
  on every re-title (landed with phase 3's news system).

## Phase 3 — single-player buildings — DONE

Sources: `stables.php`, `mercenarycamp.php` + the `companions` installer
seed, `inn.php` + `lib/inn/*`, `modules/cedrikspotions.php`,
`modules/sethsong.php`, `modules/drinks.php` + its installer seed,
`modules/lovers.php` + `modules/lovers/*`, `modules/outhouse.php`,
`modules/darkhorse.php`, `modules/game_{dice,fivesix,stones}.php`,
`news.php` + `lib/addnews.php`.

Implemented 2026-07, each system re-verified line-by-line against the local
`upstream-lotgd/` clone before porting (see the corrections subsection at
the end of this phase). New modules: `inn.rs` (bard + romance resolvers),
`tavern.rs` (the three games' logic); the buildings' menus live in
`state.rs`, the drink/potion/mount/mercenary economies in `model.rs` +
`data.rs`, the news + shared Five Sixes pot in `svc.rs` over migrations
096/097.

**Cross-cutting: the address-style choice.** Upstream keys titles, the
romance partner, and one bard outcome off a binary `sex` field. Adapt: a
one-time **address style** choice at character creation (or the newday gate)
with two flavors; it picks the title column, which of the two original
romance NPCs is "your partner", and bard outcome 15. Field
`style: u8`/enum on `Character`, serde default.

**New daily-flag fields on `Character`** (all reset in `roll_new_day`):
`lodged_today`, `flirted_today`, `heard_bard_today` (count vs 1/day),
`used_outhouse_today`, `hard_drinks_today`, `fivesix_plays_today`,
`drunkenness` (0–100, survives the day, see hangover), `mount_rounds_left`.

### Stables
- 3 mounts (original names), priced in **gems** 6 / 10 / 16 (gold 0):
  +1 / +2 / +3 daily forest fights (into `roll_new_day` like `ff` points) and
  an **offense buff, player attack ×1.2**, lasting 20 / 40 / 60 combat
  rounds per day (`mount_rounds_left` refreshed to the mount's rounds each
  newday; decrement per fight round while >0; while >0 fold atkmod 1.2 into
  the round mods).
- Trade-in when switching or selling: refund `round(cost·2/3)` (gems);
  affordability check counts the refund. Selling outright pays the same ⅔.
- Feeding exists upstream but `allowfeed` defaults **0** — skip it.
- Field: `mount: u8` (0 = none, else mount id).

### Mercenary camp
- 2 stock hires (original names; the dwarf-analog bear from phase 2 is a
  third, race-gated):
  1. fighter — **573 gold + 4 gems**; atk `5 + 2·level`, def `1 + 2·level`,
     maxhp `20 + 20·level` (level = buyer's level at purchase); ability
     **fight**.
  2. field-medic — **1000 gold + 3 gems**; atk `1 + 1·level`, def `5 + 5·level`,
     maxhp `15 + 10·level`; ability **heal 2** (restores up to 2 HP to the
     most-wounded ally each round: player first, then other companions,
     then itself — and still makes its fight roll).
- Cap: **1 hired companion** (`companionsallowed` 1). Summons (Bonecall)
  bypass the cap (upstream `ignorelimit`) — mark summoned companions with a
  flag so the cap query skips them. No duplicate same-name hires.
- Healing companions (here and at the healer):
  `round(ln(level+1) · (missing + 10) · 1.33)` gold → full HP.
- Companion struct gains `ability` (Fight/Defend/Heal(n)/Magic(n)) and
  `ignore_limit: bool`; hired ones persist across days; all wiped on dragon
  kill (already true) and on death (already true).
- Upstream extras we defer: `defend` (one companion soaks/round) and
  `magic` (self-HP-cost nuke) have no stock sellers — implement the enum
  arms when content needs them.

### The Inn (hub with sub-rooms)
- **Room for the night**: `round(level · (10 + ln(level)))` gold, once/day
  (`lodged_today`). Paying from the bank adds a **5% fee**. Effect today:
  flavor + the flag; in phase 4 the flag makes you PvP-attackable at the inn
  (upstream stores it as the "bodyguard level" too — flavor only in 1.1.2).
- **Barkeep bribes** (paid whether or not they work):
  gems: 1/2/3 gems ⇒ success `amount · 30`% (30/60/90).
  gold: `level·10` / `level·50` / `level·100` ⇒ success
  `(amount/level − 10) · (50/90) + 25`% = 25% / ≈47.2% / 75%.
  Success unlocks (per visit): the **specialty switch** (change path, keep
  `specialty_skill`; uses recompute) and, in phase 4, the who's-lodged PvP
  list. Single-player: switch is the real prize.
- **Potion shelf** (upstream Cedrik's; our NPC name original; all prices in
  gems, default **2 gems per dose**; buying N gems of one potion gives
  `floor(N/2)` doses, remainder refunded; the reset potions cap at 1 dose):
  1. charm potion: +1 charm per dose.
  2. vitality potion: **permanent** +1 max HP (and +1 current) per dose;
     survives dragon kills (upstream `carrydk` default 1) — implement as its
     own counter field folded into `max_hitpoints()`, NOT `dragon_hp_bonus`
     (which feeds investment scaling; upstream's extra-HP pref does feed
     `buffbadguy`'s `(maxhp − level·10)/5` term, so DO include it in
     `investment_points()`).
  3. mending draught: heal to max, then **overheal +20** per dose (the
     healer's normalize clips it free later — correct, upstream matches).
  4. forgetting potion: specialty → None (village chooser re-arms). 1 dose.
  5. transmutation potion: race → None (gate re-arms next day) + a sickness
     debuff: atk ×0.75, def ×0.75, **10 rounds, survives the new day**
     (needs a small persisted-debuff slot on `Character`). 1 dose.
- **The bard** (once/day): roll `e_rand(0,18)`:
  0: +2 turns · 1,2,6,13,14: +1 turn · 3: +`e_rand(10,50)` gold ·
  4: HP = `round(max(maxhp,hp) · 1.2)` (overheal) · 5,11: −1 turn (floor 0) ·
  7: −`round(maxhp·0.10)` HP (min 1) · 8: −5 gold (if ≥5) · 9: +1 gem ·
  10,12: heal to max · 15: +1 charm (style A) / +1 turn (style B) ·
  16: −`round(maxhp·0.20)` HP (min 1) · 17: nothing · 18: −1 charm.
- **Drinks + drunkenness** (3 originals mirroring the stock stat lines;
  cost = `level × costperlevel`; refuse service above **66** drunkenness;
  max **3 hard drinks**/day):
  1. house brew — 10/level, +33 drunk, not hard; roll 2:1 →
     2/3: heal `+10% of maxhp`; 1/3: +1 turn; buff: atk ×1.25, 10 rounds.
  2. fire shot — 15/level, +50 drunk, **hard**; ALWAYS both: HP
     `e_rand(−5,15)` and turns `e_rand(−1,1)`; buff: atk ×1.1, def ×0.9,
     dmg ×1.5, 12 rounds.
  3. black cask — 25/level, +50 drunk, **hard**; roll 2:3 →
     2/5: HP `e_rand(−10,−1)`; 3/5: turns `e_rand(1,3)`; buff: dmg ×1.3,
     damage-shield ×1.3, 15 rounds.
  HP results floor at 1; turn results floor at 0. **Hangover**: at newday,
  if drunkenness > 66 ⇒ −1 turn; drunkenness and hard-drink count reset to 0
  daily either way; death/dragon kill also zero drunkenness. **Sober-up**:
  each forest search multiplies drunkenness by 0.9 (round). Comment slurring
  landed with phase 4's commentary leftovers (see that section).
- **Romance** (upstream lovers module; our two partner NPCs original; partner
  = opposite style). Once/day (`flirted_today`). Flirt ladder — success test
  `e_rand(charm, T) >= T` (guaranteed at charm ≥ T):
  | # | T | success | failure |
  |---|---|---------|---------|
  | 1 | 2 | +1 charm (cap 4) | — |
  | 2 | 4 | +1 charm (cap 7) | — |
  | 3 | 7 | +1 charm (cap 11) | — |
  | 4 | 11 | +1 charm (cap 14) | −1 charm (if 0<charm<10) |
  | 5 | 14 | +1 charm (cap 18) | −1 charm (if 0<charm<13) |
  | 6 | 18 | −2 turns, +1 charm (cap 25), news item | −1 charm |
  | 7 (marry) | needs charm ≥ 22 | married (sentinel field), news | **turns = 0** |
  Married daily visit replaces flirting: 1/4 chance of a rebuff (−1 charm),
  else +1 charm and a "protection" buff (def ×1.2, 60 rounds). Marriage
  upkeep at newday: `charm −= e_rand(1, max(1, round(0.85·sqrt(dragon_kills))))`;
  at charm ≤ 0 ⇒ divorced (field cleared, charm 0, news). Field
  `married: bool` (upstream uses an INT_MAX sentinel in `marriedto`).
- **Non-flirt chat**: pure flavor bucketed by `charm + e_rand(-1,1)` in
  threes (≤0, 1–3, …, 16–18, 19+) — write 8 original lines per partner.

### The outhouse (forest nav, once/day)
- Private stall: pay **5 gold** (needs the gold) → wash-up: 60% finds **3
  gold** (`giveback` — note: less than the 5 paid), then independent 25%
  **+1 gem** (`giveturnchance` defaults 0 ⇒ no turn roll).
- Free public stall → wash-up: 60% then 1/3 → find 3 gold.
- **Either** wash fires sober-up ×0.9 (not just the paid stall).
- Skipping the wash: `e_rand(1,100) >= 50` (**51%**) → lose 1 gold (only if
  ≥1 on hand) + the embarrassing news item — the news fires even when there
  was no gold to lose.

### Dark Horse Tavern (restore `events::Tavern` into a full room)
Menu: the old gambler (3 games), the tavern board (phase 4's commentary),
the barman's enemy intel (phase 4, see its section), leave. Games:
- **Dice**: bet any amount ≤ gold. Player rolls d6, may keep or reroll
  (max 3 rolls). Old man then rolls with this AI: roll 1 — keep if
  `r > player || r == 6`; roll 2 — keep if `r >= player`; roll 3 — forced.
  Outcome: his final > yours ⇒ lose the bet; equal ⇒ push; less ⇒ win.
- **Five Sixes**: pay **5 gold** (10 plays/day). The pot: starts **100**,
  +5 per play, hard cap **5000** (overflow pocketed by the house). Roll
  5d6 and count sixes: 5 ⇒ win the whole pot (pot resets to 100, news);
  4 ⇒ win `round(pot·0.10)` (deducted, news); 3 ⇒ `round(pot·0.05)`
  (deducted, news); ≤2 ⇒ nothing. **The pot is one shared global** — needs
  a tiny shared store (a one-row table or kv; LISTEN/NOTIFY not needed, read
  fresh per play inside a transaction).
- **Stones**: a bag of **6 red + 10 blue**. Bet on "like pairs" or "unlike
  pairs". Draw two random stones at a time; **the piles belong to the two
  players** (source-verified — not a matched-pile/mixed-pile split): the
  pair lands +2 on *your* pile when it comes up the way you called (like ⇒
  same color, unlike ⇒ different), on the old man's otherwise. Stop when
  the bag empties or either pile exceeds 8. Bigger pile wins the even-money
  bet; tie is a push.

### Daily news
- New table `greendragon_news` (migration + `late-core` model, patterned on
  the existing `greendragon_characters`): id, utc day-number, `user_id`
  (nullable — null = system), body text, created_at. **180-day expiry** on
  read or via the daily rollover.
- Village menu entry "Daily News": day-paged view (today, yesterday, …),
  newest first, ~50/page.
- Writers (all landed phases): forest/graveyard deaths (with an original
  taunt line pool), dragon kills (+ new title), master-challenge losses,
  marriages/divorces/the ladder-6 flirt, Five Sixes wins, resurrections,
  outhouse embarrassment. Phase 4 adds PvP and bounty items.
- Write an original **taunt pool** (~15 lines) picked at random for death
  news — upstream has a `taunts` table; strings must be ours.

### Creature flavor leftovers
- Battle-end one-liners (ours): a shared original pool of dying lines /
  gloats drawn when a forest fight ends (upstream stores per-creature
  win/lose strings; a shared pool keeps our prose budget sane).
- Bandit purse-cut: five larcenous creature names (`data::BANDIT_CREATURES`)
  roll 1-in-8 per round, once per fight, while the player carries > 200
  gold; the cut is 20% of carried gold. Killing every foe recovers the cut
  in full off the corpse; fleeing forfeits it. **Original to late.sh** —
  source-verified that stock 1.1.2 ships *no* mid-fight steal mechanic
  (`creatureaiscript` exists but no stock script uses it), so these numbers
  are ours, not a port.

### Phase 3 audit corrections + deliberate adaptations

Source-audit corrections to what this section originally claimed (the specs
above are already fixed to match):

1. **Both mercenaries cost gems too** (4 and 3 on top of the gold).
2. **Stones piles are player-owned vs old-man-owned**; the like/unlike call
   only routes each drawn pair to one of the two people.
3. **Outhouse**: the no-wash penalty roll is `>= 50` on a d100 (51%); the
   news item isn't gated on actually losing the coin; the wash "refund" is
   the 3-gold `giveback` (a net −2 on the paid stall); sober-up fires on
   both stalls' washes.
4. **Lovers**: rungs 1–3 have no failure penalty; rung 6's failure costs a
   charm point whenever charm > 0 (no upper bound); the wedding applies the
   lover's buff immediately and costs nothing; a rejected proposal only
   zeroes turns (no charm loss). The rung-6 news fires on success only.
5. **Bard**: case 13 is +1 turn for everyone (only its flavor is
   sex-keyed); case 15 is the mechanical fork (charm vs turn) — ours keys it
   on address style (Second ⇒ +1 charm, matching the partner mapping);
   case 4 is `round(max(maxhp, hp) · 1.2)` (an overheal).
6. **Bribes** are paid win or lose (`e_rand(0,100) < chance`); the potion
   shelf is *not* bribe-gated (it hangs off the bartender screen freely);
   the specialty switch itself is free once the bribe lands.
7. **Drinks**: the newday hangover threshold is a hardcoded 66 (not the
   `maxdrunk` setting); drink HP deltas add to current HP uncapped (an
   overheal), floored at 1.
8. **Potions**: upstream sells `floor(gems/2)` doses per purchase with the
   remainder refunded — ours sells one dose per menu pick, which is
   arithmetically identical; a repeat transmutation dose *adds* 10 sickness
   rounds rather than reapplying.

Deliberate single-player/TUI adaptations (documented, not oversights):

- **Bets are a fixed stake ladder** (10/50/100/everything) standing in for
  upstream's free-text bet box; still capped by gold on hand.
- **The inn room** sets the daily flag + flavor — upstream's "room" is
  the site's log-out-for-the-night; since phase 4's PvP the flag also
  routes you to the inn's target list (the barkeep's keys). Bank payment
  keeps the +5% fee and requires a positive balance covering it.
- **The Dark Horse** restores the gambler's three games; the comment board
  and the barman's paid enemy-intel were phase-4 features and have since
  landed (see phase 4). Abandoning a game mid-hand forfeits nothing,
  exactly like navigating away upstream (the stake settles only at the end).
- **Five Sixes settles against the shared pot atomically** in the DB
  (migration 097); the stake is paid up front and refunded if the
  round-trip fails.
- **Charm floors at 0** (our field is unsigned); upstream lets the bard's
  mockery drive it negative. Nothing downstream distinguishes negative from
  zero charm in the stock systems we ship.

## Phase 4 — multiplayer

Sources: `lib/commentary.php`, `pvp.php` + `lib/pvplist.php`, `news.php`,
`mail.php` + `lib/mail/*` + `lib/systemmail.php`, `gypsy.php`, `clan.php` +
`lib/clan/*`, `modules/dag.php`, `hof.php`, `list.php`, `gardens.php`,
`rock.php`, `lib/graveyard/case_haunt*.php`. Written to be implementable
standalone. **Architectural shift**: `svc` grows a read-other-characters /
online-roster path; the session stays authoritative for its own character
(see CONTEXT.md "toward multiplayer" notes).

### Build order
commentary ✓ → roster/HoF ✓ → gypsy ✓ (folded into the commentary slice — it
is just a paid door onto the shade section) → PvP ✓ → bounties + haunt ✓ →
barman's enemy intel ✓ + rewards wiring ✓ (the two small leftovers, 2026-07)
→ clans ✓ (2026-07) → gardens ✓ / veterans' rock ✓ → bank transfers ✓ + the
mail decision resolved ✓ (2026-07) → the commentary leftovers ✓ (pagination
+ new-post markers, drunken slurring — 2026-07). Commentary first: five
other features are just sections of it. **Phase 4 is complete and the
checklist is empty**: the port is feature-complete against stock 1.1.2.

### `commentary` — the one chat primitive — DONE

Implemented 2026-07 (migration 098, `greendragon_commentary` model,
`commentary.rs` for the pure rules, svc load/post round-trips,
`Mode::Commentary` + a typing line in `state`/`screen`/`ui`). Re-verified
line-by-line against `lib/commentary.php` and every stock caller before
porting. **Source-audit corrections** to what this section originally
claimed (specs below already fixed):

1. **Display limits**: village square **25** (not 10), inn 20, Dark Horse
   board 10 (the `commentdisplay` default), shade 25, gardens **30**,
   veterans' rock **30**, clan halls 25. Allowance = `round(limit/2)` ⇒
   13 / 10 / 5 / 13 / 15 / 15 / 13.
2. **The allowance is windowed, not a flat daily counter**: a player may
   post while their posts-from-today **among the section's newest `limit`
   rows** number fewer than the allowance — once older posts scroll out of
   the display window, they stop counting ("once some of your existing
   posts have moved out of the comment area, you'll be allowed to post
   again").
3. **The venue verb is baked at post time**: a non-emote post in a
   non-"says" room is converted on insert to `:verb, "..."` — so a lament
   posted in the graveyard still "despairs" when read through the gypsy's
   trance. Verbs: gardens "whispers", rock "boasts", shade "projects"
   (gypsy) / "despairs" (graveyard), everything else "says".
4. **Retention**: comments expire on the same `expirecontent` default as
   news — 180 days (`newday_runonce.php`) — pruned on write.

- [x] Table `greendragon_commentary` (098): id, section, `user_id`
  (nullable; null = system line), `name` (speaker snapshot at post time),
  body, created. Index (section, created desc, id desc).
- [x] **Post limit**: windowed allowance per correction 2; the speak row
  shows "N left today" when under 3 remain (upstream's talkform hint).
- [x] **Emotes**: leading `:`, `::`, or `/me` renders as name + rest;
  system lines (no author) render bare. Newlines can't occur (single-line
  input); a space is inserted after any 45-char unbroken run (upstream's
  `([^\s]{45})([^\s])`, applied left to right); the typing budget is 200
  chars, less `verb.len() + 11` in baked-verb venues (upstream's
  `maxlength`).
- [x] **Rejections**: empty or bare-marker posts (our "silence" line);
  double post = identical body + same author as the section's **newest**
  row, checked at insert time against the live table.
- [x] **Rooms landed**: village square, the inn's long table, the Dark
  Horse etchings, the gardens, the veterans' rock (`rock.php`: a plain
  weathered stone to anyone without a dragon kill), and the shade channel
  from both sides — free while dead, or through the gypsy's paid trance.
  Clan halls + the waiting room landed with clans (2026-07): the halls are
  the one allowance-exempt venue, speaking in each clan's custom verb.
- [x] **The gypsy tent** (`gypsy.php`): pay `level * 20` gold per visit to
  project into the shade section. That's the whole building.

**Pagination + new-post markers landed 2026-07** (the long-deferred
leftover, source-audited first):

- **Pages** (`comscroll`): page 0 is the newest window, each page one
  window older (`ORDER BY commentid DESC LIMIT com*limit, limit`). The
  menu mirrors upstream's nav row: *older* opens off a full window
  (upstream shows Previous when `rowcount >= limit`), *newer* off a
  scrolled-back page, and *refresh always lands on page 0* (upstream's
  link drops the comscroll param — Refresh and "Last Page" share a URL).
  The `cid`/`lastcommentid` incremental re-fetch is a web caching artifact;
  skipped.
- **The watermark** (`recentcomments`): `newday.php:254` sets
  `recentcomments = lasthit` then `lasthit = now`, and both fire on paid
  resurrections too — so "new" means *posted since your previous dawn*.
  Ours: `comments_seen_before_day` on the blob, advanced from `last_day`
  (exactly upstream's `lasthit` at the blob's day granularity — the one
  deviation: ≤1 day coarser, erring toward marking more as new). Rows with
  `day >= watermark` render with a bright marker (upstream's `new.gif`,
  `postdate >= recentcomments` — the `>=` kept).
- **First unseen** jump: page `round(count/limit + 0.5) - 1` (PHP
  half-away rounding, the exact-multiple overshoot quirk included), shown
  when > 0 and not already open. The count uses the marker's `>=` (upstream
  counts strict `>` — an instant apart there, a day apart at our
  granularity, so the jump follows the marker set).
- **The allowance still counts the loaded window** (upstream's `counttoday`
  loop runs over the displayed buffer): paging back to a window without
  your posts frees the speak row even while the newest window is full of
  you — upstream's own quirk, ported by construction.

**Drunken slurring landed 2026-07** (`modules/drinks/dohook.php` +
`drunkenize.php`, the other leftover):

- The hook fires exactly where upstream's `modulehook("commentary")` sits —
  after nothing, before the 45-char run-breaking and verb baking — so a
  slurred line can outgrow the 200-char typing budget, as upstream's
  server-side slur does.
- Above 50 drunkenness the verb gains "drunkenly" — which then bakes even
  in "says" rooms (the `!= "says"` test trips on "drunkenly says"), 1=1.
- `drunkenize` 1=1: until replacements reach `drunkenness/500` of the
  original length, 9-in-10 doubles the *first* occurrence of a random slur
  letter (case-matched, skipped inside a `*hic*`; repeated picks compound
  at the same spot — "aa" → "aaa", upstream's quirk) and 1-in-10 inserts
  `*hic*` at a random spot with the five sequential stagger-shifts;
  adjacent hics collapse to `*hic*hic*` after. Emotes skip, sober lines
  skip, the slur is baked into the stored row (permanent, as upstream).
- Omitted: the backtick color-code skip (no color codes in our bodies) and
  the `noslur` player pref (no per-player prefs; everyone slurs).

Remaining deliberate single-player/TUI adaptations (documented, not
oversights):

- Speaker names are the bare character name snapshotted at post time — no
  DK-title prefix (upstream's `accounts.name` carries the title). The clan
  `<TAG>` prefix landed with clans, snapshotted into the name the same way.
- All three emote markers compose identically (name + a space + the rest);
  upstream's `::` variant differs only in marker length.
- No GM `/game` inserts or moderation tools; system lines are reserved for
  future writers (haunts, bounties).

### Online roster + Hall of Fame — DONE

Sources: `list.php`, `hof.php`. Implemented 2026-07, re-verified line-by-line
against the local `upstream-lotgd/` clone before porting. **Source-audit
corrections** to what this section originally claimed (the specs below are
already fixed to match):

1. **`dragonage` is a snapshot, not a counter.** The live counter is `age`
   ("days since level 1" — effectively days since the last dragon kill): +1
   at every new day, the paid resurrection's included, and reset to 0 by a
   kill (`age` is absent from `dragon.php`'s `$nochange` preserve list).
   Each kill stamps `dragonage = age` (the Hall of Fame's "Days" column) and
   `bestdragonage` keeps the minimum — both *are* preserved through the kill
   reset. Upstream's quirk that a same-day second kill would stamp 0 (and
   clobber the best) is kept 1=1.
2. **`resurrections` also resets on a dragon kill** (not in the preserve
   list): it counts revivals *since the last kill* — +1 whenever a dead
   character greets a new day, dawn or paid (`newday.php` increments while
   `alive != true`, regardless of the resurrection flag).
3. **Every ranking has the most/least toggle** (not just charm/HP), and the
   tie-break (level → experience → acctid) **follows the toggle's
   direction** — upstream reuses `$order` for every ORDER BY column. The
   speed ranking is inverted: its "best" sorts ascending.
4. **The wealth fuzz is the sort key too**: `hof.php` orders by the
   rand()-perturbed `gold + goldinbank` (a fresh ±5% per render; debt counts
   via the signed cast), so neighbors can swap between reloads. The "your
   rank" count compares others' fuzzed totals against your exact one.
5. **The gems ranking shows rank + name only** — exact counts never render.
   Kills shows kills/level/days/best-days, charm shows gender+race, tough
   sorts `maxhitpoints` and shows race+level, resurrects shows level, days
   shows best-days (`IF(x,x,'Unknown')` when 0).
6. **The percentile line** is `count(stat >=|<= yours)` — inclusive of
   yourself, the operator flipping with the toggle (and inverted for days) —
   over the *filtered* total, rounded and floored at 1: "top N%". The kills
   ranking only renders it for dragon-slayers; kills filters
   `dragonkills > 0`, days additionally `bestdragonage > 0`, and the
   filtered count is also the pagination denominator.
7. **`list.php`'s default landing is "Warriors Currently Online"**
   (`loggedin` AND `laston` within `LOGINTIMEOUT` 900s); the all-warriors
   roll is the paged view; the name search interleaves `%` between typed
   characters (a subsequence match) capped at `maxlistsize` 100. All three
   share the total order level DESC → dragonkills DESC → login ASC ("so
   that the ordering is total"), and the columns run alive / level / name /
   location (+ online marker) / race / sex / last-on.

- [x] **`Character` fields** (serde-defaulted): `age` (seeded 1 at creation
  — upstream rolls a fresh account's first new day at first login),
  `dragon_age`, `best_dragon_age`, `resurrections`, wired per corrections
  1–2; and `online`, a presence flag mirroring `loggedin` (stamped true by
  the entry save and every in-play save, cleared by the leave save; a
  crashed session leaves it stale and the 15-minute window absorbs it,
  exactly like upstream's `loggedin`+`laston` pairing).
- [x] **Online detection** reads `greendragon_characters.updated` (nearly
  every action saves, so it tracks activity like `laston`) ANDed with the
  blob's `online` flag, window 900s. Entering the door now always saves
  immediately — the presence stamp — not only on a day rollover. No new
  column or migration needed.
- [x] **`svc.load_roster()`**: one read of all rows
  (`GreenDragonCharacter::load_all`), each blob decoded into a `RosterEntry`
  (titled name for display/search, bare handle for the sort, level / alive /
  race, the ranked stats, signed wealth, online, idle seconds).
- [x] **Warrior list** (`Mode::WarriorList`): the online slice (default; its
  menu row re-reads the roster), the full roll, and the subsequence name
  search typed on the talk line — ordering and columns per correction 7
  (location renders village/graveyard; "Seen" is the humanized last save).
- [x] **Hall of Fame** (`Mode::HallOfFame`): the seven rankings per
  corrections 3–6; a ranking switch resets the page while the most/least
  flip keeps it (upstream's links do the same); your row starred; the
  wealth-fuzz footnote; the percentile line.

Deliberate single-player/TUI adaptations (documented, not oversights):

- **Pages hold 15 rows, not 50** (a TUI panel vs a web page), and every
  warrior-list view pages (upstream leaves the online/search views unpaged,
  capping search hits at 100).
- **No sex/gender column**: our analog (address style) is a title-column
  pick, not an identity — the list drops the column and the charm ranking
  shows race only.
- The alive column is two-state (village/graveyard); a PvP death lands the
  victim in the graveyard like any other, so upstream's "Unconscious"
  tri-state never arises here.
- No write-mail/bio links (no in-door
  mail), and both screens are village-nav only (upstream also links the
  list from logged-out pages and bios).
- The percentile line renders even when the days ranking's filter excludes
  you ("top 1%" — upstream's floor-at-1 quirk), kept 1=1.

### Gypsy tent (village) — DONE
- Pay `level · 20` gold per visit → read/post the **shade** commentary
  section (the dead post there free from the graveyard). That's the whole
  building; menu: pay / leave. Landed with the commentary slice above.

### PvP ("slay other warriors", village + inn) — DONE

Sources: `pvp.php`, `lib/pvplist.php`, `lib/pvpsupport.php`,
`lib/pvpwarning.php`, `lib/inn/inn_bartender.php`, `battle.php` (the pvp
branches: `suspend_buffs`/`suspend_companions`/`apply_bodyguard`/surprise),
`newday.php` (`playerfights`). Implemented 2026-07, re-verified line-by-line
against the local `upstream-lotgd/` clone before porting. **Source-audit
corrections** to what this section originally claimed (the specs below are
already fixed to match):

1. **The immunity experience bar is `<= 1500`**, not `< 1500`
   (`pvpwarning`'s test; `pvplist`'s filter is the same set negated:
   `age>5 OR dragonkills>0 OR pk>0 OR experience>1500`).
2. **The level-15 defender still collects the gold.** `pvpdefeat` assigns
   the zero to a typo'd `$wonamount` while paying `$winamount`, so only the
   experience is zeroed against a level-15 sleeper. Ported 1=1, bug and all
   (the attacker-side level-15 zeroing in `pvpvictory` is real and zeroes
   both).
3. **Engage re-checks `abs(level diff) <= 2`** (`setup_target`) — one level
   wider *below* than the list's `[mine−1, mine+2]` band. Both kept: the
   list filters `[−1,+2]`, the engage transaction re-checks `±2`.
4. **The sleeper defends at full health** (`maxhitpoints AS
   creaturehealth`), whatever wounds they saved with — and their stored
   attack/defense carry gear, boons, and the race bonus (our
   `attack()`/`defense()` fold the race add in, which *is* upstream's
   elf/troll `pvpadjust` re-add).
5. **Almost nothing stock sets `allowinpvp`**, so the buff/companion nuance
   collapses: every buff and companion sits PvP out on both sides (drinks,
   the lover's ward, mounts, mercenaries, Bonecall — all shelved). The one
   stock exception (found by the 2026-07 PvP audit, finding 17): the
   elf/troll `racialbenefit` buff sets `allowinpvp=1` (and `allowintrain=1`),
   so the *attacker* keeps their race bonus upstream — which ours reproduces
   by construction, the race add living in `attack()`/`defense()` rather
   than a buff (the defender's half is the `pvpadjust` re-add, correction 4).
   The one *injected* buff in any PvP fight is the inn **bodyguard**
   (`apply_bodyguard(1)`: defender attack ×1.05, attacker defense ×0.95,
   whole fight) — every inn target has `bodyguardlevel = boughtroomtoday = 1`.
6. **The sleeper can strike first**: `battle.php` rolls surprise 50/50 for
   single-foe fights, PvP included ("%s's skill allows them to get the
   first round").
7. **No flee, no skills, enforced by conversion**: `op=run` becomes a
   *fought round* ("your pride prevents you from running"), a skill pick is
   stripped ("your honor prevents..."). Ours: the fight menu is one Attack
   row and Esc resolves a round.
8. **`playerfights` decrements at engage** (`pvp.php`), not at resolution —
   abandoning a fight still spent the attack; a *refused* engage spends
   nothing. The `pvpflag` dogpile stamp lands on the target at engage too.
9. **Upstream's inn room is the site log-out** (`inn_room.php`: `location =
   inn`, `loggedin = 0`, session cleared): "who's upstairs" can hold
   players from days ago, since `boughtroomtoday` only clears at *their*
   next new day. Ours mirrors that with the `lodged_today` blob flag, which
   lingers the same way.
10. **The victim's losses read two clocks**: experience −5% of the
    *engage-time* snapshot; gold = `min(gold at engage, gold at
    settlement)` re-read fresh, the bank absorbing any shortfall
    (`pvpvictory`'s IF guard).
11. **The defender's reward has a leveled-down guard**: `pvpdefeat`
    re-reads their level and skips the payout if it dropped since engage
    (a mid-fight dragon kill would make the reward "way too rich").
12. The list's `slaydragon=0` filter is a web-flow artifact (set by
    `dragon.php`, cleared on the next village pageview) — no equivalent
    exists here; omitted.

- [x] **`Character` fields** (serde-defaulted, no migration):
  `player_fights` (3/day via `PVP_FIGHTS_PER_DAY`, refilled by
  `roll_new_day` only — the paid resurrection skips it, exactly like grave
  fights), `pk` (permanent immunity forfeit), `pvp_engaged_at` (the
  `pvpflag` timestamp, stamped through the DB by attackers), and
  `pvp_reports` (see the mail adaptation below).
- [x] **Target lists** (`Mode::PvpList(Fields|Inn)`): built off the roster
  snapshot — someone else, alive, offline (the presence window), past
  immunity, level in `[mine−1, mine+2]`, venue split on `lodged` — ordered
  level/experience/kills descending; dogpiled rows show disabled ("hunted
  too recently"); the other venue's count renders as a rumor line. The
  fields list hangs off the village ("Slay Other Warriors", fights-left in
  the row); the inn list is the barkeep bribe's second prize
  (`Mode::BarkeepEar`: who's upstairs / the specialty switch).
- [x] **Immunity warning + forfeit** (`pvpwarning`): the still-immune see
  the warning entering either list — a deliberate widening (2026-07 PvP
  audit, finding 18): upstream warns only at the fields list (`pvp.php:25`;
  `inn_bartender.php` requires the lib but never calls it), so an immune
  newbie attacking from the inn forfeits unwarned there. Informational
  only; the forfeit itself fires identically at engage on both venues —
  a successful engage while immune sets `pk = 1` forever.
- [x] **Engage** (`setup_target` as a row-locked transaction in `svc`):
  re-checks against the target's *fresh* blob (found → level ±2 → pvpflag
  10 min → awake → alive, upstream's order and precedence), stamps
  `pvp_engaged_at`, and snapshots the fight stats + gold/exp. Refusals log
  and re-read the list.
- [x] **The fight**: `FoeKind::Pvp` through the existing resolver — no
  persistent-buff injection, companions benched, the inn bodyguard as the
  lone buff, the 50/50 first-strike roll, victory-at-0-HP staunched to 1
  (`pvp.php`'s "bit of cloth").
- [x] **You win**: exp `round(10% · engageExp)` ± the level-difference
  bonus, applied locally; gold waits on the victory settlement (the fresh
  purse re-read) — both zeroed at level 15. The victim loses the taken
  gold and 5% engage-time exp, dies (our standard death hygiene), and gets
  a report; news in the field/inn variant.
- [x] **You lose**: `pvp_die()` (gold 0, −15% exp, graveyard); the sleeper
  collects `round(10 · myLevel · ln(max(1, myGold)))` + `round(10% ·
  myExp)` (exp zeroed if they're 15; gold paid regardless, correction 2)
  through the defeat settlement with the leveled-down guard; taunted news.

**Cross-player writes (the architectural piece).** Settlements are the
door's first writes to *another* player's blob. Three mechanisms keep them
from clobbering (or being clobbered by) a live session:

1. **Row-locked delta transactions**: engage and both settlements `SELECT
   ... FOR UPDATE`, decode the *fresh* blob, apply deltas (never a stale
   whole-blob overwrite), and write back with `update_data_keep_updated` —
   which deliberately does not touch `updated`, since being attacked isn't
   presence. Concurrent attackers serialize on the row lock and the second
   sees the first's `pvp_engaged_at`.
2. **The in-process write gate**: each transaction holds the victim's
   per-user save gate, ordering it against any in-flight fire-and-forget
   saves from a session in this process.
3. **The presence heartbeat**: a live session re-saves after 4 idle
   minutes (`HEARTBEAT_SECS`), so it can never drift out of the 15-minute
   online window and get targeted mid-play (upstream's `laston` refreshes
   every page load; ours only refreshed on action before this).

The residual race — the victim entering the door *during* the fight, then
saving over the settlement — is upstream's own (`pvpvictory` UPDATEs while
the victim may be mid-request) and is bounded by fight length against a
target that was offline 15+ minutes; accepted and documented.

Deliberate single-player/TUI adaptations (documented, not oversights):

- **Mail → in-blob reports**: the plan said "map systemmail onto the
  existing notification/DM systems", but the site's notifications are
  mention-shaped (actor/message/room) and a DM would put words in the
  attacker's mouth — so settlement reports ride the victim's own blob
  (`pvp_reports`, written atomically with the settlement) and drain into
  the game log at their next entry, which is exactly when upstream's mail
  got read anyway. Revisit only if an out-of-door ping proves wanted.
- **Venue is the `lodged` flag**, not upstream's location string (we have
  no location column; the village/inn split is the only one that exists).
- All engage/settle/news/report prose is original.
- The victim's death applies our standard hygiene (companions, buffs,
  drunkenness cleared) — upstream's victim UPDATE leaves them; ours keeps
  the "companions don't follow past the grave" invariant every other death
  path has.
- Abandoning mid-fight is only possible by leaving the door (Esc fights a
  round instead); the attack stays spent and the target stays flagged,
  matching upstream's walk-away.

### Bounty board (upstream Dag; our NPC name original; sits in the inn)

Sources: `modules/dag.php` + `modules/dag/{install,dohook,run,
misc_functions}.php`, `lib/pvpsupport.php` (the `pvpwin` hook fires inside
`pvpvictory` only — the attacker's win; a sleeper's win pays nothing).
Spec audited line-by-line 2026-07 against the local clone; **implemented
2026-07** (migration 099 + the `greendragon_bounty` model, svc round-trips,
`Mode::DagTable`/`BountyList`/`BountyTarget`/`BountyAmount` off the inn).
Source-audit corrections to what this section originally claimed (the specs
below are already fixed to match):

1. **Bounty immunity is Dag's own, one-notch-lenient test**, not the PvP
   list's: a target is refused when `level < 3` OR (`age < 5` AND
   `dragonkills == 0` AND `pk == 0` AND `exp < 1500`) — strict `<` on age
   and exp where `pvpwarning`/`pvplist` use `> 5` / `> 1500`, so a warrior
   at exactly age 5 or exactly 1500 exp is still PvP-immune yet already
   bountyable. Ported 1=1.
2. **Self-set bounties forfeit but stay open**: on a PvP win, rows the
   winner set are skipped (Dag "keeps" them) and are NOT closed — the next
   hunter can still collect them.
3. **Maturity gates visibility, collection, and the target's own total**
   (each filters `set_at <= now`), but the `200·level` open-total cap
   counts immature rows too (`status = open`, no date filter).
4. **No news on placement** — placing is anonymous; a target only learns
   their matured total by asking Dag ("price on yer head").
5. **Bounty gold is exempt from the level-15 zeroing**: the `pvpwin` hook
   pays after `pvpvictory`'s (possibly zeroed) payout, straight onto gold,
   with its own news line and an extra line in the victim's mail.
6. **Closure on the target's dragon kill or deletion** sets status closed
   with **winner = none ("the Green Dragon" collects)**, `closed_at`
   stamped; deleted targets also close lazily on list render. Closed rows
   expire after `expirecontent/10` = **18 days** (an admin-page sweep
   upstream; ours prunes on write, like commentary/news).

- [x] Table `greendragon_bounties` (migration 099 + a `late-core` model):
  id, target user_id, setter user_id (nullable = system), amount, `set_at`
  (**activation delay**: insert stamps `now + e_rand(0, 14400)` seconds; a
  bounty is *matured* once `set_at <= now`), status open/closed, winner
  (nullable = the house), closed_at.
- [x] **Dag's table** (inn menu row, our NPC name original): the greeting
  shows *your* open matured total; nav to the wanted list + set-a-bounty.
- [x] **Placing** (≤5/day via a daily blob counter reset in `roll_new_day`;
  at the cap the form is refused outright): pick a target (talk-line
  subsequence search over the roster, >100 matches = "narrow it down",
  multiple = disambiguation pick), amount typed on the talk line
  (`abs(int)`). Check order 1=1: no match → self-bounty refused → level +
  immunity (correction 1) → `amount < 50·targetLevel` → `gold <
  round(amount·1.10)` (the 10% fee) → `amount + sum(ALL open on target) >
  200·targetLevel` (correction 3, `>` strict — exactly reaching the cap is
  allowed) → insert + charge. No placement news. Any qualifying target
  works: no level band vs the setter; online, offline, or dead alike.
- [x] **Wanted list**: open + matured rows aggregated per target; default
  sort level desc (ties amount desc), toggleable to amount desc; columns
  amount / level / name / location-or-Online / alive / last-seen off the
  roster snapshot (no sex column, matching the warrior list).
- [x] **Collecting**: inside `pvp_settle_victory`'s transaction, sweep the
  victim's open matured bounties: rows set by others close (winner = the
  attacker) and their sum lands on the attacker's gold **on top of** the
  normal PvP payout (correction 5 — not level-15-zeroed); rows the attacker
  set stay open (correction 2) with a "Dag keeps that share" log line.
  News item + a bounty line appended to the victim's report.
- [x] **Closure hooks**: the target's dragon kill (a svc call from the kill
  path) and character deletion close all open rows to the house
  (correction 6); prune closed rows older than 18 days on write.

Deliberate single-player/TUI adaptations (documented, not oversights):

- **The broker's refusals surface as disabled rows at pick time** (yourself,
  the level floor, the immunity test) instead of upstream's rejection after
  finalize — the check set is identical, the timing one screen earlier. The
  cap check keeps its upstream position (last, inside the placement).
- **The cost is taken up front and refunded on a refusal** (the Five Sixes
  pattern); upstream "leaves the coins on the table" — net effect identical.
- The placement runs the cap check and insert in one transaction, so
  concurrent setters can't jointly pass the cap (upstream has no guard;
  strictly safer, same rules).
- The wanted list drops the sex column (as the warrior list), pages at 15
  rows, and breaks the gold sort's ties by level (upstream leaves them
  unspecified); aggregation keys on the target id in SQL, not the display
  name (upstream merges rows by name).
- The daily counter is a blob field (`bounties_set_today`) reset in the
  shared new-day effects — upstream's module pref, same reset timing (the
  hook fires on paid resurrections too).
- Amounts are typed on the talk line, digits only (upstream's free-text box
  through `abs(int)`).
- All broker prose is original; the NPC is ours ("Varn").

### Haunt (graveyard, needs the phase-1 favor economy)

Sources: `lib/graveyard/case_haunt{,2,3}.php`, `case_question.php` (the
nav gating), `newday.php:281` (the dock). Spec audited line-by-line
2026-07; **implemented 2026-07** (`Mode::Haunt` off the graveyard menu, the
`haunt` svc transaction, the dock in the shared new-day effects).
Source-audit corrections to what this section originally claimed (the specs
below are already fixed to match):

1. **There is no target filter beyond "not already haunted"**: any account
   matches the search — dead, brand-new, PvP-immune, online, any level,
   even **yourself** (upstream never checks self; kept 1=1 as a quirk: 25
   favor to maybe dock your own turn).
2. **The 25 favor is charged when the roll happens** — success or failure
   alike — but a refused target (already haunted, or vanished between
   search and attempt) costs nothing.
3. **Failure is public too**: news "X unsuccessfully haunted Y!" plus one
   of **six** failure flavor lines (ours original). Success: news + the
   target's report (upstream systemmails "You have been haunted by X").
4. **The dock fires on ANY next new day** — dawn or the paid resurrection
   (the `hauntedby` block in `newday.php` is unconditional): −1 turn, a
   message naming the haunter, mark cleared. Upstream doesn't floor the
   decrement; ours saturates at 0 (unsigned field, documented deviation).

- [x] `Character.haunted_by: String` (serde default empty; stores the
  haunter's **name**, exactly as upstream's varchar).
- [x] **The favor menu** (the existing tier panel in the graveyard):
  "Haunt a foe (25 favor)" appears at ≥25 favor, alongside the
  resurrection row at ≥100 (`case_question.php`'s two tiers).
- [x] **Target pick**: talk-line subsequence search over the roster (cap
  100, "narrow it down"); rows show name + level, sorted level then name
  (upstream `ORDER BY level,login`).
- [x] **The attempt** (a row-locked cross-player transaction, the PvP
  pattern — the "no active haunt" check must read the fresh blob):
  `haunted_by` non-empty ⇒ refuse, no charge; else deduct 25 favor (yours,
  locally), roll `e_rand(0, yourLevel) > e_rand(0, targetLevel)` (strict —
  ties fail); success writes `haunted_by = your name` + a report entry in
  the same transaction. News both ways (correction 3).
- [x] **The dock**: in the shared new-day effects (dawn AND the paid
  resurrection, correction 4): `haunted_by` non-empty ⇒ turns saturating
  −1, a log line naming the haunter, mark cleared.

Deliberate single-player/TUI adaptations (documented, not oversights):

- **The dawn dock's message rides the report drain**: the load-path new day
  rolls in `svc`, so the "X haunted your dreams" line is appended to
  `pvp_reports` before the entry save and surfaces with the other sleep
  reports; the paid resurrection (in-session) logs it directly off
  `NewDayFx`. The success notification to the victim is a report too (the
  PvP mail adaptation).
- **The self-haunt quirk lands with upstream's own effect**: the mark is
  written to your stored blob, and your live session's next save clobbers
  it — exactly what upstream's end-of-request session save does. You're out
  25 favor and the news item either way.
- The turn dock saturates at 0 (upstream's `turns--` has no floor; our
  field is unsigned).
- All prose is original: the six fumble vignettes (`data::HAUNT_FUMBLES`),
  the news lines, the warden's framing.

### The barman's enemy intel (the Dark Horse bartender) — DONE

Source: `modules/darkhorse.php` (`darkhorse_bartender`, lines 100–214).
Implemented 2026-07, audited line-by-line against the local clone first.
**Source-audit corrections** to what the docs previously claimed (both had
called this "a bribe-priced read of the online roster" — wrong twice):

1. **A flat 100 gold per name** — no bribe gate (the barman talks to
   anyone), no level scaling. The bribe economy belongs to the *inn's*
   barkeep; the Dark Horse barman just charges per question.
2. **The search runs over ALL characters** (`accounts WHERE locked=0`), not
   the online roster: offline, dead, PvP-immune, brand-new, and **yourself**
   included (no self filter — 100 gold to hear about yourself, kept 1=1).
3. **The charge lands only after the row is found**: gold `>= 100` is
   checked before the read; a vanished target refuses without charging; a
   purse under 100 gets the mock "cheapskate" stat block, also free.
4. **Over 100 hits truncates to the top hundred** (ordered level DESC) with
   a "too many names" line — a truncation, *not* the broker's "narrow it
   down" refusal; the two searches genuinely differ upstream.

- [x] **The counter** (`Mode::TavernBartender` off the taproom hub):
  entering kicks a roster read (the search's index); the intro shows the
  price and your purse.
- [x] **The search** (`Mode::IntelTarget`): talk-line subsequence match
  (the shared `name_matches`), ordered level DESC (upstream's ORDER BY),
  truncated per correction 4. Every match is pickable — no refusal rows
  (correction 2).
- [x] **The paid sheet** (`Mode::IntelSheet`, `svc.load_enemy_intel`): a
  fresh single-row read at pay time (upstream SELECTs the accounts row
  then charges), decoded and laid out row for row — titled name, race,
  level, max hitpoints, **gold on hand** (fresh, not the roster snapshot),
  weapon, armor, attack, defense. Our `attack()`/`defense()` fold the race
  bonus in, which is exactly upstream's `adjuststats` hook (the elf/troll
  display adds). Capped by the **charm comparison** in its exact bands:
  equality first, then strict `mine−10 > theirs` / `mine > theirs` /
  `mine+10 < theirs` / else — ten-apart exactly lands in the narrow bands.
- [x] **The mock sheet**: same rows, no answers, no charge (correction 3).

Deliberate single-player/TUI adaptations (documented, not oversights):

- The **"Learn about colors" menu is omitted**: it teaches the web UI's
  backtick color codes with a live practice form — meaningless in the TUI.
- Level ties in the search sort break by name (upstream leaves them to
  MySQL); the level-DESC ordering itself is upstream's.
- Walking off mid-read costs nothing (the sheet never poured); upstream's
  page renders synchronously so the case can't arise there.
- All prose original: the barman's voice, the sheet framing, and the mock
  sheet's non-answers (upstream's lisping bartender and his insult block
  are their prose). The paid sheet adds a Race row to the mock sheet's
  shape only where upstream's real sheet shows race too.

### Clans — DONE

Sources: `clan.php`, `lib/clan/*.php` (start/default/membership/motd/
withdraw/applicant*/detail/list/waiting/func), `lib/constants.php` (the rank
values), `lib/commentary.php` (the clan-tag render + the clan-section
allowance skip), `lib/all_tables.php` (the `clans` schema), `common.php`
(the dangling-membership self-heal), `village.php:211` (the nav),
`list.php:77` (online clan members), `dragon.php` (the preserve list).
Spec audited line-by-line 2026-07 against the local clone; **implemented
2026-07** (migration 101 + the `greendragon_clan` model, membership fields
on the character blob, svc round-trips over the PvP cross-player patterns,
ten `Mode::Clan*` screens off the village's "Clan Halls" row, and the two
new commentary rooms). **Source-audit corrections** to what this section
originally claimed (the specs below are already fixed to match):

1. **Clan halls have no posting allowance**: `talkform` skips the
   posts-today count entirely for `clan-*` sections — members chat without
   limit. The shared `waiting` section is *not* exempt (window 25,
   allowance 13, verb "says").
2. **Promote/demote walk a step ladder, clamped at your own rank**
   (`clan_nextrank`/`clan_previousrank` pop the founder rung off first):
   promote = one rung up (0→10→20→30, never to founder 31), target strictly
   below you, the write clamped `LEAST(yours, next)`; demote = one rung
   down, allowed on your equals but never yourself, and **hidden when the
   rung below is applicant** — a member (10) cannot be demoted, only
   removed. The founder's one self-demotion is the "step down as founder"
   row (31→30). Remove needs `target ≤ yours` and never yourself. Only
   officers+ (rank > 10) see the ops column at all. Applicant acceptance IS
   the promote row (0→10) on the membership page — there is no separate
   accept flow (and no acceptance mail; only `modulehook`s).
3. **A clan with no real members is lazily deleted at list render**: both
   the public list and the application list count `clanrank > 0` and DELETE
   rows counting zero — applicants alone don't keep a clan alive. A
   dangling membership (clan row gone) self-heals at page load
   (`common.php`: clanid/clanrank reset to 0).
4. **Leaderless auto-promote runs on hall view AND on a leader's
   withdraw**: no member above officer ⇒ the highest-ranked, oldest-joined
   member (rank > 0, `ORDER BY clanrank DESC, clanjoindate`) is promoted
   straight to leader (30, never founder). A withdrawing solitary leader
   with no other members left deletes the clan (clearing any stragglers).
5. **Founding validation**: name 5–50 chars of letters, spaces, apostrophes
   and dashes only; tag ("short name") 2–5 chars, letters only; both
   unique; fee `goldtostartclan` 10,000 gold + `gemstostartclan` 15 gems,
   checked and charged at approval; the founder's rank is literally
   `CLAN_LEADER+1` (31).
6. **The commentary tag renders for rank > 0 only** — applicants stay
   bare-named — as `<TAG>` before the name, rank-colored upstream, in
   *every* comment area, from a live join against the poster's current
   membership.
7. **MOTD and description** (≤4096 chars upstream) are officer+ edits, each
   stamping its author (shown by name); the **custom talk verb** (≤15
   chars, blank = "says") is leader+ only and is baked into non-emote posts
   exactly like any venue verb. The desc-block (`descauthor=INT_MAX`) is
   moderation tooling — out of scope.
8. **Membership page ordering** is rank DESC, dragon kills DESC, level
   DESC, join date ASC (columns rank/name/level/DKs/joined/last-on + the
   total-DK footer); the public detail page orders rank DESC, join date ASC
   (rank/name/DKs/joined, same footer). Both lists order clans by member
   count DESC. `list.php?op=clan` is the online-members slice (the standard
   online filter + `clanid`), total-ordered like the online list.
9. **Notifications**: applying system-mails every officer+ (and mails the
   *applicant* a description reminder when the clan has one); a member's
   withdraw mails the officers; an applicant's withdraw only deletes the
   stale application mail. Nothing mails on promote/demote/remove.

- [x] Table `greendragon_clans` (migration 101): id, name (unique), tag
  (unique, both case-insensitively — upstream's MySQL collation), motd +
  author, description + author, custom talk verb. Membership on
  `Character`: `clan_id`, `clan_rank` (0 applicant / 10 member / 20
  officer / 30 leader / 31 founder), `clan_joined_at`, and the denormalized
  `clan_tag` (see adaptations). All survive dragon kills (`dragon.php`'s
  preserve list) and death.
- [x] **The lobby** (village "Clan Halls", rank < 10): the registrar's
  desk — apply (clan pick off the member-count-ordered list; the officers+
  get the notice, and a chartered clan earns the registrar's read-the-
  charter reminder, upstream's two mails), file a new clan (name, tag, the
  fee — same check set; ours takes the fee before the name/tag uniqueness
  tests where upstream validates first, a refusal-order-only difference),
  the public list (→ per-clan detail
  roll, `detail.php`'s ordering + total-DK footer), and, once applied, the
  waiting area + withdraw-application rows (an applicant's withdraw is
  purely local, as upstream only deletes the stale mail).
- [x] **The hall** (rank ≥ 10, the village row walks straight in): MOTD +
  charter with author names, per-rank counts, total clan DKs; the hearth —
  commentary section `clan-{id}` (window 25, the custom verb, no
  allowance); the membership ledger with promote/demote/step-down/remove
  per correction 2 (rank writes are row-locked cross-player transactions,
  clamped `LEAST(yours, next)` against the fresh blob); the motd/charter/
  verb editor (officer+/leader+); online clan members (the warrior list's
  clan slice, presence-filtered); the shared waiting room; withdraw with
  the confirm step, succession, and empty-clan deletion per correction 4.
- [x] **The leaderless auto-promote** runs inside the own-hall load only
  (`clan_default.php`; the public detail view doesn't heal foreign clans);
  a vacancy falling to the *viewing* session also updates the live
  character in place, exactly as upstream patches `$session`.
- [x] Officer notifications (application, member withdraw) and the
  dissolved-clan notice ride the `pvp_reports` drain — the established
  mail adaptation.
- [x] The commentary tag: `<TAG>` before the name for rank > 0 posters, in
  every room.
- No stat buffs — clans are social only in stock 1.1.2.

Deliberate single-player/TUI adaptations (documented, not oversights):

- **The tag and speaker name are snapshotted at post time** (our
  commentary rows already snapshot the name; upstream re-joins accounts →
  clans live at render, so its tags update retroactively when someone
  leaves). Same trade the name column already made; rank colors dropped
  with the rest of the color-code system.
- **`clan_tag` is denormalized onto the character blob** (set at
  apply/found, cleared on leave/removal/dissolution): tags are immutable
  here — upstream's rename is superuser tooling, out of scope — so the
  copy can't go stale.
- **MOTD and charter are single talk lines capped at 200 chars** standing
  in for upstream's 4096-char textareas; the editor starts blank instead
  of prefilled, and an empty submit clears the board (as upstream's empty
  POST does).
- **The empty-clan sweep gets a founding grace (1 hour)**: our member
  writes are fire-and-forget where upstream's were synchronous, so a
  brand-new clan must not be reaped before its founder's save lands.
- **Author names are stored as snapshots** on the clan row (upstream joins
  `acctid` and already breaks on renames — its own mail-cleanup comment
  admits as much).
- **Rank changes against a live session can be clobbered by that session's
  next save** (the blob is session-authoritative) — upstream's
  end-of-request session save has the same race; the hall re-reads on
  every entry, so it self-corrects at their next visit.
- The founding fee is taken up front and refunded on a refusal (the
  Five Sixes/bounty pattern); upstream checks then charges — net effect
  identical.
- The membership ledger and detail roll page at 15 rows (the TUI panel);
  the two clan pickers don't page (clans are few).
- The withdraw confirm keeps upstream's yes/no step; Esc anywhere backs
  out without withdrawing.
- All prose is original: the registrar ("Maren", ours), the lobby, the
  hall, every notice and refusal line.

### Bank gold transfers — DONE

Source: `bank.php` (`op=transfer`/`transfer2`/`transfer3`), `newday.php`
(the counter resets). Found missing during the 2026-07 mail-decision prep —
this checklist never listed it, yet `allowgoldtransfer` defaults **1**, so
stock installs ship it. Audited line-by-line, then implemented 2026-07: the
last stock multiplayer *feature*.

- [x] **The window's gates** (`bank.php`): the nav shows at level ≥ 3
  (`mintransferlev`) or any dragon kill; a negative balance is refused at
  the window itself (upstream's in-debt teller line), not on the nav.
- [x] **Recipient search** (transfer2): the interleaved-`%` subsequence
  match, >100 hits = narrow it down, exact matches float first (upstream's
  `ORDER BY login=... DESC`); ours runs on the roster snapshot through the
  broker's talk-line flow.
- [x] **The checks** (transfer3, upstream's order): the whole holding
  (`gold + goldinbank < amt`, the balance signed), the sender's daily cap
  (`amountouttoday + amt > level·25`, `maxtransferout`), the recipient's
  per-transfer cap (`amt > theirLevel·25`, `transferperlevel` — upstream's
  refusal *says* "per day" but the check is per transfer; kept 1=1), their
  daily receive count (`transferredtoday >= 3`, `transferreceive`), the
  worthwhile minimum (`amt < senderLevel`), and the self-transfer refusal.
- [x] **The settle**: the draw is hand-first with the shortfall out of the
  bank (upstream's negative-gold overflow branch); the sum lands in the
  recipient's *bank*; `amount_out_today`/`transfers_received_today` book
  both sides and reset unconditionally at newday (`newday.php:244`,
  resurrection days included; neither is in `dragon.php`'s reset list, so
  a mid-day dragon kill preserves both — same 1=1). The recipient's
  systemmail becomes a `pvp_reports` line (the established mail
  adaptation). No news item — transfers are private, like placements.
- [x] **Cross-player write**: the recipient's half is the PvP shape — the
  write gate, `SELECT ... FOR UPDATE`, fresh-blob re-checks (upstream's
  per-transfer cap and receive count are finalize-time reads of the
  accounts row too), then the deposit, the counter bump, and the clerk's
  note in one transaction via `update_data_keep_updated`.

Deliberate single-player/TUI adaptations (documented, not oversights):

- **Name first, then the sum** (the broker's two-screen flow); upstream's
  single form takes both, then confirms. The check set and order are
  otherwise upstream's; the self-transfer surfaces as a disabled row at
  pick time (the broker pattern) rather than a finalize refusal.
- **The gold is drawn up front and refunded on a refusal** (the Five
  Sixes/bounty pattern), each part back where it came from. The
  recipient-side checks settle inside the transaction, so a stale roster
  read can't overfill an account.
- An in-flight settlement disables the transfer row (one runner at a
  time); upstream's synchronous page can't race itself.
- All prose is original; our banker is nameless (upstream's Elessa is
  theirs).

### Mail — integration decision — RESOLVED (2026-07): in-door reports only
- Upstream: 50-unread inbox cap, 1024-char bodies, 14-day retention,
  system mail from id 0 — plus an **opt-in email ping** on new mail
  (`lib/systemmail.php`, `emailonmail`/`systemmail` prefs), upstream's own
  out-of-game notification.
- **Decision: no in-door mailbox, and no out-of-door ping for now.** Every
  stock systemmail moment in our scope already rides the in-blob
  `pvp_reports` drain (PvP results, haunts, bounty lines, clan notices,
  bank transfers); player-to-player mail maps onto late.sh's DMs, which
  exist outside the door. The ping (the email analog) is **deferred
  pending an app-wide integration discussion**: the site's notification
  schema is mention-shaped (NOT-NULL actor/message/room FKs onto real chat
  rows), no door holds a Chat/Notification service handle today, and the
  session-local terminal notifier can't reach offline players — so a real
  ping means new design (a bot-user DM à la the clubhouse bartender, or a
  new notification kind), decided at the app level, not per-door.
- The remaining upstream mail writers (bios, referrals, admin/su alerts,
  donator points) belong to systems we don't ship.

### Gardens + the veterans' rock — DONE
- Gardens: a commentary room with a 0% random-event chance (stock default)
  — pure social corner, plus the nav. Landed with the commentary slice.
- Veterans' rock: commentary room gated on `dragon_kills > 0`; non-veterans
  see only a flavor dead-end. Landed with the commentary slice.

### Rewards wiring (the long-standing TODO) — DONE

Implemented 2026-07, mirroring the NetHack milestone shape
(`door/nethack/award.rs`), fired from the dragon-kill arm in `state.rs`
next to the bounty-closure hook:

- [x] **Migration 100** seeds the `greendragon_dragon_slain` reward
  template: 10,000 chips (the NetHack-Amulet / Lateania-Archdemon tier),
  `per_event` claim policy, paid once per account through
  `credit_lifetime_reward_template`.
- [x] **Profile badge**: the rankless `GDS` ("Green Dragon Slayer") award,
  granted with the chips on the first kill only (double-deduped: the
  lifetime template and the `NOT EXISTS` award insert).
- [x] **Activity feed**: `ActivityGame::GreenDragon` (key `greendragon`);
  every kill publishes "prevailed in the Green Dragon (dragon kill #N)" —
  the feed line is per kill, the chips/badge first-kill-only, exactly the
  Lateania split. `activity_game()` on the door now returns the variant
  instead of `None`.
- This is a late.sh integration, not a LoGD port — no upstream file to
  audit; the in-door counterparts (news, titles, dragon points) are the
  ported parts and unchanged.

## Phase 5 — adversarial audit backlog (2026-07)

A multi-agent sweep (2026-07) audited 13 of 19 planned surfaces before being
stopped; its surviving findings are recorded below. The sweep is replaced by
the **solo-audit flow**: one Fable session per step, full attention on one
small part, fix at the end of the step, then reset context.

### Solo-audit flow (read me first, future Fable)

Session shape: work the step queue **in order, step after step, fixing as
you go**, within one session. Don't stop after a single step — keep going
until your context nears the **200-300k token mark**, then finish the step
in hand, record everything here, and tell the user it's time to reset
(`/clear`). A step too big to finish inside the budget gets split: check
off the finished half, leave the remainder as its own unchecked box with a
scope note (see the new-day split for the shape).

Per-step protocol:

1. Pick the **first unchecked box** in the step queue below.
2. Re-read "Target / provenance" at the top of this file. Ground rules:
   `upstream-lotgd/` is the only source of truth (never memory);
   `e_rand(a,b)` inclusive; PHP `round()` half-away-from-zero vs `(int)`
   truncation; verify **comparison operators**, not just formula shapes;
   shipped `getsetting` defaults only; this file's own claims are claims,
   not facts. Deliberate deviations and the out-of-scope list are not
   findings, but verify each deviation still behaves as its bullet says.
3. For a **fix step**: re-trace every listed finding yourself (both sides,
   the cited lines) before touching code — including the "verified" ones.
   Refuted on re-trace ⇒ strike it with a note instead of fixing.
   For an **audit step**: compare the system number by number, operator by
   operator, both directions (upstream behavior we lack, port behavior
   upstream lacks that isn't documented as ours).
4. Apply the fixes. `cargo check --tests` is the verification; do not run
   the test suite or commit (the user does both).
5. Record the outcome here: check the box, strike or annotate the findings
   handled, append any new findings to the list, correct any doc bullet the
   work contradicted.
6. Move to the next unchecked step — or, if the context budget is nearly
   spent, stop and tell the user what closed this session so they can
   `/clear`.

### Step queue

Fix steps (already audited by the sweep; findings below):

- [x] **Fix: combat + specialties** — findings 2, 3, 9, 10, 11. All five
  re-traced against upstream and confirmed, all five fixed (2026-07):
  `Companion.attack/defense` are now f64 (old integer saves deserialize
  clean), the skeleton keeps its .5 stats, aura heals `round(regen/3)` per
  aura buff, heal-ability companions no longer strike, power-move bounds
  round like `e_rand`, creature gold L5→199 / L6→233 (all 16 rows
  recomputed against `creature_gold`; only those two were off).
- [x] **Fix: dragon approach + flee** — finding 4. Re-traced and confirmed;
  fixed 2026-07: `Mode::DragonApproach` (enter / run-for-the-inn, the run
  costing 1 charm floored at 0; Esc walks back to the forest free, as
  upstream's other navs stay clickable); `seen_dragon` now set at the
  approach (upstream `forest.php:52`), not on entering; the dragon fight
  drops its Flee row (`fightnav(true,false)` — skills stay) and Esc fights
  the round ("the tail blocks the exit", upstream's `op=run` conversion).
  Also landed while in the file: the dragon seek moved from the village
  menu to the forest (`lib/forest.php:23`, where upstream's nav lives), and
  the slumming nav now hides at level 1 (`lib/forest.php:15` — the sweep's
  nav-visibility nit).
- [x] **Fix: healer + bank access** — findings 5, 6. Re-traced and
  confirmed; fixed 2026-07: the healer's hut now mends wounded companions
  (same sawbones formula, `healer.php:106-117`; the village Mendery row
  opens for a wounded companion too), and the bank counter takes typed
  amounts (`Mode::BankAmount`: digits on the talk line, 0/blank = all,
  over-asking gets upstream's arithmetic refusal instead of a clamp; the
  borrow form is upstream's withdraw-with-borrow — balance drawn first,
  remainder borrowed; blank-borrow = full balance+credit, a documented
  adaptation of upstream's odd `abs(goldinbank)` default).
- [x] **Fix: titles ladder** — finding 1. Re-traced (installer seed rows
  0..31 + `lib/titles.php` + `dragon.php:238-242`) and fixed 2026-07: the
  ladder is now 32 rungs, one per kill 0..=31 (existing original names kept
  at their old thresholds so no save demotes; 22 new original names fill
  the gaps, topping out at "The Deathless"), and the title news/log fire on
  every kill unconditionally, as upstream's addnews does.
- [x] **Fix: clans** — findings 8, 12. Re-traced and confirmed; fixed
  2026-07: the public detail roll now shows rank / name / DKs / join date
  (`lib/clan/detail.php`'s columns — the level column dropped, the join
  date rendered `YYYY-MM-DD`), and the withdraw notice list counts a
  just-promoted successor as an officer (upstream promotes before selecting
  the recipients).
- [x] **Fix: docs** — finding 7 + the invalidated bullets, fixed 2026-07:
  CONTEXT.md's stat-derivation summary now carries `vitality_hp` + the race
  adds, its title-ladder line says 0..=31, and a dragon-approach bullet
  joined the faithfulness notes; PARITY's phase-2 titles bullet updated,
  the clan-founding "upstream's order" claim corrected (ours takes the fee
  first — refusal-order-only), and the bell_rand table-approximation
  disclosure added to "Already 1=1". The healer "here and at the healer"
  claim is true again since the healer fix.

Audit + fix steps (not reached by the sweep):

- [x] **Audit: the 8 forest events** — done 2026-07, **zero findings**.
  Established the hook roster first: exactly eight stock modules register
  the `forest` event (findgold, findgem, goldmine, fairy, glowingstream,
  crazyaudrey, foilwench, darkhorse — `module_addeventhook` grep; sethsong
  and cedrikspotions are inn hooks, not forest). All eight weigh `return
  100` — darkhorse's tavern-mount zero can't fire stock (no stock mount
  carries `findtavern`; the mounts table's last column is `mountdkcost`).
  Verified 1=1: the 15% trigger (`e_rand(1,100) <= 15` ≡ our `0..100 <
  15`), events rolling before the turn spend with sober-up first, findgold
  `e_rand(10L, 50L)`, findgem +1, the goldmine 1–20 table (5 nothing / 5
  gold `5L..20L` / 5 gems `1..round(L/7)+1` / 3 both `10L..40L` +
  `1..round(L/3)+1` / 2 cave-in with the strict `< chance` race roll, +10%
  exp on death, escape zeroing turns; the mount save/death arms are inert
  stock — no `mine_*` objprefs seed), fairy (`e_rand(1,7)`: 1 → +1 turn
  [`fftoaward` 1], 2–3 → +2 gems, 4–5 → +1 max HP [`hptoaward` 1, carrydk
  1 — ours rides `dragon_hp_bonus`, which feeds `investment_points` exactly
  like upstream's raw maxhp], 6–7 → specialty; gemless accept −1 turn;
  decline free), glowingstream 1–10 (death+news / near-death clamp
  `round(maxhp·0.1)` floor 1 / heal+turn / gem / turns-only 5–7 / default
  heal), crazyaudrey (1/20 hedgehog +5, triple +2, pair +1, none −1 turn
  or −1 charm at 0), foilwench (no-specialty dead-end, gemless mock,
  gem → increment), darkhorse (opens the room). Bonus re-verification: the
  stock mount seed confirms ff +1/+2/+3, atk ×1.2 for 20/40/60 rounds,
  gems 6/10/16, feeding off (`allowfeed` 0), DK cost 0.
- [x] **Audit: masters + dragon** — done 2026-07 (`train.php`,
  `village.php`, `dragon.php`). **Five findings, all fixed in the same
  step**:
  1. **Master flux was uniform-under-cap** — upstream rolls the whole
     budget then *clamps* (`min(e_rand(0,dk), round(dk*.25))`, a spike at
     the cap). `scaled_master` now rolls-then-clamps; the dragon keeps the
     uncapped uniform split (`partition_flux`, verified against
     `dragon.php`'s `e_rand(0,points)` chain).
  2. **Master fights allowed skills/flee/buffs/companions** — upstream is
     `fightnav(false,false)` with `suspend_buffs/companions('allowintrain')`
     and no stock companion sets the flag (seed: `allowintrain` 0,
     `allowinpvp` 0, `allowinshades` 1 — so torments keep companions, ours
     already did; the one flagged *buff* is the elf/troll racial one,
     `allowintrain=1`, which ours matches via the stat-derivation race add —
     see the PvP audit's finding 17). Now: single Attack row, Esc fights the
     round ("pride"), no persistent-buff injection, companions benched.
  3. **Truancy missing** (`village.php` `automaster` 1): entering the
     square with `experience > exp_for_next_level(level+1)`, yard unseen,
     level < 15 forces the challenge (courtesy full heal, news item, no
     refusing). Now intercepted in `goto(Village)`;
     `Character::truant()` carries the strict `>` test.
  4. **Victory skipped `increment_specialty`** — upstream's win advances
     the craft by one. Added to the victory arm.
  5. **Victory skipped the companion level-up** (`companionslevelup` 1):
     each companion gains its `*perlevel` growth and heals to full.
     `Companion` grew serde-defaulted `attack/defense/hp_per_level` fields
     (bakes from the mercenary tuples; summons 0, exactly upstream's
     keyless skeleton).
  Verified clean: the exp gate operators, seenmaster stamp/clear timing,
  non-lethal defeat (full heal + taunted news), win rewards (+1 level /
  +10 maxhp via derivation / +5 soulpoints / +1 atk/def via derivation /
  full heal), master pick = highest `creaturelevel <= level` (ours: the
  per-level table), dragon base 45/25/300, dragon toughening points
  `at+de + (int)((maxhp−150)/5)` ≡ our `investment_points()` at level 15,
  `round(·0.75)`, and the dragon fight keeping skills + companions (no
  suspend calls in `dragon.php`).
- [x] **Audit: new day** — done 2026-07 (`newday.php` top to bottom, plus
  the full stock `newday`/`newday-runonce` hook roster and `dragon.php`'s
  hand-off into it). **`newday.php` itself verified clean**: constants
  (turns 10, interest `e_rand(101,110)/100` with the `>4`-unused-turns and
  ≥100k gates — debtors always compound, gate 1 needs `goldinbank>=0`),
  processing order (interest reads yesterday's turns before the refill),
  watermark (`recentcomments=lasthit`), `age++` unconditional /
  `resurrections++` while dead, buff strip with `survivenewday`, the
  resurrection flag's exact skips (playerfights/soulpoints/gravefights),
  spirits `e_rand(-1,1)×2` with the −6 paid-res dock, and every hook:
  drinks (harddrinks/hangover>66 −1 turn floor 0/drunkeness), lovers
  (flirt reset + the Violet-marriage charm dock), sethsong / fivesix /
  outhouse / crazyaudrey daily prefs, the race hooks (human +2, elf/troll
  rounds=-1 buffs ≡ our stat adds), the 3 specialty refreshers
  (`(int)(skill/3)` +1 active), mount ff + buff rounds, `fedmount` (inert,
  `allowfeed` 0), and both runonce hooks (crazyaudrey rotation default −1,
  cedrikspotions `random` default 0 — inert stock). **Three findings (13,
  14, 15 below), 13+14 fixed in-step**; Audrey's village zoo (16) filed to
  the missing-feature sweep. (Split out of the masters+dragon step.)
- [x] **Audit: commentary + daily news** — done 2026-07 (`lib/commentary.php`
  + every stock `commentdisplay` call + `drinks/drunkenize.php` + `news.php`
  vs `commentary.rs`/svc/state/ui). **Zero deviations found**; verified 1=1:
  the venue table (village 25/inn 20/darkhorse 10-default/shade 25 from both
  sides/gardens+rock 30/waiting+clan 25, verbs says/whispers/boasts/projects/
  despairs/customsay — `grassyfield` has **no stock venue**, moderation-only,
  correctly absent; lodge/motd/superuser/petition out of scope), the
  allowance (`round(limit/2)` ≡ `div_ceil`, counted among the newest window
  so scrolled-out posts free you, clan-* exempt, waiting not, the <3-left
  counter, the view-gate/talkform-gate pair collapsing to one outcome), post
  prep (trim, silence rejection incl. bare markers, the 45-run break with
  the consumed-breaker restart, non-says verb baking with the emote
  escape, maxlength `200-(verb+11)`), the double-post check against the
  live newest row, the drinks hook at upstream's exact hook point (>50
  "drunkenly" verb, slur at any drunkenness, emotes skipped) and
  `drunkenize` replacement-for-replacement (rate `len·drunk/500`, 9:1
  letter:hic, first-occurrence compounding, case-matched slurs, the five
  hic stagger shifts, adjacent-hic collapse), pagination (comscroll pages,
  First Unseen `round(c/limit+0.5)-1` half-away, refresh/post landing on
  page 0), the day-granular watermark marker (documented ≤1-day-coarser
  deviation; `count_since_day` errs the same direction), and news
  (view-time prune at `expirecontent` 180, one calendar day per page,
  newest first, the empty-day line; the 50/page pager stands in as a
  200-cap, self-documented). The new-day audit's `recentcomments` note was
  fixed here instead: a kill now zeroes `comments_seen_before_day` and a
  still-owed dawn keeps the epoch stamp through `roll_new_day`, upstream's
  everything-is-new post-kill day. One nit filed (drunkenize strpos-0).
- [x] **Audit: PvP + bounty board** — done 2026-07 (`pvp.php`,
  `lib/pvplist.php`, `lib/pvpsupport.php`, `lib/pvpwarning.php`,
  `battle.php`'s pvp branches + `apply_bodyguard`, `lib/inn/
  inn_bartender.php`, `modules/dag.php` + `modules/dag/{dohook,run,
  misc_functions}.php`, the race modules' `pvpadjust`/`racialbenefit`
  hooks). **Zero mechanical deviations**; verified 1=1 operator by
  operator: the two eligibility tests (PvP list/warning `age<=5 &&
  exp<=1500` vs Dag's strict `age<5 && exp<1500` plus the level-3 floor —
  the deliberate off-by-one confirmed on both sides), the list band
  `[-1,+2]` vs the engage's `abs(diff)<=2`, the 600s dogpile window (`>`
  boundary ≡ ours `<`), the offline window + alive + presence semantics,
  engage refusal order and the attack spent at engage, full-health sleeper
  off stored stats, bodyguard(1) 1.05/0.95 inn-only whole-fight, the 50/50
  surprise, run/skill conversion, `pvpvictory` (fresh-purse min, `round(10
  ·lvl·ln(max(1,gold)))`, 10% exp + `round(base·(1+.1·Δlvl))−base` bonus,
  attacker level-15 double-zero, victim −5% engage exp with bank absorb,
  staunch at 1, news + mail-as-report), `pvpdefeat` (sleeper collects off
  the attacker's level + fresh purse, the `$wonamount` typo paying gold
  past a level-15 defender kept 1=1, exp 10% zeroed at 15, the
  leveled-down guard, attacker gold-0/−15%-exp/graveyard, taunted news),
  playerfights 3/day dawn-only refill, and the whole Dag flow (fee
  `round(amt·1.10)`, min `50·lvl` strict `<`, cap `200·lvl` strict `>`
  counting immature rows, 5/day reset on every new day, `e_rand(0,14400)`
  inclusive maturity delay, matured-only list/collection/own-head,
  self-set contracts forfeited-but-open, collection inside the victory
  settlement only and exempt from the level-15 zeroing, dragon-kill/
  deletion closure to the house, lazy stray sweep, 18-day prune, level-
  then-amount default sort). **Two findings, both doc-level (17, 18
  below), fixed in-step**; the port's code is untouched.
- [ ] **Audit: missing-feature sweep** — walk every player-reachable
  `addnav()` in village/forest/inn/graveyard/shades + every module's
  install default; list stock-on features with no port counterpart (this
  sweep caught bank transfers last time). Reverse direction too. Already
  queued by the new-day audit (finding 16): **Crazy Audrey's village
  petting zoo** (`crazyaudrey.php` `village-desc`/`village` hooks,
  `villagepercent` 20): 20% of village visits offer "pet" for 5 gold
  (`cost`) → a 5-round ×1.05 defense buff, then the basket game gated
  once/day by the `played` pref (its newday reset is what upstream's
  `newday` hook is for — the *forest* event is ungated and already 1=1).
- [ ] **Audit: licensing sweep** — every player-visible string in the port
  vs upstream prose/names; distinctive matches only, generic English is
  at most a nit.

### Findings (2026-07 sweep)

"Verified" = independently re-traced by a second adversarial agent.
Re-verify everything anyway before fixing (step 3 above).

Verified:

1. **FIXED** — **Title ladder compressed 32 → 10 tiers** — `data.rs:811` vs installer
   titles seed (`lib/installer/installer_sqlstatements.php:810-841`) +
   `lib/titles.php:25-72` + `dragon.php:178,212`. Upstream promotes at
   *every* dk 0..31; the port only at 0,1,2,3,4,5,7,10,15,20 — no promotion
   at kills 6, 8-9, 11-14, 16-19, 21+. Tier count/thresholds are mechanics
   under our own names-original/numbers-exact rule. Related: the port gates
   the title news/log on `title != old_title` (`state.rs:3506,3520`) while
   upstream fires it on every kill (`dragon.php:238-242`).
2. **FIXED** — **Skeleton Warrior +0.5 atk/def** — `specialty.rs:121-122` adds an outer
   `.round()` upstream doesn't have (`specialtydarkarts.php:177-178` stores
   floats ending in .5; the engine consumes them un-rounded). Fix: store
   companion stats as f64 or halve-adjust; HP formula already matches.
3. **FIXED** — **Regen aura under-heals companions** — `combat.rs:503` truncating
   `total_regen / 3` vs upstream `(int)round(regen/3)`
   (`lib/battle-buffs.php:151`): 1 HP short at levels 2, 5, 8, 11, …
4. **FIXED** — **Dragon approach screen missing** — upstream `forest.php:38-52` offers
   enter-the-cave vs run-away; declining costs 1 charm
   (`lib/inn/inn_default.php:30-34`), and once inside there is **no** flee
   (`dragon.php:254-258,295`, `fightnav(false)`). The port
   (`state.rs:2956`) jumps straight into `Mode::Fight` with a free 1-in-3
   flee row. Fix: add the approach screen (decline ⇒ −1 charm, floor 0)
   and remove the flee row from the dragon fight.
5. **FIXED** — **Companion healing absent at the healer's hut** — upstream heals
   companions at healer *and* merc camp (`healer.php:78-117`); the port
   only at the merc camp. Formula already ported (`model.rs:2164`);
   PARITY.md's "here and at the healer" claim is currently false.
6. **FIXED** — **Bank moves are all-or-max only** — `state.rs:3711-3749` hardcodes
   deposit-all / withdraw-all / borrow-max; upstream `bank.php` takes a
   typed amount for each (0/blank = all). The transfer window already has
   `talk_input`, so the input shape exists. Undocumented asymmetry.
7. **FIXED** — **CONTEXT.md:57 stale stat formulas** — `max_hp`/`attack`/`defense`
   summary omits `vitality_hp` and the race `1+level/5` adds that
   `model.rs:1025-1043` applies.
8. **FIXED** — **Clan detail page columns** — `state.rs:6064-6080` renders Lv and drops
   Join Date; upstream `lib/clan/detail.php:60-91` (and PARITY.md:1098's
   own spec) is rank/name/DKs/joined.

Single-trace (finder confident, no second agent yet):

9. **FIXED** — **Field-medic deals attack damage** — `combat.rs:460-469` lets every
   living companion swing; upstream's heal-ability branch never applies
   `damage_done` to the foe (riposte only, `lib/extended-battle.php:297-311`).
   The port's medic is a heal+DPS unit upstream doesn't have.
10. **FIXED** — **Power-move bonus truncates its bounds** — `combat.rs:321-323` uses
    `as i32` on `patkroll/4` and `/2`; upstream `e_rand` rounds its args
    (`lib/battle-skills.php:113`, `lib/e_rand.php:12,14`). Port bonus is
    biased ~1 low on nearly every power move.
11. **FIXED** — **Creature gold rows off by one** — `data.rs:90` L5 gold 198 (formula
    says 199) and `data.rs:97` L6 gold 234 (formula says 233) vs
    `creature_gold()` in `lib/creatures.php`. Transcription typos; all
    other levels verified exact.
12. **FIXED** — **Clan succession notice lost** — `svc.rs:1944-1961` builds the
    resignation-notice list from pre-promotion ranks; upstream
    (`lib/clan/clan_withdraw.php:17-48`) promotes first then re-reads, so a
    plain member inheriting leadership gets the mail there but not here.

New-day audit (2026-07, solo step):

13. **FIXED** — **Dragon-kill reset too shallow** — `dragon.php:96-175`'s
    DESCRIBE loop reverts every accounts column outside `$nochange` (and no
    stock module extends `dk-preserve`): `goldinbank` (debt included),
    `deathpower`, `hauntedby`, `race`, and `specialty` all reset to column
    defaults; the port kept all five across a kill. A banked fortune, favor,
    and the race/specialty choices survived a DK here and don't upstream.
    Fixed in `slay_dragon`: all five wiped; the race gate re-arms every run
    and the village specialty chooser re-arms via `Specialty::None`.
14. **FIXED** — **No forced new day after a dragon kill** — the field loop
    wipes `lasthit`, so upstream's next page load runs `newday.php` in full
    once its gates pass: fresh turns (10 + dkff + race + spirits), age → 1,
    full heal, soulpoints 55, grave fights 10, PvP fights 3, the daily
    hooks. The port limped on the old day's leftover turns and pools until
    the next calendar dawn. Fixed: `slay_dragon` sets `dawn_owed`, settled
    by `Character::dawn` when the spend/race gates close (`settle_dawn`);
    the load path now defers `roll_new_day` while any gate is pending,
    matching upstream's gates-then-day order (so a just-chosen race's bonus
    fights count into that first day's assembly).
15. **FIXED** — **The passive dawn was silent** — upstream's new-day page
    announces the day, turns, interest, spirits, hangover, and the breakup;
    the port's load-path dawn surfaced only the haunt (the divorce made the
    news but said nothing to the divorcee). Now a dawn line plus
    hangover/divorce lines ride the report drain, and the deferred/owed
    path logs the same set. Interest/spirits aren't itemized per-number —
    a deliberate log-noise call, cosmetic only.
16. **Missing: Crazy Audrey's village petting zoo** — stock feature with no
    port counterpart; filed to the missing-feature sweep step (details in
    its queue entry). The zoo is why upstream's `newday` hook resets the
    `played` pref; the forest basket event is ungated and already 1=1.

PvP + bounty audit (2026-07, solo step; both doc-level, fixed in-step):

17. **FIXED (docs)** — **"Nothing stock sets `allowinpvp`" was false**:
    `raceelf.php:128-129` / `racetroll.php:140-141` give the
    `racialbenefit` buff `allowinpvp=1` and `allowintrain=1`, so upstream's
    attacker keeps the elf/troll bonus in PvP and in master fights. The
    port's *behavior* was already exact — the race add lives in
    `attack()`/`defense()` (both PvP sides, master fights, the intel
    sheet), which is precisely what the flags produce — but PARITY's PvP
    correction 5, the masters-audit bullet, CONTEXT.md's two claims, and
    two `state.rs` comments all overstated "nothing". All corrected; every
    other stock buff (mounts' seed array, drinks' column set, the lover's
    ward) carries no flag and is rightly shelved, and every stock
    companion seeds `allowinpvp=0`/`allowintrain=0`.
18. **Documented deviation** — **the immunity warning shows on both target
    lists**; upstream calls `pvpwarning()` only in `pvp.php` (the fields
    list) — `inn.php` requires the lib but never calls it, so upstream's
    inn list warns nobody. Ours warns at both venues (informational only;
    the forfeit fires identically at engage). Kept deliberately and noted
    on the warning bullet.

Nits (fix only if already in the file; none block parity):

- ~~Slumming nav shown at level 1 (`state.rs:6553` vs `lib/forest.php:15`);
  mechanically identical at level 1, nav-visibility only.~~ Fixed with the
  dragon-approach step (the row hides below level 2).
- HoF wealth percentile reuses the display fuzz instead of an independent
  re-fuzz (`state.rs:6471` vs `hof.php:162`); same distribution, cosmetic.
- Earth Fist drops upstream's `areadamage` flag — inert in our
  single-target combat model.
- ~~Clan founding checks the fee before name/tag uniqueness
  (`state.rs:5505-5519` vs `lib/clan/applicant_new.php:38-56`); refusal
  message order only, but PARITY.md:1117 "upstream's order" overstates.~~
  Doc corrected with the docs step (behavior unchanged, order-only).
- Healer-companion targeting picks most-wounded vs upstream's
  first-other-then-self order (`state.rs:3108-3125`); reachable only with
  medic + summoned skeleton.
- Upstream's stat rail shows the day's spirits label (`pageparts.php:667`,
  Very Low..Very High, "DEAD" while dead); the port doesn't store or render
  the spirits roll after the dawn message. Cosmetic.
- `drunkenize`'s adjacent-hic collapse uses `while (strpos(...))`, whose PHP
  falsy-0 skips a `*hic**hic*` at position 0; the port's `contains` loop
  collapses it there too. Upstream-bug direction, kept.
- A fresh character's first day is seeded flat (10 turns, no spirits/race
  roll — `Character::new` documents it) where upstream rolls a real first
  `newday.php` at first login. Day-1-only, and the race gate precedes the
  first roll anyway now; left as-is.
- ~~`bell_rand` is a continuous inverse-normal approximation of upstream's
  441-row percentile table; z deltas ≲0.002 never move truncated integer
  damage. Consider a one-line disclosure in "Already 1=1".~~ Disclosure
  added with the docs step.

Clean under the sweep (audited, zero findings): Dark Horse (all three games
+ enemy intel), graveyard/favor/resurrection/haunt, outhouse/gypsy/gardens,
stables, inn (room/bard/drinks/flirt/potions/drunkenness), races, roster +
HoF, forest spawn/jitter/payouts, bank formulas + transfers, clans core
(ranks/gates/succession/validation), specialties use-economy + the other
11 skills, healer pricing, combat rolldamage core + tables.

## Out of scope (not stock / not portable)

- Donator lodge, referrals, translation/admin tooling, logdnet, holiday
  modules, `cities`/travel (add-on, not stock core), petitions/moderation UI.
- **The "King's tournament" / jousting**: long carried here as "the phase-4
  tail", but verified 2026-07 to be **absent from stock 1.1.2** — no
  tournament module ships; the only trace is `source.php:85` hiding
  `modules/tournament.php` from the view-source listing. A DragonPrime
  add-on, off-limits like the rest.
- Upstream prose, creature/master/NPC/drink/title *names* — always original.
