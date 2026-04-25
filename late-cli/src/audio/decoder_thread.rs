use ringbuf::{HeapProd, traits::Producer};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use super::{
    AudioSpec, StreamingLinearResampler, SymphoniaStreamDecoder, trim_stream_suffix,
};

pub(super) type PlaybackProducer = HeapProd<f32>;

pub(super) fn spawn_decoder_thread(
    audio_base_url: String,
    mut prod: PlaybackProducer,
    source_spec: AudioSpec,
    output_sample_rate: u32,
    stop: Arc<AtomicBool>,
    initial_decoder: SymphoniaStreamDecoder,
) {
    thread::spawn(move || {
        let mut decoder_opt = Some(initial_decoder);

        let mut chunk = Vec::with_capacity(1024 * source_spec.channels);
        let mut resampler = StreamingLinearResampler::new(
            source_spec.channels,
            source_spec.sample_rate,
            output_sample_rate,
        );
        let mut retries = 0;
        const MAX_RETRIES: usize = 10;

        while !stop.load(Ordering::Relaxed) {
            chunk.clear();

            if let Some(decoder) = &mut decoder_opt {
                for _ in 0..(1024 * source_spec.channels) {
                    match decoder.next() {
                        Some(sample) => chunk.push(sample),
                        None => {
                            decoder_opt = None;
                            break;
                        }
                    }
                }
            }

            if chunk.is_empty() {
                if decoder_opt.is_none() {
                    retries += 1;
                    if retries > MAX_RETRIES {
                        tracing::error!(
                            "audio stream failed {} times consecutively; giving up",
                            MAX_RETRIES
                        );
                        break;
                    }
                    tracing::warn!(
                        attempt = retries,
                        "audio stream ended or errored, reconnecting in 2s..."
                    );
                    thread::sleep(Duration::from_secs(2));

                    match SymphoniaStreamDecoder::new_http(&trim_stream_suffix(&audio_base_url)) {
                        Ok(new_decoder) => {
                            tracing::info!("audio stream reconnected");
                            decoder_opt = Some(new_decoder);
                            retries = 0;
                        }
                        Err(err) => {
                            tracing::error!(error = ?err, "failed to reconnect audio stream");
                        }
                    }
                } else {
                    thread::sleep(Duration::from_millis(10));
                }
                continue;
            }

            let chunk = resampler.process(&chunk);
            if chunk.is_empty() {
                continue;
            }

            let mut pending = chunk.as_slice();
            while !pending.is_empty() {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let pushed = prod.push_slice(pending);
                pending = &pending[pushed..];
                if !pending.is_empty() {
                    thread::sleep(Duration::from_millis(5));
                }
            }
        }
    });
}
