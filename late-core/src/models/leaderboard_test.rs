use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::{
    models::{
        chips::Difficulty,
        le_word,
        leaderboard::{
            DailyPuzzle, OnlineTimeIncrement, RankedEntry, apply_online_time_batch,
            fetch_leaderboard_data,
        },
        mud_character::MudCharacter,
        rubiks_cube, sliding_puzzle, sudoku,
    },
    test_utils::{create_test_user, test_db},
};

fn entry_for(entries: &[RankedEntry], user_id: Uuid) -> &RankedEntry {
    entries
        .iter()
        .find(|entry| entry.user_id == user_id)
        .expect("user on board")
}

#[tokio::test]
async fn online_time_batches_are_idempotent_and_rank_both_windows() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let alice = create_test_user(&test_db.db, "lb_online_alice").await;
    let bob = create_test_user(&test_db.db, "lb_online_bob").await;
    let deleted = create_test_user(&test_db.db, "lb_online_deleted").await;
    client
        .execute("DELETE FROM users WHERE id = $1", &[&deleted.id])
        .await
        .expect("delete user before batch");

    let first_flush = Uuid::now_v7();
    let today = Utc::now().date_naive();
    let current_month =
        NaiveDate::from_ymd_opt(today.year(), today.month(), 1).expect("current month");
    let previous_day = current_month - Duration::days(1);
    let previous_month = NaiveDate::from_ymd_opt(previous_day.year(), previous_day.month(), 1)
        .expect("previous month");
    let increment = |user_id, month_start, milliseconds| OnlineTimeIncrement {
        user_id,
        month_start,
        milliseconds,
    };
    let increments = [
        increment(alice.id, current_month, 60_000),
        increment(alice.id, current_month, 40_000),
        increment(bob.id, current_month, 90_000),
        increment(deleted.id, current_month, 500_000),
    ];
    assert_eq!(
        apply_online_time_batch(&client, first_flush, &increments)
            .await
            .expect("apply first batch"),
        2,
        "duplicate input users are folded and deleted users are skipped"
    );
    apply_online_time_batch(&client, first_flush, &increments)
        .await
        .expect("retry first batch");
    apply_online_time_batch(
        &client,
        Uuid::now_v7(),
        &[increment(alice.id, current_month, 10_000)],
    )
    .await
    .expect("apply later batch");
    apply_online_time_batch(
        &client,
        Uuid::now_v7(),
        &[
            increment(alice.id, previous_month, 50_000),
            increment(bob.id, previous_month, 200_000),
        ],
    )
    .await
    .expect("apply previous-month batch");

    let alice_total: i64 = client
        .query_one(
            "SELECT total_milliseconds FROM user_online_time WHERE user_id = $1",
            &[&alice.id],
        )
        .await
        .expect("alice online time")
        .get(0);
    assert_eq!(alice_total, 160_000, "retry must not add the batch twice");

    let alice_monthly: i64 = client
        .query_one(
            "SELECT total_milliseconds
             FROM user_online_time_monthly
             WHERE month_start = $1 AND user_id = $2",
            &[&current_month, &alice.id],
        )
        .await
        .expect("alice monthly online time")
        .get(0);
    assert_eq!(alice_monthly, 110_000);

    let data = fetch_leaderboard_data(&client)
        .await
        .expect("fetch leaderboard");
    assert_eq!(entry_for(&data.online_time.monthly, alice.id).rank, 1);
    assert_eq!(
        entry_for(&data.online_time.monthly, alice.id).value,
        110_000
    );
    assert_eq!(entry_for(&data.online_time.monthly, bob.id).rank, 2);
    assert_eq!(entry_for(&data.online_time.all_time, bob.id).rank, 1);
    assert_eq!(entry_for(&data.online_time.all_time, bob.id).value, 290_000);
    assert_eq!(entry_for(&data.online_time.all_time, alice.id).rank, 2);
    assert!(
        !data
            .online_time
            .monthly
            .iter()
            .any(|entry| entry.user_id == deleted.id)
    );
    assert!(
        !data
            .online_time
            .all_time
            .iter()
            .any(|entry| entry.user_id == deleted.id)
    );
}

