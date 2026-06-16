# Dragon

Native late.sh daily social RPG plan.

Dragon is not a Legend of the Red Dragon port, not a BBS door, and not a
Docker/dosemu project. The original game is reference material for pacing,
system shape, and social design only.

## Current Decision

Build a native late.sh game.

Stop pursuing the original BBS runtime path for now. It creates the wrong work:
BBS registration, DOS/runtime fragility, dropfile/node setup, public-hosting
licensing uncertainty, and a user experience that does not fit late.sh.

The new direction:

- use late.sh identity directly;
- keep the daily ritual and social consequence loop;
- create original setting, names, prose, events, NPCs, monsters, equipment, and
  jokes;
- use classic LORD data shapes as reference for tables and pacing;
- build Dragon from our own committed `.dat` tables;
- keep reference inventories in `assets/dragon/reference/`;
- keep raw original archives/extractions out of git.

## Reference Boundary

Useful reference:

- daily action limits;
- forest fights;
- monster buckets by level;
- combat rewards;
- trainer/master progression;
- weapon and armor tiers;
- bank/healer/shop/town loop;
- PvP attempts;
- gossip, mail, flirting, public logs, rankings, and daily happenings;
- rare forest events and hidden locations;
- skill paths and daily special uses;
- post-dragon reset/legacy loop.

Do not ship:

- original LORD prose;
- original distinctive NPC names;
- original monster/item/event text as content;
- original archives, extracted data files, screenshots, or activation data;
- a game named or branded as Legend of the Red Dragon.

Reference docs live in:

```text
assets/dragon/reference/
```

Dragon's own working data tables live in:

```text
assets/dragon/dats/
```

Local raw extraction experiments, if needed, go in:

```text
assets/dragon/raw/
```

That directory is ignored by git.

## Do We Have All Events?

No.

We have access to many useful surfaces from the local official 4.07 package:

- docs and Pascal structure notes;
- monster, weapon, armor, player, and add-on data shapes;
- many text/data files;
- Lady script/data files;
- visible strings embedded in the executable;
- observed runtime screens.

But the complete game is not a clean data archive. Some behavior is compiled
inside `LORD.EXE`:

- exact event probabilities;
- exact trigger conditions;
- hidden branches;
- combat formulas;
- state gates;
- registration/version gates;
- interactions between events, player records, and daily reset logic.

So the plan is not "import every event." The plan is:

1. inventory the important categories;
2. understand why they work;
3. build a native event system with explicit data;
4. write original events that hit the same social and gameplay beats.

## Product Thesis

The core product is not the forest combat. The core product is a daily public
consequence loop.

Players should return to see:

- who died;
- who won;
- who got rich;
- who got robbed;
- who got embarrassed;
- who proposed;
- who lied;
- who insulted whom;
- who is now scary enough to avoid;
- what weird town event happened overnight.

The Daily Happenings screen is a first-class feature, not flavor.

## Core Loop

1. Enter town.
2. Read Daily Happenings.
3. Check status, rankings, mail, bank, shops, healer, trainer, and social
   surfaces.
4. Spend limited forest fights.
5. Fight monsters or hit random forest events.
6. Gain gold, gems, experience, items, flags, injuries, and public outcomes.
7. Decide whether to bank, heal, upgrade, train, flirt, attack, rest, or keep
   pushing.
8. Spend limited PvP/social actions.
9. Leave public traces in news/logs.
10. Return tomorrow after daily reset.
11. Eventually challenge the dragon.

## Must-Have Systems

### Character

- late.sh user identity maps to one Dragon identity;
- display name;
- level;
- experience;
- hit points;
- strength;
- defense;
- charm or social stat;
- gold carried;
- banked gold;
- gems or rare currency;
- forest fights left today;
- player fights left today;
- skill uses left today;
- total dragon wins or legacy score.

### Town

Town is the menu hub and social surface.

Required locations:

- Forest;
- Weapon shop;
- Armor shop;
- Healer;
- Bank;
- Inn;
- Trainer/master;
- Daily Happenings;
- Rankings;
- Mail/notes;
- PvP target list;
- Profile/stats;
- Other Places or hidden-location entry.

### Forest

Required behavior:

- consumes daily forest fight count;
- picks monster/event from level-aware tables;
- offers risk/reward choices;
- can generate private result text and public news;
- can produce gold, gems, XP, injuries, flags, relationships, rumors, or rare
  hooks;
