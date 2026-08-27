use crate::models::crown::CrownReign;
use crate::models::profile_award::{
    CROWN_AWARD_CATEGORY, DARKROOM_BEACON_AWARD_CATEGORY, LATEANIA_ARCHDEMON_AWARD_CATEGORY,
    LATEANIA_FRONTIER_KING_AWARD_CATEGORY, LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY,
    LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY, NETHACK_AMULET_AWARD_CATEGORY,
    NETHACK_ASCENSION_AWARD_CATEGORY, award_badge, award_category_label, format_score_value,
    is_milestone_award, is_rankless_award, list_profile_awards_for_user,
    snapshot_previous_month_profile_awards, top_badge_per_game,
};
use crate::test_utils::{create_test_user, roll_crown_reigns_back_a_month, test_db};

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

/// The crown's badge is the one monthly award without a rank digit: it has
/// exactly one holder, so `CRWN1` would be noise. It is also not a
/// milestone, which is what keeps it from showing forever.
#[test]
fn the_crown_badge_carries_no_rank_digit_and_is_not_a_milestone() {
    assert_eq!(award_badge(CROWN_AWARD_CATEGORY, 1), "CRWN");
    assert_eq!(award_category_label(CROWN_AWARD_CATEGORY), "The Crown");
    assert_eq!(
        format_score_value(CROWN_AWARD_CATEGORY, 57_000),
        "57000 chips"
    );
    assert!(is_rankless_award(CROWN_AWARD_CATEGORY));
    assert!(!is_milestone_award(CROWN_AWARD_CATEGORY));
}

/// The month's badge goes to whoever held the crown last, not to whoever
/// held it longest or paid most, and the snapshot loop runs daily so it must
/// grant exactly once however many times it runs.
#[tokio::test]
async fn the_months_last_crown_holder_gets_the_badge_once() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let first = create_test_user(&test_db.db, "crown-award-first").await;
    let last = create_test_user(&test_db.db, "crown-award-last").await;

    let tx = client.transaction().await.expect("tx");
    let opened = CrownReign::open_in_tx(&tx, first.id, 5_000)
        .await
        .expect("open");
    CrownReign::close_in_tx(&tx, opened.id)
        .await
        .expect("close");
    CrownReign::open_in_tx(&tx, last.id, 7_500)
        .await
        .expect("open");
    tx.commit().await.expect("commit");
    // The snapshot only ever looks at last month, so stand on the far side
    // of the rollover the way a real month end does.
    roll_crown_reigns_back_a_month(&client).await;

    snapshot_previous_month_profile_awards(&client)
        .await
        .expect("snapshot");
    snapshot_previous_month_profile_awards(&client)
        .await
        .expect("snapshot again");

    let awards = list_profile_awards_for_user(&client, last.id)
        .await
        .expect("awards");
    let crowns: Vec<_> = awards
        .iter()
        .filter(|award| award.category == CROWN_AWARD_CATEGORY)
        .collect();
    assert_eq!(crowns.len(), 1, "the daily snapshot must grant once");
    assert_eq!(crowns[0].rank, 1);
    assert_eq!(crowns[0].score_value, 7_500);
    assert_eq!(crowns[0].badge(), "CRWN");

    let earlier = list_profile_awards_for_user(&client, first.id)
        .await
        .expect("awards");
    assert!(
        !earlier
            .iter()
            .any(|award| award.category == CROWN_AWARD_CATEGORY),
        "only the month's last holder is crowned"
    );
}
