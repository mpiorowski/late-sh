use super::tunnel_ws_url;

#[test]
fn tunnel_ws_url_uses_wss_for_https() {
    assert_eq!(
        tunnel_ws_url("https://api.late.sh/", "secret"),
        "wss://api.late.sh/api/ws/tunnel?token=secret"
    );
}

#[test]
fn tunnel_ws_url_uses_ws_for_http() {
    assert_eq!(
        tunnel_ws_url("http://localhost:4000", "secret"),
        "ws://localhost:4000/api/ws/tunnel?token=secret"
    );
}

#[test]
fn tunnel_ws_url_accepts_host_without_scheme() {
    assert_eq!(
        tunnel_ws_url("localhost:4000", "secret"),
        "ws://localhost:4000/api/ws/tunnel?token=secret"
    );
}

#[test]
fn tunnel_ws_url_defaults_public_hosts_to_wss() {
    assert_eq!(
        tunnel_ws_url("api.late.sh", "secret"),
        "wss://api.late.sh/api/ws/tunnel?token=secret"
    );
}

#[test]
fn tunnel_ws_url_escapes_token() {
    assert_eq!(
        tunnel_ws_url("https://api.late.sh", "a b&c"),
        "wss://api.late.sh/api/ws/tunnel?token=a%20b%26c"
    );
}