/// One fixture exercises the whole roster-generated pipeline: the per-puzzle
/// win-count boards in both windows, and the Arcade Wins points where every
/// weight comes from `Difficulty` (Le Word fixed 1, Rubik's fixed 3, Sudoku
/// by difficulty key), so the old `'medium'`-string hack cannot come back.
#[tokio::test]
async fn daily_boards_and_arcade_points_follow_the_roster() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let solver = create_test_user(&test_db.db, "lb_solver").await;
    let rival = create_test_user(&test_db.db, "lb_rival").await;

    let today = Utc::now().date_naive();
    let month_start =
        NaiveDate::from_ymd_opt(today.year(), today.month(), 1).expect("first of month");
    let day = |offset: i64| month_start + Duration::days(offset);

    // Solver: five Le Word wins this month plus one from last month, and one
    // hard Sudoku win. Monthly points: 5 * 1 + 5 = 10.
    for offset in [0, 1, 2, 5, 6] {
        le_word::DailyWin::record_win(&client, solver.id, day(offset), 4)
            .await
            .expect("record solver win");
    }
    le_word::DailyWin::record_win(&client, solver.id, month_start - Duration::days(10), 3)
        .await
        .expect("record solver history");
    sudoku::DailyWin::record_win(&client, solver.id, "hard".to_string(), day(0), 100)
        .await
        .expect("record solver sudoku win");

    // Rival: two Le Word wins and today's Rubik's Cube. Monthly points:
    // 2 * 1 + 3 = 5.
    for offset in [1, 2] {
        le_word::DailyWin::record_win(&client, rival.id, day(offset), 5)
            .await
            .expect("record rival win");
    }
    rubiks_cube::DailyWin::record_win(&client, rival.id, today)
        .await
        .expect("record rival rubiks win");
    sliding_puzzle::DailyWin::record_win(&client, rival.id, Difficulty::Hard, today, 50)
        .await
        .expect("record rival sliding puzzle win");

    let data = fetch_leaderboard_data(&client)
        .await
        .expect("fetch leaderboard");

    let le_word_board = data
        .daily_board(DailyPuzzle::LeWord)
        .expect("le word board present");
    let monthly = entry_for(&le_word_board.monthly, solver.id);
    assert_eq!(monthly.value, 5, "last month's win must not count");
    assert_eq!(monthly.rank, 1);
    assert_eq!(entry_for(&le_word_board.monthly, rival.id).value, 2);

    let all_time = entry_for(&le_word_board.all_time, solver.id);
    assert_eq!(all_time.value, 6, "all-time counts the older win too");
    assert_eq!(all_time.rank, 1);

    let rubiks_board = data
        .daily_board(DailyPuzzle::RubiksCube)
        .expect("rubiks board present");
    assert_eq!(entry_for(&rubiks_board.monthly, rival.id).value, 1);
    assert!(
        !rubiks_board
            .monthly
            .iter()
            .any(|entry| entry.user_id == solver.id),
        "no rubiks win, no rubiks row"
    );

    let solver_points = entry_for(&data.arcade_champions, solver.id);
    assert_eq!(solver_points.value, 10, "5 le word + 1 hard sudoku");
    assert_eq!(solver_points.rank, 1);
    let rival_points = entry_for(&data.arcade_champions, rival.id);
    assert_eq!(
        rival_points.value, 10,
        "2 le word + rubiks at medium weight + hard sliding puzzle"
    );
    assert_eq!(rival_points.rank, 1);

    let sliding_board = data
        .daily_board(DailyPuzzle::SlidingPuzzle)
        .expect("sliding puzzle board present");
    assert_eq!(entry_for(&sliding_board.monthly, rival.id).value, 1);
    let rival_status = data
        .user_daily_statuses
        .get(&rival.id)
        .expect("rival has a daily status");
    assert!(rival_status.completed(DailyPuzzle::RubiksCube));
    assert!(rival_status.completed_difficulty(DailyPuzzle::RubiksCube, "daily"));
    assert!(rival_status.completed(DailyPuzzle::SlidingPuzzle));
    assert!(rival_status.completed_difficulty(DailyPuzzle::SlidingPuzzle, "hard"));
}

/// Same-day replays upsert the win row (keep-best-score), so they must not
/// inflate the `daily_win_totals` rollup the all-time boards read: the bump
/// fires only on a fresh insert.
#[tokio::test]
async fn replayed_daily_win_does_not_double_count_all_time() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let solver = create_test_user(&test_db.db, "lb_replayer").await;
    let today = Utc::now().date_naive();

    sudoku::DailyWin::record_win(&client, solver.id, "hard".to_string(), today, 100)
        .await
        .expect("record first win");
    sudoku::DailyWin::record_win(&client, solver.id, "hard".to_string(), today, 250)
        .await
        .expect("record same-day replay");
    sudoku::DailyWin::record_win(&client, solver.id, "easy".to_string(), today, 90)
        .await
        .expect("record second tier win");

    // Raw witness on the rollup itself: two fresh wins, no replay bump.
    let wins: i64 = client
        .query_one(
            "SELECT wins FROM daily_win_totals WHERE game = 'sudoku' AND user_id = $1",
            &[&solver.id],
        )
        .await
        .expect("rollup row present")
        .get(0);
    assert_eq!(wins, 2);

    let data = fetch_leaderboard_data(&client)
        .await
        .expect("fetch leaderboard");
    let board = data
        .daily_board(DailyPuzzle::Sudoku)
        .expect("sudoku board present");
    assert_eq!(entry_for(&board.all_time, solver.id).value, 2);
}

