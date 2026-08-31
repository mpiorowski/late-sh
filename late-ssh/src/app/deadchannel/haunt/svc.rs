//! Orchestration for the haunting: arming at session start, the splash
//! door, the glitch scheduler, the own-name flicker, the invitation, and
//! the `/haunt` admin controls. This is the only haunting layer that
//! touches `App`, logging, and persistence; the machines in `state.rs`
//! stay pure, and the root files keep one routing line each.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::state::{
    ClockGlitch, FirstContactMarks, GlitchTick, HauntCommand, HauntState, INVITE_DELAY_DAYS,
    NameFlicker, WhisperState, WhisperTick,
};
use crate::app::common::primitives::Banner;
use crate::app::state::App;

/// Build the session's haunting slot. First contact is a nonrenewable
/// resource: while it is admin-scoped scaffolding nothing ever arms for
/// anyone else (GAME.md, First contact). The chain order is the spec:
/// stage-2 hits arm the stage-3 whisper (it fires on the next fresh
/// connect), and the delivered whisper schedules the stage-4 invitation.
/// Dice are per session on purpose: the same person on two evenings, or
/// two admins side by side, roll differently.
pub(crate) fn arm(
    is_admin: bool,
    enabled: Arc<AtomicBool>,
    user_id: uuid::Uuid,
    marks: FirstContactMarks,
) -> HauntState {
    let eligible = is_admin && enabled.load(Ordering::Relaxed);
    HauntState {
        whisper: (eligible && marks.whisper_at.is_none() && marks.name_hits > 0)
            .then(|| WhisperState::for_user(user_id)),
        clock_glitch: eligible.then(|| ClockGlitch::new(session_seed(user_id), 0)),
        name_flicker: eligible.then(|| NameFlicker::new(session_seed(user_id), marks.name_hits)),
        marks,
        eligible,
        enabled,
    }
}

fn session_seed(user_id: uuid::Uuid) -> u64 {
    (user_id.as_u128() as u64) ^ (uuid::Uuid::now_v7().as_u128() as u64)
}

/// One world tick for the whole haunting. Returns true when the frame
/// changed. The splash block in `tick.rs` must run first (it advances
/// `splash_ticks` and consults `HauntState::holds_splash_door` before
/// expiring the splash on its own).
pub(crate) fn tick(app: &mut App) -> bool {
    let mut changed = false;
    if app.show_splash {
        changed |= tick_splash_door(app);
    }
    changed |= tick_clock_glitch(app);
    changed |= tick_name_flicker(app);
    tick_invitation(app);
    changed |= tick_commands(app);
    changed
}

/// Drive the armed whisper for one splash tick. Release (natural, hard
/// cap, or kill switch) closes the splash here and stamps the once-ever
/// mark only on a delivered line; the stamp is also what starts the
/// invitation clock.
fn tick_splash_door(app: &mut App) -> bool {
    let Some(whisper) = app.haunt.whisper.as_mut() else {
        return false;
    };
    let enabled = app.haunt.enabled.load(Ordering::Relaxed);
    match whisper.tick(app.splash_ticks, enabled) {
        WhisperTick::Holding => {}
        WhisperTick::Released { delivered } => {
            app.haunt.whisper = None;
            app.show_splash = false;
            // Any swallowed Esc left the parser mid-escape; same reset the
            // normal splash skip does.
            app.vt_input.reset();
            if delivered {
                let now = chrono::Utc::now();
                app.haunt.marks.whisper_at = Some(now);
                app.profile_state
                    .service()
                    .set_first_contact_whisper_at(app.user_id, now);
                tracing::info!(user_id = %app.user_id, "first contact whisper delivered");
            }
        }
    }
    // The splash pays a frame per tick anyway while it is up.
    true
}

