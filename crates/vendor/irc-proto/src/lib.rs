//! Support for the IRC protocol using Tokio.

#![warn(missing_docs)]

pub mod caps;
pub mod chan;
pub mod colors;
pub mod command;
pub mod error;
#[cfg(feature = "tokio")]
pub mod irc;
#[cfg(feature = "tokio")]
pub mod line;
pub mod message;
pub mod mode;
pub mod prefix;
pub mod response;

pub use self::caps::{Capability, NegotiationVersion};
pub use self::chan::ChannelExt;
pub use self::colors::FormattedStringExt;
pub use self::command::{BatchSubCommand, CapSubCommand, Command};
#[cfg(feature = "tokio")]
pub use self::irc::IrcCodec;
pub use self::message::Message;
pub use self::mode::{ChannelMode, Mode, UserMode};
pub use self::prefix::Prefix;
pub use self::response::Response;
