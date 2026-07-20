//! The Legend of the Green Dragon combat engine: one self-contained,
//! deterministic-with-a-seed round resolver. Mirrors LoGD's `rolldamage`
//! (`lib/battle-skills.php`) faithfully, including its quirks.
//!
//! Each round both sides roll a "bell" value (see [`bell_rand`]) against the
//! relevant stat and subtract the opponent's defensive roll. Crucially these
//! rolls can land *negative* or *overshoot* the stat, so a blow can glance (and
//! a glancing blow actually heals the target — `damage` here is signed and a
//! negative value restores the target's HP, exactly as upstream). A 1-in-20
//! player crit triples the attack stat before rolling (PvE only), and an
//! attack roll that exceeds the player's attack stat triggers a power move that
//! adds bonus damage. The round rerolls until at least one side lands a nonzero
//! hit, so fights always progress.
//!
//! Kept pure: callers pass an `&mut impl Rng`, so tests seed an RNG and assert
//! exact outcomes. How a character's `attack`/`defense` are derived from
//! equipped gear lives on the character model, not here.

use rand::Rng;
use serde::{Deserialize, Serialize};

/// A combatant reduced to the two numbers the round resolver needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combatant {
    pub attack: u32,
    pub defense: u32,
}

/// What a companion does each round beyond existing (the `abilities` blob on
/// LoGD's companion rows). Fighters and defenders swing at the foe; a healer
/// only bandages — upstream rolls its attack but never applies the damage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompanionAbility {
    /// Strikes the foe each round (the stock fighter).
    #[default]
    Fight,
    /// Guards the player: the foe's companion-lash lands on a defender first.
    Defend,
    /// Restores up to this many HP a round to the most wounded ally — the
    /// player first, then other companions, then itself (the field-medic).
    Heal(u32),
}

/// A persistent ally that fights alongside the player (LoGD `apply_companion`).
/// Summoned by skills like Bonecall or hired at the mercenary camp, it
/// persists across fights until its HP reaches zero. Stored on the character,
/// so it is serde-able.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Companion {
    pub name: String,
    pub hitpoints: u32,
    pub max_hitpoints: u32,
    /// Float, as upstream: Bonecall's skeleton stores stats ending in .5 and
    /// the engine consumes them un-rounded.
    pub attack: f64,
    pub defense: f64,
    /// Per-level growth (the companions table's `*perlevel` columns): added
    /// on every master victory (`train.php` `companionslevelup` default 1).
    /// Zero for summons, as upstream's skeleton carries no perlevel keys.
    #[serde(default)]
    pub attack_per_level: u32,
    #[serde(default)]
    pub defense_per_level: u32,
    #[serde(default)]
    pub hp_per_level: u32,
    /// Flavor logged the round the companion is destroyed.
    pub dying_text: String,
    /// What it does each round beyond the basic strike.
    #[serde(default)]
    pub ability: CompanionAbility,
    /// Doesn't count against the one-hire cap (LoGD `ignorelimit`) — true
    /// for summons like Bonecall's skeleton, false for hires. Old saves hold
    /// only summons, so the default leans true.
    #[serde(default = "default_true")]
    pub ignore_limit: bool,
}

fn default_true() -> bool {
    true
}

/// A landed power move (LoGD `report_power_move`): an attack roll that beat the
/// player's attack stat by a growing margin, each tier adding bonus damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerMove {
    /// Roll > 1.5x attack stat.
    Minor,
    /// Roll > 2x.
    Power,
    /// Roll > 3x.
    Double,
    /// Roll > 4x.
    Mega,
}

impl PowerMove {
    /// Flavor for the round it lands.
    pub fn label(self) -> &'static str {
        match self {
            PowerMove::Minor => "A minor power move!",
            PowerMove::Power => "A power move!",
            PowerMove::Double => "A DOUBLE power move!!",
            PowerMove::Mega => "A MEGA power move!!!",
        }
    }
}

/// The result of one resolved round. Damage is **signed**: a negative value
/// means a glancing blow that *heals* the target (mirroring LoGD, where a
/// negative `creaturedmg` is subtracted from `creaturehealth`, i.e. added).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundOutcome {
    /// Damage the player deals to the enemy (negative heals the enemy).
    pub damage_to_enemy: i32,
    /// Damage the enemy deals to the player (negative heals the player).
    pub damage_to_player: i32,
    /// Whether the player landed the 1-in-20 triple-attack crit this round.
    pub player_crit: bool,
    /// The power move the player landed this round, if any.
    pub power_move: Option<PowerMove>,
}

