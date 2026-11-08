use late_core::models::user::{ComebackMark, User, extract_comeback_mark};
use late_core::test_utils::create_test_user;

use crate::app::comeback::state::{Moment, Start};
use crate::app::help_modal::data::HelpTopic;
use crate::test_helpers::{
    SessionWorld, chat_compose_app, make_app, make_app_in_world, make_app_with_paired_client,
    new_test_db, render_plain, wait_for_app, wait_for_render_contains, wait_until,
};

const INSTALL: &str = "curl -fsSL https://cli.late.sh/install.sh | bash";

/// A login on the day after the account's first opens the box once the
/// splash is down, with the install one-liner and the listen page, stamps
/// the account with it as of today, holds every stray key, and Esc keeps
/// it for later.
#[tokio::test]
async fn a_return_day_opens_the_box_stamps_the_account_and_esc_keeps_it_for_later() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "comeback-day-two").await;
    let world = SessionWorld {
        comeback_due: true,
        ..SessionWorld::default()
    };
    let mut app = make_app_in_world(test_db.db.clone(), user.id, "comeback-day-two-it", world);

    let frame = render_plain(&mut app);
    assert!(app.comeback.is_open(), "{frame}");
    assert!(frame.contains("the house has a radio"), "{frame}");
    assert!(
        frame.contains("over plain ssh you hear none of it."),
        "{frame}"
    );
    for station in late_core::radio::RadioStation::enabled() {
        assert!(
            frame.contains(station.label()),
            "{} missing: {frame}",
            station.label()
        );
    }
    assert!(frame.contains(INSTALL), "{frame}");
    assert!(frame.contains("localhost:3000/listen"), "{frame}");
    assert!(
        frame.contains("[Enter] the install guide, with a QR for your phone · [Esc] later"),
        "{frame}"
    );

    let today = chrono::Utc::now().date_naive();
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move {
                let client = db.get().await.expect("db client");
                let fresh = User::get(&client, user.id)
                    .await
                    .expect("get failed")
                    .expect("user");
                extract_comeback_mark(&fresh.settings)
                    == Some(ComebackMark {
                        step: Moment::HearIt.step(),
                        day: today,
                    })
            }
        },
        "the comeback mark",
    )
    .await;

    app.handle_input(b"q");
    assert!(app.comeback.is_open(), "a stray key holds the box");
    assert!(!app.show_quit_confirm);
    app.handle_input(b"\x1b");
    wait_for_app(&mut app, "the box kept for later", |app| {
        !app.comeback.is_open()
    })
    .await;
    assert!(!app.show_help);
}

#[tokio::test]
async fn enter_on_plain_ssh_opens_the_pair_guide_and_closes_the_box() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "comeback-enter").await;
    let world = SessionWorld {
        comeback_due: true,
        ..SessionWorld::default()
    };
    let mut app = make_app_in_world(test_db.db.clone(), user.id, "comeback-enter-it", world);
    render_plain(&mut app);
    assert!(app.comeback.is_open());

    app.handle_input(b"\r");
    assert!(!app.comeback.is_open());
    assert!(app.show_help);
    assert_eq!(app.help_modal_state.selected_topic(), HelpTopic::Pair);
    let frame = render_plain(&mut app);
    assert!(frame.contains(INSTALL), "{frame}");
}

/// A session with a client paired hears the radio already, so the box
/// names the keys and Enter opens the Stations modal.
#[tokio::test]
async fn a_paired_session_is_told_the_keys_and_enter_picks_a_station() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "comeback-paired").await;
    let (mut app, _control) =
        make_app_with_paired_client(test_db.db.clone(), user.id, "comeback-paired-it");

    super::show(&mut app, Moment::HearIt, Start::Command);
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("you are on the late CLI, so you can hear it."),
        "{frame}"
    );
    assert!(
        frame.contains("[Enter] pick a station · [Esc] later"),
        "{frame}"
    );
    assert!(!frame.contains(INSTALL), "{frame}");

    app.handle_input(b"\r");
    assert!(!app.comeback.is_open());
    assert!(app.stations_modal_state.is_open());
}

/// The big games: every hub card by group, the Lobby, and Enter opens the
/// Games hub.
#[tokio::test]
async fn the_big_games_name_the_hub_and_enter_opens_it() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "comeback-games").await;
    let mut app = make_app(test_db.db.clone(), user.id, "comeback-games-it");

    super::show(&mut app, Moment::BigGames, Start::Command);
    let frame = render_plain(&mut app);
    assert!(frame.contains("the big games"), "{frame}");
    for game in crate::app::door::hub::state::HubGame::roster(false) {
        assert!(
            frame.contains(game.label()),
            "{} missing: {frame}",
            game.label()
        );
    }
    assert!(!frame.contains("Night City"), "{frame}");
    assert!(frame.contains("moving in the static"), "{frame}");
    assert!(
        frame.contains("[Enter] open the Games hub · [Esc] later"),
        "{frame}"
    );

    app.handle_input(b"\r");
    assert!(!app.comeback.is_open());
    assert_eq!(app.screen, crate::app::common::primitives::Screen::Games);
}

/// The people: the page's three shelves and the web page, and Enter
/// lands on People, on the map.
#[tokio::test]
async fn the_people_name_the_three_shelves_and_enter_opens_the_map() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "comeback-people").await;
    let mut app = make_app(test_db.db.clone(), user.id, "comeback-people-it");

    super::show(&mut app, Moment::People, Start::Command);
    let frame = render_plain(&mut app);
    assert!(frame.contains("the people"), "{frame}");
    for shelf in crate::app::directory::state::Shelf::ALL {
        assert!(
            frame.contains(shelf.title()),
            "{} missing: {frame}",
            shelf.title()
        );
    }
    assert!(frame.contains("localhost:3000/profiles"), "{frame}");
    assert!(
        frame.contains("[Enter] open People · [Esc] later"),
        "{frame}"
    );

    app.handle_input(b"\r");
    assert!(!app.comeback.is_open());
    assert_eq!(app.screen, crate::app::common::primitives::Screen::Profiles);
    assert_eq!(
        app.directory_state.shelf(),
        crate::app::directory::state::Shelf::Map
    );
}

/// `/comeback N` reruns a box for anyone; a regular's login owes none.
#[tokio::test]
async fn slash_comeback_reruns_a_moment_and_says_so_on_a_bad_number() {
    let (_test_db, mut app) = chat_compose_app("comeback-cmd").await;
    assert!(!app.comeback.is_open(), "a regular is never handed the box");

    app.handle_input(b"/comeback 7\r");
    wait_for_render_contains(&mut app, "Usage: /comeback 1, 2 or 3").await;

    app.handle_input(b"/comeback 2\r");
    wait_for_app(&mut app, "the box", |app| app.comeback.is_open()).await;
    let frame = render_plain(&mut app);
    assert!(frame.contains("the big games"), "{frame}");
}
