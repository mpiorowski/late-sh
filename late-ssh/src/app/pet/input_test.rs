use late_core::models::pet::{Form, PetCompanion, PetSpecies, Pick, Stage, Trick, VisitKind};
use late_core::test_utils::{TestDb, create_test_user};

use super::{Order, handle_order, parse_order};
use crate::app::pet::state::Toy;
use crate::app::pet::svc::PetCommand;
use crate::app::state::App;
use crate::app::zen::state::{Dir, Look, Node, RiceLayout, TileKind, ZenState};
use crate::test_helpers::{
    make_app, new_test_db, render_plain, wait_for_app, wait_for_render_contains, wait_until,
};

/// An account on a Home arranged with the pet's tile tall enough for its
/// rows, its pet `days` days old (the last of them today).
async fn home_with_pet(handle: &str, days: i64) -> (TestDb, App) {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, handle).await;
    let mut app = make_app(test_db.db.clone(), user.id, handle);
    let today = chrono::Utc::now().date_naive();
    for days_ago in (0..days).rev() {
        app.pet_state
            .svc
            .act(
                user.id,
                PetCommand::CountDay,
                today - chrono::Duration::days(days_ago),
            )
            .await
            .expect("count a day");
    }
    let arranged = RiceLayout {
        root: Node::split_first(
            Dir::Row,
            23,
            Node::leaf(TileKind::Rooms),
            Node::split_second(
                Dir::Row,
                40,
                Node::leaf(TileKind::Chat),
                Node::leaf(TileKind::Pet),
            ),
        ),
        look: Look::default(),
    };
    app.home = ZenState::new(arranged.clone(), Some(arranged));
    app.resize(160, 40).expect("resize test terminal");
    wait_for_render_contains(&mut app, "╭ pet").await;
    // The session's first key asks for the day, and the answer carries
    // the pet as the row has it. The mouse moving is such a key.
    app.handle_input(b"\x1b[<35;2;2M");
    wait_for_render_contains(&mut app, &format!("day {days}")).await;
    (test_db, app)
}

fn click(app: &mut App, button: u8, x: u16, y: u16) {
    let press = format!("\x1b[<{button};{};{}M", x + 1, y + 1);
    app.handle_input(press.as_bytes());
}

#[tokio::test]
async fn a_right_click_or_the_status_row_opens_the_care_box_and_q_closes_it() {
    let (_test_db, mut app) = home_with_pet("pet-input-open", 1).await;
    let frame = app.last_pet_frame.get().expect("the tile drew");
    let status = frame.rows[0].expect("a tall tile has its status row");

    click(&mut app, 2, frame.zone.x + 1, frame.zone.y + 1);
    assert!(app.show_pet_modal, "a right click on the tile");
    let plain = render_plain(&mut app);
    for row in ["treat", "grow", "play", "tricks", "came by"] {
        assert!(plain.contains(row), "the care box lists {row}:\n{plain}");
    }
    assert!(plain.contains("an egg · day 1 · bond 0"), "{plain}");
    assert!(plain.contains("it hatches in 1 day"), "{plain}");
    assert!(
        !plain.contains("/pet reset"),
        "an egg has nothing to change yet:\n{plain}"
    );

    app.handle_input(b"q");
    assert!(!app.show_pet_modal);

    render_plain(&mut app);
    click(&mut app, 0, status.x + 1, status.y);
    assert!(app.show_pet_modal, "a click on the status row");
    app.handle_input(b"\x1b");
    wait_for_app(&mut app, "the care box to close", |app| !app.show_pet_modal).await;

    // Anywhere else, a right click is nobody's.
    render_plain(&mut app);
    click(&mut app, 2, 2, 2);
    assert!(!app.show_pet_modal);
}

