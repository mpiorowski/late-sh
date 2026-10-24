use super::*;

#[tokio::test]
async fn client_ip_untrusted_proxy_header_is_not_consumed() {
    use crate::test_helpers::{new_test_db, test_app_state, test_config};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let db = new_test_db().await;
    let mut config = test_config(db.db.config().clone());
    config.ssh_proxy_protocol = true;
    config.ssh_proxy_trusted_cidrs = vec!["10.42.0.0/16".parse().unwrap()];
    let state = test_app_state(db.db.clone(), config);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let header = b"PROXY TCP4 203.0.113.99 127.0.0.1 12345 2222\r\n";
    client.write_all(header).await.unwrap();
    let (mut stream, transport_peer) = listener.accept().await.unwrap();
    assert_eq!(
        resolve_proxied_client_addr(&state, &mut stream, transport_peer)
            .await
            .unwrap(),
        None
    );
    let mut remaining = vec![0; header.len()];
    tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut remaining))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(remaining, header);
}

#[test]
fn reject_publickey_only_advertises_only_publickey() {
    match reject_publickey_only() {
        Auth::Reject {
            proceed_with_methods,
            partial_success,
        } => {
            assert_eq!(
                proceed_with_methods,
                Some(MethodSet::from(&[MethodKind::PublicKey][..]))
            );
            assert!(!partial_success);
        }
        _ => panic!("expected reject auth"),
    }
}

#[test]
fn render_signal_starts_clean() {
    let signal = RenderSignal::new();
    assert!(!signal.dirty.load(Ordering::Acquire));
}

/// Core regression test for the stored-permit bug: after a render has
/// cleared `dirty`, a leftover `Notify` permit must NOT re-arm the
/// throttle. Otherwise every typing burst ends with a spurious render of
/// an unchanged frame.
#[tokio::test]
async fn stale_permit_does_not_arm_throttle() {
    let signal = RenderSignal::new();
    // World tick far in the future so only the notify branch can fire.
    let world_deadline = Instant::now() + Duration::from_secs(100);

    // A prior input rang the bell and was batched into a render; the
    // render cleared `dirty` after draining the queue but the permit is
    // still sitting here.
    signal.notify.notify_one();
    assert!(!signal.dirty.load(Ordering::Acquire));

    let mut input_pending = false;
    let action = next_render_action(
        world_deadline,
        &signal,
        &mut input_pending,
        Some(Instant::now()),
    )
    .await;

    assert_eq!(action, RenderAction::Skip);
    assert!(!input_pending, "stale permit must not arm the throttle");
}

#[tokio::test]
async fn dirty_permit_arms_throttle() {
    let signal = RenderSignal::new();
    let world_deadline = Instant::now() + Duration::from_secs(100);

    signal.dirty.store(true, Ordering::Release);
    signal.notify.notify_one();

    let mut input_pending = false;
    let action = next_render_action(
        world_deadline,
        &signal,
        &mut input_pending,
        Some(Instant::now()),
    )
    .await;

    assert_eq!(action, RenderAction::Skip);
    assert!(input_pending, "dirty permit must arm the throttle");
}

#[tokio::test]
async fn throttle_fires_immediately_when_gap_elapsed() {
    let signal = RenderSignal::new();
    let world_deadline = Instant::now() + Duration::from_secs(100);

    let mut input_pending = true;
    // Pretend the last render was a long time ago — the throttle is
    // already satisfied and should resolve without any wait.
    let previous_render = Some(Instant::now() - Duration::from_secs(1));

    let start = Instant::now();
    let action =
        next_render_action(world_deadline, &signal, &mut input_pending, previous_render).await;
    let elapsed = start.elapsed();

    assert_eq!(action, RenderAction::Render);
    assert!(!input_pending);
    assert!(
        elapsed < Duration::from_millis(5),
        "should fire immediately, actually waited {elapsed:?}"
    );
}

