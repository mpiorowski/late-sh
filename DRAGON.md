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

Important distinction for the current plan:

- Rewriting/porting LoRD into late.sh would need explicit adaptation/content rights.
- Running the original registered BBS door unmodified is a different, narrower path. The public Gameport order form says the $15 BBS purchase emails an activation code and asks for a BBS name, so it appears to be a normal sysop registration for running the door game.
- Before public launch, ask Gameport/Metropolis in writing whether a registered BBS copy may be hosted as a public online BBS/door and accessed through late.sh's SSH proxy.

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
- `late-ssh/src/app/door/rebels` already proves the embedded remote-terminal pattern: late.sh opens an outbound SSH connection, requests a PTY sized to the content area, feeds remote output into `vt100`, blits the parsed terminal grid into ratatui, and forwards user input raw while the remote game is running.
- Lateania is real-time/shared-world; a daily LoRD-like game should probably be a sibling door game, not another mode inside Lateania.
- The root `CONTEXT.md` and `late-ssh/src/app/door/lateania/CONTEXT.md` should be read before touching this area.

Original spiritual-successor direction, if licensing/hosting does not work:

- Build an original sibling under `late-ssh/src/app/door/`, with a fresh name and content.
- Use a menu/day-turn model rather than Lateania's live-world tick model.
- Reuse the architectural patterns from Lateania where useful: service owns truth, per-session state owns view cursor/cache, UI is snapshot-only, persistence is explicit.
- Integrate with late.sh identity, activity feed, Hub/leaderboard surfaces, and possibly Late Chips only after the core loop is fun.

## Current Plan: Registered BBS Door First

The preferred near-term plan is no longer to rewrite LoRD first. Build and test a real registered BBS/door stack, then optionally embed it in late.sh.

### V1: Working BBS + LORD, Not Connected To late.sh App

Goal: prove that the original game can run reliably before touching app UI.

Target shape:

```text
Kubernetes namespace
  lord-bbs pod
    BBS software
    dosemu2 / DOS door runner
    registered LORD BBS door
    PVC for BBS users, LORD data, scores, and config
  lord-bbs-sv ClusterIP
    internal service for testing/admin access
```

V1 requirements:

- Buy/register the **BBS** version of LORD, not the PC version.
- Pick a BBS package that can run DOS doors under Linux/container.
- Prefer Synchronet first because it is open-source and cleaner to package in repo/infra. Mystic is also capable and free to download, but check redistribution terms before baking it into images.
- Run LORD through dosemu2 or the BBS package's supported DOS-door runner path.
- Store all BBS and LORD mutable data on a PVC.
- Keep the service internal by default; expose only enough for controlled testing.
- Verify a fresh player can create/login, launch LORD, spend turns, exit, reconnect, and retain state.
- Verify multiple users/sessions do not corrupt game data.
- Verify ANSI/CP437 output looks correct in a normal terminal client.

V1 explicitly does not include:

- late.sh screen integration;
- late.sh identity mapping;
- app navigation/top bar embedding;
- rewriting LoRD data or text;
- importing LoRD content into this repo.

### V2: Embed The Running BBS Door In late.sh

Goal: connect late.sh users to the running BBS/LORD service using the existing Rebels-style terminal embedding pattern.

Target shape:

```text
user terminal
  -> SSH into late.sh
    -> service-ssh pod
      -> internal connection to lord-bbs-sv
        -> BBS launches LORD
```

Implementation direction:

- Generalize or duplicate the `door/rebels` proxy pattern for a BBS-backed door screen.
- If the BBS service exposes SSH, reuse most of the Rebels outbound SSH proxy.
- If the BBS service exposes Telnet/raw TCP, add a Telnet/raw backend and keep the same `vt100` render path.
- Add config such as `LATE_DRAGON_ENABLED`, `LATE_DRAGON_HOST`, `LATE_DRAGON_PORT`, and possibly protocol/login settings.
- Use a stable late.sh-to-BBS identity mapping. Do not connect all late.sh users as one BBS account.
- Consider derived usernames/passwords or a controlled auto-login bridge.
- Handle CP437-to-UTF-8 conversion before feeding text bytes into `vt100`, while preserving ANSI escape sequences.
- Force or strongly prefer an 80x24 viewport because old BBS doors expect that layout.
- Keep the BBS pod isolated from late.sh app secrets. It should need only its PVC and internal network access.

Open V2 questions:

- Does the BBS package expose a clean SSH service, or should late.sh connect by Telnet/raw TCP?
- Can we automate BBS account creation/login safely, or should V1 keep manual accounts?
- How much CP437 translation is needed after testing with the chosen BBS stack?
- Should the BBS/LORD screen be a top-level screen, a Door Games screen, or an Arcade/Rooms-adjacent entry?

### Licensing Email For This Path

Use the narrower hosting question, not the broader rewrite question:

```text
I want to purchase/register the BBS version of LORD and run it unmodified on a dedicated BBS instance. Users connect to late.sh over SSH, and late.sh would proxy their terminal session into that BBS. No source, text, or game assets would be copied into our codebase.

Is the BBS registration sufficient for this public hosted use, or do we need a separate public-hosting license?
```

Known contact trail:

- `sales@gameport.com`
- `info@gameport.com`
- Reddit/community reports mention `bill@playnetwebhosting.com` for recent LORD/LORD II/Planets registration handling, but this was not verified on official Gameport pages.

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
