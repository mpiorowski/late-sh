//! Pool key map tests, against a real board.
//!
//! The hazards in this key map are the keys it shares with the other boards,
//! and those only show through the whole dispatch: `board_input::handle_key`,
//! `pool_key`, and the shared map waiting behind it.

use std::collections::HashMap;

use late_core::test_utils::{TestDb, create_test_user};
use tokio::sync::broadcast;

use super::*;
use crate::app::activity::{event::ActivityEvent, publisher::ActivityPublisher};
use crate::app::common::primitives::Screen;
use crate::app::games::chips::svc::ChipService;
use crate::app::lobby::daily::{
    board_input, games::DailyGame, state::BoardEntry, svc::DailyService,
};
use crate::test_helpers::{
    make_app, new_test_db, wait_for_render_contains, wait_for_render_not_contains, wait_until,
};

/// An eight-ball match nobody has broken yet, open on the breaker's board.
async fn unbroken_rack(name: &str) -> (TestDb, App) {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, &format!("{name}-challenger")).await;
    let claimer = create_test_user(&test_db.db, &format!("{name}-claimer")).await;
    let (activity_tx, _activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let svc = DailyService::new(
        test_db.db.clone(),
        ChipService::new(test_db.db.clone()),
        ActivityPublisher::new(test_db.db.clone(), activity_tx),
    );
    let posted = svc
        .post_challenge(challenger.id, DailyGame::EightBall)
        .await
        .expect("post");
    let claimed = svc
        .claim_challenge(claimer.id, posted.id)
        .await
        .expect("claim");
    let breaker = claimed.turn_user_id.expect("somebody breaks");

    let mut app = make_app(test_db.db.clone(), breaker, name);
    app.daily.open_board_inner(
        claimed.id,
        DailyGame::EightBall,
        HashMap::new(),
        false,
        Screen::Dashboard,
        BoardEntry::Lobby,
    );
    wait_until(
        || {
            let _ = app.daily.tick();
            std::future::ready(is_pool_board(&app))
        },
        "the board loads the match",
    )
    .await;
    (test_db, app)
}

fn resign_asked(app: &App) -> bool {
    app.daily
        .board
        .as_ref()
        .expect("the board is open")
        .resign_confirm
}

fn mode(app: &App) -> ShotMode {
    app.daily
        .board
        .as_ref()
        .and_then(|board| board.detail.as_ref())
        .and_then(DailyMatchDetail::pool)
        .expect("a pool board")
        .draft
        .mode
}

#[tokio::test]
async fn the_replay_keys_never_reach_the_resign_key_behind_them() {
    // Every other board resigns on `r`, and the shared map behind this one
    // still does. Before the break there is no shot to replay, so the key has
    // nothing to do, and nothing is exactly what it has to do: pressed twice,
    // which is how a replay is started and stopped, it used to resign.
    let (_test_db, mut app) = unbroken_rack("pool-keys-replay").await;
    for key in *b"rrRR" {
        assert!(
            board_input::handle_key(&mut app, key),
            "the board takes the key"
        );
        assert!(
            !resign_asked(&app),
            "{:?} asked to resign a match with nothing to replay",
            key as char
        );
    }
}

#[tokio::test]
async fn only_lowercase_arms_a_stroke_and_shift_x_only_resigns() {
    let (_test_db, mut app) = unbroken_rack("pool-keys-case").await;
    for (key, band) in [
        (b'x', PowerBand::Light),
        (b's', PowerBand::Normal),
        (b'w', PowerBand::Strong),
    ] {
        board_input::handle_key(&mut app, key);
        assert_eq!(mode(&app), ShotMode::Stroke(band), "{:?}", key as char);
    }
    // Shifted, none of them is a stroke key: a hand that slips onto Shift or
    // leaves Caps Lock on must not re-arm the cue, and `X` must not be one
    // press away from `x`'s job as well as its own.
    for key in *b"SWX" {
        board_input::handle_key(&mut app, key);
        assert_eq!(
            mode(&app),
            ShotMode::Stroke(PowerBand::Strong),
            "{:?} changed the stroke",
            key as char
        );
    }
    assert!(resign_asked(&app), "X is the resign key and nothing else");
}

#[tokio::test]
async fn j_and_k_step_an_armed_stroke_ahead_of_the_match_chat() {
    // The match chat owns `j`/`k` on every board; pool borrows them only
    // while a stroke is armed, which is the one time they mean speed.
    let (_test_db, mut app) = unbroken_rack("pool-keys-speed").await;
    let speed = |app: &App| {
        app.daily
            .board
            .as_ref()
            .and_then(|board| board.detail.as_ref())
            .and_then(DailyMatchDetail::pool)
            .expect("a pool board")
            .draft
            .power()
            * crate::app::games::pool_core::cue::MAX_SPEED
    };
    let (before, idle) = (mode(&app), speed(&app));
    board_input::handle_key(&mut app, b'k');
    assert_eq!(mode(&app), before, "unarmed, k is not pool's");
    assert_eq!(speed(&app), idle);

    board_input::handle_key(&mut app, b's');
    board_input::handle_key(&mut app, b'k');
    let up = speed(&app);
    assert!(
        board_input::handle_key(&mut app, b'k'),
        "armed, pool takes k"
    );
    assert!((speed(&app) - (up + 0.1)).abs() < 1e-6);
    board_input::handle_key(&mut app, b'j');
    assert!((speed(&app) - up).abs() < 1e-6);
    assert_eq!(mode(&app), ShotMode::Stroke(PowerBand::Normal));
}

