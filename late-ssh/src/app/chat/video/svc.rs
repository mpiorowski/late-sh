//! The session's one video on the paired CLI, and chat's inline player.
//! The News player and the inline video share the CLI's single mpv: after
//! every draw `sync` settles the inline video against what the frame
//! showed, then sends whatever brings the CLI in line with the one that
//! wants it (the News player first). `deliver` carries every pair message
//! and gives up the live play when no CLI takes it, and `apply_report`
//! routes the CLI's `video_state` to the play it names. The inline
//! player's gate, keys, and the one place its story is logged and counted
//! are here too; its state is `state.rs`.

use std::time::Instant;

use uuid::Uuid;

use super::state::{InlineVideo, Step};
use crate::app::chat::news::watch::state::WatchOutcome;
use crate::app::chat::news::watch::svc::{self as news_watch, LostCause};
use crate::app::common::primitives::{Banner, Screen};
use crate::app::state::App;
use crate::metrics;
use crate::paired_clients::{PairControlMessage, VideoControl, VideoReport};

/// Enter on the selected message in `room_id`, or a click on its
/// thumbnail: a YouTube link plays inside the thumbnail when this session
/// can play it, the setup box opens when it cannot, and the video already
/// there pauses, resumes, or (once over) plays again. False when the
/// message carries no YouTube link, or the chat is not Home's (the inline
/// video is drawn there alone).
pub(crate) fn watch_selected(app: &mut App, room_id: Uuid) -> bool {
    if app.screen != Screen::Dashboard {
        return false;
    }
    let Some((message_id, link)) = app.chat.selected_youtube_link_in_room(room_id) else {
        return false;
    };
    let now = Instant::now();
    let here = app
        .chat
        .inline_video()
        .filter(|video| video.message_id() == message_id)
        .map(InlineVideo::holds_rect);
    match here {
        Some(true) => {
            send_control(app, VideoControl::TogglePause);
            return true;
        }
        Some(false) => {
            app.chat.replay_inline_video(now);
            let play_id = app.chat.inline_video().and_then(InlineVideo::play_id);
            record(app, play_id, WatchOutcome::Opened);
            return true;
        }
        None => {}
    }
    if !news_watch::can_play(app) {
        let inputs = news_watch::setup_inputs(app);
        app.chat.news.open_setup_box(&link.url, &inputs);
        record(app, None, WatchOutcome::Setup);
        return true;
    }
    if !app.chat.has_inline_preview(message_id) {
        app.banner = Some(Banner::error(
            "This video's thumbnail hasn't loaded, so it can't play here.",
        ));
        return true;
    }
    app.chat.start_inline_video(message_id, link.url, now);
    let play_id = app.chat.inline_video().and_then(InlineVideo::play_id);
    record(app, play_id, WatchOutcome::Opened);
    true
}

/// Esc on Home: the inline video goes away. Closing is state only: the
/// next draw's sync sends the stop. False when there was none.
pub(crate) fn stop(app: &mut App) -> bool {
    app.chat.close_inline_video()
}

/// A key for the inline video, relayed to mpv while it is up.
fn send_control(app: &mut App, control: VideoControl) {
    let Some(message) = app
        .chat
        .inline_video()
        .and_then(|video| video.control(control))
    else {
        return;
    };
    deliver(app, message);
}

/// After every draw: settle the inline video against what the frame
/// showed of it, then tell the CLI what the wanted play is (a play, a
/// move, a stop once nothing wants one).
pub(crate) fn sync(app: &mut App) {
    match app.chat.settle_inline_video(Instant::now()) {
        Some(Step::Close) => tracing::info!("chat video closed: its thumbnail left the screen"),
        Some(Step::Park) => tracing::debug!("chat video parked under something drawn over it"),
        Some(Step::Unpark) => tracing::debug!("chat video back from parked"),
        Some(Step::Keep) | None => {}
    }
    for message in app.chat.video_sync() {
        deliver(app, message);
    }
}

