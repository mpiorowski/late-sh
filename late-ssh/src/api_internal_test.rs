use super::*;
use crate::state::ActiveUser;
use axum::http::HeaderValue;
use ipnet::IpNet;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};
use uuid::Uuid;

#[test]
fn ws_payload_heartbeat_parses() {
    let json = r#"{"event": "heartbeat"}"#;
    let payload: WsPayload = serde_json::from_str(json).unwrap();
    assert!(matches!(payload, WsPayload::Heartbeat { .. }));
}

#[test]
fn ws_payload_viz_parses() {
    let json = r#"{
        "event": "viz",
        "position_ms": 1500,
        "bands": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
        "rms": 0.42
    }"#;
    let payload: WsPayload = serde_json::from_str(json).unwrap();
    match payload {
        WsPayload::Viz {
            position_ms,
            bands,
            rms,
        } => {
            assert_eq!(position_ms, 1500);
            assert_eq!(bands.len(), 8);
            assert!((rms - 0.42).abs() < f32::EPSILON);
        }
        _ => panic!("expected Viz"),
    }
}

#[test]
fn ws_payload_client_state_parses() {
    let json = r#"{
        "event": "client_state",
        "client_kind": "cli",
        "ssh_mode": "native",
        "platform": "macos",
        "muted": true,
        "volume_percent": 35
    }"#;
    let payload: WsPayload = serde_json::from_str(json).unwrap();
    match payload {
        WsPayload::ClientState {
            client_kind,
            ssh_mode,
            platform,
            capabilities,
            muted,
            volume_percent,
        } => {
            assert_eq!(client_kind, ClientKind::Cli);
            assert_eq!(ssh_mode, ClientSshMode::Native);
            assert_eq!(platform, ClientPlatform::Macos);
            assert!(capabilities.is_empty());
            assert!(muted);
            assert_eq!(volume_percent, 35);
        }
        _ => panic!("expected ClientState"),
    }
}

#[test]
fn ws_payload_player_transient_youtube_states_parse() {
    use crate::app::audio::svc::PlayerPlaybackState;

    for (state, expected) in [
        ("unstarted", PlayerPlaybackState::Unstarted),
        ("cued", PlayerPlaybackState::Cued),
        ("future_state", PlayerPlaybackState::Unknown),
    ] {
        let json = format!(
            r#"{{
                "event": "player_state",
                "item_id": "{}",
                "state": "{}",
                "offset_ms": 0,
                "duration_ms": null,
                "autoplay_blocked": false,
                "error": null
            }}"#,
            Uuid::nil(),
            state
        );
        let payload: WsPayload = serde_json::from_str(&json).unwrap();
        match payload {
            WsPayload::PlayerState(report) => {
                assert_eq!(report.item_id, Uuid::nil());
                assert_eq!(report.state, expected);
            }
            _ => panic!("expected PlayerState"),
        }
    }
}

#[test]
fn ws_payload_android_client_state_parses() {
    let json = r#"{
        "event": "client_state",
        "client_kind": "cli",
        "ssh_mode": "native",
        "platform": "android",
        "muted": false,
        "volume_percent": 30
    }"#;
    let payload: WsPayload = serde_json::from_str(json).unwrap();
    match payload {
        WsPayload::ClientState {
            client_kind,
            ssh_mode,
            platform,
            capabilities,
            muted,
            volume_percent,
        } => {
            assert_eq!(client_kind, ClientKind::Cli);
            assert_eq!(ssh_mode, ClientSshMode::Native);
            assert_eq!(platform, ClientPlatform::Android);
            assert!(capabilities.is_empty());
            assert!(!muted);
            assert_eq!(volume_percent, 30);
        }
        _ => panic!("expected ClientState"),
    }
}

#[test]
fn ws_payload_openssh_client_state_parses() {
    let json = r#"{
        "event": "client_state",
        "client_kind": "cli",
        "ssh_mode": "openssh",
        "platform": "linux",
        "muted": false,
        "volume_percent": 30
    }"#;
    let payload: WsPayload = serde_json::from_str(json).unwrap();
    match payload {
        WsPayload::ClientState {
            client_kind,
            ssh_mode,
            platform,
            capabilities,
            muted,
            volume_percent,
        } => {
            assert_eq!(client_kind, ClientKind::Cli);
            assert_eq!(ssh_mode, ClientSshMode::OpenSsh);
            assert_eq!(platform, ClientPlatform::Linux);
            assert!(capabilities.is_empty());
            assert!(!muted);
            assert_eq!(volume_percent, 30);
        }
        _ => panic!("expected ClientState"),
    }
}

