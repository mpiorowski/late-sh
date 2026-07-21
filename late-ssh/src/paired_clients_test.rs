use crate::paired_clients::*;
use crate::app::audio::client_state::{ClientKind, ClientPlatform, ClientSshMode};

fn expected_source(
    source: AudioSource,
    web_icecast_enabled: bool,
    embedded_webview_enabled: bool,
) -> PairControlMessage {
    playback_message(
        "https://audio.late.sh",
        source,
        IcecastStream::default(),
        RadioStation::default(),
        web_icecast_enabled,
        embedded_webview_enabled,
    )
}

#[test]
fn paired_client_send_control_delivers_message() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    registry.register(
        "tok1".to_string(),
        tx,
        Uuid::now_v7(),
        AudioSource::default(),
    );

    assert!(registry.send_control("tok1", PairControlMessage::ToggleMute));
    assert_eq!(rx.try_recv().unwrap(), PairControlMessage::ToggleMute);
}

#[test]
fn paired_client_unregister_if_match_removes_only_matching_entry() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let (tx1, mut rx1) = tokio::sync::mpsc::unbounded_channel();
    let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel();
    let first = registry.register(
        "tok1".to_string(),
        tx1,
        Uuid::now_v7(),
        AudioSource::default(),
    );
    let second = registry.register(
        "tok1".to_string(),
        tx2,
        Uuid::now_v7(),
        AudioSource::default(),
    );

    registry.unregister_if_match("tok1", first);

    // Only the surviving entry should receive subsequent broadcasts.
    assert!(registry.send_control("tok1", PairControlMessage::ToggleMute));
    assert!(rx1.try_recv().is_err());
    assert_eq!(rx2.try_recv().unwrap(), PairControlMessage::ToggleMute);

    registry.unregister_if_match("tok1", second);
    assert!(!registry.send_control("tok1", PairControlMessage::ToggleMute));
}

#[test]
fn paired_client_snapshot_tracks_latest_state() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let registration_id = registry.register(
        "tok1".to_string(),
        tx,
        Uuid::now_v7(),
        AudioSource::default(),
    );
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        registration_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Macos,
            capabilities: vec!["clipboard_image".to_string()],
            muted: true,
            volume_percent: 35,
            ..Default::default()
        },
    );

    let snapshot = registry.snapshot("tok1").unwrap();
    assert_eq!(snapshot.client_kind, ClientKind::Cli);
    assert_eq!(snapshot.ssh_mode, ClientSshMode::Native);
    assert_eq!(snapshot.platform, ClientPlatform::Macos);
    assert!(snapshot.supports_clipboard_image());
    assert!(snapshot.muted);
    assert_eq!(snapshot.volume_percent, 35);
}

#[test]
fn voice_cli_detection_ignores_browser_preferred_snapshot() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let user_id = Uuid::now_v7();

    let (cli_tx, _cli_rx) = tokio::sync::mpsc::unbounded_channel();
    let cli_id = registry.register("tok1".to_string(), cli_tx, user_id, AudioSource::default());
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["voice".to_string()],
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );

    let (webview_tx, _webview_rx) = tokio::sync::mpsc::unbounded_channel();
    let webview_id = registry.register(
        "tok1".to_string(),
        webview_tx,
        user_id,
        AudioSource::Youtube,
    );
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        webview_id,
        ClientAudioState {
            client_kind: ClientKind::Browser,
            ssh_mode: ClientSshMode::Webview,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );

    assert_eq!(
        registry.snapshot("tok1").unwrap().client_kind,
        ClientKind::Browser
    );
    assert!(registry.has_voice_cli("tok1"));
}

#[test]
fn cli_muted_tracks_cli_entry_and_ignores_webview_entries() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let user_id = Uuid::now_v7();

    assert_eq!(registry.cli_muted("tok1"), None);

    let (webview_tx, _webview_rx) = tokio::sync::mpsc::unbounded_channel();
    let webview_id = registry.register(
        "tok1".to_string(),
        webview_tx,
        user_id,
        AudioSource::Youtube,
    );
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        webview_id,
        ClientAudioState {
            client_kind: ClientKind::Browser,
            ssh_mode: ClientSshMode::Webview,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            muted: true,
            volume_percent: 30,
            ..Default::default()
        },
    );
    assert_eq!(registry.cli_muted("tok1"), None);

    let (cli_tx, _cli_rx) = tokio::sync::mpsc::unbounded_channel();
    let cli_id = registry.register("tok1".to_string(), cli_tx, user_id, AudioSource::Youtube);
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            muted: true,
            volume_percent: 30,
            ..Default::default()
        },
    );
    assert_eq!(registry.cli_muted("tok1"), Some(true));

    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );
    assert_eq!(registry.cli_muted("tok1"), Some(false));
}

