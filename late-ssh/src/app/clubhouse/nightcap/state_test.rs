use super::*;
use crate::app::clubhouse::nightcap::lobby::SeatChange;
use crate::app::games::chips::svc::RoundRefusal;

fn session(seats: &SharedSeats, n: u128) -> State {
    State::new(
        Some(seats.clone()),
        None,
        Uuid::from_u128(n),
        format!("user{n:03}"),
    )
}

#[test]
fn leaving_the_screen_gives_the_stool_back() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    mine.toggle_seat(0);
    assert_eq!(mine.my_seat(), Some(0));

    mine.leave_screen();

    assert_eq!(mine.my_seat(), None);
    assert_eq!(
        seats.toggle_seat(Uuid::from_u128(2), "user002", 0),
        SeatChange::SatDown,
        "the stool should be free for the next patron"
    );
}

#[test]
fn leaving_the_screen_without_a_stool_is_a_no_op() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    let mut theirs = session(&seats, 2);
    theirs.toggle_seat(3);

    mine.leave_screen();

    assert_eq!(theirs.my_seat(), Some(3));
}

#[test]
fn a_stool_someone_else_holds_says_so() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    seats.toggle_seat(Uuid::from_u128(2), "user002", 0);

    mine.toggle_seat(0);

    assert_eq!(mine.my_seat(), None);
    assert_eq!(mine.last_message.as_deref(), Some("that stool is taken."));
}

#[test]
fn taking_a_free_stool_clears_the_last_message() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    mine.toggle_menu();
    assert_eq!(mine.last_message.as_deref(), Some("take a seat first."));

    mine.toggle_seat(1);

    assert_eq!(mine.my_seat(), Some(1));
    assert_eq!(mine.last_message, None);
}

#[test]
fn only_the_seated_may_speak_or_order() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    assert!(!mine.compose_allowed());
    assert_eq!(mine.pick(Order::Drink(Drink::HouseBeer)), None);
    assert_eq!(mine.last_message.as_deref(), Some("take a seat first."));
    assert!(!mine.menu_open());

    mine.toggle_seat(0);

    assert!(mine.compose_allowed());
    mine.toggle_menu();
    assert!(mine.menu_open());
}

#[test]
fn one_order_at_a_time_until_the_house_answers() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    mine.toggle_seat(0);
    mine.toggle_menu();

    assert_eq!(
        mine.pick(Order::Drink(Drink::WhiskeyNeat)),
        Some(Order::Drink(Drink::WhiskeyNeat))
    );
    assert!(!mine.menu_open(), "an order closes the menu");
    assert_eq!(
        mine.last_message.as_deref(),
        Some("you order the whiskey neat.")
    );

    assert_eq!(mine.pick(Order::Round), None);
    assert_eq!(mine.last_message.as_deref(), Some("the house is on it."));

    // The settled order comes back over the channel svc.rs writes to.
    mine.outcome_sender()
        .send(Outcome::Poured {
            drink: Drink::WhiskeyNeat,
            balance: 1_750,
        })
        .expect("session receiver alive");
    mine.drain_outcomes();

    assert_eq!(
        mine.last_message.as_deref(),
        Some("whiskey neat, 250 chips. 1,750 left on the tab.")
    );
    assert_eq!(mine.pick(Order::Round), Some(Order::Round));
}

#[test]
fn every_outcome_reads_in_the_footer() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    let cases = [
        (
            Outcome::Comped {
                drink: Drink::HouseBeer,
                remaining: 0,
            },
            "house beer, on somebody's round.",
        ),
        (
            Outcome::Comped {
                drink: Drink::TopShelf,
                remaining: 2,
            },
            "top shelf, on somebody's round. 2 more waiting.",
        ),
        (
            Outcome::Bounced {
                drink: Drink::OldFashioned,
            },
            "your tab won't cover the old fashioned.",
        ),
        (
            Outcome::RoundBought {
                patrons: 3,
                total: 300,
                balance: 12_000,
            },
            "a round for 3, 300 chips. 12,000 left on the tab.",
        ),
        (
            Outcome::RoundRefused(RoundRefusal::EmptyHouse),
            "nobody else on a stool to buy for.",
        ),
        (
            Outcome::RoundRefused(RoundRefusal::AllHolding),
            "everyone here still has a drink coming.",
        ),
        (
            Outcome::RoundRefused(RoundRefusal::InsufficientChips {
                patrons: 5,
                total: 500,
            }),
            "a round for 5 runs 500 chips. not tonight.",
        ),
        (Outcome::Failed, "the tap sputtered. try again."),
    ];
    for (outcome, expected) in cases {
        mine.apply_outcome(outcome);
        assert_eq!(mine.last_message.as_deref(), Some(expected), "{outcome:?}");
    }
}

