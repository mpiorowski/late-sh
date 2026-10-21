use super::*;

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
