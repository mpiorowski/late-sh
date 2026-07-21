use crate::app::door::greendragon::data::*;

#[test]
fn cost_ladder_is_monotonic() {
    assert!(COST_LADDER.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(COST_LADDER.len(), MAX_LEVEL as usize);
}

#[test]
fn creature_tier_clamps() {
    assert_eq!(creature_tier(0), CREATURES[0]);
    assert_eq!(creature_tier(1), CREATURES[0]);
    assert_eq!(creature_tier(16), CREATURES[15]);
    assert_eq!(creature_tier(99), CREATURES[15]);
}

#[test]
fn exp_scales_with_dragon_kills() {
    assert_eq!(exp_to_advance(1, 0), 100);
    // base 100 + (4/4)*1*100 = 200
    assert_eq!(exp_to_advance(1, 4), 200);
    assert_eq!(exp_to_advance(15, 0), 43930);
}

#[test]
fn master_stats_follow_seed() {
    assert_eq!(master_stats(1), (2, 2, 12));
    assert_eq!(master_stats(14), (28, 28, 154));
    assert_eq!(MASTERS.len(), 14);
}

#[test]
fn every_creature_level_has_at_least_one_name() {
    assert!(CREATURE_NAMES.iter().all(|names| !names.is_empty()));
    assert_eq!(CREATURE_NAMES.len(), CREATURES.len());
}

#[test]
fn graveyard_shades_scale_off_the_player_level() {
    // Level 1: shift -1, base 8; defense round(8*0.7) = 6; hp 55.
    assert_eq!(graveyard_creature_stats(1), (8, 6, 55));
    // Level 4: shift -1, base 9 - 1 + (int)(4.5) = 12; def round(8.4) = 8.
    assert_eq!(graveyard_creature_stats(4), (12, 8, 70));
    // Level 15: no shift, base 9 + 21 = 30; def round(21.0) = 21.
    assert_eq!(graveyard_creature_stats(15), (30, 21, 125));
    // Favor payout range: 10..20 plus round(level/3).
    assert_eq!(graveyard_favor_range(1), (10, 20));
    assert_eq!(graveyard_favor_range(5), (12, 22));
    assert!(!GRAVEYARD_CREATURES.is_empty());
}

#[test]
fn title_ladder_picks_the_highest_earned_threshold() {
    use rand::{SeedableRng, rngs::StdRng};
    let mut rng = StdRng::seed_from_u64(1);
    // Fresh characters get the threshold-0 pair.
    assert_eq!(dk_title_pair(0, &mut rng), ("Mudfoot", "Mudlark"));
    // Exact thresholds and the open top end (past 31 the top rung holds).
    assert_eq!(dk_title_pair(6, &mut rng).0, "Fangtaker");
    assert_eq!(dk_title_pair(10, &mut rng), ("Dragonlord", "Dragonlady"));
    assert_eq!(dk_title_pair(99, &mut rng).0, "The Deathless");
    // One rung per kill 0..=31, upstream's 32-row seed.
    assert_eq!(TITLES.len(), 32);
    assert!(
        TITLES
            .iter()
            .enumerate()
            .all(|(i, (dk, _, _))| *dk == i as u32)
    );
}

#[test]
fn gear_name_tables_cover_every_tier() {
    assert_eq!(WEAPON_NAMES.len(), MAX_LEVEL as usize);
    assert_eq!(ARMOR_NAMES.len(), MAX_LEVEL as usize);
    // Tier 0 is the unarmed/unarmored sentinel.
    assert_eq!(weapon_name(0), "Fists");
    assert_eq!(armor_name(0), "None");
    // Tiers map to their table entry and clamp past the cap.
    assert_eq!(weapon_name(1), WEAPON_NAMES[0]);
    assert_eq!(weapon_name(MAX_LEVEL), WEAPON_NAMES[MAX_LEVEL as usize - 1]);
    assert_eq!(weapon_name(99), WEAPON_NAMES[MAX_LEVEL as usize - 1]);
    assert_eq!(armor_name(99), ARMOR_NAMES[MAX_LEVEL as usize - 1]);
}
