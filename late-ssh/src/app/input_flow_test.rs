//! App input integration tests against a real ephemeral DB.

#[tokio::test]
async fn esc_in_the_settings_langs_picker_closes_the_picker_and_keeps_settings_open() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "langs-esc-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "langs-esc-flow-it");
    app.handle_input(b"\x0f");
    wait_for_render_contains(&mut app, "langs-esc-it").await;
    // Username, Country, Timezone, Theme, IDE, Terminal, OS, then Langs.
    app.handle_input(b"jjjjjjj\r");
    wait_for_render_contains(&mut app, "[Done]").await;
    assert!(app.tag_picker.is_open());

    app.handle_input(b"\x1b");
    wait_for_render_not_contains(&mut app, "[Done]").await;
    assert!(!app.tag_picker.is_open());
    assert!(app.show_settings);
}

#[tokio::test]
async fn art_splash_tweak_is_visible_on_a_short_terminal_and_persists_every_mode() {
    use late_core::models::user::{ArtSplashMode, extract_art_splash_mode};
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "splash-tweak-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "splash-tweak-flow-it");
    app.handle_input(b"\x0f");
    wait_for_render_contains(&mut app, "splash-tweak-it").await;
    app.resize(80, 24).unwrap();
    app.handle_input(b"\t\t\t");
    for _ in 0..11 {
        app.handle_input(b"j");
    }
    wait_for_render_contains(&mut app, "Show Gallery Art on Splash").await;
    assert!(render_plain(&mut app).contains("◂ SFW    ▸"));
    for (key, expected) in [
        (b"\r".as_slice(), ArtSplashMode::Always),
        (b"\x1b[C".as_slice(), ArtSplashMode::Never),
        (b"\x1b[C".as_slice(), ArtSplashMode::Sfw),
        (b"\x1b[D".as_slice(), ArtSplashMode::Never),
    ] {
        app.handle_input(key);
        let db = test_db.db.clone();
        wait_until(
            || {
                let db = db.clone();
                async move {
                    let client = db.get().await.unwrap();
                    let stored = User::get(&client, user.id).await.unwrap().unwrap();
                    extract_art_splash_mode(&stored.settings) == expected
                }
            },
            "art splash mode to persist",
        )
        .await;
        // The profile snapshot refresh follows the database commit; wait for
        // that round trip before advancing or reopening from the snapshot.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            app.tick();
            if app.profile_state.profile().art_splash_mode == expected {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "profile snapshot did not refresh"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(render_plain(&mut app).contains(&format!("◂ {:<6} ▸", expected.label())));
    }
    app.handle_input(b"\x1b");
    wait_for_render_not_contains(&mut app, "Show Gallery Art on Splash").await;
    app.handle_input(b"\x0f");
    wait_for_render_contains(&mut app, "Theme").await;
    app.handle_input(b"\t\t\t");
    for _ in 0..11 {
        app.handle_input(b"j");
    }
    wait_for_render_contains(&mut app, "◂ Never  ▸").await;
}

#[tokio::test]
async fn art_content_dialog_routes_owner_votes_mouse_and_close_keys() {
    use crate::app::common::primitives::Screen;
    use late_core::models::artboard_piece::{ArtboardPiece, HangOutcome, HangParams};
    use late_core::models::artboard_piece_rating::ArtboardPieceRating;
    let test_db = new_test_db().await;
    let owner = create_test_user(&test_db.db, "content-owner-it").await;
    let viewer = create_test_user(&test_db.db, "content-voter-it").await;
    let client = test_db.db.get().await.unwrap();
    let HangOutcome::Hung(piece) = ArtboardPiece::hang(&client, HangParams {
        user_id: owner.id, title: "content dialog piece".to_string(), width: 12, height: 4,
        canvas: serde_json::json!({"width":12,"height":4,"cells":[[{"x":0,"y":0},{"Narrow":"#"}]],"colors":[]}),
        provenance: serde_json::json!({"cells":[[{"x":0,"y":0},"painter"]]}), glyph_count: 40,
        own_share_percent: 100, content_hash: "content-dialog-test".to_string(),
    }).await.unwrap() else { panic!("hang"); };
    let mut painter = make_app(test_db.db.clone(), owner.id, "content-owner-flow-it");
    painter.handle_input(b"4");
    wait_for_render_contains(&mut painter, "GALLERY").await;
    painter.handle_input(b"j\r");
    wait_for_render_contains(&mut painter, "content dialog piece").await;
    painter.handle_input(b"n");
    wait_for_render_not_contains(&mut painter, "Updating content rating").await;
    wait_for_render_contains(&mut painter, "Artists cannot vote on their own pieces").await;
    assert!(!render_plain(&mut painter).contains("Vote NSFW"));
    painter.handle_input(b"\r");
    wait_for_render_contains(&mut painter, "Remove my NSFW flag").await;
    painter.handle_input(b"q");
    wait_for_render_not_contains(&mut painter, " Content rating ").await;
    assert!(painter.is_running());

    let mut voter = make_app(test_db.db.clone(), viewer.id, "content-voter-flow-it");
    voter.resize(80, 24).unwrap();
    voter.handle_input(b"4");
    wait_for_render_contains(&mut voter, "GALLERY").await;
    voter.handle_input(b"j\r");
    wait_for_render_contains(&mut voter, "content dialog piece").await;
    voter.handle_input(b"\r");
    voter.handle_input(b"n");
    wait_for_render_not_contains(&mut voter, "Updating content rating").await;
    wait_for_render_contains(&mut voter, "Vote NSFW").await;
    voter.handle_input(b"1?vx");
    assert_eq!(
        voter.screen,
        Screen::Artboard,
        "dialog owns page and gallery hotkeys"
    );
    // Use the exact row published by rendering: mouse and keyboard share actions.
    render_plain(&mut voter);
    let dialog = voter
        .dartboard_state
        .as_ref()
        .unwrap()
        .gallery()
        .rating_dialog
        .as_ref()
        .unwrap();
    let areas = dialog.action_areas.take();
    let nsfw = areas[1];
    dialog.action_areas.set(areas);
    voter.handle_input(format!("\x1b[<0;{};{}M", nsfw.x + 2, nsfw.y + 1).as_bytes());
    wait_for_render_contains(&mut voter, "Your vote: NSFW").await;
    // Votes stay open under the owner's NSFW flag, and can be replaced or withdrawn.
    voter.handle_input(b"k\r");
    wait_for_render_contains(&mut voter, "Your vote: SFW").await;
    voter.handle_input(b"k\r");
    wait_for_render_contains(&mut voter, "Your vote: none").await;
    voter.handle_input(b"\x1b");
    wait_for_render_not_contains(&mut voter, " Content rating ").await;
    let rating = ArtboardPieceRating::read(&client, piece.id, viewer.id)
        .await
        .unwrap()
        .unwrap();
    assert!(rating.owner_marked_nsfw);
    assert_eq!((rating.sfw_votes, rating.nsfw_votes), (0, 0));
    painter.handle_input(b"n");
    wait_for_render_not_contains(&mut painter, "Updating content rating").await;
    painter.handle_input(b"\r");
    wait_for_render_contains(&mut painter, "Mark my piece NSFW").await;
    assert!(
        !ArtboardPieceRating::read(&client, piece.id, owner.id)
            .await
            .unwrap()
            .unwrap()
            .owner_marked_nsfw
    );

    voter.handle_input(b"n");
    wait_for_render_contains(&mut voter, " Content rating ").await;
    voter.handle_input(b"\x1b[<0;15;1M");
    assert_eq!(voter.screen, Screen::Dashboard);
    assert!(voter.dartboard_state.is_none());
    wait_for_render_contains(&mut voter, " Home ").await;
    voter.handle_input(b"4");
    wait_for_render_contains(&mut voter, "GALLERY").await;
    assert!(
        voter
            .dartboard_state
            .as_ref()
            .unwrap()
            .gallery()
            .rating_dialog
            .is_none()
    );
}

#[tokio::test]
async fn artboard_topbar_clicks_leave_framing_and_title_entry() {
    use crate::app::{artboard::gallery::state::HangFlow, common::primitives::Screen};
    use late_core::models::user::InteractionMode;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-topbar-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-topbar-flow-it");

    for naming in [false, true] {
        app.handle_input(b"4");
        wait_for_render_contains(&mut app, "Mode       view").await;
        if naming {
            app.handle_input(b"i");
            app.handle_input(b"\x1b[200~##########\n##########\n##########\n##########\x1b[201~");
            app.handle_input(b"\x1b");
            wait_for_render_contains(&mut app, "Mode       view").await;
        }
        app.begin_artboard_hang();
        wait_for_render_contains(&mut app, "Frame your work").await;
        if naming {
            app.handle_input(b"\x1b[<0;2;2M\x1b[<32;11;5M\x1b[<0;11;5m");
            wait_for_render_contains(&mut app, "frame 10x4").await;
            app.handle_input(b"\r");
            wait_for_render_contains(&mut app, "Hang it in the").await;
            app.handle_input(b"piece 12");
            assert!(render_plain(&mut app).contains("piece 12"));
        }
        assert_eq!(app.screen, Screen::Artboard);

        app.interaction_mode = InteractionMode::Keyboard;
        app.handle_input(b"\x1b[<0;15;1M");
        assert_eq!(app.screen, Screen::Artboard);
        app.interaction_mode = InteractionMode::Mouse;
        // Releases, right clicks, gaps and the already-selected number stay put.
        app.handle_input(b"\x1b[<0;15;1m\x1b[<2;15;1M\x1b[<0;14;1M\x1b[<0;21;1M");
        assert_eq!(app.screen, Screen::Artboard);

        app.handle_input(b"\x1b[<0;15;1M");
        assert_eq!(app.screen, Screen::Dashboard, "naming={naming}");
        assert!(app.dartboard_state.is_none());
        wait_for_render_contains(&mut app, " Home ").await;
        app.handle_input(b"4");
        wait_for_render_contains(&mut app, "Mode       view").await;
        assert_eq!(
            app.dartboard_state.as_ref().unwrap().gallery().hang(),
            &HangFlow::Idle
        );
        app.handle_input(b"1");
    }
}

#[tokio::test]
async fn topbar_clicks_precede_active_game_input_and_respect_app_modals() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "game-topbar-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "game-topbar-flow-it");
    app.set_screen(Screen::Arcade);
    app.game_selection = crate::app::state::GAME_SELECTION_2048;
    app.handle_input(b"\r");
    assert!(app.is_playing_game);

    app.show_help = true;
    app.handle_input(b"\x1b[<0;15;1M");
    assert_eq!(app.screen, Screen::Arcade);
    assert!(app.show_help);
    app.show_help = false;

    app.handle_input(b"\x1b[<0;15;1M");
    assert_eq!(app.screen, Screen::Dashboard);
    wait_for_render_contains(&mut app, " Home ").await;
}

#[tokio::test]
async fn gallery_moderation_opens_selected_safety_record_only_for_staff() {
    use crate::app::artboard::gallery::state::{Focus, GallerySection, RailRow};
    use crate::app::common::primitives::Screen;
    use crate::app::mod_modal::state::ModLogKind;
    use late_core::models::artboard_piece::{ArtboardPiece, HangOutcome, HangParams};

    let test_db = new_test_db().await;
    let owner = create_test_user(&test_db.db, "gallery-mod-owner").await;
    let client = test_db.db.get().await.unwrap();
    let mut piece_ids = Vec::new();
    for title in ["moderation target", "other gallery piece"] {
        let HangOutcome::Hung(piece) = ArtboardPiece::hang(&client, HangParams {
            user_id: owner.id, title: title.to_string(), width: 12, height: 4,
            canvas: serde_json::json!({"width":12,"height":4,"cells":[[{"x":0,"y":0},{"Narrow":"#"}]],"colors":[]}),
            provenance: serde_json::json!({"cells":[[{"x":0,"y":0},"painter"]]}), glyph_count: 40,
            own_share_percent: 100, content_hash: format!("gallery-mod-{title}"),
        }).await.unwrap() else { panic!("hang"); };
        piece_ids.push(piece.id);
    }

    for (role, permissions) in [
        ("regular", Permissions::default()),
        ("moderator", Permissions::new(false, true)),
        ("admin", Permissions::new(true, false)),
    ] {
        let viewer = create_test_user(&test_db.db, &format!("gallery-mod-{role}")).await;
        let mut app = make_app_with_permissions(test_db.db.clone(), viewer.id, role, permissions);
        app.handle_input(b"4");
        wait_for_render_contains(&mut app, "GALLERY").await;
        app.handle_input(b"m");
        assert!(!app.show_mod_modal, "rail has no moderation shortcut");
        app.handle_input(b"j\r");
        wait_for_render_contains(&mut app, "moderation target").await;
        app.handle_input(b"j");
        assert_eq!(
            app.dartboard_state
                .as_ref()
                .unwrap()
                .gallery()
                .selected_piece()
                .unwrap()
                .id,
            piece_ids[0],
        );

        for (width, full_piece, key) in [(80, false, b'm'), (140, false, b'm'), (140, true, b'M')] {
            app.resize(width, 44).unwrap();
            if full_piece {
                app.handle_input(b"\r");
            }
            let focus = if full_piece {
                Focus::Piece
            } else {
                Focus::List
            };
            let hints = render_plain(&mut app);
            assert_eq!(
                hints.contains("m moderate"),
                permissions.can_access_mod_surface(),
                "{role}, {width}"
            );
            let prior_log_len = app.mod_modal_state.log().len();
            app.banner = None;
            app.handle_input(&[key]);
            assert_eq!(
                app.show_mod_modal,
                permissions.can_access_mod_surface(),
                "{role}"
            );
            assert!(app.banner.is_none(), "m must never mute the paired client");
            if permissions.can_access_mod_surface() {
                let command = format!("> artboard safety view {}", piece_ids[0]);
                assert!(
                    app.mod_modal_state
                        .log()
                        .iter()
                        .any(|line| line.text == command)
                );
                assert!(
                    app.mod_modal_state
                        .log()
                        .iter()
                        .skip(prior_log_len)
                        .any(|line| {
                            line.kind == ModLogKind::Help
                                && line.text == "artboard safety view [@user|piece-id-prefix]"
                        })
                );
                assert!(
                    !app.mod_modal_state
                        .log()
                        .iter()
                        .skip(prior_log_len)
                        .any(|line| line.text.contains("rename-room"))
                );
                // Wait for this opening's asynchronous record, not an earlier cached output.
                let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
                let id_line = format!("Art id: {}", piece_ids[0]);
                loop {
                    app.tick();
                    if app
                        .mod_modal_state
                        .log()
                        .iter()
                        .skip(prior_log_len)
                        .any(|line| line.text == id_line)
                    {
                        break;
                    }
                    assert!(
                        tokio::time::Instant::now() < deadline,
                        "safety record did not arrive"
                    );
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                assert!(
                    render_plain(&mut app).contains("Art id:"),
                    "the record must be visible at {width} columns"
                );
                // A later contextual opening must not submit or erase this draft.
                if prior_log_len == 0 {
                    app.handle_input(b"help artboard");
                }
                assert_eq!(app.mod_modal_state.command_text(), "help artboard");
                app.handle_input(b"\x1b");
                wait_for_render_not_contains(&mut app, " Moderation ").await;
                assert!(!app.show_mod_modal);
            } else {
                assert_eq!(app.mod_modal_state.log().len(), prior_log_len);
            }
            assert_eq!(app.screen, Screen::Artboard);
            let gallery = app.dartboard_state.as_ref().unwrap().gallery();
            assert_eq!(gallery.focus(), focus);
            assert_eq!(gallery.selected_piece().unwrap().id, piece_ids[0]);
        }

        app.handle_input(b"n");
        wait_for_render_contains(&mut app, " Content rating ").await;
        app.handle_input(b"m");
        assert!(
            !app.show_mod_modal,
            "the voting dialog retains its own controls"
        );
        app.handle_input(b"\x1b");
        wait_for_render_not_contains(&mut app, " Content rating ").await;

        if permissions.is_admin() {
            ArtboardPiece::remove(&client, piece_ids[0]).await.unwrap();
            app.handle_input(b"m");
            wait_for_render_contains(&mut app, "no gallery piece starts with").await;
            assert!(app.mod_modal_state.log().iter().any(|line| {
                line.kind == ModLogKind::Error && line.text.contains("no gallery piece starts with")
            }));
            app.handle_input(b"\x1b");
            wait_for_render_not_contains(&mut app, " Moderation ").await;
        }

        let gallery = app.dartboard_state.as_mut().unwrap().gallery_mut();
        gallery.close_piece();
        gallery.rail_select(RailRow::Gallery(GallerySection::Mine));
        gallery.rail_activate();
        wait_for_render_contains(&mut app, "you have not hung a piece").await;
        assert!(!render_plain(&mut app).contains("m moderate"));
        let prior_log_len = app.mod_modal_state.log().len();
        app.handle_input(b"m");
        assert!(!app.show_mod_modal, "an empty listing has no target");
        assert_eq!(app.mod_modal_state.log().len(), prior_log_len);
    }
}

use crate::authz::Permissions;
use crate::test_helpers::{
    assert_render_not_contains_for, chat_compose_app, make_app, make_app_in_world,
    make_app_with_chat_service, make_app_with_permissions, new_test_db, render_plain, strip_ansi,
    wait_for_render_contains, wait_for_render_not_contains, wait_until, with_session_key,
};
use late_core::models::cyberspace_account::CyberspaceAccount;
use late_core::models::user::{RightSidebarMode, RoomListMode};
use late_core::models::user_ssh_key::{KeyLayout, UserSshKey, extract_key_layout};
use late_core::models::{
    chat_message::{ChatMessage, ChatMessageParams},
    chat_message_gild::{ChatMessageGild, GildPlacement, GildTier},
    chat_message_reaction::ChatMessageReaction,
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
    statusline::StatusComponent,
    user::User,
};
use late_core::test_utils::create_test_user;
use tokio::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn leaderboard_mouse_and_control_keys_target_rail_and_content_separately() {
    use crate::app::{common::primitives::Screen, leaderboard::state::Board};
    use late_core::models::{
        leaderboard::{LeaderboardData, RankedEntry},
        user::InteractionMode,
    };
    use ratatui::layout::Position;
    use std::sync::Arc;

    let db = new_test_db().await;
    let user = create_test_user(&db.db, "leaderboard-mouse-flow").await;
    let mut app = make_app(db.db.clone(), user.id, "leaderboard-mouse-flow");
    app.resize(100, 24).unwrap();
    app.set_screen(Screen::Leaderboard);
    app.leaderboard = Arc::new(LeaderboardData {
        monthly_chip_earners: (1..=100)
            .map(|rank| RankedEntry {
                username: format!("player{rank}"),
                user_id: Uuid::from_u128(rank as u128),
                rank,
                value: rank,
                note: None,
            })
            .collect(),
        ..LeaderboardData::default()
    });
    // Top Drinkers leads the rail; step down to the board the fixture fills.
    app.leaderboard_page.select_next();
    app.render().unwrap();
    app.handle_input(b"\n\n\x0b");
    assert_eq!(app.leaderboard_page.scroll(), 1);
    assert_eq!(app.leaderboard_page.selected_board(), Board::TopChips);
    app.handle_input(b"\r");
    assert_eq!(app.leaderboard_page.scroll(), 1, "Enter is not Ctrl+J");

    let content = (0..24)
        .flat_map(|y| (0..100).map(move |x| Position::new(x, y)))
        .find(|point| app.leaderboard_page.over_content(*point))
        .unwrap();
    let wheel = format!("\x1b[<65;{};{}M", content.x + 1, content.y + 1);
    app.handle_input(wheel.as_bytes());
    assert_eq!(app.leaderboard_page.scroll(), 4);
    app.interaction_mode = InteractionMode::Keyboard;
    app.handle_input(wheel.as_bytes());
    assert_eq!(app.leaderboard_page.scroll(), 4);
    app.interaction_mode = InteractionMode::Mouse;
    app.show_help = true;
    app.handle_input(b"\n\x0b");
    app.handle_input(wheel.as_bytes());
    assert_eq!(app.leaderboard_page.scroll(), 4, "overlay owns input");
    app.show_help = false;

    let board = (0..24)
        .flat_map(|y| (0..100).map(move |x| Position::new(x, y)))
        .find(|point| app.leaderboard_page.board_at(*point) == Some(2))
        .unwrap();
    let click = format!("\x1b[<0;{};{}M", board.x + 1, board.y + 1);
    app.handle_input(click.as_bytes());
    assert_eq!(app.leaderboard_page.selected_board(), Board::ArcadeWins);
    assert_eq!(app.leaderboard_page.scroll(), 0);
    app.render().unwrap();
    let rail_wheel = format!("\x1b[<65;{};{}M", board.x + 1, board.y + 1);
    app.handle_input(rail_wheel.as_bytes());
    assert_eq!(app.leaderboard_page.selected_board(), Board::TimeOnline);
    app.handle_input(b"k\x1b[A");
    assert_eq!(app.leaderboard_page.selected_board(), Board::TopChips);
    app.resize(40, 10).unwrap();
    app.handle_input(click.as_bytes());
    assert_eq!(
        app.leaderboard_page.selected_board(),
        Board::TopChips,
        "resize discards old hit targets"
    );
}

#[tokio::test]
async fn quit_routes_open_confirm_without_persisting_exit_command() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "quit-confirm-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "quit-confirm-flow-it");

    wait_for_render_contains(&mut app, "lounge").await;

    app.handle_input(b"\x03");
    assert!(
        app.is_running(),
        "expected Ctrl+C to no longer quit the app"
    );
    let frame = render_plain(&mut app);
    assert!(
        frame.contains(" Home "),
        "expected app to remain on Home after Ctrl+C; frame={frame:?}"
    );
    assert!(
        !frame.contains(" Quit? "),
        "expected Ctrl+C to stay inert rather than opening quit confirm; frame={frame:?}"
    );

    app.handle_input(b"q");
    wait_for_render_contains(&mut app, " Quit? ").await;
    wait_for_render_contains(&mut app, "Clicked by mistake, right?").await;
    wait_for_render_contains(&mut app, "bye, I'll be back").await;
    wait_for_render_contains(&mut app, "yeah, my bad, stay").await;

    app.handle_input(b"\x1b");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Clicked by mistake, right?"),
        "expected quit confirm to dismiss after Esc; frame={frame:?}"
    );

    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Compose (Enter send").await;
    app.handle_input(b"/exit\r");
    wait_for_render_contains(&mut app, " Quit? ").await;

    let messages = ChatMessage::list_recent(&client, lounge.id, 20)
        .await
        .expect("list recent messages");
    assert!(messages.is_empty(), "expected /exit to stay client-side");
}

