//! App input integration tests against a real ephemeral DB.

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
    user::User,
};
use late_core::test_utils::create_test_user;
use tokio::time::Duration;
use uuid::Uuid;

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
    let steps = HubGame::ALL
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
    for _ in 0..4 {
        app.handle_input(b"\t");
    }
    app.handle_input(b"jj");
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

    // `i` opens the project (showcase) composer, Esc closes it.
    app.handle_input(b"i");
    wait_for_render_contains(&mut app, " New showcase ").await;
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.chat.showcase.composing(), "showcase").await;

    // `w` opens the work-card composer, Esc closes it.
    app.handle_input(b"w");
    wait_for_render_contains(&mut app, " New work profile ").await;
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.chat.work.composing(), "work").await;

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
async fn global_ctrl_g_toggles_lobby_and_slash_shop_opens_shop() {
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

    // The Shop has no chord: /shop in the composer opens it, Esc closes.
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
async fn global_w_keeps_old_bonsai_without_dynamic_selection() {
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
    wait_for_render_contains(&mut app, " Bonsai Care ").await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains(" Dynamic Bonsai ") && !frame.contains("Branch Graph"),
        "expected w to keep the old Bonsai care modal; frame={frame:?}"
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
            !frame.contains(" Dynamic Bonsai ") && !frame.contains("Branch Graph"),
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

    app.handle_input(b"\x1b[C");
    wait_for_render_contains(&mut app, "Cursor     1,0").await;

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

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Artboard Help").await;
    assert!(
        render_plain(&mut app).contains("Artboard Help"),
        "? in view mode should open local Artboard help"
    );
    app.handle_input(b"q");

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
async fn clicking_the_mentions_hud_text_opens_mentions() {
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
    wait_for_render_contains(&mut app, "unread mention").await;

    // The HUD sits on the top border row as `1 unread mention | N chips`.
    // Border glyphs are multi-byte, so translate byte offsets into display
    // columns by char count (every glyph on this row is single-width).
    let frame = render_plain(&mut app);
    let top_row = frame.lines().next().expect("top border row").to_string();
    let char_col = |needle: &str| {
        let byte = top_row.find(needle).expect("needle on the top border");
        top_row[..byte].chars().count()
    };
    let mentions_col = char_col("unread mention");
    let chips_col = char_col("chips");

    // Clicking the chips text, right of the mentions text, must not open
    // Mentions. SGR mouse coords are 1-indexed.
    app.handle_input(format!("\x1b[<0;{};1M", chips_col + 1).as_bytes());
    assert_render_not_contains_for(&mut app, "mentioned you in", Duration::from_millis(120)).await;

    // Clicking inside the mentions text opens the Mentions view.
    app.handle_input(format!("\x1b[<0;{};1M", mentions_col + 1).as_bytes());
    wait_for_render_contains(&mut app, "mentioned you in").await;
}

#[tokio::test]
async fn forced_tour_gates_input_until_each_named_key() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "tour-gate-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "tour-gate-flow-it");

    // Arm the tour the way a first-ever session does: land in the tavern
    // with the walkthrough pending.
    app.set_screen(Screen::Clubhouse);
    app.clubhouse.tutorial = Tutorial::Pending;
    app.clubhouse.enter_screen();
    assert_eq!(app.clubhouse.tutorial, Tutorial::Welcome);

    // The gate swallows everything but the named key: no page hopping, no
    // Tab, no help modal, no reserved chords, no composer.
    for bytes in [&b"2"[..], b"\t", b"?", b"\x0f", b"\x07", b"i"] {
        app.handle_input(bytes);
    }
    assert_eq!(app.screen, Screen::Clubhouse);
    assert!(!app.show_help);
    assert_eq!(app.clubhouse.tutorial, Tutorial::Welcome);

    // The named keys walk the route in order, nothing else moves it. The
    // two Enter interludes (the music, the lobby) stay on their page.
    for (bytes, screen) in [
        (&b"1"[..], Screen::Dashboard),
        (b"\r", Screen::Dashboard),
        (b"2", Screen::Arcade),
        (b"\r", Screen::Arcade),
        (b"3", Screen::Games),
        (b"4", Screen::Artboard),
        (b"5", Screen::Profiles),
        (b"6", Screen::Leaderboard),
        (b"0", Screen::Clubhouse),
    ] {
        app.handle_input(bytes);
        assert_eq!(app.screen, screen);
    }
    assert_eq!(app.clubhouse.tutorial, Tutorial::Homecoming);

    // Enter settles in, and input is free again.
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::Done);
    app.handle_input(b"2");
    assert_eq!(app.screen, Screen::Arcade);
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

/// Ctrl+L is the escape hatch for a terminal left damaged by something outside
/// late.sh. It has to re-emit every cell: the failure mode worth pinning is a
/// repaint that clears the screen and then sends an empty diff, leaving the
/// user staring at a blank terminal that is worse than the damage.
#[tokio::test]
async fn ctrl_l_repaints_the_whole_screen_rather_than_blanking_it() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-l-repaint").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "ctrl-l-repaint-flow-it");

    wait_for_render_contains(&mut app, "lounge").await;

    // Let the screen settle: with nothing changed, a frame is only a small diff.
    let _ = app.render().expect("render");
    let settled = strip_ansi(&String::from_utf8_lossy(&app.render().expect("render")));

    app.handle_input(b"\x0c");
    let repainted = strip_ansi(&String::from_utf8_lossy(&app.render().expect("render")));

    assert!(
        repainted.contains("lounge"),
        "expected Ctrl+L to re-emit the whole screen; repainted={repainted:?}"
    );
    assert!(
        repainted.len() > settled.len(),
        "expected the Ctrl+L frame to carry more than the settled diff; \
         settled={} bytes, repainted={} bytes",
        settled.len(),
        repainted.len()
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
