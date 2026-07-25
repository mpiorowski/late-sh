//! App input integration tests against a real ephemeral DB.

use crate::authz::Permissions;
use crate::test_helpers::{
    assert_render_not_contains_for, chat_compose_app, make_app, make_app_with_chat_service,
    make_app_with_permissions, new_test_db, render_plain, wait_for_render_contains, wait_until,
    with_session_key,
};
use late_core::models::user::{RightSidebarMode, RoomListMode};
use late_core::models::user_ssh_key::{KeyLayout, UserSshKey};
use late_core::models::{
    chat_message::{ChatMessage, ChatMessageParams},
    chat_message_reaction::ChatMessageReaction,
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
    user::User,
};
use late_core::test_utils::create_test_user;
use rstest::rstest;
use tokio::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn dashboard_chat_compose_blocks_quit_shortcut() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "popup-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "popup-flow-it");

    // Wait until the async room snapshot has landed: `lounge` only renders
    // once `drain_snapshot` populates the visible Home chat rail.
    wait_for_render_contains(&mut app, "lounge").await;
    wait_for_render_contains(&mut app, " Home ").await;

    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Compose (Enter send").await;

    app.handle_input(b"q$$$");
    wait_for_render_contains(&mut app, "$$$").await;
    wait_for_render_contains(&mut app, " Home ").await;
}

#[tokio::test]
async fn q_opens_quit_confirm_and_escape_dismisses_it() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "quit-confirm-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "quit-confirm-flow-it");

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
}

#[tokio::test]
async fn ctrl_c_does_not_quit_the_app() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-c-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "ctrl-c-flow-it");

    app.handle_input(b"\x03");
    tokio::time::sleep(Duration::from_millis(60)).await;

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
}

#[tokio::test]
async fn account_delete_confirmation_rejects_wrong_username_in_dialog() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "account-delete-flow").await;
    let mut app = make_app(test_db.db.clone(), user.id, "account-delete-flow-it");

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
async fn screen_number_keys_switch_between_pages_including_pinstar() {
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
    wait_for_render_contains(&mut app, " Directory ").await;

    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Home ").await;
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
    wait_for_render_contains(&mut app, "Directory").await;

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
async fn tab_cycles_screens_forward_through_all_including_pinstar() {
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
    wait_for_render_contains(&mut app, " Directory ").await;

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
async fn global_ctrl_g_opens_hub_on_dashboard() {
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

    // Ctrl+G opens hub modal
    app.handle_input(b"\x07");
    wait_for_render_contains(&mut app, "Leaderboard").await;

    // Esc to close hub
    app.handle_input(b"\x1b");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Leaderboard"),
        "expected Esc to close hub; frame={frame:?}"
    );
}

#[tokio::test]
async fn legacy_terminal_help_byte_no_longer_opens_standalone_faq_on_dashboard() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-l-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "ctrl-l-flow-it");
    wait_for_render_contains(&mut app, " Home ").await;

    // The old terminal FAQ byte no longer opens a standalone modal; those topics now live in the guide.
    app.handle_input(b"\x0c");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Why copy sometimes silently fails"),
        "expected old FAQ byte not to open standalone FAQ; frame={frame:?}"
    );

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "CLI YouTube").await;
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
    for (label, permissions) in [
        ("regular", Permissions::default()),
        ("admin", Permissions::new(true, false)),
        ("moderator", Permissions::new(false, true)),
    ] {
        let test_db = new_test_db().await;
        let user = create_test_user(&test_db.db, &format!("ctrl-b-{label}-it")).await;
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
            &format!("ctrl-b-{label}-flow-it"),
            permissions,
        );
        wait_for_render_contains(&mut app, " Home ").await;

        app.handle_input(b"\x02");
        tokio::time::sleep(Duration::from_millis(60)).await;
        let frame = render_plain(&mut app);
        assert!(
            !frame.contains(" Dynamic Bonsai ") && !frame.contains("Branch Graph"),
            "expected Ctrl+B to stay inert for {label}; frame={frame:?}"
        );
    }
}

#[tokio::test]
async fn question_mark_opens_guide_on_dashboard() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-p-guide-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "ctrl-p-guide-flow-it");
    wait_for_render_contains(&mut app, " Home ").await;

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Install `late` / Pair Browser").await;
    wait_for_render_contains(&mut app, "?/Esc/q close").await;

    app.handle_input(b"?");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Install `late` / Pair Browser"),
        "expected ? to close guide; frame={frame:?}"
    );
}

