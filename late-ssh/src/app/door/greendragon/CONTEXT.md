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
| `combat.rs` | The pure round resolver mirroring LoGD `rolldamage`: triangular `bell_rand`, 5% triple-crit, glancing hits floored to zero, reroll-until-progress. Takes `&mut impl Rng` so it's seed-testable. Plus `simulate_fight` for balance checks. |
| `model.rs` | The persistent `Character` and all rules on it: stat derivation (`max_hp = 10*level + dragon_hp_bonus`, `attack = level + weapon_tier + dragon_attack_bonus`, `defense = level + armor_tier + dragon_defense_bonus`), leveling, shop pricing with 75% trade-in, healer cost (`round(ln(level)*(missing+10))`), banking + new-day interest (`apply_new_day_interest`, gated like LoGD), forest death (gold→0, exp×0.9), new-day reset, dragon-kill run reset. A kill banks `DRAGON_POINTS_PER_KILL` dragon points; `buy_upgrade(GypsyUpgrade)` spends them on permanent across-run boons (Vitality/Might/Guard/Stamina) — LoGD's dragon-point economy. `scaled_dragon`/`scaled_master` grow those foes with your banked investment so boons never trivialize them. Serde-able with field defaults (new boon fields default to 0, so old saves load clean). |
| `persist.rs` | JSON save envelope (`schema_version` + `character`), tolerant of missing fields. |
| `svc.rs` | `GreenDragonService` (cheap `Clone`, `Arc`-backed): async character load via a `watch` channel, fire-and-forget save/delete over `greendragon_characters`. Holds `ActivityPublisher`/`ChipService` for the not-yet-wired dragon-kill reward. |
| `state.rs` | Per-session `State`: owns the authoritative `Character` (single-player, no shared world), a `Mode` machine (Village/Forest/Fight/shops/Healer/Bank/Training/Gypsy/Graveyard), the active `Encounter`, a capped message log, and every player action as a method. Drains the load channel in `tick()`. Pure menu builders are unit-tested. |
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

## What's missing vs. the original

Everything we *have* now matches LoGD; what's below is **not built yet**. Documented so these stop surfacing as surprises.

- **EVENTS — the top gap.** LoGD's forest is not just fights: ~1/3 of searches fire a **special/travel event** (the non-combat vignettes — find gold, a fairy heals you, a gambling stranger, the old man, deathtraps, flavor encounters), and the village has **daily news/log** events. We have *none* of these; the forest is fights-only. This is the single biggest thing that makes our forest feel thinner than the original, and it's the highest-priority addition. (Will need an events module: a weighted table of original-prose events with stat/gold/turn effects, rolled in `start_forest_fight` before falling through to combat.)
- **Creature runtime scaling (`buffbadguy`, `lib/forestoutcomes.php`):** we use the **static** per-level creature stat blocks. LoGD additionally perturbs each creature by the player's investment (+0.05 strength per 100 dragonkills) and a random stat/exp flux. We deliberately kept creatures static; the dragon+master scaling already restores the endgame treadmill, but the forest itself doesn't get harder as you invest. *(Simplification, not a bug.)*
- **New-day "spirits"** (`newday.php`): LoGD nudges the day's forest turns by a random −2..+2. We grant a flat `TURNS_PER_DAY` + Stamina bonus. Omitted because unexplained turn variance reads as a bug without the accompanying flavor message.
- **No dashboard activity feed** (`activity_game()` returns `None`) and **no chip/profile award** for slaying the dragon — `svc` holds the deps but the reward path isn't wired (needs a `reward_templates` seed migration like Lateania's `086`).
- **Whole locations/systems not built:** PvP, the Stables (mounts), the Gardens, the Inn/bar social loop (Violet, marriage), the King's tournament, mail. Out of scope for the single-player core.
- **Soulpoints / specialty / buffs:** we don't track soulpoints or the specialty (death-knight/etc.) system, and combat omits LoGD buff `dmgmod`s (we have no buff sources, so it's moot until potions/events land).
