use super::*;
use std::str::FromStr;

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
fn parse_proxy_v1_tcp4_source_addr() {
    let line = b"PROXY TCP4 203.0.113.10 10.42.0.76 54231 2222\r\n";
    let addr = parse_proxy_v1_addr(line)
        .expect("parse")
        .expect("source addr");
    assert_eq!(
        addr,
        SocketAddr::from_str("203.0.113.10:54231").expect("socket addr")
    );
}

#[test]
fn parse_proxy_v1_unknown_returns_none() {
    let line = b"PROXY UNKNOWN\r\n";
    let addr = parse_proxy_v1_addr(line).expect("parse");
    assert!(addr.is_none());
}

#[test]
fn parse_proxy_v1_rejects_malformed_header() {
    let line = b"PROXY TCP4 203.0.113.10 10.42.0.76 only-one-port\r\n";
    assert!(parse_proxy_v1_addr(line).is_err());
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
    let action = next_render_action(
        world_deadline,
        &signal,
        &mut input_pending,
        previous_render,
    )
    .await;
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
    let action = next_render_action(
        world_deadline,
        &signal,
        &mut input_pending,
        previous_render,
    )
    .await;
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

    let action = next_render_action(
        world_deadline,
        &signal,
        &mut input_pending,
        previous_render,
    )
    .await;

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