// --- bell_rand: LoGD's normal-curve roll ------------------------------------

/// Low/high z bounds of LoGD's 441-entry `bell_rand` percentile table: the
/// standard normal recentred so the 5th percentile maps to 0.0 and the 95th to
/// 1.0, with the table's extreme tails capping z here.
const Z_MIN: f64 = -0.716599;
const Z_MAX: f64 = 1.712548831;
/// Maps a standard-normal z onto the recentred scale: `2 * 1.6449`, since the
/// std-normal 5th/95th percentiles are ∓1.6449 and the table places them at
/// 0.0/1.0 (a unit apart, centred on 0.5).
const Z_SCALE: f64 = 3.2897;

/// Acklam's rational approximation of the inverse standard-normal CDF, accurate
/// to ~1e-9 — the continuous form of LoGD's tabulated percentile→z lookup.
fn inv_norm(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969683028665376e+01,
        2.209460984245205e+02,
        -2.759285104469687e+02,
        1.38357751867269e+02,
        -3.066479806614716e+01,
        2.506628277459239e+00,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-03,
        -3.223964580411365e-01,
        -2.400758277161838e+00,
        -2.549732539343734e+00,
        4.374664141464968e+00,
        2.938163982698783e+00,
    ];
    const D: [f64; 4] = [
        7.784695709041462e-03,
        3.224671290700398e-01,
        2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    const PLOW: f64 = 0.02425;
    const PHIGH: f64 = 1.0 - PLOW;
    if p < PLOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= PHIGH {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    }
}

/// LoGD's `bell_rand(0, max)`: a normal-curve roll. Upstream samples
/// `mt_rand(0, 100000)` into a percentile→z table and returns `z * max`, where z
/// runs from ~-0.72 (low tail) through ~0.498 (median) to ~1.71 (high tail). We
/// reproduce that continuously via the inverse-normal CDF — which is exactly
/// what the table tabulates. **The result can be negative or exceed `max`**: the
/// long tails are load-bearing (they drive glancing hits and power moves).
pub fn bell_rand(rng: &mut impl Rng, max: f64) -> f64 {
    if max <= 0.0 {
        return 0.0;
    }
    // Match the table's percentile sampling, clamped to its 3..=99997 key range.
    let r = rng.gen_range(0u32..=100_000) as f64 / 100_000.0;
    let p = r.clamp(0.00003, 0.99997);
    let z = (0.5 + inv_norm(p) / Z_SCALE).clamp(Z_MIN, Z_MAX);
    z * max
}

/// PHP `(int)` truncation toward zero.
fn trunc(x: f64) -> i32 {
    x.trunc() as i32
}

/// PHP `round()` (half away from zero).
fn iround(x: f64) -> i32 {
    x.round() as i32
}

/// The folded per-round multipliers `rolldamage` reads from the buff set. All
/// default to neutral (1.0 / false).
#[derive(Clone, Copy, Debug)]
struct Mods {
    /// Player attack multiplier (`atkmod`).
    atkmod: f64,
    /// Player defense multiplier (`defmod`).
    defmod: f64,
    /// Enemy attack multiplier (`creatureatkmod`).
    badguyatkmod: f64,
    /// Enemy defense multiplier (`creaturedefmod`).
    badguydefmod: f64,
    /// Player outgoing-damage multiplier applied to the *final* damage (`dmgmod`).
    dmgmod: f64,
    /// Enemy outgoing-damage multiplier (`badguydmgmod`).
    badguydmgmod: f64,
    /// Difficulty knob folded into both defenses (`adjustment`, default 1.0).
    adjustment: f64,
    /// Forces damage dealt positive / damage taken non-positive (`invulnerable`).
    invulnerable: bool,
}

impl Default for Mods {
    fn default() -> Self {
        Mods {
            atkmod: 1.0,
            defmod: 1.0,
            badguyatkmod: 1.0,
            badguydefmod: 1.0,
            dmgmod: 1.0,
            badguydmgmod: 1.0,
            adjustment: 1.0,
            invulnerable: false,
        }
    }
}

