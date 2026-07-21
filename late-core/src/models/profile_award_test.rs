use crate::models::profile_award::{
    LATEANIA_ARCHDEMON_AWARD_CATEGORY, LATEANIA_FRONTIER_KING_AWARD_CATEGORY,
    LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY, LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY,
    NETHACK_AMULET_AWARD_CATEGORY, NETHACK_ASCENSION_AWARD_CATEGORY, award_badge,
    award_category_label, format_score_value,
};

#[test]
fn lateania_boss_awards_have_profile_badge_codes() {
    assert_eq!(award_badge(LATEANIA_ARCHDEMON_AWARD_CATEGORY, 1), "LMG");
    assert_eq!(award_badge(LATEANIA_FRONTIER_KING_AWARD_CATEGORY, 1), "LKN");
    assert_eq!(
        award_badge(LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY, 1),
        "LYS"
    );
    assert_eq!(
        award_category_label(LATEANIA_ARCHDEMON_AWARD_CATEGORY),
        "Lateania Archdemon"
    );
    assert_eq!(
        award_category_label(LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY),
        "Lateania Sundering Deep"
    );
    assert_eq!(
        format_score_value(LATEANIA_FRONTIER_KING_AWARD_CATEGORY, 20_000),
        "20000 chips"
    );
    assert_eq!(
        format_score_value(LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY, 0),
        "Yssgar slain"
    );
    assert_eq!(
        award_badge(LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY, 1),
        "LKA"
    );
    assert_eq!(
        award_category_label(LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY),
        "Lateania Kaethyr Ascendant"
    );
    assert_eq!(
        format_score_value(LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY, 0),
        "Kaethyr Ascendant slain"
    );
}

#[test]
fn nethack_milestone_awards_have_profile_badge_codes() {
    // Rankless like the Lateania bosses: bare code, no rank suffix.
    assert_eq!(award_badge(NETHACK_AMULET_AWARD_CATEGORY, 1), "NHA");
    assert_eq!(award_badge(NETHACK_ASCENSION_AWARD_CATEGORY, 1), "NHY");
    assert_eq!(
        award_category_label(NETHACK_ASCENSION_AWARD_CATEGORY),
        "NetHack Ascension"
    );
    assert_eq!(
        format_score_value(NETHACK_AMULET_AWARD_CATEGORY, 10_000),
        "10000 chips"
    );
}
