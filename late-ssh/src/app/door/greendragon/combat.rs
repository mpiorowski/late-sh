//! The Legend of the Green Dragon combat engine: one self-contained,
//! deterministic-with-a-seed round resolver. Mirrors LoGD's `rolldamage`
//! (`lib/battle-skills.php`).
//!
//! Every round each side rolls a "bell" (triangular) value between 0 and its
//! relevant stat and subtracts the opponent's defensive roll. A 5% player crit
//! triples the player's attack for the round (PvE only). The round rerolls
//! until at least one side lands a nonzero hit, so fights always progress.
//!
//! Kept pure: callers pass an `&mut impl Rng`, so tests seed an RNG and assert
//! exact outcomes. How a character's `attack`/`defense` are derived from
//! equipped gear lives on the character model, not here.

use rand::Rng;

/// A combatant reduced to the two numbers the round resolver needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combatant {
    pub attack: u32,
    pub defense: u32,
}

/// The result of one resolved round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundOutcome {
    /// Damage the player deals to the enemy this round.
    pub damage_to_enemy: u32,
    /// Damage the enemy deals to the player this round.
    pub damage_to_player: u32,
    /// Whether the player landed the 5% triple-damage crit this round.
    pub player_crit: bool,
}

/// LoGD's `bell_rand(0, n)`: the average of two uniform rolls in `0..=n`, which
/// biases results toward the middle of the range (a triangular distribution)
/// instead of the flat spread a single roll would give.
pub fn bell_rand(rng: &mut impl Rng, n: u32) -> u32 {
    if n == 0 {
        return 0;
    }
    let a = rng.gen_range(0..=n);
    let b = rng.gen_range(0..=n);
    (a + b) / 2
}

/// 1-in-20 player crit chance, matching LoGD's `e_rand(1,20)==1`.
const CRIT_CHANCE_DENOM: u32 = 20;
/// Crit multiplier applied to player attack on a crit.
const CRIT_MULTIPLIER: u32 = 3;

/// Resolve one PvE combat round between the player and an enemy.
///
/// Damage to a target is `bell_rand(0, attacker_attack) - bell_rand(0,
/// target_defense)`, floored at zero (a blocked/glancing hit deals nothing).
/// If both sides deal zero, the round rerolls so the fight makes progress.
pub fn resolve_round(rng: &mut impl Rng, player: Combatant, enemy: Combatant) -> RoundOutcome {
    loop {
        let crit = rng.gen_range(1..=CRIT_CHANCE_DENOM) == 1;
        let player_attack = if crit {
            player.attack.saturating_mul(CRIT_MULTIPLIER)
        } else {
            player.attack
        };

        let to_enemy = bell_rand(rng, player_attack).saturating_sub(bell_rand(rng, enemy.defense));
        let to_player = bell_rand(rng, enemy.attack).saturating_sub(bell_rand(rng, player.defense));

        if to_enemy == 0 && to_player == 0 {
            continue;
        }
        return RoundOutcome {
            damage_to_enemy: to_enemy,
            damage_to_player: to_player,
            player_crit: crit,
        };
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
    /// Flat HP healed to the player each round (`regen`).
    pub regen: u32,
    /// Heal as a fraction of damage dealt to the enemy this round (`lifetap`).
    pub lifetap: f32,
    /// Extra hits on the enemy each round (`minioncount`), each rolling
    /// `minion_min..=minion_max` damage.
    pub minion_count: u32,
    pub minion_min: u32,
    pub minion_max: u32,
    /// Reflect this fraction of damage received back at the enemy (`damageshield`).
    pub damage_shield: f32,
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
            regen: 0,
            lifetap: 0.0,
            minion_count: 0,
            minion_min: 0,
            minion_max: 0,
            damage_shield: 0.0,
            round_msg: None,
            wearoff: String::new(),
        }
    }
}

/// A round resolved with active buffs folded in: the base outcome plus the heal
/// the player gained and any buff flavor (per-round messages and wear-offs).
#[derive(Clone, Debug, PartialEq)]
pub struct BuffedOutcome {
    pub damage_to_enemy: u32,
    pub damage_to_player: u32,
    pub player_crit: bool,
    /// Total HP restored to the player this round (regen + lifetap).
    pub player_heal: u32,
    /// Buff flavor to log this round (active round messages, then wear-offs).
    pub messages: Vec<String>,
}

fn scale(stat: u32, factor: f32) -> u32 {
    (stat as f32 * factor).round() as u32
}

/// Resolve one round with `buffs` applied: stat multipliers adjust the combat
/// roll, then post-round effects (enemy damage scaling, regen/lifetap heals,
/// minion hits, the lightning damage-shield) layer on. Buffs tick down and
/// expired ones are removed, their wear-off flavor collected. Mirrors how LoGD
/// threads buff hooks through `rolldamage`.
pub fn resolve_round_buffed(
    rng: &mut impl Rng,
    player: Combatant,
    enemy: Combatant,
    buffs: &mut Vec<Buff>,
) -> BuffedOutcome {
    let (mut p_atk, mut p_def) = (1.0_f32, 1.0_f32);
    let (mut e_atk, mut e_def, mut e_dmg) = (1.0_f32, 1.0_f32, 1.0_f32);
    for b in buffs.iter() {
        p_atk *= b.player_atk_mod;
        p_def *= b.player_def_mod;
        e_atk *= b.enemy_atk_mod;
        e_def *= b.enemy_def_mod;
        e_dmg *= b.enemy_dmg_mod;
    }
    let buffed_player = Combatant {
        attack: scale(player.attack, p_atk),
        defense: scale(player.defense, p_def),
    };
    let buffed_enemy = Combatant {
        attack: scale(enemy.attack, e_atk),
        defense: scale(enemy.defense, e_def),
    };

    let base = resolve_round(rng, buffed_player, buffed_enemy);
    let damage_to_player = scale(base.damage_to_player, e_dmg);

    let mut heal = 0u32;
    let mut bonus_to_enemy = 0u32;
    let mut messages = Vec::new();
    for b in buffs.iter() {
        heal += b.regen;
        if b.lifetap > 0.0 {
            heal += scale(base.damage_to_enemy, b.lifetap);
        }
        if b.damage_shield > 0.0 {
            bonus_to_enemy += scale(damage_to_player, b.damage_shield);
        }
        for _ in 0..b.minion_count {
            let hi = b.minion_max.max(b.minion_min);
            bonus_to_enemy += rng.gen_range(b.minion_min..=hi);
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
        damage_to_enemy: base.damage_to_enemy + bonus_to_enemy,
        damage_to_player,
        player_crit: base.player_crit,
        player_heal: heal,
        messages,
    }
}

/// How a fully simulated fight ended. Used by tests and balance checks; the
/// live game steps one [`resolve_round`] per player action instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FightResult {
    PlayerWon { rounds: u32, player_hp_left: u32 },
    PlayerLost { rounds: u32, enemy_hp_left: u32 },
}

