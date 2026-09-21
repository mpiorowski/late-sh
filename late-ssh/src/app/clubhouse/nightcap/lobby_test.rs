use super::*;

fn user(n: u128) -> (Uuid, String) {
    (Uuid::from_u128(n), format!("user{n:03}"))
}

fn roster(users: &[(Uuid, String)]) -> Vec<(Uuid, String)> {
    users.to_vec()
}

#[test]
fn sitting_takes_the_seat_and_snapshot_shows_it() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);

    assert_eq!(seats.toggle_seat(id, &name, 0), SeatChange::SatDown);

    let snap = seats.snapshot();
    assert!(snap[0].as_ref().is_some_and(|s| s.username == name));
    assert_eq!(seats.seat_of(id), Some(0));
}

#[test]
fn pressing_your_own_seat_again_stands_you_up() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);
    seats.toggle_seat(id, &name, 2);

    assert_eq!(seats.toggle_seat(id, &name, 2), SeatChange::StoodUp);

    assert_eq!(seats.seat_of(id), None);
    assert!(seats.snapshot()[2].is_none());
}

#[test]
fn moving_seats_frees_the_old_one() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);
    seats.toggle_seat(id, &name, 0);

    assert_eq!(seats.toggle_seat(id, &name, 3), SeatChange::SatDown);

    assert_eq!(seats.seat_of(id), Some(3));
    assert!(seats.snapshot()[0].is_none());
}

#[test]
fn cannot_sit_in_a_seat_someone_else_holds() {
    let seats = SharedSeats::new();
    let (id_a, name_a) = user(1);
    let (id_b, name_b) = user(2);
    seats.toggle_seat(id_a, &name_a, 0);

    assert_eq!(seats.toggle_seat(id_b, &name_b, 0), SeatChange::Taken);

    assert_eq!(seats.seat_of(id_b), None);
    assert!(
        seats.snapshot()[0]
            .as_ref()
            .is_some_and(|s| s.username == name_a)
    );
}

#[test]
fn out_of_range_seat_is_rejected() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);
    assert_eq!(
        seats.toggle_seat(id, &name, SEAT_COUNT),
        SeatChange::OutOfRange
    );
}

#[test]
fn a_pour_for_someone_who_stood_up_has_no_stool_to_land_on() {
    let seats = SharedSeats::new();
    let (id, _) = user(1);
    assert_eq!(seats.record_pour(id), None);
}

#[test]
fn pours_count_up_for_the_sitting() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);
    seats.toggle_seat(id, &name, 0);

    assert_eq!(seats.record_pour(id), Some(1));
    assert_eq!(seats.record_pour(id), Some(2));
    assert_eq!(seats.snapshot()[0].as_ref().map(|s| s.drinks), Some(2));
}

#[test]
fn a_round_is_for_the_other_stools_only() {
    let seats = SharedSeats::new();
    let (buyer, buyer_name) = user(1);
    let (a, name_a) = user(2);
    let (b, name_b) = user(3);
    seats.toggle_seat(buyer, &buyer_name, 0);
    seats.toggle_seat(a, &name_a, 2);
    seats.toggle_seat(b, &name_b, 5);

    let mut patrons = seats.seated_ids_excluding(buyer);
    patrons.sort();
    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(patrons, expected);
    assert!(seats.seated_ids_excluding(a).contains(&buyer));
}

#[test]
fn sync_evicts_disconnected_occupants_only() {
    let seats = SharedSeats::new();
    let (id_a, name_a) = user(1);
    let (id_b, name_b) = user(2);
    seats.toggle_seat(id_a, &name_a, 0);
    seats.toggle_seat(id_b, &name_b, 1);

    seats.sync(&roster(&[(id_a, name_a)]));

    assert_eq!(seats.seat_of(id_a), Some(0));
    assert_eq!(seats.seat_of(id_b), None);
}

#[test]
fn vacating_frees_the_stool_for_someone_else() {
    let seats = SharedSeats::new();
    let (id_a, name_a) = user(1);
    let (id_b, name_b) = user(2);
    seats.toggle_seat(id_a, &name_a, 0);

    seats.vacate(id_a);

    assert_eq!(seats.seat_of(id_a), None);
    assert!(seats.snapshot()[0].is_none());
    assert_eq!(seats.toggle_seat(id_b, &name_b, 0), SeatChange::SatDown);
}

#[test]
fn vacating_without_a_stool_changes_nothing() {
    let seats = SharedSeats::new();
    let (id_a, name_a) = user(1);
    let (id_b, _) = user(2);
    seats.toggle_seat(id_a, &name_a, 0);

    seats.vacate(id_b);

    assert_eq!(seats.seat_of(id_a), Some(0));
}

#[test]
fn a_still_connected_user_keeps_their_stool_across_syncs() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);
    seats.toggle_seat(id, &name, 4);

    seats.sync(&roster(&[(id, name.clone())]));
    seats.sync(&roster(&[(id, name)]));

    assert_eq!(seats.seat_of(id), Some(4));
}

#[test]
fn sync_relabels_a_renamed_occupant() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);
    seats.toggle_seat(id, &name, 0);

    seats.sync(&roster(&[(id, "renamed".to_string())]));

    assert_eq!(
        seats.snapshot()[0].as_ref().map(|s| s.username.clone()),
        Some("renamed".to_string())
    );
}

#[test]
fn sync_keeps_the_drink_count_while_relabelling() {
    let seats = SharedSeats::new();
    let (id, name) = user(1);
    seats.toggle_seat(id, &name, 0);
    seats.record_pour(id);
    seats.record_pour(id);

    seats.sync(&roster(&[(id, "renamed".to_string())]));

    assert_eq!(seats.snapshot()[0].as_ref().map(|s| s.drinks), Some(2));
}
