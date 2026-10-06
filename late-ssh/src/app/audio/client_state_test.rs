use super::*;

#[test]
fn client_ssh_mode_parses_openssh() {
    let mode: ClientSshMode = serde_json::from_str(r#""openssh""#).unwrap();
    assert_eq!(mode, ClientSshMode::OpenSsh);
    assert_eq!(mode.metric_label(), Some("openssh"));
}

/// Helpers shipped in older `late` releases register as
/// `client_kind: "browser"` with `ssh_mode: "webview"`. Both must still land,
/// because a rejected `client_state` leaves that helper with no mute or volume
/// control for the life of the session.
#[test]
fn legacy_webview_helper_client_state_still_registers() {
    let state: ClientAudioState = serde_json::from_str(
        r#"{
            "client_kind": "browser",
            "ssh_mode": "webview",
            "platform": "linux",
            "capabilities": ["youtube"],
            "muted": true,
            "volume_percent": 30
        }"#,
    )
    .expect("legacy helper client_state must deserialize");

    assert_eq!(state.client_kind, ClientKind::Webview);
    assert_eq!(state.ssh_mode, ClientSshMode::Unknown);
    assert!(state.muted);
}

#[test]
fn current_webview_helper_registers_without_an_ssh_mode() {
    let state: ClientAudioState = serde_json::from_str(
        r#"{
            "client_kind": "webview",
            "platform": "linux",
            "capabilities": ["youtube"],
            "muted": false,
            "volume_percent": 30
        }"#,
    )
    .expect("current helper client_state must deserialize");

    assert_eq!(state.client_kind, ClientKind::Webview);
    assert_eq!(state.ssh_mode, ClientSshMode::Unknown);
}

/// An unrecognized mode must degrade rather than fail the whole message; the
/// CLI-usage metrics then simply skip it.
#[test]
fn unknown_ssh_mode_degrades_instead_of_failing() {
    let mode: ClientSshMode = serde_json::from_str(r#""something-new""#).unwrap();
    assert_eq!(mode, ClientSshMode::Unknown);
    assert_eq!(mode.metric_label(), None);
}
