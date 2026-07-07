//! IRC protocol errors using `failure`.

use thiserror::Error;

/// A `Result` type for IRC `ProtocolErrors`.
pub type Result<T, E = ProtocolError> = ::std::result::Result<T, E>;

/// An IRC protocol error.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// An internal I/O error.
    #[error("an io error occurred")]
    Io(#[source] std::io::Error),

    /// Error for invalid messages.
    #[error("invalid message: {}", string)]
    InvalidMessage {
        /// The string that failed to parse.
        string: String,
        /// The detailed message parsing error.
        #[source]
        cause: MessageParseError,
    },
}

impl From<std::io::Error> for ProtocolError {
    fn from(e: std::io::Error) -> ProtocolError {
        ProtocolError::Io(e)
    }
}

/// Errors that occur when parsing messages.
#[derive(Debug, Error)]
pub enum MessageParseError {
    /// The message was empty.
    #[error("empty message")]
    EmptyMessage,

    /// The command was invalid (i.e. missing).
    #[error("invalid command")]
    InvalidCommand,

    /// The mode string was malformed.
    #[error("invalid mode string: {}", string)]
    InvalidModeString {
        /// The invalid mode string.
        string: String,
        /// The detailed mode parsing error.
        #[source]
        cause: ModeParseError,
    },

    /// The subcommand used was invalid.
    #[error("invalid {} subcommand: {}", cmd, sub)]
    InvalidSubcommand {
        /// The command whose invalid subcommand was referenced.
        cmd: &'static str,
        /// The invalid subcommand.
        sub: String,
    },
}

/// Errors that occur while parsing mode strings.
#[derive(Debug, Error)]
pub enum ModeParseError {
    /// Invalid modifier used in a mode string (only + and - are valid).
    #[error("invalid mode modifier: {}", modifier)]
    InvalidModeModifier {
        /// The invalid mode modifier.
        modifier: char,
    },

    /// Missing modifier used in a mode string.
    #[error("missing mode modifier")]
    MissingModeModifier,
}
