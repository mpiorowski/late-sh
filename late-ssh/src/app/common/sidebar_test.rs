use super::*;

#[test]
fn sidebar_clock_text_falls_back_to_utc_when_timezone_missing() {
    let clock = sidebar_clock_text(None);
    assert!(clock.starts_with("UTC "));
}

fn friend(username: &str, away: bool) -> ActiveFriend {
    ActiveFriend {
        user_id: uuid::Uuid::now_v7(),
        username: username.to_string(),
        audio_source: AudioSource::default(),
        online_since: std::time::Instant::now(),
        away,
    }
}

#[test]
fn friend_names_text_keeps_every_name_when_the_row_is_wide() {
    let friends = vec![friend("ada", false), friend("bob", false)];
    assert_eq!(friend_names_text(&friends, 40, 0), "@ada @bob");
}

#[test]
fn friend_names_text_marks_an_away_friend_with_the_glyph_alone() {
    let friends = vec![friend("ada", false), friend("bob", true)];
    assert_eq!(friend_names_text(&friends, 40, 0), "@ada @bob 💤");
}

#[test]
fn friend_names_text_scrolls_past_the_names_that_do_not_fit() {
    let friends = vec![
        friend("ada", false),
        friend("bob", false),
        friend("cyd", false),
    ];
    // The full 12-wide rail shows names; the marker was retired so no
    // column is reserved.
    assert_eq!(friend_names_text(&friends, 12, 0), "@ada @bob @c");
    // Held at the start, then scrolled to the end: the tail is readable.
    // With hold 45 and step 15, the far extreme spans ticks 75..120.
    assert_eq!(friend_names_text(&friends, 12, 90), "da @bob @cyd");
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

#[test]
fn music_stage_chrome_rows_never_move() {
    for source in ALL_SOURCES {
        let lines = stage_lines(source);
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        assert_eq!(texts.len(), MUSIC_DOCK_HEIGHT as usize, "{source:?}");
        assert!(texts[0].starts_with("vol"), "{source:?}");
        assert!(texts[1].starts_with("▌ radio"), "{source:?}");
        assert!(texts[3].starts_with("▌ youtube"), "{source:?}");
        assert!(texts[5].starts_with("── "), "{source:?}");
        assert!(texts[11].contains("v+x source"), "{source:?}");
    }
}

#[test]
fn the_rule_names_the_current_station_or_youtube() {
    let texts: Vec<String> = stage_lines(AudioSource::Radio)
        .iter()
        .map(line_text)
        .collect();
    assert!(texts[5].contains("chillsynth"), "{}", texts[5]);
    let texts: Vec<String> = stage_lines(AudioSource::Youtube)
        .iter()
        .map(line_text)
        .collect();
    assert!(texts[5].contains("youtube"), "{}", texts[5]);
}

#[test]
fn music_stage_dock_rows_always_show_now_playing() {
    for source in ALL_SOURCES {
        let texts: Vec<String> = stage_lines(source).iter().map(line_text).collect();
        assert_eq!(texts[2], "chillsynth", "{source:?}");
        assert_eq!(texts[4], "fallback stream", "{source:?}");
    }
}

#[test]
fn music_stage_dock_rows_keep_listener_counts() {
    for source in ALL_SOURCES {
        let texts: Vec<String> = stage_lines(source).iter().map(line_text).collect();
        assert!(texts[1].trim_end().ends_with('1'), "{source:?}");
        assert!(texts[3].trim_end().ends_with('3'), "{source:?}");
    }
}

#[test]
fn radio_slot_rows_mark_the_current_station_and_credit_its_provider() {
    let texts: Vec<String> = stage_lines_with(
        AudioSource::Radio,
        station("datawave"),
        RadioSlots::default(),
        None,
    )
    .iter()
    .map(line_text)
    .collect();
    // Detail rows 6..10: four slot rows then the attribution row.
    assert!(texts[6].starts_with("○ chillsynth"));
    assert!(texts[6].trim_end().ends_with("v1"));
    assert!(texts[7].starts_with("○ nightride"));
    assert!(texts[8].starts_with("● datawave"));
    assert!(texts[8].trim_end().ends_with("v3"));
    assert!(texts[9].starts_with("○ classical"));
    assert!(texts[9].trim_end().ends_with("v4"));
    assert!(texts[10].contains("nightride.fm"));
    assert!(texts[11].contains("v+r tune"));
    // The current station also names the radio dock row.
    assert_eq!(texts[2], "datawave");
}

#[test]
fn an_off_slot_station_lights_no_slot_row_and_credits_its_own_provider() {
    let mut slots = RadioSlots::default();
    slots.unpin(3);
    let texts: Vec<String> = stage_lines_with(AudioSource::Radio, station("chill"), slots, None)
        .iter()
        .map(line_text)
        .collect();
    assert!(
        texts[6..10].iter().all(|row| row.starts_with("○ ")),
        "{texts:?}"
    );
    assert!(texts[9].starts_with("○ pin via v+r"), "{}", texts[9]);
    assert!(texts[9].trim_end().ends_with("v4"));
    assert!(
        texts[5].contains("lofi"),
        "the rule still names it: {}",
        texts[5]
    );
    assert!(texts[10].contains("late.sh house"), "{}", texts[10]);
}

#[test]
fn radio_dock_row_prefers_live_metadata() {
    let texts: Vec<String> = stage_lines_with(
        AudioSource::Youtube,
        station("chillsynth"),
        RadioSlots::default(),
        Some("An Artist - A Track"),
    )
    .iter()
    .map(line_text)
    .collect();
    assert_eq!(texts[2], "An Artist - A Track");
}

#[test]
fn the_footer_names_the_active_source_action() {
    let texts: Vec<String> = stage_lines(AudioSource::Youtube)
        .iter()
        .map(line_text)
        .collect();
    assert!(texts[11].contains("v+v queue"), "{}", texts[11]);
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

const OWNS_ALL: SidebarOwnership = SidebarOwnership {
    pet: true,
    tank: true,
};

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
    let owns_pet = SidebarOwnership {
        pet: true,
        tank: false,
    };
    assert_eq!(
        visible_components(&components, owns_pet, 100),
        vec![RightSidebarComponent::Pet, RightSidebarComponent::Daily]
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
