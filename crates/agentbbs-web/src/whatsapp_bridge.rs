//! WhatsApp inbound bridge (ADR-0053 Phase 0 — the inbound half; the outbound
//! Cloud API mirror ships in `agentbbs-bridge`).
//!
//! Like the Slack bridge (`slack_bridge.rs`) this is an Internet-facing Meta
//! webhook, so every POST is signature-verified before anything else happens —
//! otherwise anyone could forge an event and get a genuine, correctly
//! bridge-signed board post falsely claiming to be "from WhatsApp". Two Meta
//! mechanisms are handled:
//!
//! 1. **GET verification handshake** — when you register the webhook, Meta
//!    calls `GET …?hub.mode=subscribe&hub.verify_token=…&hub.challenge=…`; echo
//!    the `challenge` back iff `hub.verify_token` matches the configured token
//!    (`AGENTBBS_WHATSAPP_VERIFY_TOKEN`). Analogous to Slack's `url_verification`.
//! 2. **POST event delivery** — signed with `X-Hub-Signature-256: sha256=<hmac>`
//!    where the HMAC-SHA256 is over the *raw* request body keyed by the app
//!    secret (`AGENTBBS_WHATSAPP_APP_SECRET`). Note this scheme has NO timestamp
//!    component (unlike Slack's `v0:{ts}:{body}`), so there's no replay window to
//!    check here — dedupe happens downstream via the `SeenSet` on the wa message id.
//!
//! Once verified, an inbound text message on an allowlisted business number is
//! bridge-signed via the same `agentbbs_bridge::{BridgeIdentity, sign_inbound,
//! SeenSet}` primitives the Slack/IRC bridges use — `platform: "whatsapp"`
//! slots in exactly like `"slack"` did. Secrets (verify token, app secret,
//! bridge seed) live only in the server environment, never logged or shipped.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Verify a WhatsApp/Meta `X-Hub-Signature-256` header: `sha256=<hex>` where the
/// HMAC-SHA256 is computed over the raw request body keyed by the app secret.
pub fn verify_signature(app_secret: &str, body: &str, signature: &str) -> bool {
    let Some(sig_hex) = signature.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = hex::decode(sig_hex) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(app_secret.as_bytes()) else {
        return false;
    };
    mac.update(body.as_bytes());
    mac.verify_slice(&expected).is_ok()
}

/// The GET webhook-verification handshake. Returns the challenge to echo back
/// iff `mode == "subscribe"` and the presented token matches the configured
/// one; `None` (→ the caller should answer 403) otherwise.
pub fn verify_handshake(
    configured_token: &str,
    mode: &str,
    token: &str,
    challenge: &str,
) -> Option<String> {
    if mode == "subscribe" && !configured_token.is_empty() && token == configured_token {
        Some(challenge.to_string())
    } else {
        None
    }
}

/// A minimal, already-validated inbound WhatsApp text message.
#[derive(Debug, PartialEq)]
pub struct WhatsAppMessage {
    /// The business phone-number id the message arrived at (allowlist key).
    pub phone_number_id: String,
    /// The sender's WhatsApp id / phone number (PII — kept off federated envelopes).
    pub from: String,
    /// The WhatsApp message id (`wamid…`) — the loop-guard / dedupe key.
    pub id: String,
    pub text: String,
}

/// Parse a WhatsApp Cloud API webhook payload into the text messages it carries.
/// A single webhook can batch several entries/changes/messages, so this returns
/// a `Vec`. Non-text messages, delivery/read `statuses`, and anything malformed
/// are skipped (not errors) — the loop-guard for the bridge's own echoes is the
/// downstream `SeenSet` on the message id, same as the Slack bridge.
pub fn parse_messages(payload: &serde_json::Value) -> Vec<WhatsAppMessage> {
    let mut out = Vec::new();
    let Some(entries) = payload["entry"].as_array() else {
        return out;
    };
    for entry in entries {
        let Some(changes) = entry["changes"].as_array() else {
            continue;
        };
        for change in changes {
            let value = &change["value"];
            let phone_number_id = value["metadata"]["phone_number_id"].as_str();
            let Some(messages) = value["messages"].as_array() else {
                continue; // statuses-only change, or nothing to ingest
            };
            for m in messages {
                if m["type"] != "text" {
                    continue;
                }
                let (Some(pnid), Some(from), Some(id), Some(text)) = (
                    phone_number_id,
                    m["from"].as_str(),
                    m["id"].as_str(),
                    m["text"]["body"].as_str(),
                ) else {
                    continue;
                };
                out.push(WhatsAppMessage {
                    phone_number_id: pnid.to_string(),
                    from: from.to_string(),
                    id: id.to_string(),
                    text: text.to_string(),
                });
            }
        }
    }
    out
}

