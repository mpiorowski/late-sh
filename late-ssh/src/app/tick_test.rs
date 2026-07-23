use std::time::{Duration, Instant};

use late_core::models::user::RightSidebarMode;
use tokio::time::sleep;

use crate::app::state::App;
use crate::app::tick::{ANIM_HALF_TICK, HOT_TICK, IDLE_TICK};
use crate::test_helpers::chat_compose_app;

/// The ambient sidebar wave paints on anim_half edges whenever the right
/// sidebar is visible, so a session showing it never settles by design.
/// Settle-based tests turn the sidebar off to exercise the gate beneath;
/// `sidebar_wave_holds_half_rate_and_paints_on_edges` covers the wave path.
fn hide_sidebar(app: &mut App) {
    app.profile_state.profile.right_sidebar_mode = RightSidebarMode::Off;
}

/// Mirror the render loop's frame path: a changed tick renders and drains
/// the queued terminal commands, otherwise the queue keeps reporting a
/// pending output and the gate (correctly) never goes clean.
fn drain_frame(app: &mut App) {
    let _ = app.render().expect("render");
    let _ = std::mem::take(&mut app.pending_terminal_commands);
}

/// Drive ticks until `consecutive` in a row report no change. Initial
/// prefetches, the splash, chat refresh cadence, and at most one clock
/// rollover may keep early ticks dirty, so this loops with a deadline
/// instead of asserting on a fixed tick count.
async fn settle_clean(app: &mut App, consecutive: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut clean = 0usize;
    while Instant::now() < deadline {
        if app.tick() {
            clean = 0;
            drain_frame(app);
        } else {
            clean += 1;
            if clean >= consecutive {
                return;
            }
        }
        sleep(Duration::from_millis(5)).await;
    }
    let context_epoch_before = app.chat.context_epoch();
    let app_epoch_before = app.chat_ctx_epoch;
    let screen_before = app.screen;
    let dirty_again = app.tick();
    panic!(
        "app never settled to {consecutive} consecutive clean ticks\n\
         one more tick changed={dirty_again} screen={screen_before:?}->{:?}\n\
         chat_epoch {context_epoch_before}->{} app_epoch {app_epoch_before}->{}\n\
         splash={} banner={} outbox={} term_cmds={} clipboard={} image_modal={}\n\
         settings={} ultimate={} hub={} lobby={} profile={} bonsai={} bonsai2={} poll={} icon={} booth={} search={}",
        app.screen,
        app.chat.context_epoch(),
        app.chat_ctx_epoch,
        app.show_splash,
        app.banner.is_some(),
        app.notify_outbox.has_pending(),
        app.pending_terminal_commands.len(),
        app.pending_clipboard.is_some(),
        app.chat.image_modal().is_some(),
        app.show_settings,
        app.show_ultimate_modal,
        app.show_hub_modal,
        app.show_lobby_modal,
        app.show_profile_modal,
        app.show_bonsai_modal,
        app.show_bonsai_v2_modal,
        app.show_poll_modal,
        app.icon_picker_open,
        app.booth_modal_state.is_open(),
        app.room_search_modal_state.is_open(),
    );
}

/// The dirty gate's core promise: an untouched session on a static screen
/// settles to clean ticks (the render loop skips those frames), and a single
/// chat send flips it back to changed once the service events land.
#[tokio::test]
async fn idle_ticks_settle_clean_and_chat_send_marks_changed() {
    let (_test_db, mut app) = chat_compose_app("tick-gate").await;
    hide_sidebar(&mut app);

    settle_clean(&mut app, 30).await;

    app.handle_input(b"hello\r");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut woke = false;
    while Instant::now() < deadline {
        if app.tick() {
            woke = true;
            drain_frame(&mut app);
            break;
        }
        sleep(Duration::from_millis(5)).await;
    }
    assert!(woke, "chat send never produced a changed tick");

    settle_clean(&mut app, 30).await;
}

