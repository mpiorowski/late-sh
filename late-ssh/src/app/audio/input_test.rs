use super::poll_option_position;
use crate::{
    app::audio::{radio_meta::svc::ArtistTitle, youtube::YoutubeVideo},
    test_helpers::make_app,
};
use late_core::{
    api_types::{NowPlaying, Track},
    models::user::{AudioSource, RadioStation},
    test_utils::{create_test_user, test_db},
};
use std::collections::HashMap;
use tokio::sync::watch;

#[test]
fn poll_vote_suffixes_are_letters() {
    assert_eq!(poll_option_position(b'a'), Some(1));
    assert_eq!(poll_option_position(b'b'), Some(2));
    assert_eq!(poll_option_position(b'c'), Some(3));
    assert_eq!(poll_option_position(b'A'), Some(1));
    assert_eq!(poll_option_position(b'B'), Some(2));
    assert_eq!(poll_option_position(b'C'), Some(3));
}

#[test]
fn numeric_suffixes_remain_available_for_music_selection() {
    assert_eq!(poll_option_position(b'1'), None);
    assert_eq!(poll_option_position(b'2'), None);
    assert_eq!(poll_option_position(b'3'), None);
}

#[tokio::test]
async fn music_info_copies_selected_radio_track_through_osc52() {
    use base64::Engine;

    let test = test_db().await;
    let user = create_test_user(&test.db, "music-info-radio").await;
    let mut app = make_app(test.db.clone(), user.id, "music-info-radio");
    let full_track = "Com Truise - Flightwave (a full title longer than the sidebar)";
    let (_meta_tx, meta_rx) = watch::channel(HashMap::from([
        (
            "datawave".to_string(),
            ArtistTitle {
                artist: "Com Truise".to_string(),
                title: "Flightwave (a full title longer than the sidebar)".to_string(),
            },
        ),
        (
            "nightride".to_string(),
            ArtistTitle {
                artist: "Other artist".to_string(),
                title: "Other station".to_string(),
            },
        ),
    ]));
    app.radio_meta_rx = Some(meta_rx);
    let (_house_tx, house_rx) = watch::channel(HashMap::from([(
        "classical".to_string(),
        NowPlaying::new(Track {
            artist: Some("Frédéric Chopin".to_string()),
            title: "Prélude".to_string(),
            duration_seconds: None,
        }),
    )]));
    app.now_playing_rx = Some(house_rx);
    app.paired_source = AudioSource::Radio;

    for (station, suffix, expected) in [
        ("datawave", b"i", full_track),
        ("classical", b"I", "Frédéric Chopin - Prélude"),
    ] {
        app.selected_radio_station = RadioStation::from_key(station).unwrap();
        app.handle_input(b"v");
        assert!(app.music_prefix_armed);
        app.handle_input(suffix);
        assert!(!app.music_prefix_armed);
        assert_eq!(app.pending_clipboard.as_deref(), Some(expected));

        app.pending_terminal_commands.clear();
        app.render().expect("render clipboard command");
        let encoded = base64::engine::general_purpose::STANDARD.encode(expected.as_bytes());
        let command = format!("\x1b]52;c;{encoded}\x07").into_bytes();
        assert!(app.pending_terminal_commands.contains(&command));
        assert!(app.pending_clipboard.is_none());
        app.pending_terminal_commands.clear();
        app.render().expect("render again");
        assert!(!app.pending_terminal_commands.contains(&command));
    }
}

#[tokio::test]
async fn music_info_copies_youtube_when_radio_metadata_is_also_present() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "music-info-youtube").await;
    let mut app = make_app(test.db.clone(), user.id, "music-info-youtube");
    let (_tx, rx) = watch::channel(HashMap::from([(
        app.selected_radio_station.as_str().to_string(),
        NowPlaying::new(Track {
            artist: Some("Radio artist".to_string()),
            title: "Radio track".to_string(),
            duration_seconds: None,
        }),
    )]));
    app.now_playing_rx = Some(rx);
    app.audio
        .service()
        .submit_validated_video(
            user.id,
            YoutubeVideo {
                video_id: "aaaaaaaaaaa".to_string(),
                title: Some("A full YouTube title".to_string()),
                channel: Some("Music channel".to_string()),
                duration_ms: Some(60_000),
                is_stream: false,
            },
        )
        .await
        .expect("start YouTube track");
    app.paired_source = AudioSource::Youtube;

    app.handle_input(b"vi");
    assert_eq!(
        app.pending_clipboard.as_deref(),
        Some("Music channel - A full YouTube title")
    );
    assert!(!app.music_prefix_armed);
}

#[tokio::test]
async fn music_info_without_track_info_preserves_clipboard() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "music-info-missing").await;
    let mut app = make_app(test.db.clone(), user.id, "music-info-missing");
    app.pending_clipboard = Some("Earlier copy".to_string());

    for source in [AudioSource::Radio, AudioSource::Youtube] {
        app.paired_source = source;
        app.handle_input(b"vi");
        assert_eq!(app.pending_clipboard.as_deref(), Some("Earlier copy"));
        assert_eq!(
            app.banner.as_ref().map(|banner| banner.message.as_str()),
            Some("No now-playing track info available")
        );
        assert!(!app.music_prefix_armed);
        assert!(!app.chat.is_composing());
    }
}