/// Simulate a fight to the death, round by round, player striking first each
/// round. Helper for tests and offline balance tuning.
pub fn simulate_fight(
    rng: &mut impl Rng,
    player: Combatant,
    mut player_hp: u32,
    enemy: Combatant,
    mut enemy_hp: u32,
) -> FightResult {
    let mut rounds = 0;
    loop {
        rounds += 1;
        let outcome = resolve_round(rng, player, enemy);
        enemy_hp = enemy_hp.saturating_sub(outcome.damage_to_enemy);
        if enemy_hp == 0 {
            return FightResult::PlayerWon {
                rounds,
                player_hp_left: player_hp,
            };
        }
        player_hp = player_hp.saturating_sub(outcome.damage_to_player);
        if player_hp == 0 {
            return FightResult::PlayerLost {
                rounds,
                enemy_hp_left: enemy_hp,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};

    #[test]
    fn bell_rand_stays_in_range_and_centers() {
        let mut rng = StdRng::seed_from_u64(1);
        let mut sum = 0u64;
        let n = 100;
        let iters = 10_000;
        for _ in 0..iters {
            let v = bell_rand(&mut rng, n);
            assert!(v <= n);
            sum += v as u64;
        }
        let mean = sum as f64 / iters as f64;
        // Triangular over 0..=100 centers near 50.
        assert!((mean - 50.0).abs() < 3.0, "mean was {mean}");
    }

    #[test]
    fn bell_rand_zero_is_zero() {
        let mut rng = StdRng::seed_from_u64(2);
        assert_eq!(bell_rand(&mut rng, 0), 0);
    }

    #[test]
    fn round_always_makes_progress() {
        let mut rng = StdRng::seed_from_u64(3);
        let p = Combatant { attack: 5, defense: 5 };
        let e = Combatant { attack: 5, defense: 5 };
        for _ in 0..1000 {
            let o = resolve_round(&mut rng, p, e);
            assert!(o.damage_to_enemy > 0 || o.damage_to_player > 0);
        }
    }

    #[test]
    fn buff_regen_heals_and_expires() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut regen = Buff::new("Regen", 2);
        regen.regen = 5;
        let mut buffs = vec![regen];
        let p = Combatant { attack: 5, defense: 5 };
        let e = Combatant { attack: 5, defense: 5 };
        // Round 1 and 2 each grant the regen heal; the buff lasts two rounds.
        let r1 = resolve_round_buffed(&mut rng, p, e, &mut buffs);
        assert_eq!(r1.player_heal, 5);
        assert_eq!(buffs.len(), 1);
        let r2 = resolve_round_buffed(&mut rng, p, e, &mut buffs);
        assert_eq!(r2.player_heal, 5);
        // It wore off at the end of round 2.
        assert!(buffs.is_empty());
        let r3 = resolve_round_buffed(&mut rng, p, e, &mut buffs);
        assert_eq!(r3.player_heal, 0);
    }

    #[test]
    fn buff_curse_halves_incoming_damage() {
        // A foe that always deals damage, with and without the half-damage curse.
        let p = Combatant { attack: 0, defense: 0 };
        let e = Combatant { attack: 100, defense: 0 };
        let mut none: Vec<Buff> = vec![];
        let mut curse = Buff::new("Curse", 5);
        curse.enemy_dmg_mod = 0.5;
        let cursed = vec![curse];

        let mut plain_total = 0u64;
        let mut cursed_total = 0u64;
        for seed in 0..200 {
            let mut r1 = StdRng::seed_from_u64(seed);
            plain_total += resolve_round_buffed(&mut r1, p, e, &mut none).damage_to_player as u64;
            let mut r2 = StdRng::seed_from_u64(seed);
            cursed_total +=
                resolve_round_buffed(&mut r2, p, e, &mut cursed.clone()).damage_to_player as u64;
        }
        // Cursed damage should land near half of the uncursed total.
        assert!(cursed_total * 2 <= plain_total + plain_total / 5);
        assert!(cursed_total > 0);
    }

    #[test]
    fn overpowered_player_reliably_wins() {
        let mut rng = StdRng::seed_from_u64(4);
        let player = Combatant { attack: 40, defense: 30 };
        let enemy = Combatant { attack: 3, defense: 3 };
        let mut wins = 0;
        for _ in 0..200 {
            if let FightResult::PlayerWon { .. } = simulate_fight(&mut rng, player, 200, enemy, 21)
            {
                wins += 1;
            }
        }
        assert!(wins > 190, "expected near-certain wins, got {wins}/200");
    }
}
