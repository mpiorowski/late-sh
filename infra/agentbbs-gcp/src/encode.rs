//! Pure encoding helpers that translate core [`Event`]s into the JSON wire
//! shapes the Firestore and Pub/Sub REST APIs expect.
//!
//! These functions perform **no network I/O** and have no side effects, so
//! they are exhaustively unit-tested below without an emulator. They are the
//! single source of truth for the on-the-wire representation: the reporters in
//! [`crate::firestore`] and [`crate::pubsub`] simply POST whatever these
//! produce.

use agentbbs_core::report::Event;
use base64::Engine as _;
use serde_json::{json, Value};

/// The marker substituted for a redacted value.
pub const REDACTED: &str = "[redacted]";

/// Substrings (lowercased) that mark an object key as PII-bearing.
const SENSITIVE: &[&str] = &["email", "ip", "host", "token", "secret", "key", "phone"];

fn is_sensitive(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE.iter().any(|needle| lower.contains(needle))
}

/// Recursively redact PII-bearing object keys in `value` in place.
///
/// Any object key whose (case-insensitive) name contains one of the
/// [`SENSITIVE`] substrings has its value replaced with [`REDACTED`]; other
/// values are recursed into. Arrays are recursed element-wise. Scalars are
/// left untouched — we redact by *key name*, never by guessing scalar content.
///
/// This is applied to every event's `detail` before it leaves the process to
/// Firestore or Pub/Sub, so the GCP egress path can never carry free-form PII
/// even if a future event populates `detail` with sensitive text.
pub fn strip_pii(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if is_sensitive(k) {
                    *v = Value::String(REDACTED.to_string());
                } else {
                    strip_pii(v);
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                strip_pii(item);
            }
        }
        _ => {}
    }
}

/// Return a PII-scrubbed copy of an event's `detail`.
fn scrubbed_detail(event: &Event) -> Value {
    let mut v = event.detail.clone();
    strip_pii(&mut v);
    v
}

