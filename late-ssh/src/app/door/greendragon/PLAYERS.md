# Green Dragon: player quick reference

A support-oriented digest, distilled from player questions. Precise mechanics
live in `MECHANICS.md` and `PARITY.md`; this is the fast-lookup version.

**What each place actually does** (beyond chat):
- **Village Square**: mostly gossip (`Commentary::Village`), plus Mad Juna's
  petting zoo (20% of visits, 5 gold for a defense ×1.05 buff, 5 rounds,
  once/day).
- **Gardens**, **Veterans' Rock**: pure `Commentary` sections, nothing else
  hooks in.
- **Weapon/Armor shops**: full 15-row ladder for everyone, no level gate;
  affordability (net of 75% trade-in) is the only check.
- **The Mendery (Healer)**: full or percent heals, companion mending; a
  forest amenity, not in the village.
- **The Coinvault (Bank)**: loans to `-level*20`, debt always compounds,
  positive balances get a random 1-10% daily interest (only if ≤4 turns were
  left unused and under 100k gold); cross-player gold transfers.
- **The Proving Yard (Training)**: the master fight — see Death below. The
  main lever for character power.
- **Stables**: 3 mounts (gems 6/10/16), +1/+2/+3 daily forest fights
  permanently, plus a combat buff; ⅔ gem refund on sale. Mounts never die.
- **Mercenary Camp**: hires companions (gold+gems) — see Allies below.
- **The Sleeping Stag (Inn)**: the densest building — lodging (turn cost,
  flags you on the Inn's PvP list, grants a small defensive Bodyguard buff to
  whoever's attacked there), barkeep bribes (unlock the who's-upstairs PvP
  list + a free specialty switch), the potions shelf (2 gems/dose), drinks
  (drunkenness/HP/turn gambles), the bard's nightly song (once/day, free —
  see Gems below), and the romance ladder to marriage.
- **Forest Outhouse**: 5 gold private stall or free public; washing sobers
  you and has a 60% chance at 3 gold back.
- **The Dark Horse Tavern**: dice duel, stones, the shared cross-player Five
  Sixes pot, the barman's paid enemy intel (100 gold, exact stats on anyone —
  the key PvP scouting tool), plus its own comment board.
- **Graveyard**: the hub while dead — see Death below.
- **Gypsy**, **Dag (bounty broker)**, **Haunt**: paid seance into the shade
  chat channel; placing bounties on other players; a 25-favor curse from the
  graveyard.
- **Clans**, **Warrior List / Hall of Fame**: administrative/informational;
  no combat or economy payout of their own.

**Gems — ranked by real efficiency**:
1. **Forest victory passive roll**: 1-in-25 on every kill while level < 15,
   free, no risk (`model.rs` `forest_victory`). The best sustained source —
   just grind normally.
2. **The bard's song** (inn, once/day, free): ~1-in-19 chance of a bonus gem
   alongside its other random outcomes (`inn.rs::bard_song`).
3. **Goldmine forest event**: bigger payout when it hits, but ~10% of its
   own rolls either kill you outright or zero your whole day's turns.
4. Find a Gem / Glowing stream (forest events): small, free, low frequency.
5. Fairy: a net gem **sink** (costs 1 to play), not a source.
6. **Dragon kill**: `max(0, kills-7)` gems, capped 10, +1 if flawless — the
   single biggest lump sum, but gated behind 8+ dragon kills.

**Death — cost depends entirely on cause**:
- **Forest / losing to the dragon** (`Character::die`): all on-hand gold
  gone, experience ×0.9 (lose 10%), bank/gear/companions/level untouched,
  buffs and drunkenness clear. Sent to the graveyard, revived free at the
  next dawn (a game-day is 12 real hours) or instantly for 100 favor.
- **PvP, as the sleeping victim** (`pvp_slain`): a log-scaled cut of your
  gold, lose only 5% experience — milder than a plain death.
- **PvP, as the attacker who lost** (`pvp_die`): **all** gold, lose 15%
  experience — the harshest death in the game. Never attack unscouted.
- **Master/Training fights**: cannot kill you. A loss just heals you to
  full and ends the day's attempt.
- **Goldmine cave-in**: the mildest "death" — no gold or exp loss, actually
  credits +10% exp, just ends the day.
- While dead, the graveyard is its own hub (torment fights for favor, the
  mausoleum, paid resurrection) — downtime isn't fully wasted, just
  redirected away from the main progression loop.

**Allies (companions) — verified against upstream `extended-battle.php`**:
- Character death **never** clears companions (`die`/`pvp_die`/`pvp_slain`
  don't touch the list) — only a **dragon kill** wipes them
  (`model.rs::slay_dragon`).
- But a companion **can die mid-fight**, per round, independent of the
  fight's outcome (`combat.rs`: `companions.retain(|c| c.hitpoints > 0)`,
  matching upstream's `report_companion_move` returning `false` at
  `hitpoints <= 0`). A losing fight very plausibly kills your companions
  round-by-round before or as you go down too — that's the likely
  explanation any time a player reports "died and lost my ally," not a
  death-triggers-wipe bug.
- **No insurance, no refund, no revival**: a dead companion is gone from
  the list permanently; getting one back means paying full gold+gems again
  via `hire_mercenary` (verified: no refund/revive path exists upstream
  either). Contrast with mounts, which refund ⅔ gems on voluntary sale and
  can never die in combat at all — mercenaries are the strictly riskier
  gem purchase, by design.

**PvP**:
- Fully asynchronous: you fight another player's **saved** stats at full
  HP while they're offline. Buffs, companions, mounts, and specialty skills
  are benched on both sides — it's a pure gear/base-stat check.
- Target lists (`state.rs::build_pvp_rows`) only show characters who
  are **offline**, within your `level-1` to `level+2` band, not immune, and
  not "hunted" in the last 10 minutes.
- **Staying online is the real shield**: the `!e.online` filter means an
  active session cannot be targeted at all. Newbie immunity (age ≤5
  game-days, 0 dragon kills, never attacked anyone, exp ≤1500 — all four
  must hold) is fragile and forfeited forever the instant you attack
  anyone yourself.
- Best attacking tactic: scout with the Dark Horse's 100-gold enemy intel
  before every swing (exact atk/def/HP/gold), prefer higher-level/richer
  targets within your band (both gold and exp payouts scale up with the
  victim's level), and prefer Fields targets over Inn-lodged ones (lodged
  targets get a small Bodyguard defensive edge). Only 3 attacks/day, so
  spend them on confirmed kills, not blind swings.
