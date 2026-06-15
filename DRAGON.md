# Daily Dragon RPG Notes

## Purpose

Working notes for a possible late.sh door game inspired by Legend of the Red Dragon.
This is not an implementation plan for an official LoRD port.

## Research Snapshot

- Legend of the Red Dragon was created by Seth Robinson / Robinson Technologies in 1989.
- Public summaries say Robinson sold the rights to LoRD, LoRD II, and related BBS games to Metropolis Gameport in 1998.
- Michael Preslar later maintained the classic game line.
- Classic LoRD references mention official version 4.07 and a later 4.08 patch around 2009.
- Legend of the Red Dragon II: New World exists, but it is a different game shape: ANSI map movement and more real-time/top-down than the original menu-driven daily RPG.
- The old `lordlegacy.com` trail appears unreliable now; when checked on 2026-06-15, the domain redirected to a domain sale page.

## Legal/Product Boundary

Do not ship a game named Legend of the Red Dragon, reuse LoRD prose, named characters, setting, scene text, menus, or distinctive story content without a license.

The mechanics and ritual are the useful reference:

- daily action allowance;
- forest fights;
- risk/reward gold carrying versus banking;
- inn/town social hub;
- shops, healer, trainer;
- asynchronous player rivalry;
- rankings and public events;
- final dragon challenge;
- seasonal/reset-friendly progression.

The remembered value is heavily in the writing. A late.sh version should have original text, names, NPCs, jokes, events, and lore.

## Classic Game Shape

Classic LoRD is closer to a daily BBS ritual than a live MUD:

1. Player enters town and checks status, rankings, mail/news, bank, shops, inn, healer, and trainer.
2. Player spends a limited number of daily forest fights.
3. Forest encounters grant XP, gold, gems, and occasional random events.
4. Player banks gold, heals, upgrades gear, and decides whether to push risk.
5. When XP is high enough, player challenges a trainer/master to level up.
6. Player has limited PvP attempts per day, including attacks against offline players.
7. Social systems and message surfaces create rivalry and town gossip.
8. Daily reset refreshes fights/PvP attempts and advances the shared game day.
9. Long-term goal is to become strong enough to challenge the dragon.

Known/likely classic systems:

- limited daily forest fights;
- limited daily PvP;
- XP and level progression;
- trainer/master fights;
- weapon and armor purchases;
- gold and gems;
- bank deposits;
- healer;
- inn and town hub;
- random forest events;
- player rankings;
- message boards/mail;
- flirt/marriage/social systems;
- skill paths such as Death Knight, Mystical, and Thieving;
- third-party IGMs.

Unknown without original docs/source/runtime:

- exact XP curves;
- exact combat formulas;
- exact enemy tables;
- exact item prices/stats;
- exact random-event probabilities;
- all original event branches;
- version-specific 4.00a/4.07/4.08 differences;
- complete LoRD II mechanics.

## Fit For late.sh

The repo already has useful infrastructure:

- `late-ssh/src/app/door/lateania` is a persistent terminal RPG with service-owned state, snapshots, UI/input split, combat, items, shops, persistence, and activity events.
- `late-ssh/src/app/door/game.rs` defines the generic door-game contract.
- Lateania is real-time/shared-world; a daily LoRD-like game should probably be a sibling door game, not another mode inside Lateania.
- The root `CONTEXT.md` and `late-ssh/src/app/door/lateania/CONTEXT.md` should be read before touching this area.

Recommended implementation direction:

- Build an original sibling under `late-ssh/src/app/door/`, with a fresh name and content.
- Use a menu/day-turn model rather than Lateania's live-world tick model.
- Reuse the architectural patterns from Lateania where useful: service owns truth, per-session state owns view cursor/cache, UI is snapshot-only, persistence is explicit.
- Integrate with late.sh identity, activity feed, Hub/leaderboard surfaces, and possibly Late Chips only after the core loop is fun.

## MVP Candidate

First playable slice:

- character creation;
- daily turn counter;
- town menu;
- forest fight action;
- simple monster table;
- HP, XP, level, gold, bank;
- healer;
- weapon/armor shop;
- trainer fight;
- leaderboard;
- daily reset command/service timer;
- original random event text table.

Avoid for the first slice:

- LoRD-compatible data import;
- exact LoRD formulas;
- LoRD names/text;
- LoRD II map movement;
- IGM/plugin system;
- romance/social systems unless intentionally redesigned for late.sh.

## Useful Sources Checked

- https://en.wikipedia.org/wiki/Legend_of_the_Red_Dragon
- https://en.wikipedia.org/wiki/Robinson_Technologies

These sources are enough for broad product direction, not enough for exact mechanics.