- can occasionally present non-combat events.

Monster data should be original but shaped like:

```text
level_bucket
name
attack/strength
hit_points
gold_reward
experience_reward
weapon_or_attack_label
death/news templates
rarity or weight
tags
```

### Equipment

Keep numeric tier separate from display name.

This is essential for social play: a strong item can have a deceptive or funny
public name.

```text
weapon_tier
weapon_display_name
armor_tier
armor_display_name
```

Required behavior:

- weapon tiers gate attack growth;
- armor tiers gate defense growth;
- purchases cost gold and/or require stats;
- later rename/customization can become a social feature.

### Progression

- XP gates level advancement;
- advancement requires trainer/master challenge or equivalent test;
- higher levels unlock harder monster buckets;
- daily counts reset each game day;
- dragon victory partially resets the player while preserving some legacy.

### PvP

PvP must be limited, risky, and public.

Required behavior:

- limited player fights per day;
- target list excludes invalid/protected targets;
- offline attacks are possible;
- outcomes can steal gold, create injuries, or affect reputation;
- losses are newsworthy;
- players can write or select boasts/last words.

### Daily Happenings

Every meaningful action can emit a news event.

Required categories:

- town events;
- monster deaths;
- player deaths;
- PvP wins/losses;
- robbery attempts;
- romance/proposals;
- insults and rumors;
- training milestones;
- dragon encounters;
- rare weird events;
- admin/system announcements.

Each emitted event should know:

```text
visibility
participants
news template
private result template
severity
tags
game_day
```

### Social

This is a must, not a stretch goal.

Required surfaces:

- mail or notes;
- flirt/social actions;
- proposals/relationship status;
- public insults/rumors;
- inn/rest state;
- gossip generated from recent actions;
- player-authored short text with moderation boundaries.

### Events

Events should be data-driven and explicit.

Each event should define:

- id;
- location;
- trigger conditions;
- weight/rarity;
- choices;
- stat checks;
- costs;
- rewards;
- failures;
- flags set/cleared;
- public news templates;
- private result text;
- cooldowns;
- content safety/moderation category.

## Content Pillars

Dragon should feel:

- social first;
- funny but not random noise;
- dangerous enough that choices matter;
- readable in a terminal;
- quick enough for a daily ritual;
- persistent enough that yesterday matters;
- weird enough that players quote the news.

## Build Phases

### Phase 0: Reference And Content Bible

- Keep curated reference notes in `assets/dragon/reference/`.
- Keep Dragon's own `.dat` tables in `assets/dragon/dats/`.
- Inventory classic table shapes and event categories.
- Define original setting, tone, NPC cast, monster families, item tiers, and
  news voice.
- Draft original monster buckets and equipment tiers.
- Draft Daily Happenings templates.
- Define event schema at product level.

### Phase 1: Daily Core

- character creation;
- town square;
- status view;
- forest fight;
- monster rewards;
- death/recovery;
- bank;
- healer;
- weapon/armor shops;
- daily reset;
- Daily Happenings.

### Phase 2: Progression And Rivalry

- trainer/master fights;
- level advancement;
- rankings;
- PvP;
- attack result news;
- player-authored boasts/last words;
- carried-gold risk and theft.

### Phase 3: Social Gravity

- mail/notes;
- flirting/proposals/relationships;
- inn/rest state;
- gossip and rumors;
- NPC insults/praise;
- public embarrassment events;
- relationship/profile surfaces.

### Phase 4: Depth

- skill paths;
- hidden places;
- rare event chains;
- dragon challenge;
- post-dragon legacy reset;
- admin tuning tools;
- event authoring workflow.

## Implementation Notes For Later

Do not implement yet from this plan alone.

When implementation starts, first inspect:

- `CONTEXT.md`;
- `late-ssh/src/app/door/game.rs`;
- `late-ssh/src/app/door/lateania/`;
- `late-ssh/src/app/door/rebels/`.

Likely shape:

- a sibling native door under `late-ssh/src/app/door/`;
- service-owned persistent state;
- snapshot-only UI;
- explicit input/action reducer;
- deterministic game-day reset path;
- original data tables checked into the repo once written.

## Sources Checked

- https://en.wikipedia.org/wiki/Legend_of_the_Red_Dragon
- local official 4.07 package docs and structure notes in the ignored local
  scratch directory.