/// The stage-1 scheduler, one world tick. A burst only spends itself
/// while the sidebar clock is actually on screen, and only its start and
/// end frames cost a repaint: render reads `corruption(marquee_tick)` in
/// between, and the ~200ms hold spans the sidebar's ~132ms wake cadence
/// on its own.
fn tick_clock_glitch(app: &mut App) -> bool {
    if app.haunt.clock_glitch.is_none() {
        return false;
    }
    let enabled = app.haunt.enabled.load(Ordering::Relaxed);
    let clock_visible = !app.show_splash && app.right_sidebar_visible();
    let glitch = app.haunt.clock_glitch.as_mut().expect("checked above");
    match glitch.tick(
        app.marquee_tick,
        chrono::Utc::now().date_naive(),
        enabled,
        clock_visible,
    ) {
        GlitchTick::Started | GlitchTick::Ended => true,
        GlitchTick::Idle => false,
    }
}

/// The stage-2 roller: every own message that lands rolls the dice, a
/// hit corrupts that message's author label for ~300ms (the corruption
/// rides the chat rows cache key, so start and heal rebuild the rows
/// exactly once), and every hit bumps the persisted counter that arms
/// the stage-3 whisper.
fn tick_name_flicker(app: &mut App) -> bool {
    // Drained even while unarmed, so a stale echo id never waits around
    // for a later `/haunt on`.
    let landed = app.chat.take_own_message_landed();
    let Some(flicker) = app.haunt.name_flicker.as_mut() else {
        return false;
    };
    let mut changed = false;
    let enabled = app.haunt.enabled.load(Ordering::Relaxed);
    changed |= flicker.tick(app.marquee_tick);
    if let Some(message_id) = landed
        && flicker.note_own_message(
            message_id,
            app.marquee_tick,
            chrono::Utc::now().date_naive(),
            enabled,
        )
    {
        app.haunt.marks.name_hits = flicker.total_hits();
        app.profile_state
            .service()
            .record_first_contact_name_hit(app.user_id);
        tracing::info!(user_id = %app.user_id, hits = app.haunt.marks.name_hits, "first contact name flicker hit");
        changed = true;
    }
    changed
}

/// The stage-4 clock: some days after the delivered whisper, the game's
/// first voice sends its one persistent DM. Self-serve on purpose (the
/// chosen one's own session notices), so there is no cross-user sweep;
/// the conditional settings claim in the send task keeps two devices
/// from double-sending.
fn tick_invitation(app: &mut App) {
    if !app.haunt.eligible
        || app.haunt.marks.invited_at.is_some()
        || !app.haunt.enabled.load(Ordering::Relaxed)
    {
        return;
    }
    let Some(whisper_at) = app.haunt.marks.whisper_at else {
        return;
    };
    let now = chrono::Utc::now();
    if now - whisper_at < chrono::Duration::days(INVITE_DELAY_DAYS) {
        return;
    }
    send_invitation(app, now);
}

fn send_invitation(app: &mut App, now: chrono::DateTime<chrono::Utc>) {
    // Local stamp stops this session re-spawning the task every tick; the
    // DB claim inside the task is the cross-session guard.
    app.haunt.marks.invited_at = Some(now);
    app.chat
        .service
        .send_first_contact_invitation_task(app.user_id);
    tracing::info!(user_id = %app.user_id, "first contact invitation requested");
}

/// Route splash input into the held door. Returns true when consumed:
/// input is acknowledged (the machine surges static and dissolves the
/// skip hint) but the splash does not skip. Silently ignoring input would
/// read as a hung terminal, the exact panic the hard rules forbid.
pub(crate) fn note_splash_input(app: &mut App) -> bool {
    let Some(whisper) = app.haunt.whisper.as_mut() else {
        return false;
    };
    whisper.note_input(app.splash_ticks);
    // A swallowed ESC leaves the parser mid-escape; same reset as the
    // normal splash skip.
    app.vt_input.reset();
    true
}

/// Re-run the splash with the whisper armed, ignoring the marks. The
/// `/haunt replay` admin test hook (also used by tests); a completed
/// replay still re-stamps delivery through the normal release path.
pub(crate) fn replay_whisper(app: &mut App) {
    app.haunt.whisper = Some(WhisperState::for_user(app.user_id));
    app.show_splash = true;
    app.splash_ticks = 0;
}