/// The adaptive loop's cadence contract: a settled idle session on a
/// static screen asks for the idle floor, and fresh input snaps it back to
/// the hot tick (the post-input window that keeps request -> response
/// interactions at typing latency).
#[tokio::test]
async fn wake_hint_idles_when_settled_and_heats_on_input() {
    let (_test_db, mut app) = chat_compose_app("wake-hint").await;
    hide_sidebar(&mut app);

    settle_clean(&mut app, 30).await;

    // Age the app past the post-input hot window.
    app.last_input_at = Instant::now() - Duration::from_secs(10);
    assert_eq!(app.wake_hint(), IDLE_TICK, "settled idle session");

    app.handle_input(b"j");
    assert_eq!(app.wake_hint(), HOT_TICK, "input opens the hot window");
}

/// The ultimate modal's cooldown label is minute-granularity and rides the
/// per-minute global frame, so an open modal with a running cooldown
/// settles clean (no 1Hz cadence), and the running -> ready flip pays
/// exactly one one-shot frame.
#[tokio::test]
async fn open_ultimate_modal_settles_clean_then_fires_once_on_ready() {
    let (_test_db, mut app) = chat_compose_app("tick-gate-ultimate").await;
    hide_sidebar(&mut app);

    settle_clean(&mut app, 30).await;

    app.ultimate_state.set_cooldown(
        crate::app::ultimates::UltimateKind::Wonderland.id(),
        Duration::from_secs(600),
    );
    app.show_ultimate_modal = true;
    drain_frame(&mut app);
    settle_clean(&mut app, 30).await;

    app.ultimate_state.set_cooldown(
        crate::app::ultimates::UltimateKind::Wonderland.id(),
        Duration::from_millis(50),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut woke = false;
    while Instant::now() < deadline {
        if app.tick() {
            woke = true;
            drain_frame(&mut app);
            break;
        }
        sleep(Duration::from_millis(5)).await;
    }
    assert!(woke, "cooldown expiry never produced a changed tick");

    settle_clean(&mut app, 30).await;
}

/// Phase-1 tightening: an open, untouched modal is static between async
/// results, so it settles clean instead of paying a frame every tick (the
/// pre-tightening behavior). Settings is the busiest converted modal: it
/// fires a feed-list load on open and drains profile/feed events.
#[tokio::test]
async fn open_settings_modal_settles_clean() {
    let (_test_db, mut app) = chat_compose_app("tick-gate-modal").await;
    hide_sidebar(&mut app);

    settle_clean(&mut app, 30).await;

    app.handle_input(&[0x0F]); // Ctrl+O
    assert!(app.show_settings, "ctrl+o opens the settings modal");
    drain_frame(&mut app);

    settle_clean(&mut app, 30).await;
}

/// The always-on ambient wave: a session showing the sidebar never goes
/// idle. It holds the half-rate wake tier, and ticks pay frames only on
/// anim_half edges (~every 132ms), not on every wake.
#[tokio::test]
async fn sidebar_wave_holds_half_rate_and_paints_on_edges() {
    let (_test_db, mut app) = chat_compose_app("wave-cadence").await;

    // Flush startup churn (prefetches, splash, first clock render).
    let warmup = Instant::now() + Duration::from_secs(1);
    while Instant::now() < warmup {
        if app.tick() {
            drain_frame(&mut app);
        }
        sleep(Duration::from_millis(5)).await;
    }

    // Age past the post-input window so the hot tier can't mask the wave's.
    app.last_input_at = Instant::now() - Duration::from_secs(10);
    assert_eq!(
        app.wake_hint(),
        ANIM_HALF_TICK,
        "visible sidebar holds the half-rate tier for the wave"
    );

    // 500ms of dense ticks spans at least three anim_half edges; the wave
    // must pay those frames and only those.
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut changed = 0usize;
    let mut total = 0usize;
    while Instant::now() < deadline {
        total += 1;
        if app.tick() {
            changed += 1;
            drain_frame(&mut app);
        }
        sleep(Duration::from_millis(5)).await;
    }
    assert!(changed >= 2, "wave never paid an edge frame ({changed}/{total})");
    assert!(
        changed < total / 2,
        "wave must skip between edges ({changed}/{total})"
    );
}
