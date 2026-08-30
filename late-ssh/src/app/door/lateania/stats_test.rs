use super::*;

#[test]
fn modifier_follows_the_dnd_rule() {
    assert_eq!(modifier(10), 0);
    assert_eq!(modifier(11), 0);
    assert_eq!(modifier(12), 1);
    assert_eq!(modifier(8), -1);
    assert_eq!(modifier(7), -2);
    assert_eq!(modifier(18), 4);
    assert_eq!(modifier(3), -4);
}

#[test]
fn rolls_are_in_the_4d6_drop_lowest_range() {
    // Top three of 4d6 can range 3..=18; check many rolls stay in-band.
    for _ in 0..2000 {
        let s = AbilityScores::roll();
        for (_, value, _) in s.rows() {
            assert!((3..=18).contains(&value), "score {value} out of 4d6 range");
        }
    }
}

#[test]
fn defaults_are_neutral() {
    let s = AbilityScores::default();
    for (_, value, modifier) in s.rows() {
        assert_eq!(value, 10);
        assert_eq!(modifier, 0);
    }
}

#[test]
fn every_score_moves_one_number_and_says_so() {
    let s = AbilityScores {
        strength: 18,
        dexterity: 14,
        constitution: 16,
        intelligence: 8,
        wisdom: 12,
        charisma: 6,
    };
    assert_eq!(s.swing_pct(), 8);
    assert_eq!(s.crit_pct(), 4);
    assert_eq!(s.hp_bonus(1), 12);
    assert_eq!(s.hp_bonus(50), 87);
    assert_eq!(s.spell_power_pct(), -2);
    assert_eq!(s.regen_bonus(), 1);
    assert_eq!(s.price_pct(), -6);
    assert_eq!(s.tame_pct(), -6);
    assert_eq!(s.effect(Score::Strength, 1), "swings hit for +8%");
    assert_eq!(
        s.effect(Score::Dexterity, 1),
        "4% of swings crit for double"
    );
    assert_eq!(s.effect(Score::Constitution, 50), "+87 max HP at level 50");
    assert_eq!(s.effect(Score::Intelligence, 1), "spell power -2%");
    assert_eq!(s.effect(Score::Wisdom, 1), "+1 resource every tick");
    assert_eq!(
        s.effect(Score::Charisma, 1),
        "shops 6% dearer, sells 6% cheaper, taming -6%"
    );
    let frail = AbilityScores {
        dexterity: 7,
        ..Default::default()
    };
    assert_eq!(
        frail.effect(Score::Dexterity, 1),
        "4% of swings glance for half"
    );
    assert_eq!(
        AbilityScores::default().effect(Score::Dexterity, 1),
        "no crits, no glances"
    );
}

#[test]
fn the_dexterity_roll_crits_above_ten_and_glances_below() {
    assert_eq!(crit_outcome(4, 3), CritOutcome::Critical);
    assert_eq!(crit_outcome(4, 4), CritOutcome::Plain);
    assert_eq!(crit_outcome(-4, 3), CritOutcome::Glancing);
    assert_eq!(crit_outcome(-4, 4), CritOutcome::Plain);
    assert_eq!(crit_outcome(0, 0), CritOutcome::Plain);
}

#[test]
fn a_point_every_fourth_level_and_a_score_stops_at_twenty() {
    assert_eq!(points_earned(1), 0);
    assert_eq!(points_earned(4), 1);
    assert_eq!(points_earned(7), 1);
    assert_eq!(points_earned(100), 25);
    let mut s = AbilityScores {
        strength: 19,
        ..Default::default()
    };
    assert!(s.raise(Score::Strength));
    assert_eq!(s.strength, 20);
    assert!(!s.raise(Score::Strength), "capped at {SCORE_CAP}");
    assert_eq!(s.strength, 20);
    assert!(s.raise(Score::Wisdom));
    assert_eq!(s.wisdom, 11);
}

/// How many points the six scores can still take between them: the number
/// `svc` bounds a character's unplaced points by, so a point never waits on
/// a slot that does not exist.
#[test]
fn headroom_is_the_points_the_scores_can_still_take() {
    assert_eq!(AbilityScores::default().headroom(), 60);
    let nearly = AbilityScores {
        strength: 20,
        dexterity: 20,
        constitution: 20,
        intelligence: 20,
        wisdom: 20,
        charisma: 18,
    };
    assert_eq!(nearly.headroom(), 2);
    let full = AbilityScores {
        charisma: 20,
        ..nearly
    };
    assert_eq!(full.headroom(), 0);
}
