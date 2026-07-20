use super::*;

#[test]
fn client_ssh_mode_parses_openssh() {
    let mode: ClientSshMode = serde_json::from_str(r#""openssh""#).unwrap();
    assert_eq!(mode, ClientSshMode::OpenSsh);
    assert_eq!(mode.metric_label(), Some("openssh"));
}

#[test]
fn client_ssh_mode_parses_webview() {
    let mode: ClientSshMode = serde_json::from_str(r#""webview""#).unwrap();
    assert_eq!(mode, ClientSshMode::Webview);
    assert_eq!(mode.metric_label(), None);
}
