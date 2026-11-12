//! App input integration tests against a real ephemeral DB.

/// The retired stock `zen` page (`RiceLayout::default`, the tests'
/// fixture: the bonsai and the reef over a chat, a rail of clock, pet,
/// lobby, live and music) saved on the device as a layout named `zen` and
/// loaded the way `/layout zen` loads it.
use crate::app::zen::state::ChatRoomBinding;

fn load_zen_fixture(app: &mut crate::app::state::App) {
    use crate::app::zen::{presets::LayoutCommand, state::RiceLayout};
    if app.home_layouts.index_of("zen").is_none() {
        let page = app.home_layouts.current();
        app.home_layouts
            .save_new("zen", RiceLayout::default())
            .expect("room for the zen fixture");
        app.home_layouts.switch(page);
    }
    crate::app::zen::input::run_layout_command(app, LayoutCommand::Load("zen".to_string()));
}

#[tokio::test]
async fn artboard_disclaimer_hides_content_and_only_allows_the_current_visit() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "art-prompt-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "art-prompt-flow-it");
    app.resize(80, 24).unwrap();
    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Artboard may contain NSFW content").await;
    assert!(
        app.dartboard_state.is_none(),
        "no board seat is taken before consent"
    );
    let frame = render_plain(&mut app);
    assert!(frame.contains("Always View"));
    assert!(frame.contains("⁽ᵘˢᵘᵃˡˡʸ ᵇᵒᵒᵇⁱᵉˢ⁾"));
    assert!(!frame.contains("GALLERY"));
    assert!(!frame.contains("Mode       view"));
    assert!(!frame.contains("rail j/k"));
    app.handle_input(b"i\r ?");
    app.handle_input(b"\x1b[200~private drawing\x1b[201~");
    app.handle_input(b"\x1b[<0;3;6M");
    assert_eq!(app.screen, Screen::Artboard);
    assert!(!app.artboard_interacting);
    assert!(app.artboard_disclaimer_visible());
    assert!(app.dartboard_state.is_none());

    // Quit stays global on the prompt; cancelling it grants nothing.
    app.handle_input(b"q");
    assert!(app.show_quit_confirm);
    assert!(!app.artboard_content_accepted);
    app.handle_input(b"5");
    assert_eq!(app.screen, Screen::Artboard, "quit dialog keeps priority");
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.show_quit_confirm, "quit confirm").await;
    assert!(app.artboard_disclaimer_visible());
    assert!(app.dartboard_state.is_none());

    app.handle_input(b"V");
    assert!(app.dartboard_state.is_some(), "consent takes the seat");
    wait_for_render_contains(&mut app, "Mode       view").await;
    assert!(
        !app.artboard_interacting,
        "V must not also activate editing"
    );
    app.handle_input(b"i");
    app.handle_input(b"\x1b[200~PRIVATEART\x1b[201~\r");
    wait_for_render_contains(&mut app, "PRIVATEART").await;
    app.set_screen(Screen::Dashboard);
    // Tab and the top-bar mouse shortcut enter through the same prompt.
    app.set_screen(Screen::Games);
    app.handle_input(b"\t");
    assert!(app.artboard_disclaimer_visible());
    assert!(!render_plain(&mut app).contains("PRIVATEART"));
    app.handle_input(b"b");
    assert_eq!(app.screen, Screen::Dashboard);
    assert!(app.dartboard_state.is_none());
    assert!(app.profile_state.profile().artboard_disclaimer);
    app.handle_input(b"\x1b[<0;21;1M");
    assert!(app.artboard_disclaimer_visible());
    app.handle_input(b"v");
    wait_for_render_contains(&mut app, "PRIVATEART").await;
    app.handle_input(b"4");
    assert!(
        !app.artboard_disclaimer_visible(),
        "same-screen navigation keeps consent"
    );
    app.set_screen(Screen::Dashboard);
    app.handle_input(b"4\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| app.screen == Screen::Dashboard,
        "art disclaimer",
    )
    .await;
}

#[tokio::test]
async fn artboard_disclaimer_allows_numbered_page_shortcuts_without_consent() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "art-pages-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "art-pages-flow-it");
    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Artboard may contain NSFW content").await;

    app.handle_input(b"4");
    assert!(app.artboard_disclaimer_visible());
    assert!(!app.artboard_content_accepted);
    assert!(app.dartboard_state.is_none());

    for (key, screen) in [
        (b'1', Screen::Dashboard),
        (b'2', Screen::Arcade),
        (b'3', Screen::Games),
        (b'5', Screen::Profiles),
        (b'6', Screen::Leaderboard),
        (b'7', Screen::Calendars),
    ] {
        app.handle_input(&[key]);
        assert_eq!(app.screen, screen, "shortcut {}", key as char);
        assert!(!app.artboard_content_accepted);
        assert!(app.dartboard_state.is_none());
        assert!(app.artboard_disclaimer_enabled());

        app.handle_input(b"4");
        assert!(app.artboard_disclaimer_visible());
        assert!(app.dartboard_state.is_none());
    }

    let client = test_db.db.get().await.unwrap();
    let stored = User::get(&client, user.id).await.unwrap().unwrap();
    assert_eq!(stored.settings, user.settings);
}

#[tokio::test]
async fn artboard_disclaimer_is_per_device_and_settings_can_reenable_it() {
    use crate::app::{
        common::primitives::Screen,
        settings_modal::{
            mouse::Target,
            state::{Tab, TweakRow},
        },
    };
    use late_core::models::user::extract_artboard_disclaimer;
    use late_core::models::user_ssh_key::extract_key_artboard_disclaimer;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "art-device-it").await;
    let client = test_db.db.get().await.unwrap();
    // One account on two machines, as `auth_publickey` would have recorded them.
    UserSshKey::ensure(&client, user.id, "SHA256:home")
        .await
        .unwrap();
    UserSshKey::ensure(&client, user.id, "SHA256:work")
        .await
        .unwrap();
    let user_id = user.id;
    let stored_key = |fingerprint: &'static str| {
        let db = test_db.db.clone();
        async move {
            let client = db.get().await.unwrap();
            let key = UserSshKey::find_by_fingerprint(&client, user_id, fingerprint)
                .await
                .unwrap()
                .unwrap();
            extract_key_artboard_disclaimer(&key.settings)
        }
    };

    // Always View at home, before the profile has even arrived: it lands on
    // the home key and nowhere else.
    let mut home = with_session_key(
        make_app(test_db.db.clone(), user.id, "art-device-home-it"),
        "SHA256:home",
    );
    home.handle_input(b"4A");
    assert!(!home.artboard_disclaimer_visible());
    wait_for_render_contains(&mut home, "Mode       view").await;
    wait_until(
        || {
            let stored = stored_key("SHA256:home");
            async move { stored.await == Some(false) }
        },
        "Always View to reach the home key",
    )
    .await;
    assert_eq!(stored_key("SHA256:work").await, None);
    let stored = User::get(&client, user.id).await.unwrap().unwrap();
    assert_eq!(stored.username, user.username);
    assert!(
        extract_artboard_disclaimer(&stored.settings),
        "the account default stays on for every other device"
    );

    // Work inherits the account default and still asks.
    let mut work = with_session_key(
        make_app(test_db.db.clone(), user.id, "art-device-work-it"),
        "SHA256:work",
    );
    work.handle_input(b"4");
    assert!(work.artboard_disclaimer_visible());
    assert!(work.dartboard_state.is_none());
    work.handle_input(b"B");
    assert_eq!(work.screen, Screen::Dashboard);

    // The home Tweaks row shows this device's choice, not the account's, and
    // re-enabling it hides the current visit again.
    home.handle_input(b"\x0f");
    wait_for_render_contains(&mut home, &user.username).await;
    home.settings_modal_state.select_tab(Tab::Tweaks);
    home.settings_modal_state
        .select_mouse_target(Target::Tweak(TweakRow::ArtboardDisclaimer));
    assert_eq!(
        home.settings_modal_state.selected_tweak_row(),
        TweakRow::ArtboardDisclaimer
    );
    assert!(!home.settings_modal_state.device_artboard_disclaimer());
    assert!(home.settings_modal_state.draft().artboard_disclaimer);
    wait_for_render_contains(&mut home, "Artboard content disclaimer").await;
    home.handle_input(b"\r");
    assert!(home.settings_modal_state.device_artboard_disclaimer());
    assert!(home.artboard_disclaimer_visible());
    home.handle_input(b"\x1b");
    wait_for_esc_effect(&mut home, |app| !app.show_settings, "settings").await;
    assert!(!render_plain(&mut home).contains("GALLERY"));
    wait_until(
        || {
            let stored = stored_key("SHA256:home");
            async move { stored.await == Some(true) }
        },
        "the re-enabled disclaimer to reach the home key",
    )
    .await;
    home.handle_input(b"B");
    assert_eq!(home.screen, Screen::Dashboard);
}

#[tokio::test]
async fn artboard_disclaimer_tour_choice_keeps_account_settings() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "tour-choice-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "tour-choice-flow-it");
    app.resize(80, 24).unwrap();
    app.clubhouse.tutorial = Tutorial::VisitArtboard;
    app.set_screen(Screen::Artboard);
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("[S] show · [Enter] skip: the people"),
        "frame={frame:?}"
    );
    assert!(!frame.contains("Artboard may contain NSFW content"));
    assert!(!frame.contains("Always View"));
    assert!(!frame.contains("Mode       view"));

    // The standalone dialog's keys mean nothing here: the stop keeps the art hidden.
    app.handle_input(b"vab");
    assert_eq!(app.screen, Screen::Artboard);
    assert!(!app.artboard_content_accepted);
    assert!(app.dartboard_state.is_none(), "no seat before show");

    // Show grants this visit only; the account tweak is untouched.
    app.handle_input(b"s");
    assert!(app.artboard_content_accepted);
    assert!(app.dartboard_state.is_some());
    assert!(app.profile_state.profile().artboard_disclaimer);
    wait_for_render_contains(&mut app, "Mode       view").await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("[Enter] next: the people"),
        "frame={frame:?}"
    );
    assert!(!frame.contains("Artboard may contain NSFW content"));
    let client = test_db.db.get().await.unwrap();
    let stored = User::get(&client, user.id).await.unwrap().unwrap();
    assert_eq!(stored.settings, user.settings);
    assert_eq!(stored.username, user.username);

    app.clubhouse.tutorial = Tutorial::Off;
    app.set_screen(Screen::Dashboard);
    app.handle_input(b"4");
    assert!(app.artboard_disclaimer_visible());
    assert!(render_plain(&mut app).contains("Always View"));
}

/// Someone who turned the content disclaimer off already sees the art: the
/// stop takes its plain shape over the live board, `s` is just another
/// swallowed key, and Enter walks on.
#[tokio::test]
async fn artboard_tour_stop_shows_the_board_when_the_disclaimer_is_off() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "tour-view-always-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "tour-view-always-flow-it");
    app.resize(80, 24).unwrap();
    app.set_device_artboard_disclaimer(false);
    app.clubhouse.tutorial = Tutorial::VisitArtboard;
    app.set_screen(Screen::Artboard);

    assert!(!app.artboard_disclaimer_visible());
    assert!(app.dartboard_state.is_some(), "seated on landing");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("[Enter] next: the people"),
        "frame={frame:?}"
    );
    assert!(!frame.contains("[S] show"), "frame={frame:?}");

    app.handle_input(b"s");
    assert_eq!(app.screen, Screen::Artboard);
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitArtboard);
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Profiles);
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitMap);
}

#[tokio::test]
async fn artboard_disclaimer_allows_clicking_topbar_page_numbers_without_consent() {
    use crate::app::common::primitives::Screen;
    use late_core::models::user::InteractionMode;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "art-topbar-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "art-topbar-flow-it");
    // Topbar digits sit at 0-based columns 14, 16, ... on row 0; SGR is 1-based.
    for (column, screen) in [
        (12, Screen::Clubhouse),
        (14, Screen::Dashboard),
        (16, Screen::Arcade),
        (18, Screen::Games),
        (22, Screen::Profiles),
        (24, Screen::Leaderboard),
        (26, Screen::Calendars),
    ] {
        app.set_screen(Screen::Artboard);
        render_plain(&mut app);
        assert!(app.artboard_disclaimer_visible());
        let click = format!("\x1b[<0;{};1M", column + 1);

        app.interaction_mode = InteractionMode::Keyboard;
        app.handle_input(click.as_bytes());
        assert_eq!(app.screen, Screen::Artboard, "keyboard-only ignores clicks");

        app.interaction_mode = InteractionMode::Hybrid;
        app.handle_input(click.as_bytes());
        assert_eq!(app.screen, screen, "column {column}");
        assert!(!app.artboard_content_accepted);
        assert!(app.dartboard_state.is_none());
        assert!(app.artboard_disclaimer_enabled());
    }
}

#[tokio::test]
async fn artboard_disclaimer_lets_tab_and_shift_tab_cycle_pages_without_consent() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "art-tab-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "art-tab-flow-it");
    for (keys, screen) in [
        (&b"\t"[..], Screen::Profiles),
        (&b"\x1b[Z"[..], Screen::Games),
    ] {
        app.set_screen(Screen::Artboard);
        render_plain(&mut app);
        assert!(app.artboard_disclaimer_visible());

        app.handle_input(keys);
        assert_eq!(app.screen, screen, "keys {keys:?}");
        assert!(!app.artboard_content_accepted);
        assert!(app.dartboard_state.is_none());
        assert!(app.artboard_disclaimer_enabled());
    }
}

#[tokio::test]
async fn artboard_disclaimer_mouse_choices_respect_keyboard_only_mode() {
    use crate::app::common::primitives::Screen;
    use late_core::models::user::InteractionMode;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "art-mouse-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "art-mouse-flow-it");
    for index in [0, 2, 1] {
        app.set_screen(Screen::Artboard);
        render_plain(&mut app);
        let rect = app.artboard_disclaimer_choices.get()[index];
        let click = format!("\x1b[<0;{};{}M", rect.x + rect.width / 2 + 1, rect.y + 1);
        app.interaction_mode = InteractionMode::Keyboard;
        app.handle_input(click.as_bytes());
        assert!(app.artboard_disclaimer_visible());
        app.interaction_mode = InteractionMode::Hybrid;
        app.handle_input(click.replace('M', "m").as_bytes());
        assert!(app.artboard_disclaimer_visible(), "release cannot choose");
        app.handle_input(click.as_bytes());
        assert!(!app.artboard_disclaimer_visible());
        assert_eq!(
            app.screen,
            if index == 2 {
                Screen::Dashboard
            } else {
                Screen::Artboard
            }
        );
        if index == 1 {
            assert!(!app.artboard_disclaimer_enabled());
            assert!(
                app.profile_state.profile().artboard_disclaimer,
                "Always View is per device: the account default stays on"
            );
        }
        app.set_screen(Screen::Dashboard);
    }
}

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
async fn artboard_topbar_clicks_leave_framing_and_title_entry() {
    use crate::app::{artboard::gallery::state::HangFlow, common::primitives::Screen};
    use late_core::models::user::InteractionMode;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-topbar-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-topbar-flow-it");

    for naming in [false, true] {
        app.handle_input(b"4v");
        wait_for_render_contains(&mut app, "Mode       view").await;
        if naming {
            app.handle_input(b"i");
            app.handle_input(b"\x1b[200~##########\n##########\n##########\n##########\x1b[201~\r");
            app.handle_input(b"\x1b");
            wait_for_esc_effect(
                &mut app,
                |app| !app.dartboard_state.as_ref().unwrap().has_floating(),
                "dismiss floating paste",
            )
            .await;
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
        app.handle_input(b"4v");
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
    app.game_selection = late_core::models::leaderboard::ArcadeGame::Score(
        late_core::models::leaderboard::ScoreGame::TwentyFortyEight,
    );
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

use crate::authz::Permissions;
use crate::test_helpers::{
    assert_render_not_contains_for, chat_compose_app, make_app, make_app_in_world,
    make_app_with_chat_service, make_app_with_permissions, new_test_db, render_plain, strip_ansi,
    wait_for_app, wait_for_render_contains, wait_for_render_not_contains, wait_until,
    with_session_key,
};
use late_core::models::cyberspace_account::CyberspaceAccount;
use late_core::models::user_ssh_key::UserSshKey;
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
    let chips_index = app
        .leaderboard_page
        .boards()
        .iter()
        .position(|b| *b == Board::TopChips)
        .unwrap();
    app.leaderboard_page.select(chips_index);
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
        .find(|point| app.leaderboard_page.board_at(*point) == Some(chips_index + 1))
        .unwrap();
    let click = format!("\x1b[<0;{};{}M", board.x + 1, board.y + 1);
    app.handle_input(click.as_bytes());
    assert_eq!(app.leaderboard_page.selected_board(), Board::ArcadeWins);
    assert_eq!(app.leaderboard_page.scroll(), 0);
    app.render().unwrap();
    let rail_wheel = format!("\x1b[<65;{};{}M", board.x + 1, board.y + 1);
    app.handle_input(rail_wheel.as_bytes());
    assert_eq!(app.leaderboard_page.selected_board(), Board::TopChat);
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

    app.handle_input(b"4v");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"5");
    wait_for_render_contains(&mut app, " People ").await;

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
    wait_for_render_contains(&mut app, " People ").await;
    // The page opens on the map; the feed keys live one section on.
    app.handle_input(b"]");

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
    wait_for_render_contains(&mut app, " Calendars ").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " Leaderboards ").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " People ").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, "Artboard may contain NSFW content").await;
    app.handle_input(b"v");
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
    wait_for_render_contains(&mut app, "Artboard may contain NSFW content").await;
    app.handle_input(b"v");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, " People ").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, " Leaderboards ").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, " Calendars ").await;

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
    app.game_selection = late_core::models::leaderboard::ArcadeGame::Score(
        late_core::models::leaderboard::ScoreGame::TwentyFortyEight,
    );
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
        Screen::Realm,
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
async fn realm_keeps_the_originating_page_when_switching_games_or_visiting_home() {
    use crate::app::common::primitives::Screen;
    use crate::app::lobby::modal_input::return_screen_for_opening;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "realm-return-origin").await;
    let mut app = make_app(test_db.db.clone(), user.id, "realm-return-origin-it");

    app.set_screen(Screen::Profiles);
    app.realm.open_board(uuid::Uuid::new_v4(), Screen::Profiles);
    app.set_screen(Screen::Realm);
    assert_eq!(return_screen_for_opening(&app), Screen::Profiles);
    // Ctrl+E takes the board's player Home to arrange it; the backtick
    // brings them back to a board that still returns where it was opened.
    app.handle_input(b"\x05");
    assert_eq!(app.screen, Screen::Dashboard);
    assert!(app.home.arrange);
    // No tick here: the board is a fabricated one.
    flush_lone_escape(&mut app);
    assert!(!app.home.arrange);
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Realm);
    assert_eq!(return_screen_for_opening(&app), Screen::Profiles);
}

