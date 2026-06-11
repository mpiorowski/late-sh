use anyhow::Result;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use ringbuf::traits::{Observer, Producer};

use super::{AudioSpec, PlaybackQueue, StreamingLinearResampler, SymphoniaStreamDecoder};

const STARTUP_DECODER_RETRIES: usize = 3;
const STARTUP_DECODER_RETRY_DELAY: Duration = Duration::from_millis(750);

pub(super) fn spawn_decoder_thread(
    stream_url: Arc<Mutex<String>>,
    stream_generation: Arc<AtomicU64>,
    stream_flushed_generation: Arc<AtomicU64>,
    source_is_icecast: Arc<AtomicBool>,
    native_source_selected: Arc<AtomicBool>,
    mut queue: PlaybackQueue,
    source_spec: AudioSpec,
    output_sample_rate: u32,
    stop: Arc<AtomicBool>,
    ready_tx: mpsc::SyncSender<Result<()>>,
    prebuffer_samples: usize,
) {
    thread::spawn(move || {
        let mut current_stream_url = desired_stream_url(&stream_url);
        let mut current_generation = stream_generation.load(Ordering::Relaxed);
        let mut decoder_opt = match create_startup_decoder(&current_stream_url) {
            Ok(decoder) => Some(decoder),
            Err(err) => {
                let _ = ready_tx.send(Err(err));
                return;
            }
        };

        let mut ready_tx = Some(ready_tx);
        if prebuffer_samples == 0
            && let Some(ready_tx) = ready_tx.take()
        {
            let _ = ready_tx.send(Ok(()));
        }

        let mut chunk = Vec::with_capacity(1024 * source_spec.channels);
        let mut resampler = StreamingLinearResampler::new(
            source_spec.channels,
            source_spec.sample_rate,
            output_sample_rate,
        );
        let mut retries = 0;
        const MAX_RETRIES: usize = 10;

        while !stop.load(Ordering::Relaxed) {
            let desired = desired_stream_url(&stream_url);
            if desired != current_stream_url {
                let desired_generation = stream_generation.load(Ordering::Relaxed);
                tracing::info!(
                    from = %current_stream_url,
                    to = %desired,
                    "audio stream source changed"
                );
                current_stream_url = desired;
                // The URL is committed either way; adopting the generation on
                // failure too lets the reconnect path below re-enable output
                // once it gets the new stream up.
                current_generation = desired_generation;
                decoder_opt = match create_startup_decoder(&current_stream_url) {
                    Ok(decoder) => {
                        wait_for_output_flush(
                            desired_generation,
                            &stream_flushed_generation,
                            &stop,
                        );
                        // Gate on the user's intent: they may have moved to
                        // YouTube while the switch was in flight.
                        if !stop.load(Ordering::Relaxed)
                            && native_source_selected.load(Ordering::Relaxed)
                        {
                            source_is_icecast.store(true, Ordering::Relaxed);
                        }
                        Some(decoder)
                    }
                    Err(err) => {
                        tracing::error!(error = ?err, "failed to switch audio stream");
                        None
                    }
                };
                retries = 0;
            }

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

                    match SymphoniaStreamDecoder::new_http(&current_stream_url) {
                        Ok(new_decoder) => {
                            tracing::info!("audio stream reconnected");
                            decoder_opt = Some(new_decoder);
                            if native_source_selected.load(Ordering::Relaxed)
                                && stream_generation.load(Ordering::SeqCst) == current_generation
                                && stream_flushed_generation.load(Ordering::SeqCst)
                                    >= current_generation
                            {
                                source_is_icecast.store(true, Ordering::Relaxed);
                            }
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

            loop {
                if stop.load(Ordering::Relaxed) {
                    return;
                }

                if queue.vacant_len() >= chunk.len() {
                    let pushed = queue.push_slice(&chunk);
                    if pushed == chunk.len() {
                        if ready_tx.is_some()
                            && queue.occupied_len() >= prebuffer_samples
                            && let Some(ready_tx) = ready_tx.take()
                        {
                            let _ = ready_tx.send(Ok(()));
                        }
                        break;
                    }
                    tracing::warn!(
                        pushed,
                        requested = chunk.len(),
                        "audio queue accepted a partial chunk"
                    );
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
    });
}

fn create_startup_decoder(audio_base_url: &str) -> Result<SymphoniaStreamDecoder> {
    let mut attempt = 0;
    loop {
        match SymphoniaStreamDecoder::new_http(audio_base_url) {
            Ok(decoder) => return Ok(decoder),
            Err(err) if attempt < STARTUP_DECODER_RETRIES => {
                attempt += 1;
                tracing::warn!(
                    error = ?err,
                    attempt,
                    max_retries = STARTUP_DECODER_RETRIES,
                    "failed to create audio decoder during startup; retrying"
                );
                thread::sleep(STARTUP_DECODER_RETRY_DELAY);
            }
            Err(err) => return Err(err.context("failed to create audio decoder")),
        }
    }
}

fn desired_stream_url(stream_url: &Mutex<String>) -> String {
    stream_url
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn wait_for_output_flush(
    generation: u64,
    stream_flushed_generation: &AtomicU64,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::Relaxed)
        && stream_flushed_generation.load(Ordering::SeqCst) < generation
    {
        thread::sleep(Duration::from_millis(5));
    }
}
