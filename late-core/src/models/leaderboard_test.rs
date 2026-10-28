use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::{
    models::{
        chess_puzzle,
        chips::{ChipMove, Difficulty, UserChips},
        drink_round::Bar,
        drinks::{UserDrinks, WELCOME_DRINK_POINTS},
        le_word,
        leaderboard::{
            DailyPuzzle, DoorGame, LATEANIA_XP_AT_LEVEL_CAP, LATEANIA_XP_PER_PARAGON_LEVEL,
            LobbyGame, OnlineTimeIncrement, RankedEntry, ScoreGame, WeekBest, WeekGame,
            apply_online_time_batch, fetch_leaderboard_data, week_bests,
        },
        minesweeper,
        mud_character::MudCharacter,
        netwalk, nonogram, quartets, rubiks_cube, sliding_puzzle, snake, solitaire, sudoku, tetris,
        twenty_forty_eight, word_wheel,
    },
    test_utils::{
        create_test_user, roll_high_scores_back_a_month, roll_score_events_back_a_month, test_db,
    },
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
    assert_eq!(
        data.today_solver_names.get(&rival.id),
        Some(&rival.username),
        "today's solvers carry their username"
    );
}

/// The Arcade Wins weights, pinned whole: each comes from the puzzle's
/// `tiers()`, so a tier moved in the roster shows up here as a changed
/// string, never as a quietly different board.
#[test]
fn points_sql_follows_each_puzzles_tiers() {
    let three =
        "CASE difficulty_key WHEN 'easy' THEN 1 WHEN 'medium' THEN 3 WHEN 'hard' THEN 5 ELSE 0 END";
    let points: Vec<(DailyPuzzle, String)> = DailyPuzzle::ALL
        .iter()
        .map(|puzzle| (*puzzle, puzzle.points_sql()))
        .collect();
    assert_eq!(
        points,
        vec![
            (DailyPuzzle::LeWord, "1".to_string()),
            (DailyPuzzle::RubiksCube, "3".to_string()),
            (DailyPuzzle::SlidingPuzzle, three.to_string()),
            (DailyPuzzle::Sudoku, three.to_string()),
            (DailyPuzzle::Nonogram, three.to_string()),
            (DailyPuzzle::Minesweeper, three.to_string()),
            (
                DailyPuzzle::Solitaire,
                "CASE difficulty_key WHEN 'draw-1' THEN 3 WHEN 'draw-3' THEN 5 ELSE 0 END"
                    .to_string()
            ),
            (DailyPuzzle::Netwalk, three.to_string()),
            (DailyPuzzle::ChessPuzzle, three.to_string()),
            (DailyPuzzle::WordWheel, three.to_string()),
            (DailyPuzzle::Quartets, "3".to_string()),
        ]
    );
}

/// One win of `puzzle` on its board `difficulty_key`, through the
/// puzzle's own model: the win insert that has to carry the rollup bump.
async fn record_daily_win(
    client: &tokio_postgres::Client,
    puzzle: DailyPuzzle,
    user_id: Uuid,
    difficulty_key: &str,
    tier: Difficulty,
    day: NaiveDate,
) {
    let key = difficulty_key.to_string();
    match puzzle {
        DailyPuzzle::LeWord => {
            le_word::DailyWin::record_win(client, user_id, day, 4)
                .await
                .expect("le word win");
        }
        DailyPuzzle::RubiksCube => {
            rubiks_cube::DailyWin::record_win(client, user_id, day)
                .await
                .expect("rubiks cube win");
        }
        DailyPuzzle::SlidingPuzzle => {
            sliding_puzzle::DailyWin::record_win(client, user_id, tier, day, 30)
                .await
                .expect("sliding puzzle win");
        }
        DailyPuzzle::Sudoku => {
            sudoku::DailyWin::record_win(client, user_id, key, day, 100)
                .await
                .expect("sudoku win");
        }
        DailyPuzzle::Nonogram => {
            nonogram::DailyWin::record_win(client, user_id, key, day)
                .await
                .expect("nonogram win");
        }
        DailyPuzzle::Minesweeper => {
            minesweeper::DailyWin::record_win(client, user_id, key, day, 100)
                .await
                .expect("minesweeper win");
        }
        DailyPuzzle::Solitaire => {
            solitaire::DailyWin::record_win(client, user_id, key, day, 100)
                .await
                .expect("solitaire win");
        }
        DailyPuzzle::Netwalk => {
            netwalk::DailyWin::record_win(client, user_id, tier, day, 30)
                .await
                .expect("netwalk win");
        }
        DailyPuzzle::ChessPuzzle => {
            chess_puzzle::DailyWin::record_win(client, user_id, tier, day, 1)
                .await
                .expect("chess puzzle win");
        }
        DailyPuzzle::WordWheel => {
            word_wheel::DailyWin::record_win(client, user_id, tier, day, 40)
                .await
                .expect("word wheel win");
        }
        DailyPuzzle::Quartets => {
            quartets::DailyWin::record_win(client, user_id, day, 1)
                .await
                .expect("quartets win");
        }
    }
}

