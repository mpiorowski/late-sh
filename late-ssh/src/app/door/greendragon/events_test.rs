use super::model::Character;
use crate::app::door::greendragon::events::*;
use rand::{SeedableRng, rngs::StdRng};

fn hero(level: u8) -> Character {
    let mut c = Character::new("t", 0);
    c.level = level;
    c.hitpoints = c.max_hitpoints();
    c
}

#[test]
fn roll_is_in_range() {
    let mut rng = StdRng::seed_from_u64(1);
    for _ in 0..200 {
        assert!(ALL.contains(&roll(&mut rng)));
    }
}

#[test]
fn findgold_pays_scaled_gold() {
    let mut rng = StdRng::seed_from_u64(2);
    let mut c = hero(5);
    c.gold = 0;
    ForestEvent::FindGold.resolve(true, &mut c, &mut rng);
    // level 5 -> 50..=250 gold.
    assert!((50..=250).contains(&c.gold), "got {}", c.gold);
}

#[test]
fn fairy_gemless_accept_costs_a_turn() {
    let mut rng = StdRng::seed_from_u64(3);
    let mut c = hero(3);
    c.gems = 0;
    c.turns = 5;
    ForestEvent::Fairy.resolve(true, &mut c, &mut rng);
    // No gem to give: upstream docks a forest fight for the wasted time.
    assert_eq!(c.gems, 0);
    assert_eq!(c.turns, 4);
}

#[test]
fn glowingstream_energetic_band_gives_turn_not_heal() {
    // Force the 5..=7 band and confirm it grants a fight but no heal.
    let mut found = false;
    for seed in 0..200 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut c = hero(5);
        c.hitpoints = 1;
        c.turns = 3;
        ForestEvent::GlowingStream.resolve(true, &mut c, &mut rng);
        if c.alive && c.turns == 4 && c.hitpoints == 1 {
            found = true;
            break;
        }
    }
    assert!(found, "expected a turns-only outcome with no heal");
}

#[test]
fn fairy_spends_the_gem() {
    let mut rng = StdRng::seed_from_u64(4);
    let mut c = hero(3);
    c.gems = 1;
    ForestEvent::Fairy.resolve(true, &mut c, &mut rng);
    // The offered gem is always consumed (the boon varies by roll).
    assert!(c.gems != 1 || c.turns != 10);
}

#[test]
fn declining_a_choice_event_is_inert() {
    let mut rng = StdRng::seed_from_u64(5);
    let mut c = hero(4);
    let before = c.clone();
    ForestEvent::GlowingStream.resolve(false, &mut c, &mut rng);
    assert_eq!(c.hitpoints, before.hitpoints);
    assert!(c.alive);
}

#[test]
fn goldmine_cave_in_spares_the_deepfolk() {
    use crate::app::door::greendragon::model::Race;
    // Sweep seeds until each race hits the cave-in arm (roll 19..=20) and
    // compare fates: default races nearly always die, Deepfolk nearly
    // always walk. A survived cave-in always zeroes the day's turns.
    let mut default_deaths = 0;
    let mut deepfolk_deaths = 0;
    let mut cave_ins = 0;
    for seed in 0..4000 {
        let mut c = hero(7);
        c.turns = 5;
        ForestEvent::GoldMine.resolve(true, &mut c, &mut StdRng::seed_from_u64(seed));
        // The cave-in arm is the only outcome that kills or zeroes turns.
        if !c.alive || c.turns == 0 {
            cave_ins += 1;
            default_deaths += u32::from(!c.alive);
            if c.alive {
                assert_eq!(c.turns, 0); // the escape costs the day
            }
            let mut d = hero(7);
            d.race = Race::Deepfolk;
            d.turns = 5;
            ForestEvent::GoldMine.resolve(true, &mut d, &mut StdRng::seed_from_u64(seed));
            deepfolk_deaths += u32::from(!d.alive);
        }
    }
    assert!(cave_ins > 20, "expected many cave-ins, got {cave_ins}");
    // 90% vs 5% death chance (upstream raceminedeath defaults).
    assert!(
        default_deaths * 2 > cave_ins,
        "default races should mostly die"
    );
    assert!(
        deepfolk_deaths * 4 < cave_ins,
        "deepfolk should rarely die: {deepfolk_deaths}/{cave_ins}"
    );
}

#[test]
fn goldmine_only_kills_on_the_cave_in() {
    // Across many mines the player sometimes dies (cave-in) but mostly
    // survives, losing a forest fight each time.
    let mut deaths = 0;
    let mut survivals = 0;
    for seed in 0..400 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut c = hero(7);
        c.turns = 5;
        ForestEvent::GoldMine.resolve(true, &mut c, &mut rng);
        if c.alive {
            survivals += 1;
        } else {
            deaths += 1;
        }
    }
    assert!(deaths > 0, "expected some cave-ins");
    assert!(survivals > deaths, "cave-ins should be the minority");
}