#[tokio::test]
async fn backtick_detaches_a_running_roguelike_and_hops_back_in() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "door-detach-flow").await;
    let mut app = make_app(test_db.db.clone(), user.id, "door-detach-flow-it");

    // Fabricate a running NetHack game on its screen, as if launched from the
    // hub. All assertions until the final section run without awaits, so the
    // fabricated proxy's bridge task never gets polled and the status stays
    // Connecting (not Closed).
    app.set_screen(Screen::Games);
    app.enter_nethack();
    app.nethack_state
        .as_mut()
        .expect("nethack state")
        .force_running_for_test();
    app.set_screen(Screen::Nethack);
    assert_eq!(app.screen, Screen::Nethack);

    // Ordinary keys are forwarded raw to the game, not interpreted.
    app.handle_input(b"j");
    assert_eq!(app.screen, Screen::Nethack);

    // Ctrl+S belongs to the running door too, not the global Shop shortcut.
    app.handle_input(b"\x13");
    assert_eq!(app.screen, Screen::Nethack);
    assert!(!app.show_hub_modal);

    // Backtick detaches: with no other workspace stops the cycle wraps to
    // Home chat, and the running state survives for resume.
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard);
    assert!(
        app.nethack_state
            .as_ref()
            .is_some_and(|state| state.is_running()),
        "expected the detached game to stay alive"
    );

    // From Home, the same backtick hops back into the live dungeon.
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Nethack);

    // Detach again, then let the fabricated proxy die (its bridge task fails
    // to connect once polled): the next tick reaps the dead detached state so
    // the hub card stops advertising a live game.
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        app.tick(),
        "expected the reaping tick to dirty the frame so the hub pip clears"
    );
    assert!(
        app.nethack_state.is_none(),
        "expected the dead detached game to be dropped"
    );
}

#[tokio::test]
async fn backtick_hops_out_of_lateania_and_back_in_while_the_window_is_live() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "lateania-detach-flow").await;
    let mut app = make_app(test_db.db.clone(), user.id, "lateania-detach-flow-it");

    app.set_screen(Screen::Lateania);
    app.enter_lateania();
    assert!(app.lateania_state.is_some(), "the world is live");

    // Backtick hops out: unlike the roguelikes the session tears down (the
    // character autosaves out of the world), but the recency window keeps
    // Lateania on the cycle. With no other stops the hop wraps to Home chat.
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard);
    assert!(
        app.lateania_state.is_none(),
        "expected the hop-out to drop the per-session world state"
    );
    assert!(
        app.lateania_recently_active(),
        "expected the detach to arm the recency window"
    );

    // From Home, the same backtick re-joins the saved character directly,
    // skipping the character-select landing.
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Lateania);
    assert!(
        app.lateania_state.is_some(),
        "expected the hop-in to re-enter the world"
    );

    // Hop out again, then clear the window: without it Lateania is no longer
    // a stop, so backtick from Home has nowhere to go.
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard);
    app.lateania_detached_at = None;
    app.handle_input(b"`");
    assert_eq!(
        app.screen,
        Screen::Dashboard,
        "expected no hop once the recency window is gone"
    );
    assert!(app.lateania_state.is_none());
}

#[tokio::test]
async fn backtick_hops_out_of_darkroom_and_the_idle_deadline_ends_the_visit() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "darkroom-detach-flow").await;
    let mut app = make_app(test_db.db.clone(), user.id, "darkroom-detach-flow-it");

    app.set_screen(Screen::Darkroom);
    app.enter_darkroom();
    assert!(app.darkroom_state.is_some(), "the door is loaded");

    // Backtick hops out with the door still loaded (the village goes on
    // growing off-screen). With no other stops the hop wraps to Home chat.
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard);
    assert!(
        app.darkroom_state.is_some(),
        "expected the hop-out to keep the door loaded"
    );

    // From Home, the same backtick hops straight back in.
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Darkroom);

    // Sitting on the door reading is not being away: with the idle deadline
    // forced, a tick with the door as the open screen keeps the visit alive
    // and stamps presence, so a keyless exit right after (a Ctrl+G lobby
    // jump, played here as a bare screen switch) inherits none of the
    // banked on-screen time.
    app.darkroom_state
        .as_mut()
        .expect("darkroom state")
        .force_idle_for_test();
    app.tick();
    assert!(
        app.darkroom_state.is_some(),
        "expected the open screen to shield the door from the reap"
    );
    app.set_screen(Screen::Dashboard);
    app.tick();
    assert!(
        app.darkroom_state.is_some(),
        "expected on-screen time not to count toward the idle deadline"
    );

    // Now actually go away for half an hour: the next tick ends the visit
    // exactly as an explicit leave would, and the door stops being a stop
    // on the cycle.
    app.darkroom_state
        .as_mut()
        .expect("darkroom state")
        .force_idle_for_test();
    assert!(
        app.tick(),
        "expected the reaping tick to dirty the frame so the hub pip clears"
    );
    assert!(
        app.darkroom_state.is_none(),
        "expected the idle deadline to save and drop the door"
    );
    app.handle_input(b"`");
    assert_eq!(
        app.screen,
        Screen::Dashboard,
        "expected no hop once the door has idled out"
    );
}

#[tokio::test]
async fn backtick_hops_out_of_green_dragon_and_the_idle_deadline_ends_the_visit() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "greendragon-detach-flow").await;
    let mut app = make_app(test_db.db.clone(), user.id, "greendragon-detach-flow-it");

    app.set_screen(Screen::GreenDragon);
    app.enter_greendragon();
    assert!(app.greendragon_state.is_some(), "the door is loaded");

    // Same hop-out/hop-in as Dark Room: the character stays loaded (and so
    // still listed as online) across the hop.
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard);
    assert!(
        app.greendragon_state.is_some(),
        "expected the hop-out to keep the character loaded"
    );
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::GreenDragon);

    // Same presence rule as Dark Room: an open door is never reaped, and the
    // on-screen tick stamps the clock, so a keyless exit does not inherit
    // the banked on-screen time.
    app.greendragon_state
        .as_mut()
        .expect("greendragon state")
        .force_idle_for_test();
    app.tick();
    assert!(
        app.greendragon_state.is_some(),
        "expected the open screen to shield the door from the reap"
    );
    app.set_screen(Screen::Dashboard);
    app.tick();
    assert!(
        app.greendragon_state.is_some(),
        "expected on-screen time not to count toward the idle deadline"
    );

    app.greendragon_state
        .as_mut()
        .expect("greendragon state")
        .force_idle_for_test();
    assert!(
        app.tick(),
        "expected the reaping tick to dirty the frame so the hub pip clears"
    );
    assert!(
        app.greendragon_state.is_none(),
        "expected the idle deadline to save and drop the character"
    );
}

#[tokio::test]
async fn backtick_is_refused_mid_fight_in_green_dragon() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "greendragon-fight-hold").await;
    let mut app = make_app(test_db.db.clone(), user.id, "greendragon-fight-hold-it");

    app.set_screen(Screen::GreenDragon);
    app.enter_greendragon();
    app.greendragon_state
        .as_mut()
        .expect("greendragon state")
        .force_fight_for_test();

    // Esc mid-fight is a flee roll, and PvP, dragon, and master fights refuse
    // to let you run at all; a backtick hop would sidestep all of that (the
    // idle reap would erase the encounter), so the key stays with the fight.
    app.handle_input(b"`");
    assert_eq!(
        app.screen,
        Screen::GreenDragon,
        "expected the hop to be refused mid-fight"
    );
    assert!(app.greendragon_state.is_some());
}

#[tokio::test]
async fn games_hub_config_modal_saves_and_clears_the_door_rc() {
    use crate::app::common::primitives::Screen;
    use crate::app::door::hub::state::HubGame;
    use late_core::models::door_rc::{DoorRc, DoorRcGame};

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "door-rc-flow").await;
    let client = test_db.db.get().await.expect("db client");
    let mut app = make_app(test_db.db.clone(), user.id, "door-rc-flow-it");

    // Walk the hub sidebar down to NetHack and open the config box. The step
    // count comes from the selector order itself, so a game inserted above
    // NetHack moves the cursor here instead of opening another game's config.
    // A fresh account is no runner, so its roster has no Night City.
    let steps = HubGame::roster(false)
        .iter()
        .position(|game| *game == HubGame::Nethack)
        .expect("nethack is in the selector");
    app.set_screen(Screen::Games);
    app.handle_input(&b"j".repeat(steps));
    app.handle_input(b"c");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("NetHack config (.nethackrc)"),
        "expected the rc modal title; frame={frame:?}"
    );
    assert!(
        frame.contains("No custom config yet"),
        "expected the empty state before any paste; frame={frame:?}"
    );

    // A bracketed paste replaces the whole file: preview updates at once, the
    // DB row lands via the fire-and-forget save.
    app.handle_input(b"\x1b[200~OPTIONS=autopickup\nOPTIONS=color\x1b[201~");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("OPTIONS=autopickup"),
        "expected the pasted config in the preview; frame={frame:?}"
    );
    assert!(
        frame.contains(".nethackrc saved (2 lines)"),
        "expected the save banner; frame={frame:?}"
    );
    wait_until(
        || async {
            DoorRc::get(&client, user.id, DoorRcGame::Nethack)
                .await
                .expect("get door rc")
                .as_deref()
                == Some("OPTIONS=autopickup\nOPTIONS=color")
        },
        "nethack rc row saved",
    )
    .await;

    // `x` clears: back to the empty state, row deleted.
    app.handle_input(b"x");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("No custom config yet"),
        "expected the empty state after clearing; frame={frame:?}"
    );
    wait_until(
        || async {
            DoorRc::get(&client, user.id, DoorRcGame::Nethack)
                .await
                .expect("get door rc")
                .is_none()
        },
        "nethack rc row cleared",
    )
    .await;

    // Esc closes the modal and stays on the hub. The lone ESC is held for
    // escape-sequence disambiguation, so give it a moment to dispatch.
    app.handle_input(b"\x1b");
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(app.screen, Screen::Games);
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("NetHack config (.nethackrc)"),
        "expected the rc modal to close on Esc; frame={frame:?}"
    );

    // A screen switch that bypasses Esc (e.g. a reserved chord into a lobby
    // game) must not leave the modal armed to reappear on the next hub visit.
    app.handle_input(b"c");
    app.set_screen(Screen::Dashboard);
    app.set_screen(Screen::Games);
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("NetHack config (.nethackrc)"),
        "expected the rc modal to be dropped when leaving the hub; frame={frame:?}"
    );
}

#[tokio::test]
async fn account_delete_confirmation_rejects_wrong_username_in_dialog() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "account-delete-flow").await;
    let mut app = make_app(test_db.db.clone(), user.id, "account-delete-flow-it");

    app.handle_input(b"\x0f");
    wait_for_render_contains(&mut app, "Theme").await;

    // The ordinary Ctrl+O route opens and closes Settings before the more
    // specialized account-deletion dialog is exercised in the same app.
    app.handle_input(b"\x1b");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Theme"),
        "expected Esc to close settings; frame={frame:?}"
    );

    app.handle_input(b"\x0f");
    wait_for_render_contains(&mut app, "Account").await;
    wait_for_render_contains(&mut app, "account-delete-flow").await;
    for _ in 0..5 {
        app.handle_input(b"\t");
    }
    // Delete Account is the last row and the cursor clamps there, so one
    // press per row lands on it however many rows sit above it.
    app.handle_input(&b"j".repeat(crate::app::settings_modal::state::AccountRow::ALL.len()));
    wait_for_render_contains(&mut app, "Delete Account").await;

    app.handle_input(b"\rwrong-name\r");
    wait_for_render_contains(&mut app, "Typed username does not match current username.").await;

    app.handle_input(b"\x1b");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Typed username does not match current username."),
        "expected Esc to dismiss delete confirmation; frame={frame:?}"
    );
}

/// Usernames run to 32 characters, past the list's name column: a long one
/// still reads as a name and then its status, not as one run-on word.
#[tokio::test]
async fn invites_dialog_keeps_a_long_username_apart_from_its_status() {
    use late_core::models::referral::{Referral, ReferralSource};
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "invites-dialog-it").await;
    let invitee = create_test_user(&test_db.db, "an-invitee-with-a-very-long-name").await;
    assert!(invitee.username.len() > 20);
    let client = test_db.db.get().await.expect("db client");
    assert!(
        Referral::attach(
            &**client,
            invitee.id,
            user.id,
            ReferralSource::Ssh,
            crate::app::referral::state::judged_until(invitee.created),
        )
        .await
        .expect("attach")
    );
    let mut app = make_app(test_db.db.clone(), user.id, "invites-dialog-flow-it");

    app.handle_input(b"\x0f");
    wait_for_render_contains(&mut app, "invites-dialog-it").await;
    for _ in 0..5 {
        app.handle_input(b"\t");
    }
    // Invites is the first Account row.
    wait_for_render_contains(&mut app, "Delete Account").await;
    app.handle_input(b"\r");

    wait_for_render_contains(&mut app, &format!("@{} settling in", invitee.username)).await;
}

#[tokio::test]
async fn screen_number_keys_switch_between_pages_including_profiles() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "screen-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "screen-flow-it");

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " The Arcade ").await;

    app.handle_input(b"3");
    wait_for_render_contains(&mut app, " Games ").await;

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"5");
    wait_for_render_contains(&mut app, " Profiles ").await;

    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Home ").await;
}

/// A lone Esc is parsed as `pending_escape` and only dispatches on a later
/// tick (see `flush_pending_escape`), so after sending it the test must tick
/// until the effect lands before typing anything else.
async fn wait_for_esc_effect(
    app: &mut crate::app::state::App,
    done: impl Fn(&crate::app::state::App) -> bool,
    label: &str,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !done(app) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for esc effect: {label}"
        );
        app.tick();
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

#[tokio::test]
async fn profiles_page_keys_drive_the_merged_feed() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "profiles-feed-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "profiles-feed-flow-it");

    app.handle_input(b"5");
    wait_for_render_contains(&mut app, " Profiles ").await;

    // `i` opens the profile editor on a blank project form. The form's first
    // row is already being typed into, so one Esc stops typing and a second
    // leaves the untouched form for the projects list; a third closes.
    app.handle_input(b"i");
    wait_for_render_contains(&mut app, " Your profile ").await;
    assert!(app.directory_editor.editing());
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| !app.directory_editor.editing(),
        "stop typing",
    )
    .await;
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| {
            matches!(
                app.directory_editor.projects_view(),
                crate::app::directory::editor::state::ProjectsView::List { .. }
            )
        },
        "back to the list",
    )
    .await;

    // `a` on the list opens a fresh project form; the letter arrives through
    // the real parser, so this pins the list keys end to end. Esc twice
    // returns to the list and closes the editor.
    app.handle_input(b"a");
    assert!(
        app.directory_editor.editing(),
        "`a` should open a new project form"
    );
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| !app.directory_editor.editing(),
        "stop typing again",
    )
    .await;
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| {
            matches!(
                app.directory_editor.projects_view(),
                crate::app::directory::editor::state::ProjectsView::List { .. }
            )
        },
        "back to the list again",
    )
    .await;
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| !app.directory_editor.is_open(),
        "editor closed",
    )
    .await;

    // `w` opens the same editor on the card page, Esc closes it untouched.
    app.handle_input(b"w");
    wait_for_render_contains(&mut app, " Your profile ").await;
    assert_eq!(
        app.directory_editor.page(),
        crate::app::directory::editor::state::Page::Card
    );
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| !app.directory_editor.is_open(),
        "editor closed",
    )
    .await;

    // A touched card asks before closing. Only `y` discards: Enter, the key
    // a hand lands on by reflex, keeps the question up.
    app.handle_input(b"w");
    wait_for_render_contains(&mut app, " Your profile ").await;
    app.handle_input(b"\rx");
    assert!(app.directory_editor.dirty(), "typed into the headline");
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| !app.directory_editor.editing(),
        "stop typing the headline",
    )
    .await;
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| app.directory_editor.confirm_discard(),
        "asked to discard",
    )
    .await;
    app.handle_input(b"\r");
    assert!(
        app.directory_editor.confirm_discard() && app.directory_editor.is_open(),
        "enter must not discard"
    );
    app.handle_input(b"y");
    assert!(!app.directory_editor.is_open(), "y discards and closes");

    // `s` opens feed search, Esc dismisses it.
    app.handle_input(b"s");
    wait_for_render_contains(&mut app, " Search ").await;
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.directory_state.search_mode(), "search").await;
    assert_render_not_contains_for(&mut app, " Search ", Duration::from_millis(200)).await;
}

#[tokio::test]
async fn shift_tab_cycles_screens_backwards() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "screen-backtab-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "screen-backtab-flow-it");

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " Clubhouse ").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " Leaderboards ").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, "Profiles").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " Games ").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " The Arcade ").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " Home ").await;
}

#[tokio::test]
async fn tab_cycles_screens_forward_through_all_including_profiles() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "screen-tab-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "screen-tab-flow-it");

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, " The Arcade ").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, " Games ").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, " Profiles ").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, " Leaderboards ").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, " Clubhouse ").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, " Home ").await;
}

#[tokio::test]
async fn zero_twice_goes_under_the_clubhouse_for_runners_only() {
    use crate::app::deadchannel::runner::state::Look;
    use crate::app::deadchannel::runner::svc::RunnerEntry;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashMap;
    use std::sync::Arc;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "undercity-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "undercity-flow-it");

    // Not a runner: `0` lands on the clubhouse and stays there.
    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Home ").await;
    wait_for_render_contains(&mut app, "lounge").await;
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Clubhouse ").await;
    app.handle_input(b"0");
    assert_render_not_contains_for(&mut app, " Undercity ", Duration::from_millis(200)).await;

    // A runner: the second `0` goes under, the next one comes back up.
    let mut rng = StdRng::seed_from_u64(7);
    app.runner_looks = Arc::new(HashMap::from([(
        user.id,
        RunnerEntry {
            look: Look::random(1, &mut rng),
            level: 1,
            peak_level: 1,
            marks: 0,
        },
    )]));
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Undercity ").await;
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Clubhouse ").await;

    // Esc on the bare street goes up to the chat, #lounge open.
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Undercity ").await;
    app.handle_input(b"\x1b");
    wait_for_render_contains(&mut app, " Home ").await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains(" Undercity "),
        "expected Esc on the street to leave the city; frame={frame:?}"
    );
    assert_eq!(
        app.chat.selected_room_id,
        Some(lounge.id),
        "expected Esc on the street to land in #lounge"
    );
}

/// `/leave #deadchannel` on one session closes the street under every
/// session the runner has open, on this replica and every other: the looks
/// directory drops them, and the tick edge that copies it walks them back
/// up. The gate on `0` only guards the descent.
#[tokio::test]
async fn leaving_the_deadchannel_walks_a_standing_runner_back_up() {
    use crate::app::deadchannel::runner::state::Look;
    use crate::app::deadchannel::runner::svc::RunnerEntry;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashMap;
    use std::sync::Arc;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "undercity-leave-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "undercity-leave-flow-it");

    // A live directory, the shape the replica's listener feeds.
    let mut rng = StdRng::seed_from_u64(7);
    let (looks_tx, looks_rx) = tokio::sync::watch::channel(Arc::new(HashMap::from([(
        user.id,
        RunnerEntry {
            look: Look::random(1, &mut rng),
            level: 1,
            peak_level: 1,
            marks: 0,
        },
    )])));
    app.runner_looks = looks_rx.borrow().clone();
    app.runner_looks_rx = looks_rx;

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Clubhouse ").await;
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Undercity ").await;

    // The leave lands (on any session, on any replica): the directory drops
    // the runner, and this session cannot stay down there.
    looks_tx.send_replace(Arc::new(HashMap::new()));
    wait_for_render_not_contains(&mut app, " Undercity ").await;
    assert!(!app.is_runner());
    let frame = render_plain(&mut app);
    assert!(
        frame.contains(" Clubhouse "),
        "expected the leave to walk the runner up to the clubhouse; frame={frame:?}"
    );
}

/// A runner's session parked on the Undercity, one row north of the wire
/// stairs: at the railing, where the popover offers the ledge.
async fn runner_at_the_railing(
    name: &str,
) -> (late_core::test_utils::TestDb, crate::app::state::App) {
    use crate::app::deadchannel::runner::state::Look;
    use crate::app::deadchannel::runner::svc::RunnerEntry;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashMap;
    use std::sync::Arc;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, name).await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, &format!("{name}-flow"));
    let mut rng = StdRng::seed_from_u64(7);
    app.runner_looks = Arc::new(HashMap::from([(
        user.id,
        RunnerEntry {
            look: Look::random(1, &mut rng),
            level: 1,
            peak_level: 1,
            marks: 0,
        },
    )]));

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Clubhouse ").await;
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Undercity ").await;
    app.handle_input(b"k");
    wait_for_render_contains(&mut app, "look over").await;
    (test_db, app)
}

#[tokio::test]
async fn esc_steps_back_from_the_ledge() {
    let (_test_db, mut app) = runner_at_the_railing("undercity-esc-it").await;

    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "step back").await;
    // A lone Esc lands on a later tick; the render wait ticks it in.
    app.handle_input(b"\x1b");
    wait_for_render_not_contains(&mut app, "step back").await;
    wait_for_render_contains(&mut app, "look over").await;
}

#[tokio::test]
async fn leaving_the_city_mid_look_closes_it_for_the_next_descent() {
    let (_test_db, mut app) = runner_at_the_railing("undercity-leave-it").await;

    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "step back").await;
    // Up with the page key while looking over, then back down: the street,
    // not the old view.
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Clubhouse ").await;
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Undercity ").await;
    assert_render_not_contains_for(&mut app, "step back", Duration::from_millis(200)).await;
    wait_for_render_contains(&mut app, "look over").await;
}

#[tokio::test]
async fn global_ctrl_o_opens_settings_on_dashboard() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-o-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "ctrl-o-flow-it");
    wait_for_render_contains(&mut app, " Home ").await;

    // Ctrl+O opens settings modal
    app.handle_input(b"\x0f");
    wait_for_render_contains(&mut app, "Theme").await;

    // Esc to close settings, back to Home
    app.handle_input(b"\x1b");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Theme"),
        "expected Esc to close settings; frame={frame:?}"
    );
}

#[tokio::test]
async fn global_ctrl_g_toggles_lobby_and_ctrl_s_or_slash_shop_opens_shop() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-g-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "ctrl-g-flow-it");
    wait_for_render_contains(&mut app, " Home ").await;

    // Ctrl+G owns the Lobby now; the same chord closes it again. The footer
    // always says " Lobby ", so assert on modal-only section copy instead.
    app.handle_input(b"\x07");
    wait_for_render_contains(&mut app, "house tables").await;
    app.handle_input(b"\x07");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("house tables"),
        "expected Ctrl+G to close the lobby; frame={frame:?}"
    );

    app.handle_input(b"\x13");
    wait_for_render_contains(&mut app, "-- Shop --").await;
    app.handle_input(b"\x1b");
    wait_for_render_not_contains(&mut app, "-- Shop --").await;
    assert!(!app.show_hub_modal);

    // Ctrl+S also opens Shop while composing and preserves the draft.
    wait_for_render_contains(&mut app, "lounge").await;
    app.handle_input(b"iunfinished draft");
    app.handle_input(b"\x13");
    wait_for_render_contains(&mut app, "-- Shop --").await;
    app.handle_input(b"\x1b");
    wait_for_render_not_contains(&mut app, "-- Shop --").await;
    wait_for_render_contains(&mut app, "unfinished draft").await;
    app.handle_input(b"\x15");
    app.handle_input(b"\x1b");
    wait_for_render_contains(&mut app, "Compose (press i)").await;

    // /shop in the composer opens the same modal, Esc closes.
    // Composing needs a selected room, so wait for the lounge row first.
    wait_for_render_contains(&mut app, "lounge").await;
    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Compose (Enter send").await;
    app.handle_input(b"/shop\r");
    wait_for_render_contains(&mut app, "-- Shop --").await;
    app.handle_input(b"\x1b");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("-- Shop --"),
        "expected Esc to close the shop; frame={frame:?}"
    );
}

