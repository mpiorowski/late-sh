use std::collections::HashMap;

use late_core::models::presence::{ClubhouseStand, Emote, PresenceRecord, Spot};
use uuid::Uuid;

use super::{
    Crowd, DOG_CYCLE_MS, DOG_PET_MS, EMOTE_MS, Own, Placement, crowd, dog_at, pick_spot,
    shortest_path,
};
use crate::app::clubhouse::map;

fn stand(spot: Spot, since_ms: i64) -> ClubhouseStand {
    ClubhouseStand {
        spot,
        since_ms,
        emote: None,
        petted_dog_at_ms: None,
    }
}

fn record(n: u128, spot: Spot, since_ms: i64) -> PresenceRecord {
    PresenceRecord {
        session_id: Uuid::from_u128(1_000 + n),
        user_id: Uuid::from_u128(n),
        username: format!("user{n:03}"),
        clubhouse: stand(spot, since_ms),
        nightcap: None,
        street: None,
    }
}

/// A watcher who is not in the room (a far walker), so the crowd under
/// test is exactly the records.
fn watcher() -> Own<'static> {
    Own {
        session_id: Uuid::from_u128(u128::MAX),
        user_id: Uuid::from_u128(u128::MAX),
        username: "watcher",
        stand: stand(Spot::Walking { x: 1, y: 1 }, 0),
    }
}

fn room(records: &[PresenceRecord], now_ms: i64) -> Crowd {
    let mut crowd = crowd(records, &watcher(), &HashMap::new(), now_ms);
    crowd
        .people
        .retain(|p| p.user_id != Uuid::from_u128(u128::MAX));
    crowd
}

/// Sessions logging in one after another, each seating itself from what it
/// sees.
fn arrivals(n: usize) -> Vec<PresenceRecord> {
    let mut records = Vec::new();
    for i in 1..=n as u128 {
        let spot = pick_spot(&records, Uuid::from_u128(i), i as u64 * 7919);
        records.push(record(i, spot, i as i64));
    }
    records
}

#[test]
fn everyone_who_logs_in_sits_down_without_collisions() {
    let records = arrivals(20);
    let crowd = room(&records, 100);

    assert_eq!(crowd.headcount(), 20);
    assert!(
        crowd
            .people
            .iter()
            .all(|p| matches!(p.placement, Placement::Seated(_)))
    );
    let mut cells: Vec<(u16, u16)> = crowd
        .people
        .iter()
        .map(|p| p.placement.position())
        .collect();
    cells.sort_unstable();
    cells.dedup();
    assert_eq!(cells.len(), 20, "two patrons share a spot");
}

#[test]
fn a_full_house_stands_then_stacks_at_the_door() {
    let total = map::SEATS.len() + map::STANDING_SPOTS.len() + map::DOOR_STACK.len() + 3;
    let crowd = room(&arrivals(total), 100_000);

    let count = |f: fn(&Placement) -> bool| crowd.people.iter().filter(|p| f(&p.placement)).count();
    assert_eq!(
        count(|p| matches!(p, Placement::Seated(_))),
        map::SEATS.len()
    );
    assert_eq!(
        count(|p| matches!(p, Placement::Standing(_))),
        map::STANDING_SPOTS.len()
    );
    assert_eq!(
        count(|p| matches!(p, Placement::Door(_))),
        map::DOOR_STACK.len() + 3
    );
    assert_eq!(crowd.door_overflow, 3);
}

#[test]
fn a_seat_two_sessions_took_at_once_goes_to_the_earlier_claim() {
    let seat = Spot::Seat { index: 4 };
    let records = vec![record(1, seat, 2_000), record(2, seat, 1_000)];
    let crowd = room(&records, 3_000);

    assert_eq!(
        crowd.find(Uuid::from_u128(2)).map(|p| p.placement),
        Some(Placement::Seated(4))
    );
    assert_eq!(
        crowd.find(Uuid::from_u128(1)).map(|p| p.placement),
        Some(Placement::Door(0)),
        "the later claim waits at the door until its session picks again"
    );
}

