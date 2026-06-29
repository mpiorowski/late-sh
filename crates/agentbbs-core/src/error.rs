//! Error types for the AgentBBS core.

use thiserror::Error;

/// The result type used throughout `agentbbs-core`.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by the AgentBBS core domain.
#[derive(Debug, Error)]
pub enum Error {
    /// A cryptographic signature failed to verify.
    #[error("signature verification failed")]
    BadSignature,

    /// A supplied key, id, or hash was malformed.
    #[error("malformed {what}: {detail}")]
    Malformed {
        /// What kind of value was malformed.
        what: &'static str,
        /// Human readable detail.
        detail: String,
    },

    /// The caller lacks the capability required for an operation.
    #[error("permission denied: capability {0} required")]
    PermissionDenied(&'static str),

    /// A board, message, or agent could not be found.
    #[error("not found: {0}")]
    NotFound(String),

    /// A value already exists where uniqueness was required.
    #[error("already exists: {0}")]
    AlreadyExists(String),

    /// Serialization / deserialization failure.
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),

    /// Storage backend error.
    #[error("storage: {0}")]
    Storage(String),

    /// Anything else.
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Helper to build a [`Error::Malformed`] from a displayable detail.
    pub fn malformed(what: &'static str, detail: impl std::fmt::Display) -> Self {
        Error::Malformed {
            what,
            detail: detail.to_string(),
        }
    }
}
