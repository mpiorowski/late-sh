use late_core::models::artboard_piece::{
    ApplauseOutcome, ArtboardPiece, HangOutcome, HangParams, TakeDownOutcome,
};
use late_core::models::chat_message::ChatMessage;
use late_core::models::chat_room::ChatRoom;
use late_core::test_utils::create_test_user;
use serde_json::json;
use uuid::Uuid;

use super::{CardResult, GalleryResult, GalleryService, SplashRefresh, VoteCallOutcome};
use crate::app::artboard::gallery::frame::FramedPiece;
use crate::app::artboard::gallery::vote_call::DueCall;
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::svc::{ChatEvent, ChatService};
use crate::test_helpers::{new_test_db, wait_until};

pub(crate) fn hang_params(user_id: Uuid, title: &str) -> HangParams {
    HangParams {
        user_id,
        title: title.to_string(),
        width: 12,
        height: 4,
        canvas: json!({
            "width": 12,
            "height": 4,
            "cells": [[{"x": 0, "y": 0}, {"Narrow": "#"}]],
            "colors": [],
        }),
        provenance: json!({ "cells": [[{"x": 0, "y": 0}, "painter"]] }),
        glyph_count: 40,
        own_share_percent: 100,
        content_hash: format!("hash-{title}"),
    }
}

/// A service wired the way `main.rs` wires it, with the chat it posts
/// cards through.
fn test_service(db: &late_core::db::Db) -> (GalleryService, ChatService) {
    let chat = ChatService::new(db.clone(), NotificationService::new(db.clone()));
    let service = GalleryService::new(
        db.clone(),
        chat.clone(),
        "http://localhost:3000".to_string(),
    );
    (service, chat)
}

/// A one-cell piece framed the way the hang flow hands it over.
fn framed_piece() -> FramedPiece {
    let mut canvas = dartboard_core::Canvas::with_size(12, 4);
    canvas.set(dartboard_core::Pos { x: 0, y: 0 }, '#');
    FramedPiece {
        width: 12,
        height: 4,
        canvas,
        provenance: Default::default(),
        glyph_count: 40,
        own_share_percent: 100,
        credits: Vec::new(),
        content_hash: "hash-card".to_string(),
    }
}

async fn today_piece(db: &late_core::db::Db, service: &GalleryService, owner: Uuid) -> Uuid {
    let client = db.get().await.unwrap();
    let HangOutcome::Hung(piece) =
        ArtboardPiece::hang(&client, hang_params(owner, "today's splash"))
            .await
            .unwrap()
    else {
        panic!("hang");
    };
    client.execute("UPDATE artboard_pieces SET created = CURRENT_TIMESTAMP - INTERVAL '1 day' WHERE id = $1", &[&piece.id]).await.unwrap();
    service
        .refresh_splash(chrono::Utc::now().date_naive())
        .await
        .unwrap();
    assert_eq!(service.splash_piece().unwrap().piece.id, piece.id);
    piece.id
}

/// The piece greets only users who turned the Artboard content disclaimer
/// off (Always View); everyone else, newcomers first, gets the cup. A piece
/// taken down leaves new logins at once, before the hourly refresh.
#[tokio::test]
async fn splash_shows_only_without_the_disclaimer_and_only_while_the_piece_is_up() {
    let test_db = new_test_db().await;
    let service = test_service(&test_db.db).0;
    let owner = create_test_user(&test_db.db, "splash-owner").await;
    let piece = today_piece(&test_db.db, &service, owner.id).await;

    assert!(service.splash_piece_for_login(true).await.is_none());
    assert_eq!(
        service
            .splash_piece_for_login(false)
            .await
            .expect("the day's piece")
            .piece
            .id,
        piece
    );

    test_db
        .db
        .get()
        .await
        .unwrap()
        .execute(
            "UPDATE artboard_pieces SET removed_at = CURRENT_TIMESTAMP WHERE id = $1",
            &[&piece],
        )
        .await
        .unwrap();
    assert!(
        service.splash_piece().is_some(),
        "the watch still holds the piece until the hourly refresh"
    );
    assert!(service.splash_piece_for_login(false).await.is_none());
    // A day with nothing queued is the cup, never the previous day's piece.
    assert!(
        service
            .splash_piece_for_day(
                false,
                chrono::Utc::now().date_naive() + chrono::Duration::days(1)
            )
            .await
            .is_none()
    );
}