#[tokio::test]
async fn throttle_waits_for_min_render_gap() {
    let signal = RenderSignal::new();
    let world_deadline = Instant::now() + Duration::from_secs(100);

    let mut input_pending = true;
    let previous_render = Some(Instant::now());

    let start = Instant::now();
    let action =
        next_render_action(world_deadline, &signal, &mut input_pending, previous_render).await;
    let elapsed = start.elapsed();

    assert_eq!(action, RenderAction::Render);
    // Generous lower bound — timers can fire a tick or two early.
    assert!(
        elapsed >= Duration::from_millis(10),
        "throttle should wait ~{}ms, waited {:?}",
        MIN_RENDER_GAP.as_millis(),
        elapsed
    );
}

#[tokio::test]
async fn world_tick_fires_when_idle() {
    let signal = RenderSignal::new();
    // A deadline already due resolves right away.
    let world_deadline = Instant::now();

    let mut input_pending = false;
    let action = next_render_action(world_deadline, &signal, &mut input_pending, None).await;

    assert_eq!(action, RenderAction::AdvanceWorld);
}

/// When both the throttle timer and a world tick are ready at the same
/// instant, `biased` ensures world tick wins so animations aren't
/// starved under a keystroke flood.
#[tokio::test]
async fn world_tick_wins_tie_with_throttle() {
    let signal = RenderSignal::new();
    // Both the world deadline and the throttle are already due.
    let world_deadline = Instant::now() - Duration::from_millis(5);

    let mut input_pending = true;
    // Throttle is already satisfied too (previous render long ago).
    let previous_render = Some(Instant::now() - Duration::from_secs(1));

    let action =
        next_render_action(world_deadline, &signal, &mut input_pending, previous_render).await;

    assert_eq!(
        action,
        RenderAction::AdvanceWorld,
        "world tick must beat the throttle branch under `biased` select"
    );
    assert!(!input_pending);
}

#[test]
fn output_budget_accumulates_until_over_budget() {
    let budget = OutputBudget::new();
    assert!(!budget.over_budget());
    budget.record_sent(OUTPUT_BUDGET_BYTES as usize);
    assert!(!budget.over_budget(), "budget boundary itself is not over");
    budget.record_sent(1);
    assert!(budget.over_budget());
    assert_eq!(budget.outstanding(), OUTPUT_BUDGET_BYTES + 1);
}

#[test]
fn output_budget_zero_window_adjust_keeps_backlog() {
    let budget = OutputBudget::new();
    budget.record_sent((OUTPUT_BUDGET_BYTES + 1) as usize);
    // A zero window means russh still holds pending data: the client granted
    // credit but the backlog was not drained, so the session stays stalled.
    budget.on_window_adjusted(0);
    assert!(budget.over_budget());
}

#[test]
fn output_budget_positive_window_adjust_clears_backlog() {
    let budget = OutputBudget::new();
    budget.record_sent((OUTPUT_BUDGET_BYTES + 1) as usize);
    budget.on_window_adjusted(1);
    assert!(!budget.over_budget());
    assert_eq!(budget.outstanding(), 0);
}

/// A key flood: the pass hands the app its budget's worth of input, leaves
/// the rest queued, and marks the session dirty so the next pass picks it up
/// at once instead of holding the worker until the queue is dry.
#[test]
fn input_drain_stops_at_the_message_budget_and_rearms_the_loop() {
    let signal = RenderSignal::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(INPUT_QUEUE_CAP);
    for i in 0..200u8 {
        tx.try_send(vec![i]).expect("queue has room");
    }

    let mut handled = Vec::new();
    let drain = drain_input(
        &mut rx,
        &signal,
        INPUT_DRAIN_MAX_MESSAGES,
        Duration::from_secs(60),
        |data| {
            handled.push(data[0]);
            true
        },
    );

    assert_eq!(drain, InputDrain::BudgetHit);
    assert_eq!(handled, (0..64u8).collect::<Vec<_>>());
    assert_eq!(rx.len(), 136);
    assert!(signal.dirty.load(Ordering::Acquire));
}

