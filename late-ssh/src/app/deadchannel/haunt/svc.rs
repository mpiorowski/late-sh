//! Orchestration for the haunting: arming at session start, the splash
//! door, the glitch scheduler, splash input routing, and the `/haunt`
//! admin controls. This is the only haunting layer that touches `App`,
//! logging, and persistence; the machines in `state.rs` stay pure, and
//! the root files keep one routing line each.

use std::sync::atomic::Ordering;

use super::state::{ClockGlitch, GlitchTick, HauntCommand, HauntState, WhisperState, WhisperTick};
use crate::app::common::primitives::Banner;
use crate::app::state::{App, SessionConfig};

/// Build the session's haunting slot. First contact is a nonrenewable
/// resource: while it is admin-scoped scaffolding nothing ever arms for
/// anyone else (GAME.md, First contact). The whisper additionally needs
/// the once-ever mark unspent; the glitch repeats, so it arms whenever
/// the switch is on. Glitch dice are per session on purpose: the same
/// person on two evenings, or two admins side by side, roll differently.
pub(crate) fn arm(config: &SessionConfig) -> HauntState {
    let eligible =
        config.permissions.is_admin() && config.haunt_enabled.load(Ordering::Relaxed);
    HauntState {
        whisper: (eligible && !config.first_contact_whisper_done)
            .then(|| WhisperState::for_user(config.user_id)),
        clock_glitch: eligible.then(|| ClockGlitch::new(glitch_seed(config.user_id), 0)),
        whisper_done: config.first_contact_whisper_done,
        enabled: config.haunt_enabled.clone(),
    }
}

fn glitch_seed(user_id: uuid::Uuid) -> u64 {
    (user_id.as_u128() as u64) ^ (uuid::Uuid::now_v7().as_u128() as u64)
}

/// One world tick for the whole haunting: drive the held splash door,
/// step the glitch scheduler, drain `/haunt`. Returns true when the frame
/// changed. The splash block in `tick.rs` must run first (it advances
/// `splash_ticks` and consults `HauntState::holds_splash_door` before
/// expiring the splash on its own).
pub(crate) fn tick(app: &mut App) -> bool {
    let mut changed = false;
    if app.show_splash {
        changed |= tick_splash_door(app);
    }
    changed |= tick_clock_glitch(app);
    changed |= tick_commands(app);
    changed
}

/// Drive the armed whisper for one splash tick. Release (natural, hard
/// cap, or kill switch) closes the splash here and spends the once-ever
/// mark only on a delivered line.
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
                app.haunt.whisper_done = true;
                app.profile_state
                    .service()
                    .set_first_contact_whisper_done(app.user_id, true);
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

/// Re-run the splash with the whisper armed, ignoring the once-ever
/// mark. The `/haunt replay` admin test hook (also used by tests); a
/// completed replay still re-marks delivery through the normal release
/// path.
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
            let whisper = match app.haunt.whisper_done {
                true => "delivered",
                false => "pending",
            };
            let door = match app.haunt.whisper.is_some() {
                true => "armed",
                false => "idle",
            };
            let glitch = match &app.haunt.clock_glitch {
                None => "glitch idle".to_string(),
                Some(glitch) => {
                    let minutes = glitch.next_in_ticks(app.marquee_tick) * 66 / 60_000;
                    format!("glitch in ~{minutes}m")
                }
            };
            app.banner = Some(Banner::info(&format!(
                "Haunt {enabled} · whisper {whisper} · door {door} · {glitch}"
            )));
        }
        HauntCommand::On => {
            app.haunt.enabled.store(true, Ordering::Relaxed);
            // A session that connected while the switch was off armed no
            // scheduler; turning the haunt on arms one so the flip is
            // testable without reconnecting.
            if app.haunt.clock_glitch.is_none() {
                app.haunt.clock_glitch = Some(ClockGlitch::new(
                    glitch_seed(app.user_id),
                    app.marquee_tick,
                ));
            }
            tracing::info!(user_id = %app.user_id, "haunt enabled");
            app.banner = Some(Banner::success("Haunt on"));
        }
        HauntCommand::Off => {
            // A live whisper drops on its own next splash tick and the
            // scheduler stops firing: both machines read this switch.
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
        HauntCommand::Replay => {
            replay_whisper(app);
        }
        HauntCommand::Reset => {
            app.haunt.whisper_done = false;
            app.profile_state
                .service()
                .set_first_contact_whisper_done(app.user_id, false);
            app.banner = Some(Banner::success(
                "Whisper mark cleared - it arms again next session",
            ));
        }
    }
    true
}
