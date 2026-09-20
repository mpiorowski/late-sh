use super::*;
use crate::app::nightcap::lobby::SeatChange;

fn session(seats: &SharedSeats, n: u128) -> State {
    State::new(
        Some(seats.clone()),
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
    mine.order_drink();
    assert_eq!(mine.last_message.as_deref(), Some("take a seat first."));

    mine.toggle_seat(1);

    assert_eq!(mine.my_seat(), Some(1));
    assert_eq!(mine.last_message, None);
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
