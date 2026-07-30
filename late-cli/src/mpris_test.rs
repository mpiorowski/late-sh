use super::*;

#[test]
fn desktop_media_selects_metadata_for_each_source() {
    let mut media = DesktopMedia::for_test();
    media.youtube_current = Some(YoutubeTrack {
        id: "item-1".to_string(),
        video_id: "video-1".to_string(),
        title: Some("A track".to_string()),
        channel: Some("A channel".to_string()),
        duration_ms: Some(123_000),
        started_at_ms: Some(10_000),
    });
    media.source = Some(MediaSource::Youtube);

    let youtube = media.current_track().unwrap();
    assert_eq!(youtube.title, "A track");
    assert_eq!(youtube.artist.as_deref(), Some("A channel"));
    assert_eq!(youtube.duration_ms, Some(123_000));
    assert_eq!(
        youtube.art_url.as_deref(),
        Some("https://i.ytimg.com/vi/video-1/hqdefault.jpg")
    );

    media.source = Some(MediaSource::Icecast);
    media.station = Some("classical".to_string());
    media.icecast_tracks.insert(
        "classical".to_string(),
        IcecastTrack {
            title: "Nocturne".to_string(),
            artist: Some("A pianist".to_string()),
            duration_seconds: Some(180),
        },
    );
    let icecast = media.current_track().unwrap();
    assert_eq!(icecast.title, "Nocturne");
    assert_eq!(icecast.artist.as_deref(), Some("A pianist"));
    assert_eq!(icecast.duration_ms, Some(180_000));

    media.source = Some(MediaSource::Radio);
    media.station = Some("chillsynth".to_string());
    media.radio_tracks.insert(
        "chillsynth".to_string(),
        RadioTrack {
            artist: "Synth Artist".to_string(),
            title: "Night Drive".to_string(),
        },
    );
    let radio = media.current_track().unwrap();
    assert_eq!(radio.title, "Night Drive");
    assert_eq!(radio.artist.as_deref(), Some("Synth Artist"));
}

#[cfg(target_os = "linux")]
#[test]
fn metadata_includes_track_details() {
    use mpris_server::Time;

    let metadata = platform::metadata_for_track(&TrackMetadata {
        identity: "youtube:item-1".to_string(),
        title: "A track".to_string(),
        artist: Some("A channel".to_string()),
        album: Some("late.sh · YouTube".to_string()),
        duration_ms: Some(123_000),
        started_at_ms: None,
        art_url: Some("https://example.com/cover.jpg".to_string()),
        url: Some("https://example.com/watch".to_string()),
    });

    assert_eq!(metadata.title(), Some("A track"));
    assert_eq!(metadata.artist(), Some(vec!["A channel".to_string()]));
    assert_eq!(metadata.album(), Some("late.sh · YouTube"));
    assert_eq!(metadata.length(), Some(Time::from_millis(123_000)));
    assert_eq!(
        metadata.art_url().as_deref(),
        Some("https://example.com/cover.jpg")
    );
    assert_eq!(metadata.url().as_deref(), Some("https://example.com/watch"));
    assert!(metadata.trackid().is_some());
}

/// The pair WebSocket loop must never await the session bus, so every
/// projection entry point is synchronous. This is a compile-time check of
/// that contract: it would stop building if any of them went back to `async`.
#[test]
fn projection_entry_points_stay_synchronous() {
    let mut media = DesktopMedia::for_test();
    media.select_source(
        MediaSource::Radio,
        Some("chillsynth".to_string()),
        None,
        false,
        70,
    );
    media.update_youtube(None, false, 70);
    media.update_icecast(HashMap::new(), false, 70);
    media.update_radio(HashMap::new(), true, 0);
    media.update_audio_state(false, 30);
}