#[test]
fn paired_client_request_clipboard_image_reaches_cli_when_browser_paired() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");

    let (cli_tx, mut cli_rx) = tokio::sync::mpsc::unbounded_channel();
    let cli_id = registry.register(
        "tok1".to_string(),
        cli_tx,
        Uuid::now_v7(),
        AudioSource::default(),
    );
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["clipboard_image".to_string()],
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );

    let (browser_tx, mut browser_rx) = tokio::sync::mpsc::unbounded_channel();
    let browser_id = registry.register(
        "tok1".to_string(),
        browser_tx,
        Uuid::now_v7(),
        AudioSource::default(),
    );
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        browser_id,
        ClientAudioState {
            client_kind: ClientKind::Browser,
            ssh_mode: ClientSshMode::Unknown,
            platform: ClientPlatform::Unknown,
            capabilities: Vec::new(),
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );

    assert!(registry.request_clipboard_image("tok1"));
    assert!(matches!(
        cli_rx.try_recv().unwrap(),
        PairControlMessage::RequestClipboardImage { .. }
    ));
    assert!(browser_rx.try_recv().is_err());
}

#[test]
fn paired_client_request_clipboard_image_false_when_only_browser() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let (browser_tx, mut browser_rx) = tokio::sync::mpsc::unbounded_channel();
    let browser_id = registry.register(
        "tok1".to_string(),
        browser_tx,
        Uuid::now_v7(),
        AudioSource::default(),
    );
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        browser_id,
        ClientAudioState {
            client_kind: ClientKind::Browser,
            ssh_mode: ClientSshMode::Unknown,
            platform: ClientPlatform::Unknown,
            capabilities: Vec::new(),
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );

    assert!(!registry.request_clipboard_image("tok1"));
    assert!(browser_rx.try_recv().is_err());
    assert!(!registry.take_clipboard_request("tok1", None));
}

#[test]
fn paired_client_clipboard_request_consumed_once() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");

    let (cli_tx, mut cli_rx) = tokio::sync::mpsc::unbounded_channel();
    let cli_id = registry.register(
        "tok1".to_string(),
        cli_tx,
        Uuid::now_v7(),
        AudioSource::default(),
    );
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["clipboard_image".to_string()],
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );

    // No request outstanding yet: inbound payloads must be rejected.
    assert!(!registry.take_clipboard_request("tok1", None));

    assert!(registry.request_clipboard_image("tok1"));
    // First inbound payload consumes the slot; a second one is
    // unsolicited and gets dropped by the WS handler.
    assert!(registry.take_clipboard_request("tok1", None));
    assert!(!registry.take_clipboard_request("tok1", None));

    // An echoed id must match the outstanding request: a late answer to
    // an older request is refused and leaves the slot armed, then the
    // matching echo lands.
    assert!(registry.request_clipboard_image("tok1"));
    let _first_request = cli_rx.try_recv().expect("first request message");
    let current = match cli_rx.try_recv() {
        Ok(PairControlMessage::RequestClipboardImage { request_id }) => request_id,
        other => panic!("unexpected pair control message: {other:?}"),
    };
    assert!(!registry.take_clipboard_request("tok1", Some(current - 1)));
    assert!(registry.take_clipboard_request("tok1", Some(current)));

    // A timed-out request is cancelled server-side: even a correctly
    // echoed late response is then unsolicited.
    assert!(registry.request_clipboard_image("tok1"));
    registry.cancel_clipboard_request("tok1");
    assert!(!registry.take_clipboard_request("tok1", None));

    // Unregistering the last entry clears any stale outstanding request.
    assert!(registry.request_clipboard_image("tok1"));
    registry.unregister_if_match("tok1", cli_id);
    assert!(!registry.take_clipboard_request("tok1", None));
}

#[test]
fn state_update_never_sends_pair_control_message() {
    // CLI playback gating lives on the CLI side (it reads
    // SetPlaybackSource and silences the Icecast decoder when source !=
    // Icecast). The server's state-update path is pure bookkeeping and
    // must not push anything back at the client.
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let user_id = Uuid::now_v7();

    let (cli_tx, mut cli_rx) = tokio::sync::mpsc::unbounded_channel();
    let cli_id = registry.register("tok1".to_string(), cli_tx, user_id, AudioSource::Youtube);
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );
    assert!(cli_rx.try_recv().is_err());
}

#[test]
fn set_audio_source_pushes_playback_source_to_every_entry() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let user_id = Uuid::now_v7();

    let (cli_tx, mut cli_rx) = tokio::sync::mpsc::unbounded_channel();
    let cli_id = registry.register("tok1".to_string(), cli_tx, user_id, AudioSource::Icecast);
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );
    let (browser_tx, mut browser_rx) = tokio::sync::mpsc::unbounded_channel();
    let browser_id = registry.register(
        "tok1".to_string(),
        browser_tx,
        user_id,
        AudioSource::Icecast,
    );
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        browser_id,
        ClientAudioState {
            client_kind: ClientKind::Browser,
            ssh_mode: ClientSshMode::Unknown,
            platform: ClientPlatform::Unknown,
            capabilities: Vec::new(),
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );

    registry.set_audio_source(user_id, AudioSource::Youtube);
    assert_eq!(
        cli_rx.try_recv().unwrap(),
        expected_source(AudioSource::Youtube, false, false)
    );
    assert_eq!(
        browser_rx.try_recv().unwrap(),
        expected_source(AudioSource::Youtube, false, false)
    );

    registry.set_audio_source(user_id, AudioSource::Icecast);
    assert_eq!(
        cli_rx.try_recv().unwrap(),
        expected_source(AudioSource::Icecast, false, false)
    );
    assert_eq!(
        browser_rx.try_recv().unwrap(),
        expected_source(AudioSource::Icecast, false, false)
    );
}

