use super::*;

fn user(n: u128) -> (Uuid, String) {
    (Uuid::from_u128(n), format!("user{n:03}"))
}

#[test]
fn sitting_takes_the_seat_and_snapshot_shows_it() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);

    assert!(seats.toggle_seat(id, &name, 0));

    let snap = seats.snapshot();
    assert!(snap[0].as_ref().is_some_and(|s| s.username == name));
    assert_eq!(seats.seat_of(id), Some(0));
}

#[test]
fn pressing_your_own_seat_again_stands_you_up() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);
    seats.toggle_seat(id, &name, 2);

    assert!(seats.toggle_seat(id, &name, 2));

    assert_eq!(seats.seat_of(id), None);
    assert!(seats.snapshot()[2].is_none());
}

#[test]
fn moving_seats_frees_the_old_one() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);
    seats.toggle_seat(id, &name, 0);

    assert!(seats.toggle_seat(id, &name, 3));

    assert_eq!(seats.seat_of(id), Some(3));
    assert!(seats.snapshot()[0].is_none());
}

#[test]
fn cannot_sit_in_a_seat_someone_else_holds() {
    let seats = SharedSeats::new();
    let (id_a, name_a) = user(1);
    let (id_b, name_b) = user(2);
    seats.toggle_seat(id_a, &name_a, 0);

    assert!(!seats.toggle_seat(id_b, &name_b, 0));

    assert_eq!(seats.seat_of(id_b), None);
    assert!(seats.snapshot()[0].as_ref().is_some_and(|s| s.username == name_a));
}

#[test]
fn out_of_range_seat_is_rejected() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);
    assert!(!seats.toggle_seat(id, &name, SEAT_COUNT));
}

#[test]
fn ordering_a_drink_requires_a_seat() {
    let seats = SharedSeats::new();
    let (id, _) = user(1);
    assert_eq!(seats.order_drink(id), None);
}

#[test]
fn ordering_drinks_counts_up_per_round() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);
    seats.toggle_seat(id, &name, 0);

    assert_eq!(seats.order_drink(id), Some(1));
    assert_eq!(seats.order_drink(id), Some(2));
    assert_eq!(seats.snapshot()[0].as_ref().map(|s| s.drinks), Some(2));
}

#[test]
fn sync_evicts_disconnected_occupants_only() {
    let seats = SharedSeats::new();
    let (id_a, name_a) = user(1);
    let (id_b, name_b) = user(2);
    seats.toggle_seat(id_a, &name_a, 0);
    seats.toggle_seat(id_b, &name_b, 1);

    seats.sync(&[id_a]);

    assert_eq!(seats.seat_of(id_a), Some(0));
    assert_eq!(seats.seat_of(id_b), None);
}