/// The win inserts are the one thing the roster cannot generate: each must
/// compose `bump_daily_win_total_sql`, or the puzzle's all-time board stays
/// empty. Every puzzle's every board, won once and then replayed, counts
/// one win per board in the rollup.
#[tokio::test]
async fn every_daily_win_insert_bumps_the_all_time_rollup_once() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let solver = create_test_user(&test_db.db, "lb_rollup_every_daily").await;
    let today = Utc::now().date_naive();

    for puzzle in DailyPuzzle::ALL {
        let tiers = puzzle.tiers().list();
        for (difficulty_key, tier) in &tiers {
            for _ in 0..2 {
                record_daily_win(&client, *puzzle, solver.id, difficulty_key, *tier, today).await;
            }
        }
        // Raw witness on the rollup itself.
        let wins: i64 = client
            .query_one(
                "SELECT wins FROM daily_win_totals WHERE game = $1 AND user_id = $2",
                &[&puzzle.key(), &solver.id],
            )
            .await
            .unwrap_or_else(|error| panic!("{puzzle:?} rollup row: {error}"))
            .get(0);
        assert_eq!(wins, tiers.len() as i64, "{puzzle:?}");
    }
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

/// The Lateania boards read the game-owned character blobs. Adventurers ranks
/// by level with experience as the tiebreak and the class as the row note, and
/// keeps counting past the level cap: every `LATEANIA_XP_PER_PARAGON_LEVEL` of
/// xp beyond the cap's threshold is one more level on the board, so capped
/// characters still separate. PvP ranks lifetime Wildbound Waste kills. A
/// pre-class-select shell, or a character that never killed a rival, stays off.
#[tokio::test]
async fn lateania_boards_rank_living_characters() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let paragon = create_test_user(&test_db.db, "lb_lateania_paragon").await;
    let capped = create_test_user(&test_db.db, "lb_lateania_capped").await;
    let hero = create_test_user(&test_db.db, "lb_lateania_hero").await;
    let rival = create_test_user(&test_db.db, "lb_lateania_rival").await;
    let shell = create_test_user(&test_db.db, "lb_lateania_shell").await;

    let save = |user_id: Uuid, data: serde_json::Value| {
        let client = &client;
        async move {
            MudCharacter::save(client, user_id, 0, data)
                .await
                .expect("save character");
        }
    };
    // 37 paragon levels and a bit past the cap's threshold.
    save(
        paragon.id,
        json!({
            "version": 17, "class": "warrior", "level": 100,
            "xp": LATEANIA_XP_AT_LEVEL_CAP + 37 * LATEANIA_XP_PER_PARAGON_LEVEL + 10,
            "pvp_kills": 3,
        }),
    )
    .await;
    // Exactly at the cap: level 100, no paragon levels yet.
    save(
        capped.id,
        json!({
            "version": 17, "class": "druid", "level": 100,
            "xp": LATEANIA_XP_AT_LEVEL_CAP + LATEANIA_XP_PER_PARAGON_LEVEL - 1,
            "pvp_kills": 12,
        }),
    )
    .await;
    // Hero and rival share level 42; the hero's higher experience breaks the
    // tie. The rival never killed anyone.
    save(
        hero.id,
        json!({ "version": 17, "class": "runemaster", "level": 42, "xp": 900_000, "pvp_kills": 3 }),
    )
    .await;
    save(
        rival.id,
        json!({ "version": 17, "class": "warrior", "level": 42, "xp": 800_000, "pvp_kills": 0 }),
    )
    .await;
    save(
        shell.id,
        json!({ "version": 17, "class": null, "level": 1, "xp": 0, "pvp_kills": 5 }),
    )
    .await;

    let data = fetch_leaderboard_data(&client)
        .await
        .expect("fetch leaderboard");

    let adventurers = &data.lateania_adventurers;
    assert_eq!(entry_for(adventurers, paragon.id).value, 137);
    assert_eq!(entry_for(adventurers, paragon.id).rank, 1);
    assert_eq!(entry_for(adventurers, capped.id).value, 100);
    assert_eq!(entry_for(adventurers, capped.id).rank, 2);
    let hero_row = entry_for(adventurers, hero.id);
    assert_eq!(hero_row.rank, 3, "experience breaks the level tie");
    assert_eq!(hero_row.value, 42);
    assert_eq!(hero_row.note.as_deref(), Some("Runemaster"));
    assert_eq!(entry_for(adventurers, rival.id).rank, 4);
    assert!(
        !adventurers.iter().any(|entry| entry.user_id == shell.id),
        "a character without a chosen class stays off the board"
    );

    let pvp = &data.lateania_pvp;
    assert_eq!(entry_for(pvp, capped.id).value, 12);
    assert_eq!(entry_for(pvp, capped.id).rank, 1);
    assert_eq!(entry_for(pvp, paragon.id).rank, 2);
    assert_eq!(entry_for(pvp, hero.id).rank, 2, "equal kills share a rank");
    assert!(
        !pvp.iter()
            .any(|entry| entry.user_id == rival.id || entry.user_id == shell.id),
        "no kills, or no class, no PvP row"
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

/// Top Chips ranks what a player earned. Spending is a debit and never
/// counts, whatever it bought, so a heavy spender who out-earns everyone
/// tops the board instead of vanishing under a negative net; the tables
/// and gifts stay off it too. `month_figures` is the
/// same sum, so the profile figure agrees with the board; its net is every
/// row, stipend included.
#[tokio::test]
async fn top_chips_counts_earnings_and_never_spending() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let spender = create_test_user(&test_db.db, "lb_chip_spender").await;
    let saver = create_test_user(&test_db.db, "lb_chip_saver").await;
    let apply = |user_id, mv, amount| UserChips::apply(&**client, user_id, mv, amount, "lb-test");
    // Balances big enough that every move below is affordable; the stipend
    // row `ensure` writes on the way in is not an earning either.
    UserChips::ensure(&client, spender.id)
        .await
        .expect("spender chips");
    UserChips::ensure(&client, saver.id)
        .await
        .expect("saver chips");
    client
        .execute(
            "UPDATE user_chips SET balance = 100000 WHERE user_id = ANY($1)",
            &[&vec![spender.id, saver.id]],
        )
        .await
        .expect("fund accounts");

    // Spender: 3,200 earned, 11,300 spent, plus every excluded credit.
    apply(spender.id, ChipMove::DailyPuzzleWin, 500)
        .await
        .expect("puzzle win")
        .expect("affordable");
    apply(spender.id, ChipMove::QuestReward, 750)
        .await
        .expect("quest reward")
        .expect("affordable");
    apply(spender.id, ChipMove::GildReceived, 750)
        .await
        .expect("gild received")
        .expect("affordable");
    apply(spender.id, ChipMove::PotWon, 1_000)
        .await
        .expect("pot won")
        .expect("affordable");
    apply(spender.id, ChipMove::PotTicket, 1_000)
        .await
        .expect("pot ticket")
        .expect("affordable");
    apply(spender.id, ChipMove::ShopPurchase, 8_000)
        .await
        .expect("shop")
        .expect("affordable");
    apply(spender.id, ChipMove::RoundPurchase, 1_600)
        .await
        .expect("round")
        .expect("affordable");
    apply(spender.id, ChipMove::DrinkPurchase, 100)
        .await
        .expect("drink")
        .expect("affordable");
    apply(spender.id, ChipMove::GildSent, 500)
        .await
        .expect("gild sent")
        .expect("affordable");
    apply(spender.id, ChipMove::SsnakeArenaLost, 100)
        .await
        .expect("arena lost")
        .expect("affordable");
    apply(spender.id, ChipMove::PokerPayout, 5_000)
        .await
        .expect("poker payout")
        .expect("affordable");
    apply(spender.id, ChipMove::BlackjackPayout, 5_000)
        .await
        .expect("blackjack payout")
        .expect("affordable");
    apply(spender.id, ChipMove::GiftReceived, 5_000)
        .await
        .expect("gift received")
        .expect("affordable");
    apply(spender.id, ChipMove::BonsaiWatered, 200)
        .await
        .expect("bonsai")
        .expect("affordable");
    // Saver: 2,000 earned, nothing spent.
    apply(saver.id, ChipMove::DailyPuzzleWin, 2_000)
        .await
        .expect("saver win")
        .expect("affordable");

    let data = fetch_leaderboard_data(&client)
        .await
        .expect("fetch leaderboard");
    let board = &data.monthly_chip_earners;
    let spender_row = entry_for(board, spender.id);
    assert_eq!(spender_row.value, 3_200, "spending must not subtract");
    assert_eq!(entry_for(board, saver.id).value, 2_000);
    assert!(
        spender_row.rank < entry_for(board, saver.id).rank,
        "the bigger earner ranks first however much they spent"
    );
    let month = UserChips::month_figures(&client, spender.id)
        .await
        .expect("month figures");
    assert_eq!(
        month.earned, 3_200,
        "the profile figure is the board figure"
    );
    // 1,000 stipend + 18,200 credited - 11,300 spent.
    assert_eq!(month.net, 7_900, "the net is every row");
}

