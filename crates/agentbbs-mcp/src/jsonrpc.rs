//! Minimal JSON-RPC 2.0 types for the MCP bridge.
//!
//! MCP speaks JSON-RPC 2.0 over a byte stream (here, newline-delimited over
//! stdio). We implement just the subset we need rather than pulling in a heavy
//! SDK: a [`Request`], a [`Response`], and an [`RpcError`], plus a few helpers
//! to build well-formed responses.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The JSON-RPC protocol version string we speak.
pub const JSONRPC_VERSION: &str = "2.0";

/// Standard JSON-RPC error codes (a useful subset).
pub mod codes {
    /// Invalid JSON was received by the server.
    pub const PARSE_ERROR: i64 = -32700;
    /// The JSON sent is not a valid Request object.
    pub const INVALID_REQUEST: i64 = -32600;
    /// The method does not exist / is not available.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// Invalid method parameter(s).
    pub const INVALID_PARAMS: i64 = -32602;
    /// Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i64 = -32603;
    /// Application-level error (capability denied, domain error, …).
    pub const APPLICATION_ERROR: i64 = -32000;
}

/// A JSON-RPC 2.0 request (or notification, when `id` is absent).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    /// Always `"2.0"`.
    #[serde(default = "default_version")]
    pub jsonrpc: String,
    /// Correlation id. Absent for notifications.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    /// The method name, e.g. `"tools/call"`.
    pub method: String,
    /// Method parameters; defaults to `null` when omitted.
    #[serde(default)]
    pub params: Value,
}

impl Request {
    /// Build a request with the given method and params.
    pub fn new(id: impl Into<Value>, method: impl Into<String>, params: Value) -> Self {
        Request {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Some(id.into()),
            method: method.into(),
            params,
        }
    }

    /// Whether this is a notification (no id, no response expected).
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

fn default_version() -> String {
    JSONRPC_VERSION.to_string()
}

/// A JSON-RPC 2.0 error object.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcError {
    /// Numeric error code.
    pub code: i64,
    /// Human-readable message.
    pub message: String,
    /// Optional structured detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    /// Build an error with no extra data.
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        RpcError {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Build an error with attached structured data.
    pub fn with_data(code: i64, message: impl Into<String>, data: Value) -> Self {
        RpcError {
            code,
            message: message.into(),
            data: Some(data),
        }
    }
}

/// A JSON-RPC 2.0 response. Exactly one of `result` / `error` is set.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Echoes the request id (null if it could not be determined).
    pub id: Value,
    /// Present on success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Present on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    /// A success response carrying `result`.
    pub fn ok(id: Value, result: Value) -> Self {
        Response {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// A failure response carrying `error`.
    pub fn err(id: Value, error: RpcError) -> Self {
        Response {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_deserializes_with_defaults() {
        let r: Request =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).unwrap();
        assert_eq!(r.method, "ping");
        assert_eq!(r.params, Value::Null);
        assert!(!r.is_notification());
    }

    #[test]
    fn notification_has_no_id() {
        let r: Request = serde_json::from_str(r#"{"jsonrpc":"2.0","method":"note"}"#).unwrap();
        assert!(r.is_notification());
    }

    #[test]
    fn responses_serialize_one_of() {
        let ok = Response::ok(json!(1), json!({"v":2}));
        let s = serde_json::to_string(&ok).unwrap();
        assert!(s.contains("\"result\""));
        assert!(!s.contains("\"error\""));

        let er = Response::err(json!(1), RpcError::new(codes::METHOD_NOT_FOUND, "nope"));
        let s = serde_json::to_string(&er).unwrap();
        assert!(s.contains("\"error\""));
        assert!(!s.contains("\"result\""));
    }
}