/// The Lateania boards read the game-owned character blobs: level ranks the
/// adventurers with experience as the tiebreak and the class carried as the
/// row note, the visited-room list yields the deepest Frontier zone, and a
/// pre-class-select shell stays off both boards.
#[tokio::test]
async fn lateania_boards_rank_living_characters() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let hero = create_test_user(&test_db.db, "lb_lateania_hero").await;
    let rival = create_test_user(&test_db.db, "lb_lateania_rival").await;
    let shell = create_test_user(&test_db.db, "lb_lateania_shell").await;

    // Hero and rival share level 42; the hero's higher experience breaks the
    // tie. Room 2749 sits in Frontier zone 15 (rooms 2000..=2999, 50 per
    // zone); room 2000 is zone 1; room 150 is not Frontier at all.
    MudCharacter::save(
        &client,
        hero.id,
        0,
        json!({
            "version": 17,
            "class": "runemaster",
            "level": 42,
            "xp": 900_000,
            "visited": [1, 150, 2000, 2749],
        }),
    )
    .await
    .expect("save hero");
    MudCharacter::save(
        &client,
        rival.id,
        0,
        json!({
            "version": 17,
            "class": "warrior",
            "level": 42,
            "xp": 800_000,
            "visited": [1, 2000],
        }),
    )
    .await
    .expect("save rival");
    MudCharacter::save(
        &client,
        shell.id,
        0,
        json!({ "version": 17, "class": null, "level": 1, "xp": 0, "visited": [150] }),
    )
    .await
    .expect("save shell");

    let data = fetch_leaderboard_data(&client)
        .await
        .expect("fetch leaderboard");

    let hero_row = entry_for(&data.lateania_adventurers, hero.id);
    assert_eq!(hero_row.rank, 1, "experience breaks the level tie");
    assert_eq!(hero_row.value, 42);
    assert_eq!(hero_row.note.as_deref(), Some("Runemaster"));
    assert_eq!(entry_for(&data.lateania_adventurers, rival.id).rank, 2);
    assert!(
        !data
            .lateania_adventurers
            .iter()
            .any(|entry| entry.user_id == shell.id),
        "a character without a chosen class stays off the board"
    );

    assert_eq!(entry_for(&data.lateania_frontier, hero.id).value, 15);
    assert_eq!(entry_for(&data.lateania_frontier, rival.id).value, 1);
    assert!(
        !data
            .lateania_frontier
            .iter()
            .any(|entry| entry.user_id == shell.id),
        "no Frontier room visited, no Frontier row"
    );
}

