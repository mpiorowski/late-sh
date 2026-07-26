# Door Games & MUDs - Candidate Research

Investigation notes for slowly adding more door games / MUDs to late.sh.
Status: **research notes.** Last updated 2026-07-25 (added the incremental /
idle shelf: A Dark Room audited as the cheapest pattern-1 port on the list, plus
a license sweep of the famous idle games). Previous cut 2026-07-21: Brogue CE
is the next door, one marketing beat before the Green Dragon push (see root
`DRAGON.md`); TradeWars/twclone re-parked as future season/event content
despite a green spike; museum wing passed on.

## TL;DR

- **You wanted LORD but couldn't license it.** The legal, open-source answer is
  **Legend of the Green Dragon (LotGD)** - a faithful free remake of LORD. The
  catch: it's a PHP + MySQL *web* app, not a terminal door, so it needs work to
  fit our SSH model.
- **Easiest things that drop straight into our existing model:** **dopewars**
  (GPL, has a real curses terminal client + multiplayer server) and **Usurper**
  (GPL, LORD-like, already ported to 64-bit Linux). Both run as a normal process
  on a PTY - exactly how NetHack already works here.
- **TradeWars 2002 is a no-go on license** (proprietary, EIS/Pritchett own the
  trademark). The open path is **twclone** (GPL-2 clone), which would be a port,
  not the real thing.
- **The cheapest native port we have found is A Dark Room** (MPL-2.0, 8.5k lines
  of dependency-free jQuery, no server, no DB). It is a whole new *shelf* for
  late.sh, not just a door: minimalist text incrementals. See the deep dive
  below. The gates are brand etiquette and one design call, not license or
  effort.
- **MUDs are parked** (see bottom). Almost all the demand is for *doors*, not
  MUDs, and MUDs fight late.sh's quick-session format. Licensing is fine if we
  ever want one (DikuMUD LGPL, Evennia BSD), but it's not on the roadmap.

---

## How a door has to plug into late.sh

We already have three integration patterns (see root `CONTEXT.md` §2.9-2.11 and
`late-ssh/src/app/door/`). Any candidate is judged against these:

1. **Native Rust port** - like **Lateania**. Most work, full control, no
   licensing of *code* needed if we reimplement gameplay (mechanics aren't
   copyrightable; assets/text are). Right call for something we want to own.
2. **Real upstream binary on a PTY, proxied over SSH** - like **NetHack**
   (`late-nethack` host crate + russh client in `door/nethack`). Best fit for
   any game that's already a Unix terminal program. This is the cheapest way to
   add a *real* existing game - if it builds and runs on Linux and talks to a
   TTY, we can wrap it almost exactly like NetHack.
3. **Remote SSH door proxy** - like **Rebels in the Sky** (`door/rebels`). For
   games that already expose an SSH/telnet server.

**Decision rule:** prefer pattern (2) for anything that's already a Linux
terminal binary under a clean license. Fall back to (1) for web/DOS-only games
worth owning. Licensing is the gate before any of this matters.

---

## License traffic light

### 🟢 Green - clean license, go

