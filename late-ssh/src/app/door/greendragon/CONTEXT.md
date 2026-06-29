# Green Dragon door (`late-ssh/src/app/door/greendragon`)

A native, in-process door game: an open-source remake of LORD, modeled on
*Legend of the Green Dragon* (LoGD). Single-player, turn-based, DB-persisted
(one character per user). It uses the **Lateania integration pattern** (native
ratatui + a service + a `DoorGame` impl), not the nethack/rebels PTY-proxy
pattern, because LoGD is a web app with no terminal to proxy — only its balance
data and mechanics are reused.

## Module map (flat)

| File | Owns |
|---|---|
| `data.rs` | Balance tables + flavor. **Numbers** (weapon/armor cost ladder, per-level creature stat blocks 1–16, exp curve with dragonkill scaling, master/dragon stats `45/25/300`) are transcribed from the LoGD balance (`jimlunsford/lotgd`); mechanics/numbers aren't copyrightable. **Names** (creatures, masters, `WEAPON_NAMES`/`ARMOR_NAMES`) are *original to late.sh* — the seed is CC BY-NC-SA, whose NC+SA terms conflict with late.sh, so we wrote our own. Pure constants + lookups (`weapon_name`/`armor_name`). |
| `combat.rs` | The pure round resolver mirroring LoGD `rolldamage`: triangular `bell_rand`, 5% triple-crit, glancing hits floored to zero, reroll-until-progress. Takes `&mut impl Rng` so it's seed-testable. Plus `simulate_fight` for balance checks. Also the **buff engine**: a `Buff` bundle (atk/def/enemy-atk/def/dmg multipliers, regen, lifetap, minion hits, damage-shield, rounds) and `resolve_round_buffed`, the primitives every specialty skill compiles to. Mirrors LoGD's `apply_buff` fields. |
| `specialty.rs` | The twelve specialty combat skills (Mystical / Dark Arts / Thief, 4 each), ported **1=1** from LoGD's `specialtymysticpower`/`specialtydarkarts`/`specialtythiefskills` modules: each is a use-cost + a `combat::Buff` factory scaled by level/attack. **Mechanics transcribed (uncopyrightable); names + flavor original to late.sh.** |
| `events.rs` | The eight stock forest special events (findgold, findgem, goldmine, fairy, glowingstream, crazyaudrey/baskets, foilwench, darkhorse/tavern). A 15% pre-combat roll (`forestchance`), even-weighted. Each has framing prose + an optional accept/decline choice + an effect resolver (gold/gems/turns/heals/skill, plus two death paths). **Effect numbers transcribed 1=1; all prose original; no module text copied.** `darkhorse` is reduced to a rest (no PvP intel / dice / comments), `glowingstream` keeps its 1–10 table. |
| `model.rs` | The persistent `Character` and all rules on it: stat derivation (`max_hp = 10*level + dragon_hp_bonus`, `attack = level + weapon_tier + dragon_attack_bonus`, `defense = level + armor_tier + dragon_defense_bonus`), leveling, shop pricing with 75% trade-in, healer cost (`round(ln(level)*(missing+10))`), banking + new-day interest (`apply_new_day_interest`, gated like LoGD), forest death (gold→0, exp×0.9), new-day reset, dragon-kill run reset. A kill banks `DRAGON_POINTS_PER_KILL` dragon points; `buy_upgrade(GypsyUpgrade)` spends them on permanent across-run boons (Vitality/Might/Guard/Stamina) — LoGD's dragon-point economy. `scaled_dragon`/`scaled_master` grow those foes with your banked investment so boons never trivialize them. Serde-able with field defaults (new boon fields default to 0, so old saves load clean). Also the **specialty economy**: a `Specialty` (None/Mystical/DarkArts/Thief) plus `gems`, `specialty_skill`, `specialty_uses`. `choose_specialty`/`increment_specialty` (+1 skill, +1 use per 3, mirroring `incrementspecialty`), `refresh_specialty_uses` on new-day (`floor(skill/3)` + 1 for your path), `spend_specialty_uses` for casting. |
| `persist.rs` | JSON save envelope (`schema_version` + `character`), tolerant of missing fields. |
| `svc.rs` | `GreenDragonService` (cheap `Clone`, `Arc`-backed): async character load via a `watch` channel, fire-and-forget save/delete over `greendragon_characters`. Holds `ActivityPublisher`/`ChipService` for the not-yet-wired dragon-kill reward. |
| `state.rs` | Per-session `State`: owns the authoritative `Character` (single-player, no shared world), a `Mode` machine (Village/Forest/Fight/shops/Healer/Bank/Training/Gypsy/**Event**/**ChooseSpecialty**/Graveyard), the active `Encounter` (now carries `buffs`), the `pending_event`, a capped message log, and every player action as a method. The forest roll fires an event (15%) before spending a turn; fights step `resolve_round_buffed`; the fight menu lists castable specialty skills between Attack and Flee. Drains the load channel in `tick()`. Pure menu builders are unit-tested. |
| `ui.rs` | Rendering only: the live page (stat rail + mode panel + event log) and the two-column Games-hub landing card. |
| `screen.rs` | The `DoorGame` impl (`GAME`), launcher/active key+arrow handling, and `leave` (save + return to the Games hub). |

