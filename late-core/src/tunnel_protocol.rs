//! Wire protocol for the bastion ⇄ late-ssh `/tunnel` WebSocket.
//!
//! Per `devdocs/LATE-CONNECTION-BASTION.md` §4: binary frames carry opaque
//! PTY bytes (no inspection); text frames carry a small JSON control
//! vocabulary. Today the only control variant is `resize`, used to forward
//! SSH `window-change` requests.
//!
//! Defined here (rather than in `late-ssh` or `late-bastion`) so both ends
//! stay in lockstep on the wire format.

use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderValue, Request};

/// HTTP headers sent by the bastion on the WS upgrade. Defined here so
/// the backend (`late-ssh`) and client (`late-bastion`) reference the
/// same constants — drift between the two would silently cause
/// rejected handshakes that look like "bad header" on the server side.
pub const HEADER_SECRET: &str = "x-late-secret";
pub const HEADER_FINGERPRINT: &str = "x-late-fingerprint";
pub const HEADER_USERNAME: &str = "x-late-username";
pub const HEADER_PEER_IP: &str = "x-late-peer-ip";
pub const HEADER_TERM: &str = "x-late-term";
pub const HEADER_COLS: &str = "x-late-cols";
pub const HEADER_ROWS: &str = "x-late-rows";
pub const HEADER_RECONNECT: &str = "x-late-reconnect";
pub const HEADER_SESSION_ID: &str = "x-late-session-id";
pub const HEADER_VIEW_ONLY: &str = "x-late-view-only";

/// Per-client state captured before opening a `/tunnel` WebSocket and encoded
/// into the backend handshake headers.
#[derive(Debug, Clone)]
pub struct HandshakeContext {
    /// SSH pubkey fingerprint or synthetic browser-viewer fingerprint asserted
    /// by the trusted tunnel client.
    pub fingerprint: String,
    /// Username hint. The backend resolves the canonical user by fingerprint.
    pub username: String,
    /// Real client IP asserted by the trusted tunnel client. Backend rate
    /// limiting keys on this value.
    pub peer_ip: IpAddr,
    /// Terminal identifier.
    pub term: String,
    /// Initial terminal columns.
    pub cols: u16,
    /// Initial terminal rows.
    pub rows: u16,
    /// True when this dials a replacement backend for the same shell channel.
    pub reconnect: bool,
    /// Stable per-session identifier for logs and trace correlation.
    pub session_id: String,
    /// True for browser spectator sessions where late-ssh must ignore
    /// state-mutating input while still parsing terminal bytes.
    pub view_only: bool,
}

/// Build a tungstenite `Request` for `connect_async` carrying the shared secret
/// and the fields described in [`HandshakeContext`].
pub fn build_request(
    ws_url: &str,
    secret: &str,
    ctx: &HandshakeContext,
) -> anyhow::Result<Request<()>> {
    let mut req = ws_url
        .into_client_request()
        .map_err(|e| anyhow::anyhow!("invalid backend tunnel URL '{ws_url}': {e}"))?;
    let headers = req.headers_mut();
    headers.insert(HEADER_SECRET, header_value(secret)?);
    headers.insert(HEADER_FINGERPRINT, header_value(&ctx.fingerprint)?);
    headers.insert(HEADER_USERNAME, header_value(&ctx.username)?);
    headers.insert(HEADER_PEER_IP, header_value(&ctx.peer_ip.to_string())?);
    headers.insert(HEADER_TERM, header_value(&ctx.term)?);
    headers.insert(HEADER_COLS, header_value(&ctx.cols.to_string())?);
    headers.insert(HEADER_ROWS, header_value(&ctx.rows.to_string())?);
    if ctx.reconnect {
        headers.insert(HEADER_RECONNECT, HeaderValue::from_static("1"));
    }
    headers.insert(HEADER_SESSION_ID, header_value(&ctx.session_id)?);
    if ctx.view_only {
        headers.insert(HEADER_VIEW_ONLY, HeaderValue::from_static("1"));
    }
    Ok(req)
}

fn header_value(s: &str) -> anyhow::Result<HeaderValue> {
    HeaderValue::from_str(s).map_err(|e| anyhow::anyhow!("invalid header value '{s}': {e}"))
}

/// Synthetic identities used by anonymous web spectators. Kept in the shared
/// protocol module so late-web and late-ssh do not drift on the trust marker.
pub fn is_spectator_identity(username: &str, fingerprint: &str) -> bool {
    username == "spectator" || fingerprint.starts_with("web-spectator:")
}

/// Text-frame control message. Tagged on `t` so adding new variants is
/// non-breaking as long as both ends are tolerant of unknown tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum ControlFrame {
    /// Forward of SSH `window-change` (RFC 4254 §6.7). Bastion sends this
    /// whenever the user-SSH client's terminal is resized.
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
}

