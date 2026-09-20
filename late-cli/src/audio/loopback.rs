//! Linux capture of the YouTube helper's audio for the TUI equalizer.
//!
//! YouTube plays inside the `late-webview` helper's WebKitGTK iframe, so its
//! samples never pass through `late`'s own output. At spawn the parent tags
//! the helper's environment, and every audio stream WebKit opens carries
//! that tag plus this `late` process as its owner. A worker finds the tagged
//! stream in `pw-dump`, records just that stream with `pw-record`, and feeds
//! the same analyzer the native output uses. Both tools ship with PipeWire;
//! without them the TUI keeps its ambient band.

use ringbuf::{
    HeapProd, HeapRb,
    traits::{Producer, Split},
};
use serde_json::Value;
use std::{
    io::{self, Read},
    process::{Child, ChildStdout, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tokio::sync::broadcast;
use tracing::{info, warn};

use super::{ANALYZER_RING_SAMPLES, VizSample, analyzer::spawn_playback_analyzer_thread};

/// Stream property naming the player; the helper's Wayland app id.
const HELPER_APP_ID: &str = "sh.late.youtube";
/// Stream property naming the `late` process that spawned the helper, so
/// two sessions on one desktop never capture each other's player.
const OWNER_PROP: &str = "late.webview.owner";
/// Rate `pw-record` resamples the capture to.
const CAPTURE_SAMPLE_RATE: u32 = 44_100;
/// Samples read from `pw-record` per chunk (~12ms).
const CAPTURE_CHUNK_SAMPLES: usize = 512;
/// How often the worker re-reads the graph: WebKit can replace its stream
/// between videos, and a new one has to be picked up.
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(1);

/// Environment for the helper process that tags every audio stream WebKit
/// opens. `PULSE_PROP` covers WebKit's PulseAudio sink (what it uses today,
/// through pipewire-pulse); `PIPEWIRE_PROPS` covers a native PipeWire sink.
pub(crate) fn helper_stream_env(owner: u32) -> [(&'static str, String); 2] {
    [
        (
            "PULSE_PROP",
            format!("application.id={HELPER_APP_ID} {OWNER_PROP}={owner}"),
        ),
        (
            "PIPEWIRE_PROPS",
            format!("{{ application.id = {HELPER_APP_ID} {OWNER_PROP} = {owner} }}"),
        ),
    ]
}

/// Captures the helper's audio while it lives. Dropping it stops discovery
/// and the recorder within one [`DISCOVERY_INTERVAL`].
pub(crate) struct HelperAudioCapture {
    stop: Arc<AtomicBool>,
}

impl HelperAudioCapture {
    pub(crate) fn start(owner: u32, analyzer_tx: broadcast::Sender<VizSample>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        thread::spawn(move || run_capture_worker(owner, &analyzer_tx, &worker_stop));
        Self { stop }
    }
}

impl Drop for HelperAudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Debug)]
enum DiscoveryError {
    ToolMissing,
    Run(io::Error),
    Exit(ExitStatus),
    Parse(serde_json::Error),
}

/// Keeps one recorder attached to the owner's newest helper stream, and
/// owns every log line the capture produces.
fn run_capture_worker(owner: u32, analyzer_tx: &broadcast::Sender<VizSample>, stop: &AtomicBool) {
    let mut recorder: Option<Recorder> = None;
    let mut discovery_failing = false;
    // A serial whose recorder keeps exiting while its stream is still listed
    // warns once, then respawns quietly until a recorder holds it.
    let mut dropping_serial: Option<u64> = None;
    while !stop.load(Ordering::Relaxed) {
        // A recorder never reconnects, so it exits when its stream goes away
        // between videos. Discovery below tells that apart from a recorder
        // that could not hold a stream that is still listed.
        let mut exited: Option<(u64, RecorderExit)> = None;
        if let Some(active) = recorder.as_mut()
            && let Some(exit) = active.exit()
        {
            exited = Some((active.serial, exit));
            recorder = None;
        }

        match discover_helper_stream(owner) {
            Ok(target) => {
                discovery_failing = false;
                let current = recorder.as_ref().map(|active| active.serial);
                match (target, current) {
                    (Some(serial), Some(current)) if serial == current => {
                        dropping_serial = None;
                    }
                    (Some(serial), _) => {
                        recorder = None;
                        let retrying = match exited {
                            Some((exited_serial, exit)) if exited_serial == serial => {
                                if dropping_serial != Some(serial) {
                                    match exit {
                                        RecorderExit::Status(status) => warn!(
                                            serial,
                                            ?status,
                                            "youtube helper audio recorder exited while its stream is still listed; retrying"
                                        ),
                                        RecorderExit::Wait(err) => warn!(
                                            serial,
                                            error = %err,
                                            "failed to check youtube helper audio recorder; retrying"
                                        ),
                                    }
                                }
                                dropping_serial = Some(serial);
                                true
                            }
                            Some(_) | None => {
                                dropping_serial = None;
                                false
                            }
                        };
                        match Recorder::spawn(serial, analyzer_tx.clone()) {
                            Ok(started) => {
                                if !retrying {
                                    info!(
                                        serial,
                                        "capturing youtube helper audio for the equalizer"
                                    );
                                }
                                recorder = Some(started);
                            }
                            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                                warn!("pw-record not found; the youtube equalizer stays ambient");
                                return;
                            }
                            Err(err) => {
                                warn!(error = %err, serial, "failed to start pw-record for youtube helper audio");
                            }
                        }
                    }
                    (None, Some(_)) => {
                        info!("youtube helper audio stream gone; capture detached");
                        recorder = None;
                    }
                    (None, None) => {}
                }
            }
            Err(DiscoveryError::ToolMissing) => {
                warn!("pw-dump not found; the youtube equalizer stays ambient");
                return;
            }
            // A failing graph read warns once per run of failures, not once
            // a second.
            Err(DiscoveryError::Run(err)) => {
                if !discovery_failing {
                    warn!(error = %err, "failed to run pw-dump for youtube helper audio");
                }
                discovery_failing = true;
            }
            Err(DiscoveryError::Exit(status)) => {
                if !discovery_failing {
                    warn!(
                        ?status,
                        "pw-dump failed while looking for youtube helper audio"
                    );
                }
                discovery_failing = true;
            }
            Err(DiscoveryError::Parse(err)) => {
                if !discovery_failing {
                    warn!(error = %err, "failed to parse pw-dump output for youtube helper audio");
                }
                discovery_failing = true;
            }
        }

        thread::sleep(DISCOVERY_INTERVAL);
    }
}

