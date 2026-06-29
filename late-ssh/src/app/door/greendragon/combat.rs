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
