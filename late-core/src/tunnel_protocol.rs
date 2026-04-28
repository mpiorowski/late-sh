//! Wire protocol for the bastion ⇄ late-ssh `/tunnel` WebSocket.
//!
//! Per `PERSISTENT-CONNECTION-GATEWAY.md` §4: binary frames carry opaque
//! PTY bytes (no inspection); text frames carry a small JSON control
//! vocabulary. Today the only control variant is `resize`, used to forward
//! SSH `window-change` requests.
//!
//! Defined here (rather than in `late-ssh` or `late-bastion`) so both ends
//! stay in lockstep on the wire format.

use serde::{Deserialize, Serialize};

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