## Persistence

`greendragon_characters` (migration `092`, model `late-core/src/models/greendragon_character.rs`) is one JSONB blob per user, exactly like `mud_characters` — the character shape evolves without new migrations. The service computes a UTC day-number to drive the once-per-day forest-turn/heal reset on load.

## Integration points (mirror Lateania)

`Screen::GreenDragon`, `HubGame::GreenDragon`, `DoorGameId::GreenDragon`,
`App::{greendragon_service, greendragon_state, enter_greendragon,
leave_greendragon}`, `SessionConfig`/server-`State` service injection
(main/ssh/session_bootstrap/test-helpers), render draw arm, input dispatch +
Esc, and the hub launch/landing. Leaving is centralized: Esc forwards to the
game so it backs out one menu level and only leaves to the hub from the village.

## Faithfulness notes (verified against `jimlunsford/lotgd` master)

These were checked against the actual LoGD PHP source, not memory. Each names the upstream file it matches.

- **Combat** mirrors `rolldamage` (`lib/battle-skills.php`): triangular `bell_rand`, 1-in-20 player triple-crit, glancing hits floored to zero, reroll-until-progress.
- **Forest death** (`lib/forestoutcomes.php`): on-hand gold → 0, experience × 0.9 (`forestexploss` default 10%), bank untouched, sent to the graveyard until new-day. Matches exactly.
- **Forest hunt** (`forest.php`): slum / hunt / thrill shift the target creature level by **−1 / 0 / +1** (not ±2), plus a small random jitter (~1/3 of searches nudge ±1) layered at the call site in `start_forest_fight`. Thrillseeking pays +10% gold/exp.
- **Shop gating** is level-gated (`available_tiers` caps at `c.level`) so you can't grind gold to out-gear your rank — matches LoGD selling gear by level.
- **Healer cost** `round(ln(level)*(missing+10))` matches `healer.php` exactly (the optional `healmultiply` module hook is 1.0 on a stock install).
- **Bank interest** (`newday.php`): a **random 1–10% daily rate** applied on the new-day rollover, but only if ≤`FIGHTS_FOR_INTEREST` (4) turns were left unused and the balance is under `MAX_GOLD_FOR_INTEREST` (100k). RNG in `svc`, rule in `model::apply_new_day_interest`.
- **Master fight** (`train.php`): non-lethal — a loss **heals you to full** ("stays the final blow") and sends you home with no penalty; a win is +1 level, +10 max HP, +1 atk, +1 def, full heal (all via our level derivation). The master scales with investment (`scaled_master`, factor 0.33).
- **Dragon scaling** (`dragon.php`): the dragon is **not** fixed — it grows with your banked investment (attack/defense boons + earned HP/5), `round(investment·0.75)` points randomly split into +atk/+def/+5HP (`scaled_master`/`scaled_dragon`, shared `partition_flux`). This is LoGD's fix for the "boons make you undefeatable" problem.
- **Gypsy / dragon-point economy** (`Mode::Gypsy`, `GypsyUpgrade`, `buy_upgrade`): a kill banks `DRAGON_POINTS_PER_KILL` points, spent on permanent Vitality/Might/Guard/Stamina boons. Flat 1-point costs for v1.
- **Forest events** (`events.rs`, the 8 stock forest modules): a 15% pre-combat roll (`forestchance`), even weight per module (each installs at `return 100`). Effect tables transcribed 1=1 — findgold `level·10..50`; goldmine's 1–20 table (nothing / gold / gems / both / cave-in death) each costing a fight; fairy's gem-for-`e_rand(1,7)`-boon; glowingstream's 1–10 drink (death, near-death, heals, gem, full heal); crazyaudrey's three-basket match (5/2/1 fights or lose one); foilwench's gem-for-skill. Events fire *before* the turn is spent (goldmine spends it as its own effect). `darkhorse` reduced to a rest — its PvP-intel / dice / comment systems don't exist single-player. **The modules are the stock core set; the live LoGD's "hundreds of events" were separately-licensed DragonPrime add-ons we can't and don't copy.**
- **Specialties** (`specialty.rs` + buff engine): the three classes and their four skills each, ported 1=1 — Mystical (Mending Flow/regen, Stonefist/minion, Lifedrink/lifetap, Stormskin/damage-shield), Dark Arts (Bonecall/minions, Effigy/big hit, Hexweight/`badguydmgmod` 0.5, Soulwither/atk+def 0), Thief (Taunt/enemy-atk 0.5, Venom Edge/atk ×2, Vanish/enemy-atk 0, Shadowstrike/atk+def ×3). Use-economy matches LoGD (`floor(skill/3)`+1/day, +1 per 3 skill). Chosen once via `Mode::ChooseSpecialty`; advanced by gems at the fairy/foilwench. **Specialty perk-modules' mechanics are in core (uncopyrightable); our names/flavor are original.**

