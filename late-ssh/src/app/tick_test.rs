use std::time::{Duration, Instant};

use late_core::models::user::RightSidebarMode;
use late_core::models::user_ssh_key::KeyLayout;
use tokio::time::{sleep, timeout};

use crate::app::chat::svc::ChatEvent;
use crate::app::state::App;
use crate::app::tick::{ANIM_HALF_TICK, HOT_TICK, IDLE_TICK};
use crate::test_helpers::chat_compose_app;

const CLEAN_SETTLE_WINDOW: Duration = Duration::from_millis(250);
/// Watchdog, not the assertion: `CLEAN_SETTLE_WINDOW` is what the gate is
/// judged on. It has to outlast the whole session-startup prefetch cascade,
/// because every landing prefetch legitimately dirties a tick and restarts the
/// window. That cascade is a couple of seconds locally and much longer on a
/// loaded CI runner sharing one Postgres with the rest of the suite, so this is
/// sized like `test_helpers::ASYNC_TEST_TIMEOUT`: generously, and well inside
/// nextest's 5-minute terminate-after.
const CLEAN_SETTLE_TIMEOUT: Duration = Duration::from_secs(30);

/// The sidebar's animated panels (eq strip + bonsai sway) paint on anim_half
/// edges whenever the right sidebar is visible, so a session showing it never
/// settles by design. Settle-based tests turn the sidebar off to exercise the
/// gate beneath; the tail of `idle_ticks_settle_clean_and_wake_for_relevant_events`
/// turns it back on to cover the visible-sidebar path.
///
/// This writes the per-device rails rather than the profile. `ProfileState::
/// drain_snapshot` replaces the whole profile every time a `ProfileService`
/// snapshot lands, so a profile-level edit here survives only until the
/// session's first profile prefetch arrives. That prefetch is async: on an
/// unloaded machine it lands before the settle loop starts, but under load it
/// lands mid-settle, silently restores the account's `On`, and switches the
/// sidebar back on -- at which point the app dirties every anim_half edge
/// (~132ms) and can never hold the 250ms window, at any timeout. `device_rails`
/// takes precedence over the profile in `device_rails_or_profile` and nothing
/// async overwrites it, so the sidebar stays off for the whole test.
fn hide_sidebar(app: &mut App) {
    app.device_rails = Some(KeyLayout {
        room_list_mode: app.rail_modes().0,
        right_sidebar_mode: RightSidebarMode::Off,
    });
}

/// Undo [`hide_sidebar`] for the visible-sidebar case, which shares the settled
/// app. It has to go back through the rails for the same reason: they outrank
/// the profile in `device_rails_or_profile`, so writing the profile alone would
/// leave the sidebar off and its animated panels silent.
fn show_sidebar(app: &mut App) {
    app.device_rails = Some(KeyLayout {
        room_list_mode: app.rail_modes().0,
        right_sidebar_mode: RightSidebarMode::On,
    });
}

/// Mirror the render loop's frame path: a changed tick renders and drains
/// the queued terminal commands, otherwise the queue keeps reporting a
/// pending output and the gate (correctly) never goes clean.
fn drain_frame(app: &mut App) {
    let _ = app.render().expect("render");
    let _ = std::mem::take(&mut app.pending_terminal_commands);
}