/// The core `rolldamage`: returns `(creaturedmg, selfdmg, crit, player_atk_roll)`.
/// Both damages are signed and rerolled until at least one is nonzero. Mirrors
/// `lib/battle-skills.php` line for line: a negative result is halved and kept
/// negative (a glancing blow / heal), positive and negative branches multiply by
/// `dmgmod`/`badguydmgmod` in the upstream order.
fn roll_damage(
    rng: &mut impl Rng,
    player: Combatant,
    enemy: Combatant,
    m: Mods,
) -> (i32, i32, bool, f64) {
    roll_damage_raw(rng, player.attack as f64, player.defense as f64, enemy, m)
}

/// The core `rolldamage` over raw f64 attack/defense stats, so a fractional
/// combatant (a skeleton companion's `.5` block) rolls at full precision. The
/// player path passes its integer stats straight through [`roll_damage`];
/// companions call this directly (`rollcompaniondamage`, `lib/extended-battle.php`).
fn roll_damage_raw(
    rng: &mut impl Rng,
    atk_stat: f64,
    def_stat: f64,
    enemy: Combatant,
    m: Mods,
) -> (i32, i32, bool, f64) {
    let adjusted_creature_def =
        m.badguydefmod * enemy.defense as f64 / (m.adjustment * m.adjustment);
    let creature_attack = enemy.attack as f64 * m.badguyatkmod;
    let adjusted_self_def = def_stat * m.adjustment * m.defmod;

    let mut creaturedmg;
    let mut selfdmg;
    let mut crit;
    let mut patkroll;
    loop {
        let mut atk = atk_stat * m.atkmod;
        crit = rng.gen_range(1..=20) == 1;
        if crit {
            atk *= 3.0;
        }
        patkroll = bell_rand(rng, atk);
        let catkroll = bell_rand(rng, adjusted_creature_def);

        let mut cd = -trunc(catkroll - patkroll);
        if cd < 0 {
            cd = trunc(cd as f64 / 2.0);
            cd = iround(m.badguydmgmod * cd as f64);
        } else if cd > 0 {
            cd = iround(m.dmgmod * cd as f64);
        }

        let pdefroll = bell_rand(rng, adjusted_self_def);
        let catkroll2 = bell_rand(rng, creature_attack);

        let mut sd = -trunc(pdefroll - catkroll2);
        if sd < 0 {
            sd = trunc(sd as f64 / 2.0);
            sd = iround(sd as f64 * m.dmgmod);
        } else if sd > 0 {
            sd = iround(sd as f64 * m.badguydmgmod);
        }

        creaturedmg = cd;
        selfdmg = sd;
        if !(creaturedmg == 0 && selfdmg == 0) {
            break;
        }
    }
    if m.invulnerable {
        creaturedmg = creaturedmg.abs();
        selfdmg = -selfdmg.abs();
    }
    (creaturedmg, selfdmg, crit, patkroll)
}

/// Apply LoGD `report_power_move`: when the player's attack roll exceeds their
/// attack stat by a tier margin, add `e_rand(roll/4, roll/2)` damage (min 1).
fn apply_power_move(
    rng: &mut impl Rng,
    patkroll: f64,
    base_atk: u32,
    dmg: i32,
) -> (i32, Option<PowerMove>) {
    let uatk = base_atk as f64;
    let tier = if patkroll > uatk * 4.0 {
        Some(PowerMove::Mega)
    } else if patkroll > uatk * 3.0 {
        Some(PowerMove::Double)
    } else if patkroll > uatk * 2.0 {
        Some(PowerMove::Power)
    } else if patkroll > uatk * 1.5 {
        Some(PowerMove::Minor)
    } else {
        None
    };
    match tier {
        Some(t) => {
            // e_rand rounds its bounds (lib/e_rand.php), not truncates.
            let lo = iround(patkroll / 4.0);
            let hi = iround(patkroll / 2.0);
            let bonus = if hi > lo { rng.gen_range(lo..=hi) } else { lo };
            ((dmg + bonus).max(1), Some(t))
        }
        None => (dmg, None),
    }
}

/// Resolve one PvE combat round between the player and an enemy, no buffs.
pub fn resolve_round(rng: &mut impl Rng, player: Combatant, enemy: Combatant) -> RoundOutcome {
    let (cd, sd, crit, patkroll) = roll_damage(rng, player, enemy, Mods::default());
    let (cd, power) = apply_power_move(rng, patkroll, player.attack, cd);
    RoundOutcome {
        damage_to_enemy: cd,
        damage_to_player: sd,
        player_crit: crit,
        power_move: power,
    }
}