/// The realm board is a stop on the backtick cycle like a detached
/// roguelike: the key hops out with the board kept open, and from Home the
/// same key hops back onto it.
#[tokio::test]
async fn the_realm_board_is_a_backtick_stop() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "realm-hop").await;
    let mut app = make_app(test_db.db.clone(), user.id, "realm-hop-it");

    app.set_screen(Screen::Dashboard);
    app.realm.open_board(uuid::Uuid::new_v4(), Screen::Games);
    app.set_screen(Screen::Realm);

    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard, "a hop out lands on the base");
    assert!(app.realm.board.is_some(), "with the board kept open");

    app.handle_input(b"`");
    assert_eq!(
        app.screen,
        Screen::Realm,
        "and the next hop comes back to it"
    );
}

/// A realm board hopped away from and forgotten closes after the shared door
/// window, the same deadline the native remakes keep: never while it is the
/// open screen, and on-screen time does not count toward it.
#[tokio::test]
async fn the_idle_deadline_closes_a_forgotten_realm_board() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "realm-idle").await;
    let mut app = make_app(test_db.db.clone(), user.id, "realm-idle-it");

    app.set_screen(Screen::Dashboard);
    app.realm.open_board(uuid::Uuid::new_v4(), Screen::Games);
    app.set_screen(Screen::Realm);

    app.realm.force_board_idle_for_test();
    app.tick();
    assert!(
        app.realm.board.is_some(),
        "expected the open screen to shield the board from the close"
    );
    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard);
    app.tick();
    assert!(
        app.realm.board.is_some(),
        "expected on-screen time not to count toward the idle deadline"
    );

    app.realm.force_board_idle_for_test();
    assert!(
        app.tick(),
        "expected the closing tick to dirty the frame so the hub row clears"
    );
    assert!(
        app.realm.board.is_none(),
        "expected the idle deadline to close the board"
    );
    app.handle_input(b"`");
    assert_eq!(
        app.screen,
        Screen::Dashboard,
        "expected no hop once the board has idled out"
    );
}

/// The world's chat is put away when the board opens, so the map has the
/// screen, and the frame title names its key; F4 (not `c`) puts it up in
/// the rail's place and `i` types into the world's own room, and F4 again
/// hides it.
#[tokio::test]
async fn the_realm_chat_is_hidden_until_asked_for() {
    use crate::app::activity::lounge::{SYSTEM_FINGERPRINT, SYSTEM_USERNAME};
    use crate::app::common::primitives::Screen;
    use late_core::models::user::{User, UserParams};

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "realm-chat").await;
    let mut app = make_app(test_db.db.clone(), user.id, "realm-chat-it");
    {
        let client = test_db.db.get().await.expect("db");
        User::create(
            &client,
            UserParams {
                fingerprint: SYSTEM_FINGERPRINT.to_string(),
                username: SYSTEM_USERNAME.to_string(),
                settings: serde_json::json!({ "bot": true, "system": true }),
            },
        )
        .await
        .expect("create system user");
    }
    let game = app
        .realm
        .service()
        .open_season()
        .await
        .expect("open season")
        .expect("a world rises");
    let room_id = game.chat_room_id.expect("the world has its room");

    app.set_screen(Screen::Games);
    app.realm.open_board(game.id, Screen::Games);
    app.set_screen(Screen::Realm);
    wait_for_render_contains(&mut app, "Realm \u{b7} F4 chat").await;
    assert_eq!(app.realm.shown_chat_room_id(), None, "hidden by default");
    app.handle_input(b"c");
    assert_eq!(
        app.realm.shown_chat_room_id(),
        None,
        "c is not the chat key: F4 is, on every game"
    );

    app.handle_input(b"\x1bOS");
    wait_for_render_contains(&mut app, "No messages yet").await;
    wait_for_render_contains(&mut app, "F4 hide chat").await;
    wait_for_render_not_contains(&mut app, "Realm \u{b7} F4 chat").await;
    assert_eq!(app.realm.shown_chat_room_id(), Some(room_id));

    app.handle_input(b"i");
    assert!(app.chat.composing, "i opens the composer");
    assert_eq!(app.chat.composer_room_id(), Some(room_id));

    app.handle_input(b"\x1b");
    app.pending_escape_started_at = Some(std::time::Instant::now() - Duration::from_secs(1));
    crate::app::input::flush_pending_escape(&mut app);
    assert!(!app.chat.composing, "Esc puts the composer away");
    app.handle_input(b"\x1b[14~");
    wait_for_render_not_contains(&mut app, "No messages yet").await;
    assert_eq!(app.realm.shown_chat_room_id(), None, "F4 hides it again");
    assert_eq!(app.screen, Screen::Realm, "and the board stays up");
}

/// A lone Esc on the realm board peels a picked chat message first, the
/// daily board's shape: the selection goes, the board stays. Only the next
/// Esc puts the board away. Otherwise the selection would outlive the board
/// and take `j`/`k` the moment it is reopened.
#[tokio::test]
async fn a_lone_esc_lets_go_of_a_picked_realm_chat_message_before_closing_the_board() {
    use crate::app::activity::lounge::{SYSTEM_FINGERPRINT, SYSTEM_USERNAME};
    use crate::app::common::primitives::Screen;
    use late_core::models::chat_message::{ChatMessage, ChatMessageParams};
    use late_core::models::user::{User, UserParams};

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "realm-chat-esc").await;
    let mut app = make_app(test_db.db.clone(), user.id, "realm-chat-esc-it");
    {
        let client = test_db.db.get().await.expect("db");
        User::create(
            &client,
            UserParams {
                fingerprint: SYSTEM_FINGERPRINT.to_string(),
                username: SYSTEM_USERNAME.to_string(),
                settings: serde_json::json!({ "bot": true, "system": true }),
            },
        )
        .await
        .expect("create system user");
    }
    let game = app
        .realm
        .service()
        .open_season()
        .await
        .expect("open season")
        .expect("a world rises");
    let room_id = game.chat_room_id.expect("the world has its room");
    let message = {
        let client = test_db.db.get().await.expect("db");
        ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id,
                user_id: user.id,
                body: "the walls of season one".to_string(),
            },
        )
        .await
        .expect("create message")
    };

    app.set_screen(Screen::Games);
    app.realm.open_board(game.id, Screen::Games);
    app.set_screen(Screen::Realm);
    wait_for_render_contains(&mut app, "F4 chat").await;
    app.handle_input(b"\x1bOS");
    wait_for_render_contains(&mut app, "the walls of season one").await;
    assert!(
        app.chat.select_message_by_id_in_room(room_id, message.id),
        "a click picks the message"
    );

    app.handle_input(b"\x1b");
    app.pending_escape_started_at = Some(std::time::Instant::now() - Duration::from_secs(1));
    crate::app::input::flush_pending_escape(&mut app);
    assert_eq!(
        app.chat.selected_message_body_in_room(room_id),
        None,
        "Esc lets the message go"
    );
    assert_eq!(app.screen, Screen::Realm, "and the board stays up");
    assert!(app.realm.board.is_some());

    app.handle_input(b"\x1b");
    app.pending_escape_started_at = Some(std::time::Instant::now() - Duration::from_secs(1));
    crate::app::input::flush_pending_escape(&mut app);
    assert_eq!(
        app.screen,
        Screen::Games,
        "the next Esc puts the board away"
    );
    assert!(app.realm.board.is_none());
}

/// A lone Esc never reaches the realm board's own keys: it is held as a
/// pending escape and flushed later. It still has to put the colour picker
/// away before it puts the board away, and a picker left open when the board
/// closes does not come back with the next board.
#[tokio::test]
async fn a_lone_esc_puts_the_realm_colour_picker_away_first() {
    use crate::app::common::primitives::Screen;
    use crate::app::door::realm::state::RealmJoinDraft;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "realm-esc-picker").await;
    let mut app = make_app(test_db.db.clone(), user.id, "realm-esc-picker-it");
    let game_id = uuid::Uuid::new_v4();
    let picker = || RealmJoinDraft {
        game_id,
        game_name: "Season 1".to_string(),
        color: 0,
        taken: Vec::new(),
    };

    app.set_screen(Screen::Games);
    app.realm.open_board(game_id, Screen::Games);
    app.set_screen(Screen::Realm);
    app.realm.join_draft = Some(picker());

    app.handle_input(b"\x1b");
    app.pending_escape_started_at = Some(std::time::Instant::now() - Duration::from_secs(1));
    crate::app::input::flush_pending_escape(&mut app);
    assert!(app.realm.join_draft.is_none(), "the picker is put away");
    assert_eq!(app.screen, Screen::Realm, "and the board stays up");
    assert!(app.realm.board.is_some());

    app.realm.join_draft = Some(picker());
    crate::app::door::realm::input::close_board(&mut app);
    app.realm.open_board(game_id, Screen::Games);
    assert!(
        app.realm.join_draft.is_none(),
        "a closed board takes its picker with it"
    );
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
        (b"\x05", "Ctrl+E"),
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
        assert!(!app.home.arrange, "{name} arranges no Home");
    }

    app.handle_input(b"\r"); // Enter leaves edit mode and saves
    assert_eq!(app.settings_modal_state.draft().bio, "late night coder");
}

/// `/lobby` and `/guide` are the typed fallbacks for Ctrl+G and `?`, for
/// terminals that swallow the chords; `/layout 2` loads Home's second
/// layout, the same as `Ctrl+E 2`, the stock `street` on a fresh device.
#[tokio::test]
async fn slash_lobby_layout_and_guide_mirror_their_keys() {
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

    app.handle_input(b"i/layout 2\r");
    assert_eq!(app.screen, Screen::Dashboard);
    assert_eq!(app.home_layouts.current_name(), "street");
    assert!(!app.home.arrange);
    app.handle_input(b"\x05");
    app.handle_input(b"1");
    assert_eq!(app.home_layouts.current_name(), "home");

    // /edit is Ctrl+E: arrange mode, with the composer it was typed in gone,
    // so the next key is the mode's, and Esc leaves it.
    app.handle_input(b"i/edit\r");
    assert_eq!(app.screen, Screen::Dashboard);
    assert!(app.home.arrange, "/edit should enter arrange mode");
    assert!(!app.chat.is_composing());
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| !app.home.arrange,
        "Esc should leave what /edit entered",
    )
    .await;

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
async fn arranging_home_yields_the_music_chord_and_w_still_tends_the_bonsai() {
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
    app.handle_input(b"\x05");
    assert!(app.home.arrange);
    let tiles = app.home.leaf_count();
    let source = app.paired_source;

    // `v x` is the global audio-source chord. Arrange mode swallows stray
    // letters (and `X` closes a tile), so the suffix must reach the chord.
    app.handle_input(b"v");
    app.handle_input(b"x");
    assert_ne!(
        app.paired_source, source,
        "v x swaps the audio source while arranging"
    );
    assert_eq!(
        app.home.leaf_count(),
        tiles,
        "the chord suffix closes no tile"
    );

    // Done arranging, the bonsai is tended in the same modal as everywhere
    // else.
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.home.arrange, "arrange mode").await;
    app.handle_input(b"w");
    wait_for_render_contains(&mut app, " Bonsai ").await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Day 0") && frame.contains("vigor"),
        "expected w on Home to open the bonsai care modal; frame={frame:?}"
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

    app.handle_input(b"4v");
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

    app.handle_input(b"4v");
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
async fn chat_reaction_leader_routes_cancel_digits_and_wave() {
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

    // An unbound key closes the leader and is consumed instead of triggering its
    // ordinary message action. Check state directly instead of polling for the
    // absence of a reply banner.
    app.handle_input(b"r");
    assert!(
        !app.chat.is_reaction_leader_active(),
        "unbound input should close the reaction leader"
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
        "unbound input should not react",
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

    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "w 👋").await;
    app.handle_input(b"w");
    assert!(!app.chat.is_reaction_leader_active());
    assert!(
        !app.show_bonsai_modal,
        "reaction leader owns w before the global Bonsai shortcut"
    );
    wait_until(
        || async {
            ChatMessageReaction::get_by_user_and_message(&client, message.id, viewer.id)
                .await
                .expect("load wave reaction")
                .is_some_and(|reaction| reaction.icon == "👋")
        },
        "f w wave reaction to persist",
    )
    .await;
    let plain = render_plain(&mut app);
    assert!(
        plain.contains("▸reaction target"),
        "message selection should stay after waving: {plain:?}"
    );
    assert!(!plain.contains("w 👋"), "picker should close: {plain:?}");

    app.handle_input(b"fw");
    assert!(!app.chat.is_reaction_leader_active());
    wait_until(
        || async {
            ChatMessageReaction::get_by_user_and_message(&client, message.id, viewer.id)
                .await
                .expect("load toggled wave reaction")
                .is_none()
        },
        "repeating f w to remove the wave reaction",
    )
    .await;

    app.handle_input(b"w");
    assert!(
        app.show_bonsai_modal,
        "w opens Bonsai when the reaction leader is inactive"
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

    // Click the #rust row in the rail. It sits below the Core section
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
    // The modal sits in the chat tile, under the stock Home's reef; a
    // tall terminal gives its owner lines their rows.
    app.resize(160, 50).expect("resize test terminal");
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
    // Wait for the dnd room to appear in the rail.
    wait_for_render_contains(&mut app, "dnd").await;

    // Navigate to the dnd room. The rail order is lounge, mentions, news,
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
    for reading in ["unread 0", "mic -", "now -", today.as_str()] {
        assert!(
            frame.contains(reading),
            "{reading:?} shows by default: {frame:?}"
        );
    }
    assert!(
        frame.contains("Settings ^O")
            && frame.contains("Layouts ^E")
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
async fn runner_stays_off_the_fixed_bar() {
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
    let viewer = create_test_user(&test_db.db, "status-runner-viewer").await;
    let mut app = make_app(test_db.db.clone(), viewer.id, "status-runner-flow-it");
    app.resize(200, 40).expect("resize test terminal");
    app.fight.sheet = Some(Sheet::fresh(viewer.id, chrono::Utc::now().date_naive()));
    let top_row = render_top_row(&mut app);
    assert!(top_row.contains("chips"));
    assert!(!top_row.contains("rations"));
    assert!(!top_row.contains("signal"));
    assert!(!app.last_status_hits.borrow().is_empty());
}

/// `?` while arranging Home opens the guide on the Layouts topic, which
/// lists the arrange keys.
#[tokio::test]
async fn the_guide_opens_on_the_layouts_topic_while_arranging_home() {
    use crate::app::common::primitives::Screen;
    use crate::app::help_modal::data::HelpTopic;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "layouts-guide-viewer").await;
    let mut app = make_app(test_db.db.clone(), viewer.id, "layouts-guide-flow-it");
    app.resize(120, 40).expect("resize test terminal");
    app.handle_input(b"\x05");
    assert_eq!(app.screen, Screen::Dashboard);
    assert!(app.home.arrange);

    app.handle_input(b"?");
    assert!(app.show_help);
    assert_eq!(app.help_modal_state.selected_topic(), HelpTopic::Layouts);
}

/// The piece picker belongs to arrange mode on Home. Leaving the mode
/// (Ctrl+E) or a chord into a modal (Ctrl+O) closes it, so it never owns the
/// next surface's keys or writes a pick to a tile that lost the focus.
#[tokio::test]
async fn the_piece_picker_closes_when_the_page_or_a_modal_takes_over() {
    use crate::app::common::primitives::Screen;
    use crate::app::zen::state::{Node, TileKind};

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "piece-picker-viewer").await;
    let mut app = make_app(test_db.db.clone(), viewer.id, "piece-picker-flow-it");
    app.resize(120, 40).expect("resize test terminal");
    app.handle_input(b"\x05");
    assert_eq!(app.screen, Screen::Dashboard);
    assert!(app.home.arrange);
    app.home.rice.root = Node::leaf(TileKind::Ascii);
    app.home.focus = 0;

    app.handle_input(b"\r");
    assert!(
        app.piece_picker.is_open(),
        "Enter on the ascii tile opens it"
    );
    app.handle_input(b"\x05");
    assert!(!app.home.arrange, "Ctrl+E leaves arrange mode");
    assert!(!app.piece_picker.is_open(), "and the picker goes with it");

    app.handle_input(b"\x05");
    assert!(app.home.arrange);
    app.handle_input(b"\r");
    assert!(app.piece_picker.is_open());
    app.handle_input(b"\x0f");
    assert!(app.show_settings, "Ctrl+O opens Settings over the page");
    assert!(
        !app.piece_picker.is_open(),
        "the picker does not sit over it"
    );
    app.handle_input(b"\x1b[B");
    assert!(
        !app.piece_picker.is_open(),
        "the arrow went to Settings, not a picker"
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
    app.clubhouse.tour_route = crate::app::clubhouse::state::TourRoute::House;
    app.clubhouse.tutorial = Tutorial::Pending;
    app.clubhouse
        .enter_screen(crate::app::presence::svc::now_ms());
    assert_eq!(app.clubhouse.tutorial, Tutorial::Welcome);

    // The gate swallows everything but Enter: no page hopping, no Tab, no
    // help modal, no reserved chords (Home's layouts included), no composer.
    for bytes in [&b"2"[..], b"\t", b"?", b"\x0f", b"\x07", b"\x05", b"i"] {
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

    // The rest of the route is Enter alone; at the Artboard stop `s` shows
    // the art first, so this walk sees it.
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Artboard);
    app.handle_input(b"s");
    assert!(app.artboard_content_accepted);

    // The People page is three stops on one page: Enter lands on the map
    // and then steps the page's own sections, no key of the page needed.
    use crate::app::directory::state::Shelf;
    for (stage, shelf) in [
        (Tutorial::VisitMap, Shelf::Map),
        (Tutorial::VisitProfiles, Shelf::Profiles),
        (Tutorial::VisitJobs, Shelf::Jobs),
    ] {
        app.handle_input(b"\r");
        assert_eq!(app.screen, Screen::Profiles);
        assert_eq!(app.clubhouse.tutorial, stage);
        assert_eq!(app.directory_state.shelf(), shelf);
    }

    for screen in [Screen::Leaderboard, Screen::Calendars] {
        app.handle_input(b"\r");
        assert_eq!(app.screen, screen);
    }

    // The last stop is Home again with its second layout, the street, up;
    // the homecoming puts the first one back on the way to the tavern.
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Dashboard);
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitLayouts);
    assert_eq!(app.home_layouts.current_name(), "street");
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Clubhouse);
    assert_eq!(app.clubhouse.tutorial, Tutorial::Homecoming);
    assert_eq!(app.home_layouts.current_name(), "home");

    // Enter settles in, and input is free again.
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::Done);
    app.handle_input(b"2");
    assert_eq!(app.screen, Screen::Arcade);
}

