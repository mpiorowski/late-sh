//! The watch modal's orchestration: the gate deciding player or setup box,
//! the pair sends to the paired CLI, and the one place a watch's story is
//! logged and counted. The modal itself (`state.rs`) is pure; everything
//! that leaves the session goes through here.

use std::time::Instant;

use late_core::models::user::TerminalImagesMode;
use uuid::Uuid;

use super::state::{SetupInputs, WatchOutcome};
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

/// `w` on a video story: the player when the paired CLI advertises
/// `video` and the session draws Kitty graphics, the setup box naming what
/// is missing otherwise. False when the story is not a video in the
/// snapshot.
pub(crate) fn watch_article(app: &mut App, article_id: Uuid) -> bool {
    if !app.chat.news.is_watchable(article_id) {
        return false;
    }
    match can_play(app) {
        true => open_player(app, article_id),
        false => {
            let inputs = setup_inputs(app);
            let opened = app.chat.open_watch_setup(article_id, &inputs);
            if opened {
                record(app, None, WatchOutcome::Setup);
            }
            opened
        }
    }
}

/// `o` on the live strip or a Live row: a video this session can play goes
/// straight to the player; anything else opens the reader. `o` is "open",
/// not "watch", so it never lands on the setup box.
pub(crate) fn open_article(app: &mut App, article_id: Uuid) -> bool {
    match app.chat.news.is_watchable(article_id) && can_play(app) {
        true => open_player(app, article_id),
        false => app.chat.open_reader_for_article(article_id),
    }
}

fn open_player(app: &mut App, article_id: Uuid) -> bool {
    let opened = app.chat.open_watch_player(article_id, Instant::now());
    if opened {
        let play_id = app.chat.news.player_mut().map(|player| player.play_id());
        record(app, play_id, WatchOutcome::Opened);
    }
    opened
}

/// Both gates: a paired CLI that can play, and this session drawing
/// Kitty graphics (which folds in the Tweaks row and the tmux exclusion).
fn can_play(app: &App) -> bool {
    let video_cli = app
        .paired_client_registry
        .as_ref()
        .is_some_and(|registry| registry.has_video_cli(&app.session_token));
    video_cli && app.terminal_image_protocol() == Some(TerminalImageProtocol::Kitty)
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
        multiplexer_term: app.terminal_images_disabled,
        windows_terminal: app.windows_terminal,
        images_off: match images {
            TerminalImagesMode::Off => true,
            TerminalImagesMode::Auto | TerminalImagesMode::Sixel => false,
        },
    }
}

/// A key in the player, relayed to mpv while it is up.
pub(crate) fn send_control(app: &mut App, control: VideoControl) {
    let Some(message) = app
        .chat
        .news
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
    for message in app.chat.news.video_sync() {
        deliver(app, message);
    }
}

/// The SSH session is ending: stop whatever the CLI is playing for it.
pub(crate) fn end_session(app: &mut App) {
    app.chat.news.close_watch();
    sync(app);
}

/// One `video_state` from the paired CLI. A report for any play but the
/// open one is late and dropped.
pub(crate) fn apply_report(app: &mut App, play_id: u64, report: &VideoReport) {
    let Some(player) = app
        .chat
        .news
        .player_mut()
        .filter(|player| player.play_id() == play_id)
    else {
        tracing::debug!(play_id, "dropping video state for a play no longer open");
        return;
    };
    let Some(outcome) = player.apply(report, Instant::now()) else {
        return;
    };
    // The CLI's reason is for the log only, never shown raw.
    if let VideoReport::Failed { message } = report {
        late_core::error_span!(
            "news_watch_failed",
            play_id,
            message = %message,
            "paired cli could not play the news video"
        );
    }
    record(app, Some(play_id), outcome);
}

/// The world tick: a play the CLI went quiet on is lost. True while a
/// player is up, once a second, so a status that changed outside a draw
/// (a send that reached nobody) repaints.
pub(crate) fn tick(app: &mut App, now: Instant, one_hz: bool) -> bool {
    let Some(player) = app.chat.news.player_mut() else {
        return false;
    };
    let play_id = player.play_id();
    match player.expire(now) {
        Some(outcome) => {
            lost(play_id, LostCause::Silent);
            record(app, Some(play_id), outcome);
            true
        }
        None => one_hz,
    }
}

fn deliver(app: &mut App, message: PairControlMessage) {
    let delivered = app
        .paired_client_registry
        .as_ref()
        .is_some_and(|registry| registry.send_control_to_video_cli(&app.session_token, message));
    if delivered {
        return;
    }
    let Some(player) = app.chat.news.player_mut() else {
        return;
    };
    let play_id = player.play_id();
    if let Some(outcome) = player.lose() {
        lost(play_id, LostCause::Unreachable);
        record(app, Some(play_id), outcome);
    }
}

fn lost(play_id: u64, cause: LostCause) {
    match cause {
        LostCause::Silent => late_core::error_span!(
            "news_watch_lost",
            play_id,
            "no video state from the paired cli for 10s"
        ),
        LostCause::Unreachable => late_core::error_span!(
            "news_watch_lost",
            play_id,
            "no paired cli with video took the news video control"
        ),
    }
}

/// Count an outcome and log the ones without a cause of their own.
fn record(app: &mut App, play_id: Option<u64>, outcome: WatchOutcome) {
    metrics::record_news_watch(outcome);
    match outcome {
        WatchOutcome::Opened => tracing::info!(?play_id, "news video opened"),
        WatchOutcome::Setup => tracing::info!("news watch opened the setup box"),
        WatchOutcome::Played => tracing::info!(?play_id, "news video playing"),
        WatchOutcome::Ended => tracing::info!(?play_id, "news video ended"),
        // Logged with the CLI's reason by `apply_report`.
        WatchOutcome::Failed => {
            app.banner = Some(Banner::error("Couldn't play the video."));
        }
        // Logged with its cause by `lost`.
        WatchOutcome::Lost => {}
    }
}
