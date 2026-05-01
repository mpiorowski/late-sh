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
pub const HEADER_SESSION_ID: &str = "x-late-session-id";
pub const HEADER_VIA: &str = "x-late-via";
pub const HEADER_RECONNECT_REASON: &str = "x-late-reconnect-reason";

pub const TUNNEL_CLOSE_SESSION_ENDED: u16 = 4000;
pub const TUNNEL_CLOSE_RECONNECT_REQUESTED: u16 = 4100;
pub const TUNNEL_CLOSE_ABNORMAL: u16 = 1006;

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
}
