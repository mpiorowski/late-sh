use late_core::MutexRecover;
use uuid::Uuid;

use crate::app::common::primitives::{Banner, Screen};
use crate::app::scratchpad::state::ScratchpadState;
use crate::app::state::App;
use crate::state::ActiveUsers;

/// `/pair @user`: validate and post a directed invite. Lives here (not in
/// `chat/state.rs`) because the composer has no handle on `active_users` or
/// the scratchpad registry, the same reason directed daily challenges are
/// dispatched after submit rather than parsed-and-applied in one step.
pub(crate) fn request_pair_invite(app: &mut App, target_username: &str) {
    let Some(registry) = app.scratchpad_registry.clone() else {
        app.banner = Some(Banner::error("Pairing is unavailable right now"));
        return;
    };
    if app.scratchpad.is_some() || registry.is_paired(app.user_id) {
        app.banner = Some(Banner::error(
            "You're already paired — Esc out of the scratchpad first",
        ));
        return;
    }
    let Some(target_id) = find_active_user_by_username(app.active_users.as_ref(), target_username)
    else {
        app.banner = Some(Banner::error(&format!("@{target_username} is not online")));
        return;
    };
    if target_id == app.user_id {
        app.banner = Some(Banner::error("You can't pair with yourself"));
        return;
    }
    if registry.is_paired(target_id) {
        app.banner = Some(Banner::error(&format!(
            "@{target_username} is already paired with someone"
        )));
        return;
    }
    registry.invite(app.user_id, app.username.clone(), target_id);
    app.banner = Some(Banner::success(&format!(
        "Invited @{target_username} to pair"
    )));
}

/// Polled once per tick (see `tick.rs`), same cadence as the `session_rx`
/// drain: no live cross-session push exists in this codebase for invites,
/// so the target picks theirs up on their next tick.
pub(crate) fn poll_invite(app: &mut App) {
    if app.pair_invite_pending.is_some() || app.scratchpad.is_some() {
        return;
    }
    let Some(registry) = app.scratchpad_registry.clone() else {
        return;
    };
    if let Some(invite) = registry.take_invite_for(app.user_id) {
        app.banner = Some(Banner::info(&format!(
            "@{} wants to pair — Enter to join, Esc to dismiss",
            invite.from_username
        )));
        app.pair_invite_pending = Some(invite);
    }
}

/// Picks up a pairing this session did not explicitly accept — the inviter
/// side, once the target has accepted. There is no push from `accept()` back
/// to the inviter's session, so both sides converge on `Screen::Scratchpad`
/// purely by polling `registry.lookup` once per tick, same as `poll_invite`.
pub(crate) fn poll_pairing(app: &mut App) {
    if app.scratchpad.is_some() {
        return;
    }
    let Some(registry) = app.scratchpad_registry.clone() else {
        return;
    };
    let Some(shared) = registry.lookup(app.user_id) else {
        return;
    };
    let Some((partner_id, partner_username)) = shared
        .lock_recover()
        .partner_of(app.user_id)
        .map(|(id, name)| (id, name.to_string()))
    else {
        return;
    };
    app.pair_invite_pending = None;
    app.scratchpad = Some(ScratchpadState::new(
        registry,
        shared,
        app.user_id,
        partner_id,
        partner_username,
    ));
    app.set_screen(Screen::Scratchpad);
}

fn find_active_user_by_username(
    active_users: Option<&ActiveUsers>,
    username: &str,
) -> Option<Uuid> {
    let active_users = active_users?;
    let guard = active_users.lock_recover();
    guard
        .iter()
        .find(|(_, user)| user.username.eq_ignore_ascii_case(username))
        .map(|(id, _)| *id)
}

#[cfg(test)]
#[path = "invite_test.rs"]
mod invite_test;