#[tokio::test]
async fn forced_tour_can_skip_artboard_without_viewing_it() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "tour-artboard-skip-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "tour-artboard-skip-flow-it");
    app.resize(80, 24).unwrap();
    app.clubhouse.tutorial = Tutorial::VisitArtboard;
    app.set_screen(Screen::Artboard);

    let frame = render_plain(&mut app);
    assert!(
        frame.contains("the tour · [4] the artboard"),
        "frame={frame:?}"
    );
    assert!(
        frame.contains("[S] show · [Enter] skip: the people"),
        "frame={frame:?}"
    );
    assert!(!frame.contains("Mode       view"));
    assert!(!frame.contains("Back to Chat"));

    // Esc, the dialog's own Back key and the reserved chords (settings,
    // layouts)
    // all stay with the gate: the stop cannot be left sideways.
    app.handle_input(b"\x1b");
    app.pending_escape_started_at = Some(std::time::Instant::now() - Duration::from_secs(1));
    crate::app::input::flush_pending_escape(&mut app);
    app.handle_input(b"B\x0f\x05");
    assert_eq!(app.screen, Screen::Artboard);
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitArtboard);
    assert!(!app.show_settings);
    assert!(!app.artboard_content_accepted);
    assert!(
        app.dartboard_state.is_none(),
        "a skipped stop takes no seat"
    );

    for screen in [
        Screen::Profiles,
        Screen::Profiles,
        Screen::Profiles,
        Screen::Leaderboard,
        Screen::Calendars,
        Screen::Dashboard,
        Screen::Clubhouse,
    ] {
        app.handle_input(b"\r");
        assert_eq!(app.screen, screen);
        assert!(!render_plain(&mut app).contains("Mode       view"));
    }
    assert!(!app.artboard_content_accepted);
    assert!(app.profile_state.profile().artboard_disclaimer);
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::Done);

    app.handle_input(b"4");
    assert!(app.artboard_disclaimer_visible());
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
    app.clubhouse.tour_route = crate::app::clubhouse::state::TourRoute::House;
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
    app.clubhouse.tour_route = crate::app::clubhouse::state::TourRoute::House;
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

/// The `Board` route, the one every session walks: the gate swallows every
/// key the route does not take, Enter walks the door to the chat stop, the
/// practice table's break has to be played, and today's board has to be
/// won, for real, before the last Enter lands the session in the chat.
#[tokio::test]
async fn forced_tour_board_route_ends_in_the_chat_off_a_won_board() {
    use crate::app::arcade::netwalk::state::{self as netwalk, LENT_DIFFICULTY, Turn};
    use crate::app::clubhouse::state::{TOUR_ROUTE, TourRoute, Tutorial};
    use crate::app::common::primitives::Screen;
    use late_core::models::leaderboard::{ArcadeGame, DailyPuzzle};
    use late_core::models::netwalk::DailyWin;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "tour-board-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "tour-board-flow-it");
    // Room for the practice table under its header.
    app.resize(160, 50).unwrap();
    assert_eq!(TOUR_ROUTE, TourRoute::Board);
    assert_eq!(app.clubhouse.tour_route, TourRoute::Board);

    app.set_screen(Screen::Clubhouse);
    app.clubhouse.tutorial = Tutorial::Pending;
    app.clubhouse
        .enter_screen(crate::app::presence::svc::now_ms());
    assert_eq!(app.clubhouse.tutorial, Tutorial::Welcome);
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("three stops, then the board is yours"),
        "frame={frame:?}"
    );

    // The gate swallows everything but Enter: no page hopping, no Tab, no
    // help modal, no reserved chords, no composer, no arrows.
    for bytes in [
        &b"2"[..],
        b"\t",
        b"?",
        b"\x0f",
        b"\x07",
        b"\x05",
        b"i",
        b"\x1b[B",
    ] {
        app.handle_input(bytes);
    }
    assert_eq!(app.screen, Screen::Clubhouse);
    assert!(!app.show_help);
    assert_eq!(app.clubhouse.tutorial, Tutorial::Welcome);

    // Enter walks to Home for the chat stop, which leads to the table.
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Dashboard);
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitChat);
    app.handle_input(b"i");
    assert!(!app.chat.composing);
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("[Enter] next: one shot of pool"),
        "frame={frame:?}"
    );

    // Enter leads to the practice table, where the break has to be played.
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitTable);
    assert_eq!(app.screen, Screen::DailyMatch);
    assert!(!app.stations_modal_state.is_open());
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
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitTable);
    wait_for_esc_effect(&mut app, |app| app.daily.practice_played(), "break struck").await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("[Enter] next: today's board"),
        "frame={frame:?}"
    );

    // Enter leaves the table behind and opens today's board on the Arcade:
    // the Netwalk daily at its easiest, the real one, under the tour's
    // header.
    app.handle_input(b"\r");
    assert_eq!(app.screen, Screen::Arcade);
    assert!(app.daily.board.is_none());
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitBoard);
    assert!(app.is_playing_game);
    assert_eq!(app.game_selection, ArcadeGame::Puzzle(DailyPuzzle::Netwalk));
    assert_eq!(app.netwalk_state.difficulty(), LENT_DIFFICULTY);
    assert!(app.netwalk_state.is_daily_active());
    let today = chrono::Utc::now().date_naive();
    assert_eq!(app.netwalk_state.puzzle_date(), today);
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("the tour · today's board"),
        "frame={frame:?}"
    );
    assert!(
        frame.contains("[hjkl/arrows] move · [Space] turn · [f] lock"),
        "frame={frame:?}"
    );

    // Nothing leaves the board before the win: Esc, the page keys and the
    // board-switching keys stay with the gate, and Enter is a turn, not a
    // way past.
    app.handle_input(b"\x1b");
    app.pending_escape_started_at = Some(std::time::Instant::now() - Duration::from_secs(1));
    crate::app::input::flush_pending_escape(&mut app);
    app.handle_input(b"1\tp]\r");
    assert_eq!(app.screen, Screen::Arcade);
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitBoard);
    assert!(app.is_playing_game);
    assert!(app.netwalk_state.is_daily_active());
    assert_eq!(app.netwalk_state.difficulty(), LENT_DIFFICULTY);

    // One tile from done: an end of the network turned off its line. The
    // board's keys walk the cursor there and turn it back, the real win.
    let solution =
        netwalk::generate(LENT_DIFFICULTY, netwalk::daily_seed(today, LENT_DIFFICULTY)).solution;
    let leaf = solution
        .iter()
        .position(|mask| mask.count_ones() == 1)
        .expect("a spanning tree has an end");
    let mut tiles = solution.clone();
    tiles[leaf] = netwalk::rotate(tiles[leaf], Turn::Clockwise);
    app.netwalk_state
        .set_board_for_test(LENT_DIFFICULTY, tiles, 3);
    assert!(!app.netwalk_state.is_solved());
    let dimension = app.netwalk_state.dimension();
    let (row, column) = (leaf / dimension, leaf % dimension);
    let cursor = app.netwalk_state.cursor();
    let (cursor_row, cursor_column) = (cursor / dimension, cursor % dimension);
    for _ in cursor_row..row {
        app.handle_input(b"j");
    }
    for _ in row..cursor_row {
        app.handle_input(b"k");
    }
    for _ in cursor_column..column {
        app.handle_input(b"l");
    }
    for _ in column..cursor_column {
        app.handle_input(b"\x1b[D");
    }
    assert_eq!(app.netwalk_state.cursor(), leaf);
    app.handle_input(b"x");
    assert!(app.netwalk_state.is_solved());
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitBoard);
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("connected. that was day one."),
        "frame={frame:?}"
    );
    assert!(frame.contains("[Enter] next: the week"), "frame={frame:?}");

    // The win is the Arcade's own: it lands in the day's wins.
    let client = test_db.db.get().await.expect("db client");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let win = DailyWin::find(&client, user.id, LENT_DIFFICULTY.key(), today)
            .await
            .expect("query the win");
        if win.is_some() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the lent board's win never landed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Enter puts the board away and boxes the week over the Arcade lobby:
    // today's rung starred once won, tomorrow's large, look only.
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitWeek);
    assert_eq!(app.screen, Screen::Arcade);
    assert!(!app.is_playing_game);
    let frame = render_plain(&mut app);
    assert!(frame.contains("the tour · the week"), "frame={frame:?}");
    assert!(frame.contains("tomorrow"), "frame={frame:?}");
    assert!(
        frame.contains("[Enter] finish the tour · into the chat"),
        "frame={frame:?}"
    );
    app.handle_input(b"o]");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitWeek);
    assert!(!app.arcade_solvers_open);

    // Enter lands in the chat: the tour is done, input is free again (the
    // page keys work, as in the House route's test; this app has joined
    // no room to compose in), and the Arcade still holds the won board.
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::Done);
    assert_eq!(app.screen, Screen::Dashboard);
    assert!(!app.is_playing_game);
    app.handle_input(b"2");
    assert_eq!(app.screen, Screen::Arcade);
    assert!(!app.is_playing_game);
    assert!(app.netwalk_state.is_solved());
}

/// On the `Board` route a terminal too small for the table or the board
/// under its header has nothing to play: each stop says so and Enter walks
/// on, with no break struck blind.
/// `s` at the board asks first; Esc takes the question back, Enter on it
/// leaves the board unsolved and goes on to the week.
#[tokio::test]
async fn forced_tour_board_can_be_skipped_after_a_confirmation() {
    use crate::app::clubhouse::state::{TourRoute, Tutorial};
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "tour-skip-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "tour-skip-flow-it");
    app.resize(160, 50).unwrap();
    assert_eq!(app.clubhouse.tour_route, TourRoute::Board);

    app.set_screen(Screen::Clubhouse);
    app.clubhouse.tutorial = Tutorial::Pending;
    app.clubhouse
        .enter_screen(crate::app::presence::svc::now_ms());
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitChat);
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitTable);
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
    app.handle_input(b"\r");
    wait_for_esc_effect(&mut app, |app| app.daily.practice_played(), "break struck").await;
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitBoard);
    assert!(app.is_playing_game);
    let frame = render_plain(&mut app);
    assert!(frame.contains("[s] skip"), "frame={frame:?}");

    // `s` asks; the board's keys and Enter do not move the board meanwhile.
    app.handle_input(b"s");
    assert!(app.clubhouse.board_skip_asked);
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("leave the board unsolved?"),
        "frame={frame:?}"
    );
    assert!(
        frame.contains("[Enter] skip to the week · [Esc] back to the board"),
        "frame={frame:?}"
    );
    let cursor = app.netwalk_state.cursor();
    app.handle_input(b"l");
    assert_eq!(app.netwalk_state.cursor(), cursor);

    // Esc takes the question back: the board is open again, nothing moved.
    app.handle_input(b"\x1b");
    app.pending_escape_started_at = Some(std::time::Instant::now() - Duration::from_secs(1));
    crate::app::input::flush_pending_escape(&mut app);
    assert!(!app.clubhouse.board_skip_asked);
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitBoard);
    assert!(app.is_playing_game);
    let frame = render_plain(&mut app);
    assert!(frame.contains("solve it, then we go."), "frame={frame:?}");

    // Asked again and confirmed: the board goes unsolved, the week is up.
    app.handle_input(b"s\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitWeek);
    assert!(!app.clubhouse.board_skip_asked);
    assert!(!app.netwalk_state.is_solved());
    assert!(!app.is_playing_game);
    assert_eq!(app.screen, Screen::Arcade);
    let frame = render_plain(&mut app);
    assert!(frame.contains("the tour · the week"), "frame={frame:?}");
    assert!(
        frame.contains("up at 00:00 UTC: your first star."),
        "frame={frame:?}"
    );
}

#[tokio::test]
async fn forced_tour_board_route_walks_past_what_a_small_terminal_cannot_show() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "tour-small-board-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "tour-small-board-flow-it");
    app.resize(80, 12).unwrap();

    app.set_screen(Screen::Clubhouse);
    app.clubhouse.tutorial = Tutorial::Pending;
    app.clubhouse
        .enter_screen(crate::app::presence::svc::now_ms());
    app.handle_input(b"\r\r");
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
    assert!(
        frame.contains("[Enter] next: today's board"),
        "frame={frame:?}"
    );

    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitBoard);
    assert_eq!(app.screen, Screen::Arcade);
    assert!(app.daily.board.is_none());
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("this board needs a bigger window"),
        "frame={frame:?}"
    );
    assert!(frame.contains("[Enter] next: the week"), "frame={frame:?}");

    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::VisitWeek);
    assert_eq!(app.screen, Screen::Arcade);
    assert!(!app.is_playing_game);
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::Done);
    assert_eq!(app.screen, Screen::Dashboard);
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

/// A `/me` line edits as an action: `e` puts the command back in the
/// composer, and sending it rewrites that message instead of posting a new
/// one beside it.
#[tokio::test]
async fn editing_a_me_action_rewrites_it_in_place() {
    use late_core::models::chat_message::ChatMessage;

    let (test_db, mut app) = chat_compose_app("me-edit").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let bodies = || async {
        ChatMessage::list_recent(&client, lounge.id, 10)
            .await
            .expect("list lounge")
            .into_iter()
            .map(|message| message.body)
            .collect::<Vec<_>>()
    };

    app.handle_input(b"/me waves");
    app.handle_input(b"\r");
    wait_until(
        || async { bodies().await == ["\x01ACTION waves\x01"] },
        "the action is posted",
    )
    .await;
    wait_for_render_contains(&mut app, "waves").await;
    assert!(!app.chat.composing, "sending closed the composer");

    app.handle_input(b"k");
    app.handle_input(b"e");
    assert_eq!(app.chat.composer().lines().join("\n"), "/me waves");

    app.handle_input(b" again");
    app.handle_input(b"\r");
    wait_until(
        || async { bodies().await == ["\x01ACTION waves again\x01"] },
        "the action is rewritten, and nothing new is posted",
    )
    .await;
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

/// The tavern's idle composer names the tavern's keys: j/k walk the figure
/// there, so it must not offer them as message selection the way Home does.
#[tokio::test]
async fn clubhouse_composer_hint_names_the_tavern_keys() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let (_test_db, mut app) = chat_compose_app("clubhouse-hint").await;
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.chat.composing, "composer closed").await;
    app.set_screen(Screen::Clubhouse);
    app.clubhouse.tutorial = Tutorial::Done;
    app.clubhouse
        .enter_screen(crate::app::presence::svc::now_ms());

    let frame = render_plain(&mut app);
    assert!(!frame.contains("j/k select"), "frame={frame:?}");
    assert!(frame.contains("i to say something"), "frame={frame:?}");
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

    app.handle_input(b"4v");
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

/// A refused hang says so in a banner, once: on the board the notice
/// alone sits in the framing bar, where it is easy to miss.
#[tokio::test]
async fn a_refused_hang_raises_one_banner() {
    use late_core::models::artboard_piece::{ArtboardPiece, HangOutcome, HangParams};

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-cap-it").await;
    let client = test_db.db.get().await.expect("db client");
    // Today's three pieces are up already.
    for n in 0..3 {
        let hung = ArtboardPiece::hang(
            &client,
            HangParams {
                user_id: user.id,
                title: format!("piece {n}"),
                width: 12,
                height: 4,
                canvas: serde_json::json!({
                    "width": 12,
                    "height": 4,
                    "cells": [[{"x": 0, "y": 0}, {"Narrow": "#"}]],
                    "colors": [],
                }),
                provenance: serde_json::json!({ "cells": [] }),
                glyph_count: 40,
                own_share_percent: 100,
                content_hash: format!("hash-cap-{n}"),
            },
        )
        .await
        .expect("hang");
        assert!(matches!(hung, HangOutcome::Hung(_)));
    }
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-cap-flow-it");

    app.handle_input(b"4v");
    wait_for_render_contains(&mut app, "Mode       view").await;
    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Mode       active").await;
    app.handle_input(b"\x1b[200~##########\n##########\n##########\n##########\x1b[201~\r");
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| !app.dartboard_state.as_ref().unwrap().has_floating(),
        "dismiss floating paste",
    )
    .await;
    app.handle_input(b"\x1b");
    wait_for_render_contains(&mut app, "Mode       view").await;
    app.handle_input(b"\x1b");
    wait_for_render_contains(&mut app, "rail j/k").await;
    for _ in 0..5 {
        app.handle_input(b"j");
    }
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Frame your work").await;
    app.handle_input(b"\x1b[<0;2;2M");
    app.handle_input(b"\x1b[<32;11;5M");
    app.handle_input(b"\x1b[<0;11;5m");
    wait_for_render_contains(&mut app, "frame 10x4").await;
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Hang it in the").await;
    app.banner = None;
    app.handle_input(b"a fourth\r");

    let refusal = "You have hung today's three pieces already. Tomorrow.";
    wait_for_app(&mut app, "the refusal's banner", |app| {
        app.banner
            .as_ref()
            .is_some_and(|banner| banner.message == refusal)
    })
    .await;
    // Dismissed, it stays dismissed: the refusal is handed over once.
    app.banner = None;
    app.tick();
    assert!(app.banner.is_none());
}

