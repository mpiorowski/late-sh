use super::*;
use crate::state::ActiveUser;
use ipnet::IpNet;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};
use uuid::Uuid;

#[test]
fn parse_allowed_origin_accepts_valid_origin() {
    let value = parse_allowed_origin("https://late.sh");
    assert_eq!(value, HeaderValue::from_static("https://late.sh"));
}

#[test]
#[should_panic(expected = "invalid LATE_ALLOWED_ORIGINS entry")]
fn parse_allowed_origin_panics_for_invalid_origin() {
    let _ = parse_allowed_origin("bad\norigin");
}

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
            icecast_output_available,
        } => {
            assert_eq!(client_kind, ClientKind::Cli);
            assert_eq!(ssh_mode, ClientSshMode::Native);
            assert_eq!(platform, ClientPlatform::Macos);
            assert!(capabilities.is_empty());
            assert!(muted);
            assert_eq!(volume_percent, 35);
            assert!(icecast_output_available);
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
            icecast_output_available,
        } => {
            assert_eq!(client_kind, ClientKind::Cli);
            assert_eq!(ssh_mode, ClientSshMode::Native);
            assert_eq!(platform, ClientPlatform::Android);
            assert!(capabilities.is_empty());
            assert!(!muted);
            assert_eq!(volume_percent, 30);
            assert!(icecast_output_available);
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
            icecast_output_available,
        } => {
            assert_eq!(client_kind, ClientKind::Cli);
            assert_eq!(ssh_mode, ClientSshMode::OpenSsh);
            assert_eq!(platform, ClientPlatform::Linux);
            assert!(capabilities.is_empty());
            assert!(!muted);
            assert_eq!(volume_percent, 30);
            assert!(icecast_output_available);
        }
        _ => panic!("expected ClientState"),
    }
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
            peer_ip: None,
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
            peer_ip: None,
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

fn test_trusted_cidrs(cidr_strings: Vec<&str>) -> Vec<IpNet> {
    cidr_strings
        .into_iter()
        .map(|s| s.parse::<IpNet>().unwrap())
        .collect()
}