## What's missing vs. the original

Everything we *have* now matches LoGD; what's below is **not built yet**. Documented so these stop surfacing as surprises.

- **Village daily news/log.** Forest *events* now exist (`events.rs`), but the village still has no **daily news feed** (LoGD's `addnews`/`news.php` — "yesterday in Duskmere"). The events module is the natural place to surface it next.
- **Creature runtime scaling (`buffbadguy`, `lib/forestoutcomes.php`):** we use the **static** per-level creature stat blocks. LoGD additionally perturbs each creature by the player's investment (+0.05 strength per 100 dragonkills) and a random stat/exp flux. We deliberately kept creatures static; the dragon+master scaling already restores the endgame treadmill, but the forest itself doesn't get harder as you invest. *(Simplification, not a bug.)*
- **New-day "spirits"** (`newday.php`): LoGD nudges the day's forest turns by a random −2..+2. We grant a flat `TURNS_PER_DAY` + Stamina bonus. Omitted because unexplained turn variance reads as a bug without the accompanying flavor message.
- **No dashboard activity feed** (`activity_game()` returns `None`) and **no chip/profile award** for slaying the dragon — `svc` holds the deps but the reward path isn't wired (needs a `reward_templates` seed migration like Lateania's `086`).
- **Whole locations/systems not built:** PvP, the Stables (mounts), the Gardens, the Inn/bar social loop (Violet, marriage), the King's tournament, mail. Out of scope for the single-player core.
- **Soulpoints:** we don't track soulpoints (the alignment/resurrection currency). The **specialty** system and the combat **buff engine** now exist (see above); what remains unbuilt there is non-specialty buff *sources* (potions, drink buffs from the tavern, enemy-inflicted debuffs) and the specialty-gated newday/dragonkill flavor.

## Next: toward multiplayer (kickoff notes)

The single-player core is faithful and complete. The next phase is the **social/multiplayer layer** — the tavern, dice, gossip, PvP, the bar. This is where late.sh's "everyone in the same SSH room" shape can shine. Same licensing rule throughout: **mechanics/odds/payouts transcribed 1=1; all prose and names original; community add-on modules are off-limits, only the stock-core systems.**

**The one architectural shift.** Today `state.rs` is authoritative-per-session ("the session owns the truth", no shared world). Multiplayer means a **shared world**: reading *other* players' stored characters, and cross-player writes. `svc` already brokers DB access — extend it with a "load other characters" / "online roster" path. Nothing else in the single-player core has to change.

**Two foundational primitives unlock almost everything:**
1. **`commentary` (the gossip/chat primitive).** In LoGD (`lib/commentary.php`) one shared table, keyed by a `section` string ("village", "inn", "darkhorse", clan halls), powers *all* chat. Build this once and gossip, the tavern board, the inn, and clan halls all fall out. New table `greendragon_commentary` (section, author, body, timestamp), an `addcommentary`/`viewcommentary` pair, and a `Mode` that renders a section + a talk line. **This is the single highest-leverage piece — build it first.**
2. **PvP resolution** (`pvp.php`): attack another player's stored character with the *existing* `combat`/buff engine; on a win, take a slice of their on-hand gold and their place on the slay list. No new combat code — just target selection (the online roster) and the reward/notify path.

**Features, each mapping to a stock-core original file:**
- **The Dark Horse Tavern, restored** (`darkhorse.php`) — the natural first multiplayer surface; it already exists as a stub (`events::Tavern`). Three pieces, all reusing the primitives above: the **comment board** (`commentary`), the **dice gambling minigame** (`game_dice` / `game_fivesix` / `game_stones`, hooking `darkhorsegame` — transcribe odds/payouts), and **enemy intel** (read the online roster). Reducing it was the one non-1=1 compromise in the core; this is where it gets paid back.
- **The Inn / bar** (`inn.php`) — the social loop: Violet (flirt/marriage), the bard, and **drinks that grant buffs** — the buff engine already supports temporary buffs, so tavern/inn drinks are the first non-specialty buff *source*.
- **Daily news / gossip feed** (`news.php`, `addnews`) — "yesterday in Duskmere": dragon kills, deaths, PvP results. Pairs with `commentary`; the `events.rs` outcomes are already the event stream to draw from.
- **Mail** (`mail.php`), **Clans** (`clan.php`), the **King's tournament / jousting**, the **Stables/mounts** (goldmine's dropped mount rolls reconnect here) — later, each on the same two primitives.

**Suggested first slice:** `commentary` → the tavern board → dice → PvP roster. That order builds the shared-world plumbing once and lights up the most-requested rooms (tavern, gossip, dice) before the heavier systems (clans, tournament).
