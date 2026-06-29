//! An extension trait that provides the ability to check if a string is a channel name.

/// An extension trait giving strings a function to check if they are a channel.
pub trait ChannelExt {
    /// Returns true if the specified name is a channel name.
    fn is_channel_name(&self) -> bool;
}

impl ChannelExt for &str {
    fn is_channel_name(&self) -> bool {
        self.starts_with('#')
            || self.starts_with('&')
            || self.starts_with('+')
            || self.starts_with('!')
    }
}

impl ChannelExt for String {
    fn is_channel_name(&self) -> bool {
        (&self[..]).is_channel_name()
    }
}
