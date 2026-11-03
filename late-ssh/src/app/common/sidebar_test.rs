use super::*;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn sidebar_clock_text_falls_back_to_utc_when_timezone_missing() {
    let clock = sidebar_clock_text(None);
    assert!(clock.starts_with("UTC "));
}

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn empty_queue() -> QueueSnapshot {
    QueueSnapshot {
        audio_mode: crate::app::audio::svc::AudioMode::Icecast,
        current: None,
        queue: Vec::new(),
        history: Vec::new(),
        skip_progress: None,
    }
}

fn station(key: &str) -> RadioStation {
    RadioStation::from_key(key).expect("an enabled catalogue station")
}

fn stage_lines_with(
    source: AudioSource,
    selected_station: RadioStation,
    radio_slots: RadioSlots,
    radio_now_playing: Option<&str>,
) -> Vec<Line<'static>> {
    let queue = empty_queue();
    music_stage_lines(
        21,
        &MusicStageProps {
            paired_client: None,
            queue: &queue,
            source,
            selected_station,
            radio_slots,
            radio_now_playing,
            youtube_source_count: 3,
            radio_source_count: 1,
            marquee_tick: 0,
        },
    )
}

fn stage_lines(source: AudioSource) -> Vec<Line<'static>> {
    stage_lines_with(source, station("chillsynth"), RadioSlots::default(), None)
}

const ALL_SOURCES: [AudioSource; 2] = [AudioSource::Youtube, AudioSource::Radio];

fn stage_texts(source: AudioSource) -> Vec<String> {
    stage_lines(source).iter().map(line_text).collect()
}

#[test]
fn music_stage_height_is_constant() {
    for source in ALL_SOURCES {
        let texts = stage_texts(source);
        assert_eq!(texts.len(), MUSIC_DOCK_HEIGHT as usize, "{source:?}");
        assert!(texts[0].starts_with("vol"), "{source:?}");
        assert_eq!(texts[1], "- = volume  m mute", "{source:?}");
        assert!(texts[2].starts_with("▌ radio"), "{source:?}");
        assert!(texts[12].contains("v+x source"), "{source:?}");
    }
}

#[test]
fn on_radio_the_stage_reads_radio_then_its_stations_then_the_youtube_peek() {
    let texts: Vec<String> = stage_lines_with(
        AudioSource::Radio,
        station("datawave"),
        RadioSlots::default(),
        None,
    )
    .iter()
    .map(line_text)
    .collect();
    assert!(texts[2].starts_with("▌ radio"));
    // The track row falls back to the station label without live metadata.
    assert_eq!(texts[3], "datawave");
    // The heading names the station and credits its provider.
    assert_eq!(texts[4], "datawave · nightride");
    assert!(texts[5].starts_with("○ chillsynth"));
    assert!(texts[5].trim_end().ends_with("v1"));
    assert!(texts[6].starts_with("○ nightride"));
    assert!(texts[7].starts_with("● datawave"));
    assert!(texts[7].trim_end().ends_with("v3"));
    assert!(texts[8].starts_with("○ mellow"));
    assert!(texts[9].starts_with("○ plaza"));
    assert!(texts[9].trim_end().ends_with("v5"));
    assert!(texts[10].starts_with("▌ youtube"));
    assert_eq!(texts[11], "fallback stream");
    assert!(texts[12].contains("v+r tune"));
}

#[test]
fn on_youtube_radio_collapses_to_its_title_bar() {
    let texts: Vec<String> = stage_lines_with(
        AudioSource::Youtube,
        station("chillsynth"),
        RadioSlots::default(),
        Some("An Artist - A Track"),
    )
    .iter()
    .map(line_text)
    .collect();
    assert!(texts[2].starts_with("▌ radio"));
    assert!(texts[3].starts_with("▌ youtube"));
    assert_eq!(texts[4], "fallback stream");
    assert!(texts[12].contains("v+v queue"), "{}", texts[12]);
    assert!(
        texts.iter().all(|row| !row.contains("An Artist")),
        "the radio track is hidden on youtube: {texts:?}"
    );
}

