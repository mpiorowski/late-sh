//! Implementation of IRC codec for Tokio.
use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder};

use crate::error;
use crate::line::LineCodec;
use crate::message::Message;

/// An IRC codec built around an inner codec.
pub struct IrcCodec {
    inner: LineCodec,
}

impl IrcCodec {
    /// Creates a new instance of IrcCodec wrapping a LineCodec with the specific encoding.
    pub fn new(label: &str) -> error::Result<IrcCodec> {
        LineCodec::new(label).map(|codec| IrcCodec { inner: codec })
    }

    /// Sanitizes the input string by cutting up to (and including) the first occurence of a line
    /// terminiating phrase (`\r\n`, `\r`, or `\n`). This is used in sending messages through the
    /// codec to prevent the injection of additional commands.
    pub fn sanitize(mut data: String) -> String {
        // n.b. ordering matters here to prefer "\r\n" over "\r"
        if let Some((pos, len)) = ["\r\n", "\r", "\n"]
            .iter()
            .flat_map(|needle| data.find(needle).map(|pos| (pos, needle.len())))
            .min_by_key(|&(pos, _)| pos)
        {
            data.truncate(pos + len);
        }
        data
    }
}

impl Decoder for IrcCodec {
    type Item = Message;
    type Error = error::ProtocolError;

    fn decode(&mut self, src: &mut BytesMut) -> error::Result<Option<Message>> {
        self.inner
            .decode(src)
            .and_then(|res| res.map_or(Ok(None), |msg| msg.parse::<Message>().map(Some)))
    }
}

impl Encoder<Message> for IrcCodec {
    type Error = error::ProtocolError;

    fn encode(&mut self, msg: Message, dst: &mut BytesMut) -> error::Result<()> {
        self.inner.encode(IrcCodec::sanitize(msg.to_string()), dst)
    }
}