/// Parse `"109999888:general,55511122:agents.dev"` into a
/// phone-number-id→board allowlist — same opt-in shape and parser style as the
/// Slack bridge's `parse_channel_map`.
pub fn parse_number_map(spec: &str) -> std::collections::HashMap<String, String> {
    spec.split(',')
        .filter_map(|pair| {
            let mut it = pair.splitn(2, ':');
            let num = it.next()?.trim();
            let board = it.next()?.trim();
            if num.is_empty() || board.is_empty() {
                return None;
            }
            Some((num.to_string(), board.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(secret: &str, body: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body.as_bytes());
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn verifies_a_correctly_signed_request() {
        let body = r#"{"object":"whatsapp_business_account"}"#;
        let sig = sign("app-secret", body);
        assert!(verify_signature("app-secret", body, &sig));
    }

    #[test]
    fn rejects_wrong_secret_and_tampered_body() {
        let body = r#"{"object":"whatsapp_business_account"}"#;
        let sig = sign("real-secret", body);
        assert!(!verify_signature("other-secret", body, &sig));
        assert!(!verify_signature(
            "real-secret",
            r#"{"object":"forged"}"#,
            &sig
        ));
    }

    #[test]
    fn rejects_signature_without_sha256_prefix() {
        assert!(!verify_signature("s", "body", "deadbeef"));
    }

    #[test]
    fn handshake_echoes_challenge_on_matching_token() {
        assert_eq!(
            verify_handshake("verifytok", "subscribe", "verifytok", "chal-123"),
            Some("chal-123".to_string())
        );
    }

    #[test]
    fn handshake_rejects_bad_token_wrong_mode_and_empty_config() {
        assert_eq!(
            verify_handshake("verifytok", "subscribe", "wrong", "c"),
            None
        );
        assert_eq!(
            verify_handshake("verifytok", "unsubscribe", "verifytok", "c"),
            None
        );
        // An unconfigured token must never accept an empty presented token.
        assert_eq!(verify_handshake("", "subscribe", "", "c"), None);
    }

    #[test]
    fn parses_a_real_text_message() {
        let payload = serde_json::json!({
            "object": "whatsapp_business_account",
            "entry": [{
                "changes": [{
                    "value": {
                        "metadata": { "phone_number_id": "109999888" },
                        "messages": [{
                            "from": "15551234567",
                            "id": "wamid.ABC",
                            "type": "text",
                            "text": { "body": "hi board" }
                        }]
                    }
                }]
            }]
        });
        assert_eq!(
            parse_messages(&payload),
            vec![WhatsAppMessage {
                phone_number_id: "109999888".into(),
                from: "15551234567".into(),
                id: "wamid.ABC".into(),
                text: "hi board".into(),
            }]
        );
    }

    #[test]
    fn ignores_statuses_and_non_text_messages() {
        let statuses = serde_json::json!({
            "entry": [{ "changes": [{ "value": {
                "metadata": { "phone_number_id": "1" },
                "statuses": [{ "status": "delivered" }]
            }}]}]
        });
        assert!(parse_messages(&statuses).is_empty());

        let image = serde_json::json!({
            "entry": [{ "changes": [{ "value": {
                "metadata": { "phone_number_id": "1" },
                "messages": [{ "from": "x", "id": "y", "type": "image", "image": {} }]
            }}]}]
        });
        assert!(parse_messages(&image).is_empty());
    }

    #[test]
    fn parses_multiple_batched_messages() {
        let payload = serde_json::json!({
            "entry": [{ "changes": [{ "value": {
                "metadata": { "phone_number_id": "1" },
                "messages": [
                    { "from": "a", "id": "m1", "type": "text", "text": { "body": "one" } },
                    { "from": "b", "id": "m2", "type": "text", "text": { "body": "two" } }
                ]
            }}]}]
        });
        let msgs = parse_messages(&payload);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].text, "one");
        assert_eq!(msgs[1].id, "m2");
    }

    #[test]
    fn empty_or_malformed_payload_yields_no_messages() {
        assert!(parse_messages(&serde_json::json!({})).is_empty());
        assert!(parse_messages(&serde_json::json!({ "entry": "nope" })).is_empty());
    }

    #[test]
    fn number_map_parses_and_skips_malformed_pairs() {
        let m = parse_number_map("109999888:general, 55511122:agents.dev,bad,:x,y:");
        assert_eq!(m.len(), 2);
        assert_eq!(m.get("109999888"), Some(&"general".to_string()));
        assert_eq!(m.get("55511122"), Some(&"agents.dev".to_string()));
    }
}
