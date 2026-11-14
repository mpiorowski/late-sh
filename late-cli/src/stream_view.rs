//! A live stream's picture and sound, received by the CLI itself. The viewer
//! joins the stream's LiveKit room with a subscribe-only ticket (the same one
//! the web watch page gets), decodes the screen share with the SDK's own
//! WebRTC stack, and writes every frame to mpv's stdin as raw I420 on a fixed
//! canvas; mpv draws it in the TUI's rectangle like any News video
//! (`video.rs`). The stream's audio plays through the SDK's audio device, next
//! to the voice runtime when one is up.
//!
//! No browser and no webview: WebKitGTK ships without WebRTC on the distros
//! that matter, and the SDK is already linked for voice.

/// The largest canvas mpv is fed. A screen share is rarely sharper, and a
/// frame is `width * height * 3 / 2` bytes on the pipe.
const CANVAS_MAX_WIDTH: u32 = 1920;
const CANVAS_MAX_HEIGHT: u32 = 1080;
/// The smallest side worth scaling a frame to.
const CANVAS_MIN_SIDE: u32 = 16;
/// Video black in limited-range YUV.
const BLACK_LUMA: u8 = 16;
const BLACK_CHROMA: u8 = 128;
/// A stream publisher's LiveKit identity (the go-live console or the OBS
/// ingress): its audio is the program, everything else is a human mic.
const STREAM_PUBLISHER_PREFIX: &str = "stream-";

/// What the server hands the CLI to watch a stream.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct StreamTicket {
    pub(super) livekit_url: String,
    pub(super) token: String,
    /// Whether the room's voices play here too. False when the viewer
    /// already hears them through the voice runtime: one audio path per
    /// sound.
    pub(super) voices: bool,
}

impl std::fmt::Debug for StreamTicket {
    /// The token is a credential: it never reaches a log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamTicket")
            .field("livekit_url", &self.livekit_url)
            .field("voices", &self.voices)
            .finish_non_exhaustive()
    }
}

/// The fixed frame size mpv is told to expect on stdin. Raw video carries no
/// size of its own, and a WebRTC sender changes resolution whenever it likes,
/// so every frame is scaled onto this canvas instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Canvas {
    pub(super) width: u32,
    pub(super) height: u32,
}

/// Where a frame lands on the canvas: scaled to fit, centered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Placement {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

impl Canvas {
    /// The canvas for a rectangle of `width` by `height` pixels: the
    /// rectangle's own shape, shrunk to fit the cap, both sides even (I420
    /// halves its chroma planes). `None` for a rectangle too small to draw in.
    pub(super) fn for_pixels(width: u32, height: u32) -> Option<Self> {
        if width < CANVAS_MIN_SIDE || height < CANVAS_MIN_SIDE {
            return None;
        }
        let (width, height) = match width <= CANVAS_MAX_WIDTH && height <= CANVAS_MAX_HEIGHT {
            true => (width, height),
            false => fit_within(width, height, CANVAS_MAX_WIDTH, CANVAS_MAX_HEIGHT),
        };
        let (width, height) = (width & !1, height & !1);
        match width >= CANVAS_MIN_SIDE && height >= CANVAS_MIN_SIDE {
            true => Some(Self { width, height }),
            false => None,
        }
    }

    /// Where a `src_width` by `src_height` frame goes: as large as fits with
    /// its aspect kept, centered, size and offset even so the chroma planes
    /// line up.
    pub(super) fn fit(self, src_width: u32, src_height: u32) -> Placement {
        let (width, height) =
            fit_within(src_width.max(1), src_height.max(1), self.width, self.height);
        let (width, height) = ((width & !1).max(2), (height & !1).max(2));
        Placement {
            x: ((self.width - width) / 2) & !1,
            y: ((self.height - height) / 2) & !1,
            width,
            height,
        }
    }

    /// One black frame: the Y plane, then U, then V.
    pub(super) fn black(self) -> Vec<u8> {
        let luma = (self.width * self.height) as usize;
        let mut frame = vec![BLACK_CHROMA; luma + luma / 2];
        frame[..luma].fill(BLACK_LUMA);
        frame
    }

    /// Copy a scaled frame's three planes onto `frame` (a canvas-sized I420
    /// buffer) at `place`. Each source plane may be wider than its picture
    /// (`stride`).
    pub(super) fn blit(
        self,
        frame: &mut [u8],
        place: Placement,
        planes: (&[u8], &[u8], &[u8]),
        strides: (u32, u32, u32),
    ) {
        let luma = (self.width * self.height) as usize;
        let chroma = luma / 4;
        let (y_plane, rest) = frame.split_at_mut(luma);
        let (u_plane, v_plane) = rest.split_at_mut(chroma);
        blit_plane(y_plane, self.width, place, planes.0, strides.0);
        let half = Placement {
            x: place.x / 2,
            y: place.y / 2,
            width: place.width / 2,
            height: place.height / 2,
        };
        blit_plane(u_plane, self.width / 2, half, planes.1, strides.1);
        blit_plane(v_plane, self.width / 2, half, planes.2, strides.2);
    }
}

