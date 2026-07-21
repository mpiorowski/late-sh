use super::*;

fn user(n: u128) -> (Uuid, String) {
    (Uuid::from_u128(n), format!("user{n:03}"))
}

fn roster(n: usize) -> Vec<(Uuid, String)> {
    (1..=n as u128).map(user).collect()
}

#[test]
fn sync_seats_everyone_without_collisions() {
    let lobby = SharedLobby::with_seed(7);
    lobby.sync(&roster(20));
    let snap = lobby.snapshot();
    assert_eq!(snap.headcount(), 20);
    let mut cells: Vec<(u16, u16)> = snap.people.iter().map(|p| p.placement.position()).collect();
    cells.sort_unstable();
    cells.dedup();
    assert_eq!(cells.len(), 20, "two patrons share a spot");
    assert!(
        snap.people
            .iter()
            .all(|p| matches!(p.placement, Placement::Seated(_)))
    );
}

#[test]
fn overflow_fills_standing_then_stacks_at_the_door() {
    let lobby = SharedLobby::with_seed(7);
    let total = map::SEATS.len() + map::STANDING_SPOTS.len() + map::DOOR_STACK.len() + 3;
    lobby.sync(&roster(total));
    let snap = lobby.snapshot();
    assert_eq!(snap.headcount(), total);
    let seated = snap
        .people
        .iter()
        .filter(|p| matches!(p.placement, Placement::Seated(_)))
        .count();
    let standing = snap
        .people
        .iter()
        .filter(|p| matches!(p.placement, Placement::Standing(_)))
        .count();
    let door = snap
        .people
        .iter()
        .filter(|p| matches!(p.placement, Placement::Door(_)))
        .count();
    assert_eq!(seated, map::SEATS.len());
    assert_eq!(standing, map::STANDING_SPOTS.len());
    assert_eq!(door, map::DOOR_STACK.len() + 3);
    assert_eq!(snap.door_overflow, 3);
}

#[test]
fn first_walk_frees_the_seat_and_moves_from_it() {
    let lobby = SharedLobby::with_seed(7);
    let (id, name) = user(1);
    lobby.sync(&[(id, name.clone())]);
    let seat_pos = lobby.position_of(id).unwrap();

    let walked = lobby.walk(id, &name, 1, 0);
    assert_ne!(walked, seat_pos, "walker did not step off the seat");

    // The freed seat is available to the next arrival.
    let (id2, name2) = user(2);
    lobby.sync(&[(id, name.clone()), (id2, name2.clone())]);
    let snap = lobby.snapshot();
    assert!(matches!(
        snap.find(id).unwrap().placement,
        Placement::Walking(..)
    ));
    assert!(matches!(
        snap.find(id2).unwrap().placement,
        Placement::Seated(_)
    ));
}

#[test]
fn sit_claims_a_nearby_seat_and_reserves_it() {
    let lobby = SharedLobby::with_seed(7);
    let (id, name) = user(1);
    // Stand right on a known bar stool, then sit.
    let seat = map::SEATS[0];
    lobby.place(id, &name, seat.x, seat.y);
    let sat = lobby.sit(id, &name).expect("a seat was in reach");
    assert_eq!(sat, (seat.x, seat.y));
    assert!(matches!(
        lobby.snapshot().find(id).unwrap().placement,
        Placement::Seated(0)
    ));

    // A newcomer must not be auto-assigned the seat we just took.
    let (id2, name2) = user(2);
    lobby.sync(&[(id, name.clone()), (id2, name2)]);
    let snap = lobby.snapshot();
    let mine = snap.find(id).unwrap().placement.position();
    let theirs = snap.find(id2).unwrap().placement.position();
    assert_ne!(mine, theirs, "a newcomer landed on our reserved seat");
}