/// An active combat buff: a bundle of per-round modifiers mirroring the fields
/// LoGD's `apply_buff` understands. Every specialty skill compiles down to one
/// of these. Defaults are no-ops (1.0 multipliers, zero flats) so a skill sets
/// only the fields it actually changes — build one with [`Buff::new`].
#[derive(Clone, Debug, PartialEq)]
pub struct Buff {
    pub name: String,
    /// Rounds left before the buff wears off. Decremented after each round.
    pub rounds_left: u32,
    /// Multiplier on the player's attack stat (`atkmod`).
    pub player_atk_mod: f32,
    /// Multiplier on the player's defense stat (`defmod`).
    pub player_def_mod: f32,
    /// Multiplier on the enemy's attack stat (`badguyatkmod`).
    pub enemy_atk_mod: f32,
    /// Multiplier on the enemy's defense stat (`badguydefmod`).
    pub enemy_def_mod: f32,
    /// Multiplier on damage the enemy actually deals this round (`badguydmgmod`).
    pub enemy_dmg_mod: f32,
    /// Multiplier on the player's *outgoing* damage (`dmgmod`).
    pub player_dmg_mod: f32,
    /// Flat HP healed to the player each round (`regen`).
    pub regen: u32,
    /// If set, `regen` also heals the player's companions by `regen/3` (`aura`).
    pub aura: bool,
    /// Heal as a fraction of damage dealt to the enemy this round (`lifetap`).
    pub lifetap: f32,
    /// Extra hits on the enemy each round (`minioncount`), each rolling
    /// `minion_min..=minion_max` damage.
    pub minion_count: u32,
    pub minion_min: u32,
    pub minion_max: u32,
    /// Reflect this fraction of damage received back at the enemy (`damageshield`).
    pub damage_shield: f32,
    /// Forces outgoing damage positive and incoming non-positive (`invulnerable`).
    pub invulnerable: bool,
    /// Flavor shown while the buff is active.
    pub round_msg: Option<String>,
    /// Flavor shown the round it wears off.
    pub wearoff: String,
}

impl Buff {
    /// A no-op buff of `name` lasting `rounds`. Callers set the fields the skill
    /// changes; everything else stays neutral.
    pub fn new(name: impl Into<String>, rounds: u32) -> Self {
        Buff {
            name: name.into(),
            rounds_left: rounds,
            player_atk_mod: 1.0,
            player_def_mod: 1.0,
            enemy_atk_mod: 1.0,
            enemy_def_mod: 1.0,
            enemy_dmg_mod: 1.0,
            player_dmg_mod: 1.0,
            regen: 0,
            aura: false,
            lifetap: 0.0,
            minion_count: 0,
            minion_min: 0,
            minion_max: 0,
            damage_shield: 0.0,
            invulnerable: false,
            round_msg: None,
            wearoff: String::new(),
        }
    }
}

/// A round resolved with active buffs and companions folded in: the (signed)
/// damages, the heal the player gained, and any buff/companion flavor.
#[derive(Clone, Debug, PartialEq)]
pub struct BuffedOutcome {
    pub damage_to_enemy: i32,
    pub damage_to_player: i32,
    pub player_crit: bool,
    pub power_move: Option<PowerMove>,
    /// Total HP restored to the player this round (regen + lifetap).
    pub player_heal: u32,
    /// Buff/companion flavor to log this round.
    pub messages: Vec<String>,
}

