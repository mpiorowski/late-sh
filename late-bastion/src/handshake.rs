//! Build the WS upgrade request the bastion sends to late-ssh's
//! `/tunnel` endpoint.
//!
//! This is pure header-construction logic — no I/O, no async — so it
//! lives in its own module with unit tests. The proxy module composes
//! it with `tokio_tungstenite::connect_async` to actually open the WS.

use late_core::tunnel_protocol::{
    HEADER_COLS, HEADER_FINGERPRINT, HEADER_PEER_IP, HEADER_RECONNECT_REASON, HEADER_ROWS,
    HEADER_SECRET, HEADER_SESSION_ID, HEADER_TERM, HEADER_USERNAME, HEADER_VIA,
};
use std::net::IpAddr;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderValue, Request};

/// Per-shell-channel state captured by the bastion at `shell_request`
/// time and turned into a tunnel-handshake by `build_request`.
#[derive(Debug, Clone)]
pub struct HandshakeContext {
    /// SSH pubkey fingerprint asserted by the bastion (authoritative
    /// for user identity at the backend).
    pub fingerprint: String,
    /// Username the user supplied on `ssh user@late.sh`. Backend treats
    /// this as a hint and re-derives via fingerprint lookup.
    pub username: String,
    /// Real client IP (transport peer when no PROXY v1, otherwise the
    /// PROXY v1 source). Backend's per-IP rate limiter keys on this.
    pub peer_ip: IpAddr,
    /// `$TERM` from the SSH `pty-req`.
    pub term: String,
    /// PTY column count at handshake time.
    pub cols: u16,
    /// PTY row count at handshake time.
    pub rows: u16,
    /// Close code that triggered this redial. Absent on the first dial.
    pub reconnect_reason: Option<u16>,
    /// UUIDv7 minted by the bastion per shell channel; stable across
    /// reconnects so logs/metrics on either end can correlate the
    /// underlying user session.
    pub session_id: String,
}

/// Build a tungstenite `Request` for `connect_async` carrying the
/// shared secret and the fields described in `HandshakeContext`.
///
/// `ws_url` is the full backend URL, e.g. `ws://service-ssh-internal:4001/tunnel`.
/// `secret` is the value of `X-Late-Secret`.
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
    headers.insert(HEADER_VIA, HeaderValue::from_static("bastion"));
    if let Some(reason) = ctx.reconnect_reason {
        headers.insert(HEADER_RECONNECT_REASON, header_value(&reason.to_string())?);
    }
    headers.insert(HEADER_SESSION_ID, header_value(&ctx.session_id)?);
    Ok(req)
}

fn header_value(s: &str) -> anyhow::Result<HeaderValue> {
    HeaderValue::from_str(s).map_err(|e| anyhow::anyhow!("invalid header value '{s}': {e}"))
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
            reconnect_reason: None,
            session_id: "01HX7Q4N4S2NS9X9".to_string(),
        }
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
        assert_eq!(h.get(HEADER_VIA).unwrap(), "bastion");
        assert_eq!(h.get(HEADER_SESSION_ID).unwrap(), "01HX7Q4N4S2NS9X9");
        // X-Late-Reconnect-Reason is intentionally absent on first dial.
        assert!(h.get(HEADER_RECONNECT_REASON).is_none());
    }

    #[test]
    fn sets_reconnect_reason_header_when_redialing() {
        let mut c = ctx();
        c.reconnect_reason = Some(4100);
        let req = build_request("ws://backend:4001/tunnel", "hunter2", &c).unwrap();
        assert_eq!(req.headers().get(HEADER_RECONNECT_REASON).unwrap(), "4100");
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
}