#[test]
fn sit_needs_a_walker_near_a_free_seat() {
    let lobby = SharedLobby::with_seed(7);
    let (id, name) = user(1);
    // Not a walker at all: nothing to seat.
    assert!(lobby.sit(id, &name).is_none());

    // A walker out on the open floor, far from any stool.
    lobby.place(id, &name, map::SPAWN.0, map::SPAWN.1);
    assert!(
        lobby.sit(id, &name).is_none(),
        "sat despite no seat in reach"
    );
    assert!(matches!(
        lobby.snapshot().find(id).unwrap().placement,
        Placement::Walking(..)
    ));
}

#[test]
fn a_step_stands_a_seated_user_back_up() {
    let lobby = SharedLobby::with_seed(7);
    let (id, name) = user(1);
    let seat = map::SEATS[0];
    lobby.place(id, &name, seat.x, seat.y);
    lobby.sit(id, &name).unwrap();
    lobby.walk(id, &name, 0, 1);
    assert!(matches!(
        lobby.snapshot().find(id).unwrap().placement,
        Placement::Walking(..)
    ));
    // The vacated seat is free for a newcomer again.
    let (id2, name2) = user(2);
    lobby.sync(&[(id, name), (id2, name2)]);
    assert!(matches!(
        lobby.snapshot().find(id2).unwrap().placement,
        Placement::Seated(_)
    ));
}

#[test]
fn walk_respects_walls() {
    let lobby = SharedLobby::with_seed(7);
    let (id, name) = user(1);
    lobby.place(id, &name, map::SPAWN.0, map::SPAWN.1);
    for _ in 0..60 {
        lobby.walk(id, &name, 0, 1);
    }
    let (_, y) = lobby.position_of(id).unwrap();
    assert_eq!(y, map::MAP_H - 2, "walker escaped the bottom wall");
}

#[test]
fn disconnect_frees_spots_and_walkers() {
    let lobby = SharedLobby::with_seed(7);
    let (a, an) = user(1);
    let (b, bn) = user(2);
    lobby.sync(&[(a, an.clone()), (b, bn.clone())]);
    lobby.walk(b, &bn, 0, -1);
    lobby.sync(&[(a, an)]);
    let snap = lobby.snapshot();
    assert_eq!(snap.headcount(), 1);
    assert!(snap.find(b).is_none());
}

#[test]
fn door_stack_promotes_into_freed_seats() {
    let lobby = SharedLobby::with_seed(7);
    let total = map::SEATS.len() + map::STANDING_SPOTS.len() + 2;
    let full = roster(total);
    lobby.sync(&full);
    let snap = lobby.snapshot();
    let at_door: Vec<Uuid> = snap
        .people
        .iter()
        .filter(|p| matches!(p.placement, Placement::Door(_)))
        .map(|p| p.user_id)
        .collect();
    assert_eq!(at_door.len(), 2);

    // Two seated users leave; the door pair take chairs on next sync.
    let seated: Vec<Uuid> = snap
        .people
        .iter()
        .filter(|p| matches!(p.placement, Placement::Seated(_)))
        .map(|p| p.user_id)
        .take(2)
        .collect();
    let reduced: Vec<(Uuid, String)> = full
        .iter()
        .filter(|(id, _)| !seated.contains(id))
        .cloned()
        .collect();
    lobby.sync(&reduced);
    let snap = lobby.snapshot();
    for id in at_door {
        assert!(
            !matches!(snap.find(id).unwrap().placement, Placement::Door(_)),
            "door patron was not promoted"
        );
    }
}

#[test]
fn renames_apply_in_place() {
    let lobby = SharedLobby::with_seed(7);
    let (id, _) = user(1);
    lobby.sync(&[(id, "old".to_string())]);
    lobby.sync(&[(id, "new".to_string())]);
    assert_eq!(lobby.snapshot().people[0].username, "new");
}

/// Rewind the dog's step clock (and expire any nap) so the next
/// snapshot is guaranteed to advance it.
fn hurry_the_dog(lobby: &SharedLobby) {
    let mut inner = lobby.inner.lock_recover();
    inner.dog.last_step = Instant::now() - Duration::from_secs(1);
    if inner.dog.rest_until.is_some() {
        inner.dog.rest_until = Some(Instant::now() - Duration::from_millis(1));
    }
}