#[tokio::test]
async fn artboard_gallery_hangs_a_framed_piece_from_the_rail() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-gallery-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-gallery-flow-it");

    app.handle_input(b"4v");
    wait_for_render_contains(&mut app, "Mode       view").await;
    wait_for_render_contains(&mut app, "GALLERY").await;
    wait_for_render_contains(&mut app, "Hang a piece").await;

    // Paint a 10x4 block: forty glyphs, the floor for a piece.
    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Mode       active").await;
    app.handle_input(b"\x1b[200~##########\n##########\n##########\n##########\x1b[201~\r");
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| !app.dartboard_state.as_ref().unwrap().has_floating(),
        "dismiss floating paste",
    )
    .await;
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
async fn ctrl_e_arranges_home_and_the_layout_keys_are_arrange_modes_alone() {
    use crate::app::zen::state::TileKind;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "home-arrange-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    // The layout is this device's: it needs the SSH key a session has.
    UserSshKey::ensure(&client, user.id, "SHA256:arrange")
        .await
        .expect("arrange key");
    let mut app = with_session_key(
        make_app(test_db.db.clone(), user.id, "home-arrange-flow-it"),
        "SHA256:arrange",
    );
    wait_for_render_contains(&mut app, "sort/fold").await;
    let tiles = app.home.leaf_count();
    assert_eq!(app.home.focused_kind(), Some(TileKind::Chat));

    // Outside arrange mode the page is furniture: `S` splits nothing.
    app.handle_input(b"S");
    assert_eq!(app.home.leaf_count(), tiles);
    assert!(!app.home.arrange);

    // Ctrl+E: the layout keys are live, every tile has a title rule, and
    // the bottom row is a footer naming the keys for as long as the mode
    // is on, not a toast that goes away.
    app.handle_input(b"\x05");
    assert!(app.home.arrange, "Ctrl+E enters arrange mode");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("S split") && frame.contains("Esc done"),
        "the footer names the keys; frame={frame:?}"
    );
    app.handle_input(b"S");
    assert_eq!(
        app.home.leaf_count(),
        tiles + 1,
        "S splits the focused tile"
    );
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("S split"),
        "the footer stays through the mode; frame={frame:?}"
    );
    assert!(
        app.home_layout_dirty,
        "an edit marks the layout for writing"
    );

    // Esc leaves arrange mode, back on the center, and writes the layout
    // at once.
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.home.arrange, "arrange mode").await;
    assert_eq!(app.home.focused_kind(), Some(TileKind::Chat));
    assert!(!app.home_layout_dirty);
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("S split"),
        "the footer leaves with the mode; frame={frame:?}"
    );
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move {
                let client = db.get().await.expect("db client");
                let key = UserSshKey::find_by_fingerprint(&client, user.id, "SHA256:arrange")
                    .await
                    .expect("load key")
                    .expect("key exists");
                late_core::models::user_ssh_key::extract_key_home_layouts(&key.settings).is_some()
            }
        },
        "the arranged Home to reach this device's key",
    )
    .await;
    let stored = User::get(&client, user.id)
        .await
        .expect("load user")
        .expect("user exists");
    assert_eq!(
        late_core::models::user::extract_home_layout(&stored.settings),
        None,
        "the account keeps no layout of its own: it is only the seed"
    );
    // Ctrl+E is the layouts chord: it enters arrange mode, and a digit
    // loads that layout and leaves the mode. Layout 2 is the stock street
    // layout, drawn on Home like any other.
    app.handle_input(b"\x05");
    assert!(app.home.arrange, "Ctrl+E enters arrange mode");
    app.handle_input(b"2");
    assert!(!app.home.arrange, "a digit leaves the mode");
    assert_eq!(
        app.screen,
        crate::app::common::primitives::Screen::Dashboard,
        "street is a layout of Home, not a page"
    );
    assert_eq!(app.home_layouts.current_name(), "street");
    assert_eq!(
        app.home.leaf_count(),
        crate::app::zen::state::RiceLayout::street()
            .root
            .leaf_count()
    );
    assert!(!app.home_layout_dirty, "leaving the mode wrote the switch");
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move {
                let client = db.get().await.expect("db client");
                let key = UserSshKey::find_by_fingerprint(&client, user.id, "SHA256:arrange")
                    .await
                    .expect("load key")
                    .expect("key exists");
                late_core::models::user_ssh_key::extract_key_home_layouts(&key.settings)
                    .and_then(|layouts| layouts.get("current").and_then(|c| c.as_u64()))
                    == Some(1)
            }
        },
        "the switch to reach this device's key",
    )
    .await;
    // Back on layout 1, the split from before the switch is still there:
    // a page goes into its slot before another is loaded.
    app.handle_input(b"\x05");
    app.handle_input(b"1");
    assert_eq!(app.home_layouts.current_name(), "home");
    assert_eq!(app.home.leaf_count(), tiles + 1);
}

/// Alt+1..9 is the one-chord shortcut for `Ctrl+E` then the digit: on Home
/// it loads that layout without arrange mode, a composer being typed in
/// keeps it, and every other page ignores it.
#[tokio::test]
async fn alt_digit_loads_a_home_layout_in_one_chord() {
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "home-alt-digit-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    UserSshKey::ensure(&client, user.id, "SHA256:altdigit")
        .await
        .expect("alt digit key");
    let mut app = with_session_key(
        make_app(test_db.db.clone(), user.id, "home-alt-digit-flow-it"),
        "SHA256:altdigit",
    );
    wait_for_render_contains(&mut app, "sort/fold").await;
    assert_eq!(app.home_layouts.current_name(), "home");
    // The lounge first, so the composer below has a room to open in.
    crate::test_helpers::wait_for_app(&mut app, "the lounge selected", |app| {
        app.chat.selected_room_id.is_some()
    })
    .await;

    // Alt+2: street, straight away, arrange mode never on.
    app.handle_input(b"\x1b2");
    assert!(!app.home.arrange, "Alt+digit does not enter arrange mode");
    assert_eq!(app.screen, Screen::Dashboard);
    assert_eq!(app.home_layouts.current_name(), "street");

    // Alt+1 brings home back.
    app.handle_input(b"\x1b1");
    assert_eq!(app.home_layouts.current_name(), "home");

    // An empty slot says so and moves nothing.
    app.handle_input(b"\x1b9");
    assert_eq!(app.home_layouts.current_name(), "home");
    assert!(app.banner.is_some(), "an empty slot shows a banner");

    // In arrange mode it loads and leaves the mode, as `Ctrl+E 2` does.
    app.handle_input(b"\x05");
    assert!(app.home.arrange);
    app.handle_input(b"\x1b2");
    assert!(!app.home.arrange, "Alt+digit leaves arrange mode");
    assert_eq!(app.home_layouts.current_name(), "street");
    app.handle_input(b"\x1b1");

    // From the layouts list, as its note says: the list closes, the
    // layout loads, the mode is done.
    app.handle_input(b"\x05");
    app.handle_input(b"L");
    assert!(app.layout_list.is_some(), "Ctrl+E L opens the layouts list");
    app.handle_input(b"\x1b2");
    assert!(app.layout_list.is_none(), "Alt+digit closes the list");
    assert!(!app.home.arrange);
    assert_eq!(app.home_layouts.current_name(), "street");
    app.handle_input(b"\x1b1");

    // A composer being typed in keeps it: the draft must not go with a
    // layout whose chat is elsewhere.
    app.handle_input(b"i");
    assert!(app.chat.is_composing());
    app.handle_input(b"\x1b2");
    assert_eq!(app.home_layouts.current_name(), "home");
    assert!(app.chat.is_composing());
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.chat.is_composing(), "composer").await;

    // Off Home it does nothing: no page change, no layout switch.
    app.handle_input(b"2");
    assert_eq!(app.screen, Screen::Arcade);
    app.handle_input(b"\x1b2");
    assert_eq!(app.screen, Screen::Arcade);
    assert_eq!(app.home_layouts.current_name(), "home");
}

/// The page's chat is the active tile wherever it sits in the layout: focus
/// the second chat tile and it draws Home's card, the activity ticker over
/// its composer and a synthetic entry's composer included, rather than the
/// watching view every other tile draws.
#[tokio::test]
async fn the_second_chat_tile_draws_the_pages_card_when_it_is_active() {
    use crate::app::activity::lounge::{SYSTEM_FINGERPRINT, SYSTEM_USERNAME};
    use late_core::models::user::UserParams;

    const FEED_LINE: &str = "ticker-probe cleared Minesweeper";

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "page-card-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let announcements = ChatRoom::ensure_permanent(&client, "announcements")
        .await
        .expect("announcements");
    ChatRoomMember::join(&client, announcements.id, user.id)
        .await
        .expect("join announcements");
    let system = User::create(
        &client,
        UserParams {
            fingerprint: SYSTEM_FINGERPRINT.to_string(),
            username: SYSTEM_USERNAME.to_string(),
            settings: serde_json::json!({ "bot": true, "system": true }),
        },
    )
    .await
    .expect("create system user");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: system.id,
            body: format!("· {FEED_LINE}"),
        },
    )
    .await
    .expect("create feed line");

    let mut app = make_app(test_db.db.clone(), user.id, "page-card-flow-it");
    app.resize(200, 50).expect("resize test terminal");
    wait_for_render_contains(&mut app, FEED_LINE).await;
    assert_eq!(
        app.chat.activity_ticker().len(),
        1,
        "the line feeds the ticker, not the chat"
    );

    app.handle_input(b"i/layout 2\r");
    assert_eq!(app.home_layouts.current_name(), "street");
    let tiles = app.home.chat_tiles();
    let [(left, _), (right, _)] = tiles.as_slice() else {
        panic!("street holds two chats: {tiles:?}");
    };
    let (left, right) = (*left, *right);
    assert_eq!(app.home.focus, left);
    wait_for_render_contains(&mut app, FEED_LINE).await;

    let (_, rect, _) = app.home_tile_rects()[right];
    let click = format!("\x1b[<0;{};{}M", rect.x + 2, rect.y + 2);
    app.handle_input(click.as_bytes());
    assert_eq!(app.home.focus, right);
    let frame = render_plain(&mut app);
    assert!(
        frame.contains(FEED_LINE),
        "the ticker follows the focus to the second tile; frame={frame:?}"
    );

    // News on the second tile is the page's News, composer and all. The
    // first tile has News on `street`, so a pick there would only move the
    // focus back: #lounge frees it first.
    let (_, rect, _) = app.home_tile_rects()[left];
    let click = format!("\x1b[<0;{};{}M", rect.x + 2, rect.y + 2);
    app.handle_input(click.as_bytes());
    assert_eq!(app.home.focus, left);
    app.handle_input(b"\x1f");
    app.handle_input(b"lounge\r");
    assert_eq!(
        app.home.active_chat_room(),
        Some(ChatRoomBinding::Room(lounge.id))
    );
    let (_, rect, _) = app.home_tile_rects()[right];
    let click = format!("\x1b[<0;{};{}M", rect.x + 2, rect.y + 2);
    app.handle_input(click.as_bytes());
    assert_eq!(app.home.focus, right);
    app.handle_input(b"\x1f");
    app.handle_input(b"news\r");
    assert!(app.chat.news_selected);
    assert_eq!(app.home.focus, right);
    assert_eq!(app.home.active_chat_room(), Some(ChatRoomBinding::News));
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Share URL"),
        "the active News tile draws the page's composer; frame={frame:?}"
    );
}

#[tokio::test]
async fn a_second_chat_tile_on_home_keeps_its_room_and_the_rail_follows_the_active_one() {
    use crate::app::zen::state::TileKind;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "home-two-chats-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let stream = ChatRoom::get_or_create_public_room(&client, "stream-night")
        .await
        .expect("stream room");
    ChatRoomMember::join(&client, stream.id, user.id)
        .await
        .expect("join stream room");
    let mut app = make_app(test_db.db.clone(), user.id, "home-two-chats-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "stream-night").await;
    assert_eq!(app.chat.selected_room_id, Some(lounge.id));
    let first = app
        .home
        .first_tile_of(TileKind::Chat)
        .expect("the stock Home has a chat");
    assert_eq!(
        app.home.chat_tiles(),
        vec![(first, ChatRoomBinding::Current)],
        "the stock chat shows the rail's room"
    );

    // Ctrl+E, split the chat, make the new tile a chat: both tiles are
    // pinned to the room of the moment, so neither follows the rail
    // into the other's room from now on.
    app.handle_input(b"\x05");
    app.handle_input(b"S");
    let second = app.home.focus;
    assert_eq!(app.home.focused_kind(), Some(TileKind::Blank));
    app.handle_input(b" ");
    while app.home.kind_picker_selection() != Some(TileKind::Chat) {
        app.home.move_kind_picker(1);
    }
    app.handle_input(b"\r");
    assert_eq!(
        app.home.chat_tiles(),
        vec![
            (first, ChatRoomBinding::Room(lounge.id)),
            (second, ChatRoomBinding::Room(lounge.id))
        ]
    );

    // `]` rebinds the active tile (the focused one) to the next stop and
    // the rail follows it: News, as the rail orders them, then the stream.
    // The first tile keeps #lounge.
    app.handle_input(b"]");
    assert_eq!(
        app.home.chat_tiles(),
        vec![
            (first, ChatRoomBinding::Room(lounge.id)),
            (second, ChatRoomBinding::News)
        ]
    );
    app.handle_input(b"]");
    assert_eq!(
        app.home.chat_tiles(),
        vec![
            (first, ChatRoomBinding::Room(lounge.id)),
            (second, ChatRoomBinding::Room(stream.id))
        ]
    );
    assert_eq!(app.chat.selected_room_id, Some(stream.id));
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.home.arrange, "arrange mode").await;
    assert_eq!(app.home.focus, second, "Esc keeps the focus on the chat");
    // The arrange banner sits over the page's top rows, the title rules'.
    app.banner = None;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("chat · #lounge") && frame.contains("chat · #stream-night"),
        "each chat tile wears its room's rule; frame={frame:?}"
    );
    assert_eq!(
        (
            frame.matches("Compose").count(),
            frame.matches("watching").count()
        ),
        (1, 1),
        "the active tile draws Home's card, the other only watches; frame={frame:?}"
    );

    // A click on the first tile makes it the active chat and moves the
    // rail to its room; the second tile stays on the stream.
    let (_, rect, _) = app.home_tile_rects()[first];
    let click = format!("\x1b[<0;{};{}M", rect.x + 2, rect.y + 2);
    app.handle_input(click.as_bytes());
    assert_eq!(app.home.focus, first);
    assert_eq!(app.chat.selected_room_id, Some(lounge.id));
    assert_eq!(
        app.home.chat_tiles(),
        vec![
            (first, ChatRoomBinding::Room(lounge.id)),
            (second, ChatRoomBinding::Room(stream.id))
        ]
    );
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("chat · #lounge")
            && frame.contains("chat · #stream-night")
            && frame.contains("Home · lounge"),
        "both tiles keep their rooms' rules and the header follows the active one; frame={frame:?}"
    );

    // The active tile remembers where the rail takes it: the rail moving
    // to the stream rebinds the first tile, its rule says so, and the
    // next focus on it brings the rail back there.
    app.home_layout_dirty = false;
    app.chat
        .select_room_slot(crate::app::chat::state::RoomSlot::Room(stream.id));
    app.sync_visible_chat_room();
    assert_eq!(app.home_chat_room_id(), Some(stream.id));
    assert_eq!(
        app.home.chat_tiles(),
        vec![
            (first, ChatRoomBinding::Room(stream.id)),
            (second, ChatRoomBinding::Room(stream.id))
        ],
        "the rail steers the active tile"
    );
    assert!(app.home_layout_dirty, "a rebind is a layout edit");
    let frame = render_plain(&mut app);
    assert_eq!(
        frame.matches("chat · #stream-night").count(),
        2,
        "the active tile's rule names what it draws; frame={frame:?}"
    );
    let (_, rect, _) = app.home_tile_rects()[second];
    let click = format!("\x1b[<0;{};{}M", rect.x + 2, rect.y + 2);
    app.handle_input(click.as_bytes());
    let (_, rect, _) = app.home_tile_rects()[first];
    let click = format!("\x1b[<0;{};{}M", rect.x + 2, rect.y + 2);
    app.handle_input(click.as_bytes());
    assert_eq!(app.home.focus, first);
    assert_eq!(
        app.chat.selected_room_id,
        Some(stream.id),
        "the tile kept the stream"
    );

    // The picker is the same act in one step: a room picked there binds
    // the active tile and the rail follows.
    app.handle_input(b"\x1f");
    assert!(
        app.room_search_modal_state.is_open(),
        "Ctrl+/ opens the picker"
    );
    app.handle_input(b"lounge\r");
    assert_eq!(app.chat.selected_room_id, Some(lounge.id));
    assert_eq!(
        app.home.chat_tiles(),
        vec![
            (first, ChatRoomBinding::Room(lounge.id)),
            (second, ChatRoomBinding::Room(stream.id))
        ]
    );
}