fn discover_helper_stream(owner: u32) -> Result<Option<u64>, DiscoveryError> {
    let output = match Command::new("pw-dump")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(output) => output,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err(DiscoveryError::ToolMissing);
        }
        Err(err) => return Err(DiscoveryError::Run(err)),
    };
    if !output.status.success() {
        return Err(DiscoveryError::Exit(output.status));
    }
    match find_helper_stream(&output.stdout, owner) {
        Ok(serial) => Ok(serial),
        Err(err) => Err(DiscoveryError::Parse(err)),
    }
}

struct HelperStream {
    serial: u64,
    running: bool,
}

/// The `object.serial` to record from a `pw-dump` document: an audio output
/// stream tagged as this owner's helper, a running one before an idle one,
/// then the newest.
pub(crate) fn find_helper_stream(
    dump: &[u8],
    owner: u32,
) -> Result<Option<u64>, serde_json::Error> {
    let objects: Vec<Value> = serde_json::from_slice(dump)?;
    let newest = objects
        .iter()
        .filter_map(|object| helper_stream(object, owner))
        .max_by_key(|stream| (stream.running, stream.serial));
    Ok(newest.map(|stream| stream.serial))
}

fn helper_stream(object: &Value, owner: u32) -> Option<HelperStream> {
    let info = &object["info"];
    let props = &info["props"];
    let is_helper_stream = object["type"] == "PipeWire:Interface:Node"
        && props["media.class"] == "Stream/Output/Audio"
        && props["application.id"] == HELPER_APP_ID
        && is_owner(&props[OWNER_PROP], owner);
    if !is_helper_stream {
        return None;
    }
    Some(HelperStream {
        serial: props["object.serial"].as_u64()?,
        running: info["state"] == "running",
    })
}

