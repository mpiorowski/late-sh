use super::world::RoomId;
use crate::app::door::lateania::archipelago::*;

#[test]
fn destinations_are_unique_and_cover_villages_and_islands() {
    let dests = portal_destinations();
    assert_eq!(dests.len(), VILLAGES.len() + ISLAND_COUNT);
    let mut rooms: Vec<RoomId> = dests.iter().map(|(_, r)| *r).collect();
    rooms.sort_unstable();
    rooms.dedup();
    assert_eq!(rooms.len(), dests.len(), "destination rooms are unique");
    for (_, r) in &dests {
        assert!(has_waystone(*r), "every destination has a waystone");
    }
}

#[test]
fn island_blocks_do_not_overlap_villages() {
    assert!(!is_village_room(island_entrance(0)));
    assert!(is_archipelago_room(island_entrance(ISLAND_COUNT - 1)));
    assert!(is_village_room(village_room(VILLAGES.len() - 1)));
}