/// The stock `street` holds News on the left and #announcements on the
/// right. Focusing the right tile moves the rail to #announcements; a
/// #lounge pick in the picker then binds that tile to #lounge and the rail
/// shows it, the arrival sync notwithstanding: a pick made on Home stands.
/// A News pick next finds News on the left tile and moves the focus there,
/// binding nothing: the page shows no room twice.
#[tokio::test]
async fn home_street_pick_binds_the_active_tile_or_focuses_the_tile_showing_it() {
    use crate::app::zen::state::TileKind;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "street-news-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let announcements = ChatRoom::ensure_permanent(&client, "announcements")
        .await
        .expect("announcements");
    ChatRoomMember::join(&client, announcements.id, user.id)
        .await
        .expect("join announcements");
    let mut app = make_app(test_db.db.clone(), user.id, "street-news-flow-it");
    app.resize(200, 50).expect("resize test terminal");
    wait_for_render_contains(&mut app, "announcements").await;

    app.handle_input(b"i/layout 2\r");
    assert_eq!(app.home_layouts.current_name(), "street");
    let tiles = app.home.chat_tiles();
    let [
        (left, ChatRoomBinding::News),
        (right, ChatRoomBinding::Permanent(slug)),
    ] = tiles.as_slice()
    else {
        panic!("street holds News and a permanent room: {tiles:?}");
    };
    let (left, right) = (*left, *right);
    assert_eq!(slug, "announcements");
    assert_eq!(app.home.focused_kind(), Some(TileKind::Chat));
    assert_eq!(app.home.focus, left);
    assert!(
        app.chat.news_selected,
        "the rail follows the first tile onto News"
    );
    wait_for_render_contains(&mut app, "chat · news").await;

    let (_, rect, _) = app.home_tile_rects()[right];
    let click = format!("\x1b[<0;{};{}M", rect.x + 2, rect.y + 2);
    app.handle_input(click.as_bytes());
    assert_eq!(app.home.focus, right);
    assert_eq!(app.chat.selected_room_id, Some(announcements.id));
    assert!(!app.chat.news_selected);

    app.handle_input(b"\x1f");
    app.handle_input(b"lounge\r");
    assert_eq!(
        app.chat.selected_room_id,
        Some(lounge.id),
        "the pick stands on Home"
    );
    assert_eq!(app.home.focus, right);
    assert_eq!(
        app.home.chat_tiles(),
        vec![
            (left, ChatRoomBinding::News),
            (right, ChatRoomBinding::Room(lounge.id))
        ],
        "the pick binds the active tile"
    );

    app.handle_input(b"\x1f");
    app.handle_input(b"news\r");
    assert!(
        app.chat.news_selected,
        "the rail follows the focus onto News"
    );
    assert_eq!(app.home_chat_room_id(), None, "no room to act on");
    assert_eq!(
        app.home.focus, left,
        "the pick goes to the tile showing News"
    );
    assert_eq!(
        app.home.chat_tiles(),
        vec![
            (left, ChatRoomBinding::News),
            (right, ChatRoomBinding::Room(lounge.id))
        ],
        "the pick binds nothing"
    );
    let frame = render_plain(&mut app);
    assert_eq!(
        frame.matches("chat · news").count(),
        1,
        "one rule says News; frame={frame:?}"
    );

    // Leaving `home` pinned its one chat, bound to no room, to #lounge, the
    // room of that moment: `Ctrl+E 1` lands there, not on News, and
    // `Ctrl+E 2` lands on News again.
    app.handle_input(b"\x05");
    app.handle_input(b"1");
    assert_eq!(app.home_layouts.current_name(), "home");
    assert_eq!(app.chat.selected_room_id, Some(lounge.id));
    assert!(!app.chat.news_selected, "home comes back where it was");
    let home_chat = app
        .home
        .first_tile_of(TileKind::Chat)
        .expect("home has a chat");
    assert_eq!(
        app.home.chat_tiles(),
        vec![(home_chat, ChatRoomBinding::Room(lounge.id))]
    );
    app.handle_input(b"\x05");
    app.handle_input(b"2");
    assert_eq!(app.home_layouts.current_name(), "street");
    assert!(app.chat.news_selected, "street comes back on News");

    // On `home`, the rail moving to #announcements is what the tile keeps:
    // away to `street` and back, it is still there.
    app.handle_input(b"\x05");
    app.handle_input(b"1");
    app.chat
        .select_room_slot(crate::app::chat::state::RoomSlot::Room(announcements.id));
    app.sync_visible_chat_room();
    assert_eq!(
        app.home.chat_tiles(),
        vec![(home_chat, ChatRoomBinding::Room(announcements.id))]
    );
    app.handle_input(b"\x05");
    app.handle_input(b"2");
    assert!(app.chat.news_selected);
    app.handle_input(b"\x05");
    app.handle_input(b"1");
    assert_eq!(
        app.chat.selected_room_id,
        Some(announcements.id),
        "home comes back to the room it was switched to"
    );
    assert!(!app.chat.news_selected);
}

/// In arrange mode every tile takes the focus, and Tab walks them instead
/// of switching pages.
#[tokio::test]
async fn home_arrange_tab_cycles_tile_focus_instead_of_switching_pages() {
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
    app.handle_input(b"\x05");
    assert!(app.home.arrange);
    let tiles = app.home.leaf_count();
    let start = app.home.focus;

    // Tab walks the tiles in layout order and stays on the page.
    app.handle_input(b"\t");
    assert_eq!(
        app.screen,
        Screen::Dashboard,
        "Tab while arranging stays Home"
    );
    assert_eq!(
        app.home.focus,
        (start + 1) % tiles,
        "Tab focuses the next tile"
    );

    // Shift+Tab walks back.
    app.handle_input(b"\x1b[Z");
    assert_eq!(app.screen, Screen::Dashboard);
    assert_eq!(app.home.focus, start, "Shift+Tab focuses the previous tile");

    // Tab wraps past the last tile to the first.
    for _ in 0..tiles {
        app.handle_input(b"\t");
    }
    assert_eq!(app.home.focus, start, "Tab wraps around the tiles");
    assert_eq!(app.screen, Screen::Dashboard);
}

#[tokio::test]
async fn landing_page_tweak_picks_the_first_screen_except_for_new_users() {
    use crate::app::common::primitives::Screen;
    use crate::test_helpers::SessionWorld;
    use late_core::models::user::LandingPage;

    let test_db = new_test_db().await;
    for (idx, (page, is_new_user, tour_pending, expected)) in [
        (LandingPage::Clubhouse, false, false, Screen::Clubhouse),
        (LandingPage::Home, false, false, Screen::Dashboard),
        // A first session always starts in the tavern, where the tour runs.
        (LandingPage::Home, true, false, Screen::Clubhouse),
        // So does a regular whose tour flag was reset: the tour only fires
        // in the tavern, and a Home landing would never show it.
        (LandingPage::Home, false, true, Screen::Clubhouse),
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
                tour_pending,
                ..SessionWorld::default()
            },
        );
        assert_eq!(
            app.screen, expected,
            "landing {page:?}, new user {is_new_user}, tour pending {tour_pending}"
        );
        if tour_pending {
            assert_eq!(
                app.clubhouse.tutorial,
                crate::app::clubhouse::state::Tutorial::Welcome,
                "the reset tour opens at the door"
            );
        }
    }
}

/// A watch opened from Home (`s1` on the row its Live tile lists the game
/// on) is a trip into the games like any other stop: backtick comes home.
#[tokio::test]
async fn a_watch_opened_from_home_comes_home_on_backtick() {
    use crate::app::common::primitives::Screen;
    use crate::app::door::spectate::{
        proxy::LiveGame,
        state::{SpectateGame, WatchMode},
    };

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-watch-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-watch-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    let started_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("wall clock after the unix epoch")
        .as_secs()
        - 30;
    app.live_games.publish_roster_for_tests(
        SpectateGame::Dcss,
        vec![LiveGame {
            playname: "crawler".to_string(),
            started_unix,
            watchers: 0,
            status: String::new(),
        }],
    );

    // The zen layout's Live tile lists the game on its first row, with its
    // key.
    load_zen_fixture(&mut app);
    assert_eq!(app.screen, Screen::Dashboard);
    wait_for_render_contains(&mut app, "dcss").await;
    wait_for_render_contains(&mut app, "s1").await;

    // No render runs from here: the test door host is unreachable, so a
    // tick would see the stream end and drop the watch.
    app.handle_input(b"s1");
    assert_eq!(app.screen, Screen::Games, "s1 opens the watch");
    assert_eq!(
        app.spectate_state.as_ref().map(|state| state.mode()),
        Some(WatchMode::Open)
    );

    app.handle_input(b"`");
    assert_eq!(app.screen, Screen::Dashboard, "backtick comes home");
    assert_eq!(app.home_layouts.current_name(), "zen", "on its layout");
}

/// Lone Esc is held for escape-sequence disambiguation and dispatches on a
/// later tick: backdate it and flush, since an await in a flow over a
/// fabricated door game would end the game.
fn flush_lone_escape(app: &mut crate::app::state::App) {
    app.handle_input(b"\x1b");
    app.pending_escape_started_at = Some(std::time::Instant::now() - Duration::from_secs(1));
    crate::app::input::flush_pending_escape(app);
}

/// A player's watch chat is shut while they play. F4 opens it over the
/// running game, a column beside it, and from then every key is the chat's,
/// never the game's or a global shortcut's: `i` writes, Enter sends and the
/// chat keeps the keys. Esc closes it back to the game, F4 closes it from
/// the composer too (the draft goes with it), and so does the game ending.
#[tokio::test]
async fn f4_opens_the_watcher_chat_over_a_running_game_and_it_takes_every_key() {
    use crate::app::common::primitives::Screen;
    use crate::app::door::spectate::state::SpectateGame;
    use late_core::models::chat_room::{ChatRoom, WatchDoor};
    use late_core::models::chat_room_member::ChatRoomMember;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "own-chat-f4").await;
    // The player's watch-chat room, joined ahead of the session the way a
    // returning player's is, so the room list the app loads carries it.
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_watch_room(&client, WatchDoor::Nethack, "tester")
        .await
        .expect("the player's watch room");
    ChatRoomMember::join(&client, room.id, user.id)
        .await
        .expect("join the watch room");
    let mut app = make_app(test_db.db.clone(), user.id, "own-chat-f4-flow-it");
    // Wide enough for NetHack's 80 columns beside a 52-column chat.
    app.resize(160, 40).expect("resize test terminal");
    wait_for_app(&mut app, "the room list", |app| {
        app.chat.room_by_id(room.id).is_some()
    })
    .await;
    app.live_games
        .publish_chat_room_for_tests(SpectateGame::Nethack, "tester", room.id);

    // A running NetHack game under a claimed handle, as if launched from the
    // hub. Nothing awaits from here on: the fabricated proxy's bridge task
    // would be polled, fail to connect, and end the game.
    app.set_screen(Screen::Games);
    app.enter_nethack();
    let state = app.nethack_state.as_mut().expect("nethack state");
    state.force_claimed_handle_for_test("tester");
    state.force_running_for_test();
    app.set_screen(Screen::Nethack);
    // One tick makes the player's link and takes its room, ready for F4.
    app.tick();
    assert_eq!(
        app.own_watch_chat_room_id(SpectateGame::Nethack),
        Some(room.id)
    );
    let plain = render_plain(&mut app);
    assert_eq!(app.own_chat_hit.get(), None, "shut while the player plays");
    assert!(!plain.contains("Esc to the game"), "{plain}");
    assert!(
        plain.contains("\u{b7} F4 chat \u{b7} ? help"),
        "the frame title names the key ahead of the game's own: {plain}"
    );

    app.handle_input(b"\x1bOS");
    assert_eq!(app.own_watch_chat_open, Some(SpectateGame::Nethack));
    let plain = render_plain(&mut app);
    assert!(
        plain.contains("0 watching \u{b7} Esc to the game"),
        "{plain}"
    );
    assert!(plain.contains("Compose (press i)"), "{plain}");
    assert!(
        !plain.contains("F4 chat"),
        "no title hint while open: {plain}"
    );
    let pane = app.own_chat_hit.get().expect("the column was drawn");
    assert_eq!(
        pane.width, 52,
        "Realm's width: a third of the 158-column content area"
    );

    // The page and quit keys are the chat's now: nothing leaves the game.
    app.handle_input(b"3");
    app.handle_input(b"q");
    assert_eq!(app.screen, Screen::Nethack);
    assert!(!app.show_quit_confirm, "q is not the global quit here");

    app.handle_input(b"i");
    assert!(app.chat.is_composing(), "i writes");
    assert_eq!(app.chat.composer_room_id(), Some(room.id));
    app.handle_input(b"hi all");
    assert_eq!(app.chat.composer().lines().join("\n"), "hi all");
    app.handle_input(b"\r");
    assert!(!app.chat.is_composing(), "Enter sends");
    assert_eq!(
        app.own_watch_chat_open,
        Some(SpectateGame::Nethack),
        "and the chat keeps the keys"
    );

    flush_lone_escape(&mut app);
    assert_eq!(app.own_watch_chat_open, None, "Esc closes it");
    render_plain(&mut app);
    assert_eq!(app.own_chat_hit.get(), None);
    assert_eq!(app.screen, Screen::Nethack, "back in the game");

    // F4 closes it from the composer, the draft with it.
    app.handle_input(b"\x1b[14~");
    render_plain(&mut app);
    app.handle_input(b"i");
    app.handle_input(b"draft");
    app.handle_input(b"\x1bOS");
    assert_eq!(app.own_watch_chat_open, None);
    assert!(!app.chat.is_composing(), "the draft goes with the chat");

    // The game ending closes an open chat.
    app.handle_input(b"\x1bOS");
    assert_eq!(app.own_watch_chat_open, Some(SpectateGame::Nethack));
    app.leave_nethack();
    app.tick();
    assert_eq!(app.own_watch_chat_open, None, "the game ended");
}

/// Keys typed right behind F4, before a frame has drawn the column, are the
/// chat's as well: in a permadeath game a chat line must never reach the
/// game as commands. And a modal the chat's keys opened (Settings on
/// Ctrl+O) keeps the screen: F4 does not close the chat under it, which
/// would hand the modal's keys to the game.
#[tokio::test]
async fn the_watcher_chat_takes_keys_before_its_first_frame_and_keeps_them_under_a_modal() {
    use crate::app::common::primitives::Screen;
    use crate::app::door::spectate::state::SpectateGame;
    use late_core::models::chat_room::{ChatRoom, WatchDoor};
    use late_core::models::chat_room_member::ChatRoomMember;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "own-chat-f4-batch").await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_watch_room(&client, WatchDoor::Nethack, "batcher")
        .await
        .expect("the player's watch room");
    ChatRoomMember::join(&client, room.id, user.id)
        .await
        .expect("join the watch room");
    let mut app = make_app(test_db.db.clone(), user.id, "own-chat-f4-batch-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_app(&mut app, "the room list", |app| {
        app.chat.room_by_id(room.id).is_some()
    })
    .await;
    app.live_games
        .publish_chat_room_for_tests(SpectateGame::Nethack, "batcher", room.id);
    app.set_screen(Screen::Games);
    app.enter_nethack();
    let state = app.nethack_state.as_mut().expect("nethack state");
    state.force_claimed_handle_for_test("batcher");
    state.force_running_for_test();
    app.set_screen(Screen::Nethack);
    app.tick();
    render_plain(&mut app);

    // F4 and `i` in one input batch: no frame between them.
    app.handle_input(b"OS");
    app.handle_input(b"i");
    assert!(
        app.chat.is_composing(),
        "i is the chat's before the column is drawn"
    );
    assert_eq!(app.chat.composer_room_id(), Some(room.id));
    flush_lone_escape(&mut app);
    assert!(!app.chat.is_composing(), "Esc leaves the composer");
    assert_eq!(
        app.own_watch_chat_open,
        Some(SpectateGame::Nethack),
        "and the chat stays open"
    );
    render_plain(&mut app);

    // Settings opens over the chat; F4 waits for it to close.
    app.handle_input(b"");
    assert!(app.show_settings, "Ctrl+O opens Settings over the chat");
    app.handle_input(b"OS");
    assert!(app.show_settings, "Settings stays");
    assert_eq!(
        app.own_watch_chat_open,
        Some(SpectateGame::Nethack),
        "the chat stays open under the modal"
    );
    // `q` is the modal's: it closes Settings, not the app or the screen.
    app.handle_input(b"q");
    assert!(!app.show_settings, "q closes Settings");
    assert_eq!(app.screen, Screen::Nethack);
    assert!(!app.show_quit_confirm);
    assert_eq!(
        app.own_watch_chat_open,
        Some(SpectateGame::Nethack),
        "the chat outlives the modal"
    );
    app.handle_input(b"OS");
    assert_eq!(app.own_watch_chat_open, None, "then F4 closes the chat");
}

/// Zork's player opens their watchers' chat the same way, and F3, which
/// types a watcher's suggestion, closes it on the way so the story has the
/// keys for Enter.
#[tokio::test]
async fn a_zork_player_opens_their_watchers_chat_with_f4_and_f3_closes_it() {
    use crate::app::common::primitives::Screen;
    use crate::app::door::spectate::state::SpectateGame;
    use late_core::models::chat_room::{ChatRoom, WatchDoor};
    use late_core::models::chat_room_member::ChatRoomMember;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "zork-own-chat-f4").await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_watch_room(&client, WatchDoor::Zork, "tester")
        .await
        .expect("the player's watch room");
    ChatRoomMember::join(&client, room.id, user.id)
        .await
        .expect("join the watch room");
    let mut app = make_app(test_db.db.clone(), user.id, "zork-own-chat-f4-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_app(&mut app, "the room list", |app| {
        app.chat.room_by_id(room.id).is_some()
    })
    .await;
    app.live_games
        .publish_chat_room_for_tests(SpectateGame::Zork, "tester", room.id);

    // A running game under a claimed handle. Nothing awaits from here on, as
    // in the roguelike flow above: the fabricated proxy would end the game.
    app.zork_enabled = true;
    app.set_screen(Screen::Zork);
    app.zork_state
        .as_mut()
        .expect("zork state")
        .force_running_for_test();
    app.tick();

    app.handle_input(b"\x1bOS");
    let plain = render_plain(&mut app);
    assert!(plain.contains("F3 takes a >suggestion"), "{plain}");
    app.handle_input(b"i");
    assert_eq!(app.chat.composer_room_id(), Some(room.id), "i writes");
    app.handle_input(b"hi all");
    assert_eq!(
        app.chat.composer().lines().join("\n"),
        "hi all",
        "typed keys are the composer's, not the story's"
    );
    app.handle_input(b"\r");
    assert!(!app.chat.is_composing(), "Enter sends");

    app.handle_input(b"\x1bOR");
    assert_eq!(app.own_watch_chat_open, None, "F3 closes the chat");
    assert_eq!(app.screen, Screen::Zork);
}