#[tokio::test]
async fn ctrl_s_stays_in_arcade_games_but_opens_shop_from_the_menu() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-s-arcade-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "ctrl-s-arcade-flow-it");
    app.set_screen(Screen::Arcade);
    app.game_selection = crate::app::state::GAME_SELECTION_2048;
    app.handle_input(b"\r");
    assert!(app.is_playing_game);

    app.handle_input(b"\x13");
    assert!(!app.show_hub_modal);
    assert!(app.is_playing_game);
    assert_eq!(app.screen, Screen::Arcade);

    // Leaving the board restores the Shop shortcut on the game menu.
    app.handle_input(b"q");
    assert!(!app.is_playing_game);
    app.handle_input(b"\x13");
    assert!(app.show_hub_modal);
}

#[tokio::test]
async fn ctrl_s_stays_in_native_games() {
    use crate::app::common::primitives::Screen;
    use crate::app::lobby::house::tables::HouseTable;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-s-native-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "ctrl-s-native-flow-it");

    for screen in [
        Screen::Lateania,
        Screen::GreenDragon,
        Screen::Darkroom,
        Screen::DailyMatch,
        Screen::HouseTable,
        Screen::City,
    ] {
        app.set_screen(screen);
        match screen {
            Screen::Lateania => app.enter_lateania(),
            Screen::GreenDragon => app.enter_greendragon(),
            Screen::Darkroom => app.enter_darkroom(),
            Screen::HouseTable => assert!(app.house.enter(
                HouseTable::Blackjack,
                Screen::Dashboard,
                app.chip_balance
            )),
            _ => {}
        }
        app.handle_input(b"\x13");
        assert!(!app.show_hub_modal, "Ctrl+S must stay in {screen:?}");
        assert_eq!(app.screen, screen);
    }

    // Live sessions left behind must not swallow Ctrl+S on other pages.
    app.set_screen(Screen::Games);
    app.handle_input(b"\x13");
    assert!(app.show_hub_modal);
}

#[tokio::test]
async fn ctrl_s_opens_shop_from_door_launchers() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-s-launchers-it").await;
    for screen in [
        Screen::Lateania,
        Screen::GreenDragon,
        Screen::Darkroom,
        Screen::Nethack,
    ] {
        let mut app = make_app(test_db.db.clone(), user.id, "ctrl-s-launchers-flow-it");
        app.set_screen(screen);
        app.handle_input(b"\x13");
        assert!(
            app.show_hub_modal,
            "Shop is available on the {screen:?} launcher"
        );
    }
}

#[tokio::test]
async fn ctrl_s_keeps_profile_save_and_job_post_bindings() {
    use crate::app::directory::editor::{input, state::Page};

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-s-editors-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "ctrl-s-editors-flow-it");
    wait_for_render_contains(&mut app, " Home ").await;

    input::open_own(&mut app, Page::About);
    assert!(app.directory_editor.is_open());
    app.handle_input(b"\x13");
    assert!(
        !app.directory_editor.is_open(),
        "save closes an unchanged profile"
    );
    assert!(!app.show_hub_modal);

    app.jobs.post.open();
    app.handle_input(b"\x13");
    assert!(
        app.jobs.post.error().is_some(),
        "posting validates the empty form"
    );
    assert!(app.jobs.post.is_open());
    assert!(!app.show_hub_modal);
}

/// A Settings text field being edited holds typing that is not saved yet, so
/// the chords that would close or reopen the modal leave it alone. Ctrl+S
/// matters most: it is a save habit.
#[tokio::test]
async fn modal_chords_leave_a_settings_text_editor_alone() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "chords-bio-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "chords-bio-flow-it");
    wait_for_render_contains(&mut app, " Home ").await;

    app.handle_input(b"\x0f"); // Settings
    app.handle_input(b"\t"); // Bio tab
    app.handle_input(b"\r"); // start editing
    assert!(app.settings_modal_state.editing_bio());
    app.handle_input(b"late night coder");

    for (chord, name) in [
        (b"\x0f", "Ctrl+O"),
        (b"\x07", "Ctrl+G"),
        (b"\x06", "Ctrl+F"),
        (b"\x13", "Ctrl+S"),
    ] {
        app.handle_input(chord);
        assert!(app.show_settings, "{name} leaves Settings open");
        assert!(
            app.settings_modal_state.editing_bio(),
            "{name} leaves the bio editor open"
        );
        assert!(!app.show_lobby_modal, "{name} opens no Lobby");
        assert!(!app.show_hub_modal, "{name} opens no Shop");
        assert_ne!(app.screen, Screen::Zen, "{name} opens no Zen");
    }

    app.handle_input(b"\r"); // Enter leaves edit mode and saves
    assert_eq!(app.settings_modal_state.draft().bio, "late night coder");
}

/// `/lobby`, `/zen`, and `/guide` are the typed fallbacks for Ctrl+G, Ctrl+F,
/// and `?`, for terminals that swallow the chords.
#[tokio::test]
async fn slash_lobby_zen_and_guide_mirror_their_keys() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "slash-nav-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "slash-nav-flow-it");
    wait_for_render_contains(&mut app, " Home ").await;
    wait_for_render_contains(&mut app, "lounge").await;

    app.handle_input(b"i/lobby\r");
    wait_for_render_contains(&mut app, "house tables").await;
    app.handle_input(b"\x07");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("house tables"),
        "expected Ctrl+G to close the lobby /lobby opened; frame={frame:?}"
    );

    app.handle_input(b"i/guide\r");
    wait_for_render_contains(&mut app, " Guide ").await;
    app.handle_input(b"?");
    assert!(!app.show_help, "? should close the guide /guide opened");

    app.handle_input(b"i/zen\r");
    assert_eq!(app.screen, Screen::Zen);
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Dashboard);

    // /redraw re-emits every cell, the way Ctrl+R does: the frame after it
    // carries more than a settled diff.
    let _ = app.render().expect("render");
    let settled = strip_ansi(&String::from_utf8_lossy(&app.render().expect("render")));
    app.handle_input(b"i/redraw\r");
    let repainted = strip_ansi(&String::from_utf8_lossy(&app.render().expect("render")));
    assert!(
        repainted.contains("lounge") && repainted.len() > settled.len(),
        "expected /redraw to repaint the whole screen; settled={} bytes, repainted={} bytes",
        settled.len(),
        repainted.len()
    );
}

#[tokio::test]
async fn global_w_opens_bonsai_care() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "w-bonsai-mod-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        user.id,
        "w-bonsai-mod-flow-it",
        Permissions::new(false, true),
    );
    wait_for_render_contains(&mut app, " Home ").await;

    app.handle_input(b"w");
    wait_for_render_contains(&mut app, " Bonsai ").await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Day 0") && frame.contains("vigor"),
        "expected w to open the bonsai care modal; frame={frame:?}"
    );
}

#[tokio::test]
async fn zen_yields_the_music_chord_and_w_to_bonsai_care() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "zen-chords-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        user.id,
        "zen-chords-flow-it",
        Permissions::new(false, true),
    );
    wait_for_render_contains(&mut app, " Home ").await;
    app.handle_input(b"\x06");
    wait_for_render_contains(&mut app, "w tend").await;
    let tiles = app.zen.leaf_count();
    let source = app.paired_source;

    // `v x` is the global audio-source chord. The page used to claim `x`
    // (and `X` still closes a tile), so the suffix must reach the chord.
    app.handle_input(b"v");
    app.handle_input(b"x");
    assert_ne!(
        app.paired_source, source,
        "v x swaps the audio source on Zen"
    );
    assert_eq!(
        app.zen.leaf_count(),
        tiles,
        "the chord suffix closes no tile"
    );

    // The bonsai is tended in the same modal as everywhere else.
    app.handle_input(b"w");
    wait_for_render_contains(&mut app, " Bonsai ").await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Day 0") && frame.contains("vigor"),
        "expected w on Zen to open the bonsai care modal; frame={frame:?}"
    );
}

#[tokio::test]
async fn global_ctrl_b_is_ignored_for_all_users() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-b-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "ctrl-b-flow-it");
    wait_for_render_contains(&mut app, " Home ").await;

    for (label, permissions) in [
        ("regular", Permissions::default()),
        ("admin", Permissions::new(true, false)),
        ("moderator", Permissions::new(false, true)),
    ] {
        app.set_permissions(permissions);
        app.handle_input(b"\x02");
        let frame = render_plain(&mut app);
        assert!(
            !frame.contains(" Bonsai ") && !app.show_bonsai_modal,
            "expected Ctrl+B to stay inert for {label}; frame={frame:?}"
        );
    }
}

#[tokio::test]
async fn artboard_view_help_and_active_input_share_one_lifecycle() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-view-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-view-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Mode       view").await;
    wait_for_render_contains(&mut app, "Cursor     0,0").await;

    // The page lands on the rail; Enter on Board hands the keys to the
    // board cursor.
    wait_for_render_contains(&mut app, "rail j/k").await;
    app.handle_input(b"\r");
    app.handle_input(b"\x1b[C");
    wait_for_render_contains(&mut app, "Cursor     1,0").await;

    // Vim keys move the view-mode cursor like the arrows, and never paint.
    app.handle_input(b"l");
    wait_for_render_contains(&mut app, "Cursor     2,0").await;
    app.handle_input(b"j");
    wait_for_render_contains(&mut app, "Cursor     2,1").await;
    app.handle_input(b"k");
    wait_for_render_contains(&mut app, "Cursor     2,0").await;
    app.handle_input(b"h");
    wait_for_render_contains(&mut app, "Cursor     1,0").await;
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"\x10");
    wait_for_render_contains(&mut app, "Two modes").await;
    assert!(
        render_plain(&mut app).contains("Artboard Help"),
        "Ctrl+P in view mode should open local Artboard help"
    );

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, "Draw / erase").await;
    assert!(
        render_plain(&mut app).contains("Artboard Help"),
        "help Tab should stay on Artboard instead of switching page"
    );
    app.handle_input(b"q");
    assert!(
        !render_plain(&mut app).contains("Artboard Help"),
        "q should close local Artboard help"
    );

    // `?` is the global guide here as on every page; Ctrl+P is the
    // Artboard's own help.
    app.handle_input(b"?");
    wait_for_render_contains(&mut app, " Guide ").await;
    assert!(
        !render_plain(&mut app).contains("Artboard Help"),
        "? in view mode should open the global guide, not the local help"
    );
    app.handle_input(b"?");
    assert!(!app.show_help, "? should close the guide");

    // The rail folded away when the board took the keys, so the board's
    // cell (8, 3) is at screen column 10.
    app.handle_input(b"\x1b[<0;10;5M");
    wait_for_render_contains(&mut app, "Mode       active").await;
    wait_for_render_contains(&mut app, "Cursor     8,3").await;

    app.handle_input(b"1");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Mode       active"),
        "active mode should keep focus after numeric hotkeys; frame={frame:?}"
    );
    assert!(
        !frame.contains(" Home "),
        "active mode should block screen switching; frame={frame:?}"
    );

    app.handle_input(b"\x13");
    assert!(!app.show_hub_modal, "Artboard retains Ctrl+S for slot 2");

    app.handle_input(b"\x03");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Mode       swatch"),
        "Ctrl+C should copy into the primary swatch; frame={frame:?}"
    );
    assert!(
        !frame.contains(" Quit? "),
        "Ctrl+C should avoid the global quit flow; frame={frame:?}"
    );

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Mode       active").await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Tab/S+Tab"),
        "? in active mode should type into the canvas instead of opening help; frame={frame:?}"
    );

    app.handle_input(b"\x1b");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Home ").await;
}

#[tokio::test]
async fn artboard_ban_locks_user_in_view_mode() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-banned-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-banned-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Mode       view").await;
    app.set_artboard_banned_for_tests(true);

    app.handle_input(b"i");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Artboard editing is disabled for this account."),
        "expected artboard ban notice; frame={frame:?}"
    );
    assert!(
        !frame.contains("Mode       active"),
        "expected artboard ban to block active mode; frame={frame:?}"
    );

    app.handle_input(b"\x1b[<0;10;5M");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Mode       active"),
        "expected artboard ban to block click-to-edit; frame={frame:?}"
    );
}

#[tokio::test]
async fn chat_compose_preserves_screen_digits_and_non_ascii_text() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dash-chat-compose-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "dash-chat-compose-flow-it");

    // Wait for the Home chat rail so the room snapshot has populated
    // `lounge_room_id` before exercising composer-owned global shortcuts.
    wait_for_render_contains(&mut app, "lounge").await;
    wait_for_render_contains(&mut app, " Home ").await;

    app.handle_input(b"i3abc");
    wait_for_render_contains(&mut app, " Home ").await;
    wait_for_render_contains(&mut app, "3abc").await;

    app.handle_input(b"\x15"); // Ctrl+U clears the composer without closing it.
    app.handle_input(b"2hey");
    wait_for_render_contains(&mut app, "2hey").await;
    wait_for_render_contains(&mut app, "Compose (Enter send").await;

    // Real terminals send CR (0x0D) for Enter in raw mode. Bare LF (0x0A) is
    // Ctrl+J and is aliased to "insert newline in chat composer", so we'd
    // end up composing "2hey\n" instead of submitting.
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Compose (press i)").await;

    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Compose (Enter send").await;
    for (label, input) in [
        ("cyrillic", "тест"),
        ("han", "漢字"),
        ("latin diacritic", "café"),
        ("greek", "αβγ"),
    ] {
        app.handle_input(input.as_bytes());
        wait_for_render_contains(&mut app, input).await;
        assert_eq!(
            app.chat.composer().lines(),
            &[input.to_string()],
            "composer contents for {label}"
        );
        app.handle_input(b"\x15");
        assert_eq!(
            app.chat.composer().lines(),
            &[String::new()],
            "composer should clear after {label}"
        );
    }

    app.handle_input(b"q$$$");
    wait_for_render_contains(&mut app, "q$$$").await;
    assert!(
        !render_plain(&mut app).contains(" Quit? "),
        "q in the composer must remain text rather than opening quit confirm"
    );
    app.handle_input(b"\x15");

    app.handle_input(b"one two");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("one") && frame.contains("two"),
        "expected compose render to show the initial text; frame={frame:?}"
    );

    // Simulate a terminal splitting Alt+Backspace across reads: lone ESC
    // first, then DEL on the next input chunk.
    app.handle_input(b"\x1b");
    app.handle_input(b"\x7f");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("│one │") || frame.contains("│one  │"),
        "expected split Alt+Backspace to leave the composer in the intermediate `one ` state (allowing for the cursor cell to render as an extra blank); frame={frame:?}"
    );
    assert!(
        !frame.contains("two"),
        "expected split Alt+Backspace to delete the previous word; frame={frame:?}"
    );

    // Plain Backspace must still work after the word-delete chord. Insert a
    // fresh sentinel byte first so we can verify backspace removed it without
    // depending on whether delete-word keeps the separating space.
    app.handle_input(b"x\x7f!");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("one")
            && frame.contains("!")
            && !frame.contains("onex")
            && !frame.contains("one x"),
        "expected composer to keep accepting backspace and text after Alt+Backspace split, allowing for cursor-cell spacing in the rendered composer; frame={frame:?}"
    );
    assert!(
        !frame.contains("two"),
        "expected Alt+Backspace split read to delete the previous word; frame={frame:?}"
    );
}

#[tokio::test]
async fn chat_room_switch_ctrl_keys_wrap() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "chat-room-switch-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "chat-room-switch-flow-it");

    wait_for_render_contains(&mut app, "lounge").await;

    app.handle_input(b"\x10");
    wait_for_render_contains(&mut app, "+ browse rooms").await;

    app.handle_input(b"\x0e");
    wait_for_render_contains(&mut app, "lounge").await;
}

#[tokio::test]
async fn chat_reaction_leader_routes_cancel_and_reaction_digits() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-react-viewer").await;
    let author = create_test_user(&test_db.db, "f-react-author").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge.id, author.id)
        .await
        .expect("join author");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "reaction target".to_string(),
        },
    )
    .await
    .expect("create message");

    let mut app = make_app(test_db.db.clone(), viewer.id, "f-react-flow-it");
    app.resize(160, 32).expect("resize test terminal");
    wait_for_render_contains(&mut app, "reaction target").await;

    app.handle_input(b"j");
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "1 👍").await;

    // A non-digit closes the leader and is consumed instead of triggering its
    // ordinary message action. Check state directly instead of polling for the
    // absence of a reply banner.
    app.handle_input(b"r");
    assert!(
        !app.chat.is_reaction_leader_active(),
        "non-digit input should close the reaction leader"
    );
    assert!(
        app.chat.reply_target().is_none() && !app.chat.is_composing(),
        "the consumed r should not open a reply composer"
    );
    let plain = render_plain(&mut app);
    assert!(!plain.contains("1 👍"), "picker should close: {plain:?}");
    assert!(
        plain.contains("reaction target"),
        "message should remain selected: {plain:?}"
    );
    assert!(
        ChatMessageReaction::get_by_user_and_message(&client, message.id, viewer.id)
            .await
            .expect("load reaction")
            .is_none(),
        "non-digit input should not react",
    );

    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "1 👍").await;
    app.handle_input(b"1");

    wait_for_render_contains(&mut app, " Home ").await;
    wait_until(
        || async {
            ChatMessageReaction::get_by_user_and_message(&client, message.id, viewer.id)
                .await
                .expect("load reaction")
                .is_some_and(|reaction| reaction.icon == "👍")
        },
        "f leader reaction to persist",
    )
    .await;
    let plain = render_plain(&mut app);
    assert!(
        plain.contains("▸reaction target"),
        "message selection should stay after reacting: {plain:?}"
    );
    assert!(
        !plain.contains("1 👍"),
        "reaction picker should close after reacting: {plain:?}"
    );

    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "1 👍").await;
    app.handle_input(b"5");
    wait_until(
        || async {
            ChatMessageReaction::get_by_user_and_message(&client, message.id, viewer.id)
                .await
                .expect("load reaction")
                .is_some_and(|reaction| reaction.icon == "🔥")
        },
        "extended f leader reaction to persist",
    )
    .await;
}

#[tokio::test]
async fn chat_room_list_is_mouse_clickable() {
    let test_db = new_test_db().await;
    let user = {
        let user = create_test_user(&test_db.db, "chat-room-mouse-it").await;
        let author = create_test_user(&test_db.db, "chat-room-mouse-author-it").await;
        let client = test_db.db.get().await.expect("db client");
        let lounge = ChatRoom::ensure_lounge(&client)
            .await
            .expect("ensure lounge room");
        let rust = ChatRoom::get_or_create_public_room(&client, "rust")
            .await
            .expect("create rust room");
        for room in [lounge.id, rust.id] {
            ChatRoomMember::join(&client, room, user.id)
                .await
                .expect("join viewer");
            ChatRoomMember::join(&client, room, author.id)
                .await
                .expect("join author");
        }
        ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id: rust.id,
                user_id: author.id,
                body: "rust room backlog".to_string(),
            },
        )
        .await
        .expect("create rust message");
        user
    };

    let mut app = make_app(test_db.db.clone(), user.id, "chat-room-mouse-flow-it");
    wait_for_render_contains(&mut app, "rust").await;

    // Click the #rust row in the sidebar. It sits below the Core section
    // (lounge, mentions, news, "+ browse rooms") and the Channels header, at
    // rail row 10 (SGR mouse rows are 1-based).
    app.handle_input(b"\x1b[<0;5;10M");

    wait_for_render_contains(&mut app, "rust room backlog").await;
}

#[tokio::test]
async fn chat_reaction_leader_second_f_shows_reaction_owners_modal() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-owners-viewer").await;
    let author = create_test_user(&test_db.db, "f-owners-author").await;
    let thumbs_1 = create_test_user(&test_db.db, "f-owners-thumbs-1").await;
    let thumbs_2 = create_test_user(&test_db.db, "f-owners-thumbs-2").await;
    let thumbs_3 = create_test_user(&test_db.db, "f-owners-thumbs-3").await;
    let thumbs_4 = create_test_user(&test_db.db, "f-owners-thumbs-4").await;
    let thumbs_5 = create_test_user(&test_db.db, "f-owners-thumbs-5").await;
    let thumbs_6 = create_test_user(&test_db.db, "f-owners-thumbs-6").await;
    let thinking = create_test_user(&test_db.db, "f-owners-thinking").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    for user in [
        &viewer, &author, &thumbs_1, &thumbs_2, &thumbs_3, &thumbs_4, &thumbs_5, &thumbs_6,
        &thinking,
    ] {
        ChatRoomMember::join(&client, lounge.id, user.id)
            .await
            .expect("join user");
    }
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "owner reaction target".to_string(),
        },
    )
    .await
    .expect("create message");
    for user in [
        &thumbs_1, &thumbs_2, &thumbs_3, &thumbs_4, &thumbs_5, &thumbs_6,
    ] {
        ChatMessageReaction::toggle(&client, message.id, user.id, "👍")
            .await
            .expect("thumb reaction");
    }
    ChatMessageReaction::toggle(&client, message.id, thinking.id, "🤔")
        .await
        .expect("thinking reaction");
    // Two gilds at different tiers: the overlay lists them above the
    // reactions, best tier first, with the buyer under each.
    {
        let mut gild_client = test_db.db.get().await.expect("db client");
        let tx = gild_client.transaction().await.expect("tx");
        for (buyer, tier) in [(&thinking, GildTier::Bronze), (&thumbs_1, GildTier::Gold)] {
            let placed = ChatMessageGild::place_in_tx(&tx, message.id, author.id, buyer.id, tier)
                .await
                .expect("place gild");
            assert!(matches!(placed, GildPlacement::Placed(_)), "{placed:?}");
        }
        tx.commit().await.expect("commit gilds");
    }

    let mut app = make_app(test_db.db.clone(), viewer.id, "f-owners-flow-it");
    wait_for_render_contains(&mut app, "owner reaction target").await;

    app.handle_input(b"j");
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "1 👍").await;
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, " Reactions ").await;
    wait_for_render_contains(&mut app, "👍 6 reactions").await;
    wait_for_render_contains(&mut app, "[+2 more]").await;
    wait_for_render_contains(&mut app, "@f-owners-thinking").await;
    wait_for_render_contains(&mut app, "◆◆◆ 1 Gold gild").await;
    wait_for_render_contains(&mut app, "◆ 1 Bronze gild").await;
    let plain = render_plain(&mut app);
    let gold_at = plain.find("◆◆◆ 1 Gold gild").expect("gold block");
    let bronze_at = plain.find("◆ 1 Bronze gild").expect("bronze block");
    let thumbs_at = plain.find("👍 6 reactions").expect("reaction block");
    assert!(
        gold_at < bronze_at && bronze_at < thumbs_at,
        "gilds lead, best tier first, then reactions: {plain:?}"
    );
    assert!(
        plain[gold_at..bronze_at].contains("@f-owners-thumbs-1"),
        "the gold buyer sits under the gold block: {plain:?}"
    );
    assert!(
        !plain.contains("1 👍"),
        "reaction picker should be dismissed under owner modal: {plain:?}"
    );

    app.handle_input(b"\r");
    assert!(
        !app.chat.has_overlay(),
        "Enter should close the owner modal"
    );
    let plain = render_plain(&mut app);
    assert!(
        !plain.contains(" Reactions "),
        "owner modal should stay closed after Enter: {plain:?}"
    );
    assert!(
        !plain.contains("1 👍"),
        "reaction picker should stay dismissed after Enter closes modal: {plain:?}"
    );

    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "1 👍").await;
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, " Reactions ").await;
    app.handle_input(b"f");
    assert!(!app.chat.has_overlay(), "f should close the owner modal");
    assert!(
        !render_plain(&mut app).contains(" Reactions "),
        "owner modal should stay closed after f"
    );

    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "1 👍").await;
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, " Reactions ").await;
    app.handle_input(b"\x1b");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let plain = render_plain(&mut app);
    assert!(!app.chat.has_overlay(), "Esc should close the owner modal");
    assert!(
        !plain.contains(" Reactions "),
        "owner modal should stay closed after Esc: {plain:?}"
    );
}

