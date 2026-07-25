use std::time::Instant;

use late_core::MutexRecover;
use uuid::Uuid;

use crate::app::common::primitives::{Banner, Screen};
use crate::app::scratchpad::registry::{
    PAIR_INTENT_TTL, PairOutcome, PairSide, SharedScratchpad, SharedScratchpadRegistry,
};
use crate::app::scratchpad::state::ScratchpadState;
use crate::app::state::App;
use crate::state::ActiveUsers;

/// `/pair @user`: one half of the mutual handshake. Lives here (not in
/// `chat/state.rs`) because the composer has no handle on `active_users` or
/// the scratchpad registry, the same reason directed daily challenges are
/// dispatched after submit rather than parsed-and-applied in one step.
///
/// Running this never changes the target's screen. It either records our
/// intent and leaves them a banner, or completes a pairing they already
/// asked for.
pub(crate) fn request_pair(app: &mut App, target_username: &str) {
    let Some(registry) = app.scratchpad_registry.clone() else {
        app.banner = Some(Banner::error("Pairing is unavailable right now"));
        return;
    };
    let Some(target_id) = find_active_user_by_username(app.active_users.as_ref(), target_username)
    else {
        app.banner = Some(Banner::error(&format!("@{target_username} is not online")));
        return;
    };
    if target_id == app.user_id {
        app.banner = Some(Banner::error("You can't pair with yourself"));
        return;
    }
    let side = PairSide {
        user_id: app.user_id,
        username: app.username.clone(),
        session_token: app.session_token.clone(),
    };
    match registry.try_pair(side, target_id, Instant::now()) {
        PairOutcome::Waiting => {
            app.banner = Some(Banner::success(&format!(
                "Asked @{target_username} to pair. They have {} minutes to run /pair @{}",
                PAIR_INTENT_TTL.as_secs() / 60,
                app.username
            )));
        }
        PairOutcome::AlreadyAsked => {
            app.banner = Some(Banner::info(&format!(
                "Still waiting on @{target_username}. They already know you asked"
            )));
        }
        PairOutcome::Paired {
            shared,
            partner_id,
            partner_username,
        } => {
            enter_scratchpad(app, registry, shared, partner_id, partner_username);
        }
        PairOutcome::AlreadyPaired => {
            app.banner = Some(Banner::error(
                "You're already paired. Esc out of the scratchpad first",
            ));
        }
        PairOutcome::TargetBusy => {
            app.banner = Some(Banner::error(&format!(
                "@{target_username} is already paired with someone"
            )));
        }
    }
}

/// Polled once per tick (see `tick.rs`), same cadence as the `session_rx`
/// drain: no live cross-session push exists in this codebase, so the side
/// that ran `/pair` first learns about the completed pairing here, and a
/// target learns someone asked for them here. Returns true when something
/// render-visible changed.
///
/// A session already inside a scratchpad does no registry work at all: it
/// cannot join a second pairing, and nobody can post it a notice while
/// `try_pair` reports it busy.
pub(crate) fn poll(app: &mut App) -> bool {
    if app.scratchpad.is_some() {
        return false;
    }
    let Some(registry) = app.scratchpad_registry.clone() else {
        return false;
    };
    let poll = registry.poll(app.user_id, &app.session_token);
    let mut changed = false;
    if let Some(from_username) = poll.notice {
        app.banner = Some(Banner::info(&format!(
            "@{from_username} wants to pair. Run /pair @{from_username} to join them"
        )));
        changed = true;
    }
    let Some(shared) = poll.pairing else {
        return changed;
    };
    let Some((partner_id, partner_username)) = shared
        .lock_recover()
        .partner_of(app.user_id)
        .map(|(id, name)| (id, name.to_string()))
    else {
        return changed;
    };
    enter_scratchpad(app, registry, shared, partner_id, partner_username);
    true
}

fn enter_scratchpad(
    app: &mut App,
    registry: SharedScratchpadRegistry,
    shared: SharedScratchpad,
    partner_id: Uuid,
    partner_username: String,
) {
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
#[path = "pair_test.rs"]
mod pair_test;
