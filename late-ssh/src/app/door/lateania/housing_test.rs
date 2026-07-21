use super::*;

#[test]
fn catalogue_has_fifty_plus_unique_pieces() {
    assert!(FURNITURE.len() >= 50, "at least fifty furnishings");
    let mut keys: Vec<&str> = FURNITURE.iter().map(|x| x.key).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), FURNITURE.len(), "furniture keys are unique");
    for x in FURNITURE {
        assert!(x.desc.len() > 30, "{} has a real description", x.key);
        assert!(furniture_by_key(x.key).is_some());
    }
}

#[test]
fn plots_do_not_overlap_and_map_back() {
    for (i, t) in TIERS.iter().enumerate() {
        let base = plot_base(i);
        for r in base..base + t.rooms() as RoomId {
            assert_eq!(plot_of_room(r), Some(i), "room {r} maps to plot {i}");
            assert!(is_housing_room(r));
        }
    }
    assert!(is_housing_room(HOUSING_BASE), "the close is a housing room");
    assert_eq!(plot_of_room(HOUSING_BASE), None, "the close is not a plot");
}