/// The largest size with `width:height`'s aspect that fits `max_width` by
/// `max_height`, which is larger than the input when the input is small.
fn fit_within(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let (width, height, max_width, max_height) = (
        u64::from(width),
        u64::from(height),
        u64::from(max_width),
        u64::from(max_height),
    );
    // Compare the two scale factors as cross products, so nothing rounds.
    let (fit_width, fit_height) = match max_width * height <= max_height * width {
        true => (max_width, (max_width * height / width).max(1)),
        false => ((max_height * width / height).max(1), max_height),
    };
    (fit_width as u32, fit_height as u32)
}

fn blit_plane(dst: &mut [u8], dst_stride: u32, place: Placement, src: &[u8], src_stride: u32) {
    let width = place.width as usize;
    for row in 0..place.height as usize {
        let from = row * src_stride as usize;
        let to = (place.y as usize + row) * dst_stride as usize + place.x as usize;
        dst[to..to + width].copy_from_slice(&src[from..from + width]);
    }
}

/// Whether the viewer plays a remote audio track: the stream's own audio
/// always, the room's voices only when the ticket asks for them.
pub(super) fn keep_audio(voices: bool, participant_identity: &str) -> bool {
    voices || participant_identity.starts_with(STREAM_PUBLISHER_PREFIX)
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
mod feed {
    use std::io::Write;
    use std::process::ChildStdin;
    use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
    use std::sync::{Arc, Mutex};

    use futures_util::StreamExt;
    use livekit::PlatformAudio;
    use livekit::prelude::{RemoteTrack, RemoteVideoTrack, Room, RoomEvent, RoomOptions};
    use livekit::webrtc::prelude::VideoBuffer;
    use livekit::webrtc::video_stream::native::NativeVideoStream;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
    use tracing::{info, warn};

    use super::{Canvas, StreamTicket, keep_audio};

    enum FeedCommand {
        ToggleMute,
    }

    /// The running viewer: the room connection, the frame pump, and the
    /// thread writing to mpv. Dropping it leaves the room.
    pub(in super::super) struct StreamFeed {
        commands: UnboundedSender<FeedCommand>,
        failure: Arc<Mutex<Option<String>>>,
    }

    impl StreamFeed {
        /// Join the room and start feeding `to_mpv`. Returns at once; a
        /// connection that fails shows up in [`Self::failure`]. Must be
        /// called on the tokio runtime.
        pub(in super::super) fn start(
            ticket: StreamTicket,
            canvas: Canvas,
            to_mpv: ChildStdin,
        ) -> Self {
            let (commands, command_rx) = unbounded_channel();
            let failure = Arc::new(Mutex::new(None));
            let task_failure = Arc::clone(&failure);
            tokio::spawn(async move {
                if let Err(reason) = run(ticket, canvas, to_mpv, command_rx).await {
                    warn!(error = %reason, "stream viewer stopped");
                    *task_failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(reason);
                }
            });
            Self { commands, failure }
        }

        /// `m`: silence or restore everything this viewer plays.
        pub(in super::super) fn toggle_mute(&self) {
            // A closed channel means the viewer already stopped; `failure`
            // says why.
            let _ = self.commands.send(FeedCommand::ToggleMute);
        }

        /// Why the viewer gave up, once it has.
        pub(in super::super) fn failure(&self) -> Option<String> {
            self.failure
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }
    }

    /// Runs until the feed is dropped (the command channel closes) or the
    /// room goes away. `Err` is a failure worth reporting; a dropped feed is
    /// `Ok`.
    async fn run(
        ticket: StreamTicket,
        canvas: Canvas,
        to_mpv: ChildStdin,
        mut commands: UnboundedReceiver<FeedCommand>,
    ) -> Result<(), String> {
        // Held for the whole watch: it is what routes remote audio tracks to
        // the speakers. Shared with the voice runtime's when one is up.
        let audio = match PlatformAudio::new() {
            Ok(audio) => {
                if let Some(device) = audio.playout_devices().next()
                    && let Err(err) = audio.set_playout_device(&device.id)
                {
                    warn!(device = %device.name, error = ?err, "failed to select the stream playout device");
                }
                Some(audio)
            }
            Err(err) => {
                warn!(error = ?err, "no audio device for the stream; playing the picture only");
                None
            }
        };

        let (room, mut events) =
            match Room::connect(&ticket.livekit_url, &ticket.token, RoomOptions::default()).await {
                Ok(connected) => connected,
                Err(err) => return Err(format!("connecting to the stream room: {err}")),
            };
        info!(url = %ticket.livekit_url, voices = ticket.voices, "joined the stream room as a viewer");

        let to_writer = spawn_writer(to_mpv);
        let mut muted = false;
        let mut pump: Option<tokio::task::JoinHandle<()>> = None;
        let outcome = loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(FeedCommand::ToggleMute) => {
                        muted = !muted;
                        set_audio_enabled(&room, !muted);
                    }
                    None => break Ok(()),
                },
                event = events.recv() => match event {
                    Some(RoomEvent::TrackSubscribed { track, publication, participant }) => match track {
                        RemoteTrack::Video(video) => {
                            info!(
                                participant = %participant.identity(),
                                codec = %publication.mime_type(),
                                "showing the stream's video track"
                            );
                            if let Some(old) = pump.take() {
                                old.abort();
                            }
                            pump = Some(tokio::spawn(pump_frames(video, canvas, to_writer.clone())));
                        }
                        RemoteTrack::Audio(audio_track) => {
                            let identity = participant.identity().to_string();
                            match keep_audio(ticket.voices, &identity) {
                                true if muted => audio_track.disable(),
                                true => {}
                                // Disable first: unsubscribing alone leaves
                                // an audible burst until the server acks.
                                false => {
                                    audio_track.disable();
                                    publication.set_subscribed(false);
                                }
                            }
                        }
                    },
                    Some(RoomEvent::Disconnected { reason }) => {
                        break Err(format!("the stream room disconnected: {reason:?}"));
                    }
                    Some(_) => {}
                    None => break Err("the stream room's events ended".to_string()),
                },
            }
        };
        if let Some(pump) = pump {
            pump.abort();
        }
        if let Err(err) = room.close().await {
            warn!(error = ?err, "failed to leave the stream room cleanly");
        }
        drop(audio);
        outcome
    }

    fn set_audio_enabled(room: &Room, enabled: bool) {
        for participant in room.remote_participants().values() {
            for publication in participant.track_publications().values() {
                let Some(RemoteTrack::Audio(track)) = publication.track() else {
                    continue;
                };
                match enabled {
                    true => track.enable(),
                    false => track.disable(),
                }
            }
        }
    }

    /// Decoded frames onto the canvas and over to the writer. A frame that
    /// arrives while mpv is still taking the last one is dropped: the next
    /// is newer, and a queue would only be latency.
    async fn pump_frames(track: RemoteVideoTrack, canvas: Canvas, to_writer: SyncSender<Vec<u8>>) {
        let mut frames = NativeVideoStream::new(track.rtc_track());
        while let Some(frame) = frames.next().await {
            let mut source = frame.buffer.to_i420();
            let place = canvas.fit(source.width(), source.height());
            let scaled = source.scale(place.width as i32, place.height as i32);
            let mut out = canvas.black();
            canvas.blit(&mut out, place, scaled.data(), scaled.strides());
            match to_writer.try_send(out) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                // mpv is gone; `VideoPlayer::poll` reports it.
                Err(TrySendError::Disconnected(_)) => break,
            }
        }
    }

    /// The thread that owns mpv's stdin. Blocking writes stay off the
    /// runtime; the pipe closes when the last sender drops, which is mpv's
    /// end of file.
    fn spawn_writer(mut to_mpv: ChildStdin) -> SyncSender<Vec<u8>> {
        let (to_writer, frames) = sync_channel::<Vec<u8>>(1);
        std::thread::spawn(move || {
            while let Ok(frame) = frames.recv() {
                if to_mpv.write_all(&frame).is_err() {
                    break;
                }
            }
        });
        to_writer
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod feed {
    use std::process::ChildStdin;

    use super::{Canvas, StreamTicket};

    /// No LiveKit SDK on this platform: the viewer fails at once. The
    /// `video_stream` capability is never advertised here, so nothing asks.
    pub(in super::super) struct StreamFeed;

    impl StreamFeed {
        pub(in super::super) fn start(_: StreamTicket, _: Canvas, _: ChildStdin) -> Self {
            Self
        }

        pub(in super::super) fn toggle_mute(&self) {}

        pub(in super::super) fn failure(&self) -> Option<String> {
            Some("watching streams is not supported on this platform".to_string())
        }
    }
}

pub(super) use feed::StreamFeed;

#[cfg(test)]
#[path = "stream_view_test.rs"]
mod stream_view_test;
