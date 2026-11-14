//! The watch modal's orchestration: the gate deciding player or setup box,
//! the pair sends to the paired CLI, where each source's position comes
//! from and goes back to, and the one place a watch's story is logged and
//! counted. The modal itself (`state.rs`) is pure; everything that leaves
//! the session goes through here.

use std::time::Instant;

use chrono::Utc;
use late_core::models::user::TerminalImagesMode;
use uuid::Uuid;

use super::state::{LiveTicket, SetupInputs, WatchOutcome, WatchRequest, WatchSource};
use crate::app::common::primitives::Banner;
use crate::app::files::terminal_image::TerminalImageProtocol;
use crate::app::state::App;
use crate::metrics;
use crate::paired_clients::{PairControlMessage, VideoControl, VideoReport};

/// Why a live play was given up on.
#[derive(Clone, Copy, Debug)]
enum LostCause {
    /// No `video_state` for `WATCH_LOST_AFTER`.
    Silent,
    /// A send reached no paired CLI advertising `video`.
    Unreachable,
}

/// What a watch press opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Watched {
    Player,
    /// This session cannot play it: the setup box naming what is missing.
    Setup,
}

/// A watch press (`w` on a News story, Enter or a click on a YouTube link
/// in chat): the player when this session can play (`can_play`), the
/// setup box naming what is missing otherwise.
pub(crate) fn watch(app: &mut App, request: WatchRequest) -> Watched {
    match can_play(app) {
        true => {
            play(app, request);
            Watched::Player
        }
        false => {
            close(app);
            let inputs = setup_inputs(app);
            app.video.open_setup(&request.url, &inputs);
            record(app, &request.source, None, WatchOutcome::Setup);
            Watched::Setup
        }
    }
}

/// `▶ watch here` (or a bare `/watch`) in a live stream's room: the stream
/// in the player, received by the paired CLI itself. A session whose CLI
/// cannot play it gets one line pointing at the browser and nothing else:
/// it either plays here or it does not.
pub(crate) fn watch_stream(app: &mut App, room_id: Uuid) {
    // The button is a toggle: pressed on the stream already open, it stops.
    if app.video.stream_room() == Some(room_id) {
        close(app);
        return;
    }
    if !can_play_stream(app) {
        app.banner = Some(Banner::error(
            "Can't play streams in this terminal. Watch in your browser with /watch @user.",
        ));
        return;
    }
    let watcher_id = format!("cli-{}", app.presence.session_id());
    let viewer_voice_channel = app.voice.current_room(app.user_id);
    let watch = match app
        .stream_service
        .cli_watch(room_id, &watcher_id, viewer_voice_channel)
    {
        Ok(Some(watch)) => watch,
        Ok(None) => {
            app.banner = Some(Banner::error("This stream is not live right now."));
            return;
        }
        Err(error) => {
            late_core::error_span!(
                "stream_watch_ticket_failed",
                error = ?error,
                room_id = %room_id,
                "failed to mint a stream watch ticket for the paired cli"
            );
            app.banner = Some(Banner::error("Couldn't open the stream."));
            return;
        }
    };
    app.stream_service
        .note_viewer_in_room(room_id, app.user_id, &app.username);
    play(
        app,
        WatchRequest {
            source: WatchSource::LiveStream(LiveTicket {
                room_id,
                stream_id: watch.stream_id,
                watcher_id,
                livekit_url: watch.livekit_url,
                token: watch.token,
                voices: watch.voices,
            }),
            url: watch.watch_url,
            title: watch.title,
            author: watch.streamer_username,
            shared_at: Utc::now(),
        },
    );
}

/// Open the player on `request`, for a caller that checked `can_play`. It
/// resumes where the video's source last left it: a News story on the
/// reader's row, a chat link where this session left it.
pub(crate) fn play(app: &mut App, request: WatchRequest) {
    close(app);
    let resume_from = match &request.source {
        WatchSource::NewsArticle(article_id) => app.chat.news.watch_position(*article_id),
        WatchSource::ChatLink => app.video.link_position(&request.url),
        WatchSource::LiveStream(_) => None,
    };
    let source = request.source.clone();
    let play_id = app.video.open_player(request, resume_from, Instant::now());
    record(app, &source, Some(play_id), WatchOutcome::Opened);
}

