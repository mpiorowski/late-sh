use std::time::{Duration, Instant};

use late_core::models::user::RightSidebarMode;
use late_core::models::user_ssh_key::KeyLayout;
use tokio::time::{sleep, timeout};

use crate::app::chat::svc::ChatEvent;
use crate::app::common::primitives::Screen;
use crate::app::state::App;
use crate::app::tick::{ANIM_HALF_TICK, ANIM_QUARTER_TICK, HOT_TICK, IDLE_TICK};
use crate::app::zen::state::{Dir, Look, Node, RiceLayout, TileKind, ZenState};
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
         settings={} ultimate={} hub={} lobby={} profile={} bonsai={} poll={} icon={} booth={} search={}",
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

/// The Zen page paints its equalizer tiles on the anim_half edge, so a page
/// showing one has to wake that often too. Waking on the aquarium's quarter
/// tier instead halved the visualizer to ~3.8fps once the post-input hot
/// window ran out.
#[tokio::test]
async fn zen_equalizer_tiles_hold_the_half_rate_tier() {
    let (_test_db, mut app) = chat_compose_app("tick-zen-eq").await;
    app.screen = Screen::Zen;
    app.last_input_at = Instant::now() - Duration::from_secs(10);

    // The default page carries a music tile, whose eq strip animates.
    app.zen = ZenState::new(RiceLayout::default());
    assert_eq!(
        app.wake_hint(),
        ANIM_HALF_TICK,
        "zen page with a music tile wakes on its eq's paint edge"
    );

    app.zen = ZenState::new(RiceLayout {
        root: Node::split(
            Dir::Row,
            500,
            Node::leaf(TileKind::Visualizer),
            Node::leaf(TileKind::Aquarium),
        ),
        look: Look::default(),
    });
    assert_eq!(
        app.wake_hint(),
        ANIM_HALF_TICK,
        "zen page with a visualizer tile wakes on its eq's paint edge"
    );

    // No eq tile: the reef's own quarter tier is enough.
    app.zen = ZenState::new(RiceLayout {
        root: Node::split(
            Dir::Row,
            500,
            Node::leaf(TileKind::Clock),
            Node::leaf(TileKind::Aquarium),
        ),
        look: Look::default(),
    });
    assert_eq!(
        app.wake_hint(),
        ANIM_QUARTER_TICK,
        "zen page without an eq tile stays on the aquarium tier"
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

/// Away rides the 1Hz edge end to end: a session quiet past the threshold
/// writes its flag to the roster, the presence read on the same edge resolves
/// the away set from it, and the chat row epoch bumps only when that set
/// moves. Forcing `last_one_hz_index` to `None` fires the edge on the next
/// tick so the test does not sleep out a real wall-clock second.
#[tokio::test]
async fn going_away_and_coming_back_ride_the_one_hz_edge() {
    use crate::app::common::away::AWAY_AFTER;
    use crate::state::{ActiveSession, ActiveUser, ActiveUsers};

    let (_test_db, mut app) = chat_compose_app("tick-away").await;
    hide_sidebar(&mut app);
    let roster: ActiveUsers = std::sync::Arc::new(std::sync::Mutex::new(
        [(
            app.user_id,
            ActiveUser {
                username: "tick-away".to_string(),
                fingerprint: None,
                audio_source: late_core::models::user::AudioSource::default(),
                sessions: vec![ActiveSession {
                    token: app.session_token.clone(),
                    fingerprint: None,
                    peer_ip: None,
                    away: false,
                }],
                connection_count: 1,
                last_login_at: Instant::now(),
            },
        )]
        .into(),
    ));
    app.active_users = Some(roster.clone());
    let user_id = app.user_id;
    let roster_says_away = || roster.lock().unwrap()[&user_id].sessions[0].away;
    settle_clean(&mut app).await;
    app.last_one_hz_index = None;
    app.tick();
    assert!(app.away_user_ids.is_empty(), "a fresh session is here");
    let epoch = app.chat_ctx_epoch;

    app.last_input_at = Instant::now() - AWAY_AFTER;
    app.last_one_hz_index = None;
    assert!(app.tick(), "going away repaints the badge");
    assert!(roster_says_away(), "the edge writes the flag to the roster");
    assert!(app.away_user_ids.contains(&app.user_id));
    assert!(
        app.chat_ctx_epoch > epoch,
        "a new away badge invalidates chat rows"
    );
    let epoch = app.chat_ctx_epoch;

    app.last_one_hz_index = None;
    app.tick();
    assert_eq!(
        app.chat_ctx_epoch, epoch,
        "an unchanged away set must not invalidate chat rows every second"
    );

    app.last_input_at = Instant::now();
    app.last_one_hz_index = None;
    app.tick();
    assert!(!roster_says_away(), "input brings the session back");
    assert!(app.away_user_ids.is_empty());
    assert!(app.chat_ctx_epoch > epoch);
}