#[tokio::test]
async fn the_chat_keys_close_the_resign_prompt_too() {
    // `i`, `j` and `k` belong to the match chat and are routed there before
    // the board sees them. While the prompt is up they must still close it,
    // or the `y` after them (snooker's "play again") resigns the match.
    let (_test_db, mut app) = unbroken_rack("pool-keys-resign-chat").await;
    assert!(
        app.daily.board_chat_room_id().is_some(),
        "the match has its chat, which is what takes these keys"
    );
    for key in *b"ijk" {
        board_input::handle_key(&mut app, b'X');
        assert!(resign_asked(&app));
        board_input::handle_key(&mut app, key);
        assert!(
            !resign_asked(&app),
            "{:?} left the prompt armed",
            key as char
        );
    }
}

#[tokio::test]
async fn the_resign_prompt_is_answered_by_y_and_closed_by_anything_else() {
    // `X` is Shift away from the light stroke, so the slip has to be cheap:
    // the `x` that was meant closes the prompt and does nothing more, and a
    // second `X` (a Shift held too long) is not a yes.
    let (_test_db, mut app) = unbroken_rack("pool-keys-resign").await;
    let before = mode(&app);
    for key in *b"Xx" {
        board_input::handle_key(&mut app, key);
    }
    assert!(!resign_asked(&app), "x closes the prompt");
    assert_eq!(mode(&app), before, "and does not arm the stroke as well");

    for key in *b"XX" {
        board_input::handle_key(&mut app, key);
    }
    assert!(
        !resign_asked(&app),
        "a second X closes it rather than resigning"
    );

    board_input::handle_key(&mut app, b'X');
    assert!(
        board_input::handle_key(&mut app, 0x1B),
        "Esc answers the prompt"
    );
    assert!(!resign_asked(&app));
    assert!(app.daily.board.is_some(), "without leaving the board");
}

/// A snooker match where the breaker has just walked away from the pack —
/// no contact, a foul and a miss — and the board open for the fouled player.
async fn fouled_snooker(name: &str) -> (TestDb, App, DailyService, uuid::Uuid, uuid::Uuid) {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, &format!("{name}-challenger")).await;
    let claimer = create_test_user(&test_db.db, &format!("{name}-claimer")).await;
    let (activity_tx, _activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let svc = DailyService::new(
        test_db.db.clone(),
        ChipService::new(test_db.db.clone()),
        ActivityPublisher::new(test_db.db.clone(), activity_tx),
    );
    let posted = svc
        .post_challenge(challenger.id, DailyGame::Snooker)
        .await
        .expect("post");
    let claimed = svc
        .claim_challenge(claimer.id, posted.id)
        .await
        .expect("claim");
    let breaker = claimed.turn_user_id.expect("somebody breaks");
    svc.play_pool_shot(
        breaker,
        claimed.id,
        Shot {
            azimuth: std::f64::consts::PI,
            speed: 0.3,
            ..Shot::default()
        },
    )
    .await
    .expect("a foul is still a move");
    let fouled = if breaker == challenger.id {
        claimer.id
    } else {
        challenger.id
    };

    let mut app = make_app(test_db.db.clone(), fouled, name);
    app.daily.open_board_inner(
        claimed.id,
        DailyGame::Snooker,
        HashMap::new(),
        false,
        Screen::Dashboard,
        BoardEntry::Lobby,
    );
    app.set_screen(Screen::DailyMatch);
    app.resize(160, 40).expect("resize test terminal");
    wait_until(
        || {
            let _ = app.daily.tick();
            std::future::ready(is_pool_board(&app))
        },
        "the board loads the match",
    )
    .await;
    (test_db, app, svc, claimed.id, breaker)
}

fn dialog_open(app: &mut App) -> bool {
    app.daily
        .pool_draft_mut()
        .is_some_and(|(draft, state)| draft.foul_dialog_open(state))
}

#[tokio::test]
async fn a_foul_puts_a_choice_in_front_of_the_fouled_player_and_waits() {
    let (_test_db, mut app, _svc, _match_id, _breaker) = fouled_snooker("pool-foul-dialog").await;
    assert!(dialog_open(&mut app));
    wait_for_render_contains(&mut app, "How do you want to go on?").await;
    wait_for_render_contains(&mut app, "Put the balls back").await;

    // Nothing about the shot can be touched while it is up.
    let before = mode(&app);
    for key in *b"aex[" {
        board_input::handle_key(&mut app, key);
    }
    assert_eq!(mode(&app), before, "the dialog swallowed the shot keys");
    assert!(dialog_open(&mut app));

    // Walking down and back up, then "play from here": it stands aside, and
    // the shot is theirs.
    board_input::handle_key(&mut app, b'j');
    board_input::handle_key(&mut app, b'k');
    board_input::handle_key(&mut app, b'\r');
    assert!(!dialog_open(&mut app));
    wait_for_render_not_contains(&mut app, "How do you want to go on?").await;
    board_input::handle_key(&mut app, b'a');
    assert_eq!(mode(&app), ShotMode::Aim, "and the controls are back");
}

#[tokio::test]
async fn putting_the_balls_back_from_the_dialog_hands_the_table_over() {
    let (test_db, mut app, _svc, match_id, breaker) = fouled_snooker("pool-foul-back").await;
    assert!(dialog_open(&mut app));
    board_input::handle_key(&mut app, b'3');
    let client = test_db.db.get().await.expect("db client");
    wait_until(
        || {
            let client = &client;
            async move {
                late_core::models::daily_match::DailyMatch::get(client, match_id)
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|row| row.turn_user_id == Some(breaker))
            }
        },
        "the offender is back at the table",
    )
    .await;
}