| Game | License | Notes / fit |
|---|---|---|
| **dopewars** | GPL | Drug Wars / Dope Wars done right. Has a **curses text client** and a client/server **multiplayer** mode. Pure Linux terminal program → **pattern 2, near-zero friction.** Copyright Ben Webb 1998-2022, still maintained. |
| **Usurper** | GPL | **Shipped 2026-07-20** (`late-usurper` + the Usurper screen). Classic LORD-style RPG door. Rick Parrish ported it to **32/64-bit** (orig by Jakob Dangarden). Runs as a DOOR32 local-mode child on a PTY; the host generates per-session dropfiles, leases node numbers, and transcodes CP437→UTF-8. One shared world on a PVC. |
| **Legend of the Green Dragon (LotGD)** | GPL (≤0.9.7), Creative Commons (after) | **The open LORD.** Faithful remake. BUT it's **PHP + MySQL web**, not a terminal door → needs either a TUI front-end or a native port (**pattern 1**). Highest player-recognition payoff, highest effort. Active forks exist (incl. a Symfony rewrite). |
| **Wolfpack Empire** | GPLv3 | Classic large multiplayer strategy "Empire" door. Server + client, runs on Linux. Heavier/niche but clean. |
| **Dungeon Crawl Stone Soup (DCSS)** | GPL-2.0-or-later (project relicensed with every past contributor's consent) | **Shipped 2026-07-18** (`late-dcss` + the DCSS screen). Not a door - the *other* flagship roguelike - but the cleanest **pattern 2** candidate on this list. Native Linux curses binary (`crawl`), actively maintained, yearly releases. Built to be hosted: official public servers run dgamelaunch, and the game writes machine-readable `logfile`/`milestones` files (rune pickups, Zot entry, wins) - so achievements come off disk, no vt100 scraping like NetHack. Reuses the `late-nethack` host machinery almost verbatim. Wants 80x24 minimum. |
| **twclone** | GPL-2.0 (v1.0.0-rc1, Dec 2025). The README claims MIT, but the actual LICENSE/COPYING files are GPLv2 (GitHub detects GPL-2.0) | Independent TradeWars clone, **fully rewritten and now headless**: a TCP server with a **pure JSON protocol** and a **PostgreSQL** backend. No BBS, no DOSBox, no telnet/ANSI. The clean way to get TradeWars-like gameplay - but still a release candidate with ~175 open issues and federation/economy/NPC systems deferred. See deep dive below. |
| **Brogue CE** | AGPL-3.0 | The most beautiful pure-terminal roguelike ever made, and the friendliest of the classics: short runs, no grinding, stunning colored ASCII. Community Edition is actively maintained, builds a curses/terminal binary on Linux, saves per player, and public dgamelaunch servers already host it → **pattern 2, drop into the nethack/dcss host shape.** AGPL is fine for us: we build from source and can point at the pinned tarball. Best "third dungeon" candidate. |
| **Angband 4.2** | GPL-2.0 (dual-licensed Angband licence / GPLv2) | The third giant lineage next to NetHack and Crawl. Long-form dungeon diving, rock-solid ncurses build (`-mgcu`), per-user saves, still maintained → **pattern 2.** Opens the door to celebrated variants later (Sil-Q, FrogComposband), which reuse the same shape. |
| **A Dark Room** | MPL-2.0 (`LICENSE.md` + `package.json`, marked "Incompatible With Secondary Licenses") | The minimalist incremental everyone remembers: light a fire, build a village, walk an ASCII wasteland. Browser JS, so **pattern 1** - but by far the cheapest pattern-1 target we have seen (no server, no DB, no build step). Audited 2026-07-25, see the incremental shelf below. MPL is file-level copyleft and coexists with our FSL. |
| **Evolve** | MPL-2.0 | Ooze-to-space-empire incremental, 1.2k stars, genuinely deep endgame. Same clean license story as A Dark Room, and the same pattern-1 requirement - but it is 4,690 commits of content with an enormous UI surface. Green on law, huge on effort. That is not a door, that is a second Lateania. |
| **Antimatter Dimensions** | MIT | The genre's biggest number-go-up game and the cleanest license on this whole page. All numbers and tabs, so it renders in a terminal trivially. The open question is fit, not law: it is a 1000-hour optimization spreadsheet with no story and no aesthetic, the opposite pole of the genre from A Dark Room. |
| **Progress Quest** | Source released 2011, reported BSD. The Bitbucket Mercurial repo is gone, so **verify the license on whichever mirror we take** before shipping | The original zero-player RPG, and conceptually the most BBS-shaped idea in the genre: it plays itself, you log in to watch the bar fill. Tiny. A Python terminal edition already exists (`rr-/pq-cli`), so **pattern 2 is on the table** as well as a trivial native port. Best charm-per-hour on the list. |
| **NetHack variants: EvilHack / xNetHack / UnNetHack** | NGPL (same as NetHack) | Cheapest wins on the whole list: same license, same build recipe, and the **late-nethack host code reuses almost verbatim** - new crate, new port, new secret. EvilHack is what the hardcore public-server crowd plays; xNetHack is the polished modernization. Only cost is another image + pod each. |
| **Cataclysm: Dark Days Ahead** | CC-BY-SA-3.0 (code and data) | Modern zombie-survival roguelike with a real ncurses build. Genuinely popular. The catch is weight: big binary, big RAM per session, and long-lived per-player worlds on disk → **pattern 2 but a heavyweight**; treat as an experiment with one watched pod, not a casual add. |
| **The museum wing: Rogue 5.4 / Hack 1.0 / Umoria** | BSD-3-clause (Rogue 5.4.4 restoration; verify tarball license before shipping) / BSD (Hack) / GPL-3.0 (Umoria, relicensed 2017) | "Where it all began" shelf: the actual 1980 Rogue next to the roots of both family trees (Hack → NetHack, Moria → Angband). Tiny ncurses binaries, trivial hosting, one shared host crate could run all three → **pattern 2, minimal effort, maximum charm.** Great story for the public launch. |

### 🟡 Yellow - usable but read the terms

| Game | License | Notes |
|---|---|---|
| **GWT (Galactic Warriors Tournament)** | Source on GitHub, license unclear | Sci-fi LORD-like, source available; confirm license before use. |
| **Dominion** | Source on GitHub, license unclear | Fantasy RPG door; confirm license. |
| **Candy Box 2** | GPL-3.0 | A Dark Room's 2013 twin: ASCII art, hidden depth, same cult status. Perfect aesthetic fit, **wrong license for a native port** - linking GPL code into late-ssh would relicense late-ssh, which our FSL cannot do. The only clean path is a standalone GPLv3 binary we spawn on a PTY (pattern 2), which for a browser game means writing the entire thing ourselves and then giving it away. Yellow, not red: legal, just a bad trade. |
| **Swarm Simulator** | GPL-3.0 | Big idle name, 525 stars. Same GPL-versus-FSL problem as Candy Box 2, and it is CoffeeScript + AngularJS on top. Pass. |

### 🔴 Red - proprietary / licensing pain (avoid or port-only)

| Game | Why |
|---|---|
| **Legend of the Red Dragon (LORD)** | Proprietary (the licensing wall you already hit). Use **LotGD** or **Usurper** instead. |
| **TradeWars 2002** | Proprietary; EIS / John Pritchett hold trademark + rights. Would need a paid license. Use **twclone**. |
| **Barren Realms Elite / Solar Realms Elite** | Proprietary inter-BBS games (Jeff Graham / Galactic). No open source. Also designed as competitive inter-BBS, awkward for a single host. |
| **The Pit** | DOS gladiator door by James R. Berry / Midas Touch (1990; Berry died 1999). **No registration code is required to run it anymore**, so it's free to *play* - but the source is now owned by **BBSFiles.com**, with no open-source license, and there's **no clone/port**. So: same DOS-door stack as TW2002 (DOSBox + BBS + door32) and no code rights to embed or port. See note below. |
| **Land of Devastation, Arrowbridge I/II, Sinbad, Bordello, Yankee Trader** | Old proprietary/abandonware DOS doors. No clean license; only runnable via DOSBox wrappers (e.g. DoorNode) which doesn't grant rights. Treat as red unless an author releases source. |
| **DrugWars / Dope Wars (the originals)** | Originals are proprietary/abandonware - but **dopewars (green, above) is the GPL reimplementation**, so this is solved. |
| **Kittens Game** | The custom **"WET PAWS LICENSE"**: changes allowed "for personal or educational purpose", but explicitly **no commercial gain and no creation of derivative works**. A port is precisely a commercial derivative work. |
| **Cookie Clicker** | Proprietary. The source is readable, and Orteil has said so publicly, but the header is "copyright Orteil" plus "do not re-host it, do not profit from it and do not present it as your own". Readable is not licensed. |
| **Universal Paperclips** | **No license at all** and no copyright notice, so all rights are reserved by default. Every GitHub "paperclips" repo is an unofficial mirror or fork, none of which grants us anything. Frank Lantz never released it. |
| **Falcon's Eye** | Not a separate game - it's a **NetHack** frontend (graphical). We already run real NetHack; nothing new here. |

---

## TradeWars: deep dive (the one everybody asks for)

TradeWars comes up more than anything else, and trying to host the *real*
TW2002 is genuinely awful. Here's why, and the way out.

### Why proxying real TW2002 is so painful

TW2002 is a **DOS door**. To run the authentic game you need the whole stack:

- A BBS package (Synchronet / Mystic / WWIV) to act as the door host.
- **TWGS** (Trade Wars Game Server) - the standalone server build - which is
  **proprietary and paid**, and speaks **rlogin** on port 2002.
- DOSBox/DOSEMU + a **FOSSIL driver** + door32 plumbing to bridge the DOS
  binary to a socket.
- Then a telnet/rlogin -> SSH proxy on top to reach late.sh, plus **CP437 ->
  UTF-8** translation so the ANSI art doesn't turn into garbage.

That's four fragile layers and a license purchase before a single player logs
in. Someone already built the proxy half of this in Go - `erikh/trade`, an
"SSH -> telnet proxy, primarily for tradewars" that even does the CP437->UTF-8
fixups - which tells you this is a well-known pain point, not just us.

**Verdict on the real thing:** red. Proprietary server, DOS emulation, ANSI
mess. Not worth it.

### The actual answer: twclone (GPL-2, headless, JSON + Postgres)

`twclone` was **fully rewritten**, with **v1.0.0-rc1 released Dec 15 2025** (a
release candidate, not a finished 1.0.0 - ~175 open issues, federation/economy/
NPC systems still deferred), and it's shaped almost perfectly for late.sh:

- **GPL-2 licensed** (the README says MIT, but the actual LICENSE/COPYING files
  are GPLv2) - same license family as dopewars, donations/chip economy is fine.
- **Headless TCP server, no BBS** - just run the server binary.
- **Pure JSON protocol** - "all client<->server interactions use JSON." No
  telnet, no ANSI, no CP437. Any language that speaks JSON can be a client.
- **PostgreSQL backend** - which late.sh already runs.
- Forked game-engine process for clocks/economy/NPCs; "100+ connections"
  out of the box.

**Why this is better than the NetHack/Rebels approach for TradeWars:** we don't
proxy a terminal at all. We run the twclone server alongside our Postgres and
write a **native Rust TUI client** (`door/tradewars`) that speaks JSON to it -
the same ownership level as Lateania, but we don't have to design the game. We
render the universe/ports/combat ourselves in ratatui, so it looks native to
late.sh instead of being a blitted foreign terminal. The JSON protocol means no
screen-scraping for milestones either (contrast NetHack, where we scrape vt100
for the Amulet/ascension) - we read game state straight off the wire.

### Protocol spike results (2026-07-20, repo audit + local build)

Audited the full repo (docs, both bundled clients, server source, SQL, test
rig) and built it locally on Arch: `./configure && make` produces `server` and
`bigbang` cleanly (needs `touch aclocal.m4 configure Makefile.in` first to
stop autotools regen on fresh clones). Local reference clone: `~/projects/twclone`.

**The key question is answered: YES, the protocol is data, not screens.**
- Newline-delimited JSON over TCP (default port 1234), request/response
  correlated by `id`/`reply_to`; frames without `reply_to` are async push
  events (pub-sub via `subscribe.*`). Docs explicitly mandate "no prose inside
  `data`": everything is codes/ids/enums, the client renders.
- Menus are 100% client-side (`client/menus.json` is the Python client's own
  config; the server never sees it). Our ratatui UI is unconstrained.
- All game logic is server-side: trade math, combat, pathfinding
  (`move.pathfind`), autopilot routes, economy. Both bundled clients (Python
  menu client + the LLM-driven `ai_player` bot) are thin protocol clients
  using zero hidden commands. A Rust client owns only rendering, input,
  framing, and deserialization.
- Command surface: **235 commands registered** in `src/server_loop.c` (the
  shipped `published_commands.json` lists 177 and is stale). Move/trade/
  combat/planets/citadels/corps/stock market/banking/tavern gambling/mail/
  news/bounties/insurance are all real implementations with protocol-level
  integration tests (`tests.v2/`, ~185 commands covered). Discover schemas at
  runtime via `system.cmd_list` / `system.describe_schema`, NOT from the docs.
- Identity: one TCP connection = one authenticated player (session token from
  `auth.login`/`auth.register`). No single-socket multiplexing, so late-ssh
  opens one connection per active door session. Fits the arcade-handle +
  host-held random password pattern we already use for Usurper dropfiles.

**Postgres: shares our instance fine.** Postgres is the only real backend
(MySQL driver is a stub; server hard-fails on non-PG). Wants a dedicated
database (unqualified names in `public`), no extensions, no pg_cron (cron is
a DB table driven by its own engine), no superuser if we pre-create the DB and
role. Config via a `bigbang.json` (libpq conninfo), not env vars. `bigbang` is
the one-time universe generator (default 500 sectors).

**Caveats found (none fatal, all handleable):**
- **Plaintext password storage** (`repo_auth.c`: raw `strcmp` against a
  `passwd` column). Contained for us: users never type a password, our host
  mints random per-user credentials, and the server sits on the internal
  network. Never expose port 1234 publicly.
- **One blocking libpq connection per client thread**: 100 players = 100 PG
  backends. Upstream's answer is pgbouncer (their deploy script even ships a
  hardcoded password - ignore it, we do our own deploy). Our concurrent door
  sessions will be small; a hard client cap or pgbouncer in the pod solves it.