/// One fixture covers the whole door-board triple over the log-pipe fact
/// tables: all-time wins counting only win results, best score in both
/// windows, and the dive board taking its depth from milestone marks when
/// they outreach the end-of-run depth (a winner ends at the surface).
#[tokio::test]
async fn door_boards_rank_wins_depth_and_score() {
    use crate::models::door_milestone::{DoorMilestone, DoorMilestoneKind, NewDoorMilestone};
    use crate::models::door_run::{DoorRun, DoorRunResult, NewDoorRun};
    use crate::models::leaderboard::DoorGame;

    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let winner = create_test_user(&test_db.db, "lb_door_winner").await;
    let diver = create_test_user(&test_db.db, "lb_door_diver").await;

    let now = Utc::now();
    let run = |user_id, result, score, depth, offset| NewDoorRun {
        game: DoorGame::Dcss.key(),
        user_id,
        ended_at: now,
        result,
        score: Some(score),
        depth: Some(depth),
        turns: Some(1000),
        raw: json!({}),
        source_file: "logfile".to_string(),
        source_offset: offset,
    };

    // Winner: one old death at depth 8, then a win ending at the surface
    // whose Orb milestone carries the real dive depth (27).
    let mut old_death = run(winner.id, DoorRunResult::Death, 5_000, 8, 100);
    old_death.ended_at = now - Duration::days(45);
    for new_run in [
        &old_death,
        &run(winner.id, DoorRunResult::Win, 2_000_000, 1, 200),
    ] {
        assert!(
            DoorRun::insert_ignore(&client, new_run)
                .await
                .expect("insert run")
        );
    }
    assert!(
        DoorMilestone::insert_ignore(
            &client,
            &NewDoorMilestone {
                game: DoorGame::Dcss.key(),
                user_id: winner.id,
                kind: DoorMilestoneKind::Orb,
                occurred_at: now,
                raw: json!({"absdepth": "27"}),
                source_file: "milestones".to_string(),
                source_offset: 300,
            },
        )
        .await
        .expect("insert milestone")
    );

    // A milestone with a malformed depth and one with none at all: neither
    // may rank, and neither may error the whole pass (the query casts the
    // raw string to int only after a numeric guard).
    for (raw, offset) in [(json!({"absdepth": "garbage"}), 310), (json!({}), 320)] {
        assert!(
            DoorMilestone::insert_ignore(
                &client,
                &NewDoorMilestone {
                    game: DoorGame::Dcss.key(),
                    user_id: winner.id,
                    kind: DoorMilestoneKind::Rune,
                    occurred_at: now,
                    raw,
                    source_file: "milestones".to_string(),
                    source_offset: offset,
                },
            )
            .await
            .expect("insert hostile milestone")
        );
    }

    // Diver: deep death this month, no win; quits never count as wins.
    for new_run in [
        &run(diver.id, DoorRunResult::Death, 90_000, 24, 400),
        &run(diver.id, DoorRunResult::Quit, 999_999_999, 1, 500),
    ] {
        assert!(
            DoorRun::insert_ignore(&client, new_run)
                .await
                .expect("insert run")
        );
    }
    // A replayed line lands nothing.
    assert!(
        !DoorRun::insert_ignore(
            &client,
            &run(diver.id, DoorRunResult::Death, 90_000, 24, 400)
        )
        .await
        .expect("replay run")
    );

    // A NetHack ascension for the diver: the boards partition by game, so it
    // must rank on the nethack triple and leak nothing into the DCSS one.
    let mut ascension = run(diver.id, DoorRunResult::Win, 3_000_000, 50, 600);
    ascension.game = DoorGame::Nethack.key();
    ascension.source_file = "xlogfile".to_string();
    assert!(
        DoorRun::insert_ignore(&client, &ascension)
            .await
            .expect("insert nethack run")
    );

    // A Brogue escape and a mastery for the winner: both results count on
    // the wins board (WINS = win + mastery).
    for (result, offset) in [(DoorRunResult::Win, 700), (DoorRunResult::Mastery, 800)] {
        let mut brogue_run = run(winner.id, result, 20_000, 26, offset);
        brogue_run.game = DoorGame::Brogue.key();
        brogue_run.source_file = "players/lb_door_winner/BrogueRunHistory.txt".to_string();
        assert!(
            DoorRun::insert_ignore(&client, &brogue_run)
                .await
                .expect("insert brogue run")
        );
    }

    let data = fetch_leaderboard_data(&client)
        .await
        .expect("fetch leaderboard");

    let nethack = data
        .door_board(DoorGame::Nethack)
        .expect("nethack boards present");
    assert_eq!(nethack.wins.len(), 1);
    assert_eq!(entry_for(&nethack.wins, diver.id).value, 1);
    assert_eq!(entry_for(&nethack.depth.all_time, diver.id).value, 50);

    let brogue = data
        .door_board(DoorGame::Brogue)
        .expect("brogue boards present");
    assert_eq!(brogue.wins.len(), 1);
    assert_eq!(entry_for(&brogue.wins, winner.id).value, 2);

    let boards = data
        .door_board(DoorGame::Dcss)
        .expect("dcss boards present");
    // The nethack win/depth/score stayed off the DCSS boards.
    assert!(boards.depth.all_time.iter().all(|entry| entry.value != 50));

    // Wins: only the winner's one win row; the quit pays nothing.
    assert_eq!(boards.wins.len(), 1);
    assert_eq!(entry_for(&boards.wins, winner.id).value, 1);

    // Dive: the winner's depth comes from the Orb milestone mark, not the
    // surface exit; the diver's from the death row.
    assert_eq!(entry_for(&boards.depth.all_time, winner.id).value, 27);
    assert_eq!(entry_for(&boards.depth.all_time, winner.id).rank, 1);
    assert_eq!(entry_for(&boards.depth.all_time, diver.id).value, 24);
    // Monthly window: the winner's 45-day-old death is out, the milestone in.
    assert_eq!(entry_for(&boards.depth.monthly, winner.id).value, 27);

    // Score: best per player; the quit's absurd score still counts as a
    // score (crawl scored the run), and only the monthly window drops the
    // winner's old death score.
    assert_eq!(
        entry_for(&boards.score.all_time, diver.id).value,
        999_999_999
    );
    assert_eq!(
        entry_for(&boards.score.all_time, winner.id).value,
        2_000_000
    );
    assert_eq!(entry_for(&boards.score.monthly, winner.id).value, 2_000_000);
}
