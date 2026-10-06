use super::*;

#[test]
fn parse_meta_line_reads_station_records() {
    let line = r#"data: [{"station":"chillsynth","artist":"An Artist","title":"A Track"},{"station":"datawave","artist":"Other","title":"Song"}]"#;
    let stations = parse_meta_line(line).unwrap();
    assert_eq!(stations.len(), 2);
    assert_eq!(stations["chillsynth"].artist, "An Artist");
    assert_eq!(stations["chillsynth"].title, "A Track");
    assert_eq!(stations["datawave"].title, "Song");
}

#[test]
fn parse_meta_line_skips_records_missing_fields() {
    let line = r#"data: [{"station":"chillsynth","artist":"","title":"A Track"},{"station":"datawave","artist":"Other","title":"Song"}]"#;
    let stations = parse_meta_line(line).unwrap();
    assert_eq!(stations.len(), 1);
    assert!(stations.contains_key("datawave"));
}

#[test]
fn parse_meta_line_ignores_non_data_lines() {
    assert!(parse_meta_line(": keep-alive").is_none());
    assert!(parse_meta_line("event: meta").is_none());
    assert!(parse_meta_line("").is_none());
    assert!(parse_meta_line("data:").is_none());
}

#[test]
fn parse_meta_line_ignores_invalid_json() {
    assert!(parse_meta_line("data: not json").is_none());
    assert!(parse_meta_line(r#"data: {"station":"chillsynth"}"#).is_none());
}

fn track(artist: &str, title: &str) -> ArtistTitle {
    ArtistTitle {
        artist: artist.to_string(),
        title: title.to_string(),
    }
}

/// A Nightride disconnect is a gap for Nightride only: stations fed by a
/// poller keep their track.
#[test]
fn clearing_nightride_keeps_polled_stations() {
    let mut map = HashMap::new();
    map.insert("chillsynth".to_string(), track("Suzuka", "Serve"));
    map.insert("rekt".to_string(), track("Kursa", "Retro-Virus"));
    map.insert("plaza".to_string(), track("GOLDIE GOLDIE", "Perfect"));
    map.insert("coderadio".to_string(), track("City Girl", "Palette"));

    clear_nightride(&mut map);

    let mut keys: Vec<_> = map.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["coderadio", "plaza"]);
}

#[test]
fn apply_track_reports_only_real_changes() {
    let mut map = HashMap::new();
    assert!(apply_track(&mut map, "plaza", track("A", "One")));
    assert!(!apply_track(&mut map, "plaza", track("A", "One")));
    assert!(apply_track(&mut map, "plaza", track("A", "Two")));
    assert_eq!(map["plaza"], track("A", "Two"));
}