#[test]
fn both_title_bars_keep_their_listener_counts() {
    let texts = stage_texts(AudioSource::Radio);
    assert!(texts[2].trim_end().ends_with('1'));
    assert!(texts[10].trim_end().ends_with('3'));
    let texts = stage_texts(AudioSource::Youtube);
    assert!(texts[2].trim_end().ends_with('1'));
    assert!(texts[3].trim_end().ends_with('3'));
}

#[test]
fn an_off_slot_station_lights_no_slot_row_and_credits_its_own_provider() {
    let mut slots = RadioSlots::default();
    slots.unpin(2);
    let texts: Vec<String> = stage_lines_with(AudioSource::Radio, station("chill"), slots, None)
        .iter()
        .map(line_text)
        .collect();
    assert!(
        texts[5..10].iter().all(|row| row.starts_with("○ ")),
        "{texts:?}"
    );
    assert!(texts[7].starts_with("○ pin via v+r"), "{}", texts[7]);
    assert!(texts[7].trim_end().ends_with("v3"));
    assert_eq!(texts[4], "lofi · late.sh", "the heading still names it");
}

/// The rail is 21 columns: a long station keeps its whole label and the
/// provider credit gives way first.
#[test]
fn the_heading_truncates_the_provider_before_the_station() {
    let heading = |key: &str| line_text(&station_heading_line(21, station(key)));
    assert_eq!(heading("darksynth"), "darksynth · nightride");
    assert_eq!(heading("horrorsynth"), "horrorsynth · nightr…");
    assert_eq!(heading("swissclassic"), "swiss classic · radi…");
}

#[test]
fn the_radio_track_row_prefers_live_metadata() {
    let texts: Vec<String> = stage_lines_with(
        AudioSource::Radio,
        station("chillsynth"),
        RadioSlots::default(),
        Some("An Artist - A Track"),
    )
    .iter()
    .map(line_text)
    .collect();
    assert_eq!(texts[3], "An Artist - A Track");
}

fn on(component: RightSidebarComponent) -> RightSidebarComponentSetting {
    RightSidebarComponentSetting {
        component,
        enabled: true,
    }
}

fn off(component: RightSidebarComponent) -> RightSidebarComponentSetting {
    RightSidebarComponentSetting {
        component,
        enabled: false,
    }
}

const OWNS_ALL: SidebarOwnership = SidebarOwnership { pet: true };

#[test]
fn visible_components_respects_order() {
    let components = [
        on(RightSidebarComponent::Bonsai),
        on(RightSidebarComponent::Music),
        on(RightSidebarComponent::Daily),
    ];
    // Tall enough for everything: order is preserved exactly.
    assert_eq!(
        visible_components(&components, OWNS_ALL, 100),
        vec![
            RightSidebarComponent::Bonsai,
            RightSidebarComponent::Music,
            RightSidebarComponent::Daily,
        ]
    );
}

#[test]
fn visible_components_skips_disabled() {
    let components = [
        on(RightSidebarComponent::Music),
        off(RightSidebarComponent::Daily),
        on(RightSidebarComponent::Bonsai),
    ];
    assert_eq!(
        visible_components(&components, OWNS_ALL, 100),
        vec![RightSidebarComponent::Music, RightSidebarComponent::Bonsai]
    );
}

#[test]
fn visible_components_drops_from_the_bottom_up() {
    // Room for the bonsai and one row short of the music stage under it:
    // the bottom panel goes, whatever it is.
    let components = [
        on(RightSidebarComponent::Bonsai),
        on(RightSidebarComponent::Music),
        on(RightSidebarComponent::Daily),
    ];
    let height = TIME_HEIGHT + RULE_HEIGHT + BONSAI_HEIGHT + RULE_HEIGHT + MUSIC_STAGE_HEIGHT - 1;
    assert_eq!(
        visible_components(&components, OWNS_ALL, height),
        vec![RightSidebarComponent::Bonsai]
    );
}

