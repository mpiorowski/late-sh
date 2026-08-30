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

#[test]
fn sliding_puzzle_uses_its_user_facing_title() {
    assert_eq!(
        game_title(crate::app::state::GAME_SELECTION_SLIDING_PUZZLE),
        "Sliding Puzzle"
    );
}

#[tokio::test]
async fn sliding_puzzle_card_renders_rewards_and_launches() {
    use crate::{
        app::common::primitives::Screen,
        test_helpers::{make_app, new_test_db, render_plain},
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
    app.game_selection = crate::app::state::GAME_SELECTION_SLIDING_PUZZLE;

    let lobby = render_plain(&mut app);
    assert!(lobby.contains("Sliding Puzzle"));
    assert!(lobby.contains("Slide numbered tiles into order."));
    assert!(lobby.contains("✗100✗250✗500"));

    app.handle_input(b"\r");
    assert!(app.is_playing_game);
    let launched = render_plain(&mut app);
    assert!(
        launched.contains("Slide a tile into the gap: direction key or click."),
        "{launched}"
    );

    app.handle_input(b"]");
    app.resize(120, 30).expect("resize wide game terminal");
    let game = render_plain(&mut app);
    assert!(game.contains("moves"));
    assert!(game.contains("hard 5×5"));
    assert!(game.contains("500 chips"));
    assert!(game.contains("hjkl"), "{game}");
    assert!(game.contains("[]change diff"), "{game}");
    assert!(game.contains("n/rnew/reset"), "{game}");
    assert!(game.contains("d/pdaily/personal"), "{game}");
    assert!(game.contains("qexit"));
    assert!(game.contains("24"));
    assert!(!game.contains("Terminal too small"));

    app.resize(80, 24).expect("resize narrow game terminal");
    let narrow = render_plain(&mut app);
    assert!(narrow.contains("hjkl"), "{narrow}");
    assert!(narrow.contains("[]change diff"), "{narrow}");
    assert!(narrow.contains("rreset"), "{narrow}");
    assert!(narrow.contains("d/pmode"), "{narrow}");
    assert!(narrow.contains("qexit"), "{narrow}");
    // The board itself has to be on screen, not just the footer: the
    // too-small fallback would satisfy every key hint above it.
    assert!(narrow.contains("│24│"), "{narrow}");
    assert!(!narrow.contains("Terminal too small"), "{narrow}");

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
    assert_eq!(super::workspace::active_daily_stop(&app), None);
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
    app.game_selection = crate::app::state::GAME_SELECTION_SLIDING_PUZZLE;
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