#[test]
fn browser_only_token_can_play_web_icecast() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let user_id = Uuid::now_v7();

    let (browser_tx, mut browser_rx) = tokio::sync::mpsc::unbounded_channel();
    let browser_id = registry.register(
        "tok1".to_string(),
        browser_tx,
        user_id,
        AudioSource::Youtube,
    );
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        browser_id,
        ClientAudioState {
            client_kind: ClientKind::Browser,
            ssh_mode: ClientSshMode::Unknown,
            platform: ClientPlatform::Unknown,
            capabilities: Vec::new(),
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );

    registry.set_audio_source(user_id, AudioSource::Icecast);
    assert_eq!(
        browser_rx.try_recv().unwrap(),
        expected_source(AudioSource::Icecast, true, false)
    );
}

#[test]
fn browser_can_play_web_icecast_when_cli_output_is_unavailable() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let user_id = Uuid::now_v7();

    let (cli_tx, mut cli_rx) = tokio::sync::mpsc::unbounded_channel();
    let cli_id = registry.register("tok1".to_string(), cli_tx, user_id, AudioSource::Icecast);
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            muted: false,
            volume_percent: 30,
            icecast_output_available: false,
        },
    );

    let (browser_tx, mut browser_rx) = tokio::sync::mpsc::unbounded_channel();
    let browser_id = registry.register(
        "tok1".to_string(),
        browser_tx,
        user_id,
        AudioSource::Icecast,
    );
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        browser_id,
        ClientAudioState {
            client_kind: ClientKind::Browser,
            ssh_mode: ClientSshMode::Unknown,
            platform: ClientPlatform::Unknown,
            capabilities: Vec::new(),
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );

    assert!(registry.broadcast_playback_source_for_token("tok1"));
    assert_eq!(
        cli_rx.try_recv().unwrap(),
        expected_source(AudioSource::Icecast, true, false)
    );
    assert_eq!(
        browser_rx.try_recv().unwrap(),
        expected_source(AudioSource::Icecast, true, false)
    );
}

#[test]
fn embedded_webview_is_enabled_only_when_no_real_browser_is_paired() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let user_id = Uuid::now_v7();

    let (cli_tx, mut cli_rx) = tokio::sync::mpsc::unbounded_channel();
    let cli_id = registry.register("tok1".to_string(), cli_tx, user_id, AudioSource::Icecast);
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );

    let (webview_tx, mut webview_rx) = tokio::sync::mpsc::unbounded_channel();
    let webview_id = registry.register(
        "tok1".to_string(),
        webview_tx,
        user_id,
        AudioSource::Icecast,
    );
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        webview_id,
        ClientAudioState {
            client_kind: ClientKind::Browser,
            ssh_mode: ClientSshMode::Webview,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );

    registry.set_audio_source(user_id, AudioSource::Youtube);
    assert_eq!(
        cli_rx.try_recv().unwrap(),
        expected_source(AudioSource::Youtube, false, true)
    );
    assert_eq!(
        webview_rx.try_recv().unwrap(),
        expected_source(AudioSource::Youtube, false, true)
    );

    let (browser_tx, mut browser_rx) = tokio::sync::mpsc::unbounded_channel();
    let browser_id = registry.register(
        "tok1".to_string(),
        browser_tx,
        user_id,
        AudioSource::Youtube,
    );
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        browser_id,
        ClientAudioState {
            client_kind: ClientKind::Browser,
            ssh_mode: ClientSshMode::Unknown,
            platform: ClientPlatform::Unknown,
            capabilities: Vec::new(),
            muted: false,
            volume_percent: 30,
            ..Default::default()
        },
    );

    assert!(registry.broadcast_playback_source_for_token("tok1"));
    assert_eq!(
        cli_rx.try_recv().unwrap(),
        expected_source(AudioSource::Youtube, false, false)
    );
    assert_eq!(
        webview_rx.try_recv().unwrap(),
        expected_source(AudioSource::Youtube, false, false)
    );
    assert_eq!(
        browser_rx.try_recv().unwrap(),
        expected_source(AudioSource::Youtube, false, false)
    );

    registry.unregister_if_match("tok1", browser_id);

    assert!(registry.broadcast_playback_source_for_token("tok1"));
    assert_eq!(
        cli_rx.try_recv().unwrap(),
        expected_source(AudioSource::Youtube, false, true)
    );
}