#[tokio::test]
async fn unlinked_cs_command_offers_the_link_modal_without_leaving_the_room() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "cs-unlinked-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer to lounge");

    let mut app = make_app(test_db.db.clone(), viewer.id, "cs-unlinked-flow-it");
    wait_for_render_contains(&mut app, "lounge").await;

    // No link, no rail entry: the rail stays about places this user has.
    assert_render_not_contains_for(&mut app, "cyberspace", Duration::from_millis(300)).await;

    // /cs is still the way in. It opens the link funnel over the room the
    // user is already in, rather than a pane with no rail entry behind it.
    app.handle_input(b"i/cs\r");
    wait_for_render_contains(&mut app, " Link cyberspace account ").await;
    wait_for_render_contains(&mut app, "https://cyberspace.online").await;
    assert!(
        app.chat.cyberspace.modal_active(),
        "the link modal should own the input"
    );
    assert!(
        !app.chat.cyberspace_selected,
        "an unlinked user should never land in the pane"
    );
}

#[tokio::test]
async fn linked_account_gets_the_rail_entry_and_the_pane() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "cs-linked-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer to lounge");
    CyberspaceAccount::upsert_for_user(&client, viewer.id, "cs-uid", "oddity", "refresh-token")
        .await
        .expect("link cyberspace account");

    let mut app = make_app(test_db.db.clone(), viewer.id, "cs-linked-flow-it");
    wait_for_render_contains(&mut app, "lounge").await;

    // Linking earns the Core rail entry, so the pane is reachable by eye and
    // by click, not only through the command.
    wait_for_render_contains(&mut app, "cyberspace").await;

    app.handle_input(b"i/cs\r");
    wait_for_render_contains(&mut app, "Home · cyberspace").await;
    // The pane header names the account and the notification key, so the
    // rail badge is not the only thing explaining the count.
    wait_for_render_contains(&mut app, "@oddity on cyberspace.online").await;
    // Notifications are their own rail row with their own badge, so the row
    // is where the count is explained now; the pane header speaks for the
    // feed alone.
    wait_for_render_contains(&mut app, "notifications").await;
    assert!(app.chat.cyberspace_selected, "/cs should open the pane");
    assert!(
        !app.chat.cyberspace.modal_active(),
        "a linked user gets the pane, not the link modal"
    );
}

#[tokio::test]
async fn switching_screens_drops_the_open_cyberspace_room() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "cs-room-leaver").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer to lounge");
    CyberspaceAccount::upsert_for_user(&client, viewer.id, "cs-uid", "oddity", "refresh-token")
        .await
        .expect("link cyberspace account");
    CyberspaceAccount::set_circ_rooms(&client, viewer.id, &["circ-lab".to_string()])
        .await
        .expect("pin a chat room");

    let mut app = make_app(test_db.db.clone(), viewer.id, "cs-room-leave-flow-it");
    wait_for_render_contains(&mut app, "circ-lab").await;

    app.chat.select_cyberspace_room(0);
    assert_eq!(
        app.chat.cyberspace.open_circ_slug(),
        Some("circ-lab"),
        "selecting the rail entry should open the room"
    );

    // A digit, Tab, or Ctrl+G switches screens without going through the
    // rail; the room's stream and presence heartbeat must not survive it.
    app.set_screen(Screen::Arcade);
    assert_eq!(
        app.chat.cyberspace.open_circ_slug(),
        None,
        "leaving Home must drop the room session"
    );
    assert_eq!(
        app.chat.cyberspace_room_selected, None,
        "the rail must not keep pointing at a room nobody is in"
    );
    assert!(
        app.chat.cyberspace_selected,
        "coming back to Home should land on the cyberspace pane, same as Esc"
    );
}

#[tokio::test]
async fn entering_a_cyberspace_room_reads_it_before_it_types_in_it() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "cs-room-reader").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer to lounge");
    CyberspaceAccount::upsert_for_user(&client, viewer.id, "cs-uid", "oddity", "refresh-token")
        .await
        .expect("link cyberspace account");
    CyberspaceAccount::set_circ_rooms(&client, viewer.id, &["circ-lab".to_string()])
        .await
        .expect("pin a chat room");

    let mut app = make_app(test_db.db.clone(), viewer.id, "cs-room-read-flow-it");
    wait_for_render_contains(&mut app, "circ-lab").await;

    app.chat.select_cyberspace_room(0);
    assert_eq!(app.chat.cyberspace.open_circ_slug(), Some("circ-lab"));

    // Walking into a room is reading it, like every other room in the rail:
    // `k` scrolls the conversation, it does not start a message.
    app.handle_input(b"k");
    assert_eq!(
        room_draft(&app),
        "",
        "a room must not open with its composer focused"
    );

    // `i` is what focuses it, and from there the same letter is text.
    app.handle_input(b"ik");
    assert_eq!(room_draft(&app), "k");

    // Esc drops the draft and hands the room back to reading; only the next
    // one leaves the room.
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| room_draft(app).is_empty(), "room composer").await;
    app.handle_input(b"k");
    assert_eq!(room_draft(&app), "");
    assert_eq!(
        app.chat.cyberspace.open_circ_slug(),
        Some("circ-lab"),
        "the first Esc leaves the composer, not the room"
    );
}

#[tokio::test]
async fn end_in_a_room_draft_edits_the_line_instead_of_scrolling() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "cs-room-end-key").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer to lounge");
    CyberspaceAccount::upsert_for_user(&client, viewer.id, "cs-uid", "oddity", "refresh-token")
        .await
        .expect("link cyberspace account");
    CyberspaceAccount::set_circ_rooms(&client, viewer.id, &["circ-lab".to_string()])
        .await
        .expect("pin a chat room");

    let mut app = make_app(test_db.db.clone(), viewer.id, "cs-room-end-key-it");
    wait_for_render_contains(&mut app, "circ-lab").await;
    app.chat.select_cyberspace_room(0);

    app.handle_input(b"iab");
    app.handle_input(b"\x1b[H");
    app.handle_input(b"c");
    assert_eq!(room_draft(&app), "cab", "Home moves the cursor to the head");

    // End is Home's mirror while the row holds text: it must return the
    // cursor to the end of the line, not scroll the conversation.
    app.handle_input(b"\x1b[F");
    app.handle_input(b"d");
    assert_eq!(room_draft(&app), "cabd");
}

#[tokio::test]
async fn our_own_command_typed_in_their_room_never_becomes_a_message() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "cs-room-command").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer to lounge");
    CyberspaceAccount::upsert_for_user(&client, viewer.id, "cs-uid", "oddity", "refresh-token")
        .await
        .expect("link cyberspace account");
    CyberspaceAccount::set_circ_rooms(&client, viewer.id, &["circ-lab".to_string()])
        .await
        .expect("pin a chat room");

    let mut app = make_app(test_db.db.clone(), viewer.id, "cs-room-command-flow-it");
    wait_for_render_contains(&mut app, "circ-lab").await;
    app.chat.select_cyberspace_room(0);

    // `/cs chat` is ours, not theirs. It opens the picker over the room the
    // user is standing in, and the text never reaches their API as a message.
    app.handle_input(b"i/cs chat\r");
    assert!(
        app.chat.cyberspace.modal_active(),
        "the room picker should open over the room"
    );
    assert_eq!(
        app.chat.cyberspace.open_circ_slug(),
        Some("circ-lab"),
        "opening a picker must not walk the user out of the room"
    );
    assert_eq!(
        room_draft(&app),
        "",
        "the command must not stay in the draft"
    );
}

/// What is currently typed into the open cyberspace room's composer.
fn room_draft(app: &crate::app::state::App) -> String {
    app.chat
        .cyberspace
        .room_composer()
        .expect("a room is open")
        .lines()
        .join("")
}

#[tokio::test]
async fn client_side_chat_commands_render_without_persisting_messages() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "command-flow-viewer").await;
    let target = create_test_user(&test_db.db, "command-flow-target").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer to lounge");

    let private_room = ChatRoom::create_private_room(&client, "side", viewer.id)
        .await
        .expect("create room");
    ChatRoomMember::join(&client, private_room.id, viewer.id)
        .await
        .expect("join viewer to side");
    ChatRoomMember::join(&client, private_room.id, target.id)
        .await
        .expect("join target to side");
    ChatRoom::set_topic_and_rules(
        &client,
        private_room.id,
        Some("cards and tea"),
        Some("be kind\nno spoilers\ntake the bins out"),
    )
    .await
    .expect("set rules");

    let mut app = make_app(test_db.db.clone(), viewer.id, "client-commands-flow-it");

    wait_for_render_contains(&mut app, "lounge").await;
    wait_for_render_contains(&mut app, "side").await;

    app.handle_input(b"i/binds\r");
    wait_for_render_contains(&mut app, " Guide ").await;
    wait_for_render_contains(&mut app, " Chat ").await;
    wait_for_render_contains(&mut app, "/settings").await;
    app.handle_input(b"?");
    assert!(!app.show_help, "? should close the guide");

    app.handle_input(b"llll");
    app.handle_input(b"i/members\r");
    wait_for_render_contains(&mut app, "#side Members").await;
    wait_for_render_contains(&mut app, "@command-flow-viewer").await;
    wait_for_render_contains(&mut app, "@command-flow-target").await;
    app.handle_input(b"q");
    assert!(
        !app.chat.has_overlay(),
        "q should close the members overlay"
    );

    app.handle_input(b"i/rules\r");
    // Every line survives, which a one-line banner could not do.
    wait_for_render_contains(&mut app, "#side rules").await;
    wait_for_render_contains(&mut app, "be kind").await;
    wait_for_render_contains(&mut app, "no spoilers").await;
    wait_for_render_contains(&mut app, "take the bins out").await;

    let lounge_messages = ChatMessage::list_recent(&client, lounge.id, 20)
        .await
        .expect("list lounge messages");
    let private_messages = ChatMessage::list_recent(&client, private_room.id, 20)
        .await
        .expect("list private room messages");
    assert!(
        lounge_messages.is_empty() && private_messages.is_empty(),
        "expected /binds, /members, and /rules to stay client-side"
    );
}

#[tokio::test]
async fn mod_commands_route_bare_and_prefixed_forms() {
    let (_test_db, mut app) = chat_compose_app("mod-command-open").await;

    app.handle_input(b"/mod\r");

    wait_for_render_contains(&mut app, " Moderation ").await;
    wait_for_render_contains(&mut app, "access denied: moderator or admin only").await;

    app.handle_input(b"\x1b");
    tokio::time::sleep(Duration::from_millis(60)).await;
    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Compose (Enter send").await;

    app.handle_input(b"/mod help\r");

    wait_for_render_contains(
        &mut app,
        "open /mod first; moderation commands only run in the modal",
    )
    .await;
}

#[tokio::test]
async fn ignore_command_hides_messages_and_persists_across_refresh() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "ignore-flow-viewer").await;
    let target = create_test_user(&test_db.db, "ignore-flow-target").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge.id, target.id)
        .await
        .expect("join target");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: target.id,
            body: "message from ignored user".to_string(),
        },
    )
    .await
    .expect("create message");

    let (mut app, chat_service) =
        make_app_with_chat_service(test_db.db.clone(), viewer.id, "ignore-command-flow-it");
    wait_for_render_contains(&mut app, "message from ignored user").await;

    app.handle_input(b"i");
    app.handle_input(b"/ignore ignore-flow-target\r");
    wait_for_render_contains(&mut app, "Ignored @ignore-flow-target").await;

    let ignored = User::ignored_user_ids(&client, viewer.id)
        .await
        .expect("load ignore list");
    assert_eq!(ignored, vec![target.id]);

    let post_ignore_body = "fresh message from ignored user";
    chat_service.send_message_task(
        target.id,
        lounge.id,
        Some("lounge".to_string()),
        post_ignore_body.to_string(),
        Uuid::now_v7(),
        false,
    );
    wait_until(
        || async {
            ChatMessage::list_recent(&client, lounge.id, 20)
                .await
                .expect("list recent messages")
                .iter()
                .any(|message| message.body == post_ignore_body)
        },
        "post-ignore message to persist",
    )
    .await;

    // The viewer's own message is a later event in the same stream: once it
    // renders, the earlier ignored message has been drained and filtered.
    let marker_body = "marker after ignore";
    app.handle_input(b"i");
    app.handle_input(b"marker after ignore\r");
    wait_for_render_contains(&mut app, marker_body).await;
    assert!(
        !render_plain(&mut app).contains(post_ignore_body),
        "ignored user's message must not render"
    );

    let mut refreshed_app = make_app(test_db.db.clone(), viewer.id, "ignore-command-refresh-it");
    wait_for_render_contains(&mut refreshed_app, marker_body).await;
    assert!(
        !render_plain(&mut refreshed_app).contains(post_ignore_body),
        "ignored user's message must not render after reconnect"
    );
}

#[tokio::test]
async fn sheet_command_opens_character_sheet_modal_in_dnd_room() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sheet-modal-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    // Pre-create the #dnd room and join the user before the app starts so the
    // room is in the initial snapshot; this avoids the async race of /public.
    let dnd = ChatRoom::get_or_create_public_room(&client, "dnd")
        .await
        .expect("create dnd room");
    ChatRoomMember::join(&client, dnd.id, user.id)
        .await
        .expect("join dnd room");
    let mut app = make_app(test_db.db.clone(), user.id, "sheet-modal-flow-it");

    wait_for_render_contains(&mut app, "lounge").await;
    // Wait for the dnd room to appear in the sidebar.
    wait_for_render_contains(&mut app, "dnd").await;

    // Navigate to the dnd room. The sidebar order is lounge, mentions, news,
    // "+ browse rooms" (Discover, last in Core), then dnd (channels section).
    // Press l four times to reach dnd from lounge.
    app.handle_input(b"llll");
    wait_for_render_contains(&mut app, "Home · dnd").await;

    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Compose (Enter send").await;

    // /sheet is room-scoped to #dnd. Autocomplete deactivates with the
    // trailing space before \r so the enter submits rather than confirms.
    app.handle_input(b"/sheet \r");
    wait_for_render_contains(&mut app, "character sheet").await;
    wait_for_render_contains(&mut app, "sheet-modal-it").await;
}

#[tokio::test]
async fn backslash_cycles_rails_for_this_device_only_and_auto_follows_width() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "rails-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "rails-flow-it");
    wait_for_render_contains(&mut app, " Home ").await;
    // Both rails up to start with: the room rail's footer hints and the
    // sidebar's music panel are both on screen. (The sidebar's pinned presence
    // row sits under the banner popup, so the panel header is the stable
    // marker once a banner is showing.)
    wait_for_render_contains(&mut app, "sort/fold").await;
    let frame = render_plain(&mut app);
    assert!(frame.contains("music"), "sidebar missing; frame={frame:?}");

    // First press hides the room rail, and says which scope it changed.
    app.handle_input(b"\\");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("room list hidden (this device)"),
        "expected a per-device banner; frame={frame:?}"
    );
    assert!(
        !frame.contains("sort/fold"),
        "expected the room rail to be hidden; frame={frame:?}"
    );

    // Three more presses reach Auto, which at 100 columns keeps both rails.
    app.handle_input(b"\\\\\\");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("auto for this terminal size"),
        "expected the auto step in the cycle; frame={frame:?}"
    );
    assert!(
        frame.contains("sort/fold") && frame.contains("music"),
        "a 100-column terminal should keep both rails on auto; frame={frame:?}"
    );

    // A phone-sized terminal folds both rails away without touching settings,
    // and widening brings them back: auto reads the live width every frame.
    app.resize(50, 32).expect("resize narrow");
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("sort/fold") && !frame.contains("music"),
        "auto should fold both rails on a narrow terminal; frame={frame:?}"
    );
    app.resize(100, 32).expect("resize wide");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("sort/fold"),
        "auto should restore the rails when the terminal grows; frame={frame:?}"
    );

    // The account default is untouched throughout: the layout belongs to the
    // device, so the user's other machine keeps its own rails.
    let profile = app.profile_state.profile();
    assert!(
        profile.show_room_list_sidebar,
        "cycling rails must not rewrite the account default"
    );
    assert_eq!(
        profile.room_list_mode,
        late_core::models::user::RoomListMode::On
    );
}

#[tokio::test]
async fn cycling_rails_persists_only_to_authenticating_key_and_survives_unrelated_settings() {
    // End-to-end for both sides of the device-layout boundary: cycling the rails
    // writes only the authenticating key, and a later account-settings save does
    // not leak that device layout onto the account or its other keys.
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "rails-key-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    // Two devices on one account, as `auth_publickey` would have recorded them.
    UserSshKey::ensure(&client, user.id, "SHA256:phone")
        .await
        .expect("phone key");
    UserSshKey::ensure(&client, user.id, "SHA256:desktop")
        .await
        .expect("desktop key");

    let mut app = with_session_key(
        make_app(test_db.db.clone(), user.id, "rails-key-flow-it"),
        "SHA256:phone",
    );
    wait_for_render_contains(&mut app, "sort/fold").await;

    // Hide the room rail on this device only.
    app.handle_input(b"\\");
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("sort/fold"),
        "expected the rail hidden for this device; frame={frame:?}"
    );

    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move {
                let client = db.get().await.expect("db client");
                stored_layout(&client, user.id, "SHA256:phone")
                    .await
                    .expect("phone layout")
                    == Some(KeyLayout {
                        room_list_mode: RoomListMode::Off,
                        right_sidebar_mode: RightSidebarMode::On,
                    })
            }
        },
        "the cycled layout to reach this device's key",
    )
    .await;
    assert_eq!(
        stored_layout(&client, user.id, "SHA256:desktop")
            .await
            .expect("desktop layout"),
        None,
        "the account's other device must keep following the account default"
    );

    // Now touch something unrelated: Ctrl+O, Tab to the Tweaks tab, and flip
    // the first row (Sync terminal background). The save banner marks the
    // write landing.
    app.handle_input(b"\x0f");
    // Wait for the draft to hydrate from the profile snapshot before moving:
    // that hydration resets the modal to its first tab (`open_from_profile`).
    wait_for_render_contains(&mut app, "rails-key-it").await;
    app.handle_input(b"\t\t\t");
    wait_for_render_contains(&mut app, "Sync terminal background").await;
    // Enter toggles the selected row (Sync terminal background, the first
    // one), which runs the same `save()` every other settings edit runs.
    app.handle_input(b"\r");
    wait_until(
        || {
            let db = db.clone();
            async move {
                let client = db.get().await.expect("db client");
                let stored = User::get(&client, user.id)
                    .await
                    .expect("load user")
                    .expect("user exists");
                !late_core::models::user::extract_enable_background_color(&stored.settings)
            }
        },
        "background color tweak to persist",
    )
    .await;

    // That write landed, so if the device rails were riding along they would be
    // in it. The account default still has both rails on: only this device changed.
    let stored = User::get(&client, user.id)
        .await
        .expect("load user")
        .expect("user exists");
    assert_eq!(
        late_core::models::user::extract_room_list_mode(&stored.settings),
        late_core::models::user::RoomListMode::On,
        "an unrelated tweak must not republish this device's rails"
    );
    assert!(
        late_core::models::user::extract_show_room_list_sidebar(&stored.settings),
        "the legacy mirror must stay in step with the account default"
    );
    assert_eq!(
        stored_layout(&client, user.id, "SHA256:phone")
            .await
            .expect("phone layout after settings save"),
        Some(KeyLayout {
            room_list_mode: RoomListMode::Off,
            right_sidebar_mode: RightSidebarMode::On,
        }),
        "the unrelated account save must preserve this device's layout"
    );
    assert_eq!(
        stored_layout(&client, user.id, "SHA256:desktop")
            .await
            .expect("desktop layout after settings save"),
        None,
        "the unrelated account save must not publish a layout to another device"
    );

    // And the device's own choice survived the settings round trip.
    app.handle_input(b"\x1b");
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("sort/fold"),
        "expected the device rail to stay hidden; frame={frame:?}"
    );
}

#[tokio::test]
async fn the_lounge_renders_its_own_topic_header() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "lounge-topic-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer to lounge");
    ChatRoom::set_topic_and_rules(&client, lounge.id, Some("tonight: the rooms upgrade"), None)
        .await
        .expect("set lounge topic");

    // The Lounge is the one room drawn by `draw_dashboard_chat_card` instead of
    // `draw_chat_center`, so its header needs its own coverage.
    let mut app = make_app(test_db.db.clone(), viewer.id, "lounge-topic-flow-it");
    wait_for_render_contains(&mut app, "tonight: the rooms upgrade").await;
}

#[tokio::test]
async fn default_bottom_bar_shows_every_default_component_even_while_idle() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "bottom-status-default").await;
    let mut app = make_app(test_db.db.clone(), viewer.id, "bottom-status-default-it");

    let enabled = app
        .profile_state
        .profile()
        .statusline_components
        .iter()
        .filter(|setting| setting.enabled)
        .map(|setting| setting.component)
        .collect::<Vec<_>>();
    assert_eq!(
        enabled,
        vec![
            StatusComponent::Shortcuts,
            StatusComponent::Mentions,
            StatusComponent::Voice,
            StatusComponent::Live,
            StatusComponent::Date,
        ]
    );

    // Every default stays on the bar while idle. Wide enough for all of it
    // beside the sponsor line. The account has no timezone, so the date is
    // UTC's.
    app.resize(160, 40).expect("resize test terminal");
    let today = chrono::Utc::now().format("%a %-d %b").to_string();
    let frame = render_plain(&mut app);
    for reading in ["unread 0", "mic -", "live -", today.as_str()] {
        assert!(
            frame.contains(reading),
            "{reading:?} shows by default: {frame:?}"
        );
    }
    assert!(
        frame.contains("Settings ^O")
            && frame.contains("Zen ^F")
            && frame.contains("Shop ^S")
            && frame.contains("Exit qq"),
        "Keyhints should render from the default component: {frame:?}"
    );
}