/// The owner tag as PipeWire reports it: a string when it came through
/// `PULSE_PROP`, a number when it came through `PIPEWIRE_PROPS`.
fn is_owner(value: &Value, owner: u32) -> bool {
    match value {
        Value::String(text) => text.parse::<u32>() == Ok(owner),
        Value::Number(number) => number.as_u64() == Some(u64::from(owner)),
        Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => false,
    }
}

/// A `pw-record` process linked to one stream, with the reader and analyzer
/// threads that live exactly as long as its output does.
struct Recorder {
    serial: u64,
    child: Child,
}

/// How a recorder ended. A failed status check counts as ended: the worker
/// drops (kills) it and starts over.
enum RecorderExit {
    Status(ExitStatus),
    Wait(io::Error),
}

impl Recorder {
    fn spawn(serial: u64, analyzer_tx: broadcast::Sender<VizSample>) -> io::Result<Self> {
        let mut child = Command::new("pw-record")
            .args([
                "--target",
                &serial.to_string(),
                "--rate",
                &CAPTURE_SAMPLE_RATE.to_string(),
                "--channels",
                "1",
                "--format",
                "f32",
                "--latency",
                "20ms",
                "--raw",
                // Never fall back to the default source (a microphone) when
                // the stream is missing or goes away.
                "-P",
                "{ node.dont-reconnect = true node.dont-fallback = true }",
                "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take().expect("pw-record stdout is piped");
        let (samples_tx, samples_rx) = HeapRb::<f32>::new(ANALYZER_RING_SAMPLES).split();
        let analyzer_stop = Arc::new(AtomicBool::new(false));
        spawn_playback_analyzer_thread(
            samples_rx,
            analyzer_tx,
            CAPTURE_SAMPLE_RATE,
            Arc::clone(&analyzer_stop),
        );
        thread::spawn(move || {
            pump_samples(stdout, samples_tx);
            analyzer_stop.store(true, Ordering::Relaxed);
        });
        Ok(Self { serial, child })
    }

    fn exit(&mut self) -> Option<RecorderExit> {
        match self.child.try_wait() {
            Ok(None) => None,
            Ok(Some(status)) => Some(RecorderExit::Status(status)),
            Err(err) => Some(RecorderExit::Wait(err)),
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Moves recorded samples into the analyzer ring until the recorder's
/// output ends. A full ring drops samples rather than stalling the reader.
fn pump_samples(mut stdout: ChildStdout, mut samples: HeapProd<f32>) {
    let mut bytes = [0u8; CAPTURE_CHUNK_SAMPLES * 4];
    let mut chunk = [0f32; CAPTURE_CHUNK_SAMPLES];
    while stdout.read_exact(&mut bytes).is_ok() {
        if decode_audible_chunk(&bytes, &mut chunk) {
            samples.push_slice(&chunk);
        }
    }
}

/// Decodes one little-endian f32 chunk and reports whether it holds any
/// sound. Exact digital silence (a paused or muted player) is not analyzed:
/// silence must send no frames, so the TUI falls back instead of drawing a
/// flat live spectrum.
pub(crate) fn decode_audible_chunk(bytes: &[u8], chunk: &mut [f32]) -> bool {
    let (raw_samples, _) = bytes.as_chunks::<4>();
    for (sample, raw) in chunk.iter_mut().zip(raw_samples) {
        *sample = f32::from_le_bytes(*raw);
    }
    chunk.iter().any(|sample| *sample != 0.0)
}

#[cfg(test)]
#[path = "loopback_test.rs"]
mod loopback_test;