/// Wire contract with the CLI's desktop media commands (MPRIS play/pause and
/// volume): the event names must match what `late-cli`'s pair loop sends.
#[test]
fn ws_payload_set_muted_and_set_volume_parse() {
    let muted =
        serde_json::from_str::<WsPayload>(r#"{"event": "set_muted", "muted": true}"#).unwrap();
    assert!(matches!(muted, WsPayload::SetMuted { muted: true }));

    let volume =
        serde_json::from_str::<WsPayload>(r#"{"event": "set_volume", "volume_percent": 45}"#)
            .unwrap();
    assert!(matches!(
        volume,
        WsPayload::SetVolume { volume_percent: 45 }
    ));
}

#[test]
fn ws_payload_unknown_event_fails() {
    let json = r#"{"event": "unknown"}"#;
    assert!(serde_json::from_str::<WsPayload>(json).is_err());
}

#[test]
fn ws_payload_viz_missing_fields_fails() {
    let json = r#"{"event": "viz", "position_ms": 1000}"#;
    assert!(serde_json::from_str::<WsPayload>(json).is_err());
}

#[test]
fn ws_payload_viz_wrong_bands_count_fails() {
    let json = r#"{
        "event": "viz",
        "position_ms": 1000,
        "bands": [0.1, 0.2],
        "rms": 0.5
    }"#;
    assert!(serde_json::from_str::<WsPayload>(json).is_err());
}

#[test]
fn decode_clipboard_image_accepts_supported_image() {
    let png_header = b"\x89PNG\r\n\x1a\n";
    match decode_clipboard_image_message_with_max(STANDARD.encode(png_header), 1024) {
        SessionMessage::ClipboardImage { data } => assert_eq!(data, png_header),
        other => panic!("expected ClipboardImage, got {other:?}"),
    }
}

#[test]
fn decode_clipboard_image_rejects_oversize_payload_before_decode() {
    match decode_clipboard_image_message_with_max("A".repeat(11), 1) {
        SessionMessage::ClipboardImageFailed { message } => {
            assert_eq!(message, "Clipboard image is too large");
        }
        other => panic!("expected ClipboardImageFailed, got {other:?}"),
    }
}

#[test]
fn decode_clipboard_image_rejects_invalid_base64() {
    match decode_clipboard_image_message_with_max("not base64!!!".to_string(), 1024) {
        SessionMessage::ClipboardImageFailed { message } => {
            assert_eq!(message, "Clipboard image payload was invalid");
        }
        other => panic!("expected ClipboardImageFailed, got {other:?}"),
    }
}

#[test]
fn decode_clipboard_image_rejects_non_image_bytes() {
    match decode_clipboard_image_message_with_max(STANDARD.encode(b"hello"), 1024) {
        SessionMessage::ClipboardImageFailed { message } => {
            assert_eq!(
                message,
                "Clipboard image is not a supported PNG/JPEG/GIF/WebP image"
            );
        }
        other => panic!("expected ClipboardImageFailed, got {other:?}"),
    }
}

#[test]
fn truncate_ws_error_message_defaults_and_limits_length() {
    assert_eq!(
        truncate_ws_error_message("  "),
        "Clipboard image upload failed"
    );
    assert_eq!(truncate_ws_error_message("  no image  "), "no image");
    assert_eq!(truncate_ws_error_message(&"x".repeat(200)).len(), 160);
}

#[test]
fn token_hint_redacts_full_value() {
    let hint = token_hint("12345678-abcd-efgh");
    assert_eq!(hint, "12345678..(18)");
}

#[test]
fn active_user_count_uses_unique_user_entries() {
    let active_users: ActiveUsers = Arc::new(Mutex::new(HashMap::new()));
    let mut users = active_users.lock().unwrap();
    users.insert(
        Uuid::now_v7(),
        ActiveUser {
            username: "alice".to_string(),
            fingerprint: None,
            audio_source: late_core::models::user::AudioSource::Icecast,
            sessions: Vec::new(),
            connection_count: 2,
            last_login_at: Instant::now(),
        },
    );
    users.insert(
        Uuid::now_v7(),
        ActiveUser {
            username: "bob".to_string(),
            fingerprint: None,
            audio_source: late_core::models::user::AudioSource::Icecast,
            sessions: Vec::new(),
            connection_count: 1,
            last_login_at: Instant::now(),
        },
    );
    drop(users);

    assert_eq!(active_user_count(&active_users), 2);
}

#[test]
fn forwarded_for_ip_uses_first_entry() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-for",
        HeaderValue::from_static("203.0.113.10, 10.42.0.89"),
    );

    assert_eq!(
        forwarded_for_ip(&headers),
        Some("203.0.113.10".parse().unwrap())
    );
}