/// A best score belongs to the month it was set in. Playing again without
/// beating it must not carry the old best onto this month's board: the
/// monthly window reads the best-score tables by `updated`, so `updated`
/// may only move when the best does.
#[tokio::test]
async fn an_old_best_stays_off_the_monthly_board_when_the_player_plays_again() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let veteran = create_test_user(&test_db.db, "score-veteran").await;

    tetris::HighScore::update_score_if_higher(&client, veteran.id, 9_000)
        .await
        .expect("lateris best");
    twenty_forty_eight::HighScore::update_score_if_higher(&client, veteran.id, 9_000)
        .await
        .expect("2048 best");
    snake::HighScore::update_score_if_higher(&client, veteran.id, 9_000)
        .await
        .expect("snake best");
    for table in [
        "tetris_high_scores",
        "twenty_forty_eight_high_scores",
        "snake_high_scores",
    ] {
        roll_high_scores_back_a_month(&client, table).await;
    }

    // This month: a run still in progress, nowhere near the old best.
    tetris::HighScore::update_score_if_higher(&client, veteran.id, 50)
        .await
        .expect("lateris run");
    twenty_forty_eight::HighScore::update_score_if_higher(&client, veteran.id, 50)
        .await
        .expect("2048 run");
    snake::HighScore::update_score_if_higher(&client, veteran.id, 50)
        .await
        .expect("snake run");

    let data = fetch_leaderboard_data(&client)
        .await
        .expect("fetch leaderboard");
    for game in [
        ScoreGame::Lateris,
        ScoreGame::TwentyFortyEight,
        ScoreGame::Snake,
    ] {
        let board = data.score_board(game).expect("score board");
        let monthly: Vec<i64> = board
            .monthly
            .iter()
            .filter(|entry| entry.user_id == veteran.id)
            .map(|entry| entry.value)
            .collect();
        assert_eq!(
            monthly,
            Vec::<i64>::new(),
            "{game:?}: last month's best is not this month's score"
        );
        assert_eq!(entry_for(&board.all_time, veteran.id).value, 9_000);
    }
}

