use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::{
    models::{
        le_word,
        leaderboard::{DailyPuzzle, RankedEntry, fetch_leaderboard_data},
        mud_character::MudCharacter,
        rubiks_cube, sudoku,
    },
    test_utils::{create_test_user, test_db},
};

fn entry_for(entries: &[RankedEntry], user_id: Uuid) -> &RankedEntry {
    entries
        .iter()
        .find(|entry| entry.user_id == user_id)
        .expect("user on board")
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
    assert_eq!(rival_points.value, 5, "2 le word + rubiks at medium weight");
    assert_eq!(rival_points.rank, 2);

    // The rubiks win is today's, so the status projection must report it
    // under the fixed 'daily' difficulty.
    let rival_status = data
        .user_daily_statuses
        .get(&rival.id)
        .expect("rival has a daily status");
    assert!(rival_status.completed(DailyPuzzle::RubiksCube));
    assert!(rival_status.completed_difficulty(DailyPuzzle::RubiksCube, "daily"));
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