#[test]
fn effective_client_ip_uses_forwarded_header_for_trusted_proxy() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-for",
        HeaderValue::from_static("203.0.113.10, 10.42.0.89"),
    );
    let trusted_cidrs = test_trusted_cidrs(vec!["10.42.0.0/16"]);
    let peer_addr: SocketAddr = "10.42.0.89:12345".parse().unwrap();

    assert_eq!(
        if is_trusted_proxy_peer(peer_addr.ip(), &trusted_cidrs)
            && let Some(ip) = forwarded_for_ip(&headers)
        {
            ip
        } else {
            peer_addr.ip()
        },
        "203.0.113.10".parse::<IpAddr>().unwrap()
    );
}

#[test]
fn effective_client_ip_falls_back_for_untrusted_proxy() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-for",
        HeaderValue::from_static("203.0.113.10, 10.42.0.89"),
    );
    let trusted_cidrs = test_trusted_cidrs(vec!["192.168.0.0/16"]);
    let peer_addr: SocketAddr = "10.42.0.89:12345".parse().unwrap();

    assert_eq!(
        if is_trusted_proxy_peer(peer_addr.ip(), &trusted_cidrs)
            && let Some(ip) = forwarded_for_ip(&headers)
        {
            ip
        } else {
            peer_addr.ip()
        },
        "10.42.0.89".parse::<IpAddr>().unwrap()
    );
}

#[test]
fn effective_client_ip_falls_back_when_header_missing() {
    let headers = HeaderMap::new();
    let trusted_cidrs = test_trusted_cidrs(vec!["10.42.0.0/16"]);
    let peer_addr: SocketAddr = "10.42.0.89:12345".parse().unwrap();

    assert_eq!(
        if is_trusted_proxy_peer(peer_addr.ip(), &trusted_cidrs)
            && let Some(ip) = forwarded_for_ip(&headers)
        {
            ip
        } else {
            peer_addr.ip()
        },
        "10.42.0.89".parse::<IpAddr>().unwrap()
    );
}

fn audio(muted: bool, volume_percent: u8) -> KeyAudio {
    KeyAudio {
        muted,
        volume_percent,
    }
}

#[test]
fn a_freshly_booted_cli_is_aligned_to_its_stored_device_audio() {
    // The CLI boots silent at 30%, so a stored (unmuted, 30%) has to unmute it
    // or the session opens with no sound at all.
    assert_eq!(
        align_paired_audio(
            ClientKind::Cli,
            audio(true, 30),
            None,
            true,
            audio(false, 30)
        ),
        AudioAlignment {
            volume_percent: None,
            toggle_mute: true,
        }
    );
    // Stored (muted, 30%) already matches the boot state, so nothing is sent.
    assert_eq!(
        align_paired_audio(
            ClientKind::Cli,
            audio(true, 30),
            None,
            true,
            audio(true, 30)
        ),
        AudioAlignment::default()
    );
}

#[test]
fn a_stored_mute_survives_the_volume_write_that_would_clear_it() {
    // A non-zero SetVolume also clears mute on the client, so restoring
    // (muted, 60%) must decide the mute half against the state *after* the
    // volume write or the session comes back audible.
    assert_eq!(
        align_paired_audio(
            ClientKind::Cli,
            audio(true, 30),
            None,
            true,
            audio(true, 60)
        ),
        AudioAlignment {
            volume_percent: Some(60),
            toggle_mute: true,
        }
    );
    // Restoring (unmuted, 60%) needs no toggle: the volume write unmutes.
    assert_eq!(
        align_paired_audio(
            ClientKind::Cli,
            audio(true, 30),
            None,
            true,
            audio(false, 60)
        ),
        AudioAlignment {
            volume_percent: Some(60),
            toggle_mute: false,
        }
    );
    // A zero volume does not clear mute, so an unmuted target still toggles.
    assert_eq!(
        align_paired_audio(
            ClientKind::Cli,
            audio(true, 30),
            None,
            true,
            audio(false, 0)
        ),
        AudioAlignment {
            volume_percent: Some(0),
            toggle_mute: true,
        }
    );
}

#[test]
fn a_reconnecting_cli_keeps_the_state_the_user_set() {
    // Same session, second pair-WS connection (network change, ingress
    // restart): the alignment is spent, so the client's own reported state is
    // the target and nothing is sent, whatever is stored.
    assert_eq!(
        align_paired_audio(
            ClientKind::Cli,
            audio(true, 75),
            Some(true),
            false,
            audio(false, 30)
        ),
        AudioAlignment::default()
    );
    assert_eq!(
        align_paired_audio(
            ClientKind::Cli,
            audio(false, 20),
            Some(false),
            false,
            audio(true, 90)
        ),
        AudioAlignment::default()
    );
}