/// Drain the `/haunt` admin command. The composer only records it for
/// admins, so everything here trusts the caller.
fn tick_commands(app: &mut App) -> bool {
    let Some(command) = app.chat.take_requested_haunt() else {
        return false;
    };
    match command {
        HauntCommand::Status => {
            let enabled = match app.haunt.enabled.load(Ordering::Relaxed) {
                true => "on",
                false => "off",
            };
            let door = match app.haunt.whisper.is_some() {
                true => "door armed",
                false => "door idle",
            };
            let glitch = match &app.haunt.clock_glitch {
                None => "glitch idle".to_string(),
                Some(glitch) => {
                    let minutes = glitch.next_in_ticks(app.marquee_tick) * 66 / 60_000;
                    format!("glitch in ~{minutes}m")
                }
            };
            let whisper = match app.haunt.marks.whisper_at {
                Some(_) => "whisper delivered",
                None => "whisper pending",
            };
            let invite = match app.haunt.marks.invited_at {
                Some(_) => "invited",
                None => "invite pending",
            };
            app.banner = Some(Banner::info(&format!(
                "Haunt {enabled} · {glitch} · name hits {} · {door} · {whisper} · {invite}",
                app.haunt.marks.name_hits
            )));
        }
        HauntCommand::On => {
            app.haunt.enabled.store(true, Ordering::Relaxed);
            // A session that connected while the switch was off armed
            // nothing; turning the haunt on arms the repeatable machines
            // so the flip is testable without reconnecting.
            app.haunt.eligible = true;
            if app.haunt.clock_glitch.is_none() {
                app.haunt.clock_glitch = Some(ClockGlitch::new(
                    session_seed(app.user_id),
                    app.marquee_tick,
                ));
            }
            if app.haunt.name_flicker.is_none() {
                app.haunt.name_flicker = Some(NameFlicker::new(
                    session_seed(app.user_id),
                    app.haunt.marks.name_hits,
                ));
            }
            tracing::info!(user_id = %app.user_id, "haunt enabled");
            app.banner = Some(Banner::success("Haunt on"));
        }
        HauntCommand::Off => {
            // A live whisper drops on its own next splash tick and the
            // schedulers stop firing: every machine reads this switch.
            app.haunt.enabled.store(false, Ordering::Relaxed);
            tracing::info!(user_id = %app.user_id, "haunt disabled (kill switch)");
            app.banner = Some(Banner::success("Haunt off"));
        }
        HauntCommand::Glitch => match app.haunt.clock_glitch.as_mut() {
            None => {
                app.banner = Some(Banner::error("Glitch is not armed - /haunt on first"));
            }
            Some(glitch) => {
                glitch.fire_now(app.marquee_tick);
                app.banner = Some(Banner::success("Glitch fired - watch the clock"));
            }
        },
        HauntCommand::Name => match app.haunt.name_flicker.as_mut() {
            None => {
                app.banner = Some(Banner::error("Name flicker is not armed - /haunt on first"));
            }
            Some(flicker) => {
                flicker.force_next();
                app.banner = Some(Banner::success(
                    "Next message you send flickers - watch your name",
                ));
            }
        },
        HauntCommand::Replay => {
            replay_whisper(app);
        }
        HauntCommand::Invite => match app.haunt.marks.invited_at {
            Some(_) => {
                app.banner = Some(Banner::error(
                    "Already invited - /haunt reset to clear the marks",
                ));
            }
            None => {
                send_invitation(app, chrono::Utc::now());
                app.banner = Some(Banner::success("Invitation sent - check your DMs"));
            }
        },
        HauntCommand::Reset => {
            app.haunt.marks = FirstContactMarks {
                name_hits: 0,
                whisper_at: None,
                invited_at: None,
            };
            if let Some(flicker) = app.haunt.name_flicker.as_mut() {
                *flicker = NameFlicker::new(session_seed(app.user_id), 0);
            }
            app.profile_state.service().reset_first_contact(app.user_id);
            app.banner = Some(Banner::success(
                "First-contact marks cleared - the chain starts over next session",
            ));
        }
    }
    true
}
