use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use late_core::models::deadchannel_bout::Bout;
use late_core::test_utils::create_test_user;
use rand::SeedableRng;
use rand::rngs::StdRng;
use uuid::Uuid;

use super::handle_event;
use crate::app::common::primitives::Screen;
use crate::app::deadchannel::city::map::Landmark;
use crate::app::deadchannel::fight::cards::DECK;
use crate::app::deadchannel::ring::state::Duel;
use crate::app::deadchannel::ring::svc::{Board, LiveBout};
use crate::app::input::ParsedInput;
use crate::test_helpers::{make_app, new_test_db};

/// A bout `caller` called and `called` took a moment ago.
fn bout(caller: Uuid, called: Uuid) -> Bout {
    let now = Utc::now();
    Bout {
        id: Uuid::now_v7(),
        day: now.date_naive(),
        caller_user_id: caller,
        called_user_id: called,
        caller_username: "mira".to_string(),
        called_username: "tom".to_string(),
        caller_deck: Some(serde_json::json!(DECK)),
        called_deck: Some(serde_json::json!(DECK)),
        called_at: now,
        expires_at: now,
        accepted_at: Some(now),
        bell_at: Some(now),
        deadline_at: Some(now),
        state: None,
        pool_caller: 0,
        pool_called: 0,
        ended_at: None,
        outcome: None,
        caller_settled_at: None,
        called_settled_at: None,
    }
}

#[tokio::test]
async fn a_fighter_s_card_keys_stay_the_ring_s_after_the_bout_ends() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ring-keys-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "ring-keys-token");
    app.skip_splash_for_tests();
    app.set_screen(Screen::City);
    app.city.open_panel(Landmark::Ring);

    // The bout this runner fought just ended: the last round came out
    // under their fingers.
    let mut ended = bout(user.id, Uuid::now_v7());
    ended.ended_at = Some(Utc::now());
    app.ring.board = Arc::new(Board {
        calls: Vec::new(),
        live: Some(LiveBout {
            bout: ended,
            duel: Duel::deal(DECK.to_vec(), DECK.to_vec(), &mut StdRng::seed_from_u64(7)),
            bets: Vec::new(),
        }),
        records: HashMap::new(),
    });

    // A card key pressed a moment late is swallowed, not handed to the
    // page switch, and the runner is still looking at the ring.
    for digit in ['1', '3', '5'] {
        assert!(
            handle_event(&mut app, &ParsedInput::Char(digit)),
            "{digit} went to the globals"
        );
    }
    assert_eq!(app.screen, Screen::City);
    assert_eq!(app.city.panel(), Some(Landmark::Ring));
}
