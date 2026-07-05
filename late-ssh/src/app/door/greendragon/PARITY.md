# Green Dragon 1=1 parity checklist

Goal: full parity with **stock LoGD 1.1.2 (DragonPrime Edition)** — every
mechanic, formula, odds table, and cost transcribed exactly; **all prose and
names original to late.sh** (upstream text is CC BY-NC-SA and off-limits).

## Target / provenance

- **Source of truth: `jimlunsford/lotgd`** (github mirror of DragonPrime
  1.1.2, the final content-complete classic release; project ceased Sept 2019).
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
- Specialties (3 × 4 skills), use economy `floor(skill/3)+1`, gem advancement.
- Buff + companion engines; forest death (gold→0, exp×0.9); master fights
  (non-lethal loss, +5 soulpoints on win); shop ladder + 75% trade-in +
  level gating; healer full-heal cost `round(ln(level)·(missing+10))`;
  8 stock forest events at 15% (`forestchance`); exp curve + DK scaling;
  new-day spirits `e_rand(-1,1)+e_rand(-1,1)`, resurrection −6 turns;
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
  the field).
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

### Known deliberate deviations (single-player shape, documented)

- Creature table caps at level 16 (upstream has 17–18 easter-egg rows);
  multi-fight overflow clamps at 16 instead of 17.
- Doppleganger fallback (empty creature query) is unreachable with a static
  table — omitted.
- Companion incoming-damage model: foe rolls against a random companion each
  round rather than LoGD's single-target redistribution (pre-existing,
  see CONTEXT.md).
- `suicide` searching: stock default **off** — correctly absent.

## Phase 1 — the dead realm (`graveyard.php`, `shades.php`, `lib/graveyard/`)

- Soulpoints are the dead player's HP pool; max = `level·5 + 50`; dead
  atk/def = `10 + round((level−1)·1.5)`.
- Graveyard fights: `gravefightsperday` 10; graveyard creature pool at player
  level with `atk = 9+shift+int((level−1)·1.5)`, `def = int(9+shift+(level−1)·1.5)·0.7`
  (shift −1 under level 5), `hp = level·5+50`; victory pays **favor**
  (`deathpower`) `e_rand(10+round(level/3), 20+round(level/3))`; defeat ends
  the day's torments; flee 1/3, escape costs `min(favor, 5+e_rand(0,level))`.
- Mausoleum: restore soul for `round(10·(max−soul)/max)` favor; at ≥25 favor
  unlock haunt (PvP, phase 4); at ≥100 favor **resurrection**: −100 favor,
  immediate new day at −6 turns (spirits "Resurrected").
- New-day resets while dead only partially (no soulpoint/gravefight refresh on
  resurrection days).

## Phase 2 — races + titles

- Race chosen at new day (upstream `setrace` gate). Human +`bonus`(2) daily
  turns; Elf +`1+floor(level/5)` defense; Troll same on attack; Dwarf ×1.2
  forest gold; goldmine death chance 90% (5% dwarf). All names/villages ours.
- DK titles (`titles` table, `lib/titles.php`): gendered pairs keyed by
  dragon-kill threshold, highest `dk ≤ kills` wins, random among ties;
  re-titled on each kill. Title strings original.

## Phase 3 — single-player buildings

- **Stables** (`stables.php`): 3 mounts, gems 6/10/16, +1/+2/+3 daily forest
  fights, offense buff atkmod 1.2 for 20/40/60 rounds, ⅔ trade-in refund;
  feeding gated off by default (`allowfeed` 0).
- **Mercenary camp** (`mercenarycamp.php`, `companions` seed): 2 hires —
  fighter (573 gold: atk 5+2/lvl, def 1+2/lvl, hp 20+20/lvl, fight) and
  healer (1000 gold: atk 1+1/lvl, def 5+5/lvl, hp 15+10/lvl, heal 2);
  stats scale with level at purchase; cap `companionsallowed` 1 (summons
  bypass via ignorelimit); heal cost `round(ln(level+1)·(missing+10)·1.33)`.