- **Ships a dev build**: `bin/Makefile.am` bakes in ASan/UBSan. Strip
  sanitizers for the prod image.
- **Server auto-generates a universe if the DB looks empty** - point the
  conninfo carefully.
- Docs are partly aspirational and contradict the wire (e.g. `passwd` vs
  `password`, money fields int-or-string, police bribe/surrender and bank
  standing orders are stubs, federation/S2S is not real). Trust the runtime
  schema endpoints and `tests.v2/`, not the markdown.
- **Project pulse**: solo, heavily AI-assisted development; quiet since
  2026-02-14; 175 open issues (P0s are mostly a localisation epic + test
  infra, not broken gameplay; 42 "canon" deviations from real TW2002 open).
  Plan to pin a commit and treat it as ours to patch (GPL-2; the README
  claims the rewrite is MIT but COPYING/LICENSE both say GPLv2 - either way
  fine, we run it as a separate process).

**Integration shape:** twclone server + own PG database as one pod (own
image, sanitizers stripped, TLS off - internal network, SSH fronts it);
`door/tradewars` native ratatui client in late-ssh speaking NDJSON over TCP;
one shared persistent universe (Lateania-style, not per-player); milestones
and achievements read straight off the wire (no scraping). Effort sits
between dopewars and a native port: no game to design, but a full multi-panel
TUI to build. Start with the core loop (sector view, warp, port trade, ship,
bank) and grow toward planets/corps/stardock.