#[tokio::test]
async fn the_care_box_hatches_what_its_owner_picks_plays_and_teaches() {
    let (_test_db, mut app) = home_with_pet("pet-input-care", 2).await;
    wait_for_render_contains(&mut app, "ready to hatch").await;

    // A click on a ready egg hatches nothing: it opens the care box, where
    // the three animals it can be are shown with the key that picks each.
    let egg = app.last_pet_rect.get().expect("the egg drew");
    click(&mut app, 0, egg.x + 1, egg.y + 1);
    assert!(app.show_pet_modal);
    assert_eq!(app.pet_state.growth().stage(), Stage::Egg);
    let plain = render_plain(&mut app);
    for pick in ["hatch", "1 cat", "2 dog", "3 bird", "(ovo)"] {
        assert!(plain.contains(pick), "the hatch row shows {pick}:\n{plain}");
    }
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, "baby dog · day 2").await;
    assert_eq!(app.pet_state.animal(), Some(PetSpecies::Dog));
    let plain = render_plain(&mut app);
    assert!(!plain.contains("3 bird"), "the pick is made:\n{plain}");
    assert!(
        plain.contains("/pet cat, dog or bird"),
        "a hatched pet's settings are on the line:\n{plain}"
    );
    // The keys pick nothing once it has hatched.
    app.handle_input(b"1");

    // A ball and the toy mouse land in the pen the box draws.
    app.handle_input(b"b");
    assert_eq!(
        app.pet_state.plaything().map(|toy| toy.toy),
        Some(Toy::Ball)
    );
    app.handle_input(b"m");
    assert_eq!(
        app.pet_state.plaything().map(|toy| toy.toy),
        Some(Toy::Mouse)
    );

    // The teach row starts on the first trick; `t` moves it, `p` is a
    // day's practice of the one it points at.
    assert_eq!(app.pet_state.teaching(), Some(Trick::Spin));
    app.handle_input(b"t");
    assert_eq!(app.pet_state.teaching(), Some(Trick::Bow));
    app.handle_input(b"p");
    wait_for_render_contains(&mut app, "bow 1/3").await;
    assert_eq!(app.pet_state.growth().learning, Some(Trick::Bow));
    assert_eq!(app.pet_state.animal(), Some(PetSpecies::Dog));
}

/// A pet with its days grows when its owner picks one of the two drawings
/// the care box shows, by the key under it.
#[tokio::test]
async fn the_care_box_grows_the_pet_into_the_drawing_its_owner_picks() {
    let (_test_db, mut app) = home_with_pet("pet-input-grow", 7).await;
    let egg = app.last_pet_rect.get().expect("the egg drew");
    click(&mut app, 0, egg.x + 1, egg.y + 1);
    assert!(app.show_pet_modal);
    app.handle_input(b"1");
    wait_for_render_contains(&mut app, "baby cat · day 7").await;

    // Seven days is a young cat's worth: the grow row is its two tops.
    let plain = render_plain(&mut app);
    for shown in [r"/\_/\", r"|\_/|", "1 pointed", "2 straight"] {
        assert!(
            plain.contains(shown),
            "the grow row shows {shown}:\n{plain}"
        );
    }
    // There is no third drawing.
    app.handle_input(b"3");
    assert_eq!(app.pet_state.growth().stage(), Stage::Baby);

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, "young cat · day 7").await;
    assert_eq!(
        app.pet_state.portrait().form,
        Form::Young { top: Pick::Two }
    );
    let plain = render_plain(&mut app);
    assert!(plain.contains("adult in 23 days"), "{plain}");
    assert!(!plain.contains("1 pointed"), "the pick is made:\n{plain}");
    // With nothing waiting the keys pick nothing.
    app.handle_input(b"1");
    assert_eq!(
        app.pet_state.portrait().form,
        Form::Young { top: Pick::Two }
    );
}

/// A pet that came calling is in its host's care box and on its tile:
/// drawn in the pen with its name under it, and listed with the others.
#[tokio::test]
async fn a_guest_shows_in_the_care_box_and_on_the_tile() {
    let (test_db, mut app) = home_with_pet("pet-input-host", 2).await;
    let host = app.pet_state.user_id;
    let caller = create_test_user(&test_db.db, "cy").await;
    let svc = app.pet_state.svc.clone();
    svc.bootstrap(caller.id).await.expect("bootstrap");
    let today = chrono::Utc::now().date_naive();
    for days_ago in [1, 0] {
        svc.act(
            caller.id,
            PetCommand::CountDay,
            today - chrono::Duration::days(days_ago),
        )
        .await
        .expect("count a day");
    }
    svc.act(caller.id, PetCommand::Hatch(PetSpecies::Dog), today)
        .await
        .expect("hatch");
    let client = test_db.db.get().await.expect("db client");
    PetCompanion::set_name(&client, caller.id, Some("Biscuit"))
        .await
        .expect("name");
    assert!(
        PetCompanion::claim_visit(&**client, host, caller.id, VisitKind::Call, today)
            .await
            .expect("call")
    );
    let guest = format!("{}'s Biscuit", caller.username);
    assert!(!render_plain(&mut app).contains(&guest));

    // The host's next answer from the service carries who is over: here,
    // its egg hatching.
    let egg = app.last_pet_rect.get().expect("the egg drew");
    click(&mut app, 0, egg.x + 1, egg.y + 1);
    app.handle_input(b"1");
    wait_for_render_contains(&mut app, "here now").await;
    let plain = render_plain(&mut app);
    assert!(
        plain.contains(&format!("{guest} · each stays an hour")),
        "the care box lists who is over:\n{plain}"
    );
    assert!(
        plain.matches(&guest).count() >= 2,
        "and the pen draws it over its name:\n{plain}"
    );
    assert!(plain.contains("u u"), "a baby dog's ears:\n{plain}");

    app.handle_input(b"q");
    assert!(!app.show_pet_modal, "q closes the care box");
    let plain = render_plain(&mut app);
    assert_eq!(
        plain.matches(&guest).count(),
        1,
        "the tile draws it over its name:\n{plain}"
    );
    assert!(plain.contains("u u"), "{plain}");
}

