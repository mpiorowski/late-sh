//! PII scrubbing for egress.
//!
//! AgentBBS is anonymous by construction, but board descriptions and ad-hoc
//! metadata are free-form and could leak an email, IP, hostname, token, or key
//! that a careless operator pasted in. Before anything is sealed for egress we
//! recursively redact object keys that *look* sensitive, replacing their values
//! with a `"[redacted]"` marker. This is deliberately conservative
//! (key-name-based, case-insensitive, substring) — it never widens the data,
//! only narrows it.

use serde_json::Value;

/// The marker substituted for a redacted value.
pub const REDACTED: &str = "[redacted]";

/// Substrings (lowercased) that mark a key as PII-bearing.
const SENSITIVE: &[&str] = &[
    "email", "ip", "host", "token", "secret", "key", "phone",
];

fn is_sensitive(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE.iter().any(|needle| lower.contains(needle))
}

/// Recursively redact PII-bearing keys in `value` in place.
///
/// For objects, any key whose name contains a sensitive substring has its
/// value replaced with [`REDACTED`]; other values are recursed into. Arrays
/// are recursed element-wise. Scalars are left untouched (we redact by *key*,
/// never by guessing at scalar content).
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

/// Convenience: clone `value`, scrub it, and return the scrubbed copy.
pub fn scrubbed(value: &Value) -> Value {
    let mut v = value.clone();
    strip_pii(&mut v);
    v
}
