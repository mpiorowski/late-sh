use std::sync::atomic::Ordering;

use crate::app::common::primitives::Banner;
use crate::app::state::App;
use crate::metrics::{self, HandoffStage};
use crate::paired_clients::PairControlMessage;

use super::state::{HandoffCheck, ResumePoint};

/// Once per world tick: on a draining pod, ask this session's CLI to move to
/// the new pod once the session has gone quiet. Sessions without a capable
/// CLI (plain SSH, older CLIs, OpenSSH mode) are never asked and keep the
/// update banner. A live pod returns before reading anything else, since
/// every session pays this on every tick.
pub(crate) fn tick(app: &mut App) {
    if app.handoff.sent || !app.is_draining.load(Ordering::Relaxed) {
        return;
    }
    let check = HandoffCheck {
        auto_restart: app.profile_state.profile().auto_restart,
        idle: app.last_active_at.elapsed(),
        idle_needed: app.handoff.idle_needed,
        in_voice: app.voice.is_joined(app.user_id),
        watching_video: app.video.is_open(),
        composer_draft: app.chat.has_unsent_draft(),
    };
    if !check.ready() {
        return;
    }
    let Some(resume) = ask_cli_to_move(app) else {
        return;
    };
    app.handoff.sent = true;
    metrics::record_drain_handoff(HandoffStage::Sent);
    tracing::info!(
        user_id = %app.user_id,
        page = ?resume.page,
        "asked an idle cli session to move to the new pod"
    );
}

/// Whether this session's CLI can be moved: a paired CLI advertises
/// `reconnect`. The quit modal shows `r restart` only then.
pub(crate) fn can_request_move(app: &App) -> bool {
    app.paired_client_registry
        .as_ref()
        .is_some_and(|registry| registry.has_reconnect_cli(&app.session_token))
}

/// The quit modal's `r restart`: ask this session's CLI to move now, on a
/// live pod as on a draining one. The CLI opens a fresh session wherever the
/// edge sends it and lands on this page. Returns whether a CLI took the
/// request; without one nothing happens. It leaves `HandoffState::sent`
/// alone, so a restart whose dial failed still gets the drain handoff later.
pub(crate) fn request_move(app: &mut App) -> bool {
    let Some(resume) = ask_cli_to_move(app) else {
        return false;
    };
    metrics::record_drain_handoff(HandoffStage::Requested);
    tracing::info!(
        user_id = %app.user_id,
        page = ?resume.page,
        "asked a cli session to restart at the user's request"
    );
    app.banner = Some(Banner::info("Restarting your session..."));
    true
}

/// Send this session's resume point to its paired CLIs advertising
/// `reconnect`. `None` when none took it (plain SSH, older CLIs, OpenSSH
/// mode).
fn ask_cli_to_move(app: &App) -> Option<ResumePoint> {
    let registry = app.paired_client_registry.as_ref()?;
    let resume = ResumePoint {
        page: app.screen.page(),
        room_id: app.chat.selected_room_id,
    };
    let delivered = registry.send_control_to_reconnect_cli(
        &app.session_token,
        PairControlMessage::Reconnect {
            resume: resume.encode(),
        },
    );
    if delivered { Some(resume) } else { None }
}

/// Put a session the CLI moved here back where it was, from `App::new`
/// after the landing hook ran: the page as a digit on the first frame would
/// switch it, and the room as a hint chat takes once the room list arrives.
pub(crate) fn resume(app: &mut App, point: ResumePoint) {
    if let Some(room_id) = point.room_id {
        app.chat.resume_room(room_id);
    }
    let screen = point.page.screen();
    if app.screen != screen {
        app.set_screen(screen);
    }
}