#[tokio::test]
async fn failed_splash_removal_check_falls_back_to_the_cup() {
    let test_db = new_test_db().await;
    let owner = create_test_user(&test_db.db, "splash-failure-owner").await;
    let service = test_service(&test_db.db).0;
    today_piece(&test_db.db, &service, owner.id).await;
    test_db
        .db
        .get()
        .await
        .unwrap()
        .batch_execute("ALTER TABLE artboard_pieces RENAME COLUMN splash_on TO splash_day")
        .await
        .unwrap();
    assert!(service.splash_piece_for_login(false).await.is_none());
}

/// The refresh publishes the day's piece into the watch and every login
/// that day shows it; an empty day, and a service with no database,
/// publish nothing and every login is the cup.
#[tokio::test]
async fn the_refresh_publishes_todays_piece_for_every_login_that_day() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let painter = create_test_user(&test_db.db, "splash-refresh-painter").await;
    let today = chrono::Utc::now().date_naive();
    let tomorrow = today + chrono::Duration::days(1);

    let service = test_service(&test_db.db).0;

    // Nothing hung before today: the watch is empty and the door is the cup.
    assert_eq!(
        service.refresh_splash(today).await.expect("refresh"),
        SplashRefresh::Wall {
            piece: None,
            queued: 0
        }
    );
    assert_eq!(service.splash_piece(), None);

    // A piece hung today is tomorrow's; the refresh for tomorrow assigns
    // it and publishes it.
    let hung = match ArtboardPiece::hang(&client, hang_params(painter.id, "dawn"))
        .await
        .expect("hang")
    {
        HangOutcome::Hung(piece) => piece,
        other => panic!("expected the piece to hang, got {other:?}"),
    };
    let published = match service.refresh_splash(tomorrow).await.expect("refresh") {
        SplashRefresh::Wall {
            piece: Some(piece),
            queued: 0,
        } => *piece,
        other => panic!("expected tomorrow's piece and an empty queue, got {other:?}"),
    };
    assert_eq!(published.piece.id, hung.id);
    assert_eq!(published.piece.title, "dawn");
    assert_eq!(published.shown_on, tomorrow);
    // Every login of the day reads it, not only the first.
    assert_eq!(service.splash_piece(), Some(published.clone()));
    assert_eq!(service.splash_piece(), Some(published));

    // No database: the refresh is off and every login is the cup.
    let off = GalleryService::disabled();
    assert_eq!(
        off.refresh_splash(tomorrow).await.expect("refresh"),
        SplashRefresh::Off
    );
    assert_eq!(off.splash_piece(), None);
}

#[tokio::test]
async fn first_login_after_midnight_shows_the_new_days_piece() {
    let test_db = new_test_db().await;
    let service = test_service(&test_db.db).0;
    let owner = create_test_user(&test_db.db, "midnight-owner").await;
    let client = test_db.db.get().await.unwrap();
    let mut pieces = Vec::new();
    for (title, days_ago) in [("yesterday's splash", 3_i32), ("today's splash", 2_i32)] {
        let HangOutcome::Hung(piece) = ArtboardPiece::hang(&client, hang_params(owner.id, title))
            .await
            .unwrap()
        else {
            panic!("hang");
        };
        client
            .execute(
                "UPDATE artboard_pieces
                 SET created = CURRENT_TIMESTAMP - make_interval(days => $2)
                 WHERE id = $1",
                &[&piece.id, &days_ago],
            )
            .await
            .unwrap();
        pieces.push(piece.id);
    }
    let today = chrono::Utc::now().date_naive();
    service
        .refresh_splash(today - chrono::Duration::days(1))
        .await
        .unwrap();
    assert_eq!(service.splash_piece().unwrap().piece.id, pieces[0]);

    // The hourly refresh has not run since midnight: the cache still holds
    // yesterday's piece when the first login of the day arrives.
    let splash = service
        .splash_piece_for_day(false, today)
        .await
        .expect("the new day's piece, not the cup");
    assert_eq!((splash.piece.id, splash.shown_on), (pieces[1], today));
}

