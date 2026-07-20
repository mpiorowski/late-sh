use super::*;
use rand::{SeedableRng, rngs::StdRng};

#[test]
fn bell_rand_centers_near_half_with_long_tails() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut sum = 0.0;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let n = 100.0;
    let iters = 50_000;
    for _ in 0..iters {
        let v = bell_rand(&mut rng, n);
        sum += v;
        min = min.min(v);
        max = max.max(v);
    }
    let mean = sum / iters as f64;
    // Median z ~0.498; the mean sits a touch above 0.5*n thanks to the
    // skewed tails. Range can go negative and overshoot n.
    assert!((mean - 49.8).abs() < 3.0, "mean was {mean}");
    assert!(min < 0.0, "expected negative tail, min was {min}");
    assert!(max > n, "expected overshoot tail, max was {max}");
}

#[test]
fn bell_rand_zero_is_zero() {
    let mut rng = StdRng::seed_from_u64(2);
    assert_eq!(bell_rand(&mut rng, 0.0), 0.0);
}

#[test]
fn round_always_makes_progress() {
    let mut rng = StdRng::seed_from_u64(3);
    let p = Combatant {
        attack: 5,
        defense: 5,
    };
    let e = Combatant {
        attack: 5,
        defense: 5,
    };
    for _ in 0..1000 {
        let o = resolve_round(&mut rng, p, e);
        assert!(o.damage_to_enemy != 0 || o.damage_to_player != 0);
    }
}

#[test]
fn buff_regen_heals_and_expires() {
    let mut rng = StdRng::seed_from_u64(7);
    let mut regen = Buff::new("Regen", 2);
    regen.regen = 5;
    let mut buffs = vec![regen];
    let mut comps = Vec::new();
    let p = Combatant {
        attack: 5,
        defense: 5,
    };
    let e = Combatant {
        attack: 5,
        defense: 5,
    };
    let r1 = resolve_round_buffed(&mut rng, p, e, 1000, &mut buffs, &mut comps);
    assert_eq!(r1.player_heal, 5);
    assert_eq!(buffs.len(), 1);
    let r2 = resolve_round_buffed(&mut rng, p, e, 1000, &mut buffs, &mut comps);
    assert_eq!(r2.player_heal, 5);
    assert!(buffs.is_empty());
    let r3 = resolve_round_buffed(&mut rng, p, e, 1000, &mut buffs, &mut comps);
    assert_eq!(r3.player_heal, 0);
}

#[test]
fn buff_curse_reduces_incoming_damage() {
    // A foe that always deals damage, with and without the half-damage curse.
    let p = Combatant {
        attack: 0,
        defense: 0,
    };
    let e = Combatant {
        attack: 100,
        defense: 0,
    };
    let mut plain_total = 0i64;
    let mut cursed_total = 0i64;
    for seed in 0..400 {
        let mut none: Vec<Buff> = vec![];
        let mut nc = Vec::new();
        let mut r1 = StdRng::seed_from_u64(seed);
        let d = resolve_round_buffed(&mut r1, p, e, 1000, &mut none, &mut nc).damage_to_player;
        plain_total += d.max(0) as i64;

        let mut curse = Buff::new("Curse", 5);
        curse.enemy_dmg_mod = 0.5;
        let mut cursed = vec![curse];
        let mut cc = Vec::new();
        let mut r2 = StdRng::seed_from_u64(seed);
        let d =
            resolve_round_buffed(&mut r2, p, e, 1000, &mut cursed, &mut cc).damage_to_player;
        cursed_total += d.max(0) as i64;
    }
    assert!(cursed_total > 0);
    assert!(cursed_total < plain_total, "curse should reduce damage");
}

#[test]
fn companion_fights_and_can_fall() {
    // A strong enemy eventually kills a frail companion; a sturdy one helps.
    let mut rng = StdRng::seed_from_u64(11);
    let p = Combatant {
        attack: 5,
        defense: 5,
    };
    let e = Combatant {
        attack: 50,
        defense: 5,
    };
    let mut buffs = Vec::new();
    let mut comps = vec![Companion {
        name: "Skeleton".into(),
        hitpoints: 5,
        max_hitpoints: 5,
        attack: 10.0,
        defense: 1.0,
        attack_per_level: 0,
        defense_per_level: 0,
        hp_per_level: 0,
        dying_text: "It crumbles.".into(),
        ability: CompanionAbility::Fight,
        ignore_limit: true,
    }];
    let mut fell = false;
    for _ in 0..50 {
        resolve_round_buffed(&mut rng, p, e, 10_000, &mut buffs, &mut comps);
        if comps.is_empty() {
            fell = true;
            break;
        }
    }
    assert!(fell, "the companion should eventually be destroyed");
}

#[test]
fn overpowered_player_reliably_wins() {
    let mut rng = StdRng::seed_from_u64(4);
    let player = Combatant {
        attack: 40,
        defense: 30,
    };
    let enemy = Combatant {
        attack: 3,
        defense: 3,
    };
    let mut wins = 0;
    for _ in 0..200 {
        if let FightResult::PlayerWon { .. } =
            simulate_fight(&mut rng, player, 200, 200, enemy, 21, 21)
        {
            wins += 1;
        }
    }
    assert!(wins > 190, "expected near-certain wins, got {wins}/200");
}
