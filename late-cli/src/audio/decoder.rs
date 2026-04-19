use anyhow::{Context, Result};
use std::io::{self, Cursor, Read};
use symphonia::core::{
    audio::{AudioBufferRef, SampleBuffer},
    codecs::{Decoder, DecoderOptions},
    formats::{FormatOptions, FormatReader},
    io::{MediaSourceStream, ReadOnlySource},
    meta::MetadataOptions,
    probe::Hint,
};
use symphonia::default::{get_codecs, get_probe};

use super::AudioSpec;

pub(super) struct SymphoniaStreamDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    sample_buf: Vec<f32>,
    sample_pos: usize,
    spec: AudioSpec,
}

struct PrefixThenRead<R> {
    prefix: Cursor<Vec<u8>>,
    inner: R,
}

impl<R> PrefixThenRead<R> {
    fn new(prefix: Vec<u8>, inner: R) -> Self {
        Self {
            prefix: Cursor::new(prefix),
            inner,
        }
    }
}

impl<R: Read> Read for PrefixThenRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.prefix.read(buf)?;
        if n > 0 {
            return Ok(n);
        }
        self.inner.read(buf)
    }
}

impl SymphoniaStreamDecoder {
    pub(super) fn new_http(url: &str) -> Result<Self> {
        let stream_url = url.to_string() + "/stream";
        let mut resp = reqwest::blocking::get(&stream_url)
            .context("http get")?
            .error_for_status()
            .with_context(|| format!("stream request failed for {stream_url}"))?;
        let prefix = read_until_mp3_sync(&mut resp)
            .with_context(|| format!("failed to align MP3 stream from {stream_url}"))?;
        let source = ReadOnlySource::new(PrefixThenRead::new(prefix, resp));

        let mss = MediaSourceStream::new(Box::new(source), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("mp3");

        let probed = get_probe().format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;

        let format = probed.format;
        let (track_id, spec, decoder) = {
            let track = format.default_track().context("no default track")?;
            let sample_rate = track.codec_params.sample_rate.context("no sample rate")?;
            let channels = track
                .codec_params
                .channels
                .context("no channel layout")?
                .count();
            let decoder = get_codecs().make(&track.codec_params, &DecoderOptions::default())?;
            (
                track.id,
                AudioSpec {
                    sample_rate,
                    channels,
                },
                decoder,
            )
        };

        Ok(Self {
            format,
            decoder,
            track_id,
            sample_buf: Vec::new(),
            sample_pos: 0,
            spec,
        })
    }

    fn refill(&mut self) -> Result<bool> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(symphonia::core::errors::Error::IoError(_)) => return Ok(false),
                Err(err) => return Err(err.into()),
            };
            if packet.track_id() != self.track_id {
                continue;
            }

            let decoded = self.decoder.decode(&packet)?;
            self.sample_buf.clear();
            self.sample_pos = 0;
            push_interleaved_samples(&mut self.sample_buf, decoded)?;
            return Ok(true);
        }
    }

    fn spec(&self) -> AudioSpec {
        self.spec
    }
}

impl Iterator for SymphoniaStreamDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.sample_pos >= self.sample_buf.len() {
            match self.refill() {
                Ok(true) => {}
                Ok(false) => return None,
                Err(err) => {
                    tracing::warn!(error = ?err, "decoder refill error, treating as eof");
                    return None;
                }
            }
        }

        let sample = self.sample_buf.get(self.sample_pos).copied();
        self.sample_pos += 1;
        sample
    }
}

fn read_until_mp3_sync<R: Read>(reader: &mut R) -> Result<Vec<u8>> {
    const MAX_SCAN_BYTES: usize = 64 * 1024;
    const CHUNK_SIZE: usize = 4096;

    let mut buf = Vec::with_capacity(CHUNK_SIZE * 2);
    let mut chunk = [0u8; CHUNK_SIZE];

    while buf.len() < MAX_SCAN_BYTES {
        let read = reader
            .read(&mut chunk)
            .context("failed to read from audio stream")?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);

        if let Some(offset) = find_mp3_sync_offset(&buf) {
            return Ok(buf.split_off(offset));
        }
    }

    anyhow::bail!("could not find MP3 frame sync in first {} bytes", buf.len())
}

fn find_mp3_sync_offset(bytes: &[u8]) -> Option<usize> {
    if bytes.starts_with(b"ID3") {
        return Some(0);
    }

    for i in 0..=bytes.len().saturating_sub(3) {
        let b0 = bytes[i];
        let b1 = bytes[i + 1];
        let b2 = bytes[i + 2];

        if b0 != 0xFF || (b1 & 0xE0) != 0xE0 {
            continue;
        }

        let version = (b1 >> 3) & 0x03;
        let layer = (b1 >> 1) & 0x03;
        let bitrate_idx = (b2 >> 4) & 0x0F;
        let sample_rate_idx = (b2 >> 2) & 0x03;

        if version == 0x01 || layer == 0x00 {
            continue;
        }
        if bitrate_idx == 0x00 || bitrate_idx == 0x0F {
            continue;
        }
        if sample_rate_idx == 0x03 {
            continue;
        }

        return Some(i);
    }

    None
}

fn push_interleaved_samples(out: &mut Vec<f32>, decoded: AudioBufferRef<'_>) -> Result<()> {
    let spec = *decoded.spec();
    let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
    buf.copy_interleaved_ref(decoded);
    out.extend_from_slice(buf.samples());
    Ok(())
}

pub(super) fn probe_stream_spec(audio_base_url: &str) -> Result<AudioSpec> {
    let decoder = SymphoniaStreamDecoder::new_http(&trim_stream_suffix(audio_base_url))
        .context("failed to create audio decoder for stream probe")?;
    Ok(decoder.spec())
}

pub(super) fn trim_stream_suffix(audio_base_url: &str) -> String {
    audio_base_url
        .trim_end_matches('/')
        .trim_end_matches("/stream")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_stream_suffix_normalizes_base_url() {
        assert_eq!(
            trim_stream_suffix("http://audio.late.sh/stream"),
            "http://audio.late.sh"
        );
        assert_eq!(
            trim_stream_suffix("http://audio.late.sh/"),
            "http://audio.late.sh"
        );
    }

    #[test]
    fn find_mp3_sync_offset_finds_frame_after_garbage() {
        let mut bytes = vec![0x12, 0x34, 0x56, 0x78];
        bytes.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x64, 0x00, 0x00]);
        assert_eq!(find_mp3_sync_offset(&bytes), Some(4));
    }

    #[test]
    fn find_mp3_sync_offset_accepts_id3_header() {
        assert_eq!(find_mp3_sync_offset(b"ID3\x04\x00\x00"), Some(0));
    }

    #[test]
    fn find_mp3_sync_offset_checks_last_possible_offset() {
        let bytes = [0x00, 0xFF, 0xFB, 0x90];
        assert_eq!(find_mp3_sync_offset(&bytes), Some(1));
    }
}