#[test]
fn a_freed_seat_is_what_the_door_picks_next() {
    let total = map::SEATS.len() + map::STANDING_SPOTS.len() + 1;
    let mut records = arrivals(total);
    let waiting = records.last().expect("someone at the door").user_id;
    assert_eq!(records.last().map(|r| r.clubhouse.spot), Some(Spot::Door));

    // A seated patron logs out.
    let gone = records
        .iter()
        .position(|r| matches!(r.clubhouse.spot, Spot::Seat { .. }))
        .expect("a seated patron");
    let freed = records.remove(gone).clubhouse.spot;

    assert_eq!(pick_spot(&records, waiting, 12_345), freed);
}

#[test]
fn one_user_on_two_sessions_is_one_patron_where_they_last_moved() {
    let mut laptop = record(1, Spot::Seat { index: 2 }, 1_000);
    let mut phone = record(1, Spot::Walking { x: 30, y: 12 }, 2_000);
    phone.session_id = Uuid::from_u128(77);
    laptop.clubhouse.emote = Some((Emote::Wave, 2_500));
    let crowd = room(&[laptop.clone(), phone.clone()], 3_000);

    assert_eq!(crowd.headcount(), 1);
    let ann = crowd.find(Uuid::from_u128(1)).expect("one patron");
    assert_eq!(ann.placement, Placement::Walking(30, 12));
    assert_eq!(ann.emote, Some(Emote::Wave), "either device can wave");

    // A second device picking a spot does not count the first as taken.
    assert_eq!(
        pick_spot(&[laptop], Uuid::from_u128(1), 2),
        Spot::Seat { index: 2 }
    );
}

#[test]
fn emotes_and_pets_play_for_their_window_then_stop() {
    let mut ann = record(1, Spot::Seat { index: 0 }, 0);
    ann.clubhouse.emote = Some((Emote::Dance, 10_000));
    ann.clubhouse.petted_dog_at_ms = Some(10_000);

    let during = room(&[ann.clone()], 10_000 + EMOTE_MS - 1);
    assert_eq!(
        during.find(ann.user_id).and_then(|p| p.emote),
        Some(Emote::Dance)
    );
    let during = room(&[ann.clone()], 10_000 + DOG_PET_MS - 1);
    assert_eq!(
        during.dog_pet,
        Some(("user001".to_string(), (DOG_PET_MS - 1) as u128))
    );

    let after = room(&[ann.clone()], 10_000 + EMOTE_MS.max(DOG_PET_MS));
    assert_eq!(after.find(ann.user_id).and_then(|p| p.emote), None);
    assert_eq!(after.dog_pet, None);
}

#[test]
fn the_dog_trots_between_waypoints_on_open_floor_and_naps_at_them() {
    for (i, &from) in map::DOG_WAYPOINTS.iter().enumerate() {
        for &to in &map::DOG_WAYPOINTS[i..] {
            assert!(
                shortest_path(from, to).is_some(),
                "no open path from {from:?} to {to:?}"
            );
        }
    }
    let mut napped = std::collections::HashSet::new();
    let mut previous = dog_at(0);
    for t in (0..DOG_CYCLE_MS * 40).step_by(50) {
        let dog = dog_at(t);
        assert!(map::walkable(dog.x, dog.y), "dog on a blocked cell at {t}");
        let step = dog.x.abs_diff(previous.x) + dog.y.abs_diff(previous.y);
        assert!(step <= 1, "the dog jumped {step} cells at {t}");
        if dog.resting {
            napped.insert((dog.x, dog.y));
        }
        previous = dog;
    }
    assert!(napped.len() >= 2, "one waypoint only: {napped:?}");
    assert!(
        napped.iter().all(|cell| map::DOG_WAYPOINTS.contains(cell)),
        "napped off a waypoint: {napped:?}"
    );
}