- **Inn** (`inn.php` + modules):
  - Room: `round(level·(10+ln(level)))` gold, once/day, logs you out
    (PvP-attackable there in phase 4).
  - Barkeep bribes: gems ⇒ 30%/gem success; gold tiers `level·10/50/100` ⇒
    25% / ≈47.2% / 75% (`(amt/level−10)·(50/90)+25`); paid regardless;
    unlocks intel + **specialty switch** (keeps skill points).
  - Cedrik's potions (gems, default 2/dose): +1 charm; +1 permanent max HP;
    overheal +20 HP; specialty reset; race reset + sickness debuff
    (atk/def ×0.75, 10 turns, survives newday).
  - Bard's song: once/day, `e_rand(0,18)` outcome table (turns ±, gold
    10–50, gems 1, heals ±10/20%, charm ±1).
  - Drinks + drunkenness: 3 stock drinks (cost/level 10/15/25, drunkenness
    +33/+50/+50, buffs: atk 1.25×10r / atk 1.1 def 0.9 dmg 1.5×12r /
    dmg 1.3 shield 1.3×15r); hard-drink limit 3/day; refuse above 66 drunk;
    hangover −1 turn above 66 at newday; drink names ours.
  - Violet/Seth flirting: threshold ladder T = 2/4/7/11/14/18 (roll
    `e_rand(charm,T) ≥ T`), marriage at charm ≥22, failed proposal zeroes
    the day's turns; marriage upkeep `charm −= e_rand(1, max(1,round(0.85·√kills)))`
    daily, divorce at 0; married visit: 1/4 rebuff (−1 charm) else buff
    (defmod 1.2 × 60 rounds) +1 charm.
- **Outhouse** (forest, once/day): private 5 gold ⇒ wash: 60% refund 3 gold,
  25% +1 gem; public free ⇒ 60%·⅓ find 3 gold; no-wash 50% lose 1 gold +
  embarrassing news.
- **Dark Horse Tavern** (mount-gated in stock; we surface it as the restored
  `events::Tavern`): dice (keep/reroll ≤3 vs old man AI: keeps on
  beat-or-6, then beat-or-equal, then final), Five Sixes (5 gold, 5d6,
  3/4/5 sixes pay 5%/10%/100% of shared jackpot seeded 100 cap 5000,
  10 plays/day), Stones (6 red + 10 blue drawn in pairs, like/unlike bet,
  first past 8 wins).
- **Daily news** (`addnews`): schema (text, day, author), day-paged view,
  180-day expiry; writers: deaths, dragon kills, master defeats, marriages,
  jackpots, resurrections.
- **Healer/creature leftovers**: per-creature win/lose taunt hooks; the few
  stock creature AI behaviors (e.g. a bandit pickpocketing 20% of carried
  gold once, 1/8 if gold>200) — ours with original names.

## Phase 4 — multiplayer

- **`commentary` primitive first**: table (section, author, ≤200-char body,
  timestamp); 5 posts/section/day (limit/2 of display 10; inn 10, gypsy/clan
  12); `:`/`::`/`/me` emotes; double-post + empty-post rejection; newest-first
  pagination. Sections: village square, inn, gardens, darkhorse, shade,
  veterans, clan-{id}, waiting.
- **Roster + Hall of Fame**: online = active ≤15 min; HoF rankings
  (dragonkills, gold fuzzed ±5%, gems, charm, maxhp, resurrections,
  best dragon-kill speed), "your rank" percentile.
- **Gypsy**: `level·20` gold to read/post the shade section.
- **PvP** (`pvp.php`): targets level −1..+2, offline >15 min, alive,
  not immune (age ≤5 days, 0 kills, 0 pk, exp <1500); 3 fights/day;
  win: `round(10·lvl·ln(max(1,gold)))` gold + 10% victim exp (±10%/level
  diff), victim −5% exp and dies; attacker death: −15% exp, gold 0.
  Level-15 winners get nothing ("prowess"). Inn sleepers attackable.
- **Dag bounties**: place 50–200 gold/level (min/max), +10% fee, ≤5/day,
  target level ≥3 and not immune, active after `e_rand(0,4h)` delay; paid on
  PvP win (own bounties forfeit); cleared on target dragon kill.
- **Haunt** (graveyard, ≥25 favor): −25 favor, roll `e_rand(0,yourLvl) >
  e_rand(0,targetLvl)` ⇒ target loses 1 turn next newday.
- **Mail**: 50-message inbox (unread), 1024-char bodies, 14-day retention.
- **Clans**: found 10000 gold + 15 gems, name 5–50 / tag 2–5 chars, ranks
  0/10/20/30/31, hall commentary + shared waiting area, officer app mail.
- **Gardens**: commentary room, 0% event chance (stock default).
- Wire the dragon-kill chip/activity reward (`svc` holds the deps).

## Out of scope (not stock / not portable)

- Donator lodge, referrals, translation/admin tooling, logdnet, holiday
  modules, `cities`/travel (add-on, not stock core), petitions/moderation UI.
- Upstream prose, creature/master/NPC/drink/title *names* — always original.