/// A player keeps the whole screen and still knows what is going on around
/// their game: with the chat shut the frame title says who is watching and
/// what they said. Nobody watching is no word at all, the open chat's header
/// takes the count over, and a watcher coming or going is a frame even on a
/// game sitting still (nothing else signals it).
#[tokio::test]
async fn the_title_tells_a_player_who_is_watching_and_what_is_new_with_the_chat_shut() {
    use crate::app::common::primitives::Screen;
    use crate::app::door::spectate::state::{LiveGameKey, SpectateGame};
    use late_core::models::chat_room::{ChatRoom, WatchDoor};
    use late_core::models::chat_room_member::ChatRoomMember;

    // The render loop's frame path: a dirty tick draws and drains its queued
    // terminal commands, which keep the gate dirty until they go.
    let settle = |app: &mut crate::app::state::App| {
        for _ in 0..50 {
            if !app.tick() {
                return;
            }
            render_plain(app);
            let _ = std::mem::take(&mut app.pending_terminal_commands);
        }
        panic!("the session never settled");
    };

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "own-title").await;
    let first_watcher = create_test_user(&test_db.db, "own-title-first").await;
    let second_watcher = create_test_user(&test_db.db, "own-title-second").await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_watch_room(&client, WatchDoor::Nethack, "tester")
        .await
        .expect("the player's watch room");
    ChatRoomMember::join(&client, room.id, user.id)
        .await
        .expect("join the watch room");
    let mut app = make_app(test_db.db.clone(), user.id, "own-title-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_app(&mut app, "the room list", |app| {
        app.chat.room_by_id(room.id).is_some()
    })
    .await;
    app.live_games
        .publish_chat_room_for_tests(SpectateGame::Nethack, "tester", room.id);

    // A watcher talks, and the room's unread count comes in the way it does
    // for every room, on the session's next chat snapshot.
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: first_watcher.id,
            body: "nice dagger".to_string(),
        },
    )
    .await
    .expect("the watcher's message");
    app.chat.request_list();
    wait_for_app(&mut app, "the unread count", |app| {
        app.chat.unread_counts.get(&room.id) == Some(&1)
    })
    .await;

    // A running game under a claimed handle. Nothing awaits from here on, as
    // in the flows above: the fabricated proxy would end the game.
    app.set_screen(Screen::Games);
    app.enter_nethack();
    let state = app.nethack_state.as_mut().expect("nethack state");
    state.force_claimed_handle_for_test("tester");
    state.force_running_for_test();
    app.set_screen(Screen::Nethack);
    app.tick();

    let plain = render_plain(&mut app);
    assert!(
        plain.contains("\u{b7} F4 chat (1 new) \u{b7} ? help"),
        "what the watchers said, with the chat shut: {plain}"
    );
    assert!(!plain.contains("watching"), "nobody, no word: {plain}");
    settle(&mut app);

    let key = LiveGameKey::new(SpectateGame::Nethack, "tester").expect("a handle");
    let first_watch = app.live_games.open_watch(key, first_watcher.id);
    assert!(app.tick(), "a watcher coming is a frame on a still game");
    let plain = render_plain(&mut app);
    assert!(
        plain.contains("\u{b7} 1 watching \u{b7} F4 chat (1 new) \u{b7} ? help"),
        "{plain}"
    );
    settle(&mut app);

    let second_watch = app.live_games.open_watch(key, second_watcher.id);
    assert!(app.tick(), "and so is a second one");
    let plain = render_plain(&mut app);
    assert!(
        plain.contains("\u{b7} 2 watching \u{b7} F4 chat"),
        "{plain}"
    );
    settle(&mut app);

    // F4 up: the chat's header has the count, and the room reads.
    app.handle_input(b"\x1bOS");
    app.tick();
    let plain = render_plain(&mut app);
    assert!(
        plain.contains("2 watching \u{b7} Esc to the game"),
        "{plain}"
    );
    assert!(!plain.contains("\u{b7} 2 watching"), "{plain}");

    // F4 down: back in the title, with nothing new left to count.
    app.handle_input(b"\x1bOS");
    app.tick();
    let plain = render_plain(&mut app);
    assert!(
        plain.contains("\u{b7} 2 watching \u{b7} F4 chat \u{b7} ? help"),
        "{plain}"
    );
    settle(&mut app);

    drop(first_watch);
    assert!(app.tick(), "a watcher leaving is a frame too");
    let plain = render_plain(&mut app);
    assert!(
        plain.contains("\u{b7} 1 watching \u{b7} F4 chat"),
        "{plain}"
    );
    settle(&mut app);

    drop(second_watch);
    assert!(app.tick(), "the last one leaving as well");
    let plain = render_plain(&mut app);
    assert!(!plain.contains("watching"), "{plain}");
}

/// The title's word is written per door, so each door is held to it.
#[tokio::test]
async fn every_watchable_door_says_who_is_watching_in_its_title() {
    use crate::app::common::primitives::Screen;
    use crate::app::door::spectate::state::{LiveGameKey, SpectateGame};

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "door-titles").await;
    let mut app = make_app(test_db.db.clone(), user.id, "door-titles-flow-it");
    app.resize(160, 40).expect("resize test terminal");

    // Running games under a claimed handle. Nothing awaits from here on, as
    // in the flows above: the fabricated proxies would end the games.
    app.set_screen(Screen::Games);
    app.enter_nethack();
    let state = app.nethack_state.as_mut().expect("nethack state");
    state.force_claimed_handle_for_test("tester");
    state.force_running_for_test();
    app.enter_dcss();
    let state = app.dcss_state.as_mut().expect("dcss state");
    state.force_claimed_handle_for_test("tester");
    state.force_running_for_test();
    app.enter_brogue();
    let state = app.brogue_state.as_mut().expect("brogue state");
    state.force_claimed_handle_for_test("tester");
    state.force_running_for_test();
    app.zork_enabled = true;
    app.enter_zork();
    app.zork_state
        .as_mut()
        .expect("zork state")
        .force_running_for_test();

    let watcher = uuid::Uuid::now_v7();
    for (game, screen) in [
        (SpectateGame::Nethack, Screen::Nethack),
        (SpectateGame::Dcss, Screen::Dcss),
        (SpectateGame::Brogue, Screen::Brogue),
        (SpectateGame::Zork, Screen::Zork),
    ] {
        app.set_screen(screen);
        let plain = render_plain(&mut app);
        assert!(!plain.contains("watching"), "{screen:?}: {plain}");

        let key = LiveGameKey::new(game, "tester").expect("a handle");
        let _watch = app.live_games.open_watch(key, watcher);
        let plain = render_plain(&mut app);
        assert!(
            plain.contains("\u{b7} 1 watching \u{b7} "),
            "{screen:?}: {plain}"
        );
    }
}

#[tokio::test]
async fn arranging_home_keeps_the_chat_keys_off_a_tile_that_is_not_a_chat() {
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
    load_zen_fixture(&mut app);
    wait_for_render_contains(&mut app, "bonsai").await;

    // The layout opens on its chat tile: `j` selects in its room.
    assert_eq!(
        app.home.focused_kind(),
        Some(crate::app::zen::state::TileKind::Chat)
    );
    app.handle_input(b"j");
    assert_eq!(
        app.chat.selected_message_id,
        Some(message.id),
        "j on the focused chat tile selects the newest message"
    );

    // Arranging, the focus moves off the chat: the selection is dropped
    // and `j` is swallowed rather than scrolling a chat nobody is looking
    // at.
    app.handle_input(b"\x05");
    assert!(app.home.arrange);
    app.handle_input(b"\x1b[D");
    assert_ne!(
        app.home.focused_kind(),
        Some(crate::app::zen::state::TileKind::Chat)
    );
    assert_eq!(app.chat.selected_message_id, None);
    app.handle_input(b"j");
    assert_eq!(
        app.chat.selected_message_id, None,
        "j with the bonsai focused selects nothing"
    );

    // `i` composes nothing while arranging; done, the focus is back on the
    // chat tile and `i` composes in its room.
    app.handle_input(b"i");
    assert!(
        !app.chat.composing,
        "i with the bonsai focused composes nothing"
    );
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.home.arrange, "arrange mode").await;
    assert_eq!(
        app.home.focused_kind(),
        Some(crate::app::zen::state::TileKind::Chat)
    );
    app.handle_input(b"i");
    assert!(app.chat.composing, "i on the focused chat tile composes");
}

/// Arrange mode swallows stray letters, but the ones the titles name are
/// the house's (`HouseKey`): `w`, which the bonsai's title names, opens
/// the care modal from the mode, as the title says.
#[tokio::test]
async fn arranging_home_lets_the_keys_the_titles_name_through() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "zen-house-keys-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    let mut app = make_app(test_db.db.clone(), viewer.id, "zen-house-keys-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "lounge").await;

    app.handle_input(b"\x05");
    assert!(app.home.arrange);
    app.handle_input(b"x");
    assert!(!app.show_bonsai_modal, "a stray letter opens nothing");
    app.handle_input(b"w");
    assert!(
        app.show_bonsai_modal,
        "w opens the care modal while arranging"
    );
}

#[tokio::test]
async fn home_s_then_a_digit_opens_the_live_tiles_row() {
    use crate::app::zen::state::TileKind;
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
    load_zen_fixture(&mut app);
    wait_for_render_contains(&mut app, "bonsai").await;

    // The zen layout's Live tile shows the shared link.
    assert!(app.home.shows(TileKind::Live));
    wait_for_render_contains(&mut app, "news    The terminal renaissance").await;

    app.handle_input(b"sx");
    assert_eq!(
        app.chat.reader().map(|reader| reader.url()),
        None,
        "a stray suffix opens nothing"
    );
    assert!(!app.live_prefix_armed, "and disarms the prefix");

    app.handle_input(b"s1");
    assert_eq!(
        app.chat.reader().map(|reader| reader.url()),
        Some("https://example.com/terminal-renaissance"),
        "s1 opens the article"
    );
    assert!(!app.chat.is_composing(), "the key never reaches a composer");
}

#[tokio::test]
async fn home_clicks_under_the_open_tile_picker_reach_nothing() {
    use crate::app::zen::state::TileKind;
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
    load_zen_fixture(&mut app);
    wait_for_render_contains(&mut app, "bonsai").await;

    app.handle_input(b"\x05");
    assert!(app.home.arrange);
    app.home.focus = app
        .home
        .first_tile_of(TileKind::Live)
        .expect("the default has a live tile");
    wait_for_render_contains(&mut app, "Under the picker").await;
    let live = app.home.focus;

    // The picker is up over the page. The Live tile under it still records
    // its rows' click rect, but a click there belongs to the picker:
    // nothing opens and the focus stays put, so the picker converts the
    // tile it opened on.
    app.handle_input(b" ");
    assert!(app.home.kind_picker.is_some(), "space opens the picker");
    render_plain(&mut app);
    let (rows, _) = app
        .live
        .panel_hit
        .get()
        .expect("the live tile drew its rows");
    // The first row, where the article is.
    let click = format!("\x1b[<0;{};{}M", rows.x + 2, rows.y + 1);
    app.handle_input(click.as_bytes());
    assert_eq!(
        app.chat.reader().map(|reader| reader.url()),
        None,
        "a click under the picker opens nothing"
    );
    assert!(app.home.kind_picker.is_some(), "the picker stays up");
    assert_eq!(app.home.focus, live, "the focus stays under the picker");

    // With the picker closed the same click opens the article.
    app.home.close_kind_picker();
    render_plain(&mut app);
    app.handle_input(click.as_bytes());
    assert_eq!(
        app.chat.reader().map(|reader| reader.url()),
        Some("https://example.com/under-the-picker"),
        "the click opens the article once the picker is gone"
    );
}

#[tokio::test]
async fn home_a_draft_stays_in_its_room_when_the_focus_moves() {
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
    load_zen_fixture(&mut app);
    wait_for_render_contains(&mut app, "bonsai").await;

    // A second chat tile beside the zen layout's, bound to the second
    // room; the first is bound to #lounge.
    let first = app
        .home
        .first_tile_of(TileKind::Chat)
        .expect("the default has a chat");
    assert_eq!(app.home.focus, first);
    assert!(crate::app::zen::input::bind_active_chat_to_room(
        &mut app, lounge.id
    ));
    assert!(app.home.split_focused(true));
    let second = app.home.focus;
    assert!(app.home.rice.root.set_kind(second, TileKind::Chat));
    assert!(crate::app::zen::input::bind_active_chat_to_room(
        &mut app, quiet.id
    ));

    // A draft written in the second tile, then a click on the first: the
    // draft is closed rather than carried under #lounge, where Enter would
    // have posted it to the room it was written for.
    app.handle_input(b"i");
    app.handle_input(b"secret");
    assert!(app.chat.composing);
    assert_eq!(app.chat.composer_room_id(), Some(quiet.id));
    render_plain(&mut app);
    let (_, rect, _) = app.home_tile_rects()[first];
    let click = format!(
        "\x1b[<0;{};{}M",
        rect.x + rect.width / 2 + 1,
        rect.y + rect.height / 2 + 1
    );
    app.handle_input(click.as_bytes());
    assert_eq!(
        app.home.focus, first,
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
}

/// The zen fixture's chat bound to #lounge and a second chat tile split from
/// it bound to another room, the focus on the second: the page's furniture
/// sits after both, so walking off the end of them has to skip it.
async fn two_chat_tiles(
    test_db: &late_core::test_utils::TestDb,
    name: &str,
) -> (crate::app::state::App, Uuid, Uuid, usize, usize) {
    use crate::app::zen::state::TileKind;

    let viewer = create_test_user(&test_db.db, name).await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    let quiet = ChatRoom::get_or_create_public_room(&client, "zen-hop-quiet")
        .await
        .expect("second room");
    ChatRoomMember::join(&client, quiet.id, viewer.id)
        .await
        .expect("join second room");
    let mut app = make_app(test_db.db.clone(), viewer.id, name);
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "zen-hop-quiet").await;
    load_zen_fixture(&mut app);
    wait_for_render_contains(&mut app, "bonsai").await;
    let first = app
        .home
        .first_tile_of(TileKind::Chat)
        .expect("the fixture has a chat");
    assert!(crate::app::zen::input::bind_active_chat_to_room(
        &mut app, lounge.id
    ));
    assert!(app.home.split_focused(true));
    let second = app.home.focus;
    assert!(app.home.rice.root.set_kind(second, TileKind::Chat));
    assert!(crate::app::zen::input::bind_active_chat_to_room(
        &mut app, quiet.id
    ));
    (app, lounge.id, quiet.id, first, second)
}

#[tokio::test]
async fn alt_arrows_and_alt_j_k_hop_the_chat_tiles_and_a_composer_keeps_them() {
    let test_db = new_test_db().await;
    let (mut app, lounge, quiet, first, second) = two_chat_tiles(&test_db, "zen-hop-it").await;

    // Right off the last chat wraps past the furniture to the first, and
    // left comes back. The rail goes with the focus; the bindings stay.
    app.handle_input(b"\x1b[1;3C");
    assert_eq!(app.home.focus, first);
    assert_eq!(app.chat.selected_room_id, Some(lounge));
    app.handle_input(b"\x1b[1;3D");
    assert_eq!(app.home.focus, second);
    assert_eq!(app.chat.selected_room_id, Some(quiet));
    assert_eq!(
        app.home.chat_tiles(),
        vec![
            (first, ChatRoomBinding::Room(lounge)),
            (second, ChatRoomBinding::Room(quiet))
        ]
    );

    // Alt+J and Alt+K are the same walk: next and previous.
    app.handle_input(b"\x1bj");
    assert_eq!(app.home.focus, first);
    assert_eq!(app.chat.selected_room_id, Some(lounge));
    app.handle_input(b"\x1bk");
    assert_eq!(app.home.focus, second);
    assert_eq!(app.chat.selected_room_id, Some(quiet));

    // In a draft the arrows are the word jumps and Alt+J/K do nothing: the
    // focus and the draft stay.
    app.handle_input(b"i");
    app.handle_input(b"one two");
    app.handle_input(b"\x1b[1;3D");
    app.handle_input(b"\x1bj");
    app.handle_input(b"\x1bk");
    assert_eq!(app.home.focus, second);
    assert!(app.chat.composing);
    app.handle_input(b"X");
    assert_eq!(app.chat.composer().lines(), ["one Xtwo"]);
    app.chat.reset_composer();

    // Mid room jump they leave the jump's labels on the tile it began on.
    app.handle_input(b" ");
    app.handle_input(b"\x1b[1;3C");
    assert_eq!(app.home.focus, second);
    assert!(app.chat.room_jump_active);
}

#[tokio::test]
async fn a_room_named_on_another_tile_moves_the_focus_there_instead_of_showing_it_twice() {
    use crate::app::chat::state::RoomSlot;

    let test_db = new_test_db().await;
    let (mut app, lounge, quiet, first, second) = two_chat_tiles(&test_db, "zen-pick-it").await;
    let both = vec![
        (first, ChatRoomBinding::Room(lounge)),
        (second, ChatRoomBinding::Room(quiet)),
    ];

    // `Space` and the room's letter.
    let key = app
        .chat
        .room_jump_targets()
        .into_iter()
        .find_map(|(key, slot)| (slot == RoomSlot::Room(lounge)).then_some(key))
        .expect("#lounge has a jump key");
    app.handle_input(b" ");
    app.handle_input(&[key]);
    assert_eq!(app.home.focus, first);
    assert_eq!(app.chat.selected_room_id, Some(lounge));
    assert_eq!(app.home.chat_tiles(), both);

    // The `Ctrl+/` picker's pick.
    crate::app::zen::input::pick_for_active_chat(&mut app, RoomSlot::Room(quiet));
    assert_eq!(app.home.focus, second);
    assert_eq!(app.chat.selected_room_id, Some(quiet));
    assert_eq!(app.home.chat_tiles(), both);

    // Arranging, a pick is a layout edit: the focused tile takes the room.
    app.handle_input(b"\x05");
    crate::app::zen::input::pick_for_active_chat(&mut app, RoomSlot::Room(lounge));
    assert_eq!(app.home.focus, second);
    assert_eq!(
        app.home.chat_tiles(),
        vec![
            (first, ChatRoomBinding::Room(lounge)),
            (second, ChatRoomBinding::Room(lounge))
        ]
    );
}

/// The first frame comes before the rooms do: a chat tile with no room to
/// name says so rather than claiming #lounge, and keys sent on the strength
/// of that name would land nowhere.
#[tokio::test]
async fn a_chat_tile_names_no_room_until_the_rooms_load() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "chat-tile-label").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge");
    let mut app = make_app(test_db.db.clone(), user.id, "chat-tile-label-flow-it");

    // No tick yet, so no snapshot has been taken in.
    assert!(app.chat.selected_room_id.is_none());
    let first = String::from_utf8_lossy(&app.render().expect("render")).into_owned();
    assert!(!first.contains("#lounge"), "frame={first:?}");

    wait_for_render_contains(&mut app, "chat · #lounge").await;
    assert_eq!(app.chat.selected_room_id, Some(lounge.id));
}

#[tokio::test]
async fn home_pet_tile_is_petted_without_moving_the_focus() {
    use crate::app::hub::shop::{
        entitlements::ShopEntitlements, state::ShopState, svc::ShopSnapshot,
    };
    use crate::app::zen::state::{Dir, Look, Node, RiceLayout, TileKind, ZenState};
    use late_core::models::marketplace::PET_COMPANION_SKU;
    use late_core::models::pet::PetMood;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "rail-pet-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");

    let (cols, rows) = (160u16, 40u16);
    let mut app = make_app(test_db.db.clone(), viewer.id, "rail-pet-flow-it");
    app.shop_state = ShopState::for_test_snapshot(ShopSnapshot {
        entitlements: ShopEntitlements::from_owned_skus([PET_COMPANION_SKU.to_string()]),
        ..Default::default()
    });
    // A Home arranged with the pet alone beside the chat, as its owner
    // would with Ctrl+E.
    let arranged = RiceLayout {
        root: Node::split_first(
            Dir::Row,
            23,
            Node::leaf(TileKind::Rooms),
            Node::split_second(
                Dir::Row,
                23,
                Node::leaf(TileKind::Chat),
                Node::leaf(TileKind::Pet),
            ),
        ),
        look: Look::default(),
    };
    app.home = ZenState::new(arranged.clone(), Some(arranged));
    app.resize(cols, rows).expect("resize test terminal");
    wait_for_render_contains(&mut app, "╭ pet").await;

    // The pet sits on Home's rail in a tile of its own. Petting it is a
    // passing gesture: the keys stay with the chat.
    let center = app
        .home
        .first_tile_of(TileKind::Chat)
        .expect("the stock Home has a chat");
    assert_eq!(app.home.focus, center, "Home opens on its chat");
    let pet = app.last_pet_rect.get().expect("the pet drew on its tile");
    let click = format!("\x1b[<0;{};{}M", pet.x + 1, pet.y + 1);
    app.handle_input(click.as_bytes());
    assert_eq!(
        app.home.focus, center,
        "petting the pet leaves the focus on the center"
    );
    render_plain(&mut app);
    assert_eq!(
        app.pet_state.mood(),
        PetMood::Purring,
        "the click landed on the pet"
    );
}