#[tokio::test]
async fn keyhints_brief_property_toggles_and_persists_in_settings() {
    use crate::app::settings_modal::state::{StatuslinePane, Tab};

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "brief-keyhints-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "brief-keyhints-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, " Home ").await;

    app.handle_input(b"\x0f");
    wait_for_render_contains(&mut app, "brief-keyhints-it").await;
    app.handle_input(b"\t\t\t\t");
    wait_for_render_contains(&mut app, "Brief").await;
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Statusline);
    wait_for_render_contains(
        &mut app,
        "Keyboard shortcuts for navigation and common actions.",
    )
    .await;
    // Both directions leave the list, and returning preserves its selection.
    app.handle_input(b"\t");
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Account);
    app.handle_input(b"\x1b[Z");
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Statusline);
    app.handle_input(b"\x1b[C\x1b[D");
    assert_eq!(
        app.settings_modal_state.statusline_pane(),
        StatuslinePane::List
    );
    app.handle_input(b"\r"); // Open Keyhints' detail pane.
    assert_eq!(
        app.settings_modal_state.statusline_pane(),
        StatuslinePane::Detail
    );
    // Tab must switch tabs even while editing a component's options.
    app.handle_input(b"\t");
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Account);
    app.handle_input(b"\x1b[Z");
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Statusline);
    assert_eq!(
        app.settings_modal_state.statusline_pane(),
        StatuslinePane::Detail
    );

    for brief in [true, false] {
        // Right and Left edit Brief while staying in the properties pane.
        app.handle_input(if brief { b"\x1b[C" } else { b"\x1b[D" });
        assert_eq!(
            app.settings_modal_state.statusline_pane(),
            StatuslinePane::Detail
        );
        // Let each asynchronous save finish before the next edit.
        let db = test_db.db.clone();
        wait_until(
            || {
                let db = db.clone();
                async move {
                    let client = db.get().await.expect("db client");
                    let stored = User::get(&client, user.id)
                        .await
                        .expect("load user")
                        .expect("user exists");
                    stored.settings["statusline_components"]
                        .as_array()
                        .is_some_and(|entries| {
                            let enabled: Vec<_> = entries
                                .iter()
                                .filter(|entry| entry["enabled"] == true)
                                .collect();
                            let keys: Vec<_> =
                                enabled.iter().map(|entry| entry["key"].as_str()).collect();
                            keys == [
                                Some("shortcuts"),
                                Some("mentions"),
                                Some("voice"),
                                Some("live"),
                                Some("date"),
                            ] && enabled[0]["brief"] == brief
                        })
                }
            },
            "Keyhints Brief property saved",
        )
        .await;

        let hint = if brief {
            "⚙ ^o · ⚄ ^g · ◉ ^s"
        } else {
            "Settings ^O"
        };
        wait_for_render_contains(&mut app, hint).await;
        let mut reloaded = make_app(test_db.db.clone(), user.id, "keyhints-reloaded-it");
        wait_for_render_contains(&mut reloaded, hint).await;
        assert_eq!(
            reloaded.profile_state.profile().statusline_components[0].brief,
            brief
        );
    }
    app.handle_input(b"\x1b");
    wait_for_render_contains(&mut app, "Enter options").await;
    assert!(app.show_settings);
    assert_eq!(
        app.settings_modal_state.statusline_pane(),
        StatuslinePane::List
    );
    app.handle_input(b"q");
    assert!(!app.show_settings);
    wait_for_render_contains(&mut app, "Settings ^O").await;
}

#[tokio::test]
async fn runner_stays_off_the_fixed_bar_and_zen_paints_its_own_row() {
    use crate::app::common::primitives::Screen;
    use crate::app::deadchannel::fight::state::Sheet;

    let render_top_row = |app: &mut crate::app::state::App| {
        app.tick();
        app.reset_render();
        let mut terminal = vt100::Parser::new(40, 200, 0);
        terminal.process(&app.render().expect("render"));
        terminal
            .screen()
            .contents()
            .lines()
            .next()
            .expect("top border row")
            .to_string()
    };

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "status-zen-viewer").await;
    let mut app = make_app(test_db.db.clone(), viewer.id, "status-zen-flow-it");
    app.resize(200, 40).expect("resize test terminal");
    app.fight.sheet = Some(Sheet::fresh(viewer.id, chrono::Utc::now().date_naive()));
    let top_row = render_top_row(&mut app);
    assert!(top_row.contains("chips"));
    assert!(!top_row.contains("rations"));
    assert!(!top_row.contains("signal"));
    assert!(!app.last_status_hits.borrow().is_empty());

    // Zen has no frame and so no top bar: the user's bar moves to the page's
    // own bottom row.
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Zen);
    let top_row = render_top_row(&mut app);
    assert!(!top_row.contains("chips"));
    let frame = render_plain(&mut app);
    assert!(frame.contains("Settings ^O"));
    assert!(!app.last_status_hits.borrow().is_empty());

    app.handle_input(b"\x06");
    let top_row = render_top_row(&mut app);
    assert!(top_row.contains("chips"));
    assert!(!top_row.contains("rations"));
    assert!(!top_row.contains("signal"));
    assert!(!app.last_status_hits.borrow().is_empty());
}

/// Under 40x12 Zen draws its too-small notice and nothing else: the status
/// row is not painted, so no click target is left behind on the page.
#[tokio::test]
async fn zen_too_small_to_draw_keeps_no_status_click_targets() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-small-viewer").await;
    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-small-flow-it");
    app.resize(120, 40).expect("resize test terminal");
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Zen);
    wait_for_render_contains(&mut app, "w tend").await;
    assert!(!app.last_status_hits.borrow().is_empty());

    app.resize(39, 40).expect("resize test terminal");
    let frame = render_plain(&mut app);
    assert!(frame.contains("Rice needs at least"), "{frame:?}");
    assert!(!frame.contains("unread"), "{frame:?}");
    assert!(app.last_status_hits.borrow().is_empty());
}

/// `?` on Zen opens the guide on the Zen topic, which lists the layout keys;
/// with every status line component off Zen's bottom row is gone and the
/// tiles run down to the last row.
#[tokio::test]
async fn zen_guide_opens_on_zen_keys_and_the_row_goes_with_every_component_off() {
    use crate::app::common::primitives::Screen;
    use crate::app::help_modal::data::HelpTopic;
    use crate::app::profile::state::profile_params_from_profile;
    use late_core::models::profile::Profile;

    const COLS: u16 = 120;
    const ROWS: u16 = 40;
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-row-viewer").await;
    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-row-flow-it");
    app.resize(COLS, ROWS).expect("resize test terminal");
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Zen);
    wait_for_render_contains(&mut app, "w tend").await;

    app.handle_input(b"?");
    assert!(app.show_help);
    assert_eq!(app.help_modal_state.selected_topic(), HelpTopic::Zen);

    let client = test_db.db.get().await.expect("db client");
    let profile = Profile::load(&client, viewer.id)
        .await
        .expect("load profile");
    let mut params = profile_params_from_profile(&profile);
    for setting in &mut params.statusline_components {
        setting.enabled = false;
    }
    Profile::update(&client, viewer.id, params)
        .await
        .expect("save the status line");
    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-row-off-flow-it");
    app.resize(COLS, ROWS).expect("resize test terminal");
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Zen);
    // The profile lands after the first frames, which paint the defaults.
    wait_for_render_not_contains(&mut app, "Settings ^O").await;
    assert!(app.last_status_hits.borrow().is_empty());
    let mut terminal = vt100::Parser::new(ROWS, COLS, 0);
    app.reset_render();
    terminal.process(&app.render().expect("render"));
    let screen = terminal.screen().contents();
    let last_row = screen.lines().last().expect("last row");
    assert!(
        last_row.starts_with('╰'),
        "a tile's bottom border is the page's last row: {last_row:?}"
    );
}

/// Each status bar segment routes to its own destination, on whichever border
/// it sits: chips on the fixed top bar, the unread counter on the bottom one.
/// The rects come from measured span widths, which is what this exercises end
/// to end.
#[tokio::test]
async fn clicking_a_status_bar_segment_opens_its_own_destination() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "hud-mention-viewer").await;
    let author = create_test_user(&test_db.db, "hud-mention-author").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge.id, author.id)
        .await
        .expect("join author");

    let (mut app, chat_service) =
        make_app_with_chat_service(test_db.db.clone(), viewer.id, "hud-mention-flow-it");
    chat_service.send_message_task(
        author.id,
        lounge.id,
        Some("lounge".to_string()),
        "@hud-mention-viewer got a minute?".to_string(),
        Uuid::now_v7(),
        false,
    );
    // Chips sit on the top border row and the unread counter on the bottom
    // one. Wide enough that the counter fits beside the sponsor's link; a
    // segment with no room is dropped whole.
    const COLS: u16 = 160;
    const ROWS: u16 = 40;
    app.resize(COLS, ROWS).expect("resize test terminal");
    wait_for_render_contains(&mut app, "unread 1").await;
    app.tick();
    app.reset_render();
    let mut terminal = vt100::Parser::new(ROWS, COLS, 0);
    terminal.process(&app.render().expect("render"));
    let screen = terminal.screen().contents();
    let top_row = screen.lines().next().expect("top border row");
    let bottom_row = screen.lines().last().expect("bottom border row");
    // Display columns, not chars: a wide glyph ahead of a segment takes two
    // cells.
    let display_col = |row: &str, needle: &str| {
        let byte = row.find(needle).expect("needle on the border row");
        unicode_width::UnicodeWidthStr::width(&row[..byte])
    };
    let chips_col = display_col(top_row, "chips");
    let mentions_col = display_col(bottom_row, "unread");
    assert!(
        !top_row.contains("unread"),
        "the unread counter lives on the bottom bar only: {top_row:?}"
    );

    // Chips go to the Shop, so a click there must not reach Mentions. SGR
    // mouse coords are 1-indexed.
    app.handle_input(format!("\x1b[<0;{};1M", chips_col + 1).as_bytes());
    wait_for_render_contains(&mut app, "-- Shop --").await;
    assert_render_not_contains_for(&mut app, "mentioned you in", Duration::from_millis(120)).await;
    // Close the Shop that click opened before aiming at the next segment.
    app.handle_input(b"q");
    assert_render_not_contains_for(&mut app, "-- Shop --", Duration::from_millis(120)).await;

    // Clicking inside the mentions text opens the Mentions view, and a
    // composer that was open closes: Mentions has nothing to type into.
    app.handle_input(b"i");
    assert!(app.chat.composing, "i opens the lounge composer");
    app.handle_input(format!("\x1b[<0;{};{ROWS}M", mentions_col + 1).as_bytes());
    assert!(
        !app.chat.composing,
        "the jump to Mentions closes the composer"
    );
    wait_for_render_contains(&mut app, "mentioned you in").await;
}

#[tokio::test]
async fn forced_tour_walks_the_house_on_enter_with_one_shot_of_pool() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "tour-gate-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "tour-gate-flow-it");
    // Room for the practice table under its header.
    app.resize(160, 50).unwrap();

    // Arm the tour the way a first-ever session does: land in the tavern
    // with the walkthrough pending.
    app.set_screen(Screen::Clubhouse);
    app.clubhouse.tutorial = Tutorial::Pending;
    app.clubhouse
        .enter_screen(crate::app::presence::svc::now_ms());
    assert_eq!(app.clubhouse.tutorial, Tutorial::Welcome);

    // The gate swallows everything but Enter: no page hopping, no Tab, no
    // help modal, no reserved chords (Zen's included), no composer.
    for bytes in [&b"2"[..], b"\t", b"?", b"\x0f", b"\x07", b"\x06", b"i"] {
        app.handle_input(bytes);
    }
    assert_eq!(app.screen, Screen::Clubhouse);
    assert!(!app.show_help);
    assert_eq!(app.clubhouse.tutorial, Tutorial::Welcome);

    // Enter walks to Home, then the music stop holds the real Stations
    // modal open; its own keys and a lone Esc do nothing to it.
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Dashboard);
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitMusic);
    assert!(app.stations_modal_state.is_open());
    app.handle_input(b"\x1b");
    app.pending_escape_started_at = Some(std::time::Instant::now() - Duration::from_secs(1));
    crate::app::input::flush_pending_escape(&mut app);
    assert!(app.stations_modal_state.is_open());

    // On to the arcade, where the lobby stop holds the real Lobby modal.
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Arcade);
    assert!(!app.stations_modal_state.is_open());
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitLobby);
    assert!(app.show_lobby_modal);

    // Enter leads to the practice table, where the break has to be played:
    // Enter strikes it rather than skipping past.
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::DailyMatch);
    assert!(!app.show_lobby_modal);
    wait_for_esc_effect(
        &mut app,
        |app| {
            app.daily
                .board
                .as_ref()
                .is_some_and(|board| board.detail.is_some())
        },
        "practice table racked",
    )
    .await;
    assert!(!app.daily.practice_played());
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::DailyMatch);
    wait_for_esc_effect(&mut app, |app| app.daily.practice_played(), "break struck").await;
    // One shot only: Space does not strike the rack twice.
    app.handle_input(b" ");
    app.tick();
    let shots = |app: &crate::app::state::App| {
        let board = app.daily.board.as_ref().expect("the table is open");
        let pool = board.detail.as_ref().and_then(|detail| detail.pool());
        pool.expect("a pool table").state.move_count()
    };
    assert_eq!(shots(&app), 1);

    // Enter leaves the table behind for the games page.
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Games);
    assert!(app.daily.board.is_none());

    // The dungeon stop stays on that page until the fight is won: every
    // Enter or Space is a blow, and the page only turns after the last one.
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitDungeon);
    while !app.clubhouse.tour_fight.won() {
        app.handle_input(b" ");
        assert_eq!(app.screen, Screen::Games);
        assert_eq!(app.clubhouse.tutorial, Tutorial::VisitDungeon);
    }

    // The rest of the route is Enter alone.
    for screen in [
        Screen::Artboard,
        Screen::Profiles,
        Screen::Leaderboard,
        Screen::Zen,
        Screen::Clubhouse,
    ] {
        app.handle_input(b"\r");
        assert_eq!(app.screen, screen);
    }
    assert_eq!(app.clubhouse.tutorial, Tutorial::Homecoming);

    // Enter settles in, and input is free again.
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::Done);
    app.handle_input(b"2");
    assert_eq!(app.screen, Screen::Arcade);
}

/// The practice table needs more room than a default terminal has. There
/// the stop says so and Enter walks on, with no break struck blind.
#[tokio::test]
async fn forced_tour_skips_the_practice_table_on_a_small_terminal() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "tour-small-table-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "tour-small-table-flow-it");
    app.resize(80, 24).unwrap();

    app.set_screen(Screen::Clubhouse);
    app.clubhouse.tutorial = Tutorial::Pending;
    app.clubhouse
        .enter_screen(crate::app::presence::svc::now_ms());
    for _ in 0..5 {
        app.handle_input(b"\r");
    }
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitTable);
    assert_eq!(app.screen, Screen::DailyMatch);
    wait_for_esc_effect(
        &mut app,
        |app| {
            app.daily
                .board
                .as_ref()
                .is_some_and(|board| board.detail.is_some())
        },
        "practice table racked",
    )
    .await;

    let frame = render_plain(&mut app);
    assert!(
        frame.contains("this table needs a bigger window"),
        "frame={frame:?}"
    );
    assert!(frame.contains("[Enter] next: the games"), "frame={frame:?}");

    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Games);
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitGames);
    assert!(app.daily.board.is_none());
}

/// `q` at the music stop asks before quitting, and the held Stations modal
/// stays out of the prompt's way until Esc brings the tour back.
#[tokio::test]
async fn forced_tour_quit_confirm_shows_over_the_held_stations_modal() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "tour-quit-music-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "tour-quit-music-flow-it");
    app.resize(80, 24).unwrap();

    app.set_screen(Screen::Clubhouse);
    app.clubhouse.tutorial = Tutorial::Pending;
    app.clubhouse
        .enter_screen(crate::app::presence::svc::now_ms());
    app.handle_input(b"\r");
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitMusic);

    app.handle_input(b"q");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Clicked by mistake, right?"),
        "frame={frame:?}"
    );
    assert!(!frame.contains("the tour · the radio"), "frame={frame:?}");

    app.handle_input(b"\x1b");
    app.pending_escape_started_at = Some(std::time::Instant::now() - Duration::from_secs(1));
    crate::app::input::flush_pending_escape(&mut app);
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Clicked by mistake, right?"),
        "frame={frame:?}"
    );
    assert!(frame.contains("the tour · the radio"), "frame={frame:?}");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitMusic);
}

/// `/onboard` from Home puts anyone back at the tavern door with the tour
/// running from the top, as forced as a first visit.
#[tokio::test]
async fn onboard_command_starts_the_tour_again() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let (_test_db, mut app) = chat_compose_app("onboard-command").await;
    app.clubhouse.tutorial = Tutorial::Done;
    for _ in 0..3 {
        app.clubhouse.tour_fight.strike();
    }
    assert!(app.clubhouse.tour_fight.won());

    app.handle_input(b"/onboard");
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Clubhouse);
    assert_eq!(app.clubhouse.tutorial, Tutorial::Welcome);
    assert!(!app.clubhouse.tour_fight.won());

    app.handle_input(b"3");
    assert_eq!(app.screen, Screen::Clubhouse);
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Dashboard);
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitChat);
}

/// The Lounge composer is plain speech: a `/` draft is refused with a
/// banner and kept for editing, never run and never posted.
#[tokio::test]
async fn clubhouse_composer_refuses_commands() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let (_test_db, mut app) = chat_compose_app("clubhouse-no-commands").await;
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.chat.composing, "composer closed").await;
    app.set_screen(Screen::Clubhouse);
    app.clubhouse.tutorial = Tutorial::Done;
    app.clubhouse
        .enter_screen(crate::app::presence::svc::now_ms());

    app.handle_input(b"i");
    app.handle_input(b"/active");
    app.handle_input(b"\r");
    assert!(
        !app.chat.has_overlay(),
        "/active does not run in the Lounge"
    );
    assert_eq!(
        app.banner.as_ref().map(|banner| banner.message.as_str()),
        Some("Commands are off in the Lounge, use them from Home")
    );
    assert_eq!(app.chat.composer().lines().join("\n"), "/active");
    assert_eq!(app.screen, Screen::Clubhouse);
}

/// A chat overlay owns input in the Lounge, and one can still arrive there
/// without a command (a `/summary` or reaction list requested on Home that
/// lands after the walk over), so the tavern draws it rather than trap keys.
#[tokio::test]
async fn clubhouse_draws_a_chat_overlay_that_lands_there() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let (_test_db, mut app) = chat_compose_app("clubhouse-overlay").await;
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.chat.composing, "composer closed").await;
    app.set_screen(Screen::Clubhouse);
    app.clubhouse.tutorial = Tutorial::Done;
    app.clubhouse
        .enter_screen(crate::app::presence::svc::now_ms());

    app.chat.open_active_users_overlay();
    wait_for_render_contains(&mut app, "Active Users").await;

    app.handle_input(b"q");
    assert!(!app.chat.has_overlay(), "q closes it");
    assert_eq!(app.screen, Screen::Clubhouse);
}

/// Zen chat tiles draw real rooms only, so the picker there offers nothing
/// else: a pick of Mentions or News would move Home's selection and leave
/// the page looking untouched.
#[tokio::test]
async fn zen_room_picker_lists_real_rooms_only() {
    use crate::app::common::primitives::Screen;

    let (_test_db, mut app) = chat_compose_app("zen-picker-rooms").await;
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.chat.composing, "composer closed").await;

    // Home lists the synthetic entry, so the query itself is a real match.
    app.handle_input(b"\x1f");
    assert!(app.room_search_modal_state.is_open());
    app.handle_input(b"mentions");
    assert_render_not_contains_for(&mut app, "No matching rooms", Duration::from_millis(60)).await;
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| !app.room_search_modal_state.is_open(),
        "picker closed",
    )
    .await;

    app.set_screen(Screen::Zen);
    app.handle_input(b"\x1f");
    assert!(app.room_search_modal_state.is_open());
    app.handle_input(b"mentions");
    wait_for_render_contains(&mut app, "No matching rooms").await;
    app.handle_input(b"\r");
    assert!(
        !app.chat.synthetic_entry_selected(),
        "a Zen pick never lands on a synthetic entry"
    );
    assert_eq!(app.screen, Screen::Zen);
}

#[tokio::test]
async fn only_esc_closes_the_stream_modal() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "stream-qr-esc").await;
    let mut app = make_app(test_db.db.clone(), user.id, "stream-qr-esc-it");
    wait_for_render_contains(&mut app, "Home").await;

    app.stream_modal = Some(crate::app::state::StreamModal::Qr(
        crate::app::state::StreamQrModal {
            url: "https://late.sh/golive/abc".to_string(),
            title: "Go Live".to_string(),
            subtitle: "scan to broadcast".to_string(),
        },
    ));

    // The modal holds a hand-copied capability URL: ordinary keys, Enter, and
    // a left click all leave it up rather than taking the URL off the screen.
    app.handle_input(b"x");
    app.handle_input(b"\r");
    app.handle_input(b" ");
    app.handle_input(b"\x1b[<0;10;10M");
    assert!(
        app.stream_modal.is_some(),
        "only esc should close the stream qr modal"
    );

    // A lone Esc dispatches via the pending-escape flush on a later tick,
    // not through the swallow-everything gate the other keys hit.
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| app.stream_modal.is_none(),
        "esc closes the stream qr modal",
    )
    .await;
}

/// A lone Esc dispatches through `dispatch_escape`, never through the history
/// modal's own input handler, so the modal needs its arm there: without it
/// Esc leaves the modal stuck open over the room.
#[tokio::test]
async fn history_modal_opens_from_command_and_closes_on_esc() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "history-esc-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: viewer.id,
            body: "hello from the archive".to_string(),
        },
    )
    .await
    .expect("create message");

    let mut app = make_app(test_db.db.clone(), viewer.id, "history-esc-flow-it");
    wait_for_render_contains(&mut app, "lounge").await;

    app.handle_input(b"i/history\r");
    wait_for_render_contains(&mut app, "History ·").await;
    wait_for_render_contains(&mut app, "hello from the archive").await;

    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| !app.chat.history_modal.is_open(),
        "esc closes the history modal",
    )
    .await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("History ·"),
        "expected the history modal gone after Esc; frame={frame:?}"
    );
}

/// Ctrl+R is the escape hatch for a terminal left damaged by something outside
/// late.sh. It has to re-emit every cell: the failure mode worth pinning is a
/// repaint that clears the screen and then sends an empty diff, leaving the
/// user staring at a blank terminal that is worse than the damage.
#[tokio::test]
async fn ctrl_r_repaints_the_whole_screen_rather_than_blanking_it() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-r-repaint").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "ctrl-r-repaint-flow-it");

    wait_for_render_contains(&mut app, "lounge").await;

    // Let the screen settle: with nothing changed, a frame is only a small diff.
    let _ = app.render().expect("render");
    let settled = strip_ansi(&String::from_utf8_lossy(&app.render().expect("render")));

    app.handle_input(b"\x12");
    let repainted = strip_ansi(&String::from_utf8_lossy(&app.render().expect("render")));

    assert!(
        repainted.contains("lounge"),
        "expected Ctrl+R to re-emit the whole screen; repainted={repainted:?}"
    );
    assert!(
        repainted.len() > settled.len(),
        "expected the Ctrl+R frame to carry more than the settled diff; \
         settled={} bytes, repainted={} bytes",
        settled.len(),
        repainted.len()
    );
}

