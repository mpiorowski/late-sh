use crate::app::arcade::ui::*;
use ratatui::layout::Rect;

#[test]
fn centered_rect_centers_inside_larger_area() {
    let area = Rect::new(2, 3, 80, 24);
    let centered = centered_rect(area, 30, 10);

    assert_eq!(centered, Rect::new(27, 10, 30, 10));
}

#[test]
fn centered_rect_clamps_to_available_area() {
    let area = Rect::new(2, 3, 80, 24);
    let centered = centered_rect(area, 100, 40);

    assert_eq!(centered, area);
}

#[test]
fn keys_line_does_not_pad_key_only_hints() {
    let line = keys_line(vec![("hjkl↕↔", ""), ("q", "exit")]);
    let text = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();

    assert_eq!(text, "hjkl↕↔ · q exit");
}

#[tokio::test]
async fn sliding_puzzle_card_renders_rewards_and_launches() {
    use crate::{
        app::common::primitives::Screen,
        test_helpers::{make_app, new_test_db, render_plain, strip_ansi},
    };
    use late_core::{
        models::{user::RightSidebarMode, user_ssh_key::KeyLayout},
        test_utils::create_test_user,
    };

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-ui-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "sliding-puzzle-ui-token");
    app.resize(80, 24).expect("resize test terminal");
    app.device_rails = Some(KeyLayout {
        room_list_mode: app.rail_modes().0,
        right_sidebar_mode: RightSidebarMode::On,
    });
    app.set_screen(Screen::Arcade);
    app.game_selection = late_core::models::leaderboard::ArcadeGame::Puzzle(
        late_core::models::leaderboard::DailyPuzzle::SlidingPuzzle,
    );

    let lobby = render_plain(&mut app);
    assert!(lobby.contains("Sliding Puzzle"));
    assert!(lobby.contains("Slide tiles into order by number or image."));
    assert!(lobby.contains("✗100✗250✗500"));

    app.handle_input(b"\r");
    assert!(app.is_playing_game);
    // The art view is the default; an empty gallery backlog says so and
    // keeps the numbered tiles playable.
    app.sliding_puzzle_state
        .set_art_for_test(crate::app::arcade::sliding_puzzle::svc::ArtLoad::Empty);
    let launched = render_plain(&mut app);
    assert!(launched.contains("No gallery art yet"), "{launched}");
    app.handle_input(b"i");
    let numbered = render_plain(&mut app);
    assert!(numbered.contains("Numbered tiles."), "{numbered}");

    app.handle_input(b"]");
    app.resize(120, 30).expect("resize wide game terminal");
    let game = render_plain(&mut app);
    assert!(game.contains("moves"));
    assert!(game.contains("hard 5×5"));
    assert!(game.contains("500 chips"));
    assert!(game.contains("hjkl"), "{game}");
    assert!(game.contains("[]change diff"), "{game}");
    assert!(game.contains("iart"), "{game}");
    assert!(game.contains("n/rnew/reset"), "{game}");
    assert!(game.contains("d/pdaily/personal"), "{game}");
    assert!(game.contains("qexit"));
    assert!(game.contains("24"));
    assert!(!game.contains("Terminal too small"));

    app.resize(80, 24).expect("resize narrow game terminal");
    let narrow = render_plain(&mut app);
    assert!(narrow.contains("hjkl"), "{narrow}");
    assert!(narrow.contains("[]diff"), "{narrow}");
    assert!(narrow.contains("iart"), "{narrow}");
    assert!(narrow.contains("rreset"), "{narrow}");
    assert!(narrow.contains("d/pmode"), "{narrow}");
    assert!(narrow.contains("qexit"), "{narrow}");
    // The board itself has to be on screen, not just the footer: the
    // too-small fallback would satisfy every key hint above it.
    assert!(narrow.contains("│24│"), "{narrow}");
    assert!(!narrow.contains("Terminal too small"), "{narrow}");

    // Back to the art view: a failed load keeps the numbered tiles and says
    // how to retry; a landed piece replaces them, credited under the grid.
    app.handle_input(b"i");
    app.sliding_puzzle_state
        .set_art_for_test(crate::app::arcade::sliding_puzzle::svc::ArtLoad::Failed);
    app.reset_render();
    let failed_art = strip_ansi(&String::from_utf8_lossy(
        &app.render().expect("render failed art view"),
    ));
    assert!(
        failed_art.contains("Art unavailable; i twice to retry."),
        "{failed_art}"
    );
    assert!(failed_art.contains("│24│"), "{failed_art}");

    let mut canvas = dartboard_core::Canvas::with_size(12, 4);
    for y in 0..4 {
        for x in 0..12 {
            canvas.set(dartboard_core::Pos { x, y }, '#');
        }
    }
    app.sliding_puzzle_state.set_art_for_test(
        crate::app::arcade::sliding_puzzle::svc::ArtLoad::Featured(
            crate::app::arcade::sliding_puzzle::art::PuzzleArt {
                title: "sunset".to_string(),
                username: "painter".to_string(),
                canvas,
                width: 12,
                height: 4,
            },
        ),
    );
    let mut unsolved = (1..=24).chain(std::iter::once(0)).collect::<Vec<_>>();
    unsolved.swap(0, 1);
    app.sliding_puzzle_state.set_board_for_test(
        late_core::models::chips::Difficulty::Hard,
        unsolved,
        1,
    );
    app.reset_render();
    let ready_art = strip_ansi(&String::from_utf8_lossy(
        &app.render().expect("render ready art view"),
    ));
    assert!(ready_art.contains("############"), "{ready_art}");
    assert!(ready_art.contains("sunset by @painter"), "{ready_art}");
    assert!(!ready_art.contains("│24│"), "{ready_art}");

    let solved = (1..=24).chain(std::iter::once(0)).collect();
    app.sliding_puzzle_state.set_board_for_test(
        late_core::models::chips::Difficulty::Hard,
        solved,
        2,
    );
    app.reset_render();
    let solved_art = strip_ansi(&String::from_utf8_lossy(
        &app.render().expect("render solved art view"),
    ));
    assert!(solved_art.contains("SOLVED"), "{solved_art}");

    app.handle_input(b"I");
    assert!(render_plain(&mut app).contains("Numbered tiles."));

    app.handle_input(b"p");
    let personal = render_plain(&mut app);
    assert!(personal.contains("personal"), "{personal}");
    assert!(personal.contains("reward none"), "{personal}");
    assert!(!personal.contains("500 chips"), "{personal}");
    assert!(personal.contains("qexit"), "{personal}");
    assert!(
        personal.contains("Personal board. No reward; n starts a new scramble."),
        "{personal}"
    );
    assert_eq!(crate::app::workspace::arcade::active_daily_stop(&app), None);
}

