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
use crate::test_helpers::{make_app, new_test_db, wait_until};

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