/// Migration 236 repairs the bests the old submit path re-stamped. When a
/// score event reached a best before its `updated` stamp, the best was set
/// back then, whatever the stamp says, so it leaves this month's board. A
/// best set this month stays on it, whether its run is still going or
/// finished with its event written after the best.
#[tokio::test]
async fn the_timestamp_repair_takes_restamped_bests_off_the_monthly_board() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let veteran = create_test_user(&test_db.db, "score-restamped").await;
    let climber = create_test_user(&test_db.db, "score-climber").await;
    let finisher = create_test_user(&test_db.db, "score-finisher").await;

    // Last month: the veteran finishes a 9,000 run, the climber a 500 run.
    for (user_id, score) in [(veteran.id, 9_000), (climber.id, 500)] {
        tetris::HighScore::update_score_if_higher(&client, user_id, score)
            .await
            .expect("lateris best");
        tetris::HighScore::record_score_event(&client, user_id, score)
            .await
            .expect("lateris event");
        twenty_forty_eight::HighScore::update_score_if_higher(&client, user_id, score)
            .await
            .expect("2048 best");
        twenty_forty_eight::HighScore::record_score_event(&client, user_id, score)
            .await
            .expect("2048 event");
        snake::HighScore::update_score_if_higher(&client, user_id, score)
            .await
            .expect("snake best");
        snake::HighScore::record_score_event(&client, user_id, score)
            .await
            .expect("snake event");
    }
    let tables = [
        "tetris_high_scores",
        "twenty_forty_eight_high_scores",
        "snake_high_scores",
    ];
    for table in tables {
        roll_high_scores_back_a_month(&client, table).await;
    }
    roll_score_events_back_a_month(&client).await;

    // This month: the veteran plays without beating 9,000, and the old
    // submit path stamped `updated` anyway.
    for table in tables {
        client
            .execute(
                &format!("UPDATE {table} SET updated = current_timestamp WHERE user_id = $1"),
                &[&veteran.id],
            )
            .await
            .expect("old submit stamp");
    }
    // The climber is mid-run past their old best; the finisher ends a 700 run.
    tetris::HighScore::update_score_if_higher(&client, climber.id, 2_000)
        .await
        .expect("lateris run");
    twenty_forty_eight::HighScore::update_score_if_higher(&client, climber.id, 2_000)
        .await
        .expect("2048 run");
    snake::HighScore::update_score_if_higher(&client, climber.id, 2_000)
        .await
        .expect("snake run");
    tetris::HighScore::update_score_if_higher(&client, finisher.id, 700)
        .await
        .expect("lateris best");
    tetris::HighScore::record_score_event(&client, finisher.id, 700)
        .await
        .expect("lateris event");
    twenty_forty_eight::HighScore::update_score_if_higher(&client, finisher.id, 700)
        .await
        .expect("2048 best");
    twenty_forty_eight::HighScore::record_score_event(&client, finisher.id, 700)
        .await
        .expect("2048 event");
    snake::HighScore::update_score_if_higher(&client, finisher.id, 700)
        .await
        .expect("snake best");
    snake::HighScore::record_score_event(&client, finisher.id, 700)
        .await
        .expect("snake event");

    client
        .batch_execute(include_str!(
            "../../migrations/236_repair_best_score_timestamps.sql"
        ))
        .await
        .expect("timestamp repair");

    let data = fetch_leaderboard_data(&client)
        .await
        .expect("fetch leaderboard");
    for game in [
        ScoreGame::Lateris,
        ScoreGame::TwentyFortyEight,
        ScoreGame::Snake,
    ] {
        let board = data.score_board(game).expect("score board");
        let monthly: Vec<(Uuid, i64)> = board
            .monthly
            .iter()
            .filter(|entry| [veteran.id, climber.id, finisher.id].contains(&entry.user_id))
            .map(|entry| (entry.user_id, entry.value))
            .collect();
        assert_eq!(
            monthly,
            vec![(climber.id, 2_000), (finisher.id, 700)],
            "{game:?}: only bests set this month"
        );
        assert_eq!(entry_for(&board.all_time, veteran.id).value, 9_000);
    }
}