/// In-process event flowing from "russh handler dispatched a message"
/// to "render loop applied it." Carries either a chunk of PTY input
/// bytes or a window-resize directive, in a single FIFO so a sequence
/// like `[Bytes(A), Resize, Bytes(B)]` reaches the app in that order.
///
/// Used end-to-end on both backend paths:
/// - Legacy russh path: `Handler::data` → `mpsc<SshInputEvent>` ←
///   `Handler::window_change_request`. Render loop drains.
/// - `/tunnel` path: bastion encodes WS Binary/Text from this enum,
///   backend's WS receive loop decodes back into the enum and forwards
///   to the same render-loop queue.
///
/// Keeping data and resize on one ordered channel avoids the eager-
/// resize race where window-change took the app lock ahead of bytes
/// that were already queued from earlier on the SSH wire — a hazard
/// for any TUI whose handlers translate coordinates against the
/// current viewport (mouse reports, paste, block selection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshInputEvent {
    Bytes(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

impl ControlFrame {
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> HandshakeContext {
        HandshakeContext {
            fingerprint: "SHA256:abc".to_string(),
            username: "alice".to_string(),
            peer_ip: "203.0.113.7".parse().unwrap(),
            term: "xterm-256color".to_string(),
            cols: 120,
            rows: 40,
            reconnect: false,
            session_id: "01HX7Q4N4S2NS9X9".to_string(),
            view_only: false,
        }
    }

    #[test]
    fn resize_round_trips() {
        let frame = ControlFrame::Resize {
            cols: 120,
            rows: 40,
        };
        let json = frame.to_json().unwrap();
        // Field order within the JSON object is not contractually fixed,
        // so we round-trip rather than asserting on byte-equal output.
        let parsed = ControlFrame::from_json(&json).unwrap();
        assert_eq!(parsed, frame);
    }

    #[test]
    fn resize_parses_canonical_form() {
        let json = r#"{"t":"resize","cols":120,"rows":40}"#;
        let parsed = ControlFrame::from_json(json).unwrap();
        assert_eq!(
            parsed,
            ControlFrame::Resize {
                cols: 120,
                rows: 40,
            }
        );
    }

    #[test]
    fn resize_emits_tag_field() {
        let frame = ControlFrame::Resize { cols: 80, rows: 24 };
        let json = frame.to_json().unwrap();
        assert!(json.contains(r#""t":"resize""#), "actual: {}", json);
        assert!(json.contains(r#""cols":80"#), "actual: {}", json);
        assert!(json.contains(r#""rows":24"#), "actual: {}", json);
    }

    #[test]
    fn unknown_tag_is_error() {
        let json = r#"{"t":"shrug","cols":80,"rows":24}"#;
        assert!(ControlFrame::from_json(json).is_err());
    }

    #[test]
    fn missing_tag_is_error() {
        let json = r#"{"cols":80,"rows":24}"#;
        assert!(ControlFrame::from_json(json).is_err());
    }

    #[test]
    fn missing_field_is_error() {
        let json = r#"{"t":"resize","cols":80}"#;
        assert!(ControlFrame::from_json(json).is_err());
    }

    #[test]
    fn out_of_range_dimension_is_error() {
        // u16 max is 65535; 70000 must fail to parse.
        let json = r#"{"t":"resize","cols":70000,"rows":24}"#;
        assert!(ControlFrame::from_json(json).is_err());
    }

    #[test]
    fn negative_dimension_is_error() {
        let json = r#"{"t":"resize","cols":-1,"rows":24}"#;
        assert!(ControlFrame::from_json(json).is_err());
    }

    #[test]
    fn builds_full_handshake() {
        let req = build_request("ws://backend:4001/tunnel", "hunter2", &ctx()).unwrap();
        let h = req.headers();
        assert_eq!(h.get(HEADER_SECRET).unwrap(), "hunter2");
        assert_eq!(h.get(HEADER_FINGERPRINT).unwrap(), "SHA256:abc");
        assert_eq!(h.get(HEADER_USERNAME).unwrap(), "alice");
        assert_eq!(h.get(HEADER_PEER_IP).unwrap(), "203.0.113.7");
        assert_eq!(h.get(HEADER_TERM).unwrap(), "xterm-256color");
        assert_eq!(h.get(HEADER_COLS).unwrap(), "120");
        assert_eq!(h.get(HEADER_ROWS).unwrap(), "40");
        assert_eq!(h.get(HEADER_SESSION_ID).unwrap(), "01HX7Q4N4S2NS9X9");
        assert!(h.get(HEADER_RECONNECT).is_none());
        assert!(h.get(HEADER_VIEW_ONLY).is_none());
    }

    #[test]
    fn sets_optional_handshake_headers_when_flagged() {
        let mut c = ctx();
        c.reconnect = true;
        c.view_only = true;
        let req = build_request("ws://backend:4001/tunnel", "hunter2", &c).unwrap();
        assert_eq!(req.headers().get(HEADER_RECONNECT).unwrap(), "1");
        assert_eq!(req.headers().get(HEADER_VIEW_ONLY).unwrap(), "1");
    }

    #[test]
    fn ipv6_peer_ip_renders_canonically() {
        let mut c = ctx();
        c.peer_ip = "2001:db8::1".parse().unwrap();
        let req = build_request("ws://backend:4001/tunnel", "hunter2", &c).unwrap();
        assert_eq!(req.headers().get(HEADER_PEER_IP).unwrap(), "2001:db8::1");
    }

    #[test]
    fn rejects_invalid_url() {
        let err = build_request("not a url", "hunter2", &ctx()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("invalid backend tunnel URL"), "got: {msg}");
    }

    #[test]
    fn rejects_non_ascii_header_value() {
        let mut c = ctx();
        c.username = "name\nwith\nnewline".to_string();
        let err = build_request("ws://backend:4001/tunnel", "hunter2", &c).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("invalid header value"), "got: {msg}");
    }

    #[test]
    fn spectator_identity_predicate_matches_only_synthetic_markers() {
        assert!(is_spectator_identity("spectator", "SHA256:regular"));
        assert!(is_spectator_identity("alice", "web-spectator:v1"));
        assert!(is_spectator_identity("spectator", "web-spectator:v1"));
        assert!(!is_spectator_identity("alice", "SHA256:regular"));
        assert!(!is_spectator_identity("spectator-alice", "SHA256:regular"));
        assert!(!is_spectator_identity("alice", "SHA256:web-spectator:v1"));
    }
}
