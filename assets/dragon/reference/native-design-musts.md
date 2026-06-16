# Native Dragon Musts

Dragon is a native late.sh daily social RPG, not a LORD port and not a BBS door.

## Core Loop

1. Enter town.
2. Read daily happenings and check status.
3. Spend limited daily forest fights.
4. Gain gold, gems, experience, and occasional event outcomes.
5. Decide whether to bank, heal, upgrade, train, flirt, attack, or keep pushing.
6. Challenge a trainer/master when ready.
7. Use limited PvP/social actions.
8. Leave traces in public logs.
9. Return tomorrow to see consequences.
10. Eventually challenge the dragon.

## First-Class Social Loop

The game must make players care what happened while they were gone.

Required surfaces:

- Daily Happenings newspaper.
- Public death and victory logs.
- Player rankings.
- Player mail or notes.
- PvP attack results.
- Romance/flirt/proposal outcomes.
- Gossip, insults, rumors, and public embarrassment.
- Player-authored taunts, epitaphs, or boasts.
- Visible online/currently-in-realm list.

## Game Systems

Must have:

- character identity tied to late.sh user;
- level, experience, hit points, strength, defense, charm, gold, gems;
- forest fights per day;
- player fights per day;
- equipment tiers and display names;
- bank and carried-gold risk;
- healer;
- weapon shop;
- armor shop;
- trainer/master progression;
- forest encounters;
- random events;
- daily reset;
- persistent multi-player state;
- dragon challenge and post-win reset/legacy handling.

Should have after the first playable loop:

- skill paths with daily uses;
- inn room/rest state;
- marriage and relationship states;
- children/household style long-tail effects;
- hidden locations;
- rare event chains;
- event-specific flags;
- moderation tools for text users can write.

## Event Design Requirements

Events should be explicit data, not hidden code.

Each event should define:

- id;
- location;
- trigger conditions;
- weight or rarity;
- choices;
- stat checks;
- costs;
- rewards;
- failures;
- public news templates;
- private text templates;
- cooldowns or once-per-day flags;
- safety/moderation category if it can produce user-facing text.

## Content Boundary

Use classic LORD for structure and pacing reference only.

Do not ship:

- original LORD prose;
- original distinctive NPC names;
- original monster/item names as a set;
- original event text;
- original executable/data files;
- screenshots from the original game;
- registration or activation data.

Write new late.sh lore, NPCs, monster names, item names, events, and jokes.