#[test]
fn standing_up_or_leaving_closes_the_menu() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    mine.toggle_seat(2);
    mine.toggle_menu();
    assert!(mine.menu_open());

    mine.toggle_seat(2);
    assert!(!mine.menu_open(), "no stool, no bar to order from");

    mine.toggle_seat(2);
    mine.toggle_menu();
    mine.leave_screen();
    assert!(!mine.menu_open());
    assert!(!mine.close_menu(), "nothing left to peel");
}

#[test]
fn the_roster_cadence_holds_until_the_refresh_window_passes() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    // The first call always refreshes: `force_roster_refresh` starts set so
    // entering the room reconciles immediately instead of waiting a second.
    assert!(mine.roster_refresh_due());
    assert!(!mine.roster_refresh_due());

    mine.tick(ROSTER_REFRESH_TICKS);

    assert!(mine.roster_refresh_due());
}

#[test]
fn entering_the_screen_forces_the_next_refresh() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    assert!(mine.roster_refresh_due());
    assert!(!mine.roster_refresh_due());

    mine.enter_screen();

    assert!(mine.roster_refresh_due());
}

#[test]
fn a_disconnected_patron_loses_their_stool_on_refresh() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    let mut gone = session(&seats, 2);
    mine.toggle_seat(0);
    gone.toggle_seat(1);

    mine.refresh_roster(&[(Uuid::from_u128(1), "user001".to_string())]);

    assert_eq!(mine.my_seat(), Some(0));
    assert_eq!(gone.my_seat(), None);
}

#[test]
fn the_knife_needs_a_stool_and_carves_one_trimmed_line() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    mine.start_carving();
    assert_eq!(mine.carving_text(), None);
    assert_eq!(mine.last_message.as_deref(), Some("take a seat first."));

    mine.toggle_seat(3);
    mine.start_carving();
    let field = mine.carving_mut().expect("knife out");
    field.insert_str("  late again  ");
    assert_eq!(mine.carving_text().as_deref(), Some("  late again  "));

    assert_eq!(mine.take_carving(), Some((3, "late again".to_string())));
    assert_eq!(
        mine.carving_text(),
        None,
        "the knife goes back after a carve"
    );
    assert_eq!(
        mine.last_message.as_deref(),
        Some("you carve it into the wood.")
    );

    mine.start_carving();
    assert_eq!(mine.take_carving(), None, "an empty line is not carved");
    assert_eq!(mine.last_message.as_deref(), Some("nothing to carve."));
}

#[test]
fn standing_up_or_esc_drops_the_knife() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    mine.toggle_seat(0);
    mine.start_carving();
    assert!(mine.cancel_carving());
    assert!(!mine.cancel_carving(), "nothing left to drop");

    mine.start_carving();
    mine.toggle_seat(0);
    assert_eq!(mine.carving_text(), None, "no stool, nothing to carve");
}

#[test]
fn a_carve_settling_never_frees_a_pour_still_in_flight() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    mine.toggle_seat(0);
    assert_eq!(
        mine.pick(Order::Drink(Drink::HouseBeer)),
        Some(Order::Drink(Drink::HouseBeer))
    );

    mine.apply_outcome(Outcome::Carved { stool: 0 });
    assert_eq!(mine.last_message.as_deref(), Some("carved into stool 1."));
    assert_eq!(
        mine.pick(Order::Round),
        None,
        "the pour is still with the house"
    );

    mine.apply_outcome(Outcome::CarveFailed);
    assert_eq!(
        mine.last_message.as_deref(),
        Some("the knife slipped. try again.")
    );
    mine.apply_outcome(Outcome::Bounced {
        drink: Drink::HouseBeer,
    });
    assert_eq!(mine.pick(Order::Round), Some(Order::Round));
}