## The Pit (the gladiator one)

Popular ask, but it lands the same place as real TW2002, just without the paid
server:

- DOS door by **James R. Berry / Midas Touch Software (1990)**; Berry died in
  1999. Warriors fight in an arena in Regal City vs. AI or other players. Had a
  fancy optional "Pit Terminal" front-end (EGA/MIDI) back in the day.
- **Free to run now** - the bundled `register.txt` says no registration code is
  required anymore. That removes the *paywall* but not the *copyright*: the
  source is owned by **BBSFiles.com** (reportedly being updated for modern OSes),
  under no open-source license.
- **No clone or port exists.** Unlike LORD->LotGD or DrugWars->dopewars, there's
  no clean reimplementation to lean on. The only GitHub artifacts are a v4.17
  registration patch and the old front-end - not a hostable codebase.

**Verdict:** red-ish. We *could* technically run the DOS binary through the same
DOSBox + door32 + proxy stack as TW2002 (and it's free of reg fees), but that's
exactly the painful path we're trying to avoid, and we'd have no rights to port
or modify it. Not worth it while dopewars/Usurper/twclone are clean wins. If we
ever want the gladiator-arena vibe, a **native Rust original** inspired by it
(mechanics aren't copyrightable) is the only sane route - and at that point it's
really a new Lateania-style game, not "The Pit."

**Resolved 2026-07-21:** the gladiator vibe is absorbed into the Green Dragon
plan instead - a town arena where player characters fight each other, results
called into #lounge (see root `DRAGON.md`, we-own-it liberties). Gladiators
fighting is a mechanic, not a door; The Pit stays red and stays unhosted.

## The incremental shelf: A Dark Room and its neighbours

A different genre from everything above, and worth treating as its own shelf
rather than one more door. Minimalist text incrementals are the closest thing on
the open web to what late.sh already looks like: no graphics, no art budget,
progressive reveal, prose as the interface.

### Why the genre fits us structurally, not just aesthetically

Every door we run today is a **run** (NetHack, DCSS, Brogue: you start over when
you die) or a **daily-turn RPG** (Green Dragon, Usurper). An incremental is a
third shape: **a save that grows**. That is a different retention curve, and it
is the one a BBS was always built around - a reason to log in, check on the
thing, log out. It is also single-player, which sidesteps exactly the problem
that parked twclone: no player density required, so it works at 30 concurrent
users just as well as at 3,000.

**The one design call, and it is not optional:** A Dark Room has **no offline
progress**. Every timer is wall-clock while the tab is open, and `collectIncome`
runs on a 1-second tick; close the tab and the village stops. Ported faithfully,
that means you only advance while your SSH session is attached, which turns
"check in daily" into "idle in a session", the opposite of the pitch above. Two
honest options:

1. **Faithful.** Accept it. ADR is a 2-4 hour narrative arc with a real ending,
   not an endless idler, so a player realistically does it across a handful of
   sittings anyway. Cheapest and truest.
2. **Offline accrual.** Compute elapsed time on reconnect and settle the
   production tick forward. We are writing the port, so this is ours to add, but
   it rebalances the whole game (the fire, the wanderers, the population curve)
   and needs caps or the wasteland is trivial after one night away.

Pick this before writing code, not after.

### A Dark Room: license (the short version)

- **MPL-2.0**, stated in both `LICENSE.md` and `package.json`, with the Exhibit B
  "Incompatible With Secondary Licenses" notice attached. That notice only blocks
  relicensing under GPL/LGPL/AGPL, which we do not want to do anyway.
- **MPL is file-level copyleft with no network clause** (unlike Brogue's AGPL).
  Section 3.3 explicitly lets us ship a Larger Work under our own terms as long
  as the covered files stay MPL. So the practical rule is: everything derived
  from ADR (the game text, the data tables, the balance constants, anything
  transliterated rather than reinvented) lives in its own module carrying
  MPL-2.0 headers, and the rest of late-ssh stays FSL-1.1-MIT. Our repo is
  already public, so the source-availability obligation is satisfied by
  existing. Record it in `LICENSING.md` and NOTICE the way we did for Brogue.
- **Only the web version is MPL.** The iOS/Android port (Amir Rajan, RubyMotion),
  the Steam release, and the prequel *The Ensign* are separate closed products.
  Port from `doublespeakgames/adarkroom` and nothing else.
- **MPL grants no trademark rights** (§2.3, explicit).

### A Dark Room: the etiquette gate (the actual blocker)

This is the part that matters more than the license. Michael Townsend
open-sourced ADR so people could learn from it, and has said publicly he was
disheartened watching people use the brand he built to make a quick buck (the
App Store clone wave). He is not a dead author of abandonware: the game is
**still a live commercial product**, on iOS, Android and Steam.

late.sh has a chip economy. Shipping a door called "A Dark Room" next to it,
without asking, is a good way to become the thing he complained about, even
though the MPL permits it.

**Recommendation:** email him first. `michael@doublespeakgames.com` is right
there in `package.json`. A yes costs one email and buys a good story we can put
in #lounge. A no saves us the entire port. Either way: credit prominently, and
never put it behind chips or donations.

### A Dark Room: code audit (2026-07-25, shallow clone of `main`)

The verdict is that this is the cheapest pattern-1 target we have looked at, by
a wide margin. LotGD was a PHP + MySQL web application. ADR is a single-player
state machine with no backend at all.

- **8,481 lines** of plain, dependency-free jQuery-era JS across 19 files in
  `script/`. No build step, no framework, no database, no server. The single
  npm dependency is `express`, used only by `dev-server.js`.
- Biggest files: `events.js` 1,487 (encounter and combat scenes, overwhelmingly
  data), `room.js` 1,259, `world.js` 1,109 (wasteland generation and travel),
  `engine.js` 942, `outside.js` 665, `space.js` 631, `state_manager.js` 440,
  `path.js` 341, `fabricator.js` 244, `ship.js` 177.
- **Roughly 1,100 lines we delete outright**: `dropbox.js` (361, cloud saves),
  `audio.js` + `audioLibrary.js` (376), `notifications.js`, `Button.js` (131, a
  DOM widget). A further large slice of every remaining module is jQuery DOM
  construction that ratatui replaces rather than translates.
- **State is one nested JSON blob** behind a single `StateManager` (`$SM`) with
  fixed top-level categories: features, stores, character, income, timers, game,
  playStats, previous, outfit, config, wait, cooldown. That maps almost 1:1 onto
  a serde struct persisted per user, which is the Green Dragon shape we already
  run.
- **The timer model is already centralized.** Everything goes through
  `Engine.setTimeout` / `Engine.setInterval` wrappers rather than raw calls, so
  the fire cooling, temperature drift, population growth, income tick and enemy
  attack cadence all funnel through one indirection point. A ratatui tick loop
  covers the lot.
- **The text is already externalized for i18n**: `_()` calls throughout (108 in
  `room.js` alone), with `lang/*.po` files for 23 languages and a `.pot`
  template. The strings extract cleanly instead of having to be dug out of
  logic, and we would inherit the translations for free.
- Lightly maintained upstream (last commit May 2025), which is fine: we pin a
  commit and own the port.

**Effort:** comparable to Green Dragon or less, and with none of the design work,
because every balance number is sitting right there in the source.

### The rest of the genre, at a glance

The famous ones sort cleanly, and the sort is mostly bad news, which is what
makes ADR valuable:

- **Green and portable:** A Dark Room (MPL-2.0), Evolve (MPL-2.0, but enormous),
  Antimatter Dimensions (MIT, but tonally the opposite of us), Progress Quest
  (reported BSD, verify the mirror; tiny and perfectly BBS-shaped).
- **GPL, so out-of-process only:** Candy Box 2, Swarm Simulator. Aesthetically
  Candy Box 2 is the one that hurts to skip.
- **Closed, do not attempt:** Kittens Game (its licence forbids derivative works
  by name), Cookie Clicker (proprietary, "do not re-host, do not profit"),
  Universal Paperclips (no license at all, so all rights reserved).

If we want a second game on this shelf after ADR, **Progress Quest is the pick**:
smallest possible port, an existing terminal edition to crib from, and the joke
lands harder on an SSH server than it ever did on the web.

## Recommended order of attack

1. **dopewars** - **done, shipped.** GPL, terminal-native. Runs as its own
   `late-dopewars` SSH host (NetHack-style), single-player with a shared
   high-score table. See `late-ssh/src/app/door/dopewars/CONTEXT.md`.
2. **DCSS** - **done, built (prod deploy pending).** Same standalone-SSH-host
   pattern as NetHack (`late-dcss` host crate + `door/dcss` client), from-source
   0.34.1 console build with wizard mode compiled out. File-based milestones
   (no scraping) deferred to a v2. See `late-ssh/src/app/door/dcss/CONTEXT.md`.
   First rollout must be `deploy_dcss.yml` (it builds the image).
3. **Usurper** - **done, built (prod deploy pending).** Standalone-SSH-host
   pattern like DCSS (`late-usurper` host crate + `door/usurper` client), built
   from pinned source with Debian's Free Pascal, world data generated by
   scripting the EDITOR's Reset Game at image build. First rollout must be
   `deploy_usurper.yml` (it builds the image). See
   `late-ssh/src/app/door/usurper/CONTEXT.md`.
4. **Legend of the Green Dragon** - **done, shipped** as the native Green
   Dragon door: an in-process Rust remake of LoGD with per-user persistent
   characters (pattern 1, Lateania-style). See
   `late-ssh/src/app/door/greendragon/CONTEXT.md`.
5. **Brogue CE** - **done, built 2026-07-21 (prod deploy pending).**
   Standalone-SSH-host pattern like DCSS (`late-brogue` host crate + the
   `door/brogue` client), from-source CE 1.15.1 curses-only build. Identity
   is a per-player cwd under the playground (brogue opens saves/recordings/
   high scores relative to its working directory; no name flag), keyed by the
   shared arcade handle. One source patch (`scripts/brogue_hangup_save.patch`)
   adds the SIGHUP save-and-exit the curses build lacks upstream, so
   disconnects and rollouts keep runs; verified by hand against a PTY. v1
   ships without awards as planned (a future pipe can read the per-player
   run-history files host-side). AGPL-3.0 note: we run a patched build, so
   the pinned tarball + patch + Dockerfile recipe must stay public (NOTICE).
   First rollout must be `deploy_brogue.yml` (it builds the image). See
   `late-ssh/src/app/door/brogue/CONTEXT.md`. Museum wing (Rogue/Hack/
   Umoria) was considered for the same slot and passed on.
6. **TradeWars via twclone** - **parked until it can be a season.** Protocol
   spike done 2026-07-20 (see deep dive): verdict green on the tech
   (structured-data protocol end to end, Postgres coexists on our instance,
   caveats handleable). Parked anyway: an always-on persistent universe is
   appointment gaming that needs player density we don't have; at ~30
   concurrent it's an empty 500-sector world, and its stories (ambushes, corp
   wars) all require other players. The right format is **event content after
   the dragon ships**: a fresh small universe per season (`bigbang` makes
   regeneration cheap), a daily turn ration, "highest net worth by Sunday",
   #lounge coronation, universe dies. Do not build the always-on version.
7. **A Dark Room** - **candidate, not scheduled.** Native Rust port (pattern 1),
   audited 2026-07-25: MPL-2.0, 8.5k lines of backend-free JS, state already a
   single JSON blob, timers already centralized, text already externalized for
   i18n. Cheapest pattern-1 port on this page. Two things gate it, and neither
   is effort: **send Townsend an email first** (live commercial product, and he
   has been burned by brand-squatting clones), and **decide offline progress vs.
   faithful** before writing code. Opens a new shelf rather than adding one
   door; **Progress Quest** is the natural cheap follow-up on that shelf.

MUDs are intentionally **not** in this list anymore - see Parked below.

## Open questions before building anything

- For LotGD: native Rust port vs. running the PHP app behind a TUI shim? Port is
  more work but matches how we own Lateania; shim is faster but drags in
  PHP/MySQL infra.
- Commercial/non-commercial: late.sh has a chip economy and may take donations -
  the non-commercial MUD licenses (Circle/Merc/ROM) need a real read before use.
  The green-list games (GPL/BSD/MIT/LGPL) are safe on this axis.
- Multiplayer state: dopewars/Wolfpack have their own servers - decide whether
  each player gets an isolated instance (NetHack-style) or shares one persistent
  world (Lateania-style).
- For A Dark Room: offline progress or faithful-no-offline? See the incremental
  shelf above. It changes the balance of the whole game, so it is a decision to
  make before the port starts, not a feature to bolt on after.
- General rule this research surfaced: **any pattern-1 native port has to be
  license-compatible with our FSL**, which rules out GPL sources (Candy Box 2,
  Swarm Simulator) that pattern 2 would have been fine with. MPL/MIT/BSD are the
  portable licenses; GPL games have to stay a separate process. Worth applying
  to every future candidate before we get attached to one.

---

## Parked: MUDs (low demand, not on the roadmap)

Researched, deliberately shelved. **Almost all the demand we've seen is for
doors, not MUDs.** MUDs also fight late.sh's format: a door is a quick
self-contained session that drops next to NetHack/Lateania, while a MUD wants to
be your whole evening and competes with our own chat/rooms. People who want a
MUD already have hundreds of live ones to go to; nobody's nostalgic for a *dead*
MUD the way they are for a vanished door.

If interest ever shows up, the licensing is already clear:

- **DikuMUD** (gamma/alpha/II) - **LGPL since 2020.** The classic combat-MUD base.
- **Evennia** - **BSD 3-Clause.** Modern Python framework; best for *building* a
  new world rather than running a 90s one. Connect with any MUD client on `:4000`.
- **CircleMUD / tbaMUD** - custom non-commercial + attribution (inherits Diku
  terms). The non-commercial clause matters given our chip economy/donations.
- **Merc / ROM** - custom Diku-derived; ROM requires credits in the login screen.

Likely integration would be **pattern 3** (remote proxy over telnet/MUD-client),
not a native port.

---

## Sources

- [LotGD GitHub org](https://github.com/lotgd) · [DragonPrime edition](https://github.com/jimlunsford/lotgd) · [stephenKise port](https://github.com/stephenKise/Legend-of-the-Green-Dragon) · [SourceForge](https://sourceforge.net/projects/lotgd/) · [OpenSource wiki](https://opensource.fandom.com/wiki/Legend_of_the_Green_Dragon)
- [dopewars on GitHub](https://github.com/benmwebb/dopewars) · [site](https://dopewars.sourceforge.io/) · [FSF directory](https://directory.fsf.org/wiki/Dopewars) · [Libregamewiki](https://libregamewiki.org/Dopewars)
- [Usurper (rickparrish)](https://github.com/rickparrish/Usurper)
- [Wolfpack Empire](https://sourceforge.net/projects/wolfpack-empire-bbs-door/)
- [twclone (MIT)](https://github.com/rdearman/twclone) · [twclone project page](https://twclone.sourceforge.net/) · [Trade Wars - Wikipedia](https://en.wikipedia.org/wiki/Trade_Wars) · [Gary Martin interview](https://breakintochat.com/blog/2019/07/19/gary-martin-creator-tradewars-2002/)
- TradeWars hosting reality: [erikh/trade SSH->telnet proxy](https://github.com/erikh/trade) · [TWGS on Synchronet](http://wiki.synchro.net/howto:door:trade_wars_game_server) · [TW2002 on WWIV](https://docs.wwivbbs.org/en/wwiv53/chains/tradewars2002/)
- The Pit: [Break Into Chat wiki](https://breakintochat.com/wiki/The_Pit) · [My Abandonware](https://www.myabandonware.com/game/the-pit-gm6) · [v4.17 registration patch](https://github.com/rambkk/The-Pit-bbs-door-game-patch)
- [CircleMUD](https://www.circlemud.org/) · [CircleMUD wiki](https://mud.fandom.com/wiki/CircleMUD) · [Evennia](https://www.evennia.com/) · [awesome-muds](https://github.com/maldorne/awesome-muds) · [awesome-mud](https://github.com/mudcoders/awesome-mud)
- Incremental shelf: [A Dark Room source](https://github.com/doublespeakgames/adarkroom) · [LICENSE.md (MPL-2.0)](https://github.com/doublespeakgames/adarkroom/blob/main/LICENSE.md) · ["A Dark Room goes open source"](http://blog.doublespeakgames.com/news/a-dark-room-goes-open-source/) · [LWN writeup](https://lwn.net/Articles/612829/) · [Wikipedia](https://en.wikipedia.org/wiki/A_Dark_Room) · [Haskell WIP terminal port](https://github.com/jordangedney/adarkroom-port)
- Genre neighbours: [Evolve (MPL-2.0)](https://github.com/pmotschmann/Evolve) · [Antimatter Dimensions (MIT)](https://github.com/IvarK/AntimatterDimensionsSourceCode) · [Candy Box 2 (GPLv3)](https://github.com/candybox2/candybox2.github.io) · [Swarm Simulator (GPL-3.0)](https://github.com/swarmsim/swarm) · [Kittens Game (WET PAWS LICENSE)](https://github.com/nuclear-unicorn/kittensgame) · [Progress Quest](https://progressquest.com/) · [pq-cli terminal edition](https://github.com/rr-/pq-cli)
- [DoorNode (DOSBox door launcher)](https://github.com/dinchak/doornode) · [BBS door game wiki](https://breakintochat.com/wiki/BBS_door_game) · [Dominion](https://github.com/mostlygeek/dominion) · [GWT](https://github.com/Rurik/GWT)
</content>