/// The lobby card must turn green the moment this session banks a daily,
/// not on the next five-minute leaderboard pass. The signal is the session's
/// own `GameWon` Activity event, which the services publish only after the
/// win row commits; another player's win, and a win stamped on yesterday's
/// board, leave today's marks alone.
#[tokio::test]
async fn a_daily_win_turns_the_lobby_mark_green_before_the_leaderboard_refresh() {
    use crate::{
        app::{
            activity::event::{ActivityEvent, ActivityGame},
            common::primitives::Screen,
        },
        test_helpers::{make_app, new_test_db, render_plain},
    };
    use chrono::{Duration, Utc};
    use late_core::{
        models::{user::RightSidebarMode, user_ssh_key::KeyLayout},
        test_utils::create_test_user,
    };
    use tokio::sync::broadcast;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-instant-mark").await;
    let rival = create_test_user(&test_db.db, "sliding-puzzle-instant-rival").await;
    let mut app = make_app(test_db.db.clone(), user.id, "sliding-puzzle-instant-token");
    app.resize(80, 24).expect("resize test terminal");
    app.device_rails = Some(KeyLayout {
        room_list_mode: app.rail_modes().0,
        right_sidebar_mode: RightSidebarMode::On,
    });
    app.set_screen(Screen::Arcade);
    app.game_selection = late_core::models::leaderboard::ArcadeGame::Puzzle(
        late_core::models::leaderboard::DailyPuzzle::SlidingPuzzle,
    );
    let (activity_tx, activity_rx) = broadcast::channel::<ActivityEvent>(8);
    app.activity_feed_rx = Some(activity_rx);

    let today = Utc::now().date_naive();
    activity_tx
        .send(ActivityEvent::game_won_at(
            rival.id,
            "rival",
            ActivityGame::SlidingPuzzle,
            Some("hard".to_string()),
            Some(90),
            ActivityEvent::occurred_on_utc_date(today),
        ))
        .expect("rival win");
    activity_tx
        .send(ActivityEvent::game_won_at(
            user.id,
            "me",
            ActivityGame::SlidingPuzzle,
            Some("medium".to_string()),
            Some(120),
            ActivityEvent::occurred_on_utc_date(today - Duration::days(1)),
        ))
        .expect("yesterday's win");
    app.tick();
    let untouched = render_plain(&mut app);
    assert!(untouched.contains("✗100✗250✗500"), "{untouched}");

    activity_tx
        .send(ActivityEvent::game_won_at(
            user.id,
            "me",
            ActivityGame::SlidingPuzzle,
            Some("easy".to_string()),
            Some(138),
            ActivityEvent::occurred_on_utc_date(today),
        ))
        .expect("today's win");
    assert!(app.tick(), "a banked daily is a frame worth painting");
    let marked = render_plain(&mut app);
    assert!(marked.contains("✓100✗250✗500"), "{marked}");
    assert!(
        marked.contains("✗100✗250✗500"),
        "the other tiered games stay unmarked: {marked}"
    );
}

