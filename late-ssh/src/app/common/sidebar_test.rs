use super::*;

#[test]
fn sidebar_clock_text_falls_back_to_utc_when_timezone_missing() {
    let clock = sidebar_clock_text(None);
    assert!(clock.starts_with("UTC "));
}

#[test]
fn friend_names_text_keeps_every_name_when_the_row_is_wide() {
    let names = vec!["ada".to_string(), "bob".to_string()];
    assert_eq!(friend_names_text(&names, 40, 0), "@ada @bob");
}

#[test]
fn friend_names_text_scrolls_past_the_names_that_do_not_fit() {
    let names = vec!["ada".to_string(), "bob".to_string(), "cyd".to_string()];
    // The full 12-wide rail shows names; the marker was retired so no
    // column is reserved.
    assert_eq!(friend_names_text(&names, 12, 0), "@ada @bob @c");
    // Held at the start, then scrolled to the end: the tail is readable.
    assert_eq!(friend_names_text(&names, 12, 40), "da @bob @cyd");
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

fn stage_lines_with(
    source: AudioSource,
    selected_stream: IcecastStream,
    selected_station: RadioStation,
) -> Vec<Line<'static>> {
    let queue = empty_queue();
    music_stage_lines(
        21,
        &MusicStageProps {
            now_playing: None,
            paired_client: None,
            queue: &queue,
            source,
            selected_stream,
            selected_station,
            radio_now_playing: None,
            youtube_source_count: 3,
            icecast_source_count: 9,
            radio_source_count: 1,
            marquee_tick: 0,
        },
    )
}

fn stage_lines(source: AudioSource) -> Vec<Line<'static>> {
    stage_lines_with(source, IcecastStream::Chill, RadioStation::Chillsynth)
}

const ALL_SOURCES: [AudioSource; 3] = [
    AudioSource::Youtube,
    AudioSource::Icecast,
    AudioSource::Radio,
];

#[test]
fn music_stage_chrome_rows_never_move() {
    for source in ALL_SOURCES {
        let lines = stage_lines(source);
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        assert_eq!(texts.len(), MUSIC_STAGE_HEIGHT as usize, "{source:?}");
        assert!(texts[2].starts_with("▌ radio"), "{source:?}");
        assert!(texts[4].starts_with("▌ youtube"), "{source:?}");
        assert!(texts[6].starts_with("▌ icecast"), "{source:?}");
        assert!(texts[8].starts_with("── "), "{source:?}");
        assert!(texts[8].contains(source_label(source)), "{source:?}");
        assert!(texts[15].contains("v+x source"), "{source:?}");
    }
}

#[test]
fn music_stage_dock_rows_always_show_now_playing() {
    for source in ALL_SOURCES {
        let texts: Vec<String> = stage_lines(source).iter().map(line_text).collect();
        assert_eq!(texts[3], "chillsynth", "{source:?}");
        assert_eq!(texts[5], "fallback stream", "{source:?}");
        assert_eq!(texts[7], "no signal", "{source:?}");
    }
}

#[test]
fn music_stage_dock_rows_keep_listener_counts() {
    for source in ALL_SOURCES {
        let texts: Vec<String> = stage_lines(source).iter().map(line_text).collect();
        assert!(texts[2].trim_end().ends_with('1'), "{source:?}");
        assert!(texts[4].trim_end().ends_with('3'), "{source:?}");
        assert!(texts[6].trim_end().ends_with('9'), "{source:?}");
    }
}

#[test]
fn icecast_selector_rows_mark_selected_stream() {
    let texts: Vec<String> = stage_lines_with(
        AudioSource::Icecast,
        IcecastStream::Classical,
        RadioStation::Chillsynth,
    )
    .iter()
    .map(line_text)
    .collect();
    // Detail rows 9..14: progress/blank, chill, classical, padding.
    assert!(texts[10].starts_with("○ chill"));
    assert!(texts[10].trim_end().ends_with("v1"));
    assert!(texts[11].starts_with("● classical"));
    assert!(texts[11].trim_end().ends_with("v2"));
}