#[tokio::test]
async fn question_mark_opens_lateania_guide_on_lateania_screen() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "lateania-guide-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "lateania-guide-flow-it");
    wait_for_render_contains(&mut app, " Home ").await;

    // Lateania has no top-level key now: open the Games hub and launch the
    // selected (default) Lateania card.
    app.handle_input(b"3");
    wait_for_render_contains(&mut app, " Games ").await;
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, " Lateania ").await;

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Lateania is the persistent BBS-style world").await;

    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Install `late` / Pair Browser"),
        "expected Lateania guide tab instead of Pair tab; frame={frame:?}"
    );
}

#[tokio::test]
async fn artboard_view_mode_allows_cursor_movement_and_screen_hotkeys() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-view-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-view-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Mode       view").await;
    wait_for_render_contains(&mut app, "Cursor     0,0").await;

    app.handle_input(b"\x1b[C");
    wait_for_render_contains(&mut app, "Cursor     1,0").await;

    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Home ").await;
}

#[tokio::test]
async fn artboard_view_mode_click_enters_active_mode_at_clicked_canvas_cell() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-click-enter-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-click-enter-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Mode       view").await;
    wait_for_render_contains(&mut app, "Cursor     0,0").await;

    app.handle_input(b"\x1b[<0;10;5M");
    wait_for_render_contains(&mut app, "Mode       active").await;
    wait_for_render_contains(&mut app, "Cursor     8,3").await;
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
async fn active_artboard_blocks_screen_number_hotkeys_until_escape() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-active-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-active-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Mode       active").await;

    app.handle_input(b"1");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Mode       active"),
        "expected active artboard mode to keep focus after numeric hotkeys; frame={frame:?}"
    );
    assert!(
        !frame.contains(" Home "),
        "expected active artboard mode to block screen switching; frame={frame:?}"
    );

    app.handle_input(b"\x1b");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Home ").await;
}

#[tokio::test]
async fn active_artboard_ctrl_c_copies_without_quitting() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-ctrl-c-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-ctrl-c-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Mode       active").await;

    app.handle_input(b"\x03");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Mode       swatch"),
        "expected Ctrl+C to copy into the primary swatch and stay inside active artboard; frame={frame:?}"
    );
    assert!(
        !frame.contains(" Quit? "),
        "expected Ctrl+C to avoid the global quit flow; frame={frame:?}"
    );
}

#[tokio::test]
async fn artboard_help_modal_tab_switches_help_tabs_instead_of_pages() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-help-tab-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-help-tab-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"\x10");
    wait_for_render_contains(&mut app, "Two modes").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, "Draw / erase").await;

    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Artboard Help"),
        "expected Artboard help Tab to stay on Artboard instead of switching page; frame={frame:?}"
    );
}

#[tokio::test]
async fn artboard_view_mode_question_mark_opens_local_help() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-view-help-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-view-help-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Two modes").await;

    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Artboard Help"),
        "expected ? on Artboard view mode to open local help, not the global guide; frame={frame:?}"
    );
}

#[tokio::test]
async fn active_artboard_question_mark_types_into_canvas_instead_of_opening_help() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-questionmark-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-questionmark-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Mode       view").await;
    wait_for_render_contains(&mut app, "Cursor     0,0").await;

    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Mode       active").await;

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Cursor     1,0").await;

    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Mode       active"),
        "expected ? to stay inside active artboard mode; frame={frame:?}"
    );
    assert!(
        !frame.contains("Tab/S+Tab"),
        "expected ? in active artboard mode to avoid the global guide; frame={frame:?}"
    );
}

#[tokio::test]
async fn dashboard_chat_compose_treats_screen_hotkeys_as_text() {
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

    // See `dashboard_chat_compose_blocks_quit_shortcut`: wait for the Home
    // chat rail so the room snapshot has populated `lounge_room_id`.
    wait_for_render_contains(&mut app, "lounge").await;
    wait_for_render_contains(&mut app, " Home ").await;

    app.handle_input(b"i3abc");

    wait_for_render_contains(&mut app, " Home ").await;
    wait_for_render_contains(&mut app, "3abc").await;
}

#[tokio::test]
async fn chat_compose_treats_screen_hotkeys_as_text() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "chat-compose-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "chat-compose-flow-it");

    wait_for_render_contains(&mut app, "lounge").await;

    app.handle_input(b"i2hey");
    wait_for_render_contains(&mut app, "2hey").await;
    wait_for_render_contains(&mut app, "Compose (Enter send").await;

    // Real terminals send CR (0x0D) for Enter in raw mode. Bare LF (0x0A) is
    // Ctrl+J and is aliased to "insert newline in chat composer", so we'd
    // end up composing "2hey\n" instead of submitting.
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Compose (press i)").await;
}

