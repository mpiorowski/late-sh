use super::*;
use crate::app::audio::client_state::{ClientKind, ClientPlatform, ClientSshMode};

fn expected_source(source: AudioSource) -> PairControlMessage {
    playback_message("https://audio.late.sh", source, RadioStation::default())
}

#[test]
fn paired_client_send_control_delivers_message() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let (tx, mut rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    registry
        .register(
            "tok1".to_string(),
            tx,
            Uuid::now_v7(),
            AudioSource::default(),
        )
        .expect("paired register");

    assert!(registry.send_control("tok1", PairControlMessage::ToggleMute));
    assert_eq!(rx.try_recv().unwrap(), PairControlMessage::ToggleMute);
}

#[test]
fn paired_client_unregister_if_match_removes_only_matching_entry() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let (tx1, mut rx1) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let (tx2, mut rx2) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let first = registry
        .register(
            "tok1".to_string(),
            tx1,
            Uuid::now_v7(),
            AudioSource::default(),
        )
        .expect("paired register");
    let second = registry
        .register(
            "tok1".to_string(),
            tx2,
            Uuid::now_v7(),
            AudioSource::default(),
        )
        .expect("paired register");

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
    let (tx, _rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let registration_id = registry
        .register(
            "tok1".to_string(),
            tx,
            Uuid::now_v7(),
            AudioSource::default(),
        )
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        registration_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Macos,
            capabilities: vec!["clipboard_image".to_string()],
            video_blockers: Vec::new(),
            muted: true,
            volume_percent: 35,
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
fn voice_cli_detection_ignores_webview_preferred_snapshot() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let user_id = Uuid::now_v7();

    let (cli_tx, _cli_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let cli_id = registry
        .register("tok1".to_string(), cli_tx, user_id, AudioSource::default())
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["voice".to_string()],
            video_blockers: Vec::new(),
            muted: false,
            volume_percent: 30,
        },
    );

    let (webview_tx, _webview_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let webview_id = registry
        .register(
            "tok1".to_string(),
            webview_tx,
            user_id,
            AudioSource::Youtube,
        )
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        webview_id,
        ClientAudioState {
            client_kind: ClientKind::Webview,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            video_blockers: Vec::new(),
            muted: false,
            volume_percent: 30,
        },
    );

    assert_eq!(
        registry.snapshot("tok1").unwrap().client_kind,
        ClientKind::Webview
    );
    assert!(registry.has_voice_cli("tok1"));
}

#[test]
fn cli_muted_tracks_cli_entry_and_ignores_webview_entries() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let user_id = Uuid::now_v7();

    assert_eq!(registry.cli_muted("tok1"), None);

    let (webview_tx, _webview_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let webview_id = registry
        .register(
            "tok1".to_string(),
            webview_tx,
            user_id,
            AudioSource::Youtube,
        )
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        webview_id,
        ClientAudioState {
            client_kind: ClientKind::Webview,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            video_blockers: Vec::new(),
            muted: true,
            volume_percent: 30,
        },
    );
    assert_eq!(registry.cli_muted("tok1"), None);

    let (cli_tx, _cli_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let cli_id = registry
        .register("tok1".to_string(), cli_tx, user_id, AudioSource::Youtube)
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            video_blockers: Vec::new(),
            muted: true,
            volume_percent: 30,
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
            video_blockers: Vec::new(),
            muted: false,
            volume_percent: 30,
        },
    );
    assert_eq!(registry.cli_muted("tok1"), Some(false));
}

#[test]
fn paired_client_request_clipboard_image_reaches_cli_not_webview() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");

    let (cli_tx, mut cli_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let cli_id = registry
        .register(
            "tok1".to_string(),
            cli_tx,
            Uuid::now_v7(),
            AudioSource::default(),
        )
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["clipboard_image".to_string()],
            video_blockers: Vec::new(),
            muted: false,
            volume_percent: 30,
        },
    );

    let (webview_tx, mut webview_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let webview_entry_id = registry
        .register(
            "tok1".to_string(),
            webview_tx,
            Uuid::now_v7(),
            AudioSource::default(),
        )
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        webview_entry_id,
        ClientAudioState {
            client_kind: ClientKind::Webview,
            ssh_mode: ClientSshMode::Unknown,
            platform: ClientPlatform::Unknown,
            capabilities: Vec::new(),
            video_blockers: Vec::new(),
            muted: false,
            volume_percent: 30,
        },
    );

    assert!(registry.request_clipboard_image("tok1"));
    assert!(matches!(
        cli_rx.try_recv().unwrap(),
        PairControlMessage::RequestClipboardImage { .. }
    ));
    assert!(webview_rx.try_recv().is_err());
}