#[tokio::test]
async fn a_pet_put_away_leaves_its_tile_and_stops_asking_for_care() {
    use crate::app::hub::shop::{
        entitlements::ShopEntitlements, state::ShopState, svc::ShopSnapshot,
    };
    use crate::app::zen::attention::{Chore, Item};
    use crate::app::zen::state::{Dir, Guest, Look, Node, RiceLayout, TileKind, ZenState};
    use late_core::models::marketplace::PET_COMPANION_SKU;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "pet-away-viewer").await;
    let mut app = make_app(test_db.db.clone(), viewer.id, "pet-away-flow-it");
    app.shop_state = ShopState::for_test_snapshot(ShopSnapshot {
        entitlements: ShopEntitlements::from_owned_skus([PET_COMPANION_SKU.to_string()]),
        ..Default::default()
    });
    let arranged = RiceLayout {
        root: Node::split_second(
            Dir::Row,
            23,
            Node::hosting(TileKind::Chat, &[Guest::Attention]),
            Node::leaf(TileKind::Pet),
        ),
        look: Look::default(),
    };
    app.home = ZenState::new(arranged.clone(), Some(arranged));
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "╭ pet").await;

    assert!(app.last_pet_rect.get().is_some(), "the pet drew");
    assert_eq!(app.care_today().pet, Chore::Due);
    assert!(app.attention_items().contains(&Item::PetPet));

    app.pet_state.set_enabled(false);
    let frame = render_plain(&mut app);
    assert!(app.last_pet_rect.get().is_none(), "no click target is left");
    assert!(frame.contains("no pet out"), "the tile says so: {frame}");
    assert_eq!(app.care_today().pet, Chore::NotOwned);
    assert!(!app.attention_items().contains(&Item::PetPet));

    app.pet_state.set_enabled(true);
    render_plain(&mut app);
    assert!(app.last_pet_rect.get().is_some(), "the pet is back");
    assert!(app.attention_items().contains(&Item::PetPet));
}

/// The stock Home hosts a pet tile, and a pet put away leaves it holding a
/// note, so the Shop's answer to `o` must not promise the tile is gone.
#[tokio::test]
async fn putting_the_pet_away_on_the_stock_home_says_what_the_home_then_shows() {
    use crate::app::hub::shop::{catalog::ShopCategory, state::ShopState, svc::ShopService};
    use crate::app::zen::state::{RiceLayout, ZenState};
    use late_core::models::chips::UserChips;
    use late_core::models::marketplace::{PET_COMPANION_SKU, purchase_durable_item_by_sku};

    let test_db = new_test_db().await;
    let owner = create_test_user(&test_db.db, "pet-away-stock").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::admin_grant(&**client, owner.id, 1_000_000)
        .await
        .expect("fund chips");
    purchase_durable_item_by_sku(&mut client, owner.id, PET_COMPANION_SKU)
        .await
        .expect("buy the pet");
    let mut app = make_app(test_db.db.clone(), owner.id, "pet-away-stock-flow-it");
    app.resize(160, 40).expect("resize test terminal");
    let stock = RiceLayout::home();
    app.home = ZenState::new(stock.clone(), Some(stock));
    // The catalog and the purchase as the session's bootstrap loads them.
    app.shop_state = ShopState::for_test_snapshot(
        ShopService::new(test_db.db.clone())
            .refresh_user(owner.id)
            .await
            .expect("the shop's snapshot"),
    );

    app.handle_input(b"\x13");
    assert!(app.show_hub_modal, "Ctrl+S opens the Shop");
    app.shop_state.select_category(ShopCategory::Companions);
    let pet_row = app
        .shop_state
        .visible_items()
        .iter()
        .position(|item| item.is_pet_companion())
        .expect("the pet's row");
    app.shop_state.select_item(pet_row);
    app.handle_input(b"o");
    assert!(!app.pet_out(), "o puts it away");
    let answer = app
        .banner
        .as_ref()
        .expect("the switch answers")
        .message
        .clone();

    app.show_hub_modal = false;
    let frame = render_plain(&mut app);
    assert!(frame.contains("no pet out"), "the tile stays: {frame}");
    assert!(
        !answer.contains("no tile"),
        "the answer agrees with the tile: {answer}"
    );
}

#[tokio::test]
async fn home_tank_tile_is_fed_by_a_click_without_moving_the_focus() {
    use crate::app::zen::state::{Dir, Look, Node, RiceLayout, TileKind, ZenState};

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "rail-tank-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");

    let (cols, rows) = (160u16, 40u16);
    let mut app = make_app(test_db.db.clone(), viewer.id, "rail-tank-flow-it");
    // A Home arranged with the tank alone beside the chat.
    let arranged = RiceLayout {
        root: Node::split_first(
            Dir::Row,
            23,
            Node::leaf(TileKind::Rooms),
            Node::split_second(
                Dir::Row,
                23,
                Node::leaf(TileKind::Chat),
                Node::leaf(TileKind::Tank),
            ),
        ),
        look: Look::default(),
    };
    app.home = ZenState::new(arranged.clone(), Some(arranged));
    app.resize(cols, rows).expect("resize test terminal");
    wait_for_render_contains(&mut app, "╭ tank").await;
    assert!(app.aquarium_care.hungry(), "a fresh tank is unfed");

    // Feeding is a passing gesture, the pet's way: the keys stay with the
    // chat and the tank has eaten.
    let center = app
        .home
        .first_tile_of(TileKind::Chat)
        .expect("the page has a chat");
    assert_eq!(app.home.focus, center, "Home opens on its chat");
    let (_, tank, _) = app
        .home_tile_rects()
        .into_iter()
        .find(|(kind, _, _)| *kind == TileKind::Tank)
        .expect("the tank drew on its tile");
    let click = format!("\x1b[<0;{};{}M", tank.x + 2, tank.y + 2);
    app.handle_input(click.as_bytes());
    assert_eq!(
        app.home.focus, center,
        "feeding the tank leaves the focus on the center"
    );
    assert!(!app.aquarium_care.hungry(), "the click fed the tank");

    // A second click is the same as a second `a`: the tank already ate.
    app.handle_input(click.as_bytes());
    assert_eq!(
        app.banner.as_ref().map(|banner| banner.message.as_str()),
        Some("The tank already ate today")
    );
}

#[tokio::test]
async fn home_every_chat_tile_keeps_its_composer_whatever_is_focused() {
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
    load_zen_fixture(&mut app);
    wait_for_render_contains(&mut app, "bonsai").await;

    let first = app
        .home
        .first_tile_of(TileKind::Chat)
        .expect("the default has a chat");
    assert!(app.home.split_focused(true));
    let second = app.home.focus;
    assert!(app.home.rice.root.set_kind(second, TileKind::Chat));
    assert!(crate::app::zen::input::bind_active_chat_to_room(
        &mut app, quiet.id
    ));

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

    app.home.focus = first;
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

/// `[` `]` on a chat tile in arrange mode walk the rooms in the rail's
/// order, top to bottom, not the order they were loaded in: Core's fixed order puts
/// suggestions before bugs, though bugs loads first alphabetically.
#[tokio::test]
async fn home_arrange_brackets_cycle_rooms_in_rail_order() {
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
    load_zen_fixture(&mut app);
    wait_for_render_contains(&mut app, "bonsai").await;
    app.handle_input(b"\x05");
    assert!(app.home.arrange);
    assert_eq!(app.home.focused_kind(), Some(TileKind::Chat));
    assert_eq!(app.home_chat_room_id(), Some(lounge));

    // The rail runs #lounge, News, then the other permanent rooms: `]`
    // stops on News like a room, the tile holds it and the rail shows it.
    app.handle_input(b"]");
    assert_eq!(app.home.active_chat_room(), Some(ChatRoomBinding::News));
    assert!(
        app.chat.news_selected,
        "the rail follows the tile onto News"
    );
    assert_eq!(app.home_chat_room_id(), None, "no room to act on");

    let mut forward = Vec::new();
    for _ in 0..4 {
        app.handle_input(b"]");
        forward.push(app.home_chat_room_id().expect("bound room"));
    }
    assert_eq!(forward, vec![suggestions, bugs, channel, lounge]);
    assert!(!app.chat.news_selected);

    app.handle_input(b"[");
    assert_eq!(
        app.home_chat_room_id(),
        Some(channel),
        "[ wraps to the bottom"
    );
}

#[tokio::test]
async fn slash_picker_opens_the_room_picker_and_the_pick_moves_homes_chat() {
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
    load_zen_fixture(&mut app);
    wait_for_render_contains(&mut app, "bonsai").await;
    assert_eq!(app.home.focused_kind(), Some(TileKind::Chat));
    assert_eq!(
        app.home.active_chat_room(),
        Some(ChatRoomBinding::Current),
        "the zen fixture's chat tile follows Home's selection"
    );

    // `/picker` from the tile's composer opens the same modal as Ctrl+/.
    app.handle_input(b"i/picker\r");
    assert!(
        app.room_search_modal_state.is_open(),
        "/picker opens the room picker"
    );

    // The pick moves the rail, and the page's chat shows the room.
    app.handle_input(b"zen-picked\r");
    assert!(
        !app.room_search_modal_state.is_open(),
        "the pick closes the picker"
    );
    assert_eq!(app.screen, Screen::Dashboard, "the pick stays on Home");
    assert_eq!(app.home_chat_room_id(), Some(quiet.id));
    assert_eq!(
        app.chat.selected_room_id,
        Some(quiet.id),
        "the rail follows the active chat tile; it was on {home_selection:?}"
    );
}

#[tokio::test]
async fn home_arrange_space_opens_a_tile_picker_that_owns_the_keys_until_a_pick_or_esc() {
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
    load_zen_fixture(&mut app);
    wait_for_render_contains(&mut app, "bonsai").await;
    app.handle_input(b"\x05");
    assert!(app.home.arrange);
    assert_eq!(app.home.focused_kind(), Some(TileKind::Chat));
    let tiles = app.home.leaf_count();

    // Space opens the list on the tile's own kind, and names it.
    app.handle_input(b" ");
    assert_eq!(app.home.kind_picker_selection(), Some(TileKind::Chat));
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
        app.home.leaf_count(),
        tiles,
        "S under the picker splits nothing"
    );
    assert!(!app.show_quit_confirm, "q under the picker quits nothing");
    assert!(app.home.kind_picker.is_some());

    // One row down and Enter: the tile is a clock, the picker is gone.
    app.handle_input(b"j");
    app.handle_input(b"\r");
    assert!(app.home.kind_picker.is_none(), "a pick closes the picker");
    assert_eq!(app.home.focused_kind(), Some(TileKind::Clock));
    assert_eq!(app.screen, Screen::Dashboard);

    // Esc closes it without a change and stays on the page.
    app.handle_input(b" ");
    app.handle_input(b"k");
    app.handle_input(b"\x1b");
    wait_for_esc_effect(
        &mut app,
        |app| app.home.kind_picker.is_none(),
        "esc closes the tile picker",
    )
    .await;
    assert_eq!(app.home.focused_kind(), Some(TileKind::Clock));
    assert_eq!(app.screen, Screen::Dashboard);
    assert!(app.home.arrange, "Esc under the picker keeps the mode");
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

    // Core order is lounge, news, then bugs (no RSS without feeds): two
    // steps right land on it.
    app.handle_input(b"ll");
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

    // Core order here is lounge, news, then mentions: two steps right land
    // on it.
    app.handle_input(b"ll");
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

/// A game room is on the rail only while it is a favorite. Unfavoriting the
/// one open on Home moves the selection to a room the rail still lists
/// right away, so the next draft goes to the room on screen and not to the
/// game chat that just left it.
#[tokio::test]
async fn unfavoriting_the_open_game_room_moves_the_draft_with_the_screen() {
    use late_core::models::game_room::GameKind;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-unfav-game").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    let game = ChatRoom::get_or_create_game_room(&client, GameKind::Poker, "unfav-open")
        .await
        .expect("game room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join lounge");
    ChatRoomMember::join(&client, game.id, viewer.id)
        .await
        .expect("join game room");

    let mut app = make_app(test_db.db.clone(), viewer.id, "f-unfav-game-flow-it");
    app.resize(160, 32).expect("resize test terminal");
    wait_for_render_contains(&mut app, "\u{258C}lounge").await;

    // `/fav` typed in the game's own chat is what puts it on the rail.
    app.chat.start_composing_in_room(game.id);
    app.handle_input(b"/fav\r");
    wait_for_render_contains(&mut app, "Added to favorites").await;
    wait_for_render_contains(&mut app, "favorites").await;

    // Favorites sit above Core: one step left of lounge is the game room.
    app.handle_input(b"h");
    assert_eq!(app.chat.selected_room_id, Some(game.id));

    // No tick between the two keys: no snapshot gets to repair anything.
    app.handle_input(b"f");
    app.handle_input(b"i");
    assert!(!app.chat.favorite_room_ids().contains(&game.id));
    assert_eq!(app.chat.selected_room_id, Some(lounge.id));
    assert_eq!(app.chat.composer_room_id(), Some(lounge.id));
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
    // Tweaks rows: background, brightness, composer, interaction mode, plain
    // glyphs, terminal images, then Chat badges.
    app.handle_input(b"jjjjjj\r");
    // The heading fits the dialog whole, not cut at its border.
    wait_for_render_contains(&mut app, "Earn it, hide it. Games show their top badge.").await;
    wait_for_render_contains(&mut app, "LMG LKN LYS LKA").await;

    // Picker rows in label order: the eight monthly rows, then Lateania.
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
    app.handle_input(b"\r");
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
    app.handle_input(b"\r");
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
/// both, as over the picker and the scene. `q` and the page digits are
/// the globals' over both.
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

    // `q` asks to quit, and pours nothing: the glass below is the day's one.
    app.handle_input(b"q");
    assert!(app.show_quit_confirm, "`q` at the bar is the quit");
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.show_quit_confirm, "quit confirm").await;

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

/// At INK a page digit is the page switch, as over every panel: `2` goes
/// to the Arcade, and no draft is inked for it.
#[tokio::test]
async fn the_needle_leaves_the_page_digits_to_the_globals() {
    use crate::app::common::primitives::Screen;
    use crate::app::deadchannel::city::map::Landmark;
    use late_core::models::deadchannel_runner::DeadchannelRunner;

    let (test_db, mut app, user_id) =
        runner_on_the_street("undercity-needle-digits-it", 100, 30, |sheet| {
            sheet.crystals = 20
        })
        .await;

    app.city.open_panel(Landmark::Ink);
    app.fight.clear_till();
    wait_for_render_contains(&mut app, " Esc closes ").await;
    app.handle_input(b"2");
    assert_eq!(
        app.screen,
        Screen::Arcade,
        "`2` at the needle is the Arcade"
    );

    let client = test_db.db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(row.inks, None, "nothing inked");
    assert_eq!(row.crystals, 20, "no crystals spent");
}

/// At INK the down key walks the cursor to the next draft and `i` inks
/// that one onto the row, for its crystals.
#[tokio::test]
async fn the_needle_inks_the_picked_draft() {
    use crate::app::deadchannel::city::map::Landmark;
    use crate::app::deadchannel::fight::cards::Card;
    use late_core::models::deadchannel_runner::DeadchannelRunner;

    let (test_db, mut app, user_id) =
        runner_on_the_street("undercity-needle-ink-it", 100, 30, |sheet| {
            // The needle only inks a draft this climb has reached, and with
            // both picks taken no draft is owed.
            sheet.level = 6;
            sheet.peak_level = 6;
            sheet.cards = vec![Card::Jab, Card::Riposte];
            sheet.crystals = 20;
        })
        .await;

    app.city.open_panel(Landmark::Ink);
    app.fight.clear_till();
    wait_for_render_contains(&mut app, " Esc closes ").await;
    app.handle_input(b"j");
    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "the needle goes in.").await;

    let client = test_db.db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(
        row.inks,
        Some(serde_json::json!([6])),
        "the level 6 draft, under the cursor"
    );
    assert_eq!(row.crystals, 10, "ten crystals for the needle");
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
        "[Enter] fight",
        "hiss  lv 2",
        "▸ [▚]",
        "[g] step down, half pay",
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

fn screen5_click(app: &mut crate::app::state::App, rect: ratatui::layout::Rect) {
    app.handle_input(format!("\x1b[<0;{};{}M", rect.x + 1, rect.y + 1).as_bytes());
}

#[tokio::test]
async fn profiles_mouse_scroll_and_click_preserve_routing_and_narrow_back() {
    use crate::app::{
        common::primitives::Screen,
        directory::{
            mouse::{Pane, Target},
            state::Shelf,
        },
    };
    use late_core::models::user::InteractionMode;
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "profiles-mouse-jobs").await;
    let mut app = make_app(db.db.clone(), user.id, "profiles-mouse-jobs");
    app.resize(120, 24).unwrap();
    app.set_screen(Screen::Profiles);
    app.directory_state.set_shelf(Shelf::Jobs);
    app.jobs.loaded = true;
    app.jobs.items = (0..20)
        .map(|index| crate::app::jobs::state_test::posting(&format!("Company{index}"), &["rust"]))
        .collect();
    app.render().unwrap();
    let row = app
        .jobs
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| matches!(target, Target::Job(_)))
        .unwrap()
        .0;
    let wheel = format!("\x1b[<65;{};{}M", row.x + 1, row.y + 1);
    app.interaction_mode = InteractionMode::Keyboard;
    app.handle_input(wheel.as_bytes());
    assert_eq!(app.jobs.mouse.offset(Pane::JobsList), 0);
    app.interaction_mode = InteractionMode::Hybrid;
    app.handle_input(wheel.as_bytes());
    assert_eq!(app.jobs.mouse.offset(Pane::JobsList), 3);
    assert_eq!(app.jobs.selected(), 0);
    app.render().unwrap();
    let (rect, target) = app
        .jobs
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| matches!(target, Target::Job(_)))
        .unwrap();
    let Target::Job(id) = target else {
        unreachable!()
    };
    let expected = app.jobs.items.iter().position(|job| job.id == id).unwrap();
    app.handle_input(
        format!(
            "\x1b[<2;{};{}M\x1b[<0;{};{}m",
            rect.x + 1,
            rect.y + 1,
            rect.x + 1,
            rect.y + 1
        )
        .as_bytes(),
    );
    assert_eq!(app.jobs.selected(), 0, "right press and release ignored");
    screen5_click(&mut app, rect);
    assert_eq!(app.jobs.selected(), expected);
    app.render().unwrap();
    let copy = app
        .jobs
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| matches!(target, Target::Copy(_)))
        .unwrap()
        .0;
    screen5_click(&mut app, copy);
    assert_eq!(
        app.pending_clipboard,
        Some(app.jobs.items[expected].url.clone())
    );
    app.resize(60, 20).unwrap();
    app.render().unwrap();
    let back = app
        .jobs
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| *target == Target::Back)
        .unwrap()
        .0;
    screen5_click(&mut app, back);
    assert!(!app.jobs.detail_open());
    app.render().unwrap();
    app.handle_input(b"\x1b[<0;15;1M");
    assert_eq!(
        app.screen,
        Screen::Dashboard,
        "global frame remains clickable"
    );
}