/// Top Drinkers sums the buzz of every drink taken: paid or comped, either
/// bar, past the buzz cap, this month and this year. The welcome pour is not
/// a drink anybody took, non-intoxicating pours add no points, and last
/// year's drinks are off both windows.
#[tokio::test]
async fn top_drinkers_sums_buzz_by_month_and_year() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let binger = create_test_user(&test_db.db, "drinkers-binger").await;
    let steady = create_test_user(&test_db.db, "drinkers-steady").await;
    let newcomer = create_test_user(&test_db.db, "drinkers-newcomer").await;
    let lapsed = create_test_user(&test_db.db, "drinkers-lapsed").await;
    let sober = create_test_user(&test_db.db, "drinkers-sober").await;

    // Three big pours: the stored buzz caps at 4000, the board does not.
    for _ in 0..3 {
        UserDrinks::record_purchase(&client, binger.id, Bar::Tavern, 2_000, true)
            .await
            .expect("pour");
    }
    UserDrinks::record_purchase(&client, steady.id, Bar::Nightcap, 250, true)
        .await
        .expect("pour");
    UserDrinks::record_comped_pour(&client, steady.id, Bar::Tavern, 400, true)
        .await
        .expect("round credit");
    UserDrinks::record_purchase(&client, steady.id, Bar::Nightcap, 1_000, true)
        .await
        .expect("pour");
    UserDrinks::record_welcome_pour(&client, newcomer.id, WELCOME_DRINK_POINTS)
        .await
        .expect("welcome");
    UserDrinks::record_purchase(&client, lapsed.id, Bar::Tavern, 1_000, true)
        .await
        .expect("pour");
    for user in [&steady, &sober] {
        UserDrinks::record_purchase(&client, user.id, Bar::Tavern, 50, false)
            .await
            .expect("coffee");
        UserDrinks::record_comped_pour(&client, user.id, Bar::Nightcap, 400, false)
            .await
            .expect("comped water");
    }

    // steady's top shelf was the first night of the year; lapsed drank last
    // year. Only drink_pours is moved: it is the table the board reads.
    client
        .execute(
            "UPDATE drink_pours
             SET created = date_trunc('year', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
             WHERE user_id = $1 AND points = 1000",
            &[&steady.id],
        )
        .await
        .expect("backdate to new year");
    client
        .execute(
            "UPDATE drink_pours
             SET created = date_trunc('year', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
                 - interval '1 day'
             WHERE user_id = $1",
            &[&lapsed.id],
        )
        .await
        .expect("backdate to last year");

    let data = fetch_leaderboard_data(&client).await.expect("data");
    let monthly: Vec<(Uuid, i64, i64)> = data
        .top_drinkers
        .monthly
        .iter()
        .map(|entry| (entry.user_id, entry.rank, entry.value))
        .collect();
    let yearly: Vec<(Uuid, i64, i64)> = data
        .top_drinkers
        .yearly
        .iter()
        .map(|entry| (entry.user_id, entry.rank, entry.value))
        .collect();

    // In January the first night of the year is also this month.
    let steady_monthly = match Utc::now().month() {
        1 => 1_650,
        _ => 650,
    };
    assert_eq!(
        monthly,
        vec![(binger.id, 1, 6_000), (steady.id, 2, steady_monthly)]
    );
    assert_eq!(yearly, vec![(binger.id, 1, 6_000), (steady.id, 2, 1_650)]);
}

/// A Lobby match `winner` won this week, settled as `win_payout`.
async fn lobby_win(
    client: &crate::db::PooledClient,
    game: LobbyGame,
    winner: Uuid,
    loser: Uuid,
    win_payout: &str,
) {
    use crate::models::daily_match::{DailyMatch, DailyResult};
    let state = json!({});
    let challenge = DailyMatch::create_challenge(client, game.kind(), winner)
        .await
        .expect("create challenge");
    DailyMatch::claim(
        client,
        challenge.id,
        loser,
        winner,
        Utc::now() + Duration::hours(1),
        &state,
    )
    .await
    .expect("claim")
    .expect("challenge was open");
    DailyMatch::finish(
        client,
        challenge.id,
        Some(winner),
        DailyResult::Resign,
        &state,
        0,
    )
    .await
    .expect("finish");
    DailyMatch::set_win_payout(client, challenge.id, win_payout)
        .await
        .expect("settle");
}

/// The week's scoreboard: per game, the most daily wins over the week's
/// puzzle dates, the top score among the week's events, the most Lobby
/// wins that paid, and the top door run of the week, in roster order,
/// each one person's best and nobody named; a win or a score outside the
/// week is not on it, nor a Lobby win that did not pay, and a game nobody
/// played has no line.
#[tokio::test]
async fn the_weeks_best_counts_only_the_week() {
    use crate::models::door_run::{DoorRun, DoorRunResult, NewDoorRun};
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let solver = create_test_user(&test_db.db, "week_solver").await;
    let rival = create_test_user(&test_db.db, "week_rival").await;
    let last = Utc::now().date_naive();

    tetris::HighScore::record_score_event(&client, rival.id, 50_000)
        .await
        .expect("old score");
    roll_score_events_back_a_month(&client).await;
    for offset in [0, 2, 6] {
        le_word::DailyWin::record_win(&client, solver.id, last - Duration::days(offset), 4)
            .await
            .expect("solver win");
    }
    for offset in [7, 8, 9] {
        le_word::DailyWin::record_win(&client, rival.id, last - Duration::days(offset), 4)
            .await
            .expect("rival's wins the week before");
    }
    le_word::DailyWin::record_win(&client, rival.id, last, 4)
        .await
        .expect("rival win");
    sudoku::DailyWin::record_win(&client, rival.id, "hard".to_string(), last, 100)
        .await
        .expect("rival sudoku");
    tetris::HighScore::record_score_event(&client, solver.id, 500)
        .await
        .expect("solver score");
    tetris::HighScore::record_score_event(&client, rival.id, 900)
        .await
        .expect("rival score");
    lobby_win(&client, LobbyGame::Chess, solver.id, rival.id, "paid").await;
    for _ in 0..2 {
        lobby_win(&client, LobbyGame::Chess, rival.id, solver.id, "unplayed").await;
    }
    for (offset, score, ended_days_ago) in [(1, 99_999, 10), (2, 4_000, 1)] {
        DoorRun::insert_ignore(
            &client,
            &NewDoorRun {
                game: DoorGame::Dcss.key(),
                user_id: rival.id,
                ended_at: Utc::now() - Duration::days(ended_days_ago),
                result: DoorRunResult::Death,
                score: Some(score),
                depth: Some(3),
                turns: Some(100),
                raw: json!({}),
                source_file: "week".to_string(),
                source_offset: offset,
            },
        )
        .await
        .expect("door run");
    }

    let best = |game, value| WeekBest { game, value };
    assert_eq!(
        week_bests(&client, last).await.expect("week"),
        vec![
            best(WeekGame::Daily(DailyPuzzle::LeWord), 3),
            best(WeekGame::Daily(DailyPuzzle::Sudoku), 1),
            best(WeekGame::Score(ScoreGame::Lateris), 900),
            best(WeekGame::Lobby(LobbyGame::Chess), 1),
            best(WeekGame::Door(DoorGame::Dcss), 4_000),
        ]
    );
    let shown: Vec<String> = [
        best(WeekGame::Daily(DailyPuzzle::Sudoku), 1),
        best(WeekGame::Daily(DailyPuzzle::Sudoku), 6),
        best(WeekGame::Score(ScoreGame::Snake), 900),
        best(WeekGame::Score(ScoreGame::Snake), 1_234_567),
        best(WeekGame::Lobby(LobbyGame::Snooker), 2),
        best(WeekGame::Door(DoorGame::Nethack), 52_340),
    ]
    .iter()
    .map(WeekBest::shown_value)
    .collect();
    assert_eq!(
        shown,
        vec!["1 win", "6 wins", "900", "1,234,567", "2 wins", "52,340"]
    );
}

/// A day's tallies: per game anyone played, the finishes inside the UTC
/// day (the openings, for a door) and what the game calls a win (every
/// daily solve, a Lobby match with a winner, a door run that ascended; a
/// score game has none), in roster order. A play on another day is not
/// counted, a game nobody played has no line, a door won on a day it was
/// not opened keeps its line, and the shelf is the roster's.
#[tokio::test]
async fn a_days_tallies_count_the_days_finishes_by_game() {
    use crate::models::door_opening::{DoorCard, DoorOpening};
    use crate::models::door_run::{DoorRun, DoorRunResult, NewDoorRun};
    use crate::models::leaderboard::{DayGame, DayTally, Shelf, day_tallies};
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let solver = create_test_user(&test_db.db, "day_solver").await;
    let rival = create_test_user(&test_db.db, "day_rival").await;
    let today = Utc::now().date_naive();

    tetris::HighScore::record_score_event(&client, rival.id, 50_000)
        .await
        .expect("old score");
    roll_score_events_back_a_month(&client).await;
    for (user, offset) in [(solver.id, 0), (rival.id, 0), (solver.id, 3)] {
        le_word::DailyWin::record_win(&client, user, today - Duration::days(offset), 4)
            .await
            .expect("le word");
    }
    sudoku::DailyWin::record_win(&client, rival.id, "hard".to_string(), today, 100)
        .await
        .expect("sudoku");
    tetris::HighScore::record_score_event(&client, solver.id, 500)
        .await
        .expect("score");
    lobby_win(&client, LobbyGame::Chess, solver.id, rival.id, "paid").await;
    lobby_win(&client, LobbyGame::Chess, rival.id, solver.id, "unplayed").await;
    for (offset, result, ended_days_ago) in [
        (1, DoorRunResult::Death, 0),
        (2, DoorRunResult::Win, 0),
        (3, DoorRunResult::Death, 2),
    ] {
        DoorRun::insert_ignore(
            &client,
            &NewDoorRun {
                game: DoorGame::Nethack.key(),
                user_id: rival.id,
                ended_at: Utc::now() - Duration::days(ended_days_ago),
                result,
                score: Some(1_000),
                depth: Some(3),
                turns: Some(100),
                raw: json!({}),
                source_file: "day".to_string(),
                source_offset: offset,
            },
        )
        .await
        .expect("door run");
    }
    let now = Utc::now();
    for (card, days_ago) in [
        (DoorCard::Nethack, 0),
        (DoorCard::Nethack, 0),
        (DoorCard::Lateania, 0),
        (DoorCard::Brogue, 3),
    ] {
        DoorOpening::record(&client, solver.id, card, now - Duration::days(days_ago))
            .await
            .expect("opening");
    }
    DoorRun::insert_ignore(
        &client,
        &NewDoorRun {
            game: DoorGame::Dcss.key(),
            user_id: rival.id,
            ended_at: now,
            result: DoorRunResult::Win,
            score: Some(2_000_000),
            depth: Some(1),
            turns: Some(90_000),
            raw: json!({}),
            source_file: "day".to_string(),
            source_offset: 4,
        },
    )
    .await
    .expect("a win opened the day before");

    let tally = |game, plays, won| DayTally { game, plays, won };
    assert_eq!(
        day_tallies(&client, today).await.expect("day"),
        vec![
            tally(DayGame::Daily(DailyPuzzle::LeWord), 3, 3),
            tally(DayGame::Daily(DailyPuzzle::Sudoku), 1, 1),
            tally(DayGame::Score(ScoreGame::Lateris), 1, 0),
            tally(DayGame::Lobby(LobbyGame::Chess), 2, 2),
            tally(DayGame::Door(DoorCard::Lateania), 1, 0),
            tally(DayGame::Door(DoorCard::Nethack), 2, 1),
            tally(DayGame::Door(DoorCard::Dcss), 0, 1),
        ],
        "Lateania: one opening, a card with no runs to win; NetHack: two openings today, one run won today, the deaths uncounted; DCSS: a run won today and no opening keeps its line; Brogue was opened on another day"
    );
    assert!(
        day_tallies(&client, today - Duration::days(10))
            .await
            .expect("a quiet day")
            .is_empty()
    );
    let shelves: Vec<Shelf> = [
        WeekGame::Daily(DailyPuzzle::Sudoku),
        WeekGame::Score(ScoreGame::Snake),
        WeekGame::Lobby(LobbyGame::Snooker),
        WeekGame::Door(DoorGame::Dcss),
    ]
    .iter()
    .map(|game| game.shelf())
    .collect();
    assert_eq!(
        shelves,
        vec![Shelf::Arcade, Shelf::Arcade, Shelf::Lobby, Shelf::Doors]
    );
    let day_shelves: Vec<Shelf> = [
        DayGame::Daily(DailyPuzzle::Sudoku),
        DayGame::Score(ScoreGame::Snake),
        DayGame::Lobby(LobbyGame::Snooker),
        DayGame::Door(DoorCard::Zork),
    ]
    .iter()
    .map(|game| game.shelf())
    .collect();
    assert_eq!(
        day_shelves,
        vec![Shelf::Arcade, Shelf::Arcade, Shelf::Lobby, Shelf::Doors],
        "every hub card is a door of the day's shelf"
    );
}

/// A shelf's most played game is the one with the most plays, the first
/// in tally order on a tie, never one with none; the arcade's comes with
/// its finishes and as an arcade game.
#[test]
fn the_most_played_game_of_a_shelf() {
    use crate::models::door_opening::DoorCard;
    use crate::models::leaderboard::{ArcadeGame, DayGame, DayTally, Shelf};
    let tally = |game, plays| DayTally {
        game,
        plays,
        won: 0,
    };
    let tallies = [
        tally(DayGame::Daily(DailyPuzzle::Sudoku), 3),
        tally(DayGame::Daily(DailyPuzzle::LeWord), 3),
        tally(DayGame::Score(ScoreGame::Snake), 2),
        tally(DayGame::Lobby(LobbyGame::Chess), 0),
        tally(DayGame::Door(DoorCard::Nethack), 9),
    ];
    assert_eq!(
        DayTally::most_played(&tallies, Shelf::Arcade),
        Some(&tallies[0]),
        "a tie goes to the first listed"
    );
    assert_eq!(DayTally::most_played(&tallies, Shelf::Lobby), None);
    assert_eq!(
        DayTally::most_played(&tallies, Shelf::Doors),
        Some(&tallies[4])
    );
    assert_eq!(
        DayTally::most_played_arcade(&tallies),
        Some((ArcadeGame::Puzzle(DailyPuzzle::Sudoku), 3))
    );
    assert_eq!(DayTally::most_played_arcade(&tallies[3..]), None);
}