#[test]
fn visible_components_stops_at_the_first_panel_that_does_not_fit() {
    // The lobby under the music stage would fit on its own, but the walk
    // ends at the music stage: nothing below a dropped panel is shown.
    let components = [
        on(RightSidebarComponent::Music),
        on(RightSidebarComponent::Daily),
    ];
    let height = TIME_HEIGHT + RULE_HEIGHT + MUSIC_STAGE_HEIGHT - 1;
    assert_eq!(
        visible_components(&components, OWNS_ALL, height),
        Vec::<RightSidebarComponent>::new()
    );
}

#[test]
fn visible_components_skips_unowned_panels_without_stopping() {
    let components = [
        on(RightSidebarComponent::Pet),
        on(RightSidebarComponent::Tank),
        on(RightSidebarComponent::Daily),
    ];
    let no_pet = SidebarOwnership { pet: false };
    assert_eq!(
        visible_components(&components, no_pet, 100),
        vec![RightSidebarComponent::Tank, RightSidebarComponent::Daily]
    );
}

#[test]
fn visible_components_spacer_costs_no_rows() {
    let components = [
        on(RightSidebarComponent::Daily),
        on(RightSidebarComponent::Spacer),
        on(RightSidebarComponent::Bonsai),
    ];
    let height = TIME_HEIGHT + RULE_HEIGHT + DAILY_HEIGHT + RULE_HEIGHT + BONSAI_HEIGHT;
    assert_eq!(
        visible_components(&components, OWNS_ALL, height),
        vec![
            RightSidebarComponent::Daily,
            RightSidebarComponent::Spacer,
            RightSidebarComponent::Bonsai,
        ]
    );
}

#[test]
fn pet_watches_the_bonsai_and_tank_panels_it_touches() {
    let touching = [
        RightSidebarComponent::Bonsai,
        RightSidebarComponent::Pet,
        RightSidebarComponent::Tank,
    ];
    assert_eq!(
        pet_neighbours(&touching, 1),
        Neighbours {
            tank: Some(WatchSide::Below),
            bonsai: Some(WatchSide::Above),
        }
    );
    // Free space between the panels breaks the contact.
    let apart = [
        RightSidebarComponent::Bonsai,
        RightSidebarComponent::Spacer,
        RightSidebarComponent::Pet,
    ];
    assert_eq!(
        pet_neighbours(&apart, 2),
        Neighbours {
            tank: None,
            bonsai: None,
        }
    );
}

fn music_short_rows(width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| {
            draw_music_short(frame, Rect::new(0, 0, width, height), "Ambition", "youtube")
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

#[test]
fn a_short_music_tile_centres_the_track_over_the_station_and_draws_no_band() {
    let rows = music_short_rows(24, 9);
    assert!(rows[3].contains("♪ Ambition"), "{rows:#?}");
    assert!(rows[4].contains("youtube"), "{rows:#?}");
    // Nothing else is drawn: the equalizer is the visualizer tile's.
    for (y, row) in rows.iter().enumerate() {
        if y != 3 && y != 4 {
            assert_eq!(row.trim(), "", "{rows:#?}");
        }
    }

    // Two rows is text only; one row keeps the track.
    assert_eq!(music_short_rows(24, 2)[0].trim(), "♪ Ambition");
    assert_eq!(music_short_rows(24, 2)[1].trim(), "youtube");
    assert_eq!(music_short_rows(24, 1)[0].trim(), "♪ Ambition");
}

#[test]
fn the_player_names_the_station_for_the_streams_that_have_one() {
    let datawave = RadioStation::from_key("datawave").unwrap();
    let classical = RadioStation::from_key("classical").unwrap();
    assert_eq!(
        station_text(AudioSource::Radio, datawave),
        "radio · datawave"
    );
    // House mounts are stations like any other.
    assert_eq!(
        station_text(AudioSource::Radio, classical),
        "radio · classical"
    );
    // YouTube plays the queue, not a station: one word.
    assert_eq!(station_text(AudioSource::Youtube, datawave), "youtube");
}