/// Drive ticks until the app stays clean for a wall-clock window. Initial
/// prefetches, the splash, chat refresh cadence, and at most one clock
/// rollover may keep early ticks dirty. Wall time, rather than a count of
/// 5ms sleeps, keeps scheduler load from changing the success condition.
async fn settle_clean(app: &mut App) {
    let deadline = Instant::now() + CLEAN_SETTLE_TIMEOUT;
    let mut clean_since = None;
    loop {
        let changed = app.tick();
        let now = Instant::now();
        if changed {
            clean_since = None;
            drain_frame(app);
        } else {
            let since = clean_since.get_or_insert(now);
            if now.duration_since(*since) >= CLEAN_SETTLE_WINDOW {
                return;
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        sleep(Duration::from_millis(5)).await;
    }
    let context_epoch_before = app.chat.context_epoch();
    let app_epoch_before = app.chat_ctx_epoch;
    let screen_before = app.screen;
    let dirty_again = app.tick();
    panic!(
        "app never stayed clean for {}ms\n\
         one more tick changed={dirty_again} screen={screen_before:?}->{:?}\n\
         chat_epoch {context_epoch_before}->{} app_epoch {app_epoch_before}->{}\n\
         splash={} banner={} outbox={} term_cmds={} clipboard={} image_modal={}\n\
         settings={} ultimate={} hub={} lobby={} profile={} bonsai={} bonsai2={} poll={} icon={} booth={} search={}",
        CLEAN_SETTLE_WINDOW.as_millis(),
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
/// relevant event flips it back to changed once. These cases deliberately
/// share one settled app: they exercise one adaptive-cadence contract, and
/// session startup is much more expensive than any individual assertion.
#[tokio::test]
async fn idle_ticks_settle_clean_and_wake_for_relevant_events() {
    let (_test_db, mut app) = chat_compose_app("tick-gate").await;
    hide_sidebar(&mut app);

    settle_clean(&mut app).await;

    // A settled static session requests the idle floor, while even ignored
    // input opens the post-input hot window without changing app state.
    app.last_input_at = Instant::now() - Duration::from_secs(10);
    assert_eq!(app.wake_hint(), IDLE_TICK, "settled idle session");
    app.handle_input(b"\x02");
    assert_eq!(app.wake_hint(), HOT_TICK, "input opens the hot window");

    // A service event dirties exactly the next observable frame.
    let mut chat_events = app.chat.service.subscribe_events();
    let user_id = app.user_id;
    app.handle_input(b"hello\r");
    timeout(Duration::from_secs(5), async {
        loop {
            match chat_events.recv().await {
                Ok(ChatEvent::MessageCreated { message, .. })
                    if message.user_id == user_id && message.body == "hello" =>
                {
                    return;
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    panic!("chat event receiver lagged by {skipped} events");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    panic!("chat event channel closed before the message was created");
                }
            }
        }
    })
    .await
    .expect("chat send never produced MessageCreated");
    assert!(
        app.tick(),
        "queued chat message event did not dirty the tick"
    );
    drain_frame(&mut app);

    settle_clean(&mut app).await;

    // The ultimate cooldown label is minute-granularity. A running cooldown
    // stays clean, and its running -> ready edge pays one one-shot frame.
    app.ultimate_state.set_cooldown(
        crate::app::ultimates::UltimateKind::Wonderland.id(),
        Duration::from_secs(600),
    );
    app.show_ultimate_modal = true;
    drain_frame(&mut app);
    settle_clean(&mut app).await;

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

    settle_clean(&mut app).await;

    // Settings is the busiest converted static modal: opening it fires a
    // feed-list load and drains profile/feed events, then it must settle.
    app.show_ultimate_modal = false;
    app.handle_input(&[0x0F]); // Ctrl+O
    assert!(app.show_settings, "ctrl+o opens the settings modal");
    drain_frame(&mut app);

    settle_clean(&mut app).await;

    // The sidebar's animated panels hold the half-rate tier. Drive the
    // wall-clock frame counter across known phases instead of sleeping through
    // several real animation periods: ticks inside a half-rate period stay
    // clean, and the boundary itself paints.
    app.show_settings = false;
    show_sidebar(&mut app);
    app.last_input_at = Instant::now() - Duration::from_secs(10);
    assert_eq!(
        app.wake_hint(),
        ANIM_HALF_TICK,
        "visible sidebar holds the half-rate tier for its animated panels"
    );

    let even_tick = app.marquee_tick + app.marquee_tick % 2;
    set_marquee_transition(&mut app, even_tick, even_tick + 1);
    assert!(
        !app.tick(),
        "sidebar paid a frame between anim_half boundaries"
    );
    set_marquee_transition(&mut app, even_tick + 1, even_tick + 2);
    assert!(
        app.tick(),
        "sidebar did not pay a frame on an anim_half boundary"
    );
}

/// Make the next `tick` observe an exact wall-clock frame transition. Both
/// values remain forward of the app's natural startup phase; the small offset
/// leaves enough room that the call cannot cross into the following frame.
fn set_marquee_transition(app: &mut App, previous: usize, next: usize) {
    debug_assert!(next > previous);
    app.marquee_tick = previous;
    app.started_at =
        Instant::now() - Duration::from_millis(next as u64 * HOT_TICK.as_millis() as u64 + 5);
    app.last_one_hz_index = Some(next / 15);
}

/// The `/pomodoro` countdown lives entirely on the tick's 1Hz edge: a running
/// timer keeps paying frames (the HUD badge counts down in seconds), and the
/// edge that finds it expired clears it, banners, and queues the desktop
/// notification. Forcing `last_one_hz_index` to `None` fires the edge on the
/// next tick so the test does not sleep out a real wall-clock second.
#[tokio::test]
async fn pomodoro_paints_while_running_then_fires_once_on_expiry() {
    let (_test_db, mut app) = chat_compose_app("tick-gate-pomodoro").await;
    hide_sidebar(&mut app);

    settle_clean(&mut app).await;

    app.pomodoro = Some(crate::app::common::pomodoro::PomodoroTimer {
        label: "deep work".to_string(),
        ends_at: chrono::Utc::now() + chrono::Duration::minutes(25),
    });
    app.last_one_hz_index = None;
    assert!(app.tick(), "a running countdown repaints on the 1Hz edge");
    assert!(
        app.pomodoro.is_some(),
        "an unexpired countdown must survive the edge"
    );
    app.banner = None;
    drain_frame(&mut app);

    app.pomodoro = Some(crate::app::common::pomodoro::PomodoroTimer {
        label: "deep work".to_string(),
        ends_at: chrono::Utc::now() - chrono::Duration::seconds(1),
    });
    app.last_one_hz_index = None;
    assert!(app.tick(), "expiry dirties the tick");
    assert!(app.pomodoro.is_none(), "expiry clears the timer");
    let banner = app.banner.take().expect("expiry banners the label");
    assert!(
        banner.message.contains("deep work"),
        "banner should name the finished timer: {}",
        banner.message
    );
    assert!(
        app.notify_outbox.has_pending(),
        "expiry queues a desktop notification"
    );

    // Nothing left to fire: the next edge is quiet again.
    drain_frame(&mut app);
    app.last_one_hz_index = None;
    app.tick();
    assert!(app.banner.is_none(), "expiry must not re-fire");
}

/// Peer countdowns resolve on the same 1Hz edge as name styles: the owned
/// `peer_pomodoros` map follows the shared directory, and the chat context
/// epoch bumps only when a badge string actually changes (minute rollovers),
/// never on the seconds in between.
#[tokio::test]
async fn pomodoro_peer_badges_resolve_on_the_shared_edge() {
    let (_test_db, mut app) = chat_compose_app("tick-pomodoro-peers").await;
    hide_sidebar(&mut app);
    settle_clean(&mut app).await;

    let directory = crate::app::common::pomodoro::new_directory();
    app.pomodoro_directory = Some(directory.clone());
    let peer = uuid::Uuid::from_u128(0x9e);
    crate::app::common::pomodoro::set_user(
        &directory,
        peer,
        Some(&crate::app::common::pomodoro::PomodoroTimer {
            label: "their secret label".to_string(),
            ends_at: chrono::Utc::now() + chrono::Duration::minutes(25),
        }),
    );

    app.last_one_hz_index = None;
    app.tick();
    let badge = app
        .peer_pomodoros
        .get(&peer)
        .expect("the edge resolves the peer's countdown");
    assert_eq!(badge, "🍅25m");
    let epoch_after_first = app.chat_ctx_epoch;

    // Same minute on the next edge: the badge string is unchanged, so the
    // epoch must hold or every second would rebuild every cached chat row.
    app.last_one_hz_index = None;
    app.tick();
    assert_eq!(
        app.chat_ctx_epoch, epoch_after_first,
        "an unchanged badge must not invalidate chat rows"
    );

    // The peer stopping (or disconnecting) clears the entry; the next edge
    // drops the badge and that IS a row-visible change.
    crate::app::common::pomodoro::set_user(&directory, peer, None);
    app.last_one_hz_index = None;
    app.tick();
    assert!(
        app.peer_pomodoros.is_empty(),
        "a cleared entry loses its badge"
    );
    assert!(
        app.chat_ctx_epoch > epoch_after_first,
        "losing a badge invalidates chat rows"
    );
}

/// This session's own expiry retires its directory entry along with the
/// timer, through the same publish path `/pomodoro stop` uses.
#[tokio::test]
async fn pomodoro_expiry_retires_the_shared_directory_entry() {
    let (_test_db, mut app) = chat_compose_app("tick-pomodoro-retire").await;
    hide_sidebar(&mut app);
    settle_clean(&mut app).await;

    let directory = crate::app::common::pomodoro::new_directory();
    app.pomodoro_directory = Some(directory.clone());
    app.pomodoro = Some(crate::app::common::pomodoro::PomodoroTimer {
        label: "deep work".to_string(),
        ends_at: chrono::Utc::now() - chrono::Duration::seconds(1),
    });
    app.publish_pomodoro();
    assert!(
        crate::app::common::pomodoro::snapshot(&directory).contains_key(&app.user_id),
        "a running timer is published"
    );

    app.last_one_hz_index = None;
    app.tick();
    assert!(app.pomodoro.is_none(), "expiry clears the timer");
    assert!(
        crate::app::common::pomodoro::snapshot(&directory).is_empty(),
        "expiry retires the shared entry so peers stop painting the badge"
    );
}