#[rstest]
#[case::cyrillic("cyrillic", "тест")]
#[case::han("han", "漢字")]
#[case::latin_diacritic("accented", "café")]
#[case::greek("greek", "αβγ")]
#[tokio::test]
async fn chat_compose_accepts_non_ascii_typing(#[case] label: &str, #[case] input: &str) {
    let (_db, mut app) = chat_compose_app(&format!("utf8-{label}")).await;
    app.handle_input(input.as_bytes());
    wait_for_render_contains(&mut app, input).await;
}

#[tokio::test]
async fn split_read_alt_backspace_deletes_word_without_wedging_parser() {
    let (_db, mut app) = chat_compose_app("alt-backspace-split").await;

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
async fn chat_reaction_leader_uses_digits_without_switching_screens() {
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
    wait_for_render_contains(&mut app, "reaction target").await;

    app.handle_input(b"j");
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
async fn chat_reaction_leader_persists_extended_reaction_digits() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-react-extended-viewer").await;
    let author = create_test_user(&test_db.db, "f-react-extended-author").await;
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
            body: "extended reaction target".to_string(),
        },
    )
    .await
    .expect("create message");

    let mut app = make_app(test_db.db.clone(), viewer.id, "f-react-extended-flow-it");
    app.resize(160, 32).expect("resize test terminal");
    wait_for_render_contains(&mut app, "extended reaction target").await;

    app.handle_input(b"j");
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "1 👍").await;
    app.handle_input(b"5");

    wait_for_render_contains(&mut app, " Home ").await;
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
    let plain = render_plain(&mut app);
    assert!(
        !plain.contains("1 👍"),
        "reaction picker should be dismissed under owner modal: {plain:?}"
    );

    app.handle_input(b"\r");
    assert_render_not_contains_for(&mut app, " Reactions ", Duration::from_millis(250)).await;
    let plain = render_plain(&mut app);
    assert!(
        !plain.contains("1 👍"),
        "reaction picker should stay dismissed after Enter closes modal: {plain:?}"
    );

    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "1 👍").await;
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, " Reactions ").await;
    app.handle_input(b"f");
    assert_render_not_contains_for(&mut app, " Reactions ", Duration::from_millis(250)).await;

    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "1 👍").await;
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, " Reactions ").await;
    app.handle_input(b"\x1b");
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_render_not_contains_for(&mut app, " Reactions ", Duration::from_millis(250)).await;
}

#[tokio::test]
async fn chat_reaction_leader_cancels_and_consumes_non_digit_input() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-cancel-viewer").await;
    let author = create_test_user(&test_db.db, "f-cancel-author").await;
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
            body: "cancel target".to_string(),
        },
    )
    .await
    .expect("create message");

    let mut app = make_app(test_db.db.clone(), viewer.id, "f-cancel-flow-it");
    wait_for_render_contains(&mut app, "cancel target").await;

    app.handle_input(b"j");
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "1 👍").await;

    app.handle_input(b"r");
    assert_render_not_contains_for(
        &mut app,
        "Reply to @f-cancel-author",
        Duration::from_millis(250),
    )
    .await;

    let plain = render_plain(&mut app);
    assert!(!plain.contains("1 👍"), "picker should close: {plain:?}");
    assert!(
        plain.contains("cancel target"),
        "message should remain selected: {plain:?}"
    );
    assert!(
        ChatMessageReaction::get_by_user_and_message(&client, message.id, viewer.id)
            .await
            .expect("load reaction")
            .is_none(),
        "non-digit input should not react",
    );
}

#[tokio::test]
async fn help_command_renders_chat_feedback_without_persisting_message() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "help-notice-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "help-notice-flow-it");

    wait_for_render_contains(&mut app, "lounge").await;

    app.handle_input(b"i/binds\r");
    wait_for_render_contains(&mut app, " Guide ").await;
    wait_for_render_contains(&mut app, " Chat ").await;
    wait_for_render_contains(&mut app, "/settings").await;

    let messages = ChatMessage::list_recent(&client, lounge.id, 20)
        .await
        .expect("list recent messages");
    assert!(messages.is_empty(), "expected /binds to stay client-side");
}

#[tokio::test]
async fn members_command_shows_room_members_without_persisting_message() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "list-flow-viewer").await;
    let target = create_test_user(&test_db.db, "list-flow-target").await;
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

    let mut app = make_app(test_db.db.clone(), viewer.id, "list-room-members-flow-it");

    wait_for_render_contains(&mut app, "lounge").await;
    wait_for_render_contains(&mut app, "side").await;

    app.handle_input(b"llll");

    app.handle_input(b"i/members\r");
    wait_for_render_contains(&mut app, "#side Members").await;
    wait_for_render_contains(&mut app, "@list-flow-viewer").await;
    wait_for_render_contains(&mut app, "@list-flow-target").await;

    let messages = ChatMessage::list_recent(&client, private_room.id, 20)
        .await
        .expect("list recent messages");
    assert!(messages.is_empty(), "expected /members to stay client-side");
}