#[test]
fn input_drain_stops_when_the_time_budget_is_spent() {
    let signal = RenderSignal::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(INPUT_QUEUE_CAP);
    for i in 0..3u8 {
        tx.try_send(vec![i]).expect("queue has room");
    }

    let mut handled = Vec::new();
    // A spent clock still hands over one message, so every pass progresses.
    let drain = drain_input(&mut rx, &signal, 64, Duration::ZERO, |data| {
        handled.push(data[0]);
        true
    });

    assert_eq!(drain, InputDrain::BudgetHit);
    assert_eq!(handled, vec![0]);
    assert_eq!(rx.len(), 2);
    assert!(signal.dirty.load(Ordering::Acquire));
}

#[test]
fn input_drain_that_empties_the_queue_on_the_budget_is_not_a_budget_hit() {
    let signal = RenderSignal::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(INPUT_QUEUE_CAP);
    for i in 0..64u8 {
        tx.try_send(vec![i]).expect("queue has room");
    }

    let drain = drain_input(&mut rx, &signal, 64, Duration::from_secs(60), |_| true);

    assert_eq!(drain, InputDrain::Drained);
    assert!(rx.is_empty());
    assert!(!signal.dirty.load(Ordering::Acquire));
}

#[test]
fn input_drain_stops_on_quit_and_reports_an_empty_queue() {
    let signal = RenderSignal::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(INPUT_QUEUE_CAP);

    assert_eq!(
        drain_input(&mut rx, &signal, 64, Duration::from_secs(60), |_| true),
        InputDrain::Empty
    );

    for i in 0..3u8 {
        tx.try_send(vec![i]).expect("queue has room");
    }
    let drain = drain_input(&mut rx, &signal, 64, Duration::from_secs(60), |data| {
        data[0] != 1
    });
    assert_eq!(drain, InputDrain::Quit);
    assert_eq!(rx.len(), 1);
}

#[test]
fn slow_pass_warns_once_a_minute_and_only_past_the_hot_tick() {
    let now = Instant::now();
    let slow = SLOW_PASS + Duration::from_millis(1);

    assert!(
        !slow_pass_due(SLOW_PASS, None, now),
        "the hot tick itself is fine"
    );
    assert!(slow_pass_due(slow, None, now), "first slow pass warns");
    assert!(
        !slow_pass_due(slow, Some(now), now + Duration::from_secs(59)),
        "a second slow pass inside the minute stays quiet"
    );
    assert!(
        slow_pass_due(slow, Some(now), now + SLOW_PASS_WARN_EVERY),
        "a minute later it warns again"
    );
}

#[tokio::test]
async fn render_pass_bounds_mixed_input_and_rearms_until_drained() {
    use crate::session_io::WsFrameSink;
    use crate::test_helpers::{make_app, new_test_db};
    use late_core::test_utils::create_test_user;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "bounded-input").await;
    let app = Arc::new(TokioMutex::new(make_app(
        test_db.db.clone(),
        user.id,
        "bounded-input-session",
    )));
    let (input_tx, mut input_rx) = tokio::sync::mpsc::channel(INPUT_QUEUE_CAP);
    for index in 0..200u16 {
        let event = match index % 2 {
            0 => SshInputEvent::Bytes(Vec::new()),
            _ => SshInputEvent::Resize {
                cols: 80 + index % 20,
                rows: 24,
            },
        };
        input_tx.try_send(event).expect("queue has room");
    }
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(INPUT_QUEUE_CAP);
    let ctx = RenderContext {
        sink: WsFrameSink::new(output_tx),
        frame_drop_log_every: 1,
        signal: Arc::new(RenderSignal::new()),
        budget: None,
        client: metrics::SessionClient::Ssh,
        probe: None,
    };

    let mut remaining = input_rx.len();
    while remaining > 0 {
        let outcome = render_once(&app, &mut input_rx, &ctx, false)
            .await
            .expect("render pass");
        assert!(outcome.drew && !outcome.should_quit);
        assert!(outcome.pass.is_some());
        assert!(output_rx.try_recv().is_ok(), "pass sends its frame");
        let next_remaining = input_rx.len();
        assert!(next_remaining < remaining, "each pass makes progress");
        assert!(remaining - next_remaining <= INPUT_DRAIN_MAX_MESSAGES);
        assert_eq!(ctx.signal.dirty.load(Ordering::Acquire), next_remaining > 0);
        remaining = next_remaining;
        while output_rx.try_recv().is_ok() {}
    }
}
