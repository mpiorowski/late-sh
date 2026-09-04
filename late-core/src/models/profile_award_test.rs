use chrono::{Datelike, Duration, Months, Utc};

use crate::models::artboard_piece::{ArtboardPiece, HangOutcome, PieceListing};
use crate::models::artboard_piece_test::hang_params;
use crate::models::chips::{ChipMove, Difficulty, UserChips};
use crate::models::crown::CrownReign;
use crate::models::profile_award::{
    CROWN_AWARD_CATEGORY, DARKROOM_BEACON_AWARD_CATEGORY, GALLERY_AWARD_CATEGORY,
    LATEANIA_ARCHDEMON_AWARD_CATEGORY, LATEANIA_FRONTIER_KING_AWARD_CATEGORY,
    LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY, LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY,
    NETHACK_AMULET_AWARD_CATEGORY, NETHACK_ASCENSION_AWARD_CATEGORY, award_badge,
    award_category_label, format_score_value, is_milestone_award, is_rankless_award,
    list_profile_awards_for_user, snapshot_previous_month_profile_awards, top_badge_per_game,
};
use crate::models::rubiks_cube::DailyWin as RubiksCubeDailyWin;
use crate::models::sliding_puzzle::DailyWin as SlidingPuzzleDailyWin;
use crate::models::sudoku::DailyWin as SudokuDailyWin;
use crate::test_utils::{
    create_test_user, roll_artboard_pieces_back_a_month, roll_crown_reigns_back_a_month, test_db,
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

    snapshot_previous_month_profile_awards(&mut client)
        .await
        .expect("snapshot");
    snapshot_previous_month_profile_awards(&mut client)
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

/// The persisted Arcade Wins award must score the same roster as the live
/// board: every `DailyPuzzle`, at `Difficulty::points` weights. A player
/// whose month came from Sliding Puzzle and Rubik's Cube outranks one easy
/// Sudoku, exactly as the leaderboard page shows it.
#[tokio::test]
async fn arcade_wins_snapshot_scores_every_daily_puzzle_in_the_roster() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let roster = create_test_user(&test_db.db, "arcade-award-roster").await;
    let classic = create_test_user(&test_db.db, "arcade-award-classic").await;

    let this_month = Utc::now().date_naive().with_day(1).expect("first of month");
    let last_month = this_month
        .checked_sub_months(Months::new(1))
        .expect("previous month");
    let played_on = last_month + Duration::days(3);

    SlidingPuzzleDailyWin::record_win(&client, roster.id, Difficulty::Hard, played_on, 90)
        .await
        .expect("sliding puzzle win");
    RubiksCubeDailyWin::record_win(&client, roster.id, played_on)
        .await
        .expect("rubik's cube win");
    SudokuDailyWin::record_win(&client, classic.id, "easy".to_string(), played_on, 1)
        .await
        .expect("sudoku win");

    snapshot_previous_month_profile_awards(&mut client)
        .await
        .expect("snapshot");

    let arcade_award = |awards: Vec<crate::models::profile_award::ProfileAward>| {
        awards
            .into_iter()
            .find(|award| award.category == "arcade_wins")
            .map(|award| (award.rank, award.score_value))
    };
    let roster_award = arcade_award(
        list_profile_awards_for_user(&client, roster.id)
            .await
            .expect("roster awards"),
    );
    let classic_award = arcade_award(
        list_profile_awards_for_user(&client, classic.id)
            .await
            .expect("classic awards"),
    );
    assert_eq!(
        roster_award,
        Some((1, Difficulty::Hard.points() + Difficulty::Medium.points())),
        "hard Sliding Puzzle plus Rubik's Cube leads the month"
    );
    assert_eq!(classic_award, Some((2, Difficulty::Easy.points())));
}