/// Resolve one round with `buffs` and `companions` applied: stat multipliers
/// adjust the combat roll, then post-round effects (regen/lifetap heals, minion
/// hits, the lightning damage-shield, companion attacks) layer on. `enemy_hp`
/// is the foe's health entering the round, so companions neither pile onto a
/// corpse nor get struck by a foe an earlier blow already felled (upstream
/// gates the companion loop on `creaturehealth`). Each fighting companion
/// trades blows with the foe in its own paired exchange and can be struck down
/// (dead ones are removed, their dying flavor collected). Buffs tick down and
/// expired ones are removed. Mirrors how LoGD threads buff/companion hooks
/// through `rolldamage`/`rollcompaniondamage`.
pub fn resolve_round_buffed(
    rng: &mut impl Rng,
    player: Combatant,
    enemy: Combatant,
    enemy_hp: u32,
    buffs: &mut Vec<Buff>,
    companions: &mut Vec<Companion>,
) -> BuffedOutcome {
    let mut m = Mods::default();
    for b in buffs.iter() {
        m.atkmod *= b.player_atk_mod as f64;
        m.defmod *= b.player_def_mod as f64;
        m.badguyatkmod *= b.enemy_atk_mod as f64;
        m.badguydefmod *= b.enemy_def_mod as f64;
        m.badguydmgmod *= b.enemy_dmg_mod as f64;
        m.dmgmod *= b.player_dmg_mod as f64;
        if b.invulnerable {
            m.invulnerable = true;
        }
    }

    let (cd, sd, crit, patkroll) = roll_damage(rng, player, enemy, m);
    let (mut damage_to_enemy, power) = apply_power_move(rng, patkroll, player.attack, cd);
    let damage_to_player = sd;

    let mut heal = 0u32;
    let mut messages = Vec::new();

    // Companions join the fray. Each living companion trades blows with the
    // foe in its own paired exchange, exactly as upstream's
    // `report_companion_move`/`rollcompaniondamage` (`lib/extended-battle.php`):
    // it swings (a negative roll rebounds on itself, the foe's riposte), and —
    // only while the foe still stands — the foe swings back at *that* companion
    // (a negative return is the companion turning the blow into the foe). Every
    // fighting companion is answered, not just one. The companion-specific mods
    // (`compatkmod`/`compdefmod`/`compdmgmod`) are 1.0 for every stock
    // companion, so the shared roller with the player mods neutralised and the
    // enemy mods kept reproduces the roll. A healer never lands its own swing
    // (upstream's heal branch discards `creaturedmg`) but is still in reach of
    // the foe. Upstream's `defend` only suppresses the foe's bonus double-attack
    // — which we don't model — so it collapses to an ordinary fighter here.
    let comp_mods = Mods {
        atkmod: 1.0,
        defmod: 1.0,
        dmgmod: 1.0,
        badguyatkmod: m.badguyatkmod,
        badguydefmod: m.badguydefmod,
        badguydmgmod: m.badguydmgmod,
        adjustment: m.adjustment,
        invulnerable: false,
    };
    let mut foe_hp_running = enemy_hp as i32 - damage_to_enemy;
    for comp in companions.iter_mut() {
        if foe_hp_running <= 0 {
            break;
        }
        if comp.hitpoints == 0 {
            continue;
        }
        let is_heal = matches!(comp.ability, CompanionAbility::Heal(_));
        let (cdmg, sdmg, _crit, _roll) =
            roll_damage_raw(rng, comp.attack, comp.defense, enemy, comp_mods);
        // The companion's swing (healers never apply theirs).
        if !is_heal {
            if cdmg > 0 {
                damage_to_enemy += cdmg;
                foe_hp_running -= cdmg;
                messages.push(format!("{} strikes your foe for {cdmg}.", comp.name));
            } else if cdmg < 0 {
                comp.hitpoints = comp.hitpoints.saturating_sub((-cdmg) as u32);
                messages.push(format!(
                    "{} overreaches; the foe ripostes for {}.",
                    comp.name, -cdmg
                ));
            }
        }
        // The foe answers this companion, but only if it survived the swing.
        if foe_hp_running >= 0 {
            if sdmg > 0 {
                comp.hitpoints = comp.hitpoints.saturating_sub(sdmg as u32);
            } else if sdmg < 0 {
                damage_to_enemy += -sdmg;
                foe_hp_running -= -sdmg;
                messages.push(format!(
                    "{} turns the blow aside and gores your foe for {}.",
                    comp.name, -sdmg
                ));
            }
        }
        if comp.hitpoints == 0 {
            messages.push(comp.dying_text.clone());
        }
    }

    // Each aura buff heals living companions by round(its own regen / 3)
    // (`lib/battle-buffs.php`: `(int)round($buff['regen']/3)`).
    for b in buffs.iter() {
        if !b.aura {
            continue;
        }
        let aura = iround(b.regen as f64 / 3.0);
        if aura <= 0 {
            continue;
        }
        for comp in companions.iter_mut() {
            if comp.hitpoints > 0 {
                comp.hitpoints = (comp.hitpoints + aura as u32).min(comp.max_hitpoints);
            }
        }
    }
    companions.retain(|c| c.hitpoints > 0);

    for b in buffs.iter() {
        heal += b.regen;
        if b.lifetap > 0.0 && damage_to_enemy > 0 {
            heal += (damage_to_enemy as f32 * b.lifetap).round() as u32;
        }
        if b.damage_shield > 0.0 && damage_to_player > 0 {
            damage_to_enemy += (damage_to_player as f32 * b.damage_shield).round() as i32;
        }
        for _ in 0..b.minion_count {
            let hi = b.minion_max.max(b.minion_min);
            damage_to_enemy += rng.gen_range(b.minion_min..=hi) as i32;
        }
        if let Some(msg) = &b.round_msg {
            messages.push(msg.clone());
        }
    }

    for b in buffs.iter_mut() {
        b.rounds_left = b.rounds_left.saturating_sub(1);
    }
    let mut i = 0;
    while i < buffs.len() {
        if buffs[i].rounds_left == 0 {
            let expired = buffs.remove(i);
            if !expired.wearoff.is_empty() {
                messages.push(expired.wearoff);
            }
        } else {
            i += 1;
        }
    }

    BuffedOutcome {
        damage_to_enemy,
        damage_to_player,
        player_crit: crit,
        power_move: power,
        player_heal: heal,
        messages,
    }
}

