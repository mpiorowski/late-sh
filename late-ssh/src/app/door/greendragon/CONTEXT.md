# Green Dragon door (`late-ssh/src/app/door/greendragon`)

A native, in-process door game: an open-source remake of LORD, modeled on
*Legend of the Green Dragon* (LoGD). Single-player, turn-based, DB-persisted
(one character per user), with an asynchronous multiplayer layer (chat rooms,
PvP against sleeping characters, bounties, clans, transfers). It uses the
**Lateania integration pattern** (native ratatui + a service + a `DoorGame`
impl), not the nethack PTY-proxy pattern, because LoGD is a web app with no
terminal to proxy; only its balance data and mechanics are reused.

Companion docs:
- [`MECHANICS.md`](MECHANICS.md): every system as ported (combat, forest,
  the dead realm, buildings, dragon kills, races, commentary, PvP, bounties,
  clans, transfers), each against its upstream file, plus the files in depth.
- [`PARITY.md`](PARITY.md): the per-formula audit, the corrections found
  while porting, and the deliberate deviations. A dated design journal (kept
  as one by repo convention). Read it before adding or "fixing" any mechanic.
- [`PLAYERS.md`](PLAYERS.md): the support digest (what each building does,
  gem sources, death costs, allies, PvP tactics).

## Upstream source of truth