/// The gallery award ranks hangers by their best piece's applause, breaks a
/// tie toward the earlier hang so a tie never doubles the prize, needs the
/// applause floor to rank at all, and pays its chips exactly once: however
/// many replicas run the snapshot, and however the applause moves after the
/// month is settled.
#[tokio::test]
async fn the_gallery_award_ranks_best_pieces_and_pays_once() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let winner = create_test_user(&test_db.db, "gallery-award-winner").await;
    let runner_up = create_test_user(&test_db.db, "gallery-award-second").await;
    let unranked = create_test_user(&test_db.db, "gallery-award-quiet").await;
    let fans: Vec<_> = {
        let mut fans = Vec::new();
        for n in 0..4 {
            fans.push(create_test_user(&test_db.db, &format!("gallery-award-fan-{n}")).await);
        }
        fans
    };

    let hang = |user_id, title: &str, hash: &str| {
        let params = hang_params(user_id, title, hash);
        async {
            match ArtboardPiece::hang(&client, params).await.expect("hang") {
                HangOutcome::Hung(piece) => piece,
                other => panic!("expected a hang, got {other:?}"),
            }
        }
    };
    // The winner's best piece has four hands; a weaker second piece must not
    // add to the score. The runner-up ties at four but hung later, which
    // is what decides second place: with RANK both would be paid first.
    let best = hang(winner.id, "best", "hash-best").await;
    let lesser = hang(winner.id, "lesser", "hash-lesser").await;
    let second = hang(runner_up.id, "second", "hash-second").await;
    let quiet = hang(unranked.id, "quiet", "hash-quiet").await;
    for fan in &fans {
        ArtboardPiece::toggle_applause(&client, best.id, fan.id)
            .await
            .expect("applaud");
    }
    ArtboardPiece::toggle_applause(&client, lesser.id, fans[0].id)
        .await
        .expect("applaud");
    for fan in &fans {
        ArtboardPiece::toggle_applause(&client, second.id, fan.id)
            .await
            .expect("applaud");
    }
    for fan in &fans[..2] {
        ArtboardPiece::toggle_applause(&client, quiet.id, fan.id)
            .await
            .expect("applaud");
    }
    roll_artboard_pieces_back_a_month(&client).await;

    let first_pass = snapshot_previous_month_profile_awards(&mut client)
        .await
        .expect("snapshot");
    assert_eq!(
        first_pass.gallery_prizes_paid,
        vec![(winner.id, 1, 10_000), (runner_up.id, 2, 5_000)]
    );

    // Applause keeps moving after the rollover. The quiet piece clears the
    // floor now and would take third; the month is settled, so the re-run
    // (a restart, the 24h fallback, another replica) pays nobody.
    ArtboardPiece::toggle_applause(&client, quiet.id, fans[2].id)
        .await
        .expect("late applause");
    let second_pass = snapshot_previous_month_profile_awards(&mut client)
        .await
        .expect("snapshot again");
    assert!(
        second_pass.gallery_prizes_paid.is_empty(),
        "a re-run pays nothing, even after the ranking moved"
    );

    let gallery_award = |awards: Vec<crate::models::profile_award::ProfileAward>| {
        awards
            .into_iter()
            .find(|award| award.category == GALLERY_AWARD_CATEGORY)
            .map(|award| (award.badge(), award.score_value))
    };
    let awards = list_profile_awards_for_user(&client, winner.id)
        .await
        .expect("awards");
    assert_eq!(gallery_award(awards), Some(("ART1".to_string(), 4)));
    let awards = list_profile_awards_for_user(&client, runner_up.id)
        .await
        .expect("awards");
    assert_eq!(gallery_award(awards), Some(("ART2".to_string(), 4)));
    let awards = list_profile_awards_for_user(&client, unranked.id)
        .await
        .expect("awards");
    assert_eq!(
        gallery_award(awards),
        None,
        "two hands were under the floor when the month settled; the third came too late"
    );

    let balance = UserChips::find(&client, winner.id)
        .await
        .expect("chips")
        .expect("the prize opened a balance");
    assert_eq!(balance.balance, 10_000);
    let ledger = client
        .query(
            "SELECT delta FROM chip_ledger WHERE user_id = $1 AND reason = $2",
            &[&winner.id, &ChipMove::ArtboardPrize.reason()],
        )
        .await
        .expect("ledger");
    assert_eq!(ledger.len(), 1, "one prize row, however many passes ran");

    // Last month's winner is what the splash and the hall of fame show.
    let splash = ArtboardPiece::previous_month_winner(&client)
        .await
        .expect("winner")
        .expect("a winner cleared the floor");
    assert_eq!(splash.id, best.id);
    let hall = ArtboardPiece::list(&client, winner.id, PieceListing::HallOfFame)
        .await
        .expect("hall of fame");
    assert_eq!(hall.first().map(|piece| piece.id), Some(best.id));
}