#[tokio::test]
async fn exit_command_opens_quit_confirm_and_stays_client_side() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "exit-command-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join user to lounge");

    let mut app = make_app(test_db.db.clone(), user.id, "exit-command-flow-it");

    wait_for_render_contains(&mut app, "lounge").await;

    app.handle_input(b"i/exit\r");
    wait_for_render_contains(&mut app, " Quit? ").await;

    let messages = ChatMessage::list_recent(&client, lounge.id, 20)
        .await
        .expect("list recent messages");
    assert!(messages.is_empty(), "expected /exit to stay client-side");
}

#[tokio::test]
async fn bare_mod_command_opens_moderation_modal() {
    let (_test_db, mut app) = chat_compose_app("mod-command-open").await;

    app.handle_input(b"/mod\r");

    wait_for_render_contains(&mut app, " Moderation ").await;
    wait_for_render_contains(&mut app, "access denied: moderator or admin only").await;
}

#[tokio::test]
async fn prefixed_mod_command_in_chat_points_to_modal() {
    let (_test_db, mut app) = chat_compose_app("mod-command-chat-reject").await;

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
async fn unrelated_settings_edits_do_not_republish_this_device_rails() {
    // Regression: the rail rows are per device, but `save()` writes the whole
    // draft to the account and fires from every tweak. If the device layout
    // lived in that draft, changing the theme on a phone would push the phone's
    // rails onto the account, and every key with no layout of its own (the
    // desktop) would inherit them.
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "rails-leak-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "rails-leak-flow-it");
    wait_for_render_contains(&mut app, "sort/fold").await;

    // Hide the room rail on this device only.
    app.handle_input(b"\\");
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("sort/fold"),
        "expected the rail hidden for this device; frame={frame:?}"
    );

    // Now touch something unrelated: Ctrl+O, Tab to the Tweaks tab, and flip
    // the first row (Background color). The save banner marks the write landing.
    app.handle_input(b"\x0f");
    // Wait for the draft to hydrate from the profile snapshot before moving:
    // that hydration resets the modal to its first tab (`open_from_profile`).
    wait_for_render_contains(&mut app, "rails-leak-it").await;
    app.handle_input(b"\t\t\t");
    wait_for_render_contains(&mut app, "Background color").await;
    // Enter toggles the selected row (Background color, the first one), which
    // runs the same `save()` every other settings edit runs.
    app.handle_input(b"\r");
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

    // And the device's own choice survived the settings round trip.
    app.handle_input(b"\x1b");
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("sort/fold"),
        "expected the device rail to stay hidden; frame={frame:?}"
    );
}

#[tokio::test]
async fn cycling_rails_persists_onto_the_authenticating_key() {
    // End-to-end for the write path: keystroke -> App -> ProfileService -> the
    // `user_ssh_keys` row for the key this session authenticated with, and not
    // onto the account's other keys.
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
    app.handle_input(b"\\");

    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move {
                let client = db.get().await.expect("db client");
                UserSshKey::layout_for(&client, user.id, "SHA256:phone")
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
        UserSshKey::layout_for(&client, user.id, "SHA256:desktop")
            .await
            .expect("desktop layout"),
        None,
        "the account's other device must keep following the account default"
    );
}

#[tokio::test]
async fn rules_command_shows_every_line_in_an_overlay() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "rules-flow-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer to lounge");

    let room = ChatRoom::create_private_room(&client, "parlour", viewer.id)
        .await
        .expect("create room");
    ChatRoomMember::join(&client, room.id, viewer.id)
        .await
        .expect("join viewer to parlour");
    ChatRoom::set_topic_and_rules(
        &client,
        room.id,
        Some("cards and tea"),
        Some("be kind\nno spoilers\ntake the bins out"),
    )
    .await
    .expect("set rules");

    let mut app = make_app(test_db.db.clone(), viewer.id, "rules-command-flow-it");
    wait_for_render_contains(&mut app, "lounge").await;
    wait_for_render_contains(&mut app, "parlour").await;
    app.handle_input(b"llll");

    app.handle_input(b"i/rules\r");
    // Every line survives, which a one-line banner could not do.
    wait_for_render_contains(&mut app, "#parlour rules").await;
    wait_for_render_contains(&mut app, "be kind").await;
    wait_for_render_contains(&mut app, "no spoilers").await;
    wait_for_render_contains(&mut app, "take the bins out").await;

    let messages = ChatMessage::list_recent(&client, room.id, 20)
        .await
        .expect("list recent messages");
    assert!(messages.is_empty(), "expected /rules to stay client-side");
}
