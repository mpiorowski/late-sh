//! Implementation of line-delimiting codec for Tokio.

use std::io;

use bytes::BytesMut;
#[cfg(feature = "encoding")]
use encoding::label::encoding_from_whatwg_label;
#[cfg(feature = "encoding")]
use encoding::{DecoderTrap, EncoderTrap, EncodingRef};
use tokio_util::codec::{Decoder, Encoder};

use crate::error;

/// A line-based codec parameterized by an encoding.
pub struct LineCodec {
    #[cfg(feature = "encoding")]
    encoding: EncodingRef,
    next_index: usize,
}

impl LineCodec {
    /// Creates a new instance of LineCodec from the specified encoding.
    pub fn new(label: &str) -> error::Result<LineCodec> {
        #[cfg(not(feature = "encoding"))]
        let _ = label;
        Ok(LineCodec {
            #[cfg(feature = "encoding")]
            encoding: match encoding_from_whatwg_label(label) {
                Some(x) => x,
                None => {
                    return Err(error::ProtocolError::Io(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        &format!("Attempted to use unknown codec {}.", label)[..],
                    )));
                }
            },
            next_index: 0,
        })
    }
}

impl Decoder for LineCodec {
    type Item = String;
    type Error = error::ProtocolError;

    fn decode(&mut self, src: &mut BytesMut) -> error::Result<Option<String>> {
        if let Some(offset) = src[self.next_index..].iter().position(|b| *b == b'\n') {
            // Remove the next frame from the buffer.
            let line = src.split_to(self.next_index + offset + 1);

            // Set the search start index back to 0 since we found a newline.
            self.next_index = 0;

            #[cfg(feature = "encoding")]
            {
                // Decode the line using the codec's encoding.
                match self.encoding.decode(line.as_ref(), DecoderTrap::Replace) {
                    Ok(data) => Ok(Some(data)),
                    Err(data) => Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        &format!("Failed to decode {} as {}.", data, self.encoding.name())[..],
                    )
                    .into()),
                }
            }

            #[cfg(not(feature = "encoding"))]
            {
                match String::from_utf8(line.to_vec()) {
                    Ok(data) => Ok(Some(data)),
                    Err(data) => Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        &format!("Failed to decode {} as UTF-8.", data)[..],
                    )
                    .into()),
                }
            }
        } else {
            // Set the search start index to the current length since we know that none of the
            // characters we've already looked at are newlines.
            self.next_index = src.len();
            Ok(None)
        }
    }
}

impl Encoder<String> for LineCodec {
    type Error = error::ProtocolError;

    fn encode(&mut self, msg: String, dst: &mut BytesMut) -> error::Result<()> {
        #[cfg(feature = "encoding")]
        {
            // Encode the message using the codec's encoding.
            let data: error::Result<Vec<u8>> = self
                .encoding
                .encode(&msg, EncoderTrap::Replace)
                .map_err(|data| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        &format!("Failed to encode {} as {}.", data, self.encoding.name())[..],
                    )
                    .into()
                });
            // Write the encoded message to the output buffer.
            dst.extend(&data?);
        }

        #[cfg(not(feature = "encoding"))]
        {
            dst.extend(msg.into_bytes());
        }

        Ok(())
    }
}