/// An open chat composer keeps Ctrl+R for itself: it reaches the textarea's
/// keymap as redo instead of repainting the screen, and the composer stays
/// open. `/redraw` is the way to repaint from inside one.
#[tokio::test]
async fn ctrl_r_in_an_open_composer_redoes_instead_of_repainting() {
    let (_test_db, mut app) = chat_compose_app("ctrl-r-composer").await;

    app.handle_input(b"abc");
    assert_eq!(app.chat.composer().lines(), ["abc"]);
    app.chat.composer_undo();
    assert_ne!(
        app.chat.composer().lines(),
        ["abc"],
        "undo took something back"
    );

    app.handle_input(b"\x12");
    assert!(app.chat.composing, "the composer stays open");
    assert_eq!(
        app.chat.composer().lines(),
        ["abc"],
        "Ctrl+R redid the undone edit instead of repainting"
    );
}

/// Uploading an image while replying used to come back as a plain message:
/// both the `/paste-image` submit and reopening the composer with the finished
/// URL run through paths that clear the reply target.
#[tokio::test]
async fn image_upload_keeps_the_reply_it_was_composed_against() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-upload-viewer").await;
    let author = create_test_user(&test_db.db, "f-upload-author").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge.id, author.id)
        .await
        .expect("join author");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "upload target".to_string(),
        },
    )
    .await
    .expect("create message");

    let mut app = make_app(test_db.db.clone(), viewer.id, "f-upload-flow-it");
    app.resize(160, 32).expect("resize test terminal");
    wait_for_render_contains(&mut app, "upload target").await;

    app.handle_input(b"j");
    app.handle_input(b"r");
    assert!(
        app.chat.reply_target().is_some(),
        "r should open a reply composer"
    );

    // Stand in for the upload itself: the reply target travels with the
    // request from here, and the composer is reopened when the URL lands.
    let (tx, rx) = tokio::sync::oneshot::channel();
    let reply_target = app.chat.reply_target().cloned();
    assert!(
        app.chat
            .begin_image_upload(Some(lounge.id), reply_target, rx)
            .is_none(),
        "the upload should start"
    );
    tx.send(Ok("https://files.late.sh/chat/x.png".to_string()))
        .expect("deliver the uploaded url");

    wait_for_render_contains(&mut app, "files.late.sh/chat/x.png").await;
    assert!(
        app.chat.reply_target().is_some(),
        "the upload dropped the reply it was composed against"
    );
}

/// A mention read in its own room used to sit on the rail badge for the rest
/// of the session: the DB count moved but nothing republished it. Rendering
/// the mention's message now stamps `notifications.read_at` and the service
/// republishes the count in the same task, so the badge clears live.
#[tokio::test]
async fn mention_rendered_in_its_room_clears_the_rail_badge() {
    use late_core::models::notification::Notification;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-badge-viewer").await;
    let actor = create_test_user(&test_db.db, "f-badge-actor").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge.id, actor.id)
        .await
        .expect("join actor");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: actor.id,
            body: "@f-badge-viewer over here".to_string(),
        },
    )
    .await
    .expect("create mention message");
    Notification::create_mentions_batch(&client, &[viewer.id], actor.id, message.id, lounge.id)
        .await
        .expect("create mention notification");

    // The session must know its own username for the rendered-mention match.
    let mut app = make_app_in_world(
        test_db.db.clone(),
        viewer.id,
        "f-badge-flow-it",
        crate::test_helpers::SessionWorld {
            username: Some("f-badge-viewer".to_string()),
            ..Default::default()
        },
    );
    app.resize(160, 32).expect("resize test terminal");

    // The mention's message lands on screen in its own room.
    wait_for_render_contains(&mut app, "over here").await;

    // That must stamp the mention read without the Mentions entry ever being
    // opened. The stamp rides the app tick's read-cursor flush, so keep
    // ticking while polling for it; asserting a lit badge first would race
    // the very fix under test (the stamp can beat the initial count render).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        app.tick();
        app.reset_render();
        app.render().expect("render");
        let unread = Notification::unread_count(&client, viewer.id)
            .await
            .expect("unread count");
        if unread == 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the rendered mention was never stamped read"
        );
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }

    // The count republished after the stamp committed reaches the rail: the
    // badge is dark for good, with the mention read where it was said.
    wait_for_render_not_contains(&mut app, "mentions (").await;
}

/// The stored rail layout, read the way bootstrap reads it.
async fn stored_layout(
    client: &tokio_postgres::Client,
    user_id: Uuid,
    fingerprint: &str,
) -> anyhow::Result<Option<KeyLayout>> {
    let key = UserSshKey::find_by_fingerprint(client, user_id, fingerprint).await?;
    Ok(key.and_then(|key| extract_key_layout(&key.settings)))
}

#[tokio::test]
async fn whisper_holds_the_splash_door_then_releases_and_marks_delivery() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "whisper-door-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "whisper-door-it");

    // `make_app` skips the splash; the replay hook re-raises it armed, the
    // same way `/haunt replay` does.
    crate::app::deadchannel::haunt::svc::replay_whisper(&mut app);
    assert!(app.show_splash);

    // Esc does nothing: the door is held and the splash does not skip.
    app.tick();
    app.handle_input(b"\x1b");
    assert!(app.show_splash, "expected the whisper to hold the splash");

    // The machine releases on its own clock, hard-capped well under the
    // ticks driven here, and the splash comes down with it.
    for _ in 0..200 {
        app.tick();
        if !app.show_splash {
            break;
        }
    }
    assert!(!app.show_splash, "expected the whisper to release the door");

    // Delivery spends the once-ever mark (fire-and-forget write).
    wait_until(
        || async {
            let client = test_db.db.get().await.expect("db client");
            let user = User::find_by_username(&client, &user.username)
                .await
                .expect("find user")
                .expect("user exists");
            late_core::models::user::extract_first_contact_whisper_at(&user.settings).is_some()
        },
        "first contact whisper stamp persisted",
    )
    .await;
}

/// Arm stage 4 the way `/haunt invite` leaves it: the next send this
/// session makes asks for the invitation claim, due or not.
fn arm_forced_breakthrough(app: &mut crate::app::state::App) {
    let mut breakthrough =
        crate::app::deadchannel::haunt::state::Breakthrough::for_user(app.user_id);
    breakthrough.force_next();
    app.haunt.breakthrough = Some(breakthrough);
}

#[tokio::test]
async fn breakthrough_swallows_every_key_even_inside_a_running_door_game() {
    use crate::app::common::primitives::Screen;

    let (_test_db, mut app) = chat_compose_app("breakthrough-swallow").await;
    app.resize(160, 40).expect("resize test terminal");
    arm_forced_breakthrough(&mut app);

    // The send wins the invitation claim and the screen tears.
    app.handle_input(b"anyone out there\r");
    wait_for_render_contains(&mut app, "we finally reached you").await;

    // A key that opens the quit confirm does nothing while it plays.
    app.handle_input(b"q");

    // Inside a running roguelike too: backtick, which detaches, never
    // reaches the door routing. No awaits until the fabricated game is gone
    // again, so its proxy stays Connecting (see the backtick detach test).
    app.set_screen(Screen::Games);
    app.enter_nethack();
    app.nethack_state
        .as_mut()
        .expect("nethack state")
        .force_running_for_test();
    app.set_screen(Screen::Nethack);
    app.handle_input(b"`");
    assert_eq!(
        app.screen,
        Screen::Nethack,
        "expected the breakthrough to swallow the door's detach key"
    );
    app.nethack_state = None;
    app.set_screen(Screen::Dashboard);

    // The screen heals, the swallowed `q` left nothing behind, and keys work
    // again.
    wait_for_render_not_contains(&mut app, "we finally reached you").await;
    assert_render_not_contains_for(&mut app, " Quit? ", Duration::from_millis(100)).await;
    app.handle_input(b"q");
    wait_for_render_contains(&mut app, " Quit? ").await;
}

#[tokio::test]
async fn breakthrough_waits_for_a_send_from_its_own_session() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "breakthrough-own-send-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");

    // The same person on two devices of one replica: one chat service, so
    // each session hears the other's sends, as in production. Only the
    // phone has the breakthrough armed.
    let world = crate::test_helpers::SessionWorld {
        chat_service: Some(crate::app::chat::svc::ChatService::new(
            test_db.db.clone(),
            crate::app::chat::notifications::svc::NotificationService::new(test_db.db.clone()),
        )),
        ..Default::default()
    };
    let mut phone = make_app_in_world(
        test_db.db.clone(),
        user.id,
        "breakthrough-phone-it",
        world.clone(),
    );
    let mut laptop =
        make_app_in_world(test_db.db.clone(), user.id, "breakthrough-laptop-it", world);
    phone.resize(160, 40).expect("resize phone terminal");
    for app in [&mut phone, &mut laptop] {
        wait_for_render_contains(app, "lounge").await;
        app.handle_input(b"i");
        wait_for_render_contains(app, "Compose (Enter send").await;
    }
    arm_forced_breakthrough(&mut phone);

    // The laptop's send lands on the phone too, and asks for nothing there.
    laptop.handle_input(b"typed on the laptop\r");
    wait_for_render_contains(&mut phone, "typed on the laptop").await;
    assert_render_not_contains_for(
        &mut phone,
        "we finally reached you",
        Duration::from_millis(500),
    )
    .await;
    let stored = User::find_by_username(&client, &user.username)
        .await
        .expect("find user")
        .expect("user exists");
    assert_eq!(
        late_core::models::user::extract_first_contact_invited_at(&stored.settings),
        None,
        "expected another session's send to leave the invitation unclaimed"
    );

    // The phone's own send is the one that breaks through.
    phone.handle_input(b"typed on the phone\r");
    wait_for_render_contains(&mut phone, "we finally reached you").await;
}

/// The gallery end to end: paint a block, frame it from the rail, name it,
/// and find it under Mine. The rail is the only way in, so this is also the
/// rail's keyboard contract.
#[tokio::test]
async fn artboard_archives_time_travel_from_the_rail() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-archive-it").await;
    let mut archived = dartboard_core::Canvas::with_size(
        crate::dartboard::CANVAS_WIDTH,
        crate::dartboard::CANVAS_HEIGHT,
    );
    for x in 0..8 {
        archived.set(dartboard_core::Pos { x, y: 0 }, 'A');
    }
    let client = test_db.db.get().await.expect("db client");
    late_core::models::artboard::Snapshot::upsert(
        &client,
        "daily:2026-04-23",
        serde_json::to_value(&archived).expect("canvas json"),
        serde_json::json!({ "cells": [] }),
    )
    .await
    .expect("insert daily snapshot");
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-archive-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "ARCHIVES").await;
    wait_for_render_contains(&mut app, "Daily").await;

    // Down the rail past Board, four gallery rows, and Hang a piece.
    for _ in 0..6 {
        app.handle_input(b"j");
    }
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "DAILY").await;
    wait_for_render_contains(&mut app, "2026-04-23").await;
    // The key under the cursor lands on the board by itself.
    wait_for_render_contains(&mut app, "Mode       snapshot").await;
    wait_for_render_contains(&mut app, "AAAAAAAA").await;

    // Tab back to the rail, up to Board, Enter: live again.
    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, "archive").await;
    for _ in 0..6 {
        app.handle_input(b"k");
    }
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Mode       view").await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("AAAAAAAA"),
        "the live board should be back; frame={frame:?}"
    );
}

#[tokio::test]
async fn artboard_gallery_hangs_a_framed_piece_from_the_rail() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-gallery-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-gallery-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Mode       view").await;
    wait_for_render_contains(&mut app, "GALLERY").await;
    wait_for_render_contains(&mut app, "Hang a piece").await;

    // Paint a 10x4 block: forty glyphs, the floor for a piece.
    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Mode       active").await;
    app.handle_input(b"\x1b[200~##########\n##########\n##########\n##########\x1b[201~");
    app.handle_input(b"\x1b");
    wait_for_render_contains(&mut app, "Mode       view").await;

    // Esc on the board unfolds the rail; down it to Hang a piece: Board,
    // four gallery rows, then it.
    app.handle_input(b"\x1b");
    wait_for_render_contains(&mut app, "rail j/k").await;
    for _ in 0..5 {
        app.handle_input(b"j");
    }
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Frame your work").await;

    // A frame with nothing in it is refused on the bar, not hung.
    app.handle_input(b"\r");
    // The framing bar is one line; wait on the head of the notice.
    wait_for_render_contains(&mut app, "Select a frame").await;

    // Drag the frame over the block: board (0,0) to (9,3). Framing hands
    // the keys to the board, so the rail is folded and the board starts at
    // screen column 2.
    app.handle_input(b"\x1b[<0;2;2M");
    app.handle_input(b"\x1b[<32;11;5M");
    app.handle_input(b"\x1b[<0;11;5m");
    wait_for_render_contains(&mut app, "frame 10x4").await;
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Hang it in the").await;
    wait_for_render_contains(&mut app, "100% yours").await;

    // No title, no hang. The title takes every printable key: `m`, `v`,
    // `w`, `+`, `-`, and the digits are global hotkeys everywhere else and
    // must not be read as one while a title is being typed.
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Give it a title").await;
    app.handle_input(b"warm view +1 -2");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("warm view +1 -2"),
        "every key typed belongs to the title; frame={frame:?}"
    );
    assert!(
        !app.music_prefix_armed,
        "`v` in a title must not arm the music prefix"
    );
    assert!(
        !app.show_bonsai_modal,
        "`w` in a title must not open the bonsai modal"
    );
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "now hangs").await;
    wait_for_render_contains(&mut app, "warm view +1 -2").await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Mine"),
        "the hung piece should open under Mine; frame={frame:?}"
    );

    // `v` on a listing is the gallery's applause key, not the music prefix.
    // The preview pane only draws once the listing holds the piece.
    wait_for_render_contains(&mut app, "by @artboard-gallery-it").await;
    app.handle_input(b"v");
    wait_for_render_contains(&mut app, "cannot applaud your own piece").await;
    assert!(
        !app.music_prefix_armed,
        "`v` on the Artboard belongs to the gallery"
    );

    // `x` asks first; any other key withdraws the question; `x` twice
    // takes the hanger's own piece down and Mine empties.
    app.handle_input(b"x");
    wait_for_render_contains(&mut app, "x again to confirm").await;
    app.handle_input(b"j");
    wait_for_render_not_contains(&mut app, "x again to confirm").await;
    app.handle_input(b"x");
    wait_for_render_contains(&mut app, "x again to confirm").await;
    app.handle_input(b"x");
    wait_for_render_contains(&mut app, "Taken down").await;
    // The pane is narrow in the test terminal; the head of the empty
    // listing's line is enough.
    wait_for_render_contains(&mut app, "you have not hung a piece").await;

    // The letter hotkeys are off the whole page, typing or not.
    app.banner = None;
    app.handle_input(b"m");
    assert!(
        app.banner.is_none(),
        "`m` must not reach the paired-client mute from the Artboard"
    );
    app.handle_input(b"w");
    assert!(
        !app.show_bonsai_modal,
        "`w` must not open Bonsai Care from the Artboard"
    );

    // Back out: list to rail. Esc on the rail is not the page's, so the
    // digit keys still switch pages from there.
    app.handle_input(b"\x1b");
    wait_for_render_contains(&mut app, "rail j/k").await;
    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Home ").await;
}

#[tokio::test]
async fn zen_tab_cycles_tile_focus_instead_of_switching_pages() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "zen-tab-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "zen-tab-flow-it");
    wait_for_render_contains(&mut app, " Home ").await;
    app.handle_input(b"\x06");
    wait_for_render_contains(&mut app, "w tend").await;
    let tiles = app.zen.leaf_count();
    let start = app.zen.focus;

    // Tab walks the tiles in layout order and stays on the page: Zen is
    // not in the page cycle, so the global Tab would drop back to Home.
    app.handle_input(b"\t");
    assert_eq!(app.screen, Screen::Zen, "Tab on Zen stays on Zen");
    assert_eq!(
        app.zen.focus,
        (start + 1) % tiles,
        "Tab focuses the next tile"
    );

    // Shift+Tab walks back.
    app.handle_input(b"\x1b[Z");
    assert_eq!(app.screen, Screen::Zen, "Shift+Tab on Zen stays on Zen");
    assert_eq!(app.zen.focus, start, "Shift+Tab focuses the previous tile");

    // Tab wraps past the last tile to the first.
    for _ in 0..tiles {
        app.handle_input(b"\t");
    }
    assert_eq!(app.zen.focus, start, "Tab wraps around the tiles");
    assert_eq!(app.screen, Screen::Zen);
}

#[tokio::test]
async fn landing_page_tweak_picks_the_first_screen_except_for_new_users() {
    use crate::app::common::primitives::Screen;
    use crate::test_helpers::SessionWorld;
    use late_core::models::user::LandingPage;

    let test_db = new_test_db().await;
    for (idx, (page, is_new_user, expected)) in [
        (LandingPage::Clubhouse, false, Screen::Clubhouse),
        (LandingPage::Home, false, Screen::Dashboard),
        (LandingPage::Zen, false, Screen::Zen),
        // A first session always starts in the tavern, where the tour runs.
        (LandingPage::Zen, true, Screen::Clubhouse),
        (LandingPage::Home, true, Screen::Clubhouse),
    ]
    .into_iter()
    .enumerate()
    {
        let user = create_test_user(&test_db.db, &format!("landing-it-{idx}")).await;
        let app = make_app_in_world(
            test_db.db.clone(),
            user.id,
            &format!("landing-flow-it-{idx}"),
            SessionWorld {
                landing_page: Some(page),
                is_new_user,
                ..SessionWorld::default()
            },
        );
        assert_eq!(
            app.screen, expected,
            "landing {page:?}, new user {is_new_user}"
        );
    }
}

#[tokio::test]
async fn zen_is_left_only_by_ctrl_f_which_returns_where_it_was_opened() {
    use crate::app::common::primitives::Screen;
    use crate::test_helpers::SessionWorld;
    use late_core::models::user::LandingPage;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "zen-leave-it").await;
    let mut app = make_app_in_world(
        test_db.db.clone(),
        user.id,
        "zen-leave-flow-it",
        SessionWorld {
            landing_page: Some(LandingPage::Zen),
            ..SessionWorld::default()
        },
    );
    assert_eq!(app.screen, Screen::Zen);

    // Landed on Zen: there is no page to hand back, so the chord walks
    // into the tavern.
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Clubhouse);

    // Opened from The Arcade, a lone Esc stays on the page.
    app.handle_input(b"2");
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Zen);
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.pending_escape, "esc flushes on zen").await;
    assert_eq!(app.screen, Screen::Zen, "Esc never leaves Zen");

    // The chord hands the page back to where it was opened.
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Arcade);

    // Leaving by a digit forgets the return page: a later chord on Zen
    // must not jump back to The Arcade.
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Zen);
    app.handle_input(b"1");
    assert_eq!(app.screen, Screen::Dashboard);
    app.set_screen(Screen::Zen);
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Clubhouse);
}

#[tokio::test]
async fn backtick_from_zen_hops_through_the_games_and_comes_home_to_zen() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "zen-backtick-flow").await;
    let mut app = make_app(test_db.db.clone(), user.id, "zen-backtick-flow-it");

    // Zen opened over the Leaderboards, nothing waiting: the hop stays put.
    app.set_screen(Screen::Leaderboard);
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Zen);
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Zen);

    // A loaded Dark Room is a stop: the hop goes in, and the same key comes
    // home to Zen rather than Home chat.
    app.enter_darkroom();
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Darkroom);
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Zen);

    // The trip through the games kept Zen's own way back.
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Leaderboard);

    // Going in from Home comes home to Home.
    app.set_screen(Screen::Dashboard);
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Darkroom);
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard);
}

#[tokio::test]
async fn a_table_opened_from_zen_hands_back_to_zen_on_esc_and_on_backtick() {
    use crate::app::common::primitives::Screen;
    use crate::app::lobby::house::tables::HouseTable;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "zen-table-base").await;
    let mut app = make_app(test_db.db.clone(), user.id, "zen-table-base-it");

    app.set_screen(Screen::Leaderboard);
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Zen);

    // Opened from Zen the way the Lobby modal opens it; Esc hands back to
    // Zen with the Lobby reopened over it, and Zen still knows its way out.
    assert!(
        app.house
            .enter(HouseTable::Blackjack, Screen::Zen, app.chip_balance)
    );
    app.set_screen(Screen::HouseTable);
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| app.screen != Screen::HouseTable,
        "esc leaves the table",
    )
    .await;
    assert_eq!(app.screen, Screen::Zen);
    assert!(app.show_lobby_modal, "expected Esc to reopen the Lobby");
    app.handle_input(b"\x07");
    assert!(!app.show_lobby_modal);
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Leaderboard);

    // Backtick off the same table, nothing else waiting, wraps to Zen too.
    app.handle_input(b"\x06");
    assert!(
        app.house
            .enter(HouseTable::Blackjack, Screen::Zen, app.chip_balance)
    );
    app.set_screen(Screen::HouseTable);
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Zen);
    assert!(!app.show_lobby_modal, "the wrap never opens the Lobby");
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Leaderboard);
}

/// Zen opened over a table, then a Lobby jump from Zen onto a table: going
/// in from Zen, not closing it. Esc and backtick must agree that home is
/// Zen.
#[tokio::test]
async fn a_lobby_jump_from_zen_opened_over_a_table_comes_home_to_zen() {
    use crate::app::common::primitives::Screen;
    use crate::app::lobby::house::tables::HouseTable;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "zen-jump-base").await;
    let mut app = make_app(test_db.db.clone(), user.id, "zen-jump-base-it");

    // Went into the table from Home, so the chain's base is Home.
    app.set_screen(Screen::Dashboard);
    assert!(
        app.house
            .enter(HouseTable::Blackjack, Screen::Dashboard, app.chip_balance)
    );
    app.set_screen(Screen::HouseTable);
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Zen);

    // Jumped onto a table from Zen the way the Lobby modal does it.
    assert!(
        app.house
            .enter(HouseTable::Blackjack, Screen::Zen, app.chip_balance)
    );
    app.set_screen(Screen::HouseTable);
    app.handle_input(b"`");
    assert_eq!(
        app.screen,
        Screen::Zen,
        "backtick wraps to Zen, where Esc would go"
    );

    // Zen still hands back what it was first opened over: not the table,
    // which closed on the way in, but Home, where the table was opened.
    app.handle_input(b"\x06");
    assert_eq!(app.screen, Screen::Dashboard);
}

