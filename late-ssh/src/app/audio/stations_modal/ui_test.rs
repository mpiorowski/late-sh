use super::*;

fn key(key: &str) -> RadioStation {
    RadioStation::from_key(key).unwrap()
}

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

#[test]
fn a_station_row_shows_cursor_state_provider_track_and_slot_key() {
    let mut slots = RadioSlots::empty();
    slots.pin(2, key("datawave"));
    let view = StationsView {
        rows: &[],
        current: key("datawave"),
        slots,
        source: AudioSource::Radio,
    };
    let row = StationRow {
        station: key("datawave"),
        now_playing: Some("Com Truise - Flightwave".to_string()),
    };
    let text = line_text(&station_line(&row, &view, true, 72));
    assert!(text.starts_with("▸ ● datawave"), "{text}");
    assert!(text.contains("nightride"), "{text}");
    assert!(text.contains("Com Truise - Flightwave"), "{text}");
    assert!(text.trim_end().ends_with("v3"), "{text}");

    // Not current, not pinned, no metadata: hollow glyph, `no signal`, no key.
    let row = StationRow {
        station: key("classical"),
        now_playing: None,
    };
    let text = line_text(&station_line(&row, &view, false, 72));
    assert!(text.starts_with("  ○ classical"), "{text}");
    assert!(text.contains("late.sh"), "{text}");
    assert!(text.contains("no signal"), "{text}");
    assert!(
        !text.trim_end().ends_with(|c: char| c.is_ascii_digit()),
        "{text}"
    );
}

#[test]
fn the_current_station_is_lit_only_while_radio_is_the_source() {
    let view = StationsView {
        rows: &[],
        current: key("chillsynth"),
        slots: RadioSlots::empty(),
        source: AudioSource::Youtube,
    };
    let row = StationRow {
        station: key("chillsynth"),
        now_playing: None,
    };
    let text = line_text(&station_line(&row, &view, false, 72));
    assert!(text.starts_with("  ○ chillsynth"), "{text}");
}

#[test]
fn the_pinned_row_names_every_slot_and_marks_empty_ones() {
    let mut slots = RadioSlots::empty();
    slots.pin(0, key("rektify"));
    slots.pin(2, key("chill"));
    let text = line_text(&pinned_line(slots));
    assert_eq!(
        text.split_whitespace().collect::<Vec<_>>(),
        vec![
            "pinned", "v1", "ambient", "v2", "—", "v3", "lofi"
        ]
    );
}

#[test]
fn long_tracks_are_truncated_with_an_ellipsis_to_the_budget() {
    assert_eq!(truncate_to_width("abcdef", 6), "abcdef");
    assert_eq!(truncate_to_width("abcdefg", 6), "abcde…");
    assert_eq!(truncate_to_width("abc", 1), "…");
    assert_eq!(truncate_to_width("abc", 0), "");
}

#[test]
fn the_list_groups_stations_under_one_heading_per_section() {
    let rows: Vec<StationRow> = ["chillsynth", "ebsm", "plaza", "swissjazz", "fipjazz"]
        .into_iter()
        .map(|station| StationRow {
            station: key(station),
            now_playing: None,
        })
        .collect();
    let view = StationsView {
        rows: &rows,
        current: key("chillsynth"),
        slots: RadioSlots::empty(),
        source: AudioSource::Radio,
    };
    let (lines, selected_line) = list_lines(&view, Some(3), 72);
    let first_words: Vec<String> = lines
        .iter()
        .map(|line| {
            line_text(line)
                .trim_start_matches(['▸', '●', '○', ' '])
                .split("  ")
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert_eq!(
        first_words,
        vec![
            "nightride",
            "chillsynth",
            "ebsm",
            "",
            "chill",
            "plaza",
            "",
            "jazz",
            "swiss jazz",
            "fip jazz"
        ]
    );
    assert_eq!(selected_line, 8, "the cursor follows swiss jazz past the headings");
}
