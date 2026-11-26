use super::*;

#[test]
fn parse_meta_line_reads_station_records() {
    let line = r#"data: [{"station":"chillsynth","artist":"An Artist","title":"A Track"},{"station":"datawave","artist":"Other","title":"Song"}]"#;
    let stations = parse_meta_line(line).unwrap().unwrap();
    assert_eq!(stations.len(), 2);
    assert_eq!(stations["chillsynth"].artist, "An Artist");
    assert_eq!(stations["chillsynth"].title, "A Track");
    assert_eq!(stations["datawave"].title, "Song");
}

#[test]
fn parse_meta_line_skips_records_missing_fields() {
    let line = r#"data: [{"station":"chillsynth","artist":"","title":"A Track"},{"station":"datawave","artist":"Other","title":"Song"}]"#;
    let stations = parse_meta_line(line).unwrap().unwrap();
    assert_eq!(stations.len(), 1);
    assert!(stations.contains_key("datawave"));
}

#[test]
fn parse_meta_line_ignores_non_data_lines() {
    assert!(matches!(parse_meta_line(": keep-alive"), Ok(None)));
    assert!(matches!(parse_meta_line("event: meta"), Ok(None)));
    assert!(matches!(parse_meta_line(""), Ok(None)));
    assert!(matches!(parse_meta_line("data:"), Ok(None)));
}

#[test]
fn parse_meta_line_skips_keepalives() {
    assert!(matches!(parse_meta_line("data: \tkeepalive "), Ok(None)));
    assert!(parse_meta_line("data: keepalive extra").is_err());
}

#[test]
fn parse_meta_line_rejects_invalid_json() {
    assert!(parse_meta_line("data: not json").is_err());
    assert!(parse_meta_line(r#"data: {"station":"chillsynth"}"#).is_err());
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