#[tokio::test]
async fn zen_chat_keys_belong_to_the_focused_chat_tile() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-jk-viewer").await;
    let author = create_test_user(&test_db.db, "zen-jk-author").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge.id, author.id)
        .await
        .expect("join author");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "zen select target".to_string(),
        },
    )
    .await
    .expect("create message");

    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-jk-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "zen select target").await;
    app.handle_input(b"\x06");
    wait_for_render_contains(&mut app, "w tend").await;

    // The first opening lands on the chat tile: `j` selects in its room.
    assert_eq!(
        app.zen.focused_kind(),
        Some(crate::app::zen::state::TileKind::Chat)
    );
    app.handle_input(b"j");
    assert_eq!(
        app.chat.selected_message_id,
        Some(message.id),
        "j on the focused chat tile selects the newest message"
    );

    // Focus moves off the chat: the selection is dropped and `j` is
    // swallowed rather than scrolling a chat nobody is looking at.
    app.handle_input(b"\x1b[D");
    assert_ne!(
        app.zen.focused_kind(),
        Some(crate::app::zen::state::TileKind::Chat)
    );
    assert_eq!(app.chat.selected_message_id, None);
    app.handle_input(b"j");
    assert_eq!(
        app.chat.selected_message_id, None,
        "j with the bonsai focused selects nothing"
    );

    // `i` on the chat tile composes in its room; elsewhere it does nothing.
    app.handle_input(b"i");
    assert!(
        !app.chat.composing,
        "i with the bonsai focused composes nothing"
    );
    app.handle_input(b"\x1b[C");
    app.handle_input(b"i");
    assert!(app.chat.composing, "i on the focused chat tile composes");
}

#[tokio::test]
async fn zen_inbox_enter_opens_an_unread_dm_in_the_first_chat_tile() {
    use crate::app::zen::state::{KindPick, TileKind};

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-inbox-viewer").await;
    let peer = create_test_user(&test_db.db, "zen-inbox-peer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    let dm = ChatRoom::get_or_create_dm(&client, viewer.id, peer.id)
        .await
        .expect("dm room");
    ChatRoomMember::join(&client, dm.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, dm.id, peer.id)
        .await
        .expect("join peer");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: dm.id,
            user_id: peer.id,
            body: "psst".to_string(),
        },
    )
    .await
    .expect("dm message");

    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-inbox-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "lounge").await;
    app.handle_input(b"\x06");
    wait_for_render_contains(&mut app, "w tend").await;

    // The lobby tile becomes an Inbox, which lists the unread DM.
    app.zen.focus = app
        .zen
        .first_tile_of(TileKind::Lobby)
        .expect("the default has a lobby");
    app.zen.open_kind_picker();
    while app.zen.kind_picker_selection() != Some(TileKind::Inbox) {
        app.zen.move_kind_picker(1);
    }
    assert_eq!(app.zen.pick_kind(), KindPick::Changed);
    wait_for_render_contains(&mut app, "@zen-inbox-peer").await;

    app.handle_input(b"\r");
    assert_eq!(
        app.zen.focused_kind(),
        Some(TileKind::Chat),
        "Enter moves the focus to the chat tile"
    );
    assert_eq!(
        app.zen_chat_room_id(),
        Some(dm.id),
        "the chat tile now shows the DM"
    );
}

/// A Live tile shows the #lounge live strip on Zen. Enter on it opens what
/// it shows, and so does `o` from any tile, as on the card.
#[tokio::test]
async fn zen_enter_on_the_live_tile_or_o_anywhere_opens_what_the_strip_shows() {
    use crate::app::zen::state::{KindPick, TileKind};
    use late_core::models::article::{Article, ArticleParams};

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-live-viewer").await;
    let sharer = create_test_user(&test_db.db, "zen-live-sharer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    Article::create_by_user_id(
        &client,
        sharer.id,
        ArticleParams {
            user_id: sharer.id,
            url: "https://example.com/terminal-renaissance".to_string(),
            title: "The terminal renaissance".to_string(),
            summary: "• terminals are back".to_string(),
            ascii_art: "############\n#  late.sh #\n############".to_string(),
        },
    )
    .await
    .expect("share an article");

    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-live-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "lounge").await;
    app.handle_input(b"\x06");
    wait_for_render_contains(&mut app, "w tend").await;

    // The lobby tile becomes a Live tile, which shows the shared link.
    app.zen.focus = app
        .zen
        .first_tile_of(TileKind::Lobby)
        .expect("the default has a lobby");
    app.zen.open_kind_picker();
    while app.zen.kind_picker_selection() != Some(TileKind::Live) {
        app.zen.move_kind_picker(1);
    }
    assert_eq!(app.zen.pick_kind(), KindPick::Changed);
    wait_for_render_contains(&mut app, "The terminal renaissance").await;

    app.handle_input(b"\r");
    assert_eq!(
        app.chat.news_modal_url(),
        Some("https://example.com/terminal-renaissance"),
        "Enter opens the article"
    );
    assert!(!app.chat.is_composing(), "Enter never reaches a composer");

    // `o` with a chat tile focused opens the same article.
    app.chat.close_news_modal();
    app.zen.focus = app
        .zen
        .first_tile_of(TileKind::Chat)
        .expect("the default has a chat");
    app.handle_input(b"o");
    assert_eq!(
        app.chat.news_modal_url(),
        Some("https://example.com/terminal-renaissance"),
        "o opens the article from a chat tile"
    );
    assert!(!app.chat.is_composing(), "o never reaches a composer");
}

#[tokio::test]
async fn zen_clicks_under_the_open_tile_picker_reach_nothing() {
    use crate::app::zen::state::{KindPick, TileKind};
    use late_core::models::article::{Article, ArticleParams};

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-picker-click-viewer").await;
    let sharer = create_test_user(&test_db.db, "zen-picker-click-sharer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    Article::create_by_user_id(
        &client,
        sharer.id,
        ArticleParams {
            user_id: sharer.id,
            url: "https://example.com/under-the-picker".to_string(),
            title: "Under the picker".to_string(),
            summary: "• a link the strip shows".to_string(),
            ascii_art: "####\n####".to_string(),
        },
    )
    .await
    .expect("share an article");

    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-picker-click-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "lounge").await;
    app.handle_input(b"\x06");
    wait_for_render_contains(&mut app, "w tend").await;

    app.zen.focus = app
        .zen
        .first_tile_of(TileKind::Lobby)
        .expect("the default has a lobby");
    app.zen.open_kind_picker();
    while app.zen.kind_picker_selection() != Some(TileKind::Live) {
        app.zen.move_kind_picker(1);
    }
    assert_eq!(app.zen.pick_kind(), KindPick::Changed);
    wait_for_render_contains(&mut app, "Under the picker").await;
    let live = app.zen.focus;

    // The picker is up over the page. The strip under it still records
    // its click rect, but a click there belongs to the picker: nothing
    // opens and the focus stays put, so the picker converts the tile it
    // opened on.
    app.handle_input(b" ");
    assert!(app.zen.kind_picker.is_some(), "space opens the picker");
    render_plain(&mut app);
    let (strip, _) = app.live.hit.get().expect("the live tile drew its strip");
    let click = format!("\x1b[<0;{};{}M", strip.x + strip.width / 2 + 1, strip.y + 1);
    app.handle_input(click.as_bytes());
    assert_eq!(
        app.chat.news_modal_url(),
        None,
        "a click under the picker opens nothing"
    );
    assert!(app.zen.kind_picker.is_some(), "the picker stays up");
    assert_eq!(app.zen.focus, live, "the focus stays under the picker");

    // With the picker closed the same click opens the article.
    app.zen.close_kind_picker();
    render_plain(&mut app);
    app.handle_input(click.as_bytes());
    assert_eq!(
        app.chat.news_modal_url(),
        Some("https://example.com/under-the-picker"),
        "the click opens the article once the picker is gone"
    );
}

#[tokio::test]
async fn zen_a_draft_stays_in_its_room_when_the_focus_moves_and_zoom_shows_the_focused_chat() {
    use crate::app::zen::state::TileKind;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-draft-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    let quiet = ChatRoom::get_or_create_public_room(&client, "zen-quiet")
        .await
        .expect("second room");
    ChatRoomMember::join(&client, quiet.id, viewer.id)
        .await
        .expect("join second room");

    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-draft-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "zen-quiet").await;
    app.handle_input(b"\x06");
    wait_for_render_contains(&mut app, "w tend").await;

    // A second chat tile beside the default one, bound to the second room;
    // the first keeps the current room, #lounge.
    let first = app
        .zen
        .first_tile_of(TileKind::Chat)
        .expect("the default has a chat");
    assert_eq!(app.zen.focus, first);
    assert!(app.zen.split_focused(true));
    let second = app.zen.focus;
    assert!(app.zen.rice.root.set_kind(second, TileKind::Chat));
    assert!(app.zen.bind_focused_chat_room(Some(quiet.id)));

    // A draft written in the second tile, then a click on the first: the
    // draft is closed rather than carried under #lounge, where Enter would
    // have posted it to the room it was written for.
    app.handle_input(b"i");
    app.handle_input(b"secret");
    assert!(app.chat.composing);
    assert_eq!(app.chat.composer_room_id(), Some(quiet.id));
    let (cols, rows) = app.size;
    let (tiles_area, _) = crate::app::zen::layout::rice_areas(
        ratatui::layout::Rect::new(0, 0, cols, rows),
        app.zen_status_row(),
    );
    let rects = crate::app::zen::layout::tile_rects(
        &app.zen.rice.root,
        tiles_area,
        app.zen.rice.look.gap as u16,
        None,
    );
    let (_, rect) = rects[first];
    let click = format!(
        "\x1b[<0;{};{}M",
        rect.x + rect.width / 2 + 1,
        rect.y + rect.height / 2 + 1
    );
    app.handle_input(click.as_bytes());
    assert_eq!(
        app.zen.focus, first,
        "the click focused the first chat tile"
    );
    assert!(
        !app.chat.composing,
        "the draft written for the second room is closed, not shown under #lounge"
    );
    assert_eq!(app.chat.composer_room_id(), None);
    app.handle_input(b"i");
    assert_eq!(
        app.chat.composer_room_id(),
        Some(lounge.id),
        "a new draft belongs to the focused tile's room"
    );
    app.chat.reset_composer();

    // Zoom the second tile: the one pane on show is its room.
    app.handle_input(b"\x1b[C");
    assert_eq!(app.zen.focus, second);
    app.handle_input(b"z");
    assert!(app.zen.zoomed);
    let rendered = strip_ansi(&render_plain(&mut app));
    assert!(
        rendered.contains("#zen-quiet"),
        "the zoomed pane is the focused tile's room, not the first chat's:\n{rendered}"
    );
}

#[tokio::test]
async fn zen_petting_the_pet_leaves_the_focus_on_the_chat() {
    use crate::app::hub::shop::{
        entitlements::ShopEntitlements, state::ShopState, svc::ShopSnapshot,
    };
    use crate::app::zen::state::TileKind;
    use late_core::models::marketplace::PET_COMPANION_SKU;
    use late_core::models::pet::PetMood;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-pet-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");

    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-pet-flow-it");
    app.shop_state = ShopState::for_test_snapshot(ShopSnapshot {
        entitlements: ShopEntitlements::from_owned_skus([PET_COMPANION_SKU.to_string()]),
        ..Default::default()
    });
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "lounge").await;
    app.handle_input(b"\x06");
    wait_for_render_contains(&mut app, "w tend").await;

    let chat = app
        .zen
        .first_tile_of(TileKind::Chat)
        .expect("the default has a chat");
    assert_eq!(app.zen.focus, chat, "the page opens on its chat tile");

    // The pet sits on the rail, in a tile of its own. Petting it is a
    // passing gesture: the keys stay with the chat.
    let pet = app.last_pet_rect.get().expect("the pet drew on its tile");
    let click = format!("\x1b[<0;{};{}M", pet.x + 1, pet.y + 1);
    app.handle_input(click.as_bytes());
    assert_eq!(
        app.zen.focus, chat,
        "petting the pet leaves the focus on the chat tile"
    );
    render_plain(&mut app);
    assert_eq!(
        app.pet_state.mood(),
        PetMood::Purring,
        "the click landed on the pet"
    );
}

#[tokio::test]
async fn sidebar_pet_panel_is_view_only() {
    use crate::app::hub::shop::{
        entitlements::ShopEntitlements, state::ShopState, svc::ShopSnapshot,
    };
    use crate::app::profile::state::profile_params_from_profile;
    use late_core::models::marketplace::PET_COMPANION_SKU;
    use late_core::models::pet::PetMood;
    use late_core::models::profile::Profile;
    use late_core::models::user::{RightSidebarComponent, RightSidebarComponentSetting};

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "rail-pet-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    // The pet panel alone on the rail, so every cell under the clock block
    // is either the pet's or empty.
    let profile = Profile::load(&client, viewer.id)
        .await
        .expect("load profile");
    let mut params = profile_params_from_profile(&profile);
    params.right_sidebar_components = RightSidebarComponent::ALL
        .into_iter()
        .map(|component| RightSidebarComponentSetting {
            component,
            enabled: component == RightSidebarComponent::Pet,
        })
        .collect();
    Profile::update(&client, viewer.id, params)
        .await
        .expect("save the rail");

    let (cols, rows) = (160u16, 40u16);
    let mut app = make_app(test_db.db.clone(), viewer.id, "rail-pet-flow-it");
    app.shop_state = ShopState::for_test_snapshot(ShopSnapshot {
        entitlements: ShopEntitlements::from_owned_skus([PET_COMPANION_SKU.to_string()]),
        ..Default::default()
    });
    app.resize(cols, rows).expect("resize test terminal");
    wait_for_render_contains(&mut app, "── pet").await;

    // Click every cell of the rail under the top border: wherever the pet
    // stands in its box, one of these lands on it.
    let rail_width = crate::app::render::RIGHT_SIDEBAR_WIDTH;
    for y in 1..rows - 1 {
        for x in cols - 1 - rail_width..cols - 1 {
            let click = format!("\x1b[<0;{};{}M", x + 1, y + 1);
            app.handle_input(click.as_bytes());
        }
    }
    render_plain(&mut app);
    assert_eq!(
        app.pet_state.mood(),
        PetMood::Idle,
        "a click on the sidebar pet is not a pet: Zen is where it is petted"
    );
    assert!(
        !app.pet_state.petted_on(chrono::Utc::now().date_naive()),
        "the sidebar pet never claims the daily chips"
    );
}

#[tokio::test]
async fn zen_every_chat_tile_keeps_its_composer_whatever_is_focused() {
    use crate::app::zen::state::TileKind;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-composer-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    let quiet = ChatRoom::get_or_create_public_room(&client, "zen-comp")
        .await
        .expect("second room");
    ChatRoomMember::join(&client, quiet.id, viewer.id)
        .await
        .expect("join second room");

    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-composer-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "lounge").await;
    app.handle_input(b"\x06");
    wait_for_render_contains(&mut app, "w tend").await;

    let first = app
        .zen
        .first_tile_of(TileKind::Chat)
        .expect("the default has a chat");
    assert!(app.zen.split_focused(true));
    let second = app.zen.focus;
    assert!(app.zen.rice.root.set_kind(second, TileKind::Chat));
    assert!(app.zen.bind_focused_chat_room(Some(quiet.id)));

    // Both tiles carry a composer, and walking the focus moves nothing:
    // an input box that comes and goes is the layout jumping under you.
    // The focused tile's strip is live, the other's says it only watches
    // (its keys act on the focused tile, so it must not name them).
    let frame = render_plain(&mut app);
    assert_eq!(
        (
            frame.matches("Compose").count(),
            frame.matches("watching").count()
        ),
        (1, 1),
        "both chat tiles draw a composer, one live and one watching; frame={frame:?}"
    );
    assert_eq!(
        frame.matches("j/k select").count(),
        1,
        "only the live composer names the chat keys; frame={frame:?}"
    );

    app.zen.focus = first;
    crate::app::zen::input::focus_moved(&mut app);
    let frame = render_plain(&mut app);
    assert_eq!(
        (
            frame.matches("Compose").count(),
            frame.matches("watching").count()
        ),
        (1, 1),
        "the composers stay put when the focus walks; frame={frame:?}"
    );
}

/// `[` `]` on a Zen chat tile walk the rooms in the rail's order, top to
/// bottom, not the order they were loaded in: Core's fixed order puts
/// suggestions before bugs, though bugs loads first alphabetically.
#[tokio::test]
async fn zen_brackets_cycle_rooms_in_rail_order() {
    use crate::app::zen::state::TileKind;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-cycle-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let mut rooms = Vec::new();
    rooms.push(ChatRoom::ensure_lounge(&client).await.expect("lounge"));
    rooms.push(
        ChatRoom::ensure_permanent(&client, "suggestions")
            .await
            .expect("suggestions"),
    );
    rooms.push(
        ChatRoom::ensure_permanent(&client, "bugs")
            .await
            .expect("bugs"),
    );
    rooms.push(
        ChatRoom::get_or_create_public_room(&client, "zen-cycle")
            .await
            .expect("channel"),
    );
    for room in &rooms {
        ChatRoomMember::join(&client, room.id, viewer.id)
            .await
            .expect("join room");
    }
    let [lounge, suggestions, bugs, channel] = [rooms[0].id, rooms[1].id, rooms[2].id, rooms[3].id];

    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-cycle-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "zen-cycle").await;
    app.handle_input(b"\x06");
    wait_for_render_contains(&mut app, "w tend").await;
    assert_eq!(app.zen.focused_kind(), Some(TileKind::Chat));
    assert_eq!(app.zen_chat_room_id(), Some(lounge));

    let mut forward = Vec::new();
    for _ in 0..4 {
        app.handle_input(b"]");
        forward.push(app.zen_chat_room_id().expect("bound room"));
    }
    assert_eq!(forward, vec![suggestions, bugs, channel, lounge]);

    app.handle_input(b"[");
    assert_eq!(
        app.zen_chat_room_id(),
        Some(channel),
        "[ wraps to the bottom"
    );
}

#[tokio::test]
async fn zen_room_picker_binds_the_focused_chat_tile_and_slash_picker_opens_it() {
    use crate::app::common::primitives::Screen;
    use crate::app::zen::state::TileKind;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-picker-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    let quiet = ChatRoom::get_or_create_public_room(&client, "zen-picked")
        .await
        .expect("second room");
    ChatRoomMember::join(&client, quiet.id, viewer.id)
        .await
        .expect("join second room");

    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-picker-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "zen-picked").await;
    let home_selection = app.chat.selected_room_id;
    app.handle_input(b"\x06");
    wait_for_render_contains(&mut app, "w tend").await;
    assert_eq!(app.zen.focused_kind(), Some(TileKind::Chat));
    assert_eq!(
        app.zen.focused_chat_room(),
        Some(None),
        "the default chat tile follows Home's selection"
    );

    // `/picker` from the tile's composer opens the same modal as Ctrl+/.
    app.handle_input(b"i/picker\r");
    assert!(
        app.room_search_modal_state.is_open(),
        "/picker opens the room picker"
    );

    // A room picked with a chat tile focused binds that tile, like [ ]:
    // the page stays up and Home's selection is untouched.
    app.handle_input(b"zen-picked\r");
    assert!(
        !app.room_search_modal_state.is_open(),
        "the pick closes the picker"
    );
    assert_eq!(app.screen, Screen::Zen, "the pick stays on Zen");
    assert_eq!(
        app.zen.focused_chat_room(),
        Some(Some(quiet.id)),
        "the focused chat tile is bound to the picked room"
    );
    assert_eq!(app.zen_chat_room_id(), Some(quiet.id));
    assert_eq!(
        app.chat.selected_room_id, home_selection,
        "Home's selection does not move"
    );

    // With no chat tile focused the pick moves Home's selection as before.
    app.handle_input(b"\x1b[D");
    assert_ne!(app.zen.focused_kind(), Some(TileKind::Chat));
    app.handle_input(b"\x1f");
    assert!(
        app.room_search_modal_state.is_open(),
        "Ctrl+/ opens the picker"
    );
    app.handle_input(b"zen-picked\r");
    assert_eq!(app.screen, Screen::Zen);
    assert_eq!(app.chat.selected_room_id, Some(quiet.id));
}

#[tokio::test]
async fn zen_space_opens_a_tile_picker_that_owns_the_keys_until_a_pick_or_esc() {
    use crate::app::common::primitives::Screen;
    use crate::app::zen::state::TileKind;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "zen-picker-tiles").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "zen-tile-picker-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, " Home ").await;
    app.handle_input(b"\x06");
    wait_for_render_contains(&mut app, "w tend").await;
    assert_eq!(app.zen.focused_kind(), Some(TileKind::Chat));
    let tiles = app.zen.leaf_count();

    // Space opens the list on the tile's own kind, and names it.
    app.handle_input(b" ");
    assert_eq!(app.zen.kind_picker_selection(), Some(TileKind::Chat));
    let frame = render_plain(&mut app);
    assert!(
        frame.contains(" tile ") && frame.contains("current") && frame.contains("visualizer"),
        "the picker lists every kind and marks the current one; frame={frame:?}"
    );
    assert!(
        frame.contains("jk move") && frame.contains("enter pick") && frame.contains("esc close"),
        "the picker names its keys; frame={frame:?}"
    );

    // The picker owns the keys: `S` splits nothing and `q` quits nothing.
    app.handle_input(b"S");
    app.handle_input(b"q");
    assert_eq!(
        app.zen.leaf_count(),
        tiles,
        "S under the picker splits nothing"
    );
    assert!(!app.show_quit_confirm, "q under the picker quits nothing");
    assert!(app.zen.kind_picker.is_some());

    // One row down and Enter: the tile is a clock, the picker is gone.
    app.handle_input(b"j");
    app.handle_input(b"\r");
    assert!(app.zen.kind_picker.is_none(), "a pick closes the picker");
    assert_eq!(app.zen.focused_kind(), Some(TileKind::Clock));
    assert_eq!(app.screen, Screen::Zen);

    // Esc closes it without a change and stays on the page.
    app.handle_input(b" ");
    app.handle_input(b"k");
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| app.zen.kind_picker.is_none(),
        "esc closes the tile picker",
    )
    .await;
    assert_eq!(app.zen.focused_kind(), Some(TileKind::Clock));
    assert_eq!(
        app.screen,
        Screen::Zen,
        "Esc under the picker does not leave Zen"
    );
}

#[tokio::test]
async fn f_favorites_the_bugs_room_from_the_rail() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-fav-bugs").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    let bugs = ChatRoom::ensure_permanent(&client, "bugs")
        .await
        .expect("ensure bugs room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    ChatRoomMember::join(&client, bugs.id, viewer.id)
        .await
        .expect("join bugs");
    // Seeded before the app starts: a raw insert mid-test is never broadcast,
    // so it would only render if an unrelated refresh happened to land after it.
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: bugs.id,
            user_id: viewer.id,
            body: "---BUG--- a report to read".to_string(),
        },
    )
    .await
    .expect("create message");

    let mut app = make_app(test_db.db.clone(), viewer.id, "f-fav-bugs-flow-it");
    app.resize(160, 32).expect("resize test terminal");
    wait_for_render_contains(&mut app, "bugs").await;

    // Core order is lounge, then bugs: one step right lands on it.
    app.handle_input(b"l");
    assert_eq!(app.chat.selected_room_id, Some(bugs.id));
    wait_for_render_contains(&mut app, "a report to read").await;

    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "Added to favorites").await;
    assert!(app.chat.favorite_room_ids().contains(&bugs.id));

    // The favorite has to survive the profile round trip that tick mirrors
    // back into chat, and the rail has to show the section.
    wait_for_render_contains(&mut app, "favorites").await;
    assert!(app.chat.favorite_room_ids().contains(&bugs.id));

    // With a message selected, `f` belongs to the reaction leader and never
    // reaches the favorite toggle: the favorite stays exactly as it was.
    app.handle_input(b"j");
    assert!(app.chat.selected_message_id.is_some());
    app.handle_input(b"f");
    assert!(app.chat.is_reaction_leader_active());
    assert!(app.chat.favorite_room_ids().contains(&bugs.id));
}