/// A hung piece gets a card in #lounge in its hanger's name, every session
/// draws that message as the piece, nobody edits it, and taking the piece
/// down takes the card with it.
#[tokio::test]
async fn a_hung_piece_has_a_lounge_card_until_it_is_taken_down() {
    let test_db = new_test_db().await;
    let (service, chat) = test_service(&test_db.db);
    let artist = create_test_user(&test_db.db, "card-artist").await;
    let client = test_db.db.get().await.unwrap();
    ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let mut events = chat.subscribe_events();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    service.hang_task(
        artist.id,
        "cool   piece".to_string(),
        framed_piece(),
        tx.clone(),
    );
    let Some(GalleryResult::Hung(piece)) = rx.recv().await else {
        panic!("the piece did not hang");
    };
    // The card follows the hang on its own task: the line is in the room
    // before the piece points at it.
    let card = loop {
        if let ChatEvent::MessageCreated { message, .. } = next_event(&mut events).await {
            break message;
        }
    };
    assert_eq!(card.user_id, artist.id);
    assert_eq!(
        card.body,
        format!(
            "🎨 \"cool piece\" http://localhost:3000/gallery/pieces/{}",
            piece.id
        )
    );
    wait_until(
        || async {
            ArtboardPiece::find(&client, artist.id, piece.id)
                .await
                .unwrap()
                .and_then(|row| row.lounge_message_id)
                == Some(card.id)
        },
        "the piece points at its card",
    )
    .await;

    service.refresh_lounge_cards().await.unwrap();
    let cards = service.subscribe_lounge_cards().borrow().clone();
    assert_eq!(cards.get(&card.id).map(|piece| piece.id), Some(piece.id));

    let request_id = Uuid::now_v7();
    chat.edit_message_task(
        artist.id,
        card.id,
        "edited".to_string(),
        request_id,
        crate::moderation::policy::Permissions::default(),
    );
    let refusal = loop {
        match next_event(&mut events).await {
            ChatEvent::EditFailed {
                request_id: got,
                message,
                ..
            } if got == request_id => break message,
            ChatEvent::EditSucceeded {
                request_id: got, ..
            } if got == request_id => {
                panic!("the card's edit went through")
            }
            _ => {}
        }
    };
    assert_eq!(refusal, "That card can't be edited.");

    service.take_down_task(piece.id, artist.id, tx);
    let Some(GalleryResult::TakeDown { outcome, .. }) = rx.recv().await else {
        panic!("the piece did not come down");
    };
    assert_eq!(
        outcome,
        TakeDownOutcome::TakenDown {
            lounge_message_id: Some(card.id)
        }
    );
    // The card is withdrawn before the take-down answers.
    assert!(ChatMessage::get(&client, card.id).await.unwrap().is_none());
    service.refresh_lounge_cards().await.unwrap();
    assert!(service.subscribe_lounge_cards().borrow().is_empty());
}

