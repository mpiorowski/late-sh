//! Implementation of line-delimiting codec for Tokio.

use std::io;

use bytes::BytesMut;
#[cfg(feature = "encoding")]
use encoding::label::encoding_from_whatwg_label;
#[cfg(feature = "encoding")]
use encoding::{DecoderTrap, EncoderTrap, EncodingRef};
use tokio_util::codec::{Decoder, Encoder};

use crate::error;

const MAX_LINE_BYTES: usize = 8192;

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
        let search_start = self.next_index.min(src.len());

        if let Some(offset) = src[search_start..].iter().position(|b| *b == b'\n') {
            let frame_len = search_start + offset + 1;
            if frame_len > MAX_LINE_BYTES {
                let _ = src.split_to(frame_len);
                self.next_index = 0;
                return Err(line_too_long_error(frame_len));
            }

            // Remove the next frame from the buffer.
            let line = src.split_to(frame_len);

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
        } else if src.len() > MAX_LINE_BYTES {
            let len = src.len();
            src.clear();
            self.next_index = 0;
            Err(line_too_long_error(len))
        } else {
            // Set the search start index to the current length since we know that none of the
            // characters we've already looked at are newlines.
            self.next_index = src.len();
            Ok(None)
        }
    }
}

fn line_too_long_error(len: usize) -> error::ProtocolError {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("IRC line exceeded {MAX_LINE_BYTES} bytes: {len} bytes"),
    )
    .into()
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

#[cfg(test)]
mod test {
    use bytes::BytesMut;
    use tokio_util::codec::Decoder;

    use super::{LineCodec, MAX_LINE_BYTES};

    #[test]
    fn decode_rejects_unterminated_overlong_line() {
        let mut codec = LineCodec::new("utf8").unwrap();
        let mut src = BytesMut::from(vec![b'a'; MAX_LINE_BYTES + 1].as_slice());

        let err = codec.decode(&mut src).unwrap_err();

        assert!(err.to_string().contains("io error"));
        assert!(src.is_empty());
    }

    #[test]
    fn decode_rejects_overlong_line_and_recovers() {
        let mut codec = LineCodec::new("utf8").unwrap();
        let mut src = BytesMut::from(vec![b'a'; MAX_LINE_BYTES].as_slice());
        src.extend_from_slice(b"\nPING ok\n");

        let err = codec.decode(&mut src).unwrap_err();
        let next = codec.decode(&mut src).unwrap();

        assert!(err.to_string().contains("io error"));
        assert_eq!(next.as_deref(), Some("PING ok\n"));
    }
}
