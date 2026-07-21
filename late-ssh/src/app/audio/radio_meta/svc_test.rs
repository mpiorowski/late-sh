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