/// Both gates: a paired CLI that can play (it sits in the terminal and
/// already checked it, tmux included; the server's image detection maps
/// WezTerm to iTerm2 and is no witness here), and the Tweaks "Terminal
/// images" row not `Off`, the user's one way to keep graphics out.
pub(crate) fn can_play(app: &App) -> bool {
    let video_cli = app
        .paired_client_registry
        .as_ref()
        .is_some_and(|registry| registry.has_video_cli(&app.session_token));
    let images_on = match app.profile_state.profile().terminal_images {
        TerminalImagesMode::Auto | TerminalImagesMode::Sixel => true,
        TerminalImagesMode::Off => false,
    };
    video_cli && images_on
}

/// The same two gates for a live stream, against the `video_stream`
/// capability: a CLI new enough to receive a stream itself, with everything
/// a video needs but yt-dlp.
fn can_play_stream(app: &App) -> bool {
    let stream_cli = app
        .paired_client_registry
        .as_ref()
        .is_some_and(|registry| registry.has_video_stream_cli(&app.session_token));
    let images_on = match app.profile_state.profile().terminal_images {
        TerminalImagesMode::Auto | TerminalImagesMode::Sixel => true,
        TerminalImagesMode::Off => false,
    };
    stream_cli && images_on
}

fn setup_inputs(app: &App) -> SetupInputs {
    let images = app.profile_state.profile().terminal_images;
    SetupInputs {
        cli: app
            .paired_client_registry
            .as_ref()
            .and_then(|registry| registry.cli_state(&app.session_token)),
        protocol: match images {
            TerminalImagesMode::Auto | TerminalImagesMode::Off => app.detected_image_protocol,
            TerminalImagesMode::Sixel => Some(TerminalImageProtocol::Sixel),
        },
        multiplexer_term: app.multiplexer_term,
        windows_terminal: app.windows_terminal,
        images_off: match images {
            TerminalImagesMode::Off => true,
            TerminalImagesMode::Auto | TerminalImagesMode::Sixel => false,
        },
    }
}

/// Close the modal (Esc, `[x]`, a page change, another modal, the
/// session's end). What a closing player leaves goes back to its source:
/// News stores it on the reader's row, a chat link keeps it for the
/// session. Closing is state only: the next draw's `sync` sends the stop.
pub(crate) fn close(app: &mut App) {
    let Some(closed) = app.video.close() else {
        return;
    };
    match closed.source {
        WatchSource::NewsArticle(article_id) => {
            app.chat.news.remember_watch(article_id, closed.memory);
        }
        WatchSource::ChatLink => app.video.remember_link(closed.url, closed.memory),
        WatchSource::LiveStream(_) => {}
    }
}

/// A key in the player, relayed to mpv while it is up.
pub(crate) fn send_control(app: &mut App, control: VideoControl) {
    let Some(message) = app
        .video
        .player_mut()
        .and_then(|player| player.control(control))
    else {
        return;
    };
    deliver(app, message);
}

/// After every draw: tell the CLI what the drawn player wants (a play, a
/// move after `f` or a resize, a stop once it closed).
pub(crate) fn sync(app: &mut App) {
    // An inline stream lives while its room's pane draws it: switching
    // rooms or pages ends the watch.
    if app.video.inline_left_the_screen() {
        close(app);
    }
    for message in app.video.cli_messages() {
        deliver(app, message);
    }
}

/// The SSH session is ending: stop whatever the CLI is playing for it.
pub(crate) fn end_session(app: &mut App) {
    close(app);
    sync(app);
}

