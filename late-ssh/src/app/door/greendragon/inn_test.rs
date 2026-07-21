use super::model::Character;
use crate::app::door::greendragon::inn::*;
use rand::{SeedableRng, rngs::StdRng};

fn hero(level: u8) -> Character {
    let mut c = Character::new("t", 0);
    c.level = level;
    c.hitpoints = c.max_hitpoints();
    c
}

#[test]
fn bard_sings_once_and_stays_survivable() {
    // Sweep seeds: every outcome leaves HP >= 1 and marks the day.
    for seed in 0..300 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut c = hero(5);
        c.gold = 3; // too poor for the hat (case 8's no-op branch)
        bard_song(&mut c, &mut rng);
        assert!(c.heard_bard_today);
        assert!(c.hitpoints >= 1);
    }
}

#[test]
fn flirt_certain_at_threshold_and_capped() {
    let mut rng = StdRng::seed_from_u64(7);
    // At charm >= T the roll can't fail; at the cap no more charm accrues.
    let mut c = hero(3);
    c.charm = 4; // rung 1 (T=2, cap=4): certain success, already capped
    let out = flirt(&mut c, 0, &mut rng);
    assert_eq!(c.charm, 4);
    assert!(out.news.is_none());
    assert!(c.flirted_today);
}

#[test]
fn evening_upstairs_costs_turns_and_makes_news() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut c = hero(5);
    c.charm = 18; // certain at rung 6 (T=18), under its 25 cap
    c.turns = 5;
    let out = flirt(&mut c, 5, &mut rng);
    assert_eq!(c.charm, 19);
    assert_eq!(c.turns, 3);
    assert!(out.news.is_some());
}

#[test]
fn proposal_marries_at_22_and_crushes_below() {
    let mut rng = StdRng::seed_from_u64(2);
    let mut c = hero(5);
    c.charm = 22;
    c.turns = 7;
    let out = flirt(&mut c, 6, &mut rng);
    assert!(c.married);
    assert!(out.news.unwrap().contains("matrimony"));
    assert_eq!(c.turns, 7); // the wedding costs no turns
    assert!(!c.persistent_buffs.is_empty()); // the ward arrives with the vows

    let mut d = hero(5);
    d.charm = 21;
    d.turns = 7;
    let out = flirt(&mut d, 6, &mut rng);
    assert!(!d.married);
    assert!(out.news.is_none());
    assert_eq!(d.turns, 0); // rejection ends the day
    assert_eq!(d.charm, 21); // but costs no charm
}

#[test]
fn married_visit_rebuffs_a_quarter_of_the_time() {
    let (mut rebuffs, mut wards) = (0, 0);
    for seed in 0..400 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut c = hero(5);
        c.married = true;
        c.charm = 10;
        married_visit(&mut c, &mut rng);
        if c.charm == 9 {
            rebuffs += 1;
            assert!(c.persistent_buffs.is_empty());
        } else {
            assert_eq!(c.charm, 11);
            wards += 1;
            assert_eq!(c.persistent_buffs[0].slot, "lover");
        }
    }
    assert!(rebuffs > 50 && wards > rebuffs, "{rebuffs} vs {wards}");
}

#[test]
fn chat_buckets_span_the_charm_range() {
    let mut rng = StdRng::seed_from_u64(3);
    for charm in [0u32, 2, 5, 8, 11, 14, 17, 30] {
        let mut c = hero(3);
        c.charm = charm;
        let line = chat(&c, &mut rng);
        assert!(!line.is_empty());
        assert!(!c.flirted_today); // chat never spends the daily visit
    }
}