#[test]
fn the_tv_holds_each_caption_for_a_while_and_cycles() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    assert_eq!(mine.tv_pick(0), 0, "no captions, nothing to pick");
    assert_eq!(mine.tv_pick(3), 0);
    mine.tick(TV_DWELL_TICKS - 1);
    assert_eq!(mine.tv_pick(3), 0);
    mine.tick(TV_DWELL_TICKS);
    assert_eq!(mine.tv_pick(3), 1);
    mine.tick(TV_DWELL_TICKS * 3);
    assert_eq!(mine.tv_pick(3), 0, "wraps around");

    mine.note_activity("mira", "won a crown");
    assert_eq!(mine.last_activity(), Some("mira won a crown"));
}

#[test]
fn the_tick_reports_when_the_bar_moved_so_a_frame_is_drawn() {
    let seats = SharedSeats::new();
    let wall = SharedWall::new();
    let mut mine = State::new(
        Some(seats.clone()),
        Some(wall.clone()),
        Uuid::from_u128(1),
        "user001".to_string(),
    );
    assert!(!mine.refresh_snapshot(), "nothing has moved yet");

    // Another patron sits down elsewhere: this session's next refresh
    // must say so, or the stool stays empty on screen until a keypress.
    seats.toggle_seat(Uuid::from_u128(2), "user002", 4);
    assert!(mine.refresh_snapshot());
    assert!(!mine.refresh_snapshot(), "settled: the same snapshot again");

    // A carve lands on the shared wall from any session.
    wall.set_carving(late_core::models::nightcap_carving::Carving {
        stool: 4,
        user_id: Some(Uuid::from_u128(2)),
        username: Some("user002".to_string()),
        body: "was here".to_string(),
        updated: chrono::Utc::now(),
    });
    assert!(mine.refresh_snapshot());

    // A settled outcome of our own is a change even when the snapshot is not.
    assert!(!mine.drain_outcomes(), "empty channel, nothing drawn");
    mine.outcome_sender()
        .send(Outcome::CarveFailed)
        .expect("receiver alive");
    assert!(mine.drain_outcomes());

    // The TV changing caption is the only clock-driven change here.
    assert!(!mine.tick(1));
    assert!(mine.tick(TV_DWELL_TICKS));
    assert!(!mine.tick(TV_DWELL_TICKS + 1));
}

#[test]
fn only_the_house_beer_comes_off_a_round_credit() {
    // A credit buys the house measure, so a priced pick must not cash it:
    // the footer names the drink ordered, and the credit would pour
    // something else.
    assert!(Drink::HouseBeer.on_the_round());
    assert!(!Drink::WhiskeyNeat.on_the_round());
    assert!(!Drink::OldFashioned.on_the_round());
    assert!(!Drink::TopShelf.on_the_round());
}

#[test]
fn the_menu_counts_the_drinks_waiting_without_saying_anything() {
    let seats = SharedSeats::new();
    let mut mine = session(&seats, 1);
    assert!(
        !mine.toggle_menu(),
        "no stool, no menu, and nothing to count"
    );
    mine.toggle_seat(0);
    assert_eq!(mine.free_drinks(), 0);
    assert!(mine.toggle_menu(), "opening the menu asks for the count");

    mine.outcome_sender()
        .send(Outcome::Credits { waiting: 3 })
        .expect("session receiver alive");
    mine.drain_outcomes();

    assert_eq!(mine.free_drinks(), 3);
    assert_eq!(mine.last_message, None, "a count is not a footer line");
    assert!(mine.menu_open(), "and it never closes the menu");

    // A count landing mid-order is not the answer to that order.
    assert_eq!(
        mine.pick(Order::Drink(Drink::HouseBeer)),
        Some(Order::Drink(Drink::HouseBeer))
    );
    mine.apply_outcome(Outcome::Credits { waiting: 2 });
    assert_eq!(mine.free_drinks(), 2);
    assert_eq!(mine.pick(Order::Round), None, "the pour is still in flight");
    assert_eq!(mine.last_message.as_deref(), Some("the house is on it."));

    // The comped pour spends one and says what is left in the same breath,
    // so the menu needs no second read.
    mine.apply_outcome(Outcome::Comped {
        drink: Drink::HouseBeer,
        remaining: 1,
    });
    assert_eq!(mine.free_drinks(), 1);
    assert_eq!(
        mine.last_message.as_deref(),
        Some("house beer, on somebody's round. 1 more waiting.")
    );
}