/// `/mod artboard remove` takes the card out of #lounge with the piece.
#[tokio::test]
async fn a_mod_removing_a_piece_removes_its_card() {
    let test_db = new_test_db().await;
    let (service, chat) = test_service(&test_db.db);
    let artist = create_test_user(&test_db.db, "removed-artist").await;
    let admin = create_test_user(&test_db.db, "removing-admin").await;
    let client = test_db.db.get().await.unwrap();
    ChatRoom::ensure_lounge(&client).await.expect("lounge");

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    service.hang_task(artist.id, "to remove".to_string(), framed_piece(), tx);
    let Some(GalleryResult::Hung(piece)) = rx.recv().await else {
        panic!("the piece did not hang");
    };
    let card_of = || async {
        ArtboardPiece::find(&client, artist.id, piece.id)
            .await
            .unwrap()
            .and_then(|row| row.lounge_message_id)
    };
    wait_until(
        || async { card_of().await.is_some() },
        "the piece has its card",
    )
    .await;
    let card_id = card_of().await.expect("card id");

    let reply = chat
        .run_mod_command(
            admin.id,
            crate::moderation::policy::Permissions::new(true, false),
            &format!(
                "artboard remove {} off topic",
                super::super::ui::piece_id_prefix(piece.id)
            ),
        )
        .await
        .expect("the removal");
    assert_eq!(
        reply,
        vec![format!(
            "took down gallery piece {} (\"to remove\")",
            piece.id
        )]
    );
    assert!(ChatMessage::get(&client, card_id).await.unwrap().is_none());
    assert!(
        ArtboardPiece::find(&client, artist.id, piece.id)
            .await
            .unwrap()
            .is_none()
    );
}

/// The piece modal's read marks the viewer's own applause, which the
/// shared card cannot carry, and applause from the modal lands on the row.
#[tokio::test]
async fn the_card_modal_reads_and_applauds_as_its_viewer() {
    let test_db = new_test_db().await;
    let (service, _chat) = test_service(&test_db.db);
    let artist = create_test_user(&test_db.db, "modal-artist").await;
    let fan = create_test_user(&test_db.db, "modal-fan").await;
    let client = test_db.db.get().await.unwrap();
    let HangOutcome::Hung(hung) = ArtboardPiece::hang(&client, hang_params(artist.id, "modal"))
        .await
        .unwrap()
    else {
        panic!("hang");
    };

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    service.applaud_card_task(hung.id, fan.id, tx.clone());
    assert_eq!(
        rx.recv().await,
        Some(CardResult::Applause(ApplauseOutcome::Applauded(1)))
    );

    service.view_card_task(hung.id, fan.id, tx.clone());
    let Some(CardResult::Viewed(Some(seen))) = rx.recv().await else {
        panic!("the piece was not read");
    };
    assert_eq!((seen.applause, seen.applauded_by_viewer), (1, true));

    service.view_card_task(hung.id, artist.id, tx.clone());
    let Some(CardResult::Viewed(Some(seen))) = rx.recv().await else {
        panic!("the piece was not read");
    };
    assert_eq!((seen.applause, seen.applauded_by_viewer), (1, false));

    service.applaud_card_task(hung.id, artist.id, tx.clone());
    assert_eq!(
        rx.recv().await,
        Some(CardResult::Applause(ApplauseOutcome::OwnPiece))
    );

    service.view_card_task(Uuid::now_v7(), fan.id, tx);
    assert_eq!(rx.recv().await, Some(CardResult::Viewed(None)));
}

async fn next_event(events: &mut tokio::sync::broadcast::Receiver<ChatEvent>) -> ChatEvent {
    tokio::time::timeout(std::time::Duration::from_secs(5), events.recv())
        .await
        .expect("a chat event in time")
        .expect("chat event")
}

/// The `system` user the house posts as, made the way `activity/lounge.rs`
/// ensures it.
async fn system_user(db: &late_core::db::Db) -> late_core::models::user::User {
    use crate::app::activity::lounge::{SYSTEM_FINGERPRINT, SYSTEM_USERNAME};
    use late_core::models::user::{User, UserParams};

    let client = db.get().await.unwrap();
    User::create(
        &client,
        UserParams {
            fingerprint: SYSTEM_FINGERPRINT.to_string(),
            username: SYSTEM_USERNAME.to_string(),
            settings: json!({ "bot": true, "system": true }),
        },
    )
    .await
    .expect("system user")
}

/// This month's call with three days left, whatever today is:
/// `run_vote_call` posts the call it is handed.
fn due_call() -> DueCall {
    use chrono::Datelike;

    DueCall {
        period_month: chrono::Utc::now()
            .date_naive()
            .with_day(1)
            .expect("first of the month"),
        days_left: 3,
    }
}

