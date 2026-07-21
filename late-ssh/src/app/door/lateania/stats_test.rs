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