/// One extra foe's strike on the player — the multi-fight case where the
/// player attacks only their target while every other living foe still gets
/// its round (and the failed-flee free round). Mirrors the incoming-damage
/// half of `rolldamage` with the active buff multipliers folded in: signed,
/// negative = the blow glanced (heals the player).
pub fn resolve_extra_foe_strike(
    rng: &mut impl Rng,
    player: Combatant,
    foe: Combatant,
    buffs: &[Buff],
) -> i32 {
    let mut m = Mods::default();
    for b in buffs.iter() {
        m.defmod *= b.player_def_mod as f64;
        m.badguyatkmod *= b.enemy_atk_mod as f64;
        m.badguydmgmod *= b.enemy_dmg_mod as f64;
        m.dmgmod *= b.player_dmg_mod as f64;
        if b.invulnerable {
            m.invulnerable = true;
        }
    }
    let adjusted_self_def = player.defense as f64 * m.adjustment * m.defmod;
    let foe_attack = foe.attack as f64 * m.badguyatkmod;
    let pdefroll = bell_rand(rng, adjusted_self_def);
    let fatkroll = bell_rand(rng, foe_attack);
    let mut sd = -trunc(pdefroll - fatkroll);
    if sd < 0 {
        sd = trunc(sd as f64 / 2.0);
        sd = iround(sd as f64 * m.dmgmod);
    } else if sd > 0 {
        sd = iround(sd as f64 * m.badguydmgmod);
    }
    if m.invulnerable {
        sd = -sd.abs();
    }
    sd
}

/// How a fully simulated fight ended. Used by tests and balance checks; the
/// live game steps one [`resolve_round`] per player action instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FightResult {
    PlayerWon { rounds: u32, player_hp_left: u32 },
    PlayerLost { rounds: u32, enemy_hp_left: u32 },
}

/// Apply signed damage to a pool, clamping into `0..=max` (negative heals).
fn apply_damage(hp: u32, dmg: i32, max: u32) -> u32 {
    ((hp as i64 - dmg as i64).clamp(0, max as i64)) as u32
}

/// Simulate a fight to the death, round by round, player striking first each
/// round. Helper for tests and offline balance tuning.
pub fn simulate_fight(
    rng: &mut impl Rng,
    player: Combatant,
    player_max: u32,
    mut player_hp: u32,
    enemy: Combatant,
    enemy_max: u32,
    mut enemy_hp: u32,
) -> FightResult {
    let mut rounds = 0;
    loop {
        rounds += 1;
        let outcome = resolve_round(rng, player, enemy);
        enemy_hp = apply_damage(enemy_hp, outcome.damage_to_enemy, enemy_max);
        if enemy_hp == 0 {
            return FightResult::PlayerWon {
                rounds,
                player_hp_left: player_hp,
            };
        }
        player_hp = apply_damage(player_hp, outcome.damage_to_player, player_max);
        if player_hp == 0 {
            return FightResult::PlayerLost {
                rounds,
                enemy_hp_left: enemy_hp,
            };
        }
    }
}

#[cfg(test)]
#[path = "combat_test.rs"]
mod combat_test;