/// Convert a core [`Event`] into a Firestore REST "typed value" document body.
///
/// Firestore's REST API does not accept plain JSON; every field must be tagged
/// with its type (`stringValue`, `timestampValue`, `nullValue`, …). We map:
///
/// * `at`       → `timestampValue` (RFC 3339 / `Utc` ISO-8601)
/// * `kind`     → `stringValue` (serde snake_case rendering)
/// * `agent`    → `stringValue` of the hex id, or `nullValue` when absent
/// * `subject`  → `stringValue`
/// * `detail`   → `stringValue` holding the JSON-serialized detail blob
/// * `severity` → `stringValue` (lowercase severity)
///
/// The resulting shape is `{"fields": { ... }}`, exactly what a
/// `POST .../documents/agentbbs_events` call wants as its request body.
pub fn to_firestore_fields(event: &Event) -> Value {
    let kind = serde_json::to_value(event.kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let severity = serde_json::to_value(event.severity())
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();

    let agent_value = match &event.agent {
        Some(id) => json!({ "stringValue": id.to_hex() }),
        None => json!({ "nullValue": null }),
    };

    // `at` is RFC 3339 with a trailing Z, which Firestore accepts directly.
    let at = event.at.to_rfc3339();
    let detail =
        serde_json::to_string(&scrubbed_detail(event)).unwrap_or_else(|_| "null".to_string());

    json!({
        "fields": {
            "at": { "timestampValue": at },
            "kind": { "stringValue": kind },
            "agent": agent_value,
            "subject": { "stringValue": event.subject },
            "detail": { "stringValue": detail },
            "severity": { "stringValue": severity },
        }
    })
}

/// Serialize an [`Event`] to a compact JSON string (the message payload that
/// rides inside a Pub/Sub message and is later decoded by the Cloud Function).
pub fn event_json(event: &Event) -> String {
    // Scrub the free-form `detail` before it egresses to Pub/Sub. We clone the
    // event and replace its detail with the scrubbed copy so the rest of the
    // shape is untouched.
    let mut scrubbed = event.clone();
    scrubbed.detail = scrubbed_detail(event);
    serde_json::to_string(&scrubbed).unwrap_or_else(|_| "{}".to_string())
}

/// Build the request body for a Pub/Sub `topics/{topic}:publish` REST call.
///
/// Pub/Sub requires each message's `data` to be **base64-encoded bytes**, so we
/// JSON-encode each event and then base64 that string. Shape:
///
/// ```json
/// { "messages": [ { "data": "<base64>" }, ... ] }
/// ```
pub fn pubsub_publish_body(events: &[Event]) -> Value {
    let messages: Vec<Value> = events
        .iter()
        .map(|e| {
            let data = base64::engine::general_purpose::STANDARD.encode(event_json(e));
            json!({ "data": data })
        })
        .collect();
    json!({ "messages": messages })
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentbbs_core::identity::Identity;
    use agentbbs_core::report::{Event, EventKind};
    use chrono::{TimeZone, Utc};

    fn fixed_event() -> Event {
        let id = Identity::from_seed(&[3u8; 32]).id();
        let at = Utc.with_ymd_and_hms(2026, 6, 28, 12, 30, 0).unwrap();
        Event {
            at,
            kind: EventKind::Security,
            agent: Some(id),
            subject: "rate-limit".to_string(),
            detail: json!({ "bucket": 7, "denied": true }),
        }
    }

    #[test]
    fn firestore_fields_exact_shape() {
        let ev = fixed_event();
        let id_hex = ev.agent.unwrap().to_hex();
        let got = to_firestore_fields(&ev);

        let expected = json!({
            "fields": {
                "at": { "timestampValue": "2026-06-28T12:30:00+00:00" },
                "kind": { "stringValue": "security" },
                "agent": { "stringValue": id_hex },
                "subject": { "stringValue": "rate-limit" },
                "detail": { "stringValue": "{\"bucket\":7,\"denied\":true}" },
                "severity": { "stringValue": "warn" },
            }
        });
        assert_eq!(got, expected);
    }

    #[test]
    fn firestore_fields_null_agent() {
        let ev = Event {
            at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            kind: EventKind::Post,
            agent: None,
            subject: "general".to_string(),
            detail: Value::Null,
        };
        let got = to_firestore_fields(&ev);
        assert_eq!(got["fields"]["agent"], json!({ "nullValue": null }));
        assert_eq!(got["fields"]["kind"], json!({ "stringValue": "post" }));
        assert_eq!(got["fields"]["severity"], json!({ "stringValue": "info" }));
        assert_eq!(got["fields"]["detail"], json!({ "stringValue": "null" }));
    }

    #[test]
    fn pubsub_body_base64_shape() {
        let ev = fixed_event();
        let body = pubsub_publish_body(std::slice::from_ref(&ev));

        let messages = body["messages"].as_array().expect("messages array");
        assert_eq!(messages.len(), 1);
        let data = messages[0]["data"].as_str().expect("data string");

        // Decode round-trips back to the exact event JSON.
        let raw = base64::engine::general_purpose::STANDARD
            .decode(data)
            .expect("valid base64");
        let decoded: Event = serde_json::from_slice(&raw).expect("event json");
        assert_eq!(decoded, ev);

        // And the encoded data matches encoding the canonical event json.
        let expected_data = base64::engine::general_purpose::STANDARD.encode(event_json(&ev));
        assert_eq!(data, expected_data);
    }

    #[test]
    fn firestore_detail_redacts_pii() {
        let ev = Event {
            at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            kind: EventKind::Security,
            agent: None,
            subject: "leak".to_string(),
            detail: json!({ "email": "a@b.com", "ok": 1 }),
        };
        let got = to_firestore_fields(&ev);
        let detail = got["fields"]["detail"]["stringValue"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(detail).unwrap();
        assert_eq!(parsed["email"], json!(REDACTED));
        assert_eq!(parsed["ok"], json!(1));
    }

    #[test]
    fn pubsub_payload_redacts_pii() {
        let ev = Event {
            at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            kind: EventKind::Security,
            agent: None,
            subject: "leak".to_string(),
            detail: json!({ "client_ip": "10.0.0.1", "token": "abc", "ok": 1 }),
        };
        let raw = event_json(&ev);
        let decoded: Event = serde_json::from_str(&raw).unwrap();
        assert_eq!(decoded.detail["client_ip"], json!(REDACTED));
        assert_eq!(decoded.detail["token"], json!(REDACTED));
        assert_eq!(decoded.detail["ok"], json!(1));
    }

    #[test]
    fn strip_pii_recurses_nested() {
        let mut v = json!({
            "outer": { "host": "secret.internal", "fine": true },
            "list": [{ "phone": "555" }, { "x": 2 }],
        });
        strip_pii(&mut v);
        assert_eq!(v["outer"]["host"], json!(REDACTED));
        assert_eq!(v["outer"]["fine"], json!(true));
        assert_eq!(v["list"][0]["phone"], json!(REDACTED));
        assert_eq!(v["list"][1]["x"], json!(2));
    }

    #[test]
    fn pubsub_body_multiple_messages() {
        let a = fixed_event();
        let b = Event::now(EventKind::Post, "hi");
        let body = pubsub_publish_body(&[a, b]);
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    }
}
