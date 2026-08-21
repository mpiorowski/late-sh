use crate::models::profile_award::{
    DARKROOM_BEACON_AWARD_CATEGORY, LATEANIA_ARCHDEMON_AWARD_CATEGORY,
    LATEANIA_FRONTIER_KING_AWARD_CATEGORY, LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY,
    LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY, NETHACK_AMULET_AWARD_CATEGORY,
    NETHACK_ASCENSION_AWARD_CATEGORY, award_badge, award_category_label, format_score_value,
    top_badge_per_game,
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
        format_score_value(LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY, 10_000),
        "10000 chips"
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
        format_score_value(LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY, 10_000),
        "10000 chips"
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

#[test]
fn the_beacon_ending_has_its_own_badge_above_the_plain_escape() {
    assert_eq!(award_badge(DARKROOM_BEACON_AWARD_CATEGORY, 1), "ADB");
    assert_eq!(
        award_category_label(DARKROOM_BEACON_AWARD_CATEGORY),
        "A Dark Room Homefleet"
    );
    assert_eq!(
        format_score_value(DARKROOM_BEACON_AWARD_CATEGORY, 10_000),
        "10000 chips"
    );
}

#[test]
fn chat_labels_keep_only_the_top_badge_of_each_game() {
    // Everything a player could hold at once, in badge-strip order.
    let held = ["LMG", "LKN", "LYS", "LKA", "NHA", "NHY", "DCO", "DCW"];
    assert_eq!(top_badge_per_game(held), vec!["LKA", "NHY", "DCW"]);

    // A partial ladder collapses to the highest rung actually held, not to
    // the ladder's top rung.
    assert_eq!(top_badge_per_game(["LMG", "LKN"]), vec!["LKN"]);
    assert_eq!(top_badge_per_game(["BRE"]), vec!["BRE"]);
    assert_eq!(top_badge_per_game(["BRE", "BRM"]), vec!["BRM"]);

    // A Dark Room: flying out holding the fleet beacon supersedes the plain
    // escape, which is what the second ending needed a ladder for.
    assert_eq!(top_badge_per_game(["ADE", "ADB"]), vec!["ADB"]);
    assert_eq!(top_badge_per_game(["ADE"]), vec!["ADE"]);

    // Badges on no ladder (Green Dragon, the ranked monthly boards) pass
    // through untouched, and the input's order is preserved.
    assert_eq!(
        top_badge_per_game(["AW1", "GDS", "LMG", "LKA"]),
        vec!["AW1", "GDS", "LKA"]
    );
}