#[test]
fn the_dog_wanders_the_waypoints_and_stays_on_walkable_floor() {
    let lobby = SharedLobby::with_seed(7);
    let mut visited = std::collections::HashSet::new();
    for _ in 0..600 {
        hurry_the_dog(&lobby);
        let snap = lobby.snapshot();
        assert!(
            map::walkable(snap.dog.x, snap.dog.y),
            "dog stood on a blocked cell at ({}, {})",
            snap.dog.x,
            snap.dog.y
        );
        if snap.dog.resting {
            visited.insert((snap.dog.x, snap.dog.y));
        }
    }
    assert!(
        visited.len() >= 2,
        "dog never made it to a second waypoint: {visited:?}"
    );
    assert!(
        visited.iter().all(|c| map::DOG_WAYPOINTS.contains(c)),
        "dog napped off-waypoint: {visited:?}"
    );
}

#[test]
fn a_fresh_pet_freezes_the_dog() {
    let lobby = SharedLobby::with_seed(7);
    lobby.inner.lock_recover().dog.waypoint = (100, 22);
    lobby.pet_dog("alice");
    for _ in 0..5 {
        hurry_the_dog(&lobby);
        lobby.snapshot();
    }
    let snap = lobby.snapshot();
    assert_eq!((snap.dog.x, snap.dog.y), map::DOG_HOME);
}

#[test]
fn the_dog_waits_when_a_walker_comes_close() {
    let lobby = SharedLobby::with_seed(7);
    lobby.inner.lock_recover().dog.waypoint = (100, 22);
    let (id, name) = user(1);
    lobby.place(id, &name, map::DOG_HOME.0 + 2, map::DOG_HOME.1);
    for _ in 0..5 {
        hurry_the_dog(&lobby);
        lobby.snapshot();
    }
    let snap = lobby.snapshot();
    assert_eq!((snap.dog.x, snap.dog.y), map::DOG_HOME);

    // The friend wanders off; the dog resumes its errand.
    for _ in 0..30 {
        lobby.walk(id, &name, 1, 0);
    }
    hurry_the_dog(&lobby);
    let snap = lobby.snapshot();
    assert_ne!((snap.dog.x, snap.dog.y), map::DOG_HOME);
}

#[test]
fn drunk_levels_decay_and_prune() {
    let lobby = SharedLobby::with_seed(7);
    let (id, name) = user(1);
    lobby.sync(&[(id, name)]);

    let now = Utc::now();
    lobby.record_drink(id, 1_500, now);
    assert_eq!(lobby.snapshot().find(id).unwrap().drunk_level, 3);
    assert_eq!(lobby.drunk_levels().get(&id), Some(&3));

    // A drink from hours ago has partially worn off.
    lobby.record_drink(id, 1_500, now - chrono::Duration::hours(5));
    let level = lobby.snapshot().find(id).unwrap().drunk_level;
    assert_eq!(level, 2, "5h decay of 1500 points should read buzzed");

    // Fully sober entries drop out of the chat-facing map entirely.
    lobby.record_drink(id, 100, now - chrono::Duration::hours(10));
    assert_eq!(lobby.snapshot().find(id).unwrap().drunk_level, 0);
    assert!(lobby.drunk_levels().is_empty());

    // A seed pass replaces everything.
    lobby.record_drink(id, 2_000, now);
    lobby.set_drunk_states(Vec::new());
    assert!(lobby.drunk_levels().is_empty());
}

#[test]
fn emotes_and_dog_pets_show_in_snapshots() {
    let lobby = SharedLobby::with_seed(7);
    let (id, name) = user(1);
    lobby.sync(&[(id, name.clone())]);
    lobby.emote(id, Emote::Wave);
    lobby.pet_dog(&name);
    let snap = lobby.snapshot();
    assert_eq!(snap.find(id).unwrap().emote, Some(Emote::Wave));
    assert_eq!(
        snap.dog_pet.as_ref().map(|(n, _)| n.as_str()),
        Some("user001")
    );
}