/// A due vote call goes out once, as the system user, however often the
/// task looks, and never for a month with nothing on its wall.
#[tokio::test]
async fn a_due_vote_call_posts_one_line_in_lounge() {
    let test_db = new_test_db().await;
    let (service, chat) = test_service(&test_db.db);
    let artist = create_test_user(&test_db.db, "call-artist").await;
    let system = system_user(&test_db.db).await;
    let client = test_db.db.get().await.unwrap();
    ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let mut events = chat.subscribe_events();

    assert_eq!(service.run_vote_call(due_call()).await, None);

    ArtboardPiece::hang(&client, hang_params(artist.id, "the only one"))
        .await
        .unwrap();
    assert_eq!(
        service.run_vote_call(due_call()).await,
        Some(VoteCallOutcome::Posted)
    );
    let line = loop {
        if let ChatEvent::MessageCreated { message, .. } = next_event(&mut events).await {
            break message;
        }
    };
    assert_eq!(line.user_id, system.id);
    assert!(
        line.body
            .contains("wall closes in 3 days.\n1 piece hangs and none has the 3 applause"),
        "{}",
        line.body
    );
    assert_eq!(
        service.run_vote_call(due_call()).await,
        None,
        "the call is claimed: no second line"
    );

    service.refresh_lounge_cards().await.unwrap();
    assert!(service.subscribe_vote_calls().borrow().contains(&line.id));
}

/// Enter on a vote call's line walks the month: the Artboard's Vote
/// listing full frame (after the content disclaimer, for a viewer who has
/// not answered it), where `v` applauds and the last piece says it is the
/// last.
#[tokio::test]
async fn enter_on_a_vote_call_opens_the_vote_listing_full_frame() {
    use crate::app::artboard::gallery::state::{Focus, GallerySection};
    use crate::app::common::primitives::Screen;
    use crate::test_helpers::{SessionWorld, make_app_in_world, wait_for_render_contains};
    use late_core::models::chat_room_member::ChatRoomMember;

    let test_db = new_test_db().await;
    let (service, chat) = test_service(&test_db.db);
    let me = create_test_user(&test_db.db, "call-viewer").await;
    let artist = create_test_user(&test_db.db, "call-painter").await;
    system_user(&test_db.db).await;
    let client = test_db.db.get().await.unwrap();
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    ChatRoomMember::join(&client, lounge.id, me.id)
        .await
        .expect("join lounge");
    let HangOutcome::Hung(piece) = ArtboardPiece::hang(&client, hang_params(artist.id, "dusk"))
        .await
        .unwrap()
    else {
        panic!("hang");
    };
    assert_eq!(
        service.run_vote_call(due_call()).await,
        Some(VoteCallOutcome::Posted)
    );
    service.refresh_lounge_cards().await.unwrap();

    let world = SessionWorld {
        gallery_service: Some(service.clone()),
        chat_service: Some(chat),
        ..Default::default()
    };
    let mut app = make_app_in_world(test_db.db.clone(), me.id, "call-viewer-it", world);
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "wall closes in 3 days.").await;

    app.handle_input(b"j");
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Artboard);
    assert!(app.artboard_disclaimer_visible());
    app.handle_input(b"A");

    // The piece's caption: the listing has landed and the frame is up.
    wait_for_render_contains(&mut app, "\"dusk\" by @").await;
    let gallery = app.dartboard_state.as_ref().expect("the board").gallery();
    assert_eq!(gallery.viewed_section(), Some(GallerySection::Vote));
    assert_eq!(gallery.focus(), Focus::Piece);
    assert_eq!(
        gallery.selected_piece().map(|shown| shown.id),
        Some(piece.id)
    );

    app.handle_input(b"v");
    wait_for_render_contains(&mut app, "Applauded. 1 applause.").await;
    assert_eq!(
        ArtboardPiece::applause_count(&client, piece.id)
            .await
            .unwrap(),
        1
    );
    app.handle_input(b"\x1b[C");
    wait_for_render_contains(&mut app, "That was the last piece of the month.").await;
}