/// The lobby card counts today's solvers from the leaderboard snapshot, and
/// `o` opens the roster over the selected daily: tiers hardest first, the
/// names grouped under them, Esc closes it. A solver of another daily is
/// neither counted nor listed.
#[tokio::test]
async fn the_lobby_counts_todays_solvers_and_o_opens_the_roster() {
    use std::{collections::HashMap, sync::Arc};

    use crate::{
        app::common::primitives::Screen,
        test_helpers::{make_app, new_test_db, render_plain},
    };
    use late_core::{
        models::{
            leaderboard::{ArcadeGame, DailyCompletionStatus, DailyPuzzle, LeaderboardData},
            user::RightSidebarMode,
            user_ssh_key::KeyLayout,
        },
        test_utils::create_test_user,
    };
    use uuid::Uuid;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "solvers-roster-viewer").await;
    let mut app = make_app(test_db.db.clone(), user.id, "solvers-roster-token");
    app.resize(120, 30).expect("resize test terminal");
    app.device_rails = Some(KeyLayout {
        room_list_mode: app.rail_modes().0,
        right_sidebar_mode: RightSidebarMode::On,
    });
    app.set_screen(Screen::Arcade);
    app.game_selection = ArcadeGame::Puzzle(DailyPuzzle::Sudoku);

    let alice = Uuid::from_u128(0xa11ce);
    let bob = Uuid::from_u128(0xb0b);
    let carol = Uuid::from_u128(0xca401);
    let mut alice_status = DailyCompletionStatus::default();
    alice_status.mark_completed(DailyPuzzle::Sudoku, "hard".to_string());
    alice_status.mark_completed(DailyPuzzle::Sudoku, "easy".to_string());
    let mut bob_status = DailyCompletionStatus::default();
    bob_status.mark_completed(DailyPuzzle::Sudoku, "easy".to_string());
    let mut carol_status = DailyCompletionStatus::default();
    carol_status.mark_completed(DailyPuzzle::Minesweeper, "hard".to_string());
    app.leaderboard = Arc::new(LeaderboardData {
        user_daily_statuses: HashMap::from([
            (alice, alice_status),
            (bob, bob_status),
            (carol, carol_status),
        ]),
        today_solver_names: HashMap::from([
            (alice, "alice-solves".to_string()),
            (bob, "bob-solves".to_string()),
            (carol, "carol-sweeps".to_string()),
        ]),
        ..LeaderboardData::default()
    });

    let lobby = render_plain(&mut app);
    assert!(lobby.contains("2 solved [o who]"), "{lobby}");
    assert!(
        lobby.contains("1 solved"),
        "minesweeper counts carol: {lobby}"
    );
    assert!(
        !lobby.contains("alice-solves"),
        "names stay off the cards: {lobby}"
    );

    app.handle_input(b"o");
    assert!(app.arcade_solvers_open);
    let roster = render_plain(&mut app);
    assert!(roster.contains("Sudoku · 2 solved today"), "{roster}");
    let hard = roster
        .find("hard · 500 chips · 1 solved")
        .expect("hard tier");
    let medium = roster
        .find("medium · 250 chips · nobody yet")
        .expect("medium tier");
    let easy = roster
        .find("easy · 100 chips · 2 solved")
        .expect("easy tier");
    assert!(hard < medium && medium < easy, "hardest first: {roster}");
    assert!(roster.contains("alice-solves"), "{roster}");
    assert!(roster.contains("bob-solves"), "{roster}");
    assert!(
        !roster.contains("carol-sweeps"),
        "another daily's solver: {roster}"
    );

    // Enter is swallowed while the roster is up: nothing launches behind it.
    app.handle_input(b"\r");
    assert!(!app.is_playing_game);
    assert!(app.arcade_solvers_open);

    // A lone Esc is held as a possible sequence prefix until the tick
    // times it out, so poll rather than assert on the spot.
    app.handle_input(b"\x1b");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.arcade_solvers_open {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for esc to close the roster"
        );
        app.tick();
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    let closed = render_plain(&mut app);
    assert!(!closed.contains("solved today"), "{closed}");
}