#[tokio::test]
async fn profiles_mouse_editor_retains_the_draft_on_a_click_outside() {
    use crate::app::{
        common::primitives::Screen,
        directory::editor::state::{Field, MouseTarget, Page},
    };
    use late_core::models::user::InteractionMode;
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "profiles-mouse-editor").await;
    let mut app = make_app(db.db.clone(), user.id, "profiles-mouse-editor");
    app.resize(100, 30).unwrap();
    app.set_screen(Screen::Profiles);
    app.handle_input(b"w");
    app.render().unwrap();
    let about = app
        .directory_editor
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| *target == MouseTarget::Page(Page::About))
        .unwrap()
        .0;
    app.interaction_mode = InteractionMode::Keyboard;
    screen5_click(&mut app, about);
    assert_eq!(app.directory_editor.page(), Page::Card);
    app.interaction_mode = InteractionMode::Hybrid;
    screen5_click(&mut app, about);
    app.render().unwrap();
    let bio = app
        .directory_editor
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| *target == MouseTarget::Field(Field::Bio))
        .unwrap()
        .0;
    screen5_click(&mut app, bio);
    app.handle_input(b"draft bio");
    app.render().unwrap();
    screen5_click(&mut app, ratatui::layout::Rect::new(0, 0, 1, 1));
    assert!(app.directory_editor.is_open());
    assert_eq!(app.directory_editor.field_text(Field::Bio), "draft bio");
}

#[tokio::test]
async fn profiles_mouse_search_selects_person_and_profile_popup_owns_input() {
    use crate::app::{common::primitives::Screen, directory::mouse::Target};
    use late_core::models::{
        showcase::{Showcase, ShowcaseParams},
        user::InteractionMode,
    };
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "profiles-mouse-search").await;
    let author = create_test_user(&db.db, "profiles-mouse-author").await;
    let url = "https://example.com/mouse/project?full=value";
    let client = db.db.get().await.unwrap();
    Showcase::create_by_user_id(
        &client,
        author.id,
        ShowcaseParams {
            user_id: author.id,
            title: "Mouse project".to_string(),
            url: url.to_string(),
            description: "A shared project".to_string(),
            tags: vec!["rust".to_string()],
        },
    )
    .await
    .unwrap();
    drop(client);
    let mut app = make_app(db.db.clone(), user.id, "profiles-mouse-search");
    app.resize(80, 24).unwrap();
    app.set_screen(Screen::Profiles);
    app.directory_state
        .set_shelf(crate::app::directory::state::Shelf::Profiles);
    wait_for_render_contains(&mut app, "@profiles-mouse-author").await;
    app.interaction_mode = InteractionMode::Hybrid;
    app.render().unwrap();
    let search = app
        .directory_state
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| *target == Target::Key(b's'))
        .unwrap()
        .0;
    screen5_click(&mut app, search);
    app.handle_input(b"profiles-mouse-author");
    app.render().unwrap();
    let person = app
        .directory_state
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| *target == Target::Person(author.id))
        .unwrap()
        .0;
    screen5_click(&mut app, person);
    assert!(!app.directory_state.search_mode());
    assert!(app.directory_state.detail_open());
    app.render().unwrap();
    let copy = app
        .directory_state
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| *target == Target::Copy(url.to_string()))
        .unwrap()
        .0;
    screen5_click(&mut app, copy);
    assert_eq!(app.pending_clipboard, Some(url.to_string()));
    app.render().unwrap();
    let profile = app
        .directory_state
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| matches!(target, Target::Profile(id, _) if *id == author.id))
        .unwrap()
        .0;
    screen5_click(&mut app, profile);
    assert!(app.show_profile_modal);
    wait_for_render_contains(&mut app, "profile · profiles-mouse-author").await;
    app.render().unwrap();
    app.handle_input(b"\x1b[<0;15;1M");
    assert_eq!(app.screen, Screen::Profiles, "popup blocks page switches");
    assert!(!app.show_profile_modal, "outside click dismisses the popup");
}

/// `/map` typed into a composer lands on the People page, on its Map
/// section, and the tick turns the counts on for exactly as long as the
/// section is on screen. `]` steps on to Profiles and the map goes quiet.
#[tokio::test]
async fn the_map_command_lands_on_the_profiles_map_shelf() {
    use crate::app::common::primitives::Screen;
    use crate::app::directory::state::Shelf;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "usermap-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "usermap-flow-it");
    wait_for_render_contains(&mut app, "lounge").await;

    app.handle_input(b"/map\r");
    assert_eq!(
        app.screen,
        Screen::Profiles,
        "the command lands on the page"
    );
    assert_eq!(app.directory_state.shelf(), Shelf::Map, "on its map shelf");
    app.tick();
    assert!(app.usermap.is_shown(), "the tick asks for the counts");
    wait_for_render_contains(&mut app, "where everyone is").await;

    app.handle_input(b"]");
    assert_eq!(app.directory_state.shelf(), Shelf::Profiles);
    app.tick();
    assert!(
        !app.usermap.is_shown(),
        "a shelf nobody looks at counts nothing"
    );
    wait_for_render_not_contains(&mut app, "where everyone is").await;
}

/// All four arrows pan. Up and down used to walk the country list instead,
/// which read as two of the four arrows being broken — and the map was pinned
/// to the top of its box, so even after the binding was fixed there was
/// nothing above the equator to pan to.
#[tokio::test]
async fn the_usermap_pans_with_every_arrow() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "usermap-pan-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let mut app = make_app(test_db.db.clone(), user.id, "usermap-pan-flow-it");
    wait_for_render_contains(&mut app, "lounge").await;

    app.handle_input(b"/map\r");
    wait_for_render_contains(&mut app, "where everyone is").await;

    // Zoom in until there is somewhere to go on both axes. Fitted, the whole
    // world is on screen and every arrow is correctly a no-op; the map is
    // twice as wide as it is tall, so the vertical room arrives last.
    let map = app.usermap.map().expect("earth");
    for _ in 0..12 {
        let view = app.usermap.view().expect("a view once drawn");
        if view.can_pan_vertically(&map) && view.can_pan_horizontally(&map) {
            break;
        }
        app.handle_input(b"+");
        // A draw is what applies the new scale to the view.
        let _ = render_plain(&mut app);
    }
    let view = app.usermap.view().expect("view");
    assert!(
        view.can_pan_vertically(&map) && view.can_pan_horizontally(&map),
        "zooming in should eventually leave room to pan: {view:?}"
    );
    let start = view;

    app.handle_input(b"\x1b[C"); // right
    let right = app.usermap.view().expect("view");
    assert!(right.vx > start.vx, "right arrow should pan east");

    app.handle_input(b"\x1b[B"); // down
    let down = app.usermap.view().expect("view");
    assert!(down.vy > start.vy, "down arrow should pan south");

    app.handle_input(b"\x1b[A"); // up
    let up = app.usermap.view().expect("view");
    assert!(up.vy < down.vy, "up arrow should pan back north");

    app.handle_input(b"\x1b[D"); // left
    let left = app.usermap.view().expect("view");
    assert!(left.vx < right.vx, "left arrow should pan back west");
}

/// PRINTS end to end: a runner walks into the shop, paints (`?` and the
/// digits paint here: the guide and the page keys keep out), saves, steps
/// out with Esc, comes back, and the poster on the wall is what was
/// painted.
#[tokio::test]
async fn a_runner_paints_a_poster_at_prints_and_finds_it_hung() {
    use late_core::models::deadchannel_runner::DeadchannelRunner;

    let (test_db, mut app, user_id) = runner_on_the_street("prints-it", 120, 40, |_| {}).await;
    // Behind the press's counter, on the shop's floor.
    app.city.player_x = 30;
    app.city.player_y = 14;
    wait_for_render_contains(&mut app, "the poster: 32 by 12").await;

    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "nothing hangs yet: the test card shows").await;
    app.handle_input(b"HI?0");
    wait_for_render_contains(&mut app, "│HI?0").await;
    wait_for_render_contains(&mut app, " Undercity ").await;

    app.handle_input(b"\x13");
    wait_for_render_contains(&mut app, "the press runs. it hangs on your profile now.").await;
    let client = test_db.db.get().await.expect("db client");
    assert!(
        DeadchannelRunner::poster(&client, user_id)
            .await
            .expect("poster")
            .is_some(),
        "the row holds the poster"
    );

    // Out to the street, and back in: the wall is read again.
    app.handle_input(b"\x1b");
    wait_for_render_not_contains(&mut app, "the press runs").await;
    wait_for_render_contains(&mut app, "the poster: 32 by 12").await;
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "on your profile").await;
    wait_for_render_contains(&mut app, "│HI?0").await;
}

/// The whole of `O`, on the stock reef that keeps to #lounge: it frees and
/// keeps again. In another room the reef goes down, the chat takes its cells
/// and the page stops hosting the strip; back on #lounge it returns. Arrange
/// mode shows it from any room, named, and the binding is saved with the page.
#[tokio::test]
async fn a_tile_kept_to_lounge_goes_down_in_other_rooms_and_comes_back() {
    use crate::app::zen::state::{Guest, RiceLayout, TileKind};

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "only-here-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let announcements = ChatRoom::ensure_permanent(&client, "announcements")
        .await
        .expect("announcements");
    ChatRoomMember::join(&client, announcements.id, user.id)
        .await
        .expect("join announcements");
    let mut app = make_app(test_db.db.clone(), user.id, "only-here-flow-it");
    app.resize(200, 50).expect("resize test terminal");
    wait_for_render_contains(&mut app, "announcements").await;
    app.handle_input(b"\x1f");
    app.handle_input(b"lounge\r");
    assert_eq!(app.chat.selected_room_id, Some(lounge.id));

    let rect_of = |app: &crate::app::state::App, kind: TileKind| {
        app.home_tile_rects()
            .into_iter()
            .find(|(tile, _, _)| *tile == kind)
            .map(|(_, rect, _)| rect)
            .expect("the layout holds the tile")
    };
    let reef_up = rect_of(&app, TileKind::Aquarium);
    let chat_up = rect_of(&app, TileKind::Chat);
    assert!(reef_up.height > 0, "the stock reef is up on #lounge");

    // The stock reef already keeps to #lounge, by slug, and arrange mode says
    // so. `O` on #lounge frees it though the rail says the room by id; `O`
    // again keeps it, naming it. The rail and the chat refuse.
    app.handle_input(b"\x05");
    assert!(app.home.arrange);
    wait_for_render_contains(&mut app, "only #lounge").await;
    app.home.focus = app
        .home
        .first_tile_of(TileKind::Aquarium)
        .expect("the stock reef");
    app.handle_input(b"O");
    assert_eq!(
        app.banner.as_ref().map(|banner| banner.message.as_str()),
        Some("In every room")
    );
    app.handle_input(b"O");
    assert_eq!(
        app.banner.as_ref().map(|banner| banner.message.as_str()),
        Some("Only in #lounge")
    );
    wait_for_render_contains(&mut app, "only #lounge").await;
    app.home.focus = app
        .home
        .first_tile_of(TileKind::Rooms)
        .expect("the stock rail");
    app.handle_input(b"O");
    assert_eq!(
        app.banner.as_ref().map(|banner| banner.message.as_str()),
        Some("The rail and the chats stay up in every room")
    );
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.home.arrange, "leave arrange mode").await;

    // Still on #lounge, nothing moved.
    assert_eq!(rect_of(&app, TileKind::Aquarium), reef_up);
    assert!(app.home.hosts(Guest::Now));

    // Another room: the reef's rows are the chat's, and nothing is left of
    // the reef to host the strip, step the simulation or take a click.
    app.handle_input(b"\x1f");
    app.handle_input(b"announcements\r");
    assert_eq!(app.chat.selected_room_id, Some(announcements.id));
    assert_eq!(rect_of(&app, TileKind::Aquarium).height, 0);
    let chat_alone = rect_of(&app, TileKind::Chat);
    assert_eq!(chat_alone.y, reef_up.y);
    assert_eq!(chat_alone.bottom(), chat_up.bottom());
    assert!(!app.home.hosts(Guest::Now));
    assert!(!app.home_strip_hosted());
    assert_eq!(app.reef_tile_inner(), None);

    // Arranging from there brings it up so it can be reached, and leaving
    // takes it down again.
    app.handle_input(b"\x05");
    assert!(rect_of(&app, TileKind::Aquarium).height > 0);
    app.handle_input(b"\x1b");
    wait_for_esc_effect(&mut app, |app| !app.home.arrange, "leave arrange mode").await;
    assert_eq!(rect_of(&app, TileKind::Aquarium).height, 0);

    // Back on #lounge it is where it was.
    app.handle_input(b"\x1f");
    app.handle_input(b"lounge\r");
    assert_eq!(rect_of(&app, TileKind::Aquarium), reef_up);
    assert_eq!(rect_of(&app, TileKind::Chat), chat_up);
    assert!(app.home.hosts(Guest::Now));
    assert!(app.reef_tile_inner().is_some());

    // The binding is the layout's: it reads back from what is stored.
    let ordinal = app
        .home
        .first_tile_of(TileKind::Aquarium)
        .expect("the stock reef");
    let stored = app.home.rice.to_json();
    let read = RiceLayout::from_json_or(Some(&stored), RiceLayout::home());
    assert_eq!(read.root.only_at(ordinal), ChatRoomBinding::Room(lounge.id));

    // On a rail row that is no room there is nothing to keep a tile to.
    app.handle_input(b"\x05");
    app.chat.select_notifications();
    app.home.focus = ordinal;
    app.handle_input(b"O");
    assert_eq!(
        app.banner.as_ref().map(|banner| banner.message.as_str()),
        Some("Pick a room on the rail first")
    );
    assert_eq!(
        app.home.rice.root.only_at(ordinal),
        ChatRoomBinding::Room(lounge.id)
    );
}

/// The care click exists to reach the tank, and the stock reef keeps to
/// #lounge. With the tank due the click takes the chat there, so it lands on a
/// reef that can take the meal; with the tank fed, the room stays yours.
#[tokio::test]
async fn the_care_click_lands_on_the_reef_while_the_tank_is_due() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "care-reef-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    let announcements = ChatRoom::ensure_permanent(&client, "announcements")
        .await
        .expect("announcements");
    ChatRoomMember::join(&client, announcements.id, user.id)
        .await
        .expect("join announcements");
    let mut app = make_app(test_db.db.clone(), user.id, "care-reef-flow-it");
    app.resize(200, 50).expect("resize test terminal");
    wait_for_render_contains(&mut app, "announcements").await;

    app.handle_input(b"\x1f");
    app.handle_input(b"announcements\r");
    assert_eq!(app.chat.selected_room_id, Some(announcements.id));
    assert_eq!(app.reef_tile_inner(), None, "the reef is down in this room");

    crate::app::zen::input::open_care(&mut app);
    assert_eq!(app.chat.selected_room_id, Some(lounge.id));
    assert!(app.reef_tile_inner().is_some(), "the click found the reef");

    app.handle_input(b"\x1f");
    app.handle_input(b"announcements\r");
    app.aquarium_care.feed();
    crate::app::zen::input::open_care(&mut app);
    assert_eq!(app.chat.selected_room_id, Some(announcements.id));
}

/// Enter on a mention always opens what it stamps read. A page with no chat
/// tile reads the message in the history modal, so the row never clears for a
/// message nobody saw.
#[tokio::test]
async fn a_mention_answered_with_no_chat_tile_opens_in_history_and_clears() {
    use crate::app::zen::state::{Guest, Look, Node, RiceLayout, TileKind, ZenState};
    use late_core::models::notification::Notification;

    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "attend-viewer").await;
    let actor = create_test_user(&test_db.db, "attend-actor").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: actor.id,
            body: "@attend-viewer over here".to_string(),
        },
    )
    .await
    .expect("create mention message");
    Notification::create_mentions_batch(&client, &[viewer.id], actor.id, message.id, lounge.id)
        .await
        .expect("create mention notification");

    let mut app = make_app(test_db.db.clone(), viewer.id, "attend-flow-it");
    app.resize(200, 50).expect("resize test terminal");
    app.home = ZenState::new(
        RiceLayout {
            root: Node::hosting(TileKind::Clock, &[Guest::Attention]),
            look: Look::default(),
        },
        None,
    );
    app.handle_input(b"\x05");
    assert!(app.home.arrange);
    wait_for_render_contains(&mut app, "mentioned you in #lounge").await;

    app.handle_input(b"\r");
    assert!(
        app.chat.history_modal.is_open(),
        "the mention opened nothing, banner: {:?}",
        app.banner.as_ref().map(|banner| banner.message.as_str())
    );

    app.handle_input(b"\x1b");
    wait_for_render_not_contains(&mut app, "mentioned you in #lounge").await;
}
