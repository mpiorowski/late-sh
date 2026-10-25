use std::sync::atomic::Ordering;

use crate::app::state::App;
use crate::metrics::{self, HandoffStage};
use crate::paired_clients::PairControlMessage;

use super::state::{HandoffCheck, ResumePage, ResumePoint};

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
        in_voice: app.voice.is_joined(app.user_id),
        watching_video: app.video.is_open(),
        composer_draft: app
            .chat
            .composer()
            .lines()
            .iter()
            .any(|line| !line.trim().is_empty()),
    };
    if !check.ready() {
        return;
    }
    let Some(registry) = app.paired_client_registry.as_ref() else {
        return;
    };
    let resume = ResumePoint {
        page: ResumePage::from_screen(app.screen),
        room_id: app.chat.selected_room_id,
    };
    let delivered = registry.send_control_to_reconnect_cli(
        &app.session_token,
        PairControlMessage::Reconnect {
            resume: resume.encode(),
        },
    );
    if !delivered {
        return;
    }
    app.handoff.sent = true;
    metrics::record_drain_handoff(HandoffStage::Sent);
    tracing::info!(
        user_id = %app.user_id,
        page = ?resume.page,
        "asked an idle cli session to move to the new pod"
    );
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