/// One `video_state` from the paired CLI. A report for any play but the
/// open one is late and dropped. Every fifteen seconds of a News story's
/// play the position is saved (`Player::checkpoint`), so a session that
/// drops still resumes close to where it was.
pub(crate) fn apply_report(app: &mut App, play_id: u64, report: &VideoReport) {
    let now = Instant::now();
    let Some(player) = app
        .video
        .player_mut()
        .filter(|player| player.play_id() == play_id)
    else {
        tracing::debug!(play_id, "dropping video state for a play no longer open");
        return;
    };
    let source = player.source().clone();
    let outcome = player.apply(report, now);
    match (&source, player.checkpoint(now)) {
        (WatchSource::NewsArticle(article_id), Some(position)) => {
            app.chat.news.save_watch_position(*article_id, position);
        }
        // A chat link's position lives as long as the session; the close
        // keeps it. A live stream has none.
        (WatchSource::ChatLink | WatchSource::LiveStream(_), Some(_))
        | (
            WatchSource::NewsArticle(_) | WatchSource::ChatLink | WatchSource::LiveStream(_),
            None,
        ) => {}
    }
    let Some(outcome) = outcome else {
        return;
    };
    // The CLI's reason is for the log only, never shown raw.
    if let VideoReport::Failed { message } = report {
        late_core::error_span!(
            "video_watch_failed",
            play_id,
            ?source,
            message = %message,
            "paired cli could not play the video"
        );
    }
    record(app, &source, Some(play_id), outcome);
}

/// The world tick: a play the CLI went quiet on is lost, and a live
/// stream's player heartbeats once a second, which keeps this viewer in the
/// stream's watcher count and is how the player learns the stream is over.
/// True while a player is up, once a second, so a status that changed
/// outside a draw (a send that reached nobody) repaints.
pub(crate) fn tick(app: &mut App, now: Instant, one_hz: bool) -> bool {
    let Some(player) = app.video.player_mut() else {
        return false;
    };
    let (source, play_id) = (player.source().clone(), player.play_id());
    if let Some(outcome) = player.expire(now) {
        lost(&source, play_id, LostCause::Silent);
        record(app, &source, Some(play_id), outcome);
        return true;
    }
    match (&source, one_hz) {
        (WatchSource::LiveStream(ticket), true) => {
            let live = app
                .stream_service
                .watch_heartbeat(&ticket.stream_id, &ticket.watcher_id);
            let ended = match live {
                true => None,
                false => app.video.player_mut().and_then(|player| player.end()),
            };
            // A stream that is over leaves nothing to look at: the player
            // goes, inline or full screen, and one line says why.
            if let Some(outcome) = ended {
                record(app, &source, Some(play_id), outcome);
                close(app);
                app.banner = Some(Banner::info("The stream ended."));
            }
        }
        (WatchSource::LiveStream(_), false)
        | (WatchSource::NewsArticle(_) | WatchSource::ChatLink, _) => {}
    }
    one_hz
}

fn deliver(app: &mut App, message: PairControlMessage) {
    let delivered = app
        .paired_client_registry
        .as_ref()
        .is_some_and(|registry| registry.send_control_to_video_cli(&app.session_token, message));
    if delivered {
        return;
    }
    let Some(player) = app.video.player_mut() else {
        return;
    };
    let (source, play_id) = (player.source().clone(), player.play_id());
    if let Some(outcome) = player.lose() {
        lost(&source, play_id, LostCause::Unreachable);
        record(app, &source, Some(play_id), outcome);
    }
}

fn lost(source: &WatchSource, play_id: u64, cause: LostCause) {
    match cause {
        LostCause::Silent => late_core::error_span!(
            "video_watch_lost",
            play_id,
            ?source,
            "no video state from the paired cli for 10s"
        ),
        LostCause::Unreachable => late_core::error_span!(
            "video_watch_lost",
            play_id,
            ?source,
            "no paired cli with video took the video control"
        ),
    }
}

/// Count an outcome and log the ones without a cause of their own.
fn record(app: &mut App, source: &WatchSource, play_id: Option<u64>, outcome: WatchOutcome) {
    metrics::record_video_watch(source, outcome);
    match outcome {
        WatchOutcome::Opened => tracing::info!(?play_id, ?source, "video opened"),
        WatchOutcome::Setup => tracing::info!(?source, "video watch opened the setup box"),
        WatchOutcome::Played => tracing::info!(?play_id, ?source, "video playing"),
        WatchOutcome::Ended => tracing::info!(?play_id, ?source, "video ended"),
        // Logged with the CLI's reason by `apply_report`.
        WatchOutcome::Failed => {
            app.banner = Some(Banner::error("Couldn't play the video."));
        }
        // Logged with its cause by `lost`.
        WatchOutcome::Lost => {}
    }
}