#[test]
fn paired_client_request_clipboard_image_false_when_only_webview() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let (webview_tx, mut webview_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let webview_entry_id = registry
        .register(
            "tok1".to_string(),
            webview_tx,
            Uuid::now_v7(),
            AudioSource::default(),
        )
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        webview_entry_id,
        ClientAudioState {
            client_kind: ClientKind::Webview,
            ssh_mode: ClientSshMode::Unknown,
            platform: ClientPlatform::Unknown,
            capabilities: Vec::new(),
            video_blockers: Vec::new(),
            muted: false,
            volume_percent: 30,
        },
    );

    assert!(!registry.request_clipboard_image("tok1"));
    assert!(webview_rx.try_recv().is_err());
    assert!(!registry.take_clipboard_request("tok1", None));
}

#[test]
fn paired_client_clipboard_request_consumed_once() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");

    let (cli_tx, mut cli_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let cli_id = registry
        .register(
            "tok1".to_string(),
            cli_tx,
            Uuid::now_v7(),
            AudioSource::default(),
        )
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["clipboard_image".to_string()],
            video_blockers: Vec::new(),
            muted: false,
            volume_percent: 30,
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

    let (cli_tx, mut cli_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let cli_id = registry
        .register("tok1".to_string(), cli_tx, user_id, AudioSource::Youtube)
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            video_blockers: Vec::new(),
            muted: false,
            volume_percent: 30,
        },
    );
    assert!(cli_rx.try_recv().is_err());
}

#[test]
fn set_audio_source_pushes_playback_source_to_every_entry() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let user_id = Uuid::now_v7();

    let (cli_tx, mut cli_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let cli_id = registry
        .register("tok1".to_string(), cli_tx, user_id, AudioSource::Radio)
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        cli_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            ssh_mode: ClientSshMode::Native,
            platform: ClientPlatform::Linux,
            capabilities: vec!["youtube".to_string()],
            video_blockers: Vec::new(),
            muted: false,
            volume_percent: 30,
        },
    );
    let (webview_tx, mut webview_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let webview_entry_id = registry
        .register("tok1".to_string(), webview_tx, user_id, AudioSource::Radio)
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok1",
        webview_entry_id,
        ClientAudioState {
            client_kind: ClientKind::Webview,
            ssh_mode: ClientSshMode::Unknown,
            platform: ClientPlatform::Unknown,
            capabilities: Vec::new(),
            video_blockers: Vec::new(),
            muted: false,
            volume_percent: 30,
        },
    );

    registry.set_audio_source(user_id, AudioSource::Youtube);
    assert_eq!(
        cli_rx.try_recv().unwrap(),
        expected_source(AudioSource::Youtube)
    );
    assert_eq!(
        webview_rx.try_recv().unwrap(),
        expected_source(AudioSource::Youtube)
    );

    registry.set_audio_source(user_id, AudioSource::Radio);
    assert_eq!(
        cli_rx.try_recv().unwrap(),
        expected_source(AudioSource::Radio)
    );
    assert_eq!(
        webview_rx.try_recv().unwrap(),
        expected_source(AudioSource::Radio)
    );
}

#[test]
fn paired_client_register_rejects_past_per_token_cap() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let user_id = Uuid::now_v7();
    let mut receivers = Vec::new();
    for _ in 0..MAX_PAIRED_CLIENTS_PER_TOKEN {
        let (tx, rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
        receivers.push(rx);
        registry
            .register("tok1".to_string(), tx, user_id, AudioSource::default())
            .expect("registrations under the cap succeed");
    }

    let (tx, _rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    assert!(
        registry
            .register("tok1".to_string(), tx, user_id, AudioSource::default())
            .is_none(),
        "registration past the per-token cap must be rejected"
    );

    // Other tokens are unaffected by tok1 being full.
    let (tx, _rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    registry
        .register("tok2".to_string(), tx, user_id, AudioSource::default())
        .expect("other tokens still register");
}

#[test]
fn stored_device_audio_is_applied_once_per_session_and_retired_with_it() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");

    assert!(registry.audio_alignment_pending("tok1"));
    registry.note_alignment_applied("tok1");

    // A pair-WS reconnect mid-session must not be re-aligned: the session
    // already runs with whatever mute and volume the user set.
    assert!(!registry.audio_alignment_pending("tok1"));
    assert!(registry.audio_alignment_pending("tok2"));

    // The next SSH session is a new CLI process, so it aligns again.
    registry.forget_session("tok1");
    assert!(registry.audio_alignment_pending("tok1"));
}

#[test]
fn only_a_changed_device_audio_claims_a_write() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let playing = KeyAudio {
        muted: false,
        volume_percent: 30,
    };
    let muted = KeyAudio {
        muted: true,
        volume_percent: 30,
    };

    // Seeded from the connect-time read, so the value just loaded out of the
    // DB is never written straight back.
    registry.note_persisted_audio("tok1", Some(playing));
    assert!(!registry.claim_audio_write("tok1", playing));

    // The CLI re-reports its state on every heartbeat; only a real change may
    // cost a write, and only the first report of that change.
    assert!(registry.claim_audio_write("tok1", muted));
    assert!(!registry.claim_audio_write("tok1", muted));

    // A failed write clears the claim so the next report retries.
    registry.note_persisted_audio("tok1", None);
    assert!(registry.claim_audio_write("tok1", muted));

    // A device that has never stored anything writes its first report.
    assert!(registry.claim_audio_write("tok2", playing));
}