/// The SSH session is ending: stop whatever the CLI is playing for it.
pub(crate) fn end_session(app: &mut App) {
    app.chat.news.close_watch();
    app.chat.close_inline_video();
    sync(app);
}

/// One pair message for the session's video CLI. One that reaches no CLI
/// advertising `video` loses whichever play was live.
pub(crate) fn deliver(app: &mut App, message: PairControlMessage) {
    let delivered = app
        .paired_client_registry
        .as_ref()
        .is_some_and(|registry| registry.send_control_to_video_cli(&app.session_token, message));
    if delivered {
        return;
    }
    news_watch::lose_unreachable(app);
    let Some(video) = app.chat.inline_video_mut() else {
        return;
    };
    let play_id = video.play_id();
    if let Some(outcome) = video.lose() {
        lost(play_id, LostCause::Unreachable);
        record(app, play_id, outcome);
    }
}

/// One `video_state` from the paired CLI, for the News player or the
/// inline video, whichever holds `play_id`. A report for any other play
/// is late and dropped.
pub(crate) fn apply_report(app: &mut App, play_id: u64, report: &VideoReport) {
    let news = app.chat.news.player_mut().map(|player| player.play_id()) == Some(play_id);
    let inline = app.chat.inline_video().and_then(InlineVideo::play_id) == Some(play_id);
    match (news, inline) {
        (true, _) => news_watch::apply_report(app, play_id, report),
        (false, true) => apply_inline_report(app, play_id, report),
        (false, false) => {
            tracing::debug!(play_id, "dropping video state for a play no longer open");
        }
    }
}

fn apply_inline_report(app: &mut App, play_id: u64, report: &VideoReport) {
    let Some(video) = app.chat.inline_video_mut() else {
        return;
    };
    let Some(outcome) = video.apply(report, Instant::now()) else {
        return;
    };
    // The CLI's reason is for the log only, never shown raw.
    if let VideoReport::Failed { message } = report {
        late_core::error_span!(
            "chat_video_failed",
            play_id,
            message = %message,
            "paired cli could not play the chat video"
        );
    }
    record(app, Some(play_id), outcome);
}

/// The world tick: a play the CLI went quiet on is lost, the News
/// player's or the inline video's. True once a second while either is up,
/// so a status that changed outside a draw repaints.
pub(crate) fn tick(app: &mut App, now: Instant, one_hz: bool) -> bool {
    let news = news_watch::tick(app, now, one_hz);
    let inline = tick_inline(app, now, one_hz);
    news || inline
}

fn tick_inline(app: &mut App, now: Instant, one_hz: bool) -> bool {
    let Some(video) = app.chat.inline_video_mut() else {
        return false;
    };
    let play_id = video.play_id();
    match video.expire(now) {
        Some(outcome) => {
            lost(play_id, LostCause::Silent);
            record(app, play_id, outcome);
            true
        }
        None => one_hz,
    }
}

fn lost(play_id: Option<u64>, cause: LostCause) {
    match cause {
        LostCause::Silent => late_core::error_span!(
            "chat_video_lost",
            ?play_id,
            "no video state from the paired cli for 10s"
        ),
        LostCause::Unreachable => late_core::error_span!(
            "chat_video_lost",
            ?play_id,
            "no paired cli with video took the chat video control"
        ),
    }
}

/// Count an inline video's outcome and log the ones without a cause of
/// their own.
fn record(app: &mut App, play_id: Option<u64>, outcome: WatchOutcome) {
    metrics::record_chat_video(outcome);
    match outcome {
        WatchOutcome::Opened => tracing::info!(?play_id, "chat video opened"),
        WatchOutcome::Setup => tracing::info!("chat video opened the setup box"),
        WatchOutcome::Played => tracing::info!(?play_id, "chat video playing"),
        WatchOutcome::Ended => tracing::info!(?play_id, "chat video ended"),
        // Logged with the CLI's reason by `apply_inline_report`.
        WatchOutcome::Failed => {
            app.banner = Some(Banner::error("Couldn't play the video."));
        }
        // Logged with its cause by `lost`.
        WatchOutcome::Lost => {}
    }
}