#[tokio::test]
async fn f_favorites_the_mentions_entry() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-fav-mentions").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");

    let mut app = make_app(test_db.db.clone(), viewer.id, "f-fav-mentions-flow-it");
    app.resize(160, 32).expect("resize test terminal");
    // The rail renders its synthetic rows before the room list arrives; the
    // selected-row marker on lounge is what says the walk below can start.
    wait_for_render_contains(&mut app, "\u{258C}lounge").await;
    assert_eq!(app.chat.selected_room_id, Some(lounge.id));

    // Core order here is lounge, then mentions: one step right lands on it.
    app.handle_input(b"l");
    assert!(app.chat.notifications_selected);

    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "Added to favorites").await;
    let mentions_id = crate::app::chat::state::synthetic_favorite_id(
        crate::app::chat::state::RoomSlot::Notifications,
    )
    .expect("mentions is favoritable");
    assert!(app.chat.favorite_room_ids().contains(&mentions_id));

    // It has to survive the profile round trip tick mirrors back into chat,
    // and the rail has to show the section it moved into.
    wait_for_render_contains(&mut app, "favorites").await;
    assert!(app.chat.favorite_room_ids().contains(&mentions_id));

    // A second press takes it back out.
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "Removed from favorites").await;
    assert!(!app.chat.favorite_room_ids().contains(&mentions_id));
}

/// The Chat badges picker lists every badge; a game's ladder is one row, and
/// hiding it stores every rung so no lower one takes its place.
#[tokio::test]
async fn chat_badges_picker_hides_a_whole_game_ladder() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "badge-picker-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "badge-picker-flow-it");
    app.resize(160, 40).expect("resize test terminal");

    app.handle_input(b"\x0f");
    wait_for_render_contains(&mut app, "badge-picker-it").await;
    app.handle_input(b"\t\t\t");
    wait_for_render_contains(&mut app, "Chat badges").await;
    wait_for_render_contains(&mut app, "all shown").await;
    // Tweaks rows: background, brightness, right rail, room rail, composer,
    // interaction mode, plain glyphs, terminal images, then Chat badges.
    app.handle_input(b"jjjjjjjj\r");
    // The heading fits the dialog whole, not cut at its border.
    wait_for_render_contains(&mut app, "Earn it, hide it. Games show their top badge.").await;
    wait_for_render_contains(&mut app, "LMG LKN LYS LKA").await;

    // Picker rows in label order: the eight monthly rows (the crown is
    // painted as a glyph, not a code, so it has no row), then Lateania.
    app.handle_input(b"jjjjjjjj\r");
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move {
                let client = db.get().await.expect("db client");
                let stored = User::get(&client, user.id)
                    .await
                    .expect("load user")
                    .expect("user exists");
                late_core::models::user::extract_hidden_award_categories(&stored.settings)
                    == vec![
                        "lateania_archdemon".to_string(),
                        "lateania_frontier_king".to_string(),
                        "lateania_sundering_deep".to_string(),
                        "lateania_kaethyr_ascendant".to_string(),
                    ]
            }
        },
        "the whole Lateania ladder to be hidden",
    )
    .await;

    app.handle_input(b"\x1b");
    wait_for_render_contains(&mut app, "1 hidden").await;
    // Closing the picker restores top-level tab navigation.
    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, "Keyhints").await;
    assert_eq!(
        app.settings_modal_state.selected_tab(),
        crate::app::settings_modal::state::Tab::Statusline
    );
}

/// Ctrl+H / Ctrl+L and the wheel over the rail scroll it without changing
/// room; `l` still changes room, and the rail snaps back to the selection.
#[tokio::test]
async fn rail_scroll_keys_and_wheel_leave_the_selected_room_alone() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "rail-scroll").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    // Enough channels that the rail overflows a 32-row terminal.
    for i in 0..40 {
        let room = ChatRoom::get_or_create_public_room(&client, &format!("rail-scroll-{i:02}"))
            .await
            .expect("create room");
        ChatRoomMember::join(&client, room.id, viewer.id)
            .await
            .expect("join room");
    }

    let mut app = make_app(test_db.db.clone(), viewer.id, "rail-scroll-flow-it");
    app.resize(160, 32).expect("resize test terminal");
    wait_for_render_contains(&mut app, "\u{258C}lounge").await;
    wait_for_render_contains(&mut app, "rail-scroll-").await;
    let selected = app.chat.selected_room_id;
    assert_eq!(app.chat.rail_scroll_nudge(), 0);

    app.handle_input(b"\x0c");
    assert_eq!(app.chat.selected_room_id, selected, "Ctrl+L changed room");
    assert_eq!(app.chat.rail_scroll_nudge(), 3);
    app.handle_input(b"\x0c");
    assert_eq!(app.chat.rail_scroll_nudge(), 6);
    app.handle_input(b"\x08");
    assert_eq!(app.chat.selected_room_id, selected, "Ctrl+H changed room");
    assert_eq!(app.chat.rail_scroll_nudge(), 3);

    // Wheel down, then up, over the rail (column 5, row 10).
    app.handle_input(b"\x1b[<65;5;10M");
    assert_eq!(
        app.chat.selected_room_id, selected,
        "the wheel changed room"
    );
    assert_eq!(app.chat.rail_scroll_nudge(), 6);
    app.handle_input(b"\x1b[<64;5;10M");
    assert_eq!(app.chat.rail_scroll_nudge(), 3);

    // Scrolling up past the top stops there: the next press down moves.
    for _ in 0..5 {
        app.handle_input(b"\x08");
    }
    assert_eq!(app.chat.rail_scroll_nudge(), 0);
    app.handle_input(b"\x0c");
    assert_eq!(app.chat.rail_scroll_nudge(), 3);

    // A space jump centres the rail, even onto the room already selected.
    app.handle_input(b" a");
    assert_eq!(app.chat.selected_room_id, selected, "`space a` left lounge");
    assert_eq!(
        app.chat.rail_scroll_nudge(),
        0,
        "a space jump kept the rail scrolled"
    );
    app.handle_input(b"\x0c");
    assert_eq!(app.chat.rail_scroll_nudge(), 3);

    // `l` moves to the next rail entry (Mentions, after lounge).
    app.handle_input(b"l");
    assert_eq!(
        app.chat.rail_scroll_nudge(),
        0,
        "a selection change snaps the rail back to it"
    );
    // Returning to the room the rail was scrolled on does not revive the
    // old scroll: leaving it dropped the nudge for good.
    app.handle_input(b"h");
    assert_eq!(
        app.chat.selected_room_id, selected,
        "`h` went back to lounge"
    );
    assert_eq!(
        app.chat.rail_scroll_nudge(),
        0,
        "coming back to the scrolled room revived its stale scroll"
    );
    app.handle_input(b"l");

    // A click on a row of a scrolled rail selects that room and leaves the
    // rail where it was: the same row under the pointer is still that room,
    // so a second click there changes nothing. Had the rail re-centred on
    // the new selection, the row would have moved out from under the click.
    app.handle_input(b"\x0c");
    app.handle_input(b"\x0c");
    assert_eq!(app.chat.rail_scroll_nudge(), 6);
    let before_click = app.chat.selected_room_id;
    app.handle_input(b"\x1b[<0;5;22M");
    let clicked = app.chat.selected_room_id;
    assert_ne!(
        clicked, before_click,
        "the click selected the room under it"
    );
    assert!(clicked.is_some(), "the click landed on a room row");
    app.handle_input(b"\x1b[<0;5;22M");
    assert_eq!(
        app.chat.selected_room_id, clicked,
        "the rail stayed put, so the same row is still the same room"
    );
}

#[tokio::test]
async fn the_first_descent_opens_the_guide_and_the_question_mark_reopens_it() {
    use crate::app::deadchannel::runner::state::Look;
    use crate::app::deadchannel::runner::svc::RunnerEntry;
    use late_core::models::deadchannel_runner::DeadchannelRunner;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashMap;
    use std::sync::Arc;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "undercity-guide-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut rng = StdRng::seed_from_u64(7);
    let look = Look::random(1, &mut rng);
    // The claim is on the row, so the runner needs one.
    DeadchannelRunner::ensure_for_user(&client, user.id, &look.to_json())
        .await
        .expect("a runner");
    let mut app = make_app(test_db.db.clone(), user.id, "undercity-guide-flow");
    app.runner_looks = Arc::new(HashMap::from([(
        user.id,
        RunnerEntry {
            look,
            level: 1,
            peak_level: 1,
            marks: 0,
        },
    )]));

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Clubhouse ").await;
    app.handle_input(b"0");
    // The chrome names the key, and the first descent opens the guide by
    // itself once the claim answers.
    wait_for_render_contains(&mut app, " Undercity · f road · p patch · ? guide ").await;
    wait_for_render_contains(&mut app, "the street, explained").await;
    // It opens at the top: the whole game in one screen.
    wait_for_render_contains(&mut app, "the short version").await;

    // Esc closes it; `?` opens it again from the street; `q` closes it.
    app.handle_input(b"\x1b");
    wait_for_render_not_contains(&mut app, "the street, explained").await;
    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "the street, explained").await;
    app.handle_input(b"q");
    wait_for_render_not_contains(&mut app, "the street, explained").await;

    // A second descent finds the street, not the guide.
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Clubhouse ").await;
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Undercity ").await;
    assert_render_not_contains_for(
        &mut app,
        "the street, explained",
        Duration::from_millis(300),
    )
    .await;
}

/// `p` opens patch from anywhere on the street, the same panel as Enter at
/// the counter, so the heal is one key away from any fight. Enter closes it.
#[tokio::test]
async fn p_opens_patch_from_anywhere_on_the_street() {
    use crate::app::deadchannel::runner::state::Look;
    use crate::app::deadchannel::runner::svc::RunnerEntry;
    use late_core::models::deadchannel_runner::DeadchannelRunner;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashMap;
    use std::sync::Arc;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "undercity-patch-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut rng = StdRng::seed_from_u64(7);
    let look = Look::random(1, &mut rng);
    DeadchannelRunner::ensure_for_user(&client, user.id, &look.to_json())
        .await
        .expect("a runner");
    // Seen before, so the guide stays shut and the street takes the key.
    DeadchannelRunner::mark_guide_seen(&client, user.id)
        .await
        .expect("guide seen");
    let mut app = make_app(test_db.db.clone(), user.id, "undercity-patch-flow");
    app.runner_looks = Arc::new(HashMap::from([(
        user.id,
        RunnerEntry {
            look,
            level: 1,
            peak_level: 1,
            marks: 0,
        },
    )]));

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Clubhouse ").await;
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Undercity ").await;
    // The descent lands the runner at the stairs, nowhere near the counter.
    app.handle_input(b"p");
    wait_for_render_contains(&mut app, " Esc closes ").await;
    wait_for_render_contains(&mut app, "on hand ").await;
    app.handle_input(b"\r");
    wait_for_render_not_contains(&mut app, " Esc closes ").await;
}

/// A scene whose last answer was the service failing is not a fight to be
/// trapped in: Esc closes it instead of running.
#[tokio::test]
async fn esc_closes_a_scene_the_static_stopped_answering() {
    use crate::app::deadchannel::fight::svc::FightOutcome;
    use crate::app::deadchannel::runner::state::Look;
    use crate::app::deadchannel::runner::svc::RunnerEntry;
    use late_core::models::deadchannel_runner::DeadchannelRunner;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashMap;
    use std::sync::Arc;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "undercity-esc-failed-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut rng = StdRng::seed_from_u64(7);
    let look = Look::random(1, &mut rng);
    DeadchannelRunner::ensure_for_user(&client, user.id, &look.to_json())
        .await
        .expect("a runner");
    DeadchannelRunner::mark_guide_seen(&client, user.id)
        .await
        .expect("guide seen");
    let mut app = make_app(test_db.db.clone(), user.id, "undercity-esc-failed-flow");
    app.runner_looks = Arc::new(HashMap::from([(
        user.id,
        RunnerEntry {
            look,
            level: 1,
            peak_level: 1,
            marks: 0,
        },
    )]));

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Clubhouse ").await;
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Undercity ").await;
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "[Enter] fight").await;
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "[a] auto turn").await;

    // The service fails to answer the next command: an outage, as the
    // session would hear it.
    app.fight
        .outcome_tx
        .send(FightOutcome::ActionFailed)
        .expect("the session is listening");
    wait_for_render_contains(&mut app, "the static is not answering").await;

    // Esc is not a run then: the scene closes and the street is back.
    app.handle_input(b"\x1b");
    wait_for_render_not_contains(&mut app, " the end of the row ").await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains(" Undercity "),
        "expected the street under the closed scene; frame={frame:?}"
    );
}

#[tokio::test]
async fn esc_in_a_fight_is_a_run() {
    use crate::app::deadchannel::fight::data::RUN_LINES;
    use crate::app::deadchannel::runner::state::Look;
    use crate::app::deadchannel::runner::svc::RunnerEntry;
    use late_core::models::deadchannel_runner::DeadchannelRunner;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Instant;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "undercity-esc-run-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut rng = StdRng::seed_from_u64(7);
    let look = Look::random(1, &mut rng);
    DeadchannelRunner::ensure_for_user(&client, user.id, &look.to_json())
        .await
        .expect("a runner");
    DeadchannelRunner::mark_guide_seen(&client, user.id)
        .await
        .expect("guide seen");
    let mut app = make_app(test_db.db.clone(), user.id, "undercity-esc-run-flow");
    app.runner_looks = Arc::new(HashMap::from([(
        user.id,
        RunnerEntry {
            look,
            level: 1,
            peak_level: 1,
            marks: 0,
        },
    )]));

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Clubhouse ").await;
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Undercity ").await;
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "[Enter] fight").await;
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "[a] auto turn").await;

    // No dice: the flicker's hit lands on the way out, and the runner is
    // out. The scene stays up, over, on the getaway line.
    app.handle_input(b"\x1b");
    let deadline = Instant::now() + Duration::from_secs(5);
    let frame = loop {
        let frame = render_plain(&mut app);
        if RUN_LINES.iter().any(|line| frame.contains(line)) {
            break frame;
        }
        assert!(
            Instant::now() < deadline,
            "expected Esc to resolve as a run; frame={frame:?}"
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
    };
    assert!(
        frame.contains(" the end of the row "),
        "expected the scene to stay up after the run; frame={frame:?}"
    );
    assert!(
        frame.contains("it hits you for "),
        "expected the hit on the way out; frame={frame:?}"
    );
    assert!(
        frame.contains("[Enter] back to the road"),
        "expected the scene over; frame={frame:?}"
    );

    // Enter goes back to the road: the step is spent, the next one waits.
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, " the road ").await;
    wait_for_render_contains(&mut app, "rations 9/10").await;
}

/// The hand takes its digits while a fight is on: `1` to `5` play the
/// card in that slot and never switch pages, `e` ends the turn, and the
/// glyph answers.
#[tokio::test]
async fn the_number_keys_play_cards_and_e_ends_the_turn() {
    let (_test_db, mut app, _) = runner_on_the_street("undercity-cards-it", 100, 34, |_| {}).await;

    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "[Enter] fight").await;
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "[1-5] play").await;
    wait_for_render_contains(&mut app, "energy ██ ██ ██").await;

    // Whatever was dealt, slot one holds a card that costs something.
    app.handle_input(b"1");
    wait_for_render_not_contains(&mut app, "energy ██ ██ ██").await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains(" Undercity "),
        "a card key is not a page switch; frame={frame:?}"
    );
    assert!(
        !frame.contains("╭ 1 "),
        "the played slot is empty; frame={frame:?}"
    );

    // The turn ends: the flicker hits, and a full hand is back.
    app.handle_input(b"e");
    wait_for_render_contains(&mut app, "it hits you for ").await;
    wait_for_render_contains(&mut app, "energy ██ ██ ██").await;
    wait_for_render_contains(&mut app, "╭ 1 ").await;
}

/// A runner on the street: the row shaped by `shape` on today's day,
/// the guide already seen (so the street takes the keys), the session
/// descended through the clubhouse on a `width` by `height` terminal.
async fn runner_on_the_street(
    name: &str,
    width: u16,
    height: u16,
    shape: impl FnOnce(&mut crate::app::deadchannel::fight::state::Sheet),
) -> (late_core::test_utils::TestDb, crate::app::state::App, Uuid) {
    use crate::app::deadchannel::fight::state::Sheet;
    use crate::app::deadchannel::fight::svc::FightService;
    use crate::app::deadchannel::runner::state::Look;
    use crate::app::deadchannel::runner::svc::RunnerEntry;
    use late_core::models::deadchannel_runner::DeadchannelRunner;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashMap;
    use std::sync::Arc;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, name).await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let look = Look::random(1, &mut StdRng::seed_from_u64(7));
    let (row, _) = DeadchannelRunner::ensure_for_user(&client, user.id, &look.to_json())
        .await
        .expect("a runner");
    DeadchannelRunner::mark_guide_seen(&client, user.id)
        .await
        .expect("guide seen");
    let mut sheet = Sheet::from_row(&row).expect("sheet");
    sheet.day = FightService::today();
    shape(&mut sheet);
    DeadchannelRunner::store_sheet(&**client, sheet.to_write())
        .await
        .expect("store");
    let mut app = make_app(test_db.db.clone(), user.id, &format!("{name}-flow"));
    app.resize(width, height).unwrap();
    app.runner_looks = Arc::new(HashMap::from([(
        user.id,
        RunnerEntry {
            look,
            level: sheet.level,
            peak_level: sheet.peak_level,
            marks: 0,
        },
    )]));

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Clubhouse ").await;
    app.handle_input(b"0");
    wait_for_render_contains(&mut app, " Undercity ").await;
    (test_db, app, user.id)
}

/// A spent runner's `f` opens the road on the reason, and Enter (what
/// its key row offers) lands back on the street: no scene opens only to
/// repeat the refusal the road already showed.
#[tokio::test]
async fn enter_on_a_spent_runners_road_lands_back_on_the_street() {
    let (_test_db, mut app, _) = runner_on_the_street("undercity-spent-it", 100, 30, |sheet| {
        sheet.rations_left = 0
    })
    .await;

    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "the road is walked. the static will keep").await;
    wait_for_render_contains(&mut app, "[Enter] back to the street").await;

    app.handle_input(b"\r");
    wait_for_render_not_contains(&mut app, "the road is walked").await;
    assert_render_not_contains_for(&mut app, " the end of the row ", Duration::from_millis(300))
        .await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains(" Undercity "),
        "expected the street; frame={frame:?}"
    );
}

/// At Dead Air each glass key pours onto the row, and at the blade cart
/// each slot key buys onto the row. A letter the open panel does not own
/// stays with it instead of reaching a global: `w` (the cart's key, Bonsai
/// Care everywhere else) at the bar, `m` (the paired client's mute) at
/// both, as over the picker and the scene.
#[tokio::test]
async fn the_bar_and_the_cart_take_their_keys_and_keep_the_rest() {
    use crate::app::deadchannel::city::map::Landmark;
    use crate::app::deadchannel::fight::state::Drink;
    use late_core::models::deadchannel_runner::DeadchannelRunner;

    let (test_db, mut app, user_id) =
        runner_on_the_street("undercity-bar-cart-it", 100, 30, |sheet| sheet.crystals = 4).await;
    let row = || {
        let db = test_db.db.clone();
        async move {
            let client = db.get().await.expect("db client");
            DeadchannelRunner::find_by_user(&client, user_id)
                .await
                .expect("find")
                .expect("row")
        }
    };

    // Dead Air, as Enter at the bar opens it.
    app.city.open_panel(Landmark::Bar);
    app.fight.clear_till();
    wait_for_render_contains(&mut app, " Esc closes ").await;
    app.banner = None;
    app.handle_input(b"w");
    app.handle_input(b"m");
    assert!(!app.show_bonsai_modal, "`w` at the bar is not Bonsai Care");
    assert!(app.banner.is_none(), "`m` at the bar is not the mute");
    assert_eq!(app.city.panel(), Some(Landmark::Bar), "the bar stays open");

    app.handle_input(b"s");
    wait_for_render_contains(&mut app, "static on ice. it goes down like a short circuit").await;
    let poured = row().await;
    assert_eq!(poured.drink.as_deref(), Some(Drink::StaticOnIce.code()));
    assert_eq!(poured.crystals, 3, "a glass is a crystal");
    assert_eq!(
        poured.weapon_tier, 0,
        "the bar's keys buy nothing at the cart"
    );

    app.handle_input(b"\r");
    wait_for_render_not_contains(&mut app, " Esc closes ").await;

    // The blade cart, the same way.
    app.city.open_panel(Landmark::Blades);
    app.fight.clear_till();
    wait_for_render_contains(&mut app, " Esc closes ").await;
    app.handle_input(b"m");
    assert!(app.banner.is_none(), "`m` at the cart is not the mute");
    assert_eq!(
        app.city.panel(),
        Some(Landmark::Blades),
        "the cart stays open"
    );

    app.handle_input(b"w");
    wait_for_render_contains(&mut app, "comes off the rack").await;
    let carted = row().await;
    assert_eq!(carted.weapon_tier, 1, "`w` is the weapon off the cart");
    assert_eq!(carted.armor_tier, 0);
    assert_eq!(carted.crystals, 0, "three crystals for the piece");
    assert_eq!(
        carted.drink.as_deref(),
        Some(Drink::StaticOnIce.code()),
        "the glass is untouched"
    );

    // The page digits still reach the globals from over a panel.
    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Home ").await;
}

/// The road and the scene both fit a classic 80 by 24 terminal under the
/// app's frame: the map, the node under the cursor with its step-down
/// key, and the key row are on screen, and so is the whole hand.
#[tokio::test]
async fn the_road_and_the_hand_fit_an_80_by_24_terminal() {
    let (_test_db, mut app, _) = runner_on_the_street("undercity-road-24-it", 80, 24, |sheet| {
        sheet.level = 2;
        sheet.peak_level = 2;
        sheet.signal = 20;
    })
    .await;

    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "[Enter] fight").await;
    let frame = render_plain(&mut app);
    for needle in [
        " the road ",
        "▸ [f]",
        "hiss  lv 2",
        "[▚]",
        "[g] flicker, half pay",
        "esc back",
    ] {
        assert!(
            frame.contains(needle),
            "expected {needle:?} on an 80 by 24 road; frame={frame:?}"
        );
    }

    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "[1-5] play").await;
    let frame = render_plain(&mut app);
    for needle in ["╭ 1 ", "╭ 5 ", "energy ██ ██ ██", "[e] end turn", "[r] run"] {
        assert!(
            frame.contains(needle),
            "expected {needle:?} on an 80 by 24 scene; frame={frame:?}"
        );
    }
}