/// Wire contract with both paired clients: `late-cli` and `late-webview`
/// deserialize these exact event names when the server fans a desktop media
/// command back out.
#[test]
fn set_control_messages_serialize_to_wire_names() {
    assert_eq!(
        serde_json::to_string(&PairControlMessage::SetMuted { muted: true }).unwrap(),
        r#"{"event":"set_muted","muted":true}"#
    );
    assert_eq!(
        serde_json::to_string(&PairControlMessage::SetVolume { volume_percent: 45 }).unwrap(),
        r#"{"event":"set_volume","volume_percent":45}"#
    );
}

fn cli_state(capabilities: &[&str]) -> ClientAudioState {
    ClientAudioState {
        client_kind: ClientKind::Cli,
        ssh_mode: ClientSshMode::Native,
        platform: ClientPlatform::Linux,
        capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
        video_blockers: Vec::new(),
        muted: false,
        volume_percent: 30,
    }
}

/// Video controls reach only a CLI advertising `video`: the webview helper
/// (which claims every other capability) and a CLI without it are skipped.
#[test]
fn video_controls_reach_only_the_video_capable_cli() {
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let user_id = Uuid::now_v7();
    let stop = PairControlMessage::VideoStop { play_id: 1 };

    let (plain_tx, mut plain_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let plain_id = registry
        .register("tok".to_string(), plain_tx, user_id, AudioSource::default())
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy("tok", plain_id, cli_state(&["voice"]));
    let (webview_tx, mut webview_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let webview_id = registry
        .register("tok".to_string(), webview_tx, user_id, AudioSource::Youtube)
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy(
        "tok",
        webview_id,
        ClientAudioState {
            client_kind: ClientKind::Webview,
            ..cli_state(&["video", "youtube"])
        },
    );

    assert!(!registry.has_video_cli("tok"));
    assert!(!registry.send_control_to_video_cli("tok", stop.clone()));
    assert!(plain_rx.try_recv().is_err());
    assert!(webview_rx.try_recv().is_err());

    let (video_tx, mut video_rx) = tokio::sync::mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let video_id = registry
        .register("tok".to_string(), video_tx, user_id, AudioSource::default())
        .expect("paired register");
    registry.update_state_and_enforce_mute_policy("tok", video_id, cli_state(&["video"]));

    assert!(registry.has_video_cli("tok"));
    assert!(registry.send_control_to_video_cli("tok", stop.clone()));
    assert_eq!(video_rx.try_recv().unwrap(), stop);
    assert!(plain_rx.try_recv().is_err());
    assert!(webview_rx.try_recv().is_err());
}

/// `late-cli/src/ws.rs` deserializes these exact shapes.
#[test]
fn video_messages_serialize_to_wire_names() {
    use crate::paired_clients::VideoControl;
    assert_eq!(
        serde_json::to_string(&PairControlMessage::VideoPlay {
            play_id: 7,
            url: "https://www.youtube.com/watch?v=x".to_string(),
            left: 10,
            top: 3,
            cols: 100,
            rows: 28,
        })
        .unwrap(),
        r#"{"event":"video_play","play_id":7,"url":"https://www.youtube.com/watch?v=x","left":10,"top":3,"cols":100,"rows":28}"#
    );
    assert_eq!(
        serde_json::to_string(&PairControlMessage::VideoPlace {
            play_id: 7,
            left: 0,
            top: 0,
            cols: 180,
            rows: 44,
        })
        .unwrap(),
        r#"{"event":"video_place","play_id":7,"left":0,"top":0,"cols":180,"rows":44}"#
    );
    for (control, wire) in [
        (VideoControl::TogglePause, "toggle_pause"),
        (VideoControl::SeekBack, "seek_back"),
        (VideoControl::SeekForward, "seek_forward"),
        (VideoControl::ToggleMute, "toggle_mute"),
    ] {
        assert_eq!(
            serde_json::to_string(&PairControlMessage::VideoControl {
                play_id: 7,
                control
            })
            .unwrap(),
            format!(r#"{{"event":"video_control","play_id":7,"control":"{wire}"}}"#)
        );
    }
    assert_eq!(
        serde_json::to_string(&PairControlMessage::VideoStop { play_id: 7 }).unwrap(),
        r#"{"event":"video_stop","play_id":7}"#
    );
}
