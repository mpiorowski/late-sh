# Green Dragon door (`late-ssh/src/app/door/greendragon`)

A native, in-process door game: an open-source remake of LORD, modeled on
*Legend of the Green Dragon* (LoGD). Single-player, turn-based, DB-persisted
(one character per user). It uses the **Lateania integration pattern** (native
ratatui + a service + a `DoorGame` impl), not the nethack/rebels PTY-proxy
pattern, because LoGD is a web app with no terminal to proxy — only its balance
data and mechanics are reused.

## Upstream source of truth

Everything mechanical is transcribed from the **classic DragonPrime Edition**
of LoGD — the final content-complete release, **1.1.2** (DragonPrime
[ceased Sept 2019](https://dragonprime.net/index.php?topic=12736.msg106613)).
We compare against the GitHub mirror:

- **`jimlunsford/lotgd`** — <https://github.com/jimlunsford/lotgd> (raw:
  `https://raw.githubusercontent.com/jimlunsford/lotgd/master/<path>`). Self-described
  "DragonPrime Edition"; its creature/master/exp/weapon seed tables verified 1=1
  against [`data.rs`](data.rs). Key files we ported: `lib/battle-skills.php`
  (`rolldamage`), `lib/bell_rand.php`, `dragon.php`, `train.php`, `newday.php`,
  `bank.php`, `healer.php`, `lib/forestoutcomes.php`, `lib/experience.php`,
  `modules/specialty{mysticpower,darkarts,thiefskills}.php`, and the 8 forest
  event modules.

**Not** the source: the newer **`lotgd/core`** ("Daenerys") rewrite —
<https://github.com/lotgd/core>. It's a headless, **content-empty** engine
(no forest/dragon/masters/specialties), last real release v0.5.0 (Apr 2019),
**archived Jan 2026**. Newer architecture, but a dead shell — nothing to port.

Original LORD lineage for reference: [Wikipedia — Legend of the Red
Dragon](https://en.wikipedia.org/wiki/Legend_of_the_Red_Dragon). LoGD project
hub: [dragonprime.net](https://dragonprime.net/).

## Module map (flat)

| File | Owns |
|---|---|
| `data.rs` | Balance tables + flavor. **Numbers** (weapon/armor cost ladder, per-level creature stat blocks 1–16, exp curve with dragonkill scaling, master/dragon stats `45/25/300`) are transcribed from the LoGD balance (`jimlunsford/lotgd`); mechanics/numbers aren't copyrightable. **Names** (creatures, masters, `WEAPON_NAMES`/`ARMOR_NAMES`) are *original to late.sh* — the seed is CC BY-NC-SA, whose NC+SA terms conflict with late.sh, so we wrote our own. Pure constants + lookups (`weapon_name`/`armor_name`). |
| `combat.rs` | The pure round resolver mirroring LoGD `rolldamage` faithfully, including its quirks. `bell_rand` is the **normal-curve** roll (an inverse-normal-CDF reproduction of LoGD's 441-entry percentile→z table): it can return **negative or overshoot** the stat, and **damage is signed** — a glancing blow (negative) *heals* the target, exactly as upstream. 1-in-20 triple-crit, `dmgmod`/`badguydmgmod` damage stages, **power moves** (`report_power_move`: roll > 1.5/2/3/4× attack adds bonus damage), reroll-until-progress, `invulnerable`. Plus `simulate_fight`. The **buff engine** (`Buff` + `resolve_round_buffed`) mirrors `apply_buff` fields (atk/def/enemy-atk/def/dmg/`dmgmod` multipliers, regen, `aura`, lifetap, minions, damage-shield, rounds), and the **companion engine** (`Companion`): persistent allies that strike the foe and can be struck down. |
| `specialty.rs` | The twelve specialty combat skills (Mystical / Dark Arts / Thief, 4 each), ported **1=1** from LoGD's `specialtymysticpower`/`specialtydarkarts`/`specialtythiefskills` modules. Each is a use-cost + a `SkillEffect` factory scaled by level/attack: usually a `combat::Buff`, but **Bonecall** takes the companions-enabled path and `Summon`s a persistent stat-blocked skeleton (`apply_companion`); Mending Flow carries `aura`. **Mechanics transcribed (uncopyrightable); names + flavor original to late.sh.** |
| `events.rs` | The eight stock forest special events (findgold, findgem, goldmine, fairy, glowingstream, crazyaudrey/baskets, foilwench, darkhorse/tavern). A 15% pre-combat roll (`forestchance`), even-weighted. Each has framing prose + an optional accept/decline choice + an effect resolver (gold/gems/turns/heals/skill, plus two death paths). **Effect numbers transcribed 1=1; all prose original; no module text copied.** `darkhorse` is reduced to a rest (no PvP intel / dice / comments), `glowingstream` keeps its 1–10 table. |
| `model.rs` | The persistent `Character` and all rules on it: stat derivation (`max_hp = 10*level + dragon_hp_bonus`, `attack = level + weapon_tier + dragon_attack_bonus`, `defense = level + armor_tier + dragon_defense_bonus`), leveling (+5 soulpoints/master), shop pricing with 75% trade-in, healer cost (`round(ln(level)*(missing+10))`), banking + new-day interest (`apply_new_day_interest`, gated like LoGD), forest death (gold→0, exp×0.9), new-day reset (with `spirits` ±2 jitter + `RESURRECTION_TURNS` -6 after a death, and `soulpoints = 50 + 5*level`). **Dragon-kill** (`slay_dragon(flawless)`): retains on-hand gold + `50*kills` (cap 300), `max(0,kills-7)` gems, charm +5, auto-applied permanent attack/defense/HP boons (LoGD's retained dragon points — no Gypsy shop), flawless +150g/+1gem, specialty skill/uses restart. `scaled_dragon`/`scaled_master` grow those foes with your banked investment so the boons never trivialize them. Serde-able with field defaults (new fields default to 0/empty, so old saves load clean). Also the **specialty economy**: a `Specialty` (None/Mystical/DarkArts/Thief) plus `gems`, `specialty_skill`, `specialty_uses`; `charm`/`soulpoints` (tracked for parity); and `companions`. `choose_specialty`/`increment_specialty` (+1 skill, +1 use per 3), `refresh_specialty_uses` on new-day (`floor(skill/3)` + 1 for your path), `spend_specialty_uses` for casting. |
| `persist.rs` | JSON save envelope (`schema_version` + `character`), tolerant of missing fields. |
| `svc.rs` | `GreenDragonService` (cheap `Clone`, `Arc`-backed): async character load via a `watch` channel, fire-and-forget save/delete over `greendragon_characters`. Holds `ActivityPublisher`/`ChipService` for the not-yet-wired dragon-kill reward. |
| `state.rs` | Per-session `State`: owns the authoritative `Character` (single-player, no shared world), a `Mode` machine (Village/Forest/Fight/shops/Healer/Bank/Training/**Event**/**ChooseSpecialty**/Graveyard), the active `Encounter` (carries `buffs` + `took_damage`, the latter driving the flawless dragon bonus), the `pending_event`, a capped message log, and every player action as a method. The forest roll fires an event (15%) before spending a turn; fights step `resolve_round_buffed` (signed damage via `apply_signed`, companions threaded through); the fight menu lists castable specialty skills between Attack and Flee. Drains the load channel in `tick()`. Pure menu builders are unit-tested. |
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

These were checked against the actual LoGD PHP source (see [Upstream source of
truth](#upstream-source-of-truth) above — <https://github.com/jimlunsford/lotgd>),
not memory. Each names the upstream file it matches.

- **Combat** mirrors `rolldamage` (`lib/battle-skills.php`) faithfully: `bell_rand` is the normal-curve roll (inverse-normal-CDF reproduction of the 441-entry percentile→z table, so it can go negative / overshoot), **signed damage** where a glancing blow heals the target, 1-in-20 player triple-crit, the `dmgmod`/`badguydmgmod` damage stages, `report_power_move` bonus damage at 1.5/2/3/4× the attack stat, reroll-until-progress, and `invulnerable`. (Earlier this port used a clamped triangular roll with floored glancing hits — that was *not* faithful; fixed.)
- **Companions** (`apply_companion`): persistent allies stored on the character that strike the foe each round and can be struck down (and crumble). Bonecall summons the stat-blocked skeleton warrior; Mending Flow's `aura` heals them. Our one simplification: the foe makes a separate roll against a random companion each round (rather than LoGD's single-target redistribution), so companions don't soak the player's incoming hits.
- **Forest death** (`lib/forestoutcomes.php`): on-hand gold → 0, experience × 0.9 (`forestexploss` default 10%), bank untouched, sent to the graveyard until new-day. Matches exactly.
- **Forest hunt** (`forest.php`): slum / hunt / thrill shift the target creature level by **−1 / 0 / +1** (not ±2), plus a small random jitter (~1/3 of searches nudge ±1) layered at the call site in `start_forest_fight`. Thrillseeking pays +10% gold/exp.
- **Shop gating** is level-gated (`available_tiers` caps at `c.level`) so you can't grind gold to out-gear your rank — matches LoGD selling gear by level.
- **Healer cost** `round(ln(level)*(missing+10))` matches `healer.php` exactly (the optional `healmultiply` module hook is 1.0 on a stock install).
- **Bank interest** (`newday.php`): a **random 1–10% daily rate** applied on the new-day rollover, but only if ≤`FIGHTS_FOR_INTEREST` (4) turns were left unused and the balance is under `MAX_GOLD_FOR_INTEREST` (100k). RNG in `svc`, rule in `model::apply_new_day_interest`.
- **Master fight** (`train.php`): non-lethal — a loss **heals you to full** ("stays the final blow") and sends you home with no penalty; a win is +1 level, +10 max HP, +1 atk, +1 def, **+5 soulpoints**, full heal. The master scales with investment (`scaled_master`, factor 0.33).
- **Dragon kill** (`dragon.php`): on-hand gold is **retained** and topped up by `START_GOLD·kills` (cap 300), `max(0, kills-7)` gems accrue (cap 10), charm +5, and a flawless (no-damage) kill adds +150 gold (over the cap) and a gem. Permanent dragon-point boons are **auto-applied** across the reset (+1 atk, +1 def, +5 HP per kill — LoGD's retained `at`/`de` points + earned-HP retention) — there is no Gypsy shop. The specialty path is kept but its skill/uses restart at 0 (each module's `dragonkill` hook). The dragon itself **scales** with your banked investment (`scaled_dragon`, `round(investment·0.75)` points split into +atk/+def/+5HP), so the boons never trivialize it.
- **Forest events** (`events.rs`, the 8 stock forest modules): a 15% pre-combat roll (`forestchance`), even weight per module. Effect tables transcribed 1=1 — findgold `level·10..50`; goldmine's 1–20 table (nothing / gold / gems via `round(level/7)+1` & `round(level/3)+1` / both / cave-in death that still credits +10% exp) each costing a fight; fairy's gem-for-`e_rand(1,7)`-boon (a gemless accept costs a fight); glowingstream's 1–10 drink (death, near-death, full-heal+turn, gem, **turn-only** 5–7, default full heal); crazyaudrey's three-basket match (5/2/1 fights, or lose a fight — or a charm point when no fight is left); foilwench's gem-for-skill. `darkhorse` reduced to a rest — its PvP-intel / dice / comment systems don't exist single-player. **The modules are the stock core set; the live LoGD's "hundreds of events" were separately-licensed DragonPrime add-ons we can't and don't copy.**
- **Specialties** (`specialty.rs` + buff/companion engine): the three classes and their four skills each, ported 1=1 — Mystical (Mending Flow/regen+`aura`, Stonefist/minion, Lifedrink/lifetap, Stormskin/damage-shield), Dark Arts (**Bonecall/persistent skeleton companion**, Effigy/big hit, Hexweight/`badguydmgmod` 0.5, Soulwither/atk+def 0), Thief (Taunt/enemy-atk 0.5, Venom Edge/atk ×2, Vanish/enemy-atk 0, Shadowstrike/atk+def ×3). Use-economy matches LoGD (`floor(skill/3)`+1/day, +1 per 3 skill); skill/uses restart on dragon kill. Chosen once via `Mode::ChooseSpecialty`; advanced by gems at the fairy/foilwench. **Specialty perk-modules' mechanics are in core (uncopyrightable); our names/flavor are original.**

## What's missing vs. the original

Everything we *have* now matches LoGD; what's below is **not built yet**. Documented so these stop surfacing as surprises.

- **Charm & soulpoints are tracked but inert.** Both stats now exist on `Character` and update at the upstream points (charm +5/kill; soulpoints `50+5·level`/newday, +5/master), but nothing *consumes* them yet — they feed the not-yet-built social (flirting/marriage) and resurrection/alignment systems. Tracked now so those systems drop in cleanly.
- **Creature runtime scaling (`buffbadguy`, `lib/forestoutcomes.php`):** we use the **static** per-level creature stat blocks. LoGD additionally perturbs each creature by the player's investment (+0.05 strength per 100 dragonkills) and a random stat/exp flux. We deliberately kept creatures static; the dragon+master scaling already restores the endgame treadmill, but the forest itself doesn't get harder as you invest. *(Simplification, not a bug.)* The flawless-forest-fight turn refund (`forestvictory`) is likewise omitted.
- **Partial healing.** The healer only offers a full heal; LoGD also sells 10–100% partial heals. Minor convenience gap.
- **Village daily news/log.** Forest *events* now exist (`events.rs`), but the village still has no **daily news feed** (LoGD's `addnews`/`news.php` — "yesterday in Duskmere"). The events module is the natural place to surface it next.
- **No dashboard activity feed** (`activity_game()` returns `None`) and **no chip/profile award** for slaying the dragon — `svc` holds the deps but the reward path isn't wired (needs a `reward_templates` seed migration like Lateania's `086`).
- **The real Gypsy building** (`gypsy.php`) is *not* built. Note this is **not** the dragon-point shop a prior pass invented (now removed) — the actual stock gypsy is a fortune-teller you pay `level·20` gold to "talk with the dead" (see other players' graveyard/PvP records). It only becomes meaningful with the shared-world layer, so it belongs with the multiplayer phase below, not the single-player core.
- **Whole locations/systems not built:** PvP, the Stables (mounts), the Gardens, the Inn/bar social loop (Violet, marriage), the King's tournament, mail. Out of scope for the single-player core. Non-specialty buff *sources* (potions, tavern drinks, enemy debuffs) are likewise unbuilt, though the buff engine supports them.

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
- **The Gypsy** (`gypsy.php`) — the real fortune-teller: pay `level·20` gold to "talk with the dead" and read other players' graveyard/PvP history. Pure shared-world read, so it falls out of the online-roster path. (Distinct from the removed dragon-point shop.)
- **Mail** (`mail.php`), **Clans** (`clan.php`), the **King's tournament / jousting**, the **Stables/mounts** (goldmine's dropped mount rolls reconnect here) — later, each on the same two primitives.

**Suggested first slice:** `commentary` → the tavern board → dice → PvP roster. That order builds the shared-world plumbing once and lights up the most-requested rooms (tavern, gossip, dice) before the heavier systems (clans, tournament).