#[test]
fn the_words_after_pet_are_an_order_or_nothing() {
    assert_eq!(parse_order("cat"), Some(Order::Species(PetSpecies::Cat)));
    assert_eq!(
        parse_order(" Bird "),
        Some(Order::Species(PetSpecies::Bird))
    );
    assert_eq!(parse_order("away"), Some(Order::Away));
    assert_eq!(parse_order("out"), Some(Order::Out));
    assert_eq!(parse_order("reset"), Some(Order::ResetAsk));
    assert_eq!(parse_order("reset yes"), Some(Order::Reset));
    for nothing in [
        "",
        "fish",
        "cat dog",
        "reset now",
        "reset yes please",
        "away yes",
    ] {
        assert_eq!(parse_order(nothing), None, "{nothing:?}");
    }
}

#[tokio::test]
async fn pet_orders_change_what_it_is_put_it_away_and_start_it_over() {
    let (test_db, mut app) = home_with_pet("pet-input-orders", 2).await;
    let said = |app: &mut App, order| handle_order(app, order).map(|banner| banner.message);

    // An egg is no animal, and a reset is asked about before it is done.
    assert_eq!(
        said(&mut app, Order::Species(PetSpecies::Dog)),
        Some("It is still an egg: you pick what hatches when it is ready".to_string())
    );
    app.pet_state.hatch(PetSpecies::Cat);
    wait_for_app(&mut app, "the hatch", |app| {
        app.pet_state.animal() == Some(PetSpecies::Cat)
    })
    .await;
    let asked = said(&mut app, Order::ResetAsk).expect("a reset says what it takes");
    assert!(asked.contains("/pet reset yes"), "{asked}");
    assert_eq!(
        app.pet_state.growth().stage(),
        Stage::Baby,
        "asking does nothing"
    );

    // The row answers a change of species on a later tick.
    assert_eq!(said(&mut app, Order::Species(PetSpecies::Bird)), None);
    wait_for_app(&mut app, "the bird", |app| {
        app.pet_state.animal() == Some(PetSpecies::Bird)
    })
    .await;

    // Away: it shows nowhere and takes no orders but the one back out. The
    // switch lands on the row from a task of its own.
    let (db, user_id) = (test_db.db.clone(), app.pet_state.user_id);
    let row_out = |out: bool| {
        let db = db.clone();
        async move {
            wait_until(
                || async {
                    let client = db.get().await.expect("db client");
                    PetCompanion::ensure(&client, user_id)
                        .await
                        .expect("the row")
                        .enabled
                        == out
                },
                "the row to hear the switch",
            )
            .await;
        }
    };
    assert!(said(&mut app, Order::Away).is_some());
    assert!(!app.pet_out());
    row_out(false).await;
    let away = Some("Your pet is put away: /pet out brings it back".to_string());
    assert_eq!(said(&mut app, Order::Species(PetSpecies::Dog)), away);
    assert_eq!(said(&mut app, Order::Reset), away);
    assert_eq!(app.pet_state.animal(), Some(PetSpecies::Bird));
    assert_eq!(
        said(&mut app, Order::Out),
        Some("Your pet is back out".to_string())
    );
    assert!(app.pet_out());
    assert_eq!(
        said(&mut app, Order::Out),
        Some("Your pet is already out".to_string())
    );

    // The row decides a reset, and it refuses a pet it still holds as put
    // away.
    row_out(true).await;
    assert_eq!(said(&mut app, Order::Reset), None);
    wait_for_app(&mut app, "the egg", |app| {
        app.pet_state.growth().stage() == Stage::Egg
    })
    .await;
    assert_eq!(app.pet_state.animal(), None);
}
