use super::*;

#[test]
fn catalogue_keys_are_unique_and_the_default_is_enabled() {
    let mut keys: Vec<&str> = CATALOGUE.iter().map(|station| station.key).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), CATALOGUE.len(), "duplicate station key");
    assert_eq!(RadioStation::default().as_str(), "chillsynth");
    assert_eq!(RadioStation::enabled().count(), 16);
}

#[test]
fn station_lookup_is_strict_about_unknown_and_disabled_keys() {
    assert_eq!(
        RadioStation::from_key("rektify").map(|s| s.label()),
        Some("ambient")
    );
    // Labels are not keys.
    assert_eq!(RadioStation::from_key("ambient"), None);
    assert_eq!(RadioStation::from_key("lofi"), None);
    // Stations the Nightride feed carries but late.sh does not offer.
    assert_eq!(RadioStation::from_key("rekt"), None);
    assert_eq!(RadioStation::from_key(""), None);
    // Settings parsing falls back instead of failing.
    assert_eq!(
        RadioStation::from_settings_str("rekt"),
        RadioStation::default()
    );
}

#[test]
fn stream_urls_resolve_house_mounts_against_the_base_and_keep_direct_urls() {
    let classical = RadioStation::from_key("classical").unwrap();
    assert_eq!(
        classical.stream_url("https://late.sh/stream/"),
        "https://late.sh/stream/classical"
    );
    assert_eq!(classical.provider(), Provider::House);
    let chillsynth = RadioStation::from_key("chillsynth").unwrap();
    assert_eq!(
        chillsynth.stream_url("https://late.sh/stream"),
        "https://stream.nightride.fm/chillsynth.mp3"
    );
    assert_eq!(chillsynth.provider().attribution(), "nightride.fm · live");
}

#[test]
fn station_serializes_as_its_key() {
    let station = RadioStation::from_key("rektify").unwrap();
    assert_eq!(serde_json::to_string(&station).unwrap(), "\"rektify\"");
    let parsed: RadioStation = serde_json::from_str("\"datawave\"").unwrap();
    assert_eq!(parsed.as_str(), "datawave");
    let fallback: RadioStation = serde_json::from_str("\"nope\"").unwrap();
    assert_eq!(fallback, RadioStation::default());
}

fn key(key: &str) -> RadioStation {
    RadioStation::from_key(key).unwrap()
}

#[test]
fn default_slots_keep_the_current_station_reachable() {
    let slots = RadioSlots::defaults_for(key("datawave"));
    assert_eq!(slots.get(0), Some(key("chillsynth")));
    assert_eq!(slots.get(3), Some(key("classical")));
    // A current station outside the defaults takes slot 1.
    let slots = RadioSlots::defaults_for(key("rektify"));
    assert_eq!(slots.get(0), Some(key("rektify")));
    assert_eq!(slots.get(1), Some(key("nightride")));
    assert_eq!(slots.position_of(key("chillsynth")), None);
}

#[test]
fn pinning_moves_a_station_between_slots_and_unpinning_empties_one() {
    let mut slots = RadioSlots::default();
    slots.pin(2, key("chillsynth"));
    assert_eq!(slots.get(0), None, "the station left its old slot");
    assert_eq!(slots.get(2), Some(key("chillsynth")));
    slots.pin(RADIO_SLOTS, key("rektify"));
    assert_eq!(
        slots.position_of(key("rektify")),
        None,
        "out of range is ignored"
    );
    slots.unpin(2);
    assert_eq!(slots.get(2), None);
    assert_eq!(slots.get(4), None);
}

#[test]
fn slots_round_trip_through_json_and_tolerate_bad_rows() {
    let value = serde_json::json!(["classical", null, "rektify", "classical", "extra"]);
    let slots = RadioSlots::from_json(&value).unwrap();
    assert_eq!(slots.get(0), Some(key("classical")));
    assert_eq!(slots.get(1), None);
    assert_eq!(slots.get(2), Some(key("rektify")));
    assert_eq!(slots.get(3), None, "a repeated key keeps its first slot");
    assert_eq!(
        slots.to_json(),
        serde_json::json!(["classical", null, "rektify", null])
    );
    assert_eq!(
        RadioSlots::from_json(&serde_json::json!(["rekt", "nope"])).unwrap(),
        RadioSlots::empty()
    );
    assert_eq!(RadioSlots::from_json(&serde_json::json!("chill")), None);
}