#[test]
fn a_webview_helper_follows_the_live_cli_mute() {
    // Helper respawn while the session is muted: follow the CLI, not the
    // stored value and not the helper's own boot default.
    assert_eq!(
        align_paired_audio(
            ClientKind::Webview,
            audio(false, 30),
            Some(true),
            false,
            audio(false, 30)
        ),
        AudioAlignment {
            volume_percent: None,
            toggle_mute: true,
        }
    );
    // No CLI entry and the alignment already spent: keep its own state.
    assert_eq!(
        align_paired_audio(
            ClientKind::Webview,
            audio(true, 30),
            None,
            false,
            audio(false, 30)
        ),
        AudioAlignment::default()
    );
}

#[test]
fn alignment_echoes_are_not_persisted_as_intent() {
    // Restoring a stored (muted, 60%) sends SetVolume(60) then ToggleMute,
    // and the CLI re-reports `client_state` after each. Persisting those
    // echoes would write (unmuted, 60) and then (muted, 60) for a restore
    // that changed nothing, with the row wrong in between.
    let mut flow = PairAudioFlow::new(Some(audio(true, 60)));
    assert_eq!(
        flow.on_report(ClientKind::Cli, audio(true, 30)),
        ReportAction::Align {
            target: audio(true, 60)
        }
    );
    let plan = align_paired_audio(
        ClientKind::Cli,
        audio(true, 30),
        None,
        true,
        audio(true, 60),
    );
    assert_eq!(plan.control_count(), 2);
    flow.note_alignment_sent(plan);
    // Echo of the volume write (which also unmuted), then of the toggle.
    assert_eq!(
        flow.on_report(ClientKind::Cli, audio(false, 60)),
        ReportAction::Ignore
    );
    assert_eq!(
        flow.on_report(ClientKind::Cli, audio(true, 60)),
        ReportAction::Ignore
    );
    // The first post-echo report is real intent: the user pressed `m`.
    assert_eq!(
        flow.on_report(ClientKind::Cli, audio(false, 60)),
        ReportAction::Persist
    );
}

#[test]
fn a_webview_helper_report_is_never_persisted() {
    // The CLI's volume-up keeps mute, the helper's clears it, and `+` is
    // broadcast to both, so on a muted session the two report different mute
    // states for the same keypress. The CLI is the surface of record; the
    // helper's report must never reach the device row.
    let mut flow = PairAudioFlow::new(Some(audio(true, 30)));
    assert!(matches!(
        flow.on_report(ClientKind::Webview, audio(false, 30)),
        ReportAction::Align { .. }
    ));
    flow.note_alignment_sent(AudioAlignment {
        volume_percent: None,
        toggle_mute: true,
    });
    // Echo of the toggle: matches the target, alignment has landed.
    assert_eq!(
        flow.on_report(ClientKind::Webview, audio(true, 30)),
        ReportAction::Ignore
    );
    // `+` on the muted session: the helper unmutes itself and reports it.
    assert_eq!(
        flow.on_report(ClientKind::Webview, audio(false, 35)),
        ReportAction::Ignore
    );
}

#[test]
fn an_older_cli_without_a_client_kind_still_persists() {
    // Old CLIs predate `client_kind` and deserialize as `Unknown`; the
    // webview gate must not silently end persistence for them.
    let mut flow = PairAudioFlow::new(Some(audio(false, 30)));
    assert!(matches!(
        flow.on_report(ClientKind::Unknown, audio(false, 30)),
        ReportAction::Align { .. }
    ));
    flow.note_alignment_sent(AudioAlignment::default());
    assert_eq!(
        flow.on_report(ClientKind::Unknown, audio(true, 30)),
        ReportAction::Persist
    );
}

#[test]
fn a_failed_device_audio_read_disables_alignment_and_persistence() {
    // A transient DB error at connect must not read as "never stored":
    // aligning to fresh-boot defaults and persisting the client's echo would
    // overwrite the real row. The session keeps its own state instead.
    let mut flow = PairAudioFlow::new(None);
    assert_eq!(
        flow.on_report(ClientKind::Cli, audio(true, 60)),
        ReportAction::Ignore
    );
    assert_eq!(
        flow.on_report(ClientKind::Cli, audio(false, 45)),
        ReportAction::Ignore
    );
}

fn test_trusted_cidrs(cidr_strings: Vec<&str>) -> Vec<IpNet> {
    cidr_strings
        .into_iter()
        .map(|s| s.parse::<IpNet>().unwrap())
        .collect()
}