#[test]
fn radio_selector_rows_mark_selected_station() {
    let texts: Vec<String> = stage_lines_with(
        AudioSource::Radio,
        IcecastStream::Chill,
        RadioStation::Datawave,
    )
    .iter()
    .map(line_text)
    .collect();
    // Detail rows 9..14: five selectors then the attribution row.
    assert!(texts[9].starts_with("○ chillsynth"));
    assert!(texts[10].starts_with("○ nightride"));
    assert!(texts[11].starts_with("● datawave"));
    assert!(texts[11].trim_end().ends_with("v3"));
    assert!(texts[12].starts_with("○ spacesynth"));
    assert!(texts[13].starts_with("○ ambient"));
    assert!(texts[13].trim_end().ends_with("v5"));
    assert!(texts[14].contains("nightride.fm"));
    // The selected station also names the radio dock row.
    assert_eq!(texts[3], "datawave");
}

#[test]
fn radio_dock_row_prefers_sse_metadata() {
    let queue = empty_queue();
    let lines = music_stage_lines(
        21,
        &MusicStageProps {
            now_playing: None,
            paired_client: None,
            queue: &queue,
            source: AudioSource::Youtube,
            selected_stream: IcecastStream::Chill,
            selected_station: RadioStation::Chillsynth,
            radio_now_playing: Some("An Artist - A Track"),
            youtube_source_count: 3,
            icecast_source_count: 9,
            radio_source_count: 1,
            marquee_tick: 0,
        },
    );
    let texts: Vec<String> = lines.iter().map(line_text).collect();
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

#[test]
fn visible_components_respects_order() {
    let components = [
        on(RightSidebarComponent::Bonsai),
        on(RightSidebarComponent::Music),
        on(RightSidebarComponent::Visualizer),
        on(RightSidebarComponent::Daily),
    ];
    // Tall enough for everything: order is preserved exactly.
    assert_eq!(
        visible_components(&components, 100),
        vec![
            RightSidebarComponent::Bonsai,
            RightSidebarComponent::Music,
            RightSidebarComponent::Visualizer,
            RightSidebarComponent::Daily,
        ]
    );
}

#[test]
fn visible_components_skips_disabled() {
    let components = [
        off(RightSidebarComponent::Visualizer),
        on(RightSidebarComponent::Music),
        off(RightSidebarComponent::Daily),
        on(RightSidebarComponent::Bonsai),
    ];
    assert_eq!(
        visible_components(&components, 100),
        vec![RightSidebarComponent::Music, RightSidebarComponent::Bonsai]
    );
}

#[test]
fn visible_components_drops_by_priority_not_position() {
    // Music sits at the TOP of the display order. With room for only one
    // panel, music survives (lowest shrink priority) even though the old
    // cut-from-the-top rule would have dropped it first.
    let components = [
        on(RightSidebarComponent::Music),
        on(RightSidebarComponent::Bonsai),
    ];
    let height = TIME_HEIGHT + RULE_HEIGHT + MUSIC_STAGE_HEIGHT + 1;
    assert_eq!(
        visible_components(&components, height),
        vec![RightSidebarComponent::Music]
    );
}

#[test]
fn visible_components_skips_unfit_panel_without_stopping() {
    // Bonsai (10) doesn't fit but the visualizer (4) below the cut still
    // does: the walk skips bonsai instead of ending, so lower-priority
    // panels that fit are kept.
    let components = [
        on(RightSidebarComponent::Visualizer),
        on(RightSidebarComponent::Music),
        on(RightSidebarComponent::Bonsai),
    ];
    let height =
        TIME_HEIGHT + RULE_HEIGHT + MUSIC_STAGE_HEIGHT + RULE_HEIGHT + VISUALIZER_HEIGHT;
    assert_eq!(
        visible_components(&components, height),
        vec![
            RightSidebarComponent::Visualizer,
            RightSidebarComponent::Music,
        ]
    );
}