Everything mechanical is transcribed from the **classic DragonPrime Edition**
of LoGD, the final content-complete release, **1.1.2** (DragonPrime
[ceased Sept 2019](https://dragonprime.net/index.php?topic=12736.msg106613)).
We compare against the GitHub mirror:

- **`jimlunsford/lotgd`**: <https://github.com/jimlunsford/lotgd> (raw:
  `https://raw.githubusercontent.com/jimlunsford/lotgd/master/<path>`). Self-described
  "DragonPrime Edition"; its creature/master/exp/weapon seed tables verified 1=1
  against [`data.rs`](data.rs). Key files we ported: `lib/battle-skills.php`
  (`rolldamage`), `lib/bell_rand.php`, `dragon.php`, `train.php`, `newday.php`,
  `bank.php`, `healer.php`, `lib/forestoutcomes.php`, `lib/experience.php`,
  `modules/specialty{mysticpower,darkarts,thiefskills}.php`, the 8 forest
  event modules, `graveyard.php` + `lib/graveyard/case_*.php`, and
  `lib/commentary.php` + its stock room callers (`gypsy.php`, `rock.php`,
  `gardens.php`, ...).

**Not** the source: the newer **`lotgd/core`** ("Daenerys") rewrite,
<https://github.com/lotgd/core>. It's a headless, **content-empty** engine
(no forest/dragon/masters/specialties), archived; nothing to port. Also
checked: **NB-Core/lotgd** ("+nb" v2.0.5) and
**stephenKise/Legend-of-the-Green-Dragon** are live PHP-8 modernizations of the
same 1.1.2 game with no mechanics changes, useful only as bug tie-breakers.

A local clone of the upstream source lives at **`upstream-lotgd/`** in the
repo root (gitignored; re-fetch per PARITY.md). Verify against those files
directly, never from memory.

Licensing rule: mechanics, odds and payouts are transcribed 1=1 from stock
core (not copyrightable); all names, prose and flavor are original to late.sh
(the seed is CC BY-NC-SA, whose NC+SA terms conflict with late.sh); the
separately licensed DragonPrime add-on modules are off-limits.

Original LORD lineage for reference: [Wikipedia, Legend of the Red
Dragon](https://en.wikipedia.org/wiki/Legend_of_the_Red_Dragon). LoGD project
hub: [dragonprime.net](https://dragonprime.net/).

## Module map (flat)

Every file has a `*_test.rs` sibling where it holds logic. Detail per file is
in `MECHANICS.md`, "Files in depth".

| File | Owns |
|---|---|
| `data.rs` | Balance tables (transcribed numbers) and original names: weapon/armor ladders, creature stat blocks 1-16, exp curve, masters/dragon, the graveyard shades, title ladder, the NPCs, haunt-fumble vignettes |
| `combat.rs` | The pure round resolver mirroring LoGD `rolldamage` with its quirks (normal-curve `bell_rand`, signed damage, power moves), `simulate_fight`, the buff engine and the companion engine |
| `specialty.rs` | The twelve specialty skills (Mystical / Dark Arts / Thief, 4 each) as buff or companion factories |
| `events.rs` | The eight stock forest events (15% pre-combat roll) with framing prose, choices and effect resolvers |
| `inn.rs` | The Sleeping Stag's resolvers: the bard's song, the flirt ladder and marriage |
| `tavern.rs` | The Dark Horse games: dice duel, stones, Five Sixes (pot settled through `svc`) |
| `commentary.rs` | The shared chat primitive's pure rules: rooms, windows, post preparation, the daily allowance, drunken slurring |
| `model.rs` | The persistent `Character` and every rule on it: stats, leveling, shops, healer, signed banking, death, new days, the dead realm, forest math, dragon kills, specialties, races, building economies, PvP, bounties, haunt, clans, transfers |
| `persist.rs` | JSON save envelope (`schema_version` + `character`), tolerant of missing fields, with the v1 → v2 migration |
| `svc.rs` | `GreenDragonService`: character load/save, news, the Five Sixes pot, commentary, the roster read, and every cross-player write (PvP, bounties, haunt, clans, transfers), plus dragon-kill rewards |
| `state.rs` | Per-session `State`: the authoritative `Character`, the `Mode` machine (every screen and talk line), the active `Encounter`, the new-day gates, the idle heartbeat save |
| `ui.rs` | Rendering only: the live page (stat rail + mode panel + event log) and the two-column Games-hub landing card |
| `screen.rs` | The `DoorGame` impl (`GAME`), key handling, the backtick hop (refused mid-fight), `leave`, and the idle reap after `door::game::IDLE_WINDOW` |

## Persistence

`greendragon_characters` (migration `092`, model `late-core/src/models/greendragon_character.rs`) is one JSONB blob per user, exactly like `mud_characters`: the character shape evolves without new migrations. The service computes a game-day number to drive the once-per-game-day forest-turn/heal reset on load: **one game day per 12 real hours** (00:00/12:00 UTC), a deliberate halving of upstream's `daysperday` 4 (see PARITY.md's user-feedback sweep), anchored at `svc::GAME_EPOCH` so every stored day number (saves, commentary watermarks, news) stays on one continuous count. Side tables: `greendragon_news` (096), `greendragon_settings` (097, the Five Sixes pot), `greendragon_commentary` (098), `greendragon_bounties` (099, one row per contract, `set_at` stamped up to 4h in the future as the maturity delay, closed rows pruned after 18 days), and `greendragon_clans` (101, the clan rows only; membership lives on the character blobs, and a clan whose last real member leaves is deleted lazily at list render). Dragon kills pay 10k chips each (migration 159) and award the `GDS` badge on the first (migration 100).

## Integration points (mirror Lateania)

`Screen::GreenDragon`, `HubGame::GreenDragon`, `DoorGameId::GreenDragon`,
`App::{greendragon_service, greendragon_state, enter_greendragon,
leave_greendragon}`, `SessionConfig`/server-`State` service injection
(main/ssh/session_bootstrap/test-helpers), render draw arm, input dispatch +
Esc, and the hub launch/landing. Leaving is centralized: Esc forwards to the
game so it backs out one menu level and only leaves to the hub from the village.
The door survives a screen switch: `greendragon_state.is_some()` keeps it on
the backtick cycle and lights its hub pip, and `App::tick` ends a visit left
off screen for the idle window with the same save that drops the presence
flag.

## Multiplayer layer

`state.rs` is authoritative per session, and the shared world sits on two
patterns:
- **Reading other characters**: `svc.load_roster()` decodes every stored
  character for the warrior list, Hall of Fame, and target lists. Online means
  the blob's `online` flag AND a save within 15 minutes; a 4-minute heartbeat
  save keeps live sessions inside that window, so they cannot be targeted.
- **Cross-player writes**: row-locked delta transactions (fresh-blob decode +
  `update_data_keep_updated`, which never touches the presence-feeding
  `updated` column, held under the victim's in-process write gate). PvP
  settlements, bounties, the haunt, clan rank changes, officer notices and
  bank transfers all use it; `PARITY.md`'s PvP section has the full
  concurrency story.

What runs on it (commentary rooms, PvP, bounties and the haunt, enemy intel,
clans, bank transfers, news, dragon-kill rewards) is in `MECHANICS.md`, "The
multiplayer systems".

## Parity status

The port is feature-complete against stock LoGD 1.1.2. Anything beyond it is
original late.sh content (a creative decision, not a parity item) under the
licensing rule above.

Deliberate gaps:
- **No King's tournament**: stock 1.1.2 contains no tournament module (only a
  hidden `source.php` reference); out of scope in `PARITY.md`.
- **No in-door mailbox.** Every systemmail moment rides the `pvp_reports`
  drain, DMs exist site-side, and an out-of-door ping waits on an app-wide
  notification design (`PARITY.md`, mail section).
- The Dark Horse's web-only "colors" tutorial is dropped.

## Tests

Pure modules are unit-tested in their `*_test.rs` siblings (combat, data,
events, inn, tavern, commentary, model, persist, state's menu builders).
Run them scoped: `make test-llm ARGS="-p late-ssh --lib -E 'test(greendragon)'"`.
